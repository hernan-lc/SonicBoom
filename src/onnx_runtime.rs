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
    ort::environment::Environment::current().context("ONNX Runtime initialization failed")?;
    Ok(())
}

/// Hardware execution providers in priority order for a model at `model_path`.
///
/// Vulkan (ORT's WebGPU EP, backed by Vulkan on Linux and Metal on macOS) is
/// the default GPU path. CPU is not listed here on purpose: ORT always keeps
/// the CPU EP as the final fallback, both for nodes an EP cannot execute and
/// for entire sessions whose EP registration fails.
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

    // Apple Silicon: CoreML (ANE/GPU) stays the preferred path on macOS; the
    // WebGPU EP is registered after it as a second GPU option.
    #[cfg(target_os = "macos")]
    if !force_cpu {
        use ort::ep::CoreML;
        let cache_dir = model_path.with_extension("mlmodelc");
        providers.push(
            CoreML::default()
                .with_model_cache_dir(cache_dir.to_string_lossy().into_owned())
                .build(),
        );
    }

    // Explicit vendor EPs (opt-in at build time). They are compiled without
    // WebGPU because no ORT prebuilt binary combines those backends on Linux.
    #[cfg(feature = "cuda")]
    if !force_cpu {
        use ort::ep::CUDA;
        providers.push(CUDA::default().build());
    }
    #[cfg(feature = "rocm")]
    if !force_cpu {
        use ort::ep::ROCm;
        providers.push(ROCm::default().build());
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
        providers.push(ep.build());
    }

    providers
}

/// Attach the configured GPU execution providers to a session builder.
///
/// Planned follow-up optimizations (quantized assets, I/O binding, etc.) are
/// tracked in `docs/future-optimizations.md`.
///
/// Provider registration is non-fatal by design: ORT's dispatch defaults to
/// `error_on_failure = false`, so a missing/unusable GPU silently leaves the
/// CPU EP in place. If the builder itself rejects the provider list, we log
/// the reason and continue with a plain (CPU-only) builder.
pub fn with_gpu(builder: SessionBuilder, model_path: &Path) -> SessionBuilder {
    let providers = execution_providers(model_path);
    if providers.is_empty() {
        return builder;
    }
    builder
        .with_execution_providers(providers)
        .unwrap_or_else(|err| {
            tracing::warn!("GPU execution provider setup failed ({err}); falling back to CPU");
            err.recover()
        })
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
