//! Codec-only noise isolation: encode a WAV (or exact silence) and decode it
//! again, with no talker involved, to see what the int8 codec adds by itself.
//!
//! ```sh
//! cargo run --release --example qwen_codec_roundtrip -- input.wav out.wav   # or "silence" as input
//! ```
use std::path::Path;

use anyhow::Result;
use sonicboom::tts::providers::qwen_gguf::codec::Codec;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3,
        "usage: qwen_codec_roundtrip <in.wav|silence> <out.wav>"
    );
    let pcm: Vec<f32> = if args[1] == "silence" {
        vec![0.0; 72_000]
    } else {
        let mut wav = hound::WavReader::open(&args[1])?;
        let spec = wav.spec();
        let raw: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => wav
                .samples::<f32>()
                .collect::<std::result::Result<_, _>>()?,
            hound::SampleFormat::Int => wav
                .samples::<i32>()
                .map(|v| v.map(|v| v as f32 / (1u64 << (spec.bits_per_sample - 1)) as f32))
                .collect::<std::result::Result<_, _>>()?,
        };
        sonicboom::tts::audio::resample_reference(&raw, spec.sample_rate, 24_000)
    };
    let mut codec = Codec::load(Path::new(
        &std::env::var("QWEN_CODEC_DIR").unwrap_or_else(|_| "models/qwen3-tts-1.7b-gguf".into()),
    ))?;
    let codes = codec.encode(&pcm)?;
    let out = codec.decode(&codes)?;
    let mut writer = hound::WavWriter::create(
        &args[2],
        hound::WavSpec {
            channels: 1,
            sample_rate: 24_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for v in &out {
        writer.write_sample(*v)?;
    }
    writer.finalize()?;
    println!("frames={} in={} out={}", codes.len(), pcm.len(), out.len());
    Ok(())
}
