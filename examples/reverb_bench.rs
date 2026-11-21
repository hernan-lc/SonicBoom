//! Latency probe for the Reverb-TTS provider: load time, then repeated syntheses
//! with a per-stage breakdown (the provider logs it on the `reverb-tts synthesized` line).
//!
//! ```sh
//! REVERB_DEVICE=cpu REVERB_THREADS=6 cargo run --release --example reverb_bench -- \
//!   voices/<reference>.wav "Exact transcript of the reference." "Text to speak." 3
//! ```
//!
//! Arguments: reference WAV, its transcript, text, runs (default 3). The first run
//! encodes the reference; later runs hit the conditioning cache. Env: `REVERB_DEVICE`
//! (`cpu`|`auto`), `REVERB_THREADS`, `REVERB_STEPS`, `REVERB_LANG` (default `en`),
//! `REVERB_MODEL_DIR` (default `models/reverb-tts`).

use std::{path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use sonicboom::tts::providers::{
    TtsProvider,
    reverb_tts::{ReverbTtsConfig, ReverbTtsProvider},
    types::{CloneMode, DevicePreference, ProviderRequest, ReferenceAudio},
};

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn read_wav(path: &str) -> Result<ReferenceAudio> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("open {path}"))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    let pcm = samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    Ok(ReferenceAudio {
        pcm,
        sample_rate: spec.sample_rate,
    })
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() >= 3,
        "usage: reverb_bench REF.wav TRANSCRIPT TEXT [RUNS]"
    );
    let reference = read_wav(&args[0])?;
    let runs: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(3);
    let device = match std::env::var("REVERB_DEVICE").as_deref() {
        Ok("auto") => DevicePreference::Auto,
        _ => DevicePreference::Cpu,
    };
    let config = ReverbTtsConfig {
        enabled: true,
        model_dir: PathBuf::from(env("REVERB_MODEL_DIR", "models/reverb-tts".to_string())),
        device,
        steps: env("REVERB_STEPS", 8),
        cpu_threads: env("REVERB_THREADS", 4),
        ..Default::default()
    };
    println!(
        "device={:?} threads={} steps={}",
        config.device, config.cpu_threads, config.steps
    );
    let provider: Arc<ReverbTtsProvider> = ReverbTtsProvider::new(config);
    let started = Instant::now();
    provider.load("reverb-tts")?;
    println!("load: {:.2?}", started.elapsed());

    let mut request = ProviderRequest::new(
        "reverb-tts",
        "bench",
        &args[2],
        env("REVERB_LANG", "en".to_string()),
    );
    request.device = device;
    request.clone_mode = Some(CloneMode::Icl);
    request.reference_transcript = Some(args[1].clone());
    request.reference_audio = Some(reference);
    request.seed = Some(42);
    let mut hash = None;
    for run in 0..runs {
        let started = Instant::now();
        let audio = provider.synthesize(&request)?;
        let secs = audio.samples.len() as f64 / f64::from(audio.sample_rate);
        let elapsed = started.elapsed().as_secs_f64();
        let digest: u64 = audio.samples.iter().fold(0xcbf29ce484222325, |h, s| {
            (h ^ u64::from(s.to_bits())).wrapping_mul(0x100000001b3)
        });
        println!(
            "run {run}: {secs:.2}s audio in {elapsed:.2}s = {:.2}x  samples={} hash={digest:016x}{}",
            secs / elapsed,
            audio.samples.len(),
            match hash {
                Some(h) if h == digest => "  (identical to run 0)",
                Some(_) => "  (DIFFERS from run 0)",
                None => "",
            }
        );
        hash.get_or_insert(digest);
        if run == 0 && std::env::var_os("REVERB_WRITE").is_some() {
            let mut writer = hound::WavWriter::create(
                "reverb_bench.wav",
                hound::WavSpec {
                    channels: 1,
                    sample_rate: audio.sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )?;
            for s in &audio.samples {
                writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
            }
            writer.finalize()?;
        }
    }
    Ok(())
}
