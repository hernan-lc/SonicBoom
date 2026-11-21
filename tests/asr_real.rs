//! Real-model smoke test and CPU benchmark. Ignored by default: it needs the
//! pinned int8 assets (~801 MB) in `SONICBOOM_ASR_MODEL_DIR` or
//! `models/fluid-speech-pro`. Run with `cargo test --test asr_real -- --ignored --nocapture`.

use std::path::PathBuf;
use std::time::Instant;

use sonicboom::asr::{AsrManager, MODEL_ID};

fn model_dir() -> PathBuf {
    std::env::var_os("SONICBOOM_ASR_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("models/fluid-speech-pro"))
}

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ldc93s1.wav"))
        .expect("LDC93S1 fixture")
}

/// Extend or trim a 16 kHz mono PCM16 WAV to exactly `seconds` seconds.
fn wav_of_length(source: &[u8], seconds: usize) -> Vec<u8> {
    let mut reader = hound::WavReader::new(source).expect("read fixture");
    let spec = reader.spec();
    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>().unwrap();
    let needed = seconds * spec.sample_rate as usize;
    let mut out = Vec::with_capacity(needed);
    while out.len() < needed {
        for sample in &samples {
            if out.len() == needed {
                break;
            }
            out.push(*sample);
        }
    }
    out.truncate(needed);
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for sample in out {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }
    cursor.into_inner()
}

fn peak_rss_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[tokio::test]
#[ignore = "needs pinned ASR weights and a real speech fixture"]
async fn real_model_transcribes_and_benchmarks() {
    let dir = model_dir();
    assert!(
        dir.join("encoder-model.int8.onnx.data").is_file(),
        "set SONICBOOM_ASR_MODEL_DIR to the pinned int8 model directory"
    );
    let manager = AsrManager::new(dir.clone(), None);
    assert_eq!(manager.status().await.model, MODEL_ID);
    let model = manager.ensure_ready().await.expect("download/load model");
    assert_eq!(manager.status().await.state, "ready");

    let source = fixture();
    let started = Instant::now();
    let short = model.transcribe(&source, None).expect("transcribe fixture");
    let short_secs = started.elapsed().as_secs_f64();
    println!("fixture: {:?} in {short_secs:.2}s", short);
    // Cross-checked against onnx-asr 0.12.0 (CPU, same pinned int8 files):
    // it produces the same tokens, including "duck" for this fixture.
    assert_eq!(
        short.text,
        "She had your duck suit and greasy washwater all year."
    );

    let pcm5 = wav_of_length(&source, 5);
    let started = Instant::now();
    let five = model.transcribe(&pcm5, Some("en")).expect("5 s clip");
    let five_secs = started.elapsed().as_secs_f64();

    let pcm30 = wav_of_length(&source, 30);
    let started = Instant::now();
    let thirty = model.transcribe(&pcm30, Some("en")).expect("30 s clip");
    let thirty_secs = started.elapsed().as_secs_f64();

    let total: u64 = dir
        .join("encoder-model.int8.onnx.data")
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0)
        + dir
            .join("decoder_joint-model.int8.onnx.data")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
    println!("5s: {five_secs:.2}s, 30s: {thirty_secs:.2}s, peak RSS: {} KiB", peak_rss_kib());
    println!("weights on disk: {total} bytes");
    assert_eq!(five.language, "en");
    assert_eq!(thirty.language, "en");
}
