//! ONNX Runtime graphs for Reverb-TTS: the shared Qwen3 `core`, the embedding and head
//! graphs (CPU, or the Vulkan/WebGPU EP when requested) and the Higgs codec
//! encoder/decoder (always CPU: its convolution decoder returns near-silence on RADV).
//!
//! Graph contracts are documented in the ONNX repository's model card.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ort::session::{Session, SessionInputValue};
use ort::value::{Tensor, TensorRef};
use rand::rngs::StdRng;

use super::audio;
use super::onnx_patch;
use super::sampler::{self, CODEBOOKS, CODES, SamplerConfig};
use crate::tts::providers::error::ProviderError;

const LAYERS: usize = 24;
const KV_HEADS: usize = 6;
const HEAD_DIM: usize = 128;
const HIDDEN: usize = 1024;
/// Samples per codec frame (24 kHz, 25 frames/s).
pub const SAMPLES_PER_FRAME: usize = 960;
/// Codec sample rate.
pub const SAMPLE_RATE: u32 = 24_000;

fn fail(context: &str, error: impl std::fmt::Display) -> ProviderError {
    ProviderError::inference_failed(format!("reverb-tts {context}: {error}"))
}

/// Build one session, GPU first when requested. Returns the session and
/// whether a hardware EP was really registered/committed.
fn session(path: PathBuf, threads: usize, gpu: bool) -> Result<(Session, bool), ProviderError> {
    let builder = || {
        Session::builder()
            .map_err(|e| fail("session setup", e))?
            .with_intra_threads(threads.clamp(1, 32))
            .map_err(|e| fail("thread setup", e))
    };
    let candidate = if gpu {
        crate::onnx_runtime::with_gpu(builder()?, &path)
    } else {
        crate::onnx_runtime::GpuSessionBuilder {
            builder: builder()?,
            gpu: false,
        }
    };
    let mut builder = candidate.builder;
    // Optional per-session ORT profile, written when the session is dropped
    // (same `SONICBOOM_PROFILE=<dir>` switch as the Supertonic graphs).
    if let Ok(dir) = std::env::var("SONICBOOM_PROFILE") {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
        builder = builder
            .with_profiling(Path::new(&dir).join(format!("{name}.json")))
            .map_err(|e| fail("profiling setup", e))?;
    }
    match builder.commit_from_file(&path) {
        Ok(session) => Ok((session, candidate.gpu)),
        Err(err) if candidate.gpu => {
            tracing::warn!("reverb-tts GPU session failed ({err}); retrying on CPU");
            let mut cpu = Session::builder()
                .map_err(|e| fail("session setup", e))?
                .with_intra_threads(threads.clamp(1, 32))
                .map_err(|e| fail("thread setup", e))?;
            Ok((
                cpu.commit_from_file(&path)
                    .map_err(|e| fail(&format!("load {}", path.display()), e))?,
                false,
            ))
        }
        Err(err) => Err(fail(&format!("load {}", path.display()), err)),
    }
}

fn joined<T>(
    handle: std::thread::ScopedJoinHandle<'_, Result<T, ProviderError>>,
) -> Result<T, ProviderError> {
    handle
        .join()
        .unwrap_or_else(|_| Err(fail("load", "worker panicked")))
}

/// Wall time per stage, summed since the last [`ReverbNative::take_timings`].
#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub encode: Duration,
    pub prefix: Duration,
    pub embed: Duration,
    pub core: Duration,
    pub heads: Duration,
    pub decode: Duration,
    /// Target passes (`embed -> core -> heads`) run.
    pub passes: usize,
}

type Inputs<'a> = Vec<(&'static str, SessionInputValue<'a>)>;

/// Reference codes `[8, frames]` (row-major) and how many frames the speaker speaks.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceCodes {
    pub codes: Vec<i64>,
    pub frames: usize,
    pub speech_frames: usize,
}

impl VoiceCodes {
    /// Frames the target length is estimated from: speech frames, at most all of them.
    pub fn pace_frames(&self) -> usize {
        self.speech_frames.clamp(1, self.frames.max(1))
    }

    /// Cache encoding: u32 frames, u32 speech frames, then u16 codes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.codes.len() * 2);
        out.extend_from_slice(&(self.frames as u32).to_le_bytes());
        out.extend_from_slice(&(self.speech_frames as u32).to_le_bytes());
        for &code in &self.codes {
            out.extend_from_slice(&(code as u16).to_le_bytes());
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let frames = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
        let speech_frames = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
        let body = bytes.get(8..)?;
        if frames == 0 || body.len() != CODEBOOKS * frames * 2 {
            return None;
        }
        let (pairs, _) = body.as_chunks::<2>();
        let codes: Vec<i64> = pairs
            .iter()
            .map(|c| i64::from(u16::from_le_bytes(*c)))
            .collect();
        if codes.iter().any(|&c| c >= CODES as i64) {
            return None;
        }
        Some(Self {
            codes,
            frames,
            speech_frames,
        })
    }
}

/// Keys and values of the prompt prefix (text + reference), reused by every pass.
pub struct Prefix {
    k: Vec<f32>,
    v: Vec<f32>,
    len: usize,
    reference_frames: usize,
}

pub struct ReverbNative {
    tokenizer: tokenizers::Tokenizer,
    embed_prefix: Session,
    core: Session,
    embed_target: Session,
    heads: Session,
    encoder: Session,
    /// CPU codec decoder: the reference for the GPU probe and the fallback.
    decoder: Session,
    /// The decoder with its broken-stride `ConvTranspose` rewritten, on the GPU EP. Only set
    /// after a probe showed it matches the CPU decoder and is faster; see `onnx_patch`.
    decoder_gpu: Option<Session>,
    /// Whether the embedding/core/head graphs run on the GPU EP.
    pub gpu: bool,
    timings: Timings,
}

impl ReverbNative {
    pub fn open(dir: &Path, threads: usize, gpu: bool) -> Result<Self, ProviderError> {
        let _gpu_guard = gpu.then(crate::onnx_runtime::gpu_lock);
        crate::onnx_runtime::initialize().map_err(|e| fail("runtime", format!("{e:#}")))?;
        let onnx = |name: &str| dir.join("onnx").join(name);
        // The codec encoder and the reference decoder run on the CPU. With the transformer on
        // the GPU the cores are otherwise idle, so the codec may use them all (decode ~900 -> ~600 ms at 12 threads).
        let codec_threads = if gpu {
            let cores = std::thread::available_parallelism().map_or(threads, |n| n.get());
            threads.max(cores.min(12))
        } else {
            threads
        };
        // Sessions are built concurrently: graph optimization of the 1 GB core dominates the
        // load and is single threaded, so the codec graphs and the small graphs are built
        // in its shadow. Dawn (the GPU EP) is not safe against overlapping session creation,
        // so GPU sessions are created one after another on a single thread.
        let (tokenizer, embed_prefix, core, embed_target, heads, encoder, decoder) =
            std::thread::scope(|scope| {
                let encoder =
                    scope.spawn(|| session(onnx("codec_encoder.onnx"), codec_threads, false));
                let decoder =
                    scope.spawn(|| session(onnx("codec_decoder.onnx"), codec_threads, false));
                let tokenizer = scope.spawn(|| {
                    tokenizers::Tokenizer::from_file(dir.join("model/tokenizer.json")).map_err(
                        |e| ProviderError::model_missing(format!("reverb-tts tokenizer: {e}")),
                    )
                });
                let core = scope.spawn(|| session(onnx("core.onnx"), threads, gpu));
                let (embed_prefix, embed_target, heads) = if gpu {
                    (
                        session(onnx("embed_prefix.onnx"), threads, gpu),
                        session(onnx("embed_target.onnx"), threads, gpu),
                        session(onnx("heads.onnx"), threads, gpu),
                    )
                } else {
                    let small = |name: &'static str| {
                        scope.spawn(move || session(onnx(name), threads, false))
                    };
                    let (a, b, c) = (
                        small("embed_prefix.onnx"),
                        small("embed_target.onnx"),
                        small("heads.onnx"),
                    );
                    (joined(a), joined(b), joined(c))
                };
                (
                    joined(tokenizer),
                    embed_prefix,
                    joined(core),
                    embed_target,
                    heads,
                    joined(encoder),
                    joined(decoder),
                )
            });
        let (tokenizer, embed_prefix, core, embed_target, heads, encoder, decoder) = (
            tokenizer?,
            embed_prefix?,
            core?,
            embed_target?,
            heads?,
            encoder?,
            decoder?,
        );
        let (embed_prefix, gpu1) = embed_prefix;
        let (embed_target, gpu2) = embed_target;
        let (heads, gpu3) = heads;
        let (core, gpu4) = core;
        let (encoder, _) = encoder;
        let (decoder, _) = decoder;
        // A requested GPU is only real if at least one EP registered and its
        // session committed. Otherwise rebuild everything on CPU so the
        // reported device matches the actual placement.
        let gpu_used = gpu && (gpu1 || gpu2 || gpu3 || gpu4);
        if gpu && !gpu_used {
            tracing::warn!("reverb-tts GPU session unavailable; rebuilding the graphs on the CPU");
            drop(_gpu_guard);
            return Self::open(dir, threads, false);
        }
        let mut native = Self {
            tokenizer,
            embed_prefix,
            core,
            embed_target,
            heads,
            // The codec graphs are convolutional and stay on the CPU.
            encoder,
            decoder,
            decoder_gpu: None,
            gpu: gpu_used,
            timings: Timings::default(),
        };
        if gpu_used && !native.self_check() {
            tracing::warn!("reverb-tts GPU self-check failed; rebuilding the graphs on the CPU");
            drop(_gpu_guard);
            return Self::open(dir, threads, false);
        }
        if gpu_used {
            native.decoder_gpu = native.build_gpu_decoder(&onnx("codec_decoder.onnx"), threads);
        }
        Ok(native)
    }

    /// Build the codec decoder on the GPU when it can be done safely.
    ///
    /// ORT's WebGPU `ConvTranspose` is wrong for some strides on RADV, which makes the stock
    /// decoder near silent on the GPU. The pinned decoder is rewritten in memory (see
    /// `onnx_patch`), run once against the CPU decoder on fixed codes, and kept only when it
    /// matches (>= 40 dB SNR) and is faster. Any failure leaves the CPU decoder in charge.
    /// `SONICBOOM_REVERB_CODEC_GPU=0` disables it.
    fn build_gpu_decoder(&mut self, path: &Path, threads: usize) -> Option<Session> {
        if std::env::var("SONICBOOM_REVERB_CODEC_GPU")
            .is_ok_and(|v| matches!(v.trim(), "0" | "off" | "false" | "no"))
        {
            return None;
        }
        let started = Instant::now();
        let bytes = std::fs::read(path).ok()?;
        let (patched, rewritten) = match onnx_patch::rewrite_conv_transposes(&bytes) {
            Ok(Some(done)) => done,
            Ok(None) => return None,
            Err(reason) => {
                tracing::warn!(%reason, "reverb-tts GPU codec decoder skipped (graph not rewritable)");
                return None;
            }
        };
        drop(bytes);
        let builder = Session::builder()
            .map_err(|e| e.to_string())
            .and_then(|b| {
                b.with_intra_threads(threads.clamp(1, 32))
                    .map_err(|e| e.to_string())
            })
            .inspect_err(
                |error| tracing::warn!(%error, "reverb-tts GPU codec decoder setup failed"),
            )
            .ok()?;
        let mut candidate = crate::onnx_runtime::with_gpu(builder, path);
        if !candidate.gpu {
            tracing::warn!("reverb-tts GPU codec decoder skipped (no hardware EP)");
            return None;
        }
        let mut session = candidate
            .builder
            .commit_from_memory(&patched)
            .inspect_err(
                |error| tracing::warn!(%error, "reverb-tts GPU codec decoder failed to load"),
            )
            .ok()?;
        drop(patched);

        // Deterministic codes: the same input on both decoders.
        let frames = 24;
        let codes: Vec<i64> = (0..CODEBOOKS * frames)
            .map(|i| ((i * 977 + 17) % CODES) as i64)
            .collect();
        let cpu_started = Instant::now();
        let cpu = run_decoder(&mut self.decoder, &codes, frames).ok()?;
        let cpu_time = cpu_started.elapsed();
        run_decoder(&mut session, &codes, frames).ok()?; // pipeline/shader compilation
        let gpu_started = Instant::now();
        let gpu = run_decoder(&mut session, &codes, frames).ok()?;
        let gpu_time = gpu_started.elapsed();

        let signal: f64 = cpu.iter().map(|v| f64::from(*v).powi(2)).sum();
        let noise: f64 = cpu
            .iter()
            .zip(&gpu)
            .map(|(a, b)| f64::from(a - b).powi(2))
            .sum();
        let snr_db = 10.0 * (signal / noise.max(1e-30)).log10();
        if cpu.len() != gpu.len() || !signal.is_finite() || snr_db.is_nan() || snr_db < 40.0 {
            tracing::warn!(
                snr_db,
                cpu_samples = cpu.len(),
                gpu_samples = gpu.len(),
                "reverb-tts GPU codec decoder rejected: output differs from the CPU decoder"
            );
            return None;
        }
        if gpu_time >= cpu_time {
            tracing::info!(
                cpu_ms = cpu_time.as_millis() as u64,
                gpu_ms = gpu_time.as_millis() as u64,
                "reverb-tts GPU codec decoder not faster than the CPU decoder; keeping the CPU"
            );
            return None;
        }
        tracing::info!(
            rewritten_nodes = rewritten,
            snr_db,
            cpu_ms = cpu_time.as_millis() as u64,
            gpu_ms = gpu_time.as_millis() as u64,
            setup_ms = started.elapsed().as_millis() as u64,
            "reverb-tts codec decoder runs on the GPU"
        );
        Some(session)
    }

    /// Cheap sanity check of the (possibly GPU) transformer: finite log-probabilities
    /// whose mass over the real codes is a probability.
    fn self_check(&mut self) -> bool {
        let frames = 8;
        let tokens = vec![sampler::MASK; CODEBOOKS * frames];
        let Ok(lp) = self.target_pass(
            &tokens,
            frames,
            0,
            &[],
            &[],
            &vec![0.0; frames * frames],
            false,
        ) else {
            return false;
        };
        lp.iter().all(|v| v.is_finite())
            && (0..CODEBOOKS * frames).all(|row| {
                let mass: f32 = lp[row * CODES..(row + 1) * CODES]
                    .iter()
                    .map(|v| v.exp())
                    .sum();
                (0.5..=1.001).contains(&mass)
            })
    }

    /// Stage timings since the previous call.
    pub fn take_timings(&mut self) -> Timings {
        std::mem::take(&mut self.timings)
    }

    pub fn tokenize(&self, prompt: &str) -> Result<Vec<i64>, ProviderError> {
        let encoding = self
            .tokenizer
            .encode(prompt, false)
            .map_err(|e| fail("tokenizer", e))?;
        Ok(encoding.get_ids().iter().map(|&id| i64::from(id)).collect())
    }

    /// Encode 24 kHz mono reference PCM to codec codes (upstream `TTS.voice`).
    pub fn encode_reference(&mut self, pcm24: &[f32]) -> Result<VoiceCodes, ProviderError> {
        let started = Instant::now();
        let voice = self.encode_reference_inner(pcm24);
        self.timings.encode += started.elapsed();
        voice
    }

    fn encode_reference_inner(&mut self, pcm24: &[f32]) -> Result<VoiceCodes, ProviderError> {
        let mut wav = audio::raise_quiet(pcm24);
        // Whole codec frames, as the training references were.
        let padded = wav.len().div_ceil(SAMPLES_PER_FRAME) * SAMPLES_PER_FRAME;
        wav.resize(padded, 0.0);
        let wav16 = crate::tts::audio::resample_reference(&wav, SAMPLE_RATE, 16_000);
        let speech = audio::speech_length(&wav, SAMPLE_RATE);
        let frames = wav.len() / SAMPLES_PER_FRAME;
        let wav24_t = TensorRef::from_array_view(([1usize, 1, wav.len()], &*wav))
            .map_err(|e| fail("reference input", e))?;
        let wav16_t = TensorRef::from_array_view(([1usize, wav16.len()], &*wav16))
            .map_err(|e| fail("reference input", e))?;
        let inputs: Inputs = vec![("wav24", wav24_t.into()), ("wav16", wav16_t.into())];
        let outputs = self
            .encoder
            .run(inputs)
            .map_err(|e| fail("reference encoding", e))?;
        let (shape, data) = outputs["codes"]
            .try_extract_tensor::<i64>()
            .map_err(|e| fail("reference codes", e))?;
        if shape.len() != 3 || shape[1] as usize != CODEBOOKS || shape[2] as usize != frames {
            return Err(fail(
                "reference codes",
                format!("unexpected shape {shape:?}, expected [1, 8, {frames}]"),
            ));
        }
        if data.iter().any(|&c| !(0..CODES as i64).contains(&c)) {
            return Err(fail("reference codes", "code out of range"));
        }
        Ok(VoiceCodes {
            codes: data.to_vec(),
            frames,
            speech_frames: ((speech * 25.0).round_ties_even() as usize).clamp(1, frames),
        })
    }

    /// Run the prompt prefix once and keep its keys and values.
    pub fn run_prefix(
        &mut self,
        text_ids: &[i64],
        voice: &VoiceCodes,
    ) -> Result<Prefix, ProviderError> {
        let started = Instant::now();
        let prefix = self.run_prefix_inner(text_ids, voice);
        self.timings.prefix += started.elapsed();
        prefix
    }

    fn run_prefix_inner(
        &mut self,
        text_ids: &[i64],
        voice: &VoiceCodes,
    ) -> Result<Prefix, ProviderError> {
        let ids_t = TensorRef::from_array_view(([1usize, text_ids.len()], text_ids))
            .map_err(|e| fail("prefix input", e))?;
        let codes_t =
            TensorRef::from_array_view(([1usize, CODEBOOKS, voice.frames], &*voice.codes))
                .map_err(|e| fail("prefix input", e))?;
        let inputs: Inputs = vec![("text_ids", ids_t.into()), ("ref_codes", codes_t.into())];
        let outputs = self
            .embed_prefix
            .run(inputs)
            .map_err(|e| fail("prefix embedding", e))?;
        let (shape, x) = outputs["x"]
            .try_extract_tensor::<f32>()
            .map_err(|e| fail("prefix embedding", e))?;
        let len = shape[1] as usize;
        let x = x.to_vec();
        drop(outputs);
        let (_, kv) = self.core_run(&x, len, 0, &[], &[], &vec![0.0; len * len], true)?;
        let (k, v) = kv.expect("keys and values requested");
        Ok(Prefix {
            k,
            v,
            len,
            reference_frames: voice.frames,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn core_run(
        &mut self,
        x: &[f32],
        s: usize,
        past: usize,
        past_k: &[f32],
        past_v: &[f32],
        bias: &[f32],
        want_kv: bool,
    ) -> Result<(Vec<f32>, Option<(Vec<f32>, Vec<f32>)>), ProviderError> {
        let kv_shape = [LAYERS, 1, KV_HEADS, past, HEAD_DIM];
        let inputs: Inputs = vec![
            (
                "x",
                TensorRef::from_array_view(([1usize, s, HIDDEN], x))
                    .map_err(|e| fail("core input", e))?
                    .into(),
            ),
            (
                "past_k",
                TensorRef::from_array_view((kv_shape, past_k))
                    .map_err(|e| fail("core input", e))?
                    .into(),
            ),
            (
                "past_v",
                TensorRef::from_array_view((kv_shape, past_v))
                    .map_err(|e| fail("core input", e))?
                    .into(),
            ),
            (
                "bias",
                TensorRef::from_array_view(([s, past + s], bias))
                    .map_err(|e| fail("core input", e))?
                    .into(),
            ),
        ];
        let outputs = self.core.run(inputs).map_err(|e| fail("core pass", e))?;
        let hidden = outputs["hidden"]
            .try_extract_tensor::<f32>()
            .map_err(|e| fail("core output", e))?
            .1
            .to_vec();
        let kv = if want_kv {
            let k = outputs["new_k"]
                .try_extract_tensor::<f32>()
                .map_err(|e| fail("core output", e))?
                .1
                .to_vec();
            let v = outputs["new_v"]
                .try_extract_tensor::<f32>()
                .map_err(|e| fail("core output", e))?
                .1
                .to_vec();
            Some((k, v))
        } else {
            None
        };
        Ok((hidden, kv))
    }

    /// One pass over the target: tokens `[8, F]` -> log-probabilities `[8, F, 1024]`.
    #[allow(clippy::too_many_arguments)]
    fn target_pass(
        &mut self,
        tokens: &[i64],
        frames: usize,
        past: usize,
        past_k: &[f32],
        past_v: &[f32],
        bias: &[f32],
        _polish: bool,
    ) -> Result<Vec<f32>, ProviderError> {
        let t0 = Instant::now();
        let tokens_t = TensorRef::from_array_view(([1usize, CODEBOOKS, frames], tokens))
            .map_err(|e| fail("target input", e))?;
        let inputs: Inputs = vec![("tokens", tokens_t.into())];
        let outputs = self
            .embed_target
            .run(inputs)
            .map_err(|e| fail("target embedding", e))?;
        let x = outputs["x"]
            .try_extract_tensor::<f32>()
            .map_err(|e| fail("target embedding", e))?
            .1
            .to_vec();
        drop(outputs);
        let t1 = Instant::now();
        let (hidden, _) = self.core_run(&x, frames, past, past_k, past_v, bias, false)?;
        let t2 = Instant::now();
        let hidden_t = Tensor::from_array(([1usize, frames, HIDDEN], hidden))
            .map_err(|e| fail("heads input", e))?;
        let inputs: Inputs = vec![("hidden", hidden_t.into())];
        let outputs = self.heads.run(inputs).map_err(|e| fail("heads pass", e))?;
        let log_probs = outputs["log_probs"]
            .try_extract_tensor::<f32>()
            .map_err(|e| fail("heads output", e))?
            .1
            .to_vec();
        let t3 = Instant::now();
        self.timings.embed += t1 - t0;
        self.timings.core += t2 - t1;
        self.timings.heads += t3 - t2;
        self.timings.passes += 1;
        Ok(log_probs)
    }

    /// Run the sampler over `frames` target frames. `progress(pass)` returns `false`
    /// to cancel. Returns the `[8, frames]` codes (row-major).
    pub fn generate_codes(
        &mut self,
        prefix: &Prefix,
        frames: usize,
        config: &SamplerConfig,
        rng: &mut StdRng,
        mut progress: impl FnMut(usize) -> bool,
    ) -> Result<Vec<i64>, ProviderError> {
        let _gpu_guard = self.gpu.then(crate::onnx_runtime::gpu_lock);
        let stride = prefix.len + frames;
        let plain = vec![0.0f32; frames * stride];
        let mut polish = plain.clone();
        for row in 0..frames {
            for col in prefix.len - prefix.reference_frames..prefix.len {
                polish[row * stride + col] = config.polish_bias;
            }
        }
        let mut pass = 0usize;
        sampler::sample(frames, config, rng, |tokens, books, polishing| {
            pass += 1;
            if !progress(pass) {
                return Err(ProviderError::inference_failed(
                    "inference cancelled or deadline exceeded",
                ));
            }
            let bias = if polishing { &polish } else { &plain };
            let all = self.target_pass(
                tokens, frames, prefix.len, &prefix.k, &prefix.v, bias, polishing,
            )?;
            let row = frames * CODES;
            Ok(all[books.start * row..books.end * row].to_vec())
        })
    }

    /// Decode `[8, frames]` codes to 24 kHz mono samples.
    pub fn decode(&mut self, codes: &[i64], frames: usize) -> Result<Vec<f32>, ProviderError> {
        let started = Instant::now();
        let audio = self.decode_inner(codes, frames);
        self.timings.decode += started.elapsed();
        audio
    }

    fn decode_inner(&mut self, codes: &[i64], frames: usize) -> Result<Vec<f32>, ProviderError> {
        // The GPU decoder shares Dawn with the transformer, so it takes the same lock.
        let on_gpu = self.decoder_gpu.as_mut().map(|gpu| {
            let _gpu_guard = crate::onnx_runtime::gpu_lock();
            run_decoder(gpu, codes, frames)
        });
        match on_gpu {
            Some(Ok(audio)) => return Ok(audio),
            Some(Err(error)) => {
                tracing::warn!(%error, "reverb-tts GPU codec decoder failed; using the CPU decoder");
                self.decoder_gpu = None;
            }
            None => {}
        }
        run_decoder(&mut self.decoder, codes, frames)
    }
}

/// Run a codec decoder session: codes `[8, frames]` -> 24 kHz mono samples.
fn run_decoder(
    decoder: &mut Session,
    codes: &[i64],
    frames: usize,
) -> Result<Vec<f32>, ProviderError> {
    let codes_t = TensorRef::from_array_view(([1usize, CODEBOOKS, frames], codes))
        .map_err(|e| fail("decoder input", e))?;
    let inputs: Inputs = vec![("codes", codes_t.into())];
    let outputs = decoder.run(inputs).map_err(|e| fail("codec decoding", e))?;
    let (_, samples) = outputs["audio"]
        .try_extract_tensor::<f32>()
        .map_err(|e| fail("codec output", e))?;
    Ok(samples.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_codes_round_trip_and_reject_corruption() {
        let voice = VoiceCodes {
            codes: (0..CODEBOOKS * 5)
                .map(|i| (i * 37 % CODES) as i64)
                .collect(),
            frames: 5,
            speech_frames: 4,
        };
        let bytes = voice.to_bytes();
        assert_eq!(VoiceCodes::from_bytes(&bytes), Some(voice.clone()));
        assert_eq!(voice.pace_frames(), 4);
        assert_eq!(VoiceCodes::from_bytes(&bytes[..bytes.len() - 1]), None);
        assert_eq!(VoiceCodes::from_bytes(&[]), None);
        let mut bad = bytes.clone();
        let n = bad.len();
        bad[n - 1] = 0xFF; // code >= 1024
        bad[n - 2] = 0xFF;
        assert_eq!(VoiceCodes::from_bytes(&bad), None);
        // Speech frames never exceed the recording.
        let long = VoiceCodes {
            speech_frames: 99,
            ..voice
        };
        assert_eq!(long.pace_frames(), 5);
    }
}
