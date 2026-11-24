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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tokio::sync::watch;

use self::mel::Features;
use self::tdt::Tdt;
use self::vocab::Vocab;

pub const MODEL_ID: &str = "fluid-speech-pro";
pub const SAMPLE_RATE: u32 = 16_000;
pub const DEFAULT_MAX_SECONDS: f32 = 30.0;
pub const ATTRIBUTION: &str = "Fluid Speech Pro by FluidVoice (CC BY 4.0)";

/// Languages advertised by the upstream model card; detection is automatic.
pub const SUPPORTED_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
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
    check_duration(
        reader.duration() as f32 / f64::from(rate) as f32,
        max_seconds(),
    )?;
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
        return Err(AsrError::Decode(
            "WAV has an invalid sample rate or bit depth".into(),
        ));
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
    check_duration(
        reader.duration() as f32 / f64::from(rate) as f32,
        max_seconds(),
    )?;
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
                return Err(AsrError::Decode(
                    "integer WAV above 32 bits is unsupported".into(),
                ));
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

/// Uniform engine status for the Engines view: transcription is loaded from
/// the same panel as the synthesis providers, without pretending to be a TTS
/// provider.
#[derive(Debug, Clone, Serialize)]
pub struct AsrEngineStatus {
    pub id: &'static str,
    pub kind: &'static str,
    pub state: &'static str,
    pub enabled: bool,
    pub model: &'static str,
    pub active_model: Option<&'static str>,
    pub progress: Option<f32>,
    pub detail: Option<String>,
    pub license: &'static str,
    pub prepare_path: &'static str,
}

/// Clears the "a load is already scheduled" flag when a background load ends,
/// including on an early return after another load won the race.
struct LoadGuard<'a>(&'a AtomicBool);

impl Drop for LoadGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// One shared engine. Download and load are owned by the manager, not by an
/// HTTP request: a client timeout or disconnect can no longer cancel an 801 MB
/// download halfway through. Transcription itself is serialized by a semaphore
/// so a single 0.63B model never doubles its resident memory.
pub struct AsrManager {
    // `std` lock: the download progress callback is synchronous and runs on
    // the async thread, so a Tokio `blocking_write` would panic there. Guards
    // are never held across an await.
    lifecycle: std::sync::RwLock<Lifecycle>,
    /// Serializes "is it ready?" + "schedule a load" with terminal publishes,
    /// so a finishing load can never be overwritten by a stale caller.
    publish_lock: std::sync::Mutex<()>,
    load_lock: tokio::sync::Mutex<()>,
    load_scheduled: AtomicBool,
    /// Version counter: a waiter subscribes before re-checking the lifecycle,
    /// so a state change on another worker can never be missed.
    version: watch::Sender<u64>,
    slot: tokio::sync::Semaphore,
    model_dir: PathBuf,
    hf_token: Option<String>,
}

impl AsrManager {
    pub fn new(model_dir: PathBuf, hf_token: Option<String>) -> Self {
        Self {
            lifecycle: std::sync::RwLock::new(Lifecycle::Absent),
            publish_lock: std::sync::Mutex::new(()),
            load_lock: tokio::sync::Mutex::new(()),
            load_scheduled: AtomicBool::new(false),
            version: watch::Sender::new(0),
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

    /// Same lifecycle as `/api/asr/status`, in the shape the Engines view uses
    /// for every engine. `detail` is diagnostic text, never a request body.
    pub fn engine_status(&self) -> AsrEngineStatus {
        let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
        let (state, progress, detail) = match &*lifecycle {
            Lifecycle::Absent => (AsrState::Absent, None, None),
            Lifecycle::Downloading(p) => (
                AsrState::Downloading,
                Some(*p),
                Some(format!("Downloading files: {:.0}%", p * 100.0)),
            ),
            Lifecycle::Loading => (
                AsrState::Loading,
                None,
                Some("Loading model...".to_string()),
            ),
            Lifecycle::Ready(_) => (AsrState::Ready, None, None),
            Lifecycle::Failed(message) => (AsrState::Failed, None, Some(message.clone())),
        };
        AsrEngineStatus {
            id: "asr",
            kind: "asr",
            state: state.as_str(),
            enabled: true,
            model: MODEL_ID,
            active_model: if state == AsrState::Ready {
                Some(MODEL_ID)
            } else {
                None
            },
            progress,
            detail,
            license: ATTRIBUTION,
            prepare_path: "/api/asr/prepare",
        }
    }

    fn publish(&self, next: Lifecycle) {
        let _guard = self.publish_lock.lock().unwrap_or_else(|p| p.into_inner());
        self.publish_inner(next);
    }

    fn publish_inner(&self, next: Lifecycle) {
        *self.lifecycle.write().unwrap_or_else(|p| p.into_inner()) = next;
        // Drop the borrow before sending: a live `Ref` would deadlock `send`.
        let version = self.version.borrow().wrapping_add(1);
        let _ = self.version.send(version);
    }

    /// Schedule the download+load and return immediately. The spawned task is
    /// owned by the manager, so dropping an HTTP future cannot cancel it.
    pub fn start_load(self: &Arc<Self>) {
        let _guard = self.publish_lock.lock().unwrap_or_else(|p| p.into_inner());
        {
            let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
            if matches!(*lifecycle, Lifecycle::Ready(_)) {
                return;
            }
        }
        if self.load_scheduled.swap(true, Ordering::SeqCst) {
            return;
        }
        // Publish before spawning: a caller reading status right after
        // `prepare()` must never see `absent` again.
        self.publish_inner(Lifecycle::Downloading(0.0));
        drop(_guard);
        let this = Arc::clone(self);
        tokio::spawn(async move { this.run_load().await });
    }

    /// The one background download+load. Concurrent callers share it: a queued
    /// task re-checks readiness after acquiring the load lock.
    async fn run_load(self: &Arc<Self>) {
        let _guard = LoadGuard(&self.load_scheduled);
        let _load = self.load_lock.lock().await;
        {
            let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
            if matches!(*lifecycle, Lifecycle::Ready(_)) {
                return;
            }
        }
        let files = manifest::pinned();
        let dir = self.model_dir.clone();
        let token = self.hf_token.clone();
        let progress_manager = Arc::clone(self);
        let downloaded = crate::tts::providers::assets::ensure_pinned_files(
            MODEL_ID,
            &dir,
            &files,
            token.as_deref(),
            move |fraction| {
                let mut lifecycle = progress_manager
                    .lifecycle
                    .write()
                    .unwrap_or_else(|p| p.into_inner());
                if let Lifecycle::Downloading(current) = &mut *lifecycle {
                    *current = fraction.clamp(0.0, 1.0);
                }
                let version = progress_manager.version.borrow().wrapping_add(1);
                let _ = progress_manager.version.send(version);
            },
            || {},
            || false,
        )
        .await;
        if let Err(error) = downloaded {
            self.publish(Lifecycle::Failed(error.to_string()));
            return;
        }
        self.publish(Lifecycle::Loading);
        let opened = tokio::task::spawn_blocking(move || FluidModel::open(&dir)).await;
        match opened {
            Ok(Ok(model)) => {
                let shared: Arc<dyn Transcriber> = Arc::new(model);
                self.publish(Lifecycle::Ready(Arc::clone(&shared)));
            }
            Ok(Err(error)) => self.publish(Lifecycle::Failed(error.to_string())),
            Err(error) => self.publish(Lifecycle::Failed(format!(
                "model load task failed: {error}"
            ))),
        }
    }

    /// Wait for the background load to reach a terminal state. A receiver is
    /// created before each check, so an update from another worker either shows
    /// up in that check or wakes the `changed()` call.
    pub async fn wait_ready(self: &Arc<Self>) -> Result<Arc<dyn Transcriber>, AsrError> {
        let mut version = self.version.subscribe();
        loop {
            {
                let lifecycle = self.lifecycle.read().unwrap_or_else(|p| p.into_inner());
                match &*lifecycle {
                    Lifecycle::Ready(model) => return Ok(Arc::clone(model)),
                    Lifecycle::Failed(message) => return Err(AsrError::Failed(message.clone())),
                    _ => {}
                }
            }
            version
                .changed()
                .await
                .map_err(|_| AsrError::Failed("model loader was dropped".into()))?;
        }
    }

    /// A transcription waits for readiness; the download itself is background
    /// work and survives this future being dropped by an HTTP timeout.
    pub async fn ensure_ready(self: &Arc<Self>) -> Result<Arc<dyn Transcriber>, AsrError> {
        self.start_load();
        self.wait_ready().await
    }

    /// Activation hook for the Engines view: starts the background load and
    /// reports its current state instead of holding the HTTP request open.
    pub async fn prepare(self: &Arc<Self>) -> Result<AsrStatus, AsrError> {
        self.start_load();
        Ok(self.status().await)
    }

    pub async fn transcribe(
        self: &Arc<Self>,
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

static MANAGER: OnceLock<Arc<AsrManager>> = OnceLock::new();

/// Process-wide manager, initialized on first use. `SONICBOOM_ASR_MODEL_DIR`
/// overrides the default model directory; `HF_TOKEN` is used for downloads.
/// An `Arc` lets background loads outlive the request that triggered them.
pub fn manager() -> Arc<AsrManager> {
    Arc::clone(MANAGER.get_or_init(|| {
        let dir = std::env::var_os("SONICBOOM_ASR_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("models/fluid-speech-pro"));
        let token = std::env::var("HF_TOKEN")
            .ok()
            .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok())
            .filter(|value| !value.trim().is_empty());
        Arc::new(AsrManager::new(dir, token))
    }))
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

    struct FakeTranscriber;

    impl Transcriber for FakeTranscriber {
        fn transcribe(
            &self,
            _wav: &[u8],
            _language: Option<&str>,
        ) -> Result<Transcription, AsrError> {
            Ok(Transcription {
                text: String::new(),
                language: "auto".into(),
                duration: 0.0,
            })
        }
    }

    fn manager_for_test() -> Arc<AsrManager> {
        Arc::new(AsrManager::new(
            std::env::temp_dir().join("sonicboom-asr-lifecycle-test"),
            None,
        ))
    }

    #[tokio::test]
    async fn wait_ready_wakes_when_the_background_load_publishes_ready() {
        let manager = manager_for_test();
        let waiter = {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move { manager.wait_ready().await })
        };
        tokio::task::yield_now().await;
        manager.publish(Lifecycle::Ready(Arc::new(FakeTranscriber)));
        let model = tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(model.transcribe(&[], None).is_ok());
    }

    #[tokio::test]
    async fn dropped_waiter_does_not_reset_the_background_state() {
        let manager = manager_for_test();
        let waiter = {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move { manager.wait_ready().await })
        };
        manager.publish(Lifecycle::Downloading(0.3));
        // Simulate an HTTP timeout dropping the waiting request.
        waiter.abort();
        let _ = waiter.await;
        let status = manager.status().await;
        assert_eq!(status.state, "downloading");
        assert_eq!(status.progress, Some(0.3));
    }

    #[tokio::test]
    async fn prepare_of_a_ready_model_does_not_schedule_another_load() {
        let manager = manager_for_test();
        manager.publish(Lifecycle::Ready(Arc::new(FakeTranscriber)));
        let status = manager.prepare().await.unwrap();
        assert_eq!(status.state, "ready");
        assert!(!manager.load_scheduled.load(Ordering::SeqCst));
    }

    #[test]
    fn engine_status_mirrors_the_lifecycle() {
        let manager = manager_for_test();
        manager.publish(Lifecycle::Downloading(0.42));
        let engine = manager.engine_status();
        assert_eq!(engine.id, "asr");
        assert_eq!(engine.state, "downloading");
        assert_eq!(engine.progress, Some(0.42));
        assert_eq!(engine.prepare_path, "/api/asr/prepare");
        assert!(engine.detail.unwrap().contains("42"));
        assert!(engine.active_model.is_none());
        manager.publish(Lifecycle::Ready(Arc::new(FakeTranscriber)));
        let engine = manager.engine_status();
        assert_eq!(engine.state, "ready");
        assert_eq!(engine.active_model, Some(MODEL_ID));
    }

    #[test]
    fn language_hint_is_only_approximate() {
        assert_eq!(detect_language("Hello, this is a test."), "en");
        assert_eq!(detect_language("Hola, esto es una prueba."), "es");
        assert_eq!(detect_language("xyzzy"), "auto");
    }
}
