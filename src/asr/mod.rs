//! Local speech-to-text with Fluid Speech Pro (FastConformer-TDT, ONNX).
//!
//! The engine is deliberately separate from `tts::providers`: a `TtsProvider`
//! synthesizes voices and has no transcript/codec concept, so squeezing ASR
//! into it would force meaningless `synthesize`/`prepare_reference` methods.
//! Instead this module owns a single lazily-downloaded, CPU-only instance and
//! exposes its lifecycle via `/api/asr/status`.

pub mod manifest;
pub mod mel;
pub mod tdt;
pub mod vocab;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use self::mel::Features;
use self::tdt::Tdt;
use self::vocab::Vocab;

pub const MODEL_ID: &str = "fluid-speech-pro";
pub const SAMPLE_RATE: u32 = 16_000;
pub const DEFAULT_MAX_SECONDS: f32 = 30.0;
pub const ATTRIBUTION: &str = "Fluid Speech Pro by FluidVoice (CC BY 4.0)";

/// Languages advertised by the upstream model card; detection is automatic.
pub const SUPPORTED_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu",
    "it", "lv", "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Decode(String),
    #[error("{0}")]
    Model(String),
    #[error("audio is {seconds:.1} s long; the limit is {limit:.1} s (split the recording first)")]
    TooLong { seconds: f32, limit: f32 },
    #[error("another transcription is already running")]
    Busy,
    #[error("transcription failed: {0}")]
    Failed(String),
}

impl From<std::io::Error> for AsrError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Transcription {
    pub text: String,
    pub language: String,
    pub duration: f32,
}

/// Small object-safe contract; blocking implementations may hold ONNX sessions.
pub trait Transcriber: Send + Sync {
    fn transcribe(&self, wav: &[u8], language: Option<&str>) -> Result<Transcription, AsrError>;
}

/// Per-request duration cap, overridable without recompiling.
pub fn max_seconds() -> f32 {
    std::env::var("SONICBOOM_ASR_MAX_SECONDS")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(DEFAULT_MAX_SECONDS)
}

pub(crate) fn check_duration(seconds: f32, limit: f32) -> Result<(), AsrError> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(AsrError::Decode("audio duration is invalid".into()));
    }
    if seconds > limit {
        return Err(AsrError::TooLong { seconds, limit });
    }
    Ok(())
}

/// Cheap header/duration validation, safe to run before downloading weights.
pub fn validate_wav(bytes: &[u8]) -> Result<(), AsrError> {
    let reader = hound::WavReader::new(bytes)
        .map_err(|_| AsrError::Decode("body is not a decodable WAV file".into()))?;
    let rate = validate_reader(&reader)?;
    check_duration(reader.duration() as f32 / f64::from(rate) as f32, max_seconds())?;
    Ok(())
}

fn validate_reader<R: std::io::Read>(reader: &hound::WavReader<R>) -> Result<u32, AsrError> {
    let spec = reader.spec();
    if !(1..=2).contains(&spec.channels) {
        return Err(AsrError::Decode(format!(
            "WAV has {} channels; mono or stereo is required",
            spec.channels
        )));
    }
    if spec.sample_rate == 0 || spec.bits_per_sample == 0 || spec.bits_per_sample > 64 {
        return Err(AsrError::Decode("WAV has an invalid sample rate or bit depth".into()));
    }
    Ok(spec.sample_rate)
}

/// Decode a WAV body into mono `f32` at its original rate. PCM 8/16/24/32 and
/// IEEE float are supported; stereo is mixed down explicitly. Rate and sample
/// count are validated before any large allocation.
fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), AsrError> {
    let mut reader = hound::WavReader::new(bytes)
        .map_err(|_| AsrError::Decode("body is not a decodable WAV file".into()))?;
    let rate = validate_reader(&reader)?;
    let spec = reader.spec();
    check_duration(reader.duration() as f32 / f64::from(rate) as f32, max_seconds())?;
    if reader.len() > 16_000_000 {
        return Err(AsrError::Decode("WAV has too many samples".into()));
    }
    let pcm: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AsrError::Decode(format!("corrupt float WAV samples: {e}")))?,
        hound::SampleFormat::Int => {
            if spec.bits_per_sample > 32 {
                return Err(AsrError::Decode("integer WAV above 32 bits is unsupported".into()));
            }
            let scale = 2f32.powf(f32::from(spec.bits_per_sample) - 1.0);
            reader
                .samples::<i32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AsrError::Decode(format!("corrupt integer WAV samples: {e}")))?
                .into_iter()
                .map(|sample| sample as f32 / scale)
                .collect()
        }
    };
    let mono = if spec.channels == 2 {
        let mut mixed = Vec::with_capacity(pcm.len() / 2);
        let mut pairs = pcm.into_iter();
        while let (Some(left), Some(right)) = (pairs.next(), pairs.next()) {
            mixed.push((left + right) * 0.5);
        }
        mixed
    } else {
        pcm
    };
    Ok((mono, rate))
}

/// Approximate language hint for the response. The model detects language
/// internally but does not emit a language id; an explicit `language` field in
/// a request is validated and echoed instead of being fed to the graph.
fn detect_language(text: &str) -> &'static str {
    let text = format!(" {} ", text.trim().to_lowercase());
    let has = |needle: &str| text.contains(needle);
    if has("ñ")
        || has("¿")
        || has("¡")
        || has(" el ")
        || has(" la ")
        || has(" hola ")
        || has(" una ")
        || has(" que ")
    {
        "es"
    } else if has("ç") || has("ê") || has("à") || has(" je ") || has(" nous ") {
        "fr"
    } else if has("ß") || has("ü") || has("ö") || has("ä") || has(" und ") || has(" ich ") {
        "de"
    } else if has("è") || has(" il ") || has(" non ") || has(" perché ") {
        "it"
    } else if has("ão") || has("õ") || has(" não ") || has(" você ") {
        "pt"
    } else if has("ą") || has("ę") || has("ł") || has(" ż") || has(" jest ") {
        "pl"
    } else if has(" the ") || has(" and ") || has(" is ") {
        "en"
    } else {
        "auto"
    }
}

/// Concrete CPU-only ONNX engine.
pub struct FluidModel {
    tdt: Mutex<Tdt>,
    vocab: Vocab,
}

impl FluidModel {
    pub fn open(dir: &Path) -> Result<Self, AsrError> {
        let vocab = Vocab::parse(&std::fs::read_to_string(dir.join("vocab.txt"))?)?;
        let tdt = Tdt::open(dir, vocab.size())?;
        Ok(Self {
            tdt: Mutex::new(tdt),
            vocab,
        })
    }
}

impl Transcriber for FluidModel {
    fn transcribe(&self, wav: &[u8], language: Option<&str>) -> Result<Transcription, AsrError> {
        let (pcm, rate) = decode_wav(wav)?;
        let mono16 = crate::tts::audio::resample_reference(&pcm, rate, SAMPLE_RATE);
        let duration = mono16.len() as f32 / SAMPLE_RATE as f32;
        check_duration(duration, max_seconds())?;
        let features: Features = mel::nemo_features(&mono16)?;
        let mut tdt = self.tdt.lock().map_err(|_| AsrError::Busy)?;
        let tokens = tdt.transcribe(&features, self.vocab.blank())?;
        drop(tdt);
        let text = self.vocab.decode(&tokens);
        let language = language
            .map(str::to_string)
            .unwrap_or_else(|| detect_language(&text).to_string());
        Ok(Transcription {
            text,
            language,
            duration,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AsrState {
    Absent,
    Downloading,
    Loading,
    Ready,
    Failed,
}

impl AsrState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Downloading => "downloading",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AsrStatus {
    pub state: &'static str,
    pub model: &'static str,
    pub revision: &'static str,
    pub progress: Option<f32>,
    pub error: Option<String>,
    pub max_duration_seconds: f32,
}

enum Lifecycle {
    Absent,
    Downloading(f32),
    Loading,
    Ready(Arc<dyn Transcriber>),
    Failed(String),
}

/// One shared engine, serialized by a semaphore so a single 0.63B model
/// never doubles its resident memory.
pub struct AsrManager {
    // `std` lock: the download progress callback is synchronous and runs on
    // the async thread, so a Tokio `blocking_write` would panic there. Guards
    // are never held across an await.
    lifecycle: std::sync::RwLock<Lifecycle>,
    load_lock: tokio::sync::Mutex<()>,
    slot: tokio::sync::Semaphore,
    model_dir: PathBuf,
    hf_token: Option<String>,
}

impl AsrManager {
    pub fn new(model_dir: PathBuf, hf_token: Option<String>) -> Self {
        Self {
            lifecycle: std::sync::RwLock::new(Lifecycle::Absent),
            load_lock: tokio::sync::Mutex::new(()),
            slot: tokio::sync::Semaphore::new(1),
            model_dir,
            hf_token,
        }
    }

    pub async fn status(&self) -> AsrStatus {
        let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
        let (state, progress, error) = match &*lifecycle {
            Lifecycle::Absent => (AsrState::Absent, None, None),
            Lifecycle::Downloading(p) => (AsrState::Downloading, Some(*p), None),
            Lifecycle::Loading => (AsrState::Loading, None, None),
            Lifecycle::Ready(_) => (AsrState::Ready, None, None),
            Lifecycle::Failed(message) => (AsrState::Failed, None, Some(message.clone())),
        };
        AsrStatus {
            state: state.as_str(),
            model: MODEL_ID,
            revision: manifest::REVISION,
            progress,
            error,
            max_duration_seconds: max_seconds(),
        }
    }

    /// Download (if needed) and load once; concurrent callers share the work.
    pub async fn ensure_ready(&self) -> Result<Arc<dyn Transcriber>, AsrError> {
        {
            let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
            if let Lifecycle::Ready(model) = &*lifecycle {
                return Ok(Arc::clone(model));
            }
        }
        let _load = self.load_lock.lock().await;
        {
            let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
            if let Lifecycle::Ready(model) = &*lifecycle {
                return Ok(Arc::clone(model));
            }
        }
        let files = manifest::pinned();
        {
            let mut lifecycle = self.lifecycle.write().unwrap_or_else(|p| p.into_inner());
            *lifecycle = Lifecycle::Downloading(0.0);
        }
        let dir = self.model_dir.clone();
        let progress_lifecycle = &self.lifecycle;
        let downloaded = crate::tts::providers::assets::ensure_pinned_files(
            MODEL_ID,
            &dir,
            &files,
            self.hf_token.as_deref(),
            |fraction| {
                let mut lifecycle = progress_lifecycle.write().unwrap_or_else(|p| p.into_inner());
                if let Lifecycle::Downloading(current) = &mut *lifecycle {
                    *current = fraction.clamp(0.0, 1.0);
                }
            },
            || {},
            || false,
        )
        .await;
        if let Err(error) = downloaded {
            let message = error.to_string();
            *self.lifecycle.write().unwrap_or_else(|p| p.into_inner()) =
                Lifecycle::Failed(message.clone());
            return Err(AsrError::Failed(message));
        }
        {
            let mut lifecycle = self.lifecycle.write().unwrap_or_else(|p| p.into_inner());
            *lifecycle = Lifecycle::Loading;
        }
        let model = tokio::task::spawn_blocking(move || FluidModel::open(&dir))
            .await
            .map_err(|e| AsrError::Failed(format!("model load task failed: {e}")))?;
        match model {
            Ok(model) => {
                let shared: Arc<dyn Transcriber> = Arc::new(model);
                *self.lifecycle.write().unwrap_or_else(|p| p.into_inner()) =
                    Lifecycle::Ready(Arc::clone(&shared));
                Ok(shared)
            }
            Err(error) => {
                let message = error.to_string();
                *self.lifecycle.write().unwrap_or_else(|p| p.into_inner()) =
                    Lifecycle::Failed(message.clone());
                Err(AsrError::Failed(message))
            }
        }
    }

    /// Activation hook for the Engines view / explicit warm-up.
    pub async fn prepare(&self) -> Result<AsrStatus, AsrError> {
        self.ensure_ready().await?;
        Ok(self.status().await)
    }

    pub async fn transcribe(
        &self,
        wav: Vec<u8>,
        language: Option<String>,
    ) -> Result<Transcription, AsrError> {
        let _slot = self.slot.acquire().await.map_err(|_| AsrError::Busy)?;
        let model = self.ensure_ready().await?;
        tokio::task::spawn_blocking(move || model.transcribe(&wav, language.as_deref()))
            .await
            .map_err(|e| AsrError::Failed(format!("transcription task failed: {e}")))?
    }
}

static MANAGER: OnceLock<AsrManager> = OnceLock::new();

/// Process-wide manager, initialized on first use. `SONICBOOM_ASR_MODEL_DIR`
/// overrides the default model directory; `HF_TOKEN` is used for downloads.
pub fn manager() -> &'static AsrManager {
    MANAGER.get_or_init(|| {
        let dir = std::env::var_os("SONICBOOM_ASR_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("models/fluid-speech-pro"));
        let token = std::env::var("HF_TOKEN")
            .ok()
            .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok())
            .filter(|value| !value.trim().is_empty());
        AsrManager::new(dir, token)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for &sample in samples {
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn duration_limit_is_enforced_and_configurable() {
        check_duration(5.9, 30.0).unwrap();
        assert!(matches!(
            check_duration(30.1, 30.0),
            Err(AsrError::TooLong { .. })
        ));
        assert!(check_duration(f32::NAN, 30.0).is_err());
    }

    #[test]
    fn decodes_pcm16_mono_and_stereo_and_rejects_bad_inputs() {
        let mono = wav(&[0, 1000, -1000, 0], 16000, 1);
        let (pcm, rate) = decode_wav(&mono).unwrap();
        assert_eq!(rate, 16000);
        assert_eq!(pcm.len(), 4);
        assert!((pcm[1] - 1000.0 / 32768.0).abs() < 1e-6);

        let stereo = wav(&[1000, -1000, 2000, -2000], 16000, 2);
        let (mixed, _) = decode_wav(&stereo).unwrap();
        assert_eq!(mixed.len(), 2);
        assert_eq!(mixed[0], 0.0);

        assert!(decode_wav(b"not a wav").is_err());
        assert!(decode_wav(&wav(&[0; 6], 16000, 3)).is_err());
    }

    #[test]
    fn language_hint_is_only_approximate() {
        assert_eq!(detect_language("Hello, this is a test."), "en");
        assert_eq!(detect_language("Hola, esto es una prueba."), "es");
        assert_eq!(detect_language("xyzzy"), "auto");
    }
}
