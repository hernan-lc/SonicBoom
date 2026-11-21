//! Reverb-TTS provider: zero-shot voice cloning with Software Mansion's Reverb model
//! (masked-diffusion TTS over Qwen3 layers) and the Higgs Audio v2 codec, run from ONNX
//! exports on ONNX Runtime. No Python or PyTorch.
//!
//! **License:** the Reverb weights are CC BY-NC 4.0 (non-commercial, attribution) and the
//! codec follows the Boson Higgs Audio 2 Community License. The provider is disabled by
//! default and downloads weights only after `[providers.reverb_tts] enabled = true`.
//!
//! Cloning uses ICL mode: a reference recording of 3-10 s plus its exact transcript.
//! Conversion scripts and parity evidence: <https://github.com/nglmercer/Reverb-TTS-onnx>.

pub mod audio;
#[cfg(feature = "reverb-tts")]
pub mod native;
pub mod sampler;
pub mod text;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::TtsProvider;
use super::assets::PinnedFile;
use super::catalog::REVERB_TTS_MODELS;
use super::error::ProviderError;
use super::types::{
    CloneMode, DevicePreference, PcmAudio, ProviderCapabilities, ProviderRequest, ProviderStatus,
    ProviderStatusState,
};
pub use text::REVERB_LANGUAGES;

pub const REVERB_TTS_PROVIDER_ID: &str = "reverb-tts";
pub const REVERB_TTS_SAMPLE_RATE: u32 = 24_000;
/// Pinned ONNX export of the Reverb transformer (melser/Reverb-TTS-onnx).
pub const REVERB_ONNX_REPO: &str = "melser/Reverb-TTS-onnx";
pub const REVERB_ONNX_REVISION: &str = "ca95b327a5624f0988511e192510970885430421";
/// Pinned ONNX export of the Higgs Audio v2 codec (melser/Higgs-Audio-2-Tokenizer-onnx).
pub const REVERB_CODEC_REPO: &str = "melser/Higgs-Audio-2-Tokenizer-onnx";
pub const REVERB_CODEC_REVISION: &str = "372d4e54f001d1e3fcbb4ac10ac5dd4641af8271";
/// Tokenizer comes from the original repository at its `v1.0.0` tag.
pub const REVERB_ORIGINAL_REPO: &str = "software-mansion/Reverb-TTS";
pub const REVERB_ORIGINAL_REVISION: &str = "72be799c8fb6d56a206d260b425c48bf3bd5943d";

/// Reference length bounds (seconds). Upstream recommends 3-10 s; longer references
/// make worse takes.
pub const REVERB_REF_MIN_SECS: f64 = 2.0;
pub const REVERB_REF_MAX_SECS: f64 = crate::tts::voices::REFERENCE_MAX_SECS;
pub const REVERB_REF_RECOMMENDED_MAX_SECS: f64 = 15.0;
/// Seconds cut from every take's end, where the codec can leave a short noise burst.
const END_CUT_SECS: f64 = 0.2;
/// Longest single piece of text (frames, 90 s); sentences beyond it need punctuation.
const MAX_PIECE_FRAMES: usize = 90 * text::FRAME_RATE;
/// Output bound for one request (ten minutes of samples).
const MAX_SAMPLES: usize = REVERB_TTS_SAMPLE_RATE as usize * 600;
/// Version of the reference preprocessing, part of the conditioning cache key.
pub const REVERB_PROMPT_PREP_VERSION: &str = "raise-quiet-pad960-v1";

/// Files every installed bundle must contain (relative to the model dir).
pub const REVERB_TTS_REQUIRED_FILES: &[&str] = &[
    "model/tokenizer.json",
    "onnx/core.onnx",
    "onnx/embed_prefix.onnx",
    "onnx/embed_target.onnx",
    "onnx/heads.onnx",
    "onnx/codec_decoder.onnx",
    "onnx/codec_encoder.onnx",
];

/// `(path, repo, revision, sha256, bytes)` of every pinned download.
const MANIFEST: &[(&str, &str, &str, &str, u64)] = &[
    (
        "onnx/core.onnx",
        REVERB_ONNX_REPO,
        REVERB_ONNX_REVISION,
        "1f3fb3c10ef8ee65e29ba8cf4ab9d5d0ff86ef6cabf674ebdf0985dd2c6e1856",
        1_057_630_611,
    ),
    (
        "onnx/embed_prefix.onnx",
        REVERB_ONNX_REPO,
        REVERB_ONNX_REVISION,
        "904e37fac82a4ccededad06a32e2641de6145d73f4428a9b7b13e3d7a4cf7c62",
        98_160_722,
    ),
    (
        "onnx/embed_target.onnx",
        REVERB_ONNX_REPO,
        REVERB_ONNX_REVISION,
        "c0963c8374148bec1ea53972dff6f62624fa4665e1786bed0bb17727f8ee308d",
        33_587_818,
    ),
    (
        "onnx/heads.onnx",
        REVERB_ONNX_REPO,
        REVERB_ONNX_REVISION,
        "5a41608150e9b5f850536d55fce856c582b95656dcaa4be201690f314ef8a0d0",
        33_588_897,
    ),
    (
        "onnx/codec_decoder.onnx",
        REVERB_CODEC_REPO,
        REVERB_CODEC_REVISION,
        "ee3305707c5f0abe2813a1ebea6900e09a764f05d7520ad5f8eeb1618d93d2af",
        86_503_143,
    ),
    (
        "onnx/codec_encoder.onnx",
        REVERB_CODEC_REPO,
        REVERB_CODEC_REVISION,
        "1f55729f7ecc54e79af8ed2d6ce23841ae86a0ef221f7c817130314ea0a0dabd",
        654_441_001,
    ),
    (
        "model/tokenizer.json",
        REVERB_ORIGINAL_REPO,
        REVERB_ORIGINAL_REVISION,
        "11a8be5d2c46fe27ff15450e91c3cfdfc7b9949f2de8612ae2991d865c51b5fc",
        2_170_389,
    ),
];

/// Pinned download manifest (every file verified by SHA-256 before use).
pub fn reverb_manifest() -> Vec<PinnedFile> {
    MANIFEST
        .iter()
        .map(|(path, repo, revision, sha256, size)| PinnedFile {
            path,
            url: PinnedFile::huggingface(repo, revision, path),
            sha256,
            size: *size,
        })
        .collect()
}

/// Reverb-TTS provider configuration (`[providers.reverb_tts]`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReverbTtsConfig {
    pub enabled: bool,
    pub model_dir: PathBuf,
    pub model_id: String,
    /// Pinned export revision label (part of voice-profile identity).
    pub revision: String,
    /// `cpu` (default) or `auto`: Vulkan (WebGPU EP) for the transformer graphs.
    pub device: DevicePreference,
    pub cache_capacity: usize,
    /// Main sampler passes (upstream default 8).
    pub steps: usize,
    pub cpu_threads: usize,
}

impl Default for ReverbTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_dir: PathBuf::from("./models/reverb-tts"),
            model_id: REVERB_TTS_MODELS[0].to_string(),
            revision: REVERB_ONNX_REVISION.to_string(),
            device: DevicePreference::Cpu,
            cache_capacity: 16,
            steps: 8,
            cpu_threads: 4,
        }
    }
}

struct ReverbState {
    loaded_model: Option<String>,
    load_error: Option<String>,
    ensuring: bool,
    download_progress: f32,
    downloading: bool,
    /// `cpu` or `vulkan`, as resolved when the model was loaded.
    device: Option<&'static str>,
}

/// Native Reverb-TTS provider (ICL voice cloning).
pub struct ReverbTtsProvider {
    config: Mutex<ReverbTtsConfig>,
    pending_config: Mutex<Option<ReverbTtsConfig>>,
    lifecycle: Mutex<()>,
    state: Mutex<ReverbState>,
    #[cfg(feature = "reverb-tts")]
    model: Mutex<Option<native::ReverbNative>>,
    /// Cached reference codes keyed by `(provider, revision, reference hash, mode)`.
    codes: Mutex<super::worker::ConditioningCache>,
    last_voice: Mutex<Option<(String, String)>>,
}

impl ReverbTtsProvider {
    pub fn new(config: ReverbTtsConfig) -> Arc<Self> {
        Arc::new(Self {
            codes: Mutex::new(super::worker::ConditioningCache::new(config.cache_capacity)),
            config: Mutex::new(config),
            pending_config: Mutex::new(None),
            lifecycle: Mutex::new(()),
            state: Mutex::new(ReverbState {
                loaded_model: None,
                load_error: None,
                ensuring: false,
                download_progress: 0.0,
                downloading: false,
                device: None,
            }),
            #[cfg(feature = "reverb-tts")]
            model: Mutex::new(None),
            last_voice: Mutex::new(None),
        })
    }

    fn config_snapshot(&self) -> ReverbTtsConfig {
        self.config.lock().map(|c| c.clone()).unwrap_or_default()
    }

    fn same_model(a: &ReverbTtsConfig, b: &ReverbTtsConfig) -> bool {
        a.enabled == b.enabled
            && a.model_dir == b.model_dir
            && a.model_id == b.model_id
            && a.revision == b.revision
            && a.device == b.device
            && a.cpu_threads == b.cpu_threads
    }

    pub fn reload_pending(&self) -> bool {
        self.pending_config
            .lock()
            .map(|p| p.is_some())
            .unwrap_or(true)
    }

    fn model_change_pending(&self) -> bool {
        let current = self.config_snapshot();
        self.pending_config
            .lock()
            .map(|p| {
                p.as_ref()
                    .is_some_and(|next| !Self::same_model(&current, next))
            })
            .unwrap_or(true)
    }

    pub fn reconfigure(&self, config: ReverbTtsConfig) {
        let Ok(_lifecycle) = self.lifecycle.try_lock() else {
            if let Ok(mut pending) = self.pending_config.lock() {
                *pending = Some(config);
            }
            return;
        };
        if let Ok(mut pending) = self.pending_config.lock() {
            *pending = None;
        }
        let reload = !Self::same_model(&self.config_snapshot(), &config);
        if let Ok(mut current) = self.config.lock() {
            *current = config;
        }
        if !reload {
            return;
        }
        #[cfg(feature = "reverb-tts")]
        if let Ok(mut model) = self.model.lock() {
            *model = None;
        }
        if let Ok(mut last) = self.last_voice.lock() {
            *last = None;
        }
        if let Ok(mut cache) = self.codes.lock() {
            *cache = super::worker::ConditioningCache::new(self.config_snapshot().cache_capacity);
        }
        if let Ok(mut state) = self.state.lock() {
            state.loaded_model = None;
            state.load_error = None;
            state.device = None;
        }
    }

    pub fn sync_config(&self, next: ReverbTtsConfig) {
        let changed = self.config.lock().map(|c| *c != next).unwrap_or(true);
        if changed || self.reload_pending() {
            self.reconfigure(next);
        }
    }

    fn check_model(model: &str) -> Result<(), ProviderError> {
        if REVERB_TTS_MODELS.contains(&model) {
            Ok(())
        } else {
            Err(ProviderError::unsupported_model(
                REVERB_TTS_PROVIDER_ID,
                model,
                format!("expected one of: {}", REVERB_TTS_MODELS.join(", ")),
            ))
        }
    }

    pub fn check_model_language(model: &str, language: &str) -> Result<(), ProviderError> {
        Self::check_model(model)?;
        if REVERB_LANGUAGES.contains(&language) {
            Ok(())
        } else {
            Err(ProviderError::unsupported_language(
                REVERB_TTS_PROVIDER_ID,
                language,
                REVERB_LANGUAGES,
            ))
        }
    }

    pub fn validate_bundle(model_dir: &Path) -> Result<(), ProviderError> {
        let missing: Vec<&str> = REVERB_TTS_REQUIRED_FILES
            .iter()
            .copied()
            .filter(|file| !model_dir.join(file).is_file())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(ProviderError::model_missing(format!(
                "reverb-tts ONNX bundle incomplete in '{}': missing {} \
                 (enable [providers.reverb_tts] to download the pinned files)",
                model_dir.display(),
                missing.join(", ")
            )))
        }
    }

    pub fn validate_reference(pcm: &[f32], sample_rate: u32) -> Result<(), ProviderError> {
        if pcm.iter().any(|s| !s.is_finite()) {
            return Err(ProviderError::invalid_voice(
                "reference samples must be finite",
            ));
        }
        if pcm.is_empty() {
            return Err(ProviderError::invalid_voice(
                "reference audio is empty; provide 3-10 s of clean speech from one speaker",
            ));
        }
        if !(8000..=192_000).contains(&sample_rate) {
            return Err(ProviderError::invalid_voice(
                "reference sample rate must be 8000..=192000 Hz",
            ));
        }
        let secs = pcm.len() as f64 / f64::from(sample_rate);
        if secs < REVERB_REF_MIN_SECS {
            return Err(ProviderError::invalid_voice(format!(
                "reference audio is too short ({secs:.1}s); provide 3-10 s (minimum {REVERB_REF_MIN_SECS:.0}s)"
            )));
        }
        if secs > REVERB_REF_MAX_SECS {
            return Err(ProviderError::invalid_voice(format!(
                "reference audio is too long ({secs:.1}s); keep it under {REVERB_REF_MAX_SECS:.0}s \
                 (3-10 s works best)"
            )));
        }
        Ok(())
    }

    /// Reference transcript as the model reads it (brackets dropped, single spaces).
    pub fn prepare_transcript(transcript: &str) -> Result<String, ProviderError> {
        let prepared = text::prepare_text(transcript);
        if prepared.is_empty() {
            return Err(ProviderError::invalid_voice(
                "reverb-tts needs the exact transcript of the reference recording (mode icl)",
            ));
        }
        Ok(prepared)
    }

    fn reference_hash(pcm: &[f32], sample_rate: u32) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(sample_rate.to_le_bytes());
        for sample in pcm {
            hasher.update(sample.to_le_bytes());
        }
        hex::encode(hasher.finalize())
    }

    fn conditioning_key(&self, reference_hash: &str) -> String {
        super::worker::ConditioningCache::cache_key(
            REVERB_TTS_PROVIDER_ID,
            &format!(
                "{}|{}|{}",
                self.config_snapshot().revision,
                crate::tts::audio::REFERENCE_RESAMPLER_VERSION,
                REVERB_PROMPT_PREP_VERSION
            ),
            reference_hash,
            None,
            "icl",
        )
    }

    /// GPU (Vulkan through the WebGPU EP) is opt-in: `device = "auto"`, a build with
    /// `webgpu`, and neither `SONICBOOM_EP=cpu` nor `SONICBOOM_REVERB_EP=cpu`.
    fn wants_gpu(config: &ReverbTtsConfig) -> bool {
        let forced_cpu =
            |name: &str| std::env::var(name).is_ok_and(|v| v.eq_ignore_ascii_case("cpu"));
        cfg!(feature = "webgpu")
            && config.device == DevicePreference::Auto
            && !forced_cpu("SONICBOOM_EP")
            && !forced_cpu("SONICBOOM_REVERB_EP")
    }

    fn load_expected(
        &self,
        model_id: &str,
        expected: &ReverbTtsConfig,
    ) -> Result<(), ProviderError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| ProviderError::inference_failed("reverb-tts lifecycle lock poisoned"))?;
        if self.model_change_pending() || !Self::same_model(&self.config_snapshot(), expected) {
            return Err(ProviderError::loading("obsolete reverb-tts load discarded"));
        }
        if self
            .state
            .lock()
            .map_err(|_| ProviderError::inference_failed("reverb-tts state lock poisoned"))?
            .loaded_model
            .is_some()
        {
            return Ok(());
        }
        Self::check_model(model_id)?;
        let config = self.config_snapshot();
        Self::validate_bundle(&config.model_dir)?;
        #[cfg(feature = "reverb-tts")]
        {
            super::assets::verify_local_files(&config.model_dir, &reverb_manifest())?;
            let _load_resource = super::worker::model_load_guard();
            let load_started = std::time::Instant::now();
            let native = native::ReverbNative::open(
                &config.model_dir,
                config.cpu_threads,
                Self::wants_gpu(&config),
            )?;
            if self.model_change_pending() {
                return Err(ProviderError::loading("obsolete reverb-tts load discarded"));
            }
            let device = if native.gpu { "vulkan" } else { "cpu" };
            tracing::info!(
                device,
                elapsed_ms = load_started.elapsed().as_millis() as u64,
                "reverb-tts model loaded"
            );
            if let Ok(mut slot) = self.model.lock() {
                *slot = Some(native);
            }
            if let Ok(mut state) = self.state.lock() {
                state.loaded_model = Some(model_id.to_string());
                state.load_error = None;
                state.device = Some(device);
            }
            Ok(())
        }
        #[cfg(not(feature = "reverb-tts"))]
        {
            let _ = config;
            Err(ProviderError::model_missing(
                "reverb-tts files are present but this build omits the native backend; \
                 rebuild with `--features reverb-tts`",
            ))
        }
    }

    pub fn ensure_background(self: &Arc<Self>, hf_token: Option<String>) {
        {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if state.ensuring || state.loaded_model.is_some() {
                return;
            }
            state.ensuring = true;
            state.download_progress = 0.0;
            state.downloading = false;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let retry_token = hf_token.clone();
            let result = this.ensure_async(hf_token).await;
            if let Ok(mut state) = this.state.lock() {
                state.ensuring = false;
                state.downloading = false;
                match &result {
                    Ok(()) => state.load_error = None,
                    Err(error) if error.message().contains("obsolete") => state.load_error = None,
                    Err(error) => state.load_error = Some(error.message().to_string()),
                }
            }
            if result
                .as_ref()
                .is_err_and(|e| e.message().contains("obsolete"))
            {
                this.kick_ensure_if_needed(retry_token);
            }
            if let Err(error) = &result {
                tracing::error!("reverb-tts background prepare failed: {error}");
            }
        });
    }

    async fn ensure_async(self: &Arc<Self>, hf_token: Option<String>) -> Result<(), ProviderError> {
        if !cfg!(feature = "reverb-tts") {
            return Err(ProviderError::model_missing(
                "native backend is not compiled; rebuild with --features reverb-tts",
            ));
        }
        let config = self.config_snapshot();
        if !config.enabled {
            return Err(ProviderError::model_missing(
                "reverb-tts provider is disabled ([providers.reverb_tts] enabled=false)",
            ));
        }
        let progress_self = Arc::clone(self);
        let download_self = Arc::clone(self);
        super::assets::ensure_pinned_files(
            REVERB_TTS_PROVIDER_ID,
            &config.model_dir,
            &reverb_manifest(),
            hf_token.as_deref(),
            move |p| {
                if let Ok(mut state) = progress_self.state.lock() {
                    state.download_progress = p;
                }
            },
            move || {
                if let Ok(mut state) = download_self.state.lock() {
                    state.downloading = true;
                }
            },
            || false,
        )
        .await?;
        let loader = Arc::clone(self);
        let model_id = config.model_id.clone();
        tokio::task::spawn_blocking(move || loader.load_expected(&model_id, &config))
            .await
            .map_err(|e| {
                ProviderError::inference_failed(format!("reverb-tts load task failed: {e}"))
            })?
    }

    fn is_resident(&self) -> bool {
        let loaded = self
            .state
            .lock()
            .map(|s| s.loaded_model.is_some())
            .unwrap_or(false);
        #[cfg(feature = "reverb-tts")]
        return loaded && self.model.lock().map(|m| m.is_some()).unwrap_or(false);
        #[cfg(not(feature = "reverb-tts"))]
        loaded
    }

    pub fn kick_ensure_if_needed(self: &Arc<Self>, hf_token: Option<String>) {
        if self.reload_pending() {
            return;
        }
        let resident = self.is_resident();
        let needs = {
            let config = self.config_snapshot();
            let Ok(state) = self.state.lock() else {
                return;
            };
            config.enabled && !resident && state.load_error.is_none() && !state.ensuring
        };
        if needs {
            self.ensure_background(hf_token);
        }
    }

    #[cfg(feature = "reverb-tts")]
    fn require_loaded(&self) -> Result<(), ProviderError> {
        if self.model.lock().ok().is_some_and(|m| m.is_some()) {
            return Ok(());
        }
        let state = self.state.lock().ok();
        match state {
            Some(state) if state.ensuring && state.download_progress >= 1.0 => Err(
                ProviderError::loading("reverb-tts files verified; the model is loading"),
            ),
            Some(state) if state.ensuring => Err(ProviderError::loading(format!(
                "reverb-tts files are downloading or verifying ({:.0}% complete)",
                state.download_progress * 100.0
            ))),
            Some(state) => match &state.load_error {
                Some(detail) => Err(ProviderError::inference_failed(format!(
                    "reverb-tts model unavailable: {detail}"
                ))),
                None => Err(ProviderError::loading("reverb-tts model is not loaded yet")),
            },
            None => Err(ProviderError::loading("reverb-tts model is not loaded yet")),
        }
    }
}

impl TtsProvider for ReverbTtsProvider {
    fn id(&self) -> &'static str {
        REVERB_TTS_PROVIDER_ID
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            provider_id: REVERB_TTS_PROVIDER_ID.to_string(),
            models: REVERB_TTS_MODELS.iter().map(|s| s.to_string()).collect(),
            languages: REVERB_LANGUAGES.iter().map(|s| s.to_string()).collect(),
            clone_modes: vec!["icl".to_string()],
            devices: vec!["cpu".to_string(), "auto".to_string()],
            requires_reference_audio: true,
            requires_reference_transcript: true,
            sample_rate: Some(REVERB_TTS_SAMPLE_RATE),
        }
    }

    fn status(&self) -> ProviderStatus {
        let config = self.config_snapshot();
        if !config.enabled {
            return ProviderStatus {
                provider_id: REVERB_TTS_PROVIDER_ID.to_string(),
                enabled: false,
                state: ProviderStatusState::Disabled,
                active_model: None,
                effective_device: None,
                model_revision: Some(config.revision),
                detail: Some(
                    "provider disabled in configuration ([providers.reverb_tts] enabled=false)"
                        .to_string(),
                ),
            };
        }
        let state = self.state.lock().ok();
        let loaded = state.as_ref().and_then(|s| s.loaded_model.clone());
        let error = state.as_ref().and_then(|s| s.load_error.clone());
        let ensuring = state.as_ref().is_some_and(|s| s.ensuring);
        let downloading = state.as_ref().is_some_and(|s| s.downloading);
        let progress = state.as_ref().map(|s| s.download_progress).unwrap_or(0.0);
        let device = state.as_ref().and_then(|s| s.device).unwrap_or("cpu");
        drop(state);
        let installed = Self::validate_bundle(&config.model_dir).is_ok();
        let status = if loaded.is_some() {
            ProviderStatusState::Ready
        } else if error.is_some() {
            ProviderStatusState::Error
        } else if ensuring || installed {
            ProviderStatusState::Loading
        } else {
            ProviderStatusState::NotInstalled
        };
        let detail = error.or_else(|| {
            if loaded.is_some() {
                None
            } else if !cfg!(feature = "reverb-tts") {
                Some("native backend not compiled; rebuild with `--features reverb-tts`".into())
            } else if ensuring && downloading {
                Some(format!("reverb-tts files downloading ({:.0}% complete)", progress * 100.0))
            } else if ensuring {
                Some(format!("reverb-tts files verifying ({:.0}% complete)", progress * 100.0))
            } else if installed {
                Some("reverb-tts files present; loading".to_string())
            } else {
                Some(format!(
                    "reverb-tts ONNX files not installed in '{}' (enable [providers.reverb_tts] to download)",
                    config.model_dir.display()
                ))
            }
        });
        ProviderStatus {
            provider_id: REVERB_TTS_PROVIDER_ID.to_string(),
            enabled: true,
            state: status,
            active_model: loaded.or(Some(config.model_id)),
            effective_device: Some(device.to_string()),
            model_revision: Some(config.revision),
            // The web UI validates this block against a fixed shape (Vulkan backend
            // diagnostics), so a provider without those fields must leave it empty:
            // any other JSON makes the whole /api/providers response unreadable.
            detail,
        }
    }

    fn load(&self, model_id: &str) -> Result<(), ProviderError> {
        Self::check_model(model_id)?;
        let config = self.config_snapshot();
        Self::validate_bundle(&config.model_dir)?;
        self.load_expected(model_id, &config)
    }

    fn unload(&self) {
        let Ok(_lifecycle) = self.lifecycle.lock() else {
            return;
        };
        #[cfg(feature = "reverb-tts")]
        if let Ok(mut model) = self.model.lock() {
            *model = None;
        }
        if let Ok(mut last) = self.last_voice.lock() {
            *last = None;
        }
        if let Ok(mut state) = self.state.lock() {
            state.loaded_model = None;
            state.device = None;
        }
    }

    fn invalidate_voice(&self, voice: &str) {
        if let Ok(mut last) = self.last_voice.lock()
            && last.as_ref().is_some_and(|(id, _)| id == voice)
            && let Some((_, key)) = last.take()
            && let Ok(mut cache) = self.codes.lock()
        {
            cache.remove(&key);
        }
    }

    fn prepare_reference(
        &self,
        model_id: &str,
        reference_pcm: &[f32],
        reference_rate: u32,
        transcript: Option<&str>,
        _mode: CloneMode,
    ) -> Result<String, ProviderError> {
        Self::check_model(model_id)?;
        Self::validate_reference(reference_pcm, reference_rate)?;
        Self::prepare_transcript(transcript.unwrap_or(""))?;
        Ok(self.conditioning_key(&Self::reference_hash(reference_pcm, reference_rate)))
    }

    fn synthesize(&self, request: &ProviderRequest) -> Result<PcmAudio, ProviderError> {
        Self::check_model_language(&request.model, &request.language)?;
        if !matches!(
            request.device,
            DevicePreference::Cpu | DevicePreference::Auto
        ) {
            return Err(ProviderError::device_unavailable(
                "provider 'reverb-tts' runs on 'cpu' or 'auto' (Vulkan); CUDA is unsupported",
            ));
        }
        if request.text.trim().is_empty() {
            return Err(ProviderError::invalid_voice("text cannot be empty"));
        }
        let reference = request.reference_audio.as_ref().ok_or_else(|| {
            ProviderError::invalid_voice(
                "reverb-tts cloning requires reference audio; synthesize through an authorized \
                 voice profile (POST /api/voices/clones, mode icl)",
            )
        })?;
        Self::validate_reference(&reference.pcm, reference.sample_rate)?;
        let transcript =
            Self::prepare_transcript(request.reference_transcript.as_deref().unwrap_or(""))?;
        let config = self.config_snapshot();
        if !config.enabled {
            return Err(ProviderError::model_missing(
                "reverb-tts provider is disabled ([providers.reverb_tts] enabled=false)",
            ));
        }
        Self::validate_bundle(&config.model_dir)?;
        #[cfg(not(feature = "reverb-tts"))]
        {
            let _ = (transcript, reference);
            Err(ProviderError::model_missing(
                "reverb-tts files are present but this build omits the native backend; \
                 rebuild with `--features reverb-tts`",
            ))
        }
        #[cfg(feature = "reverb-tts")]
        {
            self.synthesize_native(request, reference, &transcript, &config)
        }
    }
}

#[cfg(feature = "reverb-tts")]
impl ReverbTtsProvider {
    fn synthesize_native(
        &self,
        request: &ProviderRequest,
        reference: &super::types::ReferenceAudio,
        transcript: &str,
        config: &ReverbTtsConfig,
    ) -> Result<PcmAudio, ProviderError> {
        use rand::SeedableRng;

        let started = std::time::Instant::now();
        let _lifecycle = crate::tts::control::lock_cancellable(&self.lifecycle)
            .map_err(ProviderError::inference_failed)?;
        self.require_loaded()?;
        if self.reload_pending() {
            return Err(ProviderError::loading(
                "model reconfiguration pending; retry after the provider is ready",
            ));
        }
        let control = crate::tts::control::current();
        let device = self
            .state
            .lock()
            .ok()
            .and_then(|s| s.device)
            .unwrap_or("cpu");
        if let Some(control) = &control {
            control.set_context(
                REVERB_TTS_PROVIDER_ID,
                &request.model,
                &request.voice,
                device,
            );
        }
        let mut guard = self
            .model
            .lock()
            .map_err(|_| ProviderError::inference_failed("reverb-tts model lock poisoned"))?;
        let native = guard
            .as_mut()
            .ok_or_else(|| ProviderError::loading("reverb-tts model is not loaded"))?;

        // Reference codes, cached per recording.
        let key =
            self.conditioning_key(&Self::reference_hash(&reference.pcm, reference.sample_rate));
        let cached = self
            .codes
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(&key))
            .and_then(|bytes| native::VoiceCodes::from_bytes(&bytes));
        let reference_cached = cached.is_some();
        let voice = match cached {
            Some(voice) => voice,
            None => {
                let pcm24 = crate::tts::audio::resample_reference(
                    &reference.pcm,
                    reference.sample_rate,
                    REVERB_TTS_SAMPLE_RATE,
                );
                let voice = native.encode_reference(&pcm24)?;
                if let Ok(mut cache) = self.codes.lock() {
                    cache.insert(key.clone(), voice.to_bytes());
                }
                voice
            }
        };
        if let Ok(mut last) = self.last_voice.lock() {
            *last = Some((request.voice.clone(), key));
        }

        let prepared = text::prepare_text(&request.text);
        if prepared.is_empty() {
            return Err(ProviderError::invalid_voice("text cannot be empty"));
        }
        let pieces = text::split_text(
            &prepared,
            transcript,
            voice.pace_frames(),
            text::LONGEST_FRAMES,
            text::PIECE_FRAMES,
        );
        let sampler_config = sampler::SamplerConfig {
            steps: config.steps,
            ..Default::default()
        };
        sampler_config
            .validate()
            .map_err(ProviderError::invalid_voice)?;
        let seed = request.seed.unwrap_or_else(rand::random);
        if let Some(control) = &control {
            control.set_seed(seed);
        }
        let passes = sampler::pass_count(&sampler_config);
        let mut audio: Vec<f32> = Vec::new();
        let mut done_frames = 0usize;
        for (index, piece) in pieces.iter().enumerate() {
            if control.as_ref().is_some_and(|c| c.stopped()) {
                return Err(ProviderError::inference_failed(
                    "inference cancelled or deadline exceeded",
                ));
            }
            let frames = text::estimate_frames(piece, transcript, voice.pace_frames());
            if frames > MAX_PIECE_FRAMES {
                return Err(ProviderError::invalid_voice(
                    "a single sentence is too long for reverb-tts (about 90 s of speech); \
                     add sentence punctuation to split the text",
                ));
            }
            let ids = native.tokenize(&text::prompt(piece, transcript, &request.language))?;
            let prefix = native.run_prefix(&ids, &voice)?;
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed.wrapping_add(index as u64));
            let codes =
                native.generate_codes(&prefix, frames, &sampler_config, &mut rng, |pass| {
                    control.as_ref().is_none_or(|c| {
                        c.update("generating", done_frames + frames * pass / passes)
                    })
                })?;
            done_frames += frames;
            let pcm = native.decode(&codes, frames)?;
            let cut = pcm
                .len()
                .saturating_sub((END_CUT_SECS * f64::from(REVERB_TTS_SAMPLE_RATE)) as usize);
            audio.extend(audio::trim_silence(
                &pcm[..cut],
                REVERB_TTS_SAMPLE_RATE,
                0.1,
            ));
            if audio.len() > MAX_SAMPLES {
                return Err(ProviderError::inference_failed(
                    "reverb-tts output exceeds ten-minute bound",
                ));
            }
        }
        drop(guard);
        let audio_secs = audio.len() as f64 / f64::from(REVERB_TTS_SAMPLE_RATE);
        let elapsed = started.elapsed().as_secs_f64();
        tracing::info!(
            device,
            chars = request.text.chars().count(),
            pieces = pieces.len(),
            audio_s = format!("{audio_secs:.1}"),
            elapsed_s = format!("{elapsed:.1}"),
            speed = format!("{:.2}x", audio_secs / elapsed.max(1e-6)),
            reference_cached,
            "reverb-tts synthesized"
        );
        if audio.is_empty()
            || audio.iter().any(|s| !s.is_finite())
            || !audio.iter().any(|s| s.abs() > 1e-4)
        {
            return Err(ProviderError::inference_failed(
                "reverb-tts synthesis produced no audio",
            ));
        }
        Ok(PcmAudio {
            samples: audio,
            sample_rate: REVERB_TTS_SAMPLE_RATE,
            channels: 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> Arc<ReverbTtsProvider> {
        ReverbTtsProvider::new(ReverbTtsConfig::default())
    }

    #[test]
    fn manifest_is_pinned_and_complete() {
        let manifest = reverb_manifest();
        assert_eq!(manifest.len(), REVERB_TTS_REQUIRED_FILES.len());
        for file in REVERB_TTS_REQUIRED_FILES {
            let entry = manifest.iter().find(|f| f.path == *file).expect(file);
            assert_eq!(entry.sha256.len(), 64, "{file}");
            assert!(entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(entry.url.starts_with("https://huggingface.co/"));
            assert!(entry.url.contains("/resolve/"));
            assert!(entry.size > 0);
        }
        let core = manifest
            .iter()
            .find(|f| f.path == "onnx/core.onnx")
            .unwrap();
        assert!(core.url.contains(REVERB_ONNX_REVISION));
        let encoder = manifest
            .iter()
            .find(|f| f.path == "onnx/codec_encoder.onnx")
            .unwrap();
        assert!(
            encoder.url.contains(REVERB_CODEC_REPO) && encoder.url.contains(REVERB_CODEC_REVISION)
        );
        let tokenizer = manifest
            .iter()
            .find(|f| f.path == "model/tokenizer.json")
            .unwrap();
        assert!(tokenizer.url.contains(REVERB_ORIGINAL_REVISION));
        // No revision is a mutable branch name.
        assert!(manifest.iter().all(|f| !f.url.contains("/main/")));
    }

    #[test]
    fn model_and_language_validation() {
        assert!(ReverbTtsProvider::check_model_language("reverb-tts", "es").is_ok());
        for language in ["en", "de", "fr", "it", "pl", "pt"] {
            assert!(ReverbTtsProvider::check_model_language("reverb-tts", language).is_ok());
        }
        assert_eq!(
            ReverbTtsProvider::check_model_language("reverb-tts", "ja")
                .unwrap_err()
                .code(),
            "unsupported_language"
        );
        assert!(ReverbTtsProvider::check_model_language("other-model", "es").is_err());
    }

    #[test]
    fn reference_validation_enforces_length_and_finite_samples() {
        let ok = vec![0.1f32; 24_000 * 5];
        assert!(ReverbTtsProvider::validate_reference(&ok, 24_000).is_ok());
        assert!(ReverbTtsProvider::validate_reference(&[], 24_000).is_err());
        assert!(ReverbTtsProvider::validate_reference(&vec![0.1; 24_000], 24_000).is_err());
        assert!(ReverbTtsProvider::validate_reference(&vec![0.1; 24_000 * 40], 24_000).is_err());
        assert!(ReverbTtsProvider::validate_reference(&ok, 100).is_err());
        let mut nan = ok.clone();
        nan[3] = f32::NAN;
        assert!(ReverbTtsProvider::validate_reference(&nan, 24_000).is_err());
    }

    #[test]
    fn transcript_is_required_and_cleaned() {
        assert_eq!(
            ReverbTtsProvider::prepare_transcript("  Hola   (mundo)  ").unwrap(),
            "Hola mundo"
        );
        assert!(ReverbTtsProvider::prepare_transcript("  ").is_err());
        assert!(ReverbTtsProvider::prepare_transcript("()").is_err());
    }

    #[test]
    fn prepare_reference_requires_icl_and_a_transcript() {
        let p = provider();
        let pcm = vec![0.1f32; 24_000 * 5];
        let key = p
            .prepare_reference(
                "reverb-tts",
                &pcm,
                24_000,
                Some("Hola mundo."),
                CloneMode::Icl,
            )
            .unwrap();
        assert!(!key.is_empty());
        // The key depends on the audio only, not on the transcript.
        let again = p
            .prepare_reference(
                "reverb-tts",
                &pcm,
                24_000,
                Some("Otra cosa."),
                CloneMode::Icl,
            )
            .unwrap();
        assert_eq!(key, again);
        let other = p
            .prepare_reference(
                "reverb-tts",
                &vec![0.2f32; 24_000 * 5],
                24_000,
                Some("x"),
                CloneMode::Icl,
            )
            .unwrap();
        assert_ne!(key, other);
        assert!(
            p.prepare_reference("reverb-tts", &pcm, 24_000, None, CloneMode::Icl)
                .is_err()
        );
        assert!(
            p.prepare_reference("other-model", &pcm, 24_000, Some("x"), CloneMode::Icl)
                .is_err()
        );
    }

    #[test]
    fn disabled_provider_reports_status_and_rejects_synthesis() {
        let p = provider();
        let status = p.status();
        assert!(!status.enabled);
        assert_eq!(status.state, ProviderStatusState::Disabled);
        let mut request = ProviderRequest::new("reverb-tts", "voice", "Hola", "es");
        request.clone_mode = Some(CloneMode::Icl);
        request.reference_transcript = Some("Hola mundo.".into());
        request.reference_audio = Some(super::super::types::ReferenceAudio {
            pcm: vec![0.1; 24_000 * 5],
            sample_rate: 24_000,
        });
        assert_eq!(p.synthesize(&request).unwrap_err().code(), "model_missing");
        // Missing reference audio is an explicit voice error before anything loads.
        request.reference_audio = None;
        assert_eq!(p.synthesize(&request).unwrap_err().code(), "invalid_voice");
        request.text = " ".into();
        assert!(p.synthesize(&request).is_err());
    }

    #[test]
    fn enabled_without_files_is_not_installed() {
        let dir = std::env::temp_dir().join(format!("reverb-empty-{}", uuid::Uuid::new_v4()));
        let p = ReverbTtsProvider::new(ReverbTtsConfig {
            enabled: true,
            model_dir: dir,
            ..Default::default()
        });
        let status = p.status();
        assert_eq!(status.state, ProviderStatusState::NotInstalled);
        assert_eq!(status.active_model.as_deref(), Some("reverb-tts"));
        assert!(status.detail.unwrap().contains("not installed"));
        assert_eq!(p.load("reverb-tts").unwrap_err().code(), "model_missing");
        assert!(p.load("other-model").is_err());
    }

    #[test]
    fn reconfigure_only_reloads_on_model_changes() {
        let p = provider();
        let mut next = ReverbTtsConfig {
            steps: 12,
            cache_capacity: 4,
            ..Default::default()
        };
        p.sync_config(next.clone());
        assert_eq!(p.config_snapshot().steps, 12);
        assert!(!p.reload_pending());
        next.enabled = true;
        p.sync_config(next);
        assert!(p.config_snapshot().enabled);
    }

    /// Token ids from the official Python tokenizer (`transformers.Qwen2Tokenizer`
    /// on Reverb's `model/tokenizer.json`) for the exact upstream prompt layout.
    #[cfg(feature = "reverb-tts")]
    #[test]
    fn tokenizer_matches_the_official_python_ids() {
        let Some(dir) = std::env::var_os("REVERB_TTS_MODEL_DIR") else {
            eprintln!("skipped: set REVERB_TTS_MODEL_DIR to a dir holding model/tokenizer.json");
            return;
        };
        let tokenizer =
            tokenizers::Tokenizer::from_file(Path::new(&dir).join("model/tokenizer.json")).unwrap();
        let cases: &[(&str, &str, &str, &[u32])] = &[
            (
                "Buenos días, ¿cómo estás hoy?",
                "Perdona el retraso, tío.",
                "es",
                &[
                    62028, 62029, 280, 62030, 62031, 3423, 62032, 62033, 3273, 67, 5510, 579, 302,
                    10547, 78, 11, 257, 28738, 13, 43948, 30779, 11, 20401, 66, 45777, 1524, 5777,
                    32959, 30, 62034,
                ],
            ),
            (
                "Hello world! It's 3.5 o'clock.",
                "Hi there, this is a reference.",
                "en",
                &[
                    62028, 62029, 264, 62030, 62031, 3423, 62032, 62033, 10220, 909, 11, 384, 349,
                    260, 4791, 13, 16267, 1602, 0, 937, 530, 220, 18, 13, 20, 289, 40765, 13,
                    62034,
                ],
            ),
            (
                "Größe, Straße & Käse – schön.",
                "Das ist ein Test.",
                "de",
                &[
                    62028, 62029, 411, 62030, 62031, 3423, 62032, 62033, 23180, 4956, 3665, 2877,
                    13, 61105, 11, 19345, 17265, 220, 5, 639, 1960, 309, 220, 158, 222, 241, 47568,
                    13, 62034,
                ],
            ),
            (
                "Zażółć gęślą jaźń, 12 koni.",
                "Cześć, to jest test.",
                "pl",
                &[
                    62028, 62029, 455, 62030, 62031, 3423, 62032, 62033, 34, 2533, 18178, 11, 301,
                    10392, 1089, 13, 1587, 39330, 32163, 6277, 321, 4296, 60400, 4853, 9414, 27397,
                    14407, 11, 220, 16, 17, 12620, 72, 13, 62034,
                ],
            ),
        ];
        for (text, reference, language, expected) in cases {
            let encoding = tokenizer
                .encode(text::prompt(text, reference, language), false)
                .unwrap();
            assert_eq!(encoding.get_ids(), *expected, "{language}: {text}");
        }
    }

    #[test]
    fn capabilities_describe_icl_cloning() {
        let capabilities = provider().capabilities();
        assert_eq!(capabilities.provider_id, "reverb-tts");
        assert_eq!(capabilities.clone_modes, vec!["icl"]);
        assert!(
            capabilities.requires_reference_audio && capabilities.requires_reference_transcript
        );
        assert_eq!(capabilities.sample_rate, Some(24_000));
        assert_eq!(capabilities.languages.len(), 7);
    }
}
