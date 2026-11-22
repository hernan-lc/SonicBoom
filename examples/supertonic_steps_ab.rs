//! A/B of Supertonic flow-matching step counts: latency, waveform drift and
//! round-trip WER (local ASR) per step count.
//!
//! Phase 1 times synthesis only (no ASR running), phase 2 transcribes the saved
//! WAVs, so ASR work never pollutes the latency numbers.
//!
//! ```sh
//! cargo run --release --features webgpu --example supertonic_steps_ab -- \
//!   [steps=1,2,3,5] [trials=5] [ep=default|cpu] [asr=1]
//! ```
//!
//! Environment: `SONICBOOM_MODEL_DIR` (default `models`),
//! `SONICBOOM_AUDIO_OUT` (default `temp_audio/steps-ab`).

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;
use sonicboom::tts::{
    audio::{self, AudioFormat},
    download::ModelPaths,
    inference,
    model::ModelHandle,
};

/// (language, voice, text). The reference for WER is the text itself.
const CASES: &[(&str, &str, &str)] = &[
    (
        "en",
        "F1",
        "A gentle breeze moved through the open window while everyone listened.",
    ),
    (
        "en",
        "M1",
        "The quick brown fox jumps over the lazy dog near the river bank.",
    ),
    (
        "en",
        "F2",
        "Please remember to bring your passport and boarding pass to the airport tomorrow morning.",
    ),
    (
        "es",
        "F1",
        "La lluvia caía suavemente sobre los tejados de la ciudad mientras todos dormían.",
    ),
    (
        "es",
        "M2",
        "Mañana por la tarde iremos al mercado para comprar frutas frescas y verduras.",
    ),
];

fn model_paths(root: &Path) -> ModelPaths {
    let voices = ["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"]
        .iter()
        .map(|n| {
            (
                n.to_string(),
                root.join("voice_styles").join(format!("{n}.json")),
            )
        })
        .collect();
    ModelPaths {
        duration_predictor: root.join("onnx/duration_predictor.onnx"),
        text_encoder: root.join("onnx/text_encoder.onnx"),
        vector_estimator: root.join("onnx/vector_estimator.onnx"),
        vocoder: root.join("onnx/vocoder.onnx"),
        unicode_indexer: root.join("onnx/unicode_indexer.json"),
        tts_config: root.join("onnx/tts.json"),
        voice_files: voices,
    }
}

fn drift(a: &[f32], b: &[f32]) -> (f32, f32, f32) {
    let n = a.len().min(b.len()).max(1);
    let (mut max, mut sum, mut sq) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..n {
        let d = (a[i] - b[i]).abs();
        max = max.max(d);
        sum += d;
        sq += d * d;
    }
    (max, sum / n as f32, (sq / n as f32).sqrt())
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Word error rate via Levenshtein distance over words.
fn wer(reference: &str, hypothesis: &str) -> f32 {
    let (r, h) = (words(reference), words(hypothesis));
    if r.is_empty() {
        return 0.0;
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    prev[h.len()] as f32 / r.len() as f32
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let steps_list: Vec<usize> = args
        .get(1)
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .filter(|v: &Vec<usize>| !v.is_empty())
        .unwrap_or_else(|| vec![1, 2, 3, 5]);
    let trials: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let ep = args.get(3).cloned().unwrap_or_else(|| "default".into());
    let run_asr = args.get(4).map(|s| s != "0").unwrap_or(true);
    let reference_steps = *steps_list.iter().max().unwrap();

    let root = PathBuf::from(std::env::var("SONICBOOM_MODEL_DIR").unwrap_or_else(|_| "models".into()));
    let out = PathBuf::from(
        std::env::var("SONICBOOM_AUDIO_OUT").unwrap_or_else(|_| "temp_audio/steps-ab".into()),
    );
    std::fs::create_dir_all(&out)?;

    // SAFETY: single-threaded setup before any sessions exist.
    unsafe {
        std::env::set_var("SONICBOOM_EP", &ep);
        std::env::set_var("SONICBOOM_SEED", "42");
    }
    let handle = ModelHandle::load(&model_paths(&root))?;
    let sr = handle.sample_rate();
    println!("ep={ep} trials={trials} steps={steps_list:?} (drift reference: {reference_steps} steps)");

    // Phase 1: latency + audio. results[case][step_idx] = (samples, median_s)
    let mut results: Vec<Vec<(Vec<f32>, f64)>> = Vec::new();
    for (ci, (lang, voice, text)) in CASES.iter().enumerate() {
        let mut row = Vec::new();
        for &steps in &steps_list {
            // warm-up (shader / pipeline compile, allocator)
            for _ in 0..2 {
                inference::synthesize(&handle, text, lang, voice, steps, 200)?;
            }
            let mut times = Vec::new();
            let mut last = Vec::new();
            for _ in 0..trials {
                let t = Instant::now();
                last = inference::synthesize(&handle, text, lang, voice, steps, 200)?;
                times.push(t.elapsed().as_secs_f64());
            }
            let med = median(&mut times);
            let wav = audio::encode_audio(&last, sr, AudioFormat::Wav)?;
            std::fs::write(out.join(format!("case{ci}_steps{steps}.wav")), wav)?;
            row.push((last, med));
        }
        results.push(row);
    }

    println!("\n== latency (median of {trials}) and drift vs {reference_steps} steps ==");
    println!("case lang voice steps  median_s  audio_s   RTF   max_abs   mean_abs   rms_diff");
    for (ci, (lang, voice, _)) in CASES.iter().enumerate() {
        let reference = &results[ci][steps_list.iter().position(|&s| s == reference_steps).unwrap()].0;
        for (si, &steps) in steps_list.iter().enumerate() {
            let (samples, med) = &results[ci][si];
            let audio_s = samples.len() as f64 / sr as f64;
            let (max, mean, rms) = drift(reference, samples);
            println!(
                "{ci:>4} {lang:>4} {voice:>5} {steps:>5} {med:>9.3} {audio_s:>8.2} {:>5.2} {max:>9.5} {mean:>10.6} {rms:>10.6}",
                med / audio_s
            );
        }
    }
    println!("\n== per-step-count medians across cases ==");
    for (si, &steps) in steps_list.iter().enumerate() {
        let mut m: Vec<f64> = results.iter().map(|r| r[si].1).collect();
        let mut rtf: Vec<f64> = results
            .iter()
            .map(|r| r[si].1 / (r[si].0.len() as f64 / sr as f64))
            .collect();
        println!(
            "steps={steps}: median latency {:.3}s, median RTF {:.3}",
            median(&mut m),
            median(&mut rtf)
        );
    }

    drop(handle);

    // Phase 2: round-trip WER with the local ASR (CPU, after all timing).
    if run_asr {
        println!("\n== round-trip WER (local fluid-speech-pro ASR) ==");
        let mgr = sonicboom::asr::manager();
        let mut totals = vec![(0.0f32, 0usize); steps_list.len()];
        for (ci, (lang, _, text)) in CASES.iter().enumerate() {
            for (si, &steps) in steps_list.iter().enumerate() {
                let wav = std::fs::read(out.join(format!("case{ci}_steps{steps}.wav")))?;
                let tr = mgr.transcribe(wav, Some((*lang).to_string())).await?;
                let w = wer(text, &tr.text);
                totals[si].0 += w;
                totals[si].1 += 1;
                println!("case{ci} steps={steps} WER={w:.3} :: {}", tr.text);
            }
        }
        println!();
        for (si, &steps) in steps_list.iter().enumerate() {
            println!(
                "steps={steps}: mean WER {:.3} over {} cases",
                totals[si].0 / totals[si].1 as f32,
                totals[si].1
            );
        }
    }
    Ok(())
}
