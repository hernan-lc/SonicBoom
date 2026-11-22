//! Round-trip WER of WAV files against a reference text with the local ASR.
//!
//! ```sh
//! cargo run --release --example asr_wer -- "Reference text." en a.wav b.wav ...
//! ```
//!
//! Prints the transcript and word error rate per file. Needs the ASR model
//! (`SONICBOOM_ASR_MODEL_DIR`, downloaded on first use).

use anyhow::{Result, ensure};

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

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 3,
        "usage: asr_wer REFERENCE_TEXT LANG FILE.wav..."
    );
    let (reference, lang) = (&args[0], &args[1]);
    let manager = sonicboom::asr::manager();
    for path in &args[2..] {
        let wav = std::fs::read(path)?;
        let transcription = manager.transcribe(wav, Some(lang.clone())).await?;
        println!(
            "{path}\tWER={:.3}\t{}",
            wer(reference, &transcription.text),
            transcription.text
        );
    }
    Ok(())
}
