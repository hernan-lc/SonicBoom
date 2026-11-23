use anyhow::{Result, anyhow};
use ort::session::Session;
use serde::Deserialize;
use std::collections::HashMap;

use crate::tts::{download::ModelPaths, text::TextProcessor};

// ============================================================================
// tts.json config
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    pub ae: AeConfig,
    pub ttl: TtlConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AeConfig {
    pub sample_rate: u32,
    pub base_chunk_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtlConfig {
    pub chunk_compress_factor: usize,
    pub latent_dim: usize,
}

// ============================================================================
// Voice style
// ============================================================================

#[derive(Debug, Deserialize)]
struct StyleTensor {
    data: serde_json::Value,
    dims: Vec<usize>,
}

impl StyleTensor {
    fn flatten(&self) -> Vec<f32> {
        fn collect(v: &serde_json::Value, out: &mut Vec<f32>) {
            match v {
                serde_json::Value::Array(arr) => {
                    for item in arr {
                        collect(item, out);
                    }
                }
                serde_json::Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        out.push(f as f32);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        collect(&self.data, &mut out);
        out
    }
}

#[derive(Debug, Deserialize)]
struct VoiceStyleRaw {
    style_ttl: StyleTensor,
    style_dp: StyleTensor,
}

#[derive(Debug)]
pub struct VoiceStyle {
    pub style_ttl: Vec<f32>,
    pub style_ttl_dims: (usize, usize, usize),
    pub style_dp: Vec<f32>,
    pub style_dp_dims: (usize, usize, usize),
}

// ============================================================================
// ModelHandle
// ============================================================================

pub struct ModelHandle {
    pub sessions: std::sync::Mutex<Sessions>,
    pub text_processor: TextProcessor,
    pub voice_styles: HashMap<String, VoiceStyle>,
    pub config: TtsConfig,
}

/// Holds all ONNX sessions together so they can be locked as a unit.
/// This prevents deadlocks and avoids contention between sessions during sequential inference.
pub struct Sessions {
    pub duration_predictor: Session,
    pub text_encoder: Session,
    pub vector_estimator: Session,
    pub vocoder: Session,
}

impl ModelHandle {
    pub fn sample_rate(&self) -> u32 {
        self.config.ae.sample_rate
    }
}

fn build_session(path: &std::path::Path) -> Result<Session> {
    crate::onnx_runtime::initialize()?;
    let mut builder = Session::builder()?;

    #[cfg(target_os = "android")]
    {
        builder = builder
            .with_intra_threads(
                std::env::var("SONICBOOM_CPU_THREADS")
                    .ok()
                    .map(|s| s.parse::<usize>())
                    .transpose()?
                    .unwrap_or(2)
                    .clamp(1, 32),
            )
            .map_err(|e| anyhow!("{e}"))?;
    }

    // Vulkan (WebGPU EP) is tried first on desktop; ORT falls back to CPU for
    // unsupported nodes and when no usable GPU/driver is present.
    builder = crate::onnx_runtime::with_gpu(builder, path);

    // Optional per-session ORT profile (used by the Vulkan compatibility probe).
    if let Ok(profile_dir) = std::env::var("SONICBOOM_PROFILE") {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
        let profile_path = std::path::Path::new(&profile_dir).join(format!("{name}.json"));
        builder = builder
            .with_profiling(profile_path)
            .map_err(|e| anyhow!("profiling setup failed: {e}"))?;
    }

    Ok(builder.commit_from_file(path)?)
}

/// Build one session and log how long ONNX Runtime (graph optimisation plus
/// execution-provider kernel setup/weight upload) took.
fn timed_session(label: &'static str, path: &std::path::Path) -> Result<Session> {
    let started = std::time::Instant::now();
    let session = build_session(path)?;
    tracing::info!(
        session = label,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "ONNX session ready"
    );
    Ok(session)
}

/// Create the four Supertonic graphs.
///
/// `parallel` is only safe for CPU-only builds: Dawn/D3D12 faults with an access
/// violation in `webgpu_dawn.dll` when more than one WebGPU session is created
/// at the same time, even if the whole bundle is inside the GPU lock.
fn create_sessions(paths: &ModelPaths, parallel: bool) -> Result<Sessions> {
    if !parallel {
        // Slowest graph first; each session is fully committed before the next
        // one starts, so Dawn never sees concurrent creation or teardown.
        return Ok(Sessions {
            vector_estimator: timed_session("vector_estimator", &paths.vector_estimator)?,
            vocoder: timed_session("vocoder", &paths.vocoder)?,
            text_encoder: timed_session("text_encoder", &paths.text_encoder)?,
            duration_predictor: timed_session("duration_predictor", &paths.duration_predictor)?,
        });
    }

    // On CPU the graphs are independent, so wall-clock load time is that of the
    // slowest graph instead of the sum of all four.
    std::thread::scope(|scope| -> Result<Sessions> {
        let vector_estimator =
            scope.spawn(|| timed_session("vector_estimator", &paths.vector_estimator));
        let vocoder = scope.spawn(|| timed_session("vocoder", &paths.vocoder));
        let text_encoder = scope.spawn(|| timed_session("text_encoder", &paths.text_encoder));
        let duration_predictor =
            scope.spawn(|| timed_session("duration_predictor", &paths.duration_predictor));
        let join = |handle: std::thread::ScopedJoinHandle<'_, Result<Session>>, label: &str| {
            handle
                .join()
                .map_err(|_| anyhow!("{label} session creation panicked"))?
        };
        Ok(Sessions {
            duration_predictor: join(duration_predictor, "duration_predictor")?,
            text_encoder: join(text_encoder, "text_encoder")?,
            vector_estimator: join(vector_estimator, "vector_estimator")?,
            vocoder: join(vocoder, "vocoder")?,
        })
    })
}

impl ModelHandle {
    pub fn load(paths: &ModelPaths) -> Result<Self> {
        tracing::info!("Loading ONNX sessions...");
        let started = std::time::Instant::now();
        let sessions = if crate::onnx_runtime::gpu_enabled() {
            // One lock for the whole bundle, and no overlapping session
            // creation: this is the shape that survives both RADV and D3D12.
            let _gpu = crate::onnx_runtime::gpu_lock();
            create_sessions(paths, false)?
        } else {
            create_sessions(paths, true)?
        };
        let sessions = std::sync::Mutex::new(sessions);
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "All ONNX sessions created"
        );

        let text_processor = TextProcessor::load(&paths.unicode_indexer)?;

        let config: TtsConfig = serde_json::from_str(&std::fs::read_to_string(&paths.tts_config)?)?;
        tracing::info!(
            "Config: sample_rate={}, base_chunk_size={}, chunk_compress={}, latent_dim={}",
            config.ae.sample_rate,
            config.ae.base_chunk_size,
            config.ttl.chunk_compress_factor,
            config.ttl.latent_dim,
        );

        let mut voice_styles = HashMap::new();
        for (name, path) in &paths.voice_files {
            let data = std::fs::read_to_string(path)?;
            let raw: VoiceStyleRaw = serde_json::from_str(&data)?;
            let ttl_dims = (
                raw.style_ttl.dims[0],
                raw.style_ttl.dims[1],
                raw.style_ttl.dims[2],
            );
            let dp_dims = (
                raw.style_dp.dims[0],
                raw.style_dp.dims[1],
                raw.style_dp.dims[2],
            );
            let style = VoiceStyle {
                style_ttl: raw.style_ttl.flatten(),
                style_ttl_dims: ttl_dims,
                style_dp: raw.style_dp.flatten(),
                style_dp_dims: dp_dims,
            };
            voice_styles.insert(name.clone(), style);
        }

        tracing::info!(
            "Model loaded successfully ({} voice styles)",
            voice_styles.len()
        );

        Ok(Self {
            sessions,
            text_processor,
            voice_styles,
            config,
        })
    }

    /// Run a couple of tiny syntheses so the GPU execution provider compiles
    /// its shaders/pipelines now instead of during the first user request
    /// (measured ~0.8 s on Vulkan/RADV for the very first inference).
    ///
    /// Two different text lengths are used because the WebGPU EP builds
    /// additional pipelines for new tensor shapes. Failures are logged and
    /// ignored: warm-up is an optimisation, never a readiness requirement.
    /// Disable with `SONICBOOM_WARMUP=0`; skipped when forced to CPU.
    pub fn warm_up(&self) {
        let disabled = std::env::var("SONICBOOM_WARMUP").is_ok_and(|v| v == "0");
        let cpu_only = std::env::var("SONICBOOM_EP").is_ok_and(|v| v.eq_ignore_ascii_case("cpu"));
        if disabled || cpu_only {
            return;
        }
        let Some(voice) = self.default_voice() else {
            return;
        };
        let started = std::time::Instant::now();
        const SAMPLES: [&str; 2] = [
            "Hello.",
            "A gentle breeze moved through the open window while everyone listened quietly.",
        ];
        for text in SAMPLES {
            if let Err(error) = crate::tts::inference::synthesize(self, text, "en", voice, 2, 200) {
                tracing::warn!("GPU warm-up synthesis failed (ignored): {error}");
                return;
            }
        }
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Model warm-up complete"
        );
    }

    pub fn default_voice(&self) -> Option<&str> {
        for name in ["M1", "F1", "M2", "F2"] {
            if self.voice_styles.contains_key(name) {
                return Some(name);
            }
        }
        self.voice_styles.keys().next().map(|s| s.as_str())
    }
}
