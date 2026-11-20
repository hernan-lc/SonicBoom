//! Cosine similarity between Qwen speaker embeddings of audio files, as a
//! rough voice-similarity metric for A/B testing synthesis settings.
//!
//! ```sh
//! cargo run --release --example qwen_speaker_sim -- reference.wav a.wav b.wav ...
//! ```
//! The first file is the reference. Inputs may be 16/32-bit int or float WAV.
use std::path::Path;

use anyhow::{Result, ensure};
use sonicboom::tts::providers::qwen_gguf::{codec::Codec, conditioning::MelExtractor};

fn read(path: &str) -> Result<Vec<f32>> {
    let mut wav = hound::WavReader::open(path)?;
    let spec = wav.spec();
    ensure!(spec.channels == 1, "{path}: mono WAV required");
    let pcm: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => wav
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?,
        hound::SampleFormat::Int => wav
            .samples::<i32>()
            .map(|v| v.map(|v| v as f32 / (1u64 << (spec.bits_per_sample - 1)) as f32))
            .collect::<std::result::Result<_, _>>()?,
    };
    Ok(sonicboom::tts::audio::resample_reference(
        &pcm,
        spec.sample_rate,
        24_000,
    ))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb).max(1e-12)
}

fn main() -> Result<()> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        files.len() >= 2,
        "usage: qwen_speaker_sim reference.wav a.wav ..."
    );
    let mut codec = Codec::load(Path::new("models/qwen3-tts-1.7b-gguf"))?;
    let mel = MelExtractor::default();
    let mut embed = |path: &str| -> Result<Vec<f32>> { codec.speaker(mel.extract(&read(path)?)?) };
    let reference = embed(&files[0])?;
    for file in &files[1..] {
        let e = embed(file)?;
        println!(
            "{:.4}  {}",
            cosine(&reference, &e),
            file.rsplit('/').next().unwrap_or(file)
        );
    }
    Ok(())
}
