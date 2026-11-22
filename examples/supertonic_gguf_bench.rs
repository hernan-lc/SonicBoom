//! A/B latency probe: Supertonic 3 via ONNX Runtime vs GGUF via audio.cpp/ggml.
//!
//! Both backends synthesize the same text, voice, seed and flow-step count in
//! one process. Reports cold load, warm-up, median synthesis, RTF and peak RSS.
//! See `benchmarks/supertonic-gguf.md` for the protocol and decision criteria.
//!
//! ```sh
//! cargo run --release --features supertonic-gguf --example supertonic_gguf_bench -- \
//!   "A gentle breeze moved through the open window." 5 5
//! ```

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use audio_cpp::{Backend, ModelFamily, Registry, Request, RunMode, TaskKind};
use futures_util::StreamExt;
use serde::Deserialize;
use sonicboom::tts::{download::ModelPaths, inference, model::ModelHandle};

/// Pinned GGUF asset (mirror of `benchmarks/gguf/pin.json`).
#[derive(Debug, Deserialize)]
struct Pin {
    repo: String,
    revision: String,
    path: String,
    sha256: String,
    size: u64,
}

const SEED: u64 = 42;
const VOICE: &str = "F1";
const WARMUPS: usize = 2;

fn onnx_paths() -> ModelPaths {
    let models = PathBuf::from("models");
    let voice_files = ["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"]
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
        voice_files,
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let mut file = fs::File::open(path).with_context(|| path.display().to_string())?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Download the pinned GGUF once; verify size + SHA-256 before use.
async fn ensure_gguf(pin: &Pin, dest: &Path) -> Result<()> {
    if let Ok(meta) = fs::metadata(dest)
        && meta.len() == pin.size
        && sha256_file(dest)?.eq_ignore_ascii_case(&pin.sha256)
    {
        println!("[gguf] cached model: {}", dest.display());
        return Ok(());
    }
    let url = format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        pin.repo, pin.revision, pin.path
    );
    println!("[gguf] downloading {url}");
    let response = reqwest::get(&url).await?.error_for_status()?;
    let partial = dest.with_extension("gguf.part");
    let mut file = fs::File::create(&partial)?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        use std::io::Write;
        file.write_all(&chunk?)?;
    }
    drop(file);
    let digest = sha256_file(&partial)?;
    let len = fs::metadata(&partial)?.len();
    if len != pin.size || !digest.eq_ignore_ascii_case(&pin.sha256) {
        fs::remove_file(&partial)?;
        anyhow::bail!("GGUF hash/size mismatch: size={len} sha256={digest}");
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&partial, dest)?;
    Ok(())
}

fn median_secs(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn stats(samples: &[f32]) -> String {
    let n = samples.len().max(1) as f32;
    let mean_abs = samples.iter().map(|v| v.abs()).sum::<f32>() / n;
    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / n).sqrt();
    let peak = samples.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    format!("n={n} mean_abs={mean_abs:.5} rms={rms:.5} peak={peak:.5}")
}

fn write_wav(path: &str, samples: &[f32], sample_rate: i32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in samples {
        writer.write_sample((sample.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

fn peak_rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
}

#[derive(Debug, serde::Serialize)]
struct ResultRow {
    backend: &'static str,
    text: String,
    steps: usize,
    trials: usize,
    sample_rate: i32,
    samples: usize,
    load_secs: f64,
    warmup_secs: f64,
    median_synth_secs: f64,
    rtf: f64,
    peak_rss_kb: Option<u64>,
    stats: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let text = args.get(1).cloned().unwrap_or_else(|| {
        "A gentle breeze moved through the open window while everyone listened.".into()
    });
    let steps: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(5);
    let trials: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(5);
    let threads: usize = std::env::var("GGUF_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
        .unwrap_or(4);

    let pin_text = fs::read_to_string("benchmarks/gguf/pin.json")?;
    let pin: Pin = serde_json::from_str(&pin_text)?;
    let gguf_path = std::env::var("GGUF_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("models/gguf/supertonic-3-orig.gguf"));
    ensure_gguf(&pin, &gguf_path).await?;

    let device = std::env::var("BENCH_DEVICE").unwrap_or_else(|_| "cpu".into());
    let backend_label = match device.as_str() {
        "cpu" => "cpu",
        "vulkan" => "vulkan",
        other => anyhow::bail!("BENCH_DEVICE must be 'cpu' or 'vulkan', got '{other}'"),
    };

    // Reproducible noise so both backends see equivalent inputs. `default` is
    // ORT's WebGPU EP (Vulkan on Linux), matching audio.cpp's Vulkan backend.
    // SAFETY: single-threaded setup before any inference session is created.
    unsafe {
        std::env::set_var("SONICBOOM_SEED", SEED.to_string());
        std::env::set_var(
            "SONICBOOM_EP",
            if backend_label == "vulkan" {
                "default"
            } else {
                "cpu"
            },
        );
    }

    let mut rows = Vec::new();

    // ---- ONNX Runtime baseline ----
    let load_started = Instant::now();
    let onnx = ModelHandle::load(&onnx_paths())?;
    let onnx_load = load_started.elapsed().as_secs_f64();
    println!("[onnx] model load: {onnx_load:.3}s");

    let warmup_started = Instant::now();
    for _ in 0..WARMUPS {
        let _ = inference::synthesize(&onnx, &text, "en", VOICE, steps, 200)?;
    }
    let onnx_warmup = warmup_started.elapsed().as_secs_f64();

    let mut onnx_durations = Vec::new();
    let mut onnx_samples = Vec::new();
    for _ in 0..trials {
        let started = Instant::now();
        let samples = inference::synthesize(&onnx, &text, "en", VOICE, steps, 200)?;
        onnx_durations.push(started.elapsed().as_secs_f64());
        onnx_samples = samples;
    }
    let onnx_median = median_secs(onnx_durations);
    let onnx_sr = onnx.sample_rate() as i32;
    let onnx_secs = onnx_samples.len() as f64 / onnx_sr as f64;
    rows.push(ResultRow {
        backend: match backend_label {
            "vulkan" => "onnx-webgpu-vulkan",
            _ => "onnx-cpu",
        },
        text: text.clone(),
        steps,
        trials,
        sample_rate: onnx_sr,
        samples: onnx_samples.len(),
        load_secs: onnx_load,
        warmup_secs: onnx_warmup,
        median_synth_secs: onnx_median,
        rtf: onnx_median / onnx_secs,
        peak_rss_kb: peak_rss_kb(),
        stats: stats(&onnx_samples),
    });

    // ---- GGUF / audio.cpp ----
    let registry = Registry::new()?;
    if !registry.supports_family("supertonic") {
        anyhow::bail!(
            "audio.cpp was built without the supertonic family; rebuild with \
             AUDIOCPP_MODELS=supertonic (see benchmarks/supertonic-gguf.md)"
        );
    }
    let load_started = Instant::now();
    let model = registry.load(
        &gguf_path.to_string_lossy(),
        Some(ModelFamily::Supertonic),
        None,
    )?;
    let gguf_load = load_started.elapsed().as_secs_f64();
    println!("[gguf] model load: {gguf_load:.3}s");

    let ggml_backend = if backend_label == "vulkan" {
        Backend::Vulkan
    } else {
        Backend::Cpu
    };
    let session = model.create_task_session(
        TaskKind::Tts,
        RunMode::Offline,
        ggml_backend,
        0,
        threads as i32,
        None,
    )?;
    // Option names are audio.cpp's Supertonic request keys (docs/tts.md).
    let request = Request::tts(&text)
        .language("en")
        .option("voice_id", VOICE)
        .option("num_inference_steps", steps as u64)
        .option("speed", 1.0f32)
        .option("seed", SEED);

    let run_gguf = || -> Result<(Vec<f32>, i32)> {
        let result = session.run_offline(&request)?;
        let buffer = result
            .audio_output
            .context("GGUF returned no audio_output")?;
        let sample_rate = buffer.sample_rate;
        let samples = buffer.samples.context("GGUF audio_output has no samples")?;
        Ok((samples, sample_rate))
    };

    let warmup_started = Instant::now();
    for _ in 0..WARMUPS {
        let _ = run_gguf()?;
    }
    let gguf_warmup = warmup_started.elapsed().as_secs_f64();

    let mut gguf_durations = Vec::new();
    let mut gguf_samples = Vec::new();
    let mut gguf_sr = 44_100;
    for _ in 0..trials {
        let started = Instant::now();
        let (samples, sample_rate) = run_gguf()?;
        gguf_durations.push(started.elapsed().as_secs_f64());
        gguf_samples = samples;
        gguf_sr = sample_rate;
    }
    let gguf_median = median_secs(gguf_durations);
    let gguf_secs = gguf_samples.len() as f64 / gguf_sr as f64;
    rows.push(ResultRow {
        backend: match backend_label {
            "vulkan" => "gguf-vulkan-audiocpp",
            _ => "gguf-cpu-audiocpp",
        },
        text: text.clone(),
        steps,
        trials,
        sample_rate: gguf_sr,
        samples: gguf_samples.len(),
        load_secs: gguf_load,
        warmup_secs: gguf_warmup,
        median_synth_secs: gguf_median,
        rtf: gguf_median / gguf_secs,
        peak_rss_kb: peak_rss_kb(),
        stats: stats(&gguf_samples),
    });

    let out_dir = PathBuf::from("/tmp/opencode/gguf-bench");
    fs::create_dir_all(&out_dir)?;
    write_wav(
        &out_dir
            .join(format!("onnx-{backend_label}.wav"))
            .to_string_lossy(),
        &onnx_samples,
        onnx_sr,
    )?;
    write_wav(
        &out_dir
            .join(format!("gguf-{backend_label}.wav"))
            .to_string_lossy(),
        &gguf_samples,
        gguf_sr,
    )?;

    for row in &rows {
        println!(
            "[{}] load={:.3}s warmup={:.3}s median={:.3}s rtf={:.3} sr={} {}",
            row.backend,
            row.load_secs,
            row.warmup_secs,
            row.median_synth_secs,
            row.rtf,
            row.sample_rate,
            row.stats
        );
        println!("{}", serde_json::to_string(row)?);
    }
    println!(
        "WAVs: {}/onnx-{backend_label}.wav, {}/gguf-{backend_label}.wav",
        out_dir.display(),
        out_dir.display()
    );
    Ok(())
}
