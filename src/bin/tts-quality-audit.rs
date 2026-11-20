//! Explicit offline diagnostics; never logs text or raw references.
use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use sonicboom::tts::{
    audio::{AudioFormat, encode_audio},
    providers::{PcmAudio, ProviderRequest, TtsProvider, types::ReferenceAudio},
    quality::DetectionMode,
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn reference(path: &Path) -> Result<ReferenceAudio> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    ensure!(spec.channels == 1, "reference must be mono");
    let pcm = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => reader
            .samples::<i32>()
            .map(|v| v.map(|s| s as f32 / 2f32.powi(spec.bits_per_sample as i32 - 1)))
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    ensure!(pcm.iter().all(|s| s.is_finite()), "non-finite reference");
    Ok(ReferenceAudio {
        pcm,
        sample_rate: spec.sample_rate,
    })
}
enum Engine {
    Mock,
    Pocket(std::sync::Arc<sonicboom::tts::providers::pocket_tts::PocketTtsProvider>),
    Reverb(std::sync::Arc<sonicboom::tts::providers::reverb_tts::ReverbTtsProvider>),
}
impl Engine {
    fn load(provider: &str, model: &str, dir: &Path) -> Result<Self> {
        match provider {
            "mock" => Ok(Self::Mock),
            "pocket" => {
                use sonicboom::tts::providers::pocket_tts::{PocketTtsConfig, PocketTtsProvider};
                let config = PocketTtsConfig {
                    enabled: true,
                    model_dir: dir.into(),
                    model_id: model.into(),
                    lsd_steps: std::env::var("AUDIT_POCKET_STEPS")
                        .unwrap_or_else(|_| "1".into())
                        .parse()?,
                    temperature: std::env::var("AUDIT_POCKET_TEMPERATURE")
                        .unwrap_or_else(|_| "0.3".into())
                        .parse()?,
                    ..Default::default()
                };
                ensure!(
                    (1..=8).contains(&config.lsd_steps)
                        && config.temperature.is_finite()
                        && (0.0..=2.0).contains(&config.temperature),
                    "invalid Pocket audit settings"
                );
                let p = PocketTtsProvider::new(config);
                p.load(model)?;
                Ok(Self::Pocket(p))
            }
            "reverb" => {
                use sonicboom::tts::providers::DevicePreference;
                use sonicboom::tts::providers::reverb_tts::{ReverbTtsConfig, ReverbTtsProvider};
                let device = match std::env::var("AUDIT_DEVICE")
                    .unwrap_or_else(|_| "cpu".into())
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "cpu" => DevicePreference::Cpu,
                    "vulkan" | "auto" => DevicePreference::Auto,
                    other => bail!("AUDIT_DEVICE must be cpu or vulkan for reverb (got {other})"),
                };
                let p = ReverbTtsProvider::new(ReverbTtsConfig {
                    enabled: true,
                    model_dir: dir.into(),
                    model_id: model.into(),
                    device,
                    steps: std::env::var("AUDIT_REVERB_STEPS")
                        .unwrap_or_else(|_| "8".into())
                        .parse()?,
                    cpu_threads: std::env::var("AUDIT_THREADS")
                        .unwrap_or_else(|_| "6".into())
                        .parse()?,
                    ..Default::default()
                });
                p.load(model)?;
                Ok(Self::Reverb(p))
            }
            _ => bail!("unknown or uncompiled provider {provider}"),
        }
    }
    fn synthesize(&self, request: &ProviderRequest) -> Result<(PcmAudio, serde_json::Value)> {
        match self {
            Self::Mock => {
                // Encodes target identity, not speech. Never a quality score.
                let bytes = Sha256::digest(request.text.as_bytes());
                Ok((
                    PcmAudio::mono(
                        bytes.iter().map(|v| (*v as f32 - 128.0) / 256.0).collect(),
                        24000,
                    ),
                    json!({"termination":"mock", "cache_hit":false}),
                ))
            }
            Self::Pocket(p) => Ok((
                p.synthesize(request)?,
                json!({"termination":"not_exposed", "cache_hit":null}),
            )),
            Self::Reverb(p) => Ok((
                p.synthesize(request)?,
                json!({"termination":"fixed_passes", "cache_hit":null, "device":p.status().effective_device}),
            )),
        }
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 6 || args.iter().any(|arg| arg == "--help") {
        println!(
            "tts-quality-audit PROVIDER MODEL LANGUAGE MODEL_DIR NEW_OUTPUT_DIR [AUTHORIZED_REFERENCE_WAV]\nProviders: mock, pocket, reverb (REVERB_REFERENCE_TRANSCRIPT, AUDIT_DEVICE=cpu|vulkan). Reference positional argument explicitly certifies authorized diagnostic use. No outputs overwritten. Default seed 42. Writes hashed inputs only.\nEnv: AUDIT_SEED, AUDIT_QUALITY (off|report), AUDIT_DEVICE (cpu|vulkan), AUDIT_ASR_JSON (offline ASR transcripts array for echo detection), AUDIT_TEXTS_JSON, AUDIT_ALT_REFERENCE_WAV, AUDIT_ALT_REFERENCE_TRANSCRIPT."
        );
        return Ok(());
    }
    let (provider, model, language, dir, out) = (
        &args[1],
        &args[2],
        &args[3],
        Path::new(&args[4]),
        PathBuf::from(&args[5]),
    );
    let mut request = ProviderRequest::new(model, "audit-authorized-A", "", language);
    request.seed = Some(
        std::env::var("AUDIT_SEED")
            .unwrap_or_else(|_| "42".into())
            .parse::<u64>()
            .context("AUDIT_SEED must be a u64")?,
    );
    let reference_hash = if provider != "mock" {
        let path = Path::new(args.get(6).context("supply authorized reference WAV")?);
        let bytes = fs::read(path)?;
        request.reference_audio = Some(reference(path)?);
        Some(hash(&bytes))
    } else {
        None
    };
    if provider == "pocket" {
        request.clone_mode = Some(sonicboom::tts::providers::CloneMode::AudioPrompt);
    }
    let primary_reference = request.reference_audio.clone();
    let alternate = std::env::var("AUDIT_ALT_REFERENCE_WAV")
        .ok()
        .map(|path| -> Result<_> { Ok((reference(Path::new(&path))?, hash(&fs::read(path)?))) })
        .transpose()?;
    if provider == "reverb" {
        request.clone_mode = Some(sonicboom::tts::providers::CloneMode::Icl);
        request.reference_transcript = Some(
            std::env::var("REVERB_REFERENCE_TRANSCRIPT")
                .context("set exact REVERB_REFERENCE_TRANSCRIPT")?,
        );
    }
    // Deterministic quality pass for every row; default `report` so the
    // manifest always carries numerical audio-health evidence.
    let quality_mode = match std::env::var("AUDIT_QUALITY") {
        Ok(raw) => DetectionMode::parse(raw.trim())
            .with_context(|| format!("AUDIT_QUALITY '{raw}' is not a known mode"))?,
        Err(_) => DetectionMode::Report,
    };
    // Optional offline ASR transcripts (one per text) enable echo detection.
    // The audio path never runs ASR; this is benchmark-only.
    let asr: Option<Vec<String>> = match std::env::var("AUDIT_ASR_JSON") {
        Ok(raw) => Some(serde_json::from_str(&raw).context("AUDIT_ASR_JSON must be an array")?),
        Err(_) => None,
    };
    let ort_build = if provider == "mock" {
        "not used (mock)".to_string()
    } else {
        sonicboom::onnx_runtime::initialize()?;
        ort::info().to_string()
    };
    let device_label = std::env::var("AUDIT_DEVICE").unwrap_or_else(|_| "cpu".into());
    fs::create_dir(&out).context("output directory must be fresh; create parent beforehand")?;
    let mut manifest = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("manifest.jsonl"))?;
    let revision = match provider.as_str() {
        "pocket" => sonicboom::tts::providers::pocket_tts::POCKET_TTS_UPSTREAM_REVISION,
        "reverb" => sonicboom::tts::providers::reverb_tts::REVERB_ONNX_REVISION,
        _ => "mock-v1",
    };
    let git = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    let mut engine = Engine::load(provider, model, dir)?;
    let mut test_hash = None;
    let mut index = String::from(
        "<!doctype html><meta charset=utf-8><title>TTS audit</title><p>Speech-content and listening review pending. Mock audio is not speech.</p>",
    );
    let defaults = [
        "hola",
        "test",
        "mundo",
        "test",
        "test",
        "¿Hola?",
        "sí",
        "no",
        "¿Qué tal?",
        "hola test",
        "Hola mundo. Esto es una prueba.",
    ];
    let mut texts: Vec<String> = match std::env::var("AUDIT_TEXTS_JSON") {
        Ok(value) => serde_json::from_str(&value)?,
        Err(_) => defaults.iter().map(|s| (*s).into()).collect(),
    };
    let alternation_start = texts.len();
    if alternate.is_some() {
        texts.extend(["test".into(), "test".into(), "test".into(), "test".into()]);
    }
    let primary_transcript = request.reference_transcript.clone();
    let alternate_transcript = if provider == "reverb" && alternate.is_some() {
        Some(
            std::env::var("AUDIT_ALT_REFERENCE_TRANSCRIPT")
                .context("set exact alternate transcript")?,
        )
    } else {
        None
    };
    let executable_hash = hash(&fs::read(std::env::current_exe()?)?);
    ensure!(
        !texts.is_empty() && texts.len() <= 100 && texts.iter().all(|s| !s.trim().is_empty()),
        "invalid audit text matrix"
    );
    for (i, text) in texts.iter().enumerate() {
        if i == 3 {
            drop(engine);
            engine = Engine::load(provider, model, dir)?;
        }
        let use_alternate =
            alternate.is_some() && i >= alternation_start && (i - alternation_start) % 2 == 1;
        let effective_reference_hash = if use_alternate {
            let (audio, hash) = alternate.as_ref().unwrap();
            request.reference_audio = Some(audio.clone());
            request.reference_transcript = alternate_transcript.clone();
            request.voice = "audit-authorized-B".into();
            Some(hash.clone())
        } else {
            request.reference_audio = primary_reference.clone();
            request.reference_transcript = primary_transcript.clone();
            request.voice = "audit-authorized-A".into();
            reference_hash.clone()
        };
        request.text = text.clone();
        let started = Instant::now();
        let (audio, metrics) = engine.synthesize(&request)?;
        let elapsed = started.elapsed().as_secs_f64();
        ensure!(
            !audio.samples.is_empty() && audio.samples.iter().all(|s| s.is_finite()),
            "invalid PCM"
        );
        let wav = encode_audio(&audio.samples, audio.sample_rate, AudioFormat::Wav)?;
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav))?;
        ensure!(
            reader.len() as usize == audio.samples.len(),
            "WAV sample count mismatch"
        );
        let wav_hash = hash(&wav);
        if i == 1 {
            test_hash = Some(wav_hash.clone());
        }
        let name = format!("{:02}.wav", i + 1);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(&name))?
            .write_all(&wav)?;
        let n = audio.samples.len() as f64;
        let rms = (audio
            .samples
            .iter()
            .map(|s| (*s as f64).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        // Deterministic quality pass (audio health + optional ASR echo) so the
        // manifest always carries numerical evidence alongside the WAV.
        let asr_row = asr
            .as_ref()
            .and_then(|rows| rows.get(i))
            .map(String::as_str);
        let report = sonicboom::tts::quality::evaluate(
            &audio.samples,
            audio.sample_rate,
            request.reference_transcript.as_deref().unwrap_or(""),
            text,
            asr_row,
            quality_mode,
        );
        let row = json!({"request_id":uuid::Uuid::new_v4().to_string(),"job_id":null,"boundary":"direct-provider","provider":provider,"model":model,"revision":revision,"inference_binary_sha256":executable_hash,"ort_build":ort_build,"git_sha":String::from_utf8_lossy(&git.stdout).trim(),"effective_device":device_label,"language":language,"voice":request.voice,"reference_hash":effective_reference_hash,"text_hash":hash(text.as_bytes()),"seed":request.seed,"reference_preprocessing":sonicboom::tts::audio::REFERENCE_RESAMPLER_VERSION,"mode":request.clone_mode.map(|m|m.as_str()),"pocket_steps":std::env::var("AUDIT_POCKET_STEPS").unwrap_or_else(|_|"1".into()),"pocket_temperature":std::env::var("AUDIT_POCKET_TEMPERATURE").unwrap_or_else(|_|"0.3".into()),"chunk_index":null,"sample_count":audio.samples.len(),"sample_rate":audio.sample_rate,"wav_hash":wav_hash,"output":name,"elapsed_secs":elapsed,"duration_secs":audio.duration_secs(),"rtf":elapsed/audio.duration_secs(),"peak":audio.samples.iter().fold(0f32,|a,b|a.max(b.abs())),"rms":rms,"dc_offset":audio.samples.iter().map(|s| *s as f64).sum::<f64>()/n,"clipping_fraction":audio.samples.iter().filter(|s|s.abs()>=1.0).count() as f64/n,"same_as_first_test":if *text=="test" {Some(test_hash.as_ref()==Some(&wav_hash))}else{None},"quality":report,"asr_available":asr_row.is_some(),"metrics":metrics,"content_review":"NOT RUN"});
        writeln!(manifest, "{row}")?;
        manifest.flush()?;
        index.push_str(&format!(
            "<p>{:02} <audio controls src=\"{name}\"></audio></p>",
            i + 1
        ));
        eprintln!(
            "{}: {:.2}s, {:.2}s audio",
            name,
            elapsed,
            audio.duration_secs()
        );
    }
    fs::write(out.join("listen.html"), index)?;
    Ok(())
}
