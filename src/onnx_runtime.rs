//! Native runtime initialization, execution-provider selection and an
//! offline, real ONNX inference probe.
use std::path::Path;

use anyhow::{Context, Result};
use ort::{ep::ExecutionProviderDispatch, session::builder::SessionBuilder};

/// Load explicitly before any session API (whose implicit loader may panic).
pub fn initialize() -> Result<()> {
    #[cfg(any(target_os = "android", feature = "android-ort"))]
    {
        let path = std::env::var_os("ORT_DYLIB_PATH")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("PREFIX").map(|p| std::path::PathBuf::from(p).join("lib/sonicboom/libonnxruntime.so")))
            .context("Set ORT_DYLIB_PATH to a compatible Android libonnxruntime.so; run scripts/termux/install.sh")?;
        anyhow::ensure!(
            path.is_file(),
            "ONNX Runtime library missing: {}",
            path.display()
        );
        ort::init_from(&path)
            .with_context(|| {
                format!(
                    "Cannot load ONNX Runtime from {} (check Android ARM64 ABI and API version)",
                    path.display()
                )
            })?
            .commit();
    }
    let env =
        ort::environment::Environment::current().context("ONNX Runtime initialization failed")?;
    static SEVERITY: std::sync::Once = std::sync::Once::new();
    SEVERITY.call_once(|| env.set_log_level(log_severity()));
    Ok(())
}

/// ONNX Runtime log severity, applied at the source so filtered messages are
/// never formatted or passed into the Rust log pipeline. The default is
/// `error`: per-session graph notes and per-dispatch WebGPU traces are
/// suppressed. `SONICBOOM_ORT_LOG=warning` shows GPU fallback notes;
/// `info` or `verbose` is for debugging ORT itself.
fn log_severity() -> ort::logging::LogLevel {
    use ort::logging::LogLevel;
    match std::env::var("SONICBOOM_ORT_LOG")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "verbose" | "trace" => LogLevel::Verbose,
        "info" => LogLevel::Info,
        "warning" | "warn" => LogLevel::Warning,
        "fatal" => LogLevel::Fatal,
        _ => LogLevel::Error,
    }
}

/// Process-wide lock for work that creates or runs sessions on the GPU EP.
///
/// Dawn (the WebGPU backend) is not safe against overlapping session creation and
/// execution on some drivers (AMD RADV and D3D12 have both faulted inside
/// `webgpu_dawn.dll`). Everything that creates or runs GPU sessions holds this
/// lock, including all four Supertonic sessions as one bundle.
pub fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether any hardware EP may be registered. Callers use this to keep Dawn
/// work strictly serialized; CPU-only loads can stay parallel.
pub fn gpu_enabled() -> bool {
    !execution_providers(std::path::Path::new("")).is_empty()
}

/// Hardware execution providers in priority order for a model at `model_path`.
///
/// Vulkan (ORT's WebGPU EP, backed by Vulkan on Linux and D3D12 on Windows and
/// Metal on macOS) is the default GPU path. CPU is not listed here on purpose:
/// it is ORT's implicit final fallback. Providers use `error_on_failure()` so
/// a failed registration is *detected* and reported instead of silently
/// leaving the session on CPU (see [`with_gpu`]).
///
/// `SONICBOOM_EP=cpu` forces CPU-only inference (useful for debugging or on
/// machines without a working GPU/driver).
pub fn execution_providers(model_path: &Path) -> Vec<ExecutionProviderDispatch> {
    let force_cpu = std::env::var("SONICBOOM_EP").is_ok_and(|v| v.eq_ignore_ascii_case("cpu"));
    // No provider may be compiled in on minimal/Android builds.
    #[allow(unused_mut)]
    let mut providers = Vec::new();
    // Not every target compiles a provider that needs these values.
    let _ = (model_path, force_cpu);

    // Apple Silicon: CoreML (ANE/GPU) stays the preferred path on macOS builds
    // with the `coreml` feature; the WebGPU EP is registered after it as a
    // second GPU option (and is the only hardware EP without that feature).
    #[cfg(all(target_os = "macos", feature = "coreml"))]
    if !force_cpu {
        use ort::ep::CoreML;
        let cache_dir = model_path.with_extension("mlmodelc");
        providers.push(
            CoreML::default()
                .with_model_cache_dir(cache_dir.to_string_lossy().into_owned())
                .build()
                .error_on_failure(),
        );
    }

    // Explicit vendor EPs (opt-in at build time). They are compiled without
    // WebGPU because no ORT prebuilt binary combines those backends on Linux.
    #[cfg(feature = "cuda")]
    if !force_cpu {
        use ort::ep::CUDA;
        providers.push(CUDA::default().build().error_on_failure());
    }
    #[cfg(feature = "rocm")]
    if !force_cpu {
        use ort::ep::ROCm;
        providers.push(ROCm::default().build().error_on_failure());
    }

    // Vulkan on Linux via Dawn/WebGPU. Enabled by default on desktop; Android
    // builds use their own dynamically loaded ORT 1.24 and skip it entirely.
    #[cfg(all(feature = "webgpu", not(target_os = "android")))]
    if !force_cpu {
        use ort::ep::{WebGPU, webgpu::BufferCacheMode};
        // Buffer-cache modes are configurable because the default bucket
        // cache returned corrupt dynamic-shape decoder output on some RADV
        // drivers. `SONICBOOM_WEBGPU_BUFFER_CACHE=disabled|lazy|simple|bucket`.
        let cache = match std::env::var("SONICBOOM_WEBGPU_BUFFER_CACHE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "disabled" | "off" => BufferCacheMode::Disabled,
            "lazy" | "lazyrelease" => BufferCacheMode::LazyRelease,
            "simple" => BufferCacheMode::Simple,
            _ => BufferCacheMode::Bucket,
        };
        let ep = WebGPU::default()
            .with_storage_buffer_cache_mode(cache)
            .with_uniform_buffer_cache_mode(cache)
            .with_query_resolve_buffer_cache_mode(cache)
            .with_default_buffer_cache_mode(cache);
        // Linux uses Vulkan; Windows uses D3D12; macOS leaves Dawn's default
        // (Metal) alone. All three are the WebGPU EP's native GPU backend.
        #[cfg(target_os = "linux")]
        let ep = ep.with_dawn_backend_type(ort::ep::webgpu::DawnBackendType::Vulkan);
        #[cfg(target_os = "windows")]
        let ep = ep.with_dawn_backend_type(ort::ep::webgpu::DawnBackendType::D3D12);
        providers.push(ep.build().error_on_failure());
    }

    providers
}

/// A session builder plus whether a hardware EP was actually registered.
///
/// `gpu` is `true` only when ORT accepted at least one EP from
/// [`execution_providers`]. Callers use it both to report the real device and
/// to retry session creation on CPU if the GPU session cannot be committed.
pub struct GpuSessionBuilder {
    pub builder: SessionBuilder,
    pub gpu: bool,
}

/// Attach the configured GPU execution providers to a session builder.
///
/// Planned follow-up optimizations (quantized assets, I/O binding, etc.) are
/// tracked in `docs/future-optimizations.md`.
///
/// Hardware EPs are always attempted on default builds (GPU first, CPU as
/// fallback). Each EP is registered separately with `error_on_failure`, so a
/// missing or unusable GPU is reported instead of silently producing a CPU
/// session that is advertised as GPU. A failed registration leaves the
/// builder unchanged and the caller retries on CPU.
pub fn with_gpu(builder: SessionBuilder, model_path: &Path) -> GpuSessionBuilder {
    let mut builder = builder;
    let mut gpu = false;
    for ep in execution_providers(model_path) {
        let name = format!("{ep:?}");
        builder = match builder.with_execution_providers([ep]) {
            Ok(ok) => {
                gpu = true;
                ok
            }
            Err(err) => {
                tracing::warn!("GPU execution provider setup failed ({name}: {err}); falling back to CPU");
                err.recover()
            }
        };
    }
    GpuSessionBuilder { builder, gpu }
}

/// Session creation, execution, output validation and destruction, twice.
/// This tests runtime compatibility, not provider model quality/readiness.
pub fn self_test() -> Result<()> {
    initialize()?;
    for _ in 0..2 {
        let mut session = ort::session::Session::builder()?
            .with_intra_threads(1)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .commit_from_memory(include_bytes!("../assets/runtime-identity.onnx"))?;
        let input = ort::value::Tensor::from_array(([3usize], vec![1.0f32, -2.0, 3.5]))?;
        let output = session.run(ort::inputs!["x" => input])?;
        let (_, values) = output["y"].try_extract_tensor::<f32>()?;
        anyhow::ensure!(
            values == [1.0, -2.0, 3.5],
            "ONNX inference returned incorrect output"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn offline_identity_inference_and_session_recreation() {
        super::self_test().expect("native ONNX runtime must create, run and destroy real sessions");
    }
}
