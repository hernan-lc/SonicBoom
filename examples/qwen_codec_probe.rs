//! Compare the Qwen ONNX codec (int8 speaker encoder, codec encoder, decoder)
//! on CPU vs the default Vulkan-first policy: load time and per-call latency.
//!
//! ```sh
//! cargo run --release --features qwen-gguf --example qwen_codec_probe -- [cpu|default]
//! ```
//!
//! Run each mode as its own process so first-call costs are visible.

use std::{path::Path, time::Instant};

use anyhow::Result;
use sonicboom::tts::providers::qwen_gguf::{codec::Codec, models::CODEBOOKS};

fn main() -> Result<()> {
    let ep = std::env::args().nth(1).unwrap_or_else(|| "default".into());
    // SAFETY: single-threaded setup before any sessions are created.
    unsafe { std::env::set_var("SONICBOOM_EP", &ep) };
    let dir = Path::new("models/qwen3-tts-1.7b-gguf");

    let t = Instant::now();
    let mut codec = Codec::load(dir)?;
    println!("[{ep}] Codec::load            : {:?}", t.elapsed());

    // 3 s of a deterministic 24 kHz tone as the reference clip.
    let pcm: Vec<f32> = (0..72_000)
        .map(|i| (i as f32 * 0.05).sin() * 0.3 + (i as f32 * 0.013).sin() * 0.2)
        .collect();
    let mels: Vec<f32> = (0..300 * 128)
        .map(|i| ((i % 97) as f32) * 0.01 - 0.4)
        .collect();

    let mut codes = Vec::new();
    for round in 1..=3 {
        let t = Instant::now();
        let spk = codec.speaker(mels.clone())?;
        println!(
            "[{ep}] speaker  #{round}          : {:?} ({} dims)",
            t.elapsed(),
            spk.len()
        );
        let t = Instant::now();
        codes = codec.encode(&pcm)?;
        println!(
            "[{ep}] encode   #{round}          : {:?} ({} frames)",
            t.elapsed(),
            codes.len()
        );
    }
    let frames: Vec<[i64; CODEBOOKS]> = codes.iter().take(60).copied().collect();
    for round in 1..=3 {
        let t = Instant::now();
        let audio = codec.decode(&frames)?;
        println!(
            "[{ep}] decode   #{round} (60 fr) : {:?} ({} samples)",
            t.elapsed(),
            audio.len()
        );
    }
    Ok(())
}
