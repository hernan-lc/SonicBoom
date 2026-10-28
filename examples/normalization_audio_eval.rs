//! Paired raw-vs-normalized Supertonic audio evaluation.
//!
//! This is a manual acoustic-evaluation harness, separate from normalization
//! unit tests: it never asserts speech quality. It prepares each case twice
//! (normalization `off`, then `auto`), synthesizes both through the real
//! [`SupertonicProvider`] boundary, and writes WAVs plus a JSONL manifest that
//! records the exact text the provider received, the normalization identity,
//! the generated chunks, and the unicode-indexer drop count.
//!
//! ```sh
//! cargo run --locked --example normalization_audio_eval --no-default-features
//! ```
//!
//! Environment variables:
//! - `SONICBOOM_MODEL_DIR` (default `models`)
//! - `SONICBOOM_AUDIO_OUT` (default `temp_audio/normalization-eval`)
//! - `SONICBOOM_EVAL_LANGUAGE` (default `es`)
//! - `SONICBOOM_EVAL_LOCALE` (default `es-ES`)
//! - `SONICBOOM_EVAL_STYLE` (default `precise`)
//! - `SONICBOOM_EVAL_VOICE` (default `F1`)
//!
//! Outputs are ignored by git (`temp_audio/`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use sonicboom::tts::{
    audio::{AudioFormat, encode_audio},
    download::ModelPaths,
    model::ModelHandle,
    normalization::{
        NormalizationMode, NormalizeLimits, NormalizeOptions, ReadingStyle, SpeechText, normalize,
    },
    providers::{
        ProviderRequest, TtsProvider,
        supertonic::{SUPERTONIC_MODEL_ID, SupertonicProvider},
    },
    segmentation::{SegmentPolicy, segment_text},
    text::DEFAULT_MAX_CHUNK_CHARS,
};

const CASES: &[(&str, &str)] = &[
    // Padded count / grouping shapes: raw synthesizes the literal glyphs,
    // normalized synthesizes the resolved Spanish reading.
    ("quantity-grouped", "Hay 10.005 solicitudes."),
    ("quantity-padded-five", "Hay 00042 registros."),
    ("quantity-padded-three", "Hay 007 errores."),
    ("quantity-padded-two", "Hay 00 notificaciones."),
    // Identifiers stay literal in both runs; the pair should sound the same.
    ("identifier-standalone", "El código es 007."),
    ("identifier-proper-noun", "007 Bond"),
    // Direct readings isolate model pronunciation from normalization.
    ("baseline-grouped", "diez mil cinco solicitudes."),
    ("baseline-padded-three", "cero cero siete errores."),
    ("baseline-padded-five", "cuarenta y dos registros."),
];

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn model_paths(dir: &Path) -> ModelPaths {
    let voices = ["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"]
        .iter()
        .map(|name| {
            (
                (*name).to_string(),
                dir.join("voice_styles").join(format!("{name}.json")),
            )
        })
        .collect();
    ModelPaths {
        duration_predictor: dir.join("onnx/duration_predictor.onnx"),
        text_encoder: dir.join("onnx/text_encoder.onnx"),
        vector_estimator: dir.join("onnx/vector_estimator.onnx"),
        vocoder: dir.join("onnx/vocoder.onnx"),
        unicode_indexer: dir.join("onnx/unicode_indexer.json"),
        tts_config: dir.join("onnx/tts.json"),
        voice_files: voices,
    }
}

fn options(
    mode: NormalizationMode,
    language: &str,
    locale: &str,
    style: ReadingStyle,
) -> NormalizeOptions {
    NormalizeOptions::resolve_with_style(
        mode,
        language,
        Some(locale),
        NormalizeLimits::default(),
        true,
        style,
    )
    .expect("valid evaluation options")
}

fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<String> {
    let bytes = encode_audio(samples, sample_rate, AudioFormat::Wav)?;
    std::fs::write(path, &bytes)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

fn main() -> Result<()> {
    let model_dir = PathBuf::from(env_or("SONICBOOM_MODEL_DIR", "models"));
    let out_dir = PathBuf::from(env_or(
        "SONICBOOM_AUDIO_OUT",
        "temp_audio/normalization-eval",
    ));
    let language = env_or("SONICBOOM_EVAL_LANGUAGE", "es");
    let locale = env_or("SONICBOOM_EVAL_LOCALE", "es-ES");
    let style = ReadingStyle::parse(&env_or("SONICBOOM_EVAL_STYLE", "precise"))
        .context("SONICBOOM_EVAL_STYLE must be precise, natural or digits")?;
    let voice = env_or("SONICBOOM_EVAL_VOICE", "F1");

    ensure!(
        model_dir.join("onnx/tts.json").is_file(),
        "model directory {} has no onnx/tts.json",
        model_dir.display()
    );
    std::fs::create_dir_all(&out_dir)?;

    let handle = Arc::new(ModelHandle::load(&model_paths(&model_dir))?);
    let provider = SupertonicProvider::new(model_dir.clone());
    provider.attach_ready(Arc::clone(&handle), SUPERTONIC_MODEL_ID);

    let raw_options = options(NormalizationMode::Off, &language, &locale, style);
    let normalized_options = options(NormalizationMode::Auto, &language, &locale, style);
    let mut manifest = String::new();

    for (id, input) in CASES {
        let raw = SpeechText::prepare(input, &raw_options)?;
        let normalized_result = normalize(input, &normalized_options)?;
        let normalized = SpeechText::prepare(input, &normalized_options)?;
        ensure!(
            normalized.as_str() == normalized_result.spoken,
            "prepared text diverged from normalize()"
        );

        let changes: Vec<_> = normalized_result
            .changes
            .iter()
            .map(|change| {
                json!({
                    "kind": change.kind,
                    "before": &input[change.start..change.end],
                    "after": change.replacement,
                })
            })
            .collect();
        let warnings: Vec<_> = normalized_result
            .warnings
            .iter()
            .map(|warning| {
                json!({
                    "code": warning.code.as_str(),
                    "before": &input[warning.start..warning.end],
                })
            })
            .collect();

        let mut runs = Vec::new();
        for (variant, speech) in [("raw", &raw), ("normalized", &normalized)] {
            if variant == "normalized" && normalized.as_str() == raw.as_str() {
                continue;
            }
            let fragments = segment_text(
                speech.as_str(),
                &SegmentPolicy::supertonic(DEFAULT_MAX_CHUNK_CHARS),
            );
            let dropped = handle.text_processor.encode_with_report(speech.as_str()).2;
            let mut request = ProviderRequest::new(
                SUPERTONIC_MODEL_ID,
                voice.clone(),
                speech.as_str(),
                language.clone(),
            );
            request.speech_identity = Some(speech.identity().to_string());
            let audio = provider
                .synthesize(&request)
                .with_context(|| format!("synthesizing {id} ({variant})"))?;
            ensure!(
                audio.is_audible() && audio.samples.iter().all(|s| s.is_finite()),
                "{id} ({variant}) produced silent or non-finite audio"
            );
            let filename = format!("{id}-{variant}.wav");
            let path = out_dir.join(&filename);
            let wav_hash = write_wav(&path, &audio.samples, audio.sample_rate)?;
            let peak = audio
                .samples
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            runs.push(json!({
                "variant": variant,
                "text": speech.as_str(),
                "identity": speech.identity(),
                "wav": filename,
                "wav_sha256": wav_hash,
                "sample_count": audio.samples.len(),
                "sample_rate": audio.sample_rate,
                "duration_secs": audio.duration_secs(),
                "peak": peak,
                "chunks": fragments.iter().map(|f| f.text.clone()).collect::<Vec<_>>(),
                "indexer_dropped_chars": dropped,
            }));
        }

        manifest.push_str(&serde_json::to_string(&json!({
            "id": id,
            "input": input,
            "language": language,
            "locale": locale,
            "reading_style": style.as_str(),
            "voice": voice,
            "ruleset_version": normalized_result.ruleset_version,
            "raw_spoken": raw.as_str(),
            "normalized_spoken": normalized.as_str(),
            "changes": changes,
            "warnings": warnings,
            "runs": runs,
        }))?);
        manifest.push('\n');
    }

    let manifest_path = out_dir.join("manifest.jsonl");
    std::fs::write(&manifest_path, manifest)?;
    println!(
        "wrote {} paired cases to {}",
        CASES.len(),
        manifest_path.display()
    );
    Ok(())
}
