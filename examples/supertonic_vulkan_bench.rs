//! Compatibility + latency probe for Supertonic 3 on ORT's WebGPU EP
//! (which uses Vulkan on Linux). Compares CPU and Vulkan-backed sessions.
//!
//! ```sh
//! cargo run --release --features webgpu --example supertonic_vulkan_bench -- \
//!   "A gentle breeze moved through the open window." 5 4
//! ```

use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use sonicboom::tts::{
    audio, audio::AudioFormat, download::ModelPaths, inference, model::ModelHandle,
};

fn model_paths() -> ModelPaths {
    let models = PathBuf::from("models");
    let voices = ["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"]
        .iter()
        .map(|name| {
            (
                name.to_string(),
                models.join("voice_styles").join(format!("{name}.json")),
            )
        })
        .collect();
    ModelPaths {
        duration_predictor: models.join("onnx").join("duration_predictor.onnx"),
        text_encoder: models.join("onnx").join("text_encoder.onnx"),
        vector_estimator: models.join("onnx").join("vector_estimator.onnx"),
        vocoder: models.join("onnx").join("vocoder.onnx"),
        unicode_indexer: models.join("onnx").join("unicode_indexer.json"),
        tts_config: models.join("onnx").join("tts.json"),
        voice_files: voices,
    }
}

fn load(ep: &str, profile_dir: Option<&str>) -> Result<ModelHandle> {
    // SAFETY: single-threaded setup before any sessions are created.
    unsafe {
        std::env::set_var("SONICBOOM_EP", ep);
        match profile_dir {
            Some(dir) => std::env::set_var("SONICBOOM_PROFILE", dir),
            None => std::env::remove_var("SONICBOOM_PROFILE"),
        }
    }
    let started = Instant::now();
    let handle = ModelHandle::load(&model_paths())?;
    println!("[{ep}] model load: {:?}", started.elapsed());
    Ok(handle)
}

fn synth(
    handle: &ModelHandle,
    text: &str,
    steps: usize,
) -> Result<(Vec<f32>, std::time::Duration)> {
    let started = Instant::now();
    let samples = inference::synthesize(handle, text, "en", "F1", steps, 200)?;
    Ok((samples, started.elapsed()))
}

fn stats(samples: &[f32]) -> String {
    let n = samples.len().max(1) as f32;
    let mean_abs = samples.iter().map(|v| v.abs()).sum::<f32>() / n;
    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / n).sqrt();
    let peak = samples.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    format!(
        "n={} mean_abs={mean_abs:.5} rms={rms:.5} peak={peak:.5}",
        samples.len()
    )
}

fn diff(a: &[f32], b: &[f32]) -> String {
    let n = a.len().min(b.len()).max(1);
    let mut max = 0.0f32;
    let mut sum = 0.0f32;
    for i in 0..n {
        let d = (a[i] - b[i]).abs();
        max = max.max(d);
        sum += d;
    }
    format!("n={n} max_abs={max:.6} mean_abs={:.6}", sum / n as f32)
}

fn provider_counts(dir: &str, model: &str) -> Vec<(String, usize)> {
    let Ok(mut files) = std::fs::read_dir(dir) else {
        return vec![("profile-dir-missing".into(), 0)];
    };
    let mut paths: Vec<PathBuf> = files
        .by_ref()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{model}.json")))
        })
        .collect();
    paths.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
    let Some(path) = paths.last() else {
        return vec![("profile-missing".into(), 0)];
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![("profile-unreadable".into(), 0)];
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return vec![("profile-invalid".into(), 0)];
    };
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    if let Some(entries) = json.as_array() {
        for entry in entries {
            if entry.get("cat").and_then(|v| v.as_str()) != Some("Node") {
                continue;
            }
            let name = entry
                .get("args")
                .and_then(|a| a.get("provider"))
                .and_then(|v| v.as_str())
                .unwrap_or("<none>");
            *counts.entry(name.to_string()).or_default() += 1;
        }
    }
    counts.into_iter().collect()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let text = args.get(1).cloned().unwrap_or_else(|| {
        "A gentle breeze moved through the open window while everyone listened.".into()
    });
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let trials: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);

    let profile_dir = "/tmp/opencode/vulkan-probe/profiles";
    let gpu_profile_dir = format!("{profile_dir}/webgpu");
    std::fs::create_dir_all(&gpu_profile_dir)?;
    // Reproducible initial noise so CPU and GPU see identical inputs.
    // SAFETY: single-threaded setup before any sessions are created.
    unsafe { std::env::set_var("SONICBOOM_SEED", "42") };

    let cpu = load("cpu", Some(profile_dir))?;
    // Any value other than `cpu` uses the default Vulkan-first policy.
    let vulkan = load("default", Some(&gpu_profile_dir))?;

    for (label, handle) in [("cpu", &cpu), ("webgpu-vulkan", &vulkan)] {
        for _ in 0..2 {
            let _ = synth(handle, &text, steps)?; // warm-up
        }
        let mut durations = Vec::new();
        let mut last = Vec::new();
        for _ in 0..trials {
            let (samples, elapsed) = synth(handle, &text, steps)?;
            durations.push(elapsed.as_secs_f64());
            last = samples;
        }
        durations.sort_by(f64::total_cmp);
        let median = durations[durations.len() / 2];
        println!(
            "[{label}] median={median:.3}s samples={:?} stats={}",
            durations,
            stats(&last)
        );
        let wav = audio::encode_audio(&last, handle.sample_rate(), AudioFormat::Wav)?;
        std::fs::write(format!("/tmp/opencode/vulkan-probe/{label}.wav"), wav)?;
    }

    let (cpu_samples, _) = synth(&cpu, &text, steps)?;
    let (gpu_samples, _) = synth(&vulkan, &text, steps)?;
    let (cpu_samples2, _) = synth(&cpu, &text, steps)?;
    println!("cpu vs webgpu : {}", diff(&cpu_samples, &gpu_samples));
    println!("cpu vs cpu    : {}", diff(&cpu_samples, &cpu_samples2));

    drop(cpu);
    drop(vulkan);
    println!(
        "CPU profile     : {:?}",
        provider_counts(profile_dir, "duration_predictor")
    );
    println!(
        "WebGPU profile  : {:?}",
        provider_counts(&format!("{profile_dir}/webgpu"), "duration_predictor")
    );
    println!(
        "WebGPU profile (vector_estimator): {:?}",
        provider_counts(&format!("{profile_dir}/webgpu"), "vector_estimator")
    );
    Ok(())
}
