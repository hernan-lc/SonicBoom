//! Does the int8 Qwen codec produce the same audio on the WebGPU EP as on CPU?
//! Encodes a reference tone on each device, then decodes the CPU codes on each.
//!
//! ```sh
//! cargo run --release --features qwen-gguf --example qwen_codec_gpu_check
//! ```
use std::path::Path;

use anyhow::Result;
use sonicboom::tts::providers::qwen_gguf::{codec::Codec, models::CODEBOOKS};

fn stats(label: &str, a: &[f32]) {
    let n = a.len().max(1) as f32;
    let rms = (a.iter().map(|v| v * v).sum::<f32>() / n).sqrt();
    let peak = a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!(
        "{label:<34} samples={:<6} rms={rms:.5} peak={peak:.4}",
        a.len()
    );
}

fn main() -> Result<()> {
    let dir = Path::new("models/qwen3-tts-1.7b-gguf");
    let pcm: Vec<f32> = (0..72_000)
        .map(|i| (i as f32 * 0.05).sin() * 0.3 + (i as f32 * 0.013).sin() * 0.2)
        .collect();
    // SAFETY: single-threaded setup before sessions are created.
    unsafe { std::env::set_var("SONICBOOM_QWEN_CODEC_EP", "cpu") };
    let mut cpu = Codec::load(dir)?;
    unsafe { std::env::set_var("SONICBOOM_QWEN_CODEC_EP", "gpu") };
    let mut gpu = Codec::load(dir)?;

    let cpu_codes = cpu.encode(&pcm)?;
    let gpu_codes = gpu.encode(&pcm)?;
    let same = cpu_codes
        .iter()
        .zip(&gpu_codes)
        .flat_map(|(a, b)| a.iter().zip(b))
        .filter(|(x, y)| x == y)
        .count();
    let total = cpu_codes.len().min(gpu_codes.len()) * CODEBOOKS;
    println!(
        "encoder: cpu frames={} gpu frames={} identical codes={same}/{total}",
        cpu_codes.len(),
        gpu_codes.len()
    );

    let frames: Vec<[i64; CODEBOOKS]> = cpu_codes.iter().take(30).copied().collect();
    let a = cpu.decode(&frames)?;
    let b = gpu.decode(&frames)?;
    stats("decode(cpu codes) on CPU", &a);
    stats("decode(cpu codes) on GPU", &b);
    let n = a.len().min(b.len());
    let err: f32 = (0..n).map(|i| (a[i] - b[i]).powi(2)).sum();
    let sig: f32 = (0..n).map(|i| a[i].powi(2)).sum();
    println!(
        "decoder SNR GPU vs CPU: {:.1} dB",
        10.0 * (sig / err.max(1e-20)).log10()
    );
    Ok(())
}
