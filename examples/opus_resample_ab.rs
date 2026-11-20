//! Write the same 24 kHz float WAV as Ogg Opus twice: through the previous
//! linear 24 -> 48 kHz upsampling and through the current band-limited one.
//!
//! ```sh
//! cargo run --release --example opus_resample_ab -- in.wav out_prefix
//! ```
use anyhow::Result;
use sonicboom::tts::audio::{encode_opus, resample_pcm_linear};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(args.len() == 3, "usage: opus_resample_ab in.wav out_prefix");
    let mut wav = hound::WavReader::open(&args[1])?;
    let rate = wav.spec().sample_rate;
    let pcm: Vec<f32> = wav
        .samples::<f32>()
        .collect::<std::result::Result<_, _>>()?;
    // Previous behaviour: linear upsample first; 48 kHz input is then a no-op.
    let old = encode_opus(&resample_pcm_linear(&pcm, rate, 48_000), 48_000)?;
    let new = encode_opus(&pcm, rate)?;
    std::fs::write(format!("{}_antes_lineal.ogg", args[2]), old)?;
    std::fs::write(format!("{}_ahora_bandlimited.ogg", args[2]), new)?;
    Ok(())
}
