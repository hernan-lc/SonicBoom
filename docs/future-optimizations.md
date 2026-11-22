# Future optimization backlog

> **Retired providers.** Qwen3-TTS (GGUF/ONNX and Candle) and Pocket TTS were
> removed; only Supertonic and Reverb-TTS remain. The Qwen and Pocket sections
> below are archived measurements and tuning notes for code that no longer
> exists (their environment variables, examples and flags are gone). They are
> kept for historical reference only.

Current state (validated 2026-11-18, AMD BC-250 / RADV, Supertonic 3, voice F1):

- ONNX inference uses ORT's WebGPU EP by default — Vulkan on Linux — with automatic CPU fallback (`SONICBOOM_EP=cpu` forces CPU).
- Supertonic and Reverb-TTS share this policy (archived: Pocket TTS and Qwen, see the note above).
- Measured Supertonic latency, 5 denoising steps: CPU ~1.6 s vs Vulkan ~0.36–0.44 s (~3.7–4.4×). CPU/Vulkan waveform difference: `max 7.4e-5`, `mean 1e-6` with a fixed seed.
- Manual end-to-end RTF across all models is acceptable, so the items below are
  **improvements, not blockers**. Benchmark each one before enabling it.

## Startup optimizations (2026-11-20, AMD BC-250 / RADV)

Measured with `examples/startup_probe.rs` (fresh process per run) and
`examples/qwen_codec_probe.rs`, plus the full server with Qwen3 GGUF enabled.

| Change | Before | After |
|---|---|---|
| GPU warm-up after load (`ModelHandle::warm_up`, `SONICBOOM_WARMUP=0` disables) | first synthesis 0.84–1.03 s (up to 2.7 s in the server) vs 0.15–0.18 s steady | first synthesis 0.16–0.18 s; warm-up costs ~0.8 s before `Ready` |
| Supertonic's 4 ORT sessions created in parallel | 0.97 s | 0.70 s |
| Qwen: codec sessions, embeddings and tokenizer loaded while llama.cpp loads the GGUFs | ~7.5 s until Qwen ready | ~3.0–3.8 s |
| Qwen int8 codec on CPU instead of WebGPU (`SONICBOOM_QWEN_CODEC_EP=gpu` restores it) | speaker 65 ms, encode ~500 ms, decode 6.1–6.6 s | speaker 42 ms, encode 421 ms, decode 6.2 s; no Dawn registration |
| Supertonic cached-file verification reuses the identity-bound `*.verified.json` record (files >= 1 MiB) | re-hashes ~400 MB every start (0.65 s warm, more with a cold page cache) | one full hash, then skipped while inode/size/mtime/ctime are unchanged |

Server start to everything ready: ~7.5 s -> ~4 s, and the first request no
longer pays the shader/pipeline compilation. Per-session timings are logged
(`ONNX session ready`, `Qwen GGUF runtime ready`, ...).

Tried and rejected (no measurable change, within +/-30 ms noise): ORT graph
optimization level 1/2/3, WebGPU `preferredLayout` NCHW vs NHWC, Dawn
validation mode disabled/wgpuOnly/basic. The Qwen decoder (~6 s for 60 frames)
is CPU-bound and is the next thing to look at for Qwen latency.

fp16 Supertonic graphs (2026-11-20): whole-graph `convert_float_to_float16`
fails to load (Div/Cast type mismatches). Casting only around Conv/MatMul/Gemm
works. With fp32 `duration_predictor`/`text_encoder` and fp16 math in
`vector_estimator` + `vocoder`, steady-state synthesis drops ~197 -> ~165 ms
(-16%, re-measured), cold start is not faster (extra Cast nodes), and the
waveform differs from fp32 by max 0.12 / mean 4.4e-3 versus the project bar of
~1e-4 / ~1e-6 (peak/RMS stay within ~2%). Weight-only fp16 halves the files but
is not faster and shifts durations. Not adopted: it misses the quality bar by
orders of magnitude for a modest latency gain.

## Flow steps, community fp16/int8 and MNN (2026-11-22, AMD BC-250 / RADV, WebGPU/Vulkan)

Flow steps (`examples/supertonic_steps_ab.rs`, seed 42, median of 5, 5 sentences
en/es, round-trip WER with the local `fluid-speech-pro` ASR):

| Steps | Median latency | RTF | Mean WER |
|---|---|---|---|
| 5 (default) | 195 ms | 0.037 | 0.000 |
| 3 | 134 ms (-31%) | 0.025 | 0.000 |
| 2 | 101 ms (-48%) | 0.019 | 0.045 (dropped/swapped words) |
| 1 | 70 ms (-64%) | 0.013 | 0.985 (unintelligible) |

Latency is ~31 ms per step plus ~40 ms fixed. Waveform drift versus 5 steps is
large (max ~0.5-2) for every other count: different step counts are different
audio, so the ~1e-4 bar only applies to graph conversions, not to this knob.
WER over 5 sentences is a weak gate: 3 steps looks intelligible, but naturalness
was not listened to and the sample is small. 5 stays the default; use 3 only
after a listening check on the target voices/languages.

Community conversions (Kyumdroid fp16, ahk-d fp16 and int8; fp32 I/O, all load
in ORT CPU), 5-step flow loop through the fp32 vocoder, versus fp32:

| Candidate | Waveform max / mean drift | Verdict |
|---|---|---|
| Kyumdroid fp16 | 1.5e-2 / 6.6e-4 | Fail (~2-3 orders above the bar) |
| ahk-d fp16 | 9.5e-3 / 6.3e-4 | Fail; numerically the same recipe as Kyumdroid |
| ahk-d int8 | 4.3e-1 / 3.5e-2 | Fail; only `vector_estimator` is int8 (MatMulInteger) |

Cross-checked independently on a single estimator step (different seed):
Kyumdroid max 4-6e-3 / mean 7e-4-1e-3. Better than the in-house partial fp16
(max 0.12) but still not within the bar. Not adopted.

MNN 3.6.2 (converter + Vulkan backend): all four graphs convert and dynamic
lengths work, but `vector_estimator` differs from ORT by max 2.5e-3-1.5e-2 /
mean 1e-4-6e-4 on the latent (CPU and Vulkan; `Precision_High` does not help).
Vulkan only runs with `-DMNN_VULKAN_IMAGE=OFF` (image mode segfaults on this GPU)
and needs the separate `libMNN_Vulkan.so` loaded. The Rust bindings are young
(hundreds of downloads). Not adopted; no latency was measured because the numerics
already disqualify it.

## Qwen3 request latency (2026-11-20, AMD BC-250 / RADV, 12 logical CPUs)

Measured with `examples/qwen_gguf_smoke.rs` (`QWEN_GGUF_REPEAT`, `QWEN_GGUF_THREADS`,
`QWEN_GGUF_GPU_LAYERS`), 31 frames / 2.48 s of audio, fixed seed (output is
deterministic run to run). Baseline split: conditioning 0.7 s, talker+predictor
generation 1.3 s (Vulkan), **int8 codec decode 5.2 s (~72 % of the request)**.

| Test | Result |
|---|---|
| Talker/predictor on CPU instead of Vulkan | generation 50.9 s vs 1.3 s: keep Vulkan |
| llama.cpp threads 2 / 4 / 8 | generation 1.3 s in all cases: no effect |
| Codec on WebGPU instead of CPU | decode 3.1 s vs 2.6 s, conditioning 1.1 s vs 0.55 s, different codes: keep CPU |
| Codec threads 2 / 4 / 6 / 8 / 12 (chunk 48) | decode 3.3 / 2.8 / 2.6 / 2.5 / 2.35 s, audio bit-identical |
| Decoder chunk 6 / 12 / 24 / 48 / 200 (2 threads) | decode 7.3 / 5.2 / 3.9 / 3.3 / 3.5 s |

Implemented: `SONICBOOM_QWEN_CODEC_THREADS` (default now up to 6 cores, was 2;
audio unchanged; decode 5.2 -> 3.8 s) and `SONICBOOM_QWEN_DECODER_CHUNK`
(default 12, unchanged). Chunk 48 plus 6 threads decodes in 2.5 s (request
7.1 s -> ~4.3 s) but changes the audio: 22.6 dB SNR, max 0.026 against a 0.23
peak, because the int8 decoder quantizes dynamically per call and carries state
per chunk. Listen before making it the default.

Next, in order of expected gain:
1. Listen A/B (chunk 12 vs 48) and, if acceptable, raise the default chunk.
2. Find why each decoder call costs ~1 s regardless of frame count (profile the
   ONNX graph with `with_profiling`; look at the dynamic-quantize/attention nodes
   and the zero-length history inputs). Fixing that beats any chunk tuning.
3. ~~Adopt the published fp16 decoder~~ done (2026-11-20): `qwen3_tts_decoder.fp16.onnx`
   replaces the int8 decoder in the manifest (230 MB vs 222 MB), with fp16 tensor I/O
   through the `ort` `half` feature. Same codes and seeds: decode 1.7-1.9 s for
   2.7 s of audio (was ~4 s), noise floor -85.6 vs -60.1 dBFS in the production
   pipeline, ASR WER unchanged at 0.026. The encoders stay int8 (the fp32 exports
   have no external weight files, and the fp16 speaker export is invalid).
   Untested follow-ups: fp32 weights rebuilt from the fp16 ones (~460 MB, may avoid
   fp16 casts on CPU), mixed int8 transformer + fp16 convolutions.
4. Stream decoded chunks to the client as they are produced instead of waiting
   for the whole decode (time to first audio, not total time).
5. Talker q5_k -> q4_k only if generation (1.3 s) becomes the bottleneck.

## Qwen audio quality (2026-11-20)

Scored with the Fluid Speech Pro ONNX ASR (`FluidVoice-Labs/fluid-speech-pro-onnx`, int8)
on 6 English sentences cloned from a clean reference, plus Qwen speaker-embedding
cosine similarity to the reference and noise/spectrum metrics. Every variant tied:
WER 0.026 (the only error is "river bank" vs "riverbank"), speaker similarity
0.963-0.965, noise floor -62 to -67 dBFS.

| Variant | Result |
|---|---|
| Talker on Vulkan vs CPU (llama.cpp, q5_k) | identical WER and similarity: Vulkan does not degrade the talker (CPU takes ~50 s per clip) |
| Presets balanced / stable / expressive | no measurable difference on this set |
| Decoder chunk 12 vs 48 | WER equal, similarity 0.9646 vs 0.9632, no new clicks |
| Int8 codec on WebGPU | broken audio (see above) |

Found and fixed: the Opus delivery stage upsampled 24 kHz audio (Qwen, Pocket TTS)
to 48 kHz by linear interpolation, which leaves spectral images at about -33 dB in
12-20 kHz (hiss/harshness). `audio::resample_for_output` now uses the band-limited
resampler for sources up to 36 kHz (decoded Opus: 12-16 kHz energy -32.7 dB ->
-78.5 dB). Supertonic (44.1 kHz) keeps the previous path bit for bit.

Not measurable here: timbre and naturalness for real, noisy or non-English
references (ICL clones the reference's noise and room), which ASR cannot score.

## Where the Qwen hiss comes from (2026-11-20)

A faint broadband floor remains in Qwen output (about -66 dBFS in the quietest
frames; Supertonic writes exact zeros in pauses). Isolating it:

| Stage tested | Noise floor | Verdict |
|---|---|---|
| Reference: original / trimmed / 75 dB dither / 12 s long | -66.5 / -63.9 / -64.2 / -63.0 dBFS | not the reference |
| Talker on Vulkan vs CPU | -66.5 vs -62.9 dBFS | not Vulkan |
| Decoder chunk 12 vs 48 | -66.5 vs -65.4 dBFS | not our chunking |
| 24 kHz -> 48 kHz Opus resampling | fixed separately (images -33 dB -> -78 dB) | was ours, fixed |
| **Same codes, int8 vs fp16 decoder** | **-66.5 vs -74.4 dBFS** | int8 quantization adds ~8 dB |
| Exact silence through encoder+decoder (int8) | decodes to -26 dBFS noise | digital silence is out of distribution for the codec |

So the remaining floor is the codec model itself (about -74 dBFS), plus ~8 dB that
comes from the int8 decoder asset we pinned. The fp16 decoder is also 2x faster on
CPU (1.97 s vs 4.12 s for 2.7 s of audio) and closer to the reference voice
(speaker similarity 0.981 vs 0.965), with the same ASR WER. The experiment wrapped
the fp16 graph with Cast nodes at its inputs/outputs; production now feeds fp16
tensors directly and ships the fp16 decoder (see next steps, item 3).

Reference tips: avoid references with exact digital silence (e.g. raw TTS output);
the codec turns zeros into noise. Real recordings with a little room tone are fine.

## Candidates

| Idea | Why | How to validate | Status |
|---|---|---|---|
| Use OnnxSlim / int8 Supertonic assets | Upstream slim assets exist; community int8 drops weights 398→102 MB and peak RSS ~1026→327 MB | Compare RTF and waveform similarity vs the current fp32 baseline; check language coverage | Not tried |
| GPU graph capture | Removes per-run CPU submission overhead | Already tested via `SONICBOOM_WEBGPU_GRAPH_CAPTURE=1`: no gain (0.383→0.380 s) — CPU-fallback nodes and dynamic shapes prevent capture | Measured, no gain |
| I/O binding (GPU-resident tensors) | Avoids host↔device copies of latents/token ids; `ort` exposes `IoBinding` | Profile copy time, then A/B end-to-end RTF | Not tried |
| Move remaining CPU nodes to GPU | Profile shows ~42% of `duration_predictor` and ~7% of `vector_estimator` nodes still on CPU (shape/control ops) | Re-run `SONICBOOM_PROFILE` after each opset/graph rewrite; verify output equivalence | Tracked |
| fp16 / NHWC layout options | WebGPU EP supports `preferredLayout`; fp16 can cut memory bandwidth | A/B accuracy and RTF; watch for drift in flow-matching latent | `preferredLayout` measured: no change; fp16 measured 2026-11-20: rejected (see below) |
| Honor `model.inference_steps` in native `SupertonicProvider` | It hard-coded 5 steps while the config allows 1–50 | Set steps via config and confirm latency/quality changes | Fixed; A/B measured 2026-11-22 (see below) |
| CPU-thread tuning for fallback paths | `SONICBOOM_CPU_THREADS` is only read on Android; desktop uses ORT defaults | Sweep 1/2/4/8 threads on CPU and on the fallback nodes | Not tried |
| Parallel chunk synthesis | Long text is split at 200 chars and synthesized sequentially | Measure RTF for multi-chunk inputs; verify chunk boundary audio | Not tried |
| Vendor EPs for NVIDIA images | CUDA/TensorRT can beat WebGPU on NVIDIA; build with `--no-default-features --features cuda` | Compare RTF and output vs Vulkan on the same GPU | Build available, unbenchmarked |
| Native Vulkan EP (`onnx-vulkan-rs`) | Avoids the WebGPU/Dawn abstraction; ~9× vs CPU reported on some graphs | Check operator/opset coverage for all four Supertonic graphs; A/B RTF and quality | Experimental, not integrated |

## Reproducing the baseline

```sh
cargo build --release --no-default-features --features webgpu,server \
  --example supertonic_vulkan_bench
LD_LIBRARY_PATH=$PWD/target/release/examples \
  ./target/release/examples/supertonic_vulkan_bench "Your sentence" 5 3
```

The benchmark forces CPU with `SONICBOOM_EP=cpu` and runs the default
Vulkan-first path side by side, with a fixed `SONICBOOM_SEED`. ORT profiles
are written to `/tmp/opencode/vulkan-probe/profiles` and report node counts
per execution provider.

Any optimization that changes numerical output must keep CPU/Vulkan waveform
difference in the same range as today (`~1e-4` max, `~1e-6` mean) or document
an explicit, audible quality trade-off.

### Vulkan-first with CPU fallback (2026-11-20)

Per-graph EP selection was added (`graph_ep`): puts the four conditioning/flow graphs on Vulkan and keeps the
decoder on CPU; `gpu` forces everything on Vulkan; Bisect with the
same text/reference (output RMS):

| Graph on GPU | RMS | Verdict |
|---|---:|---|
| CPU (control) | 0.0619 | reference |
| `text_conditioner` | 0.0619 | correct |
| `mimi_encoder` | 0.0649 | correct |
| `flow_lm_main_int8` | 0.0690 | correct but 2.1x slower |
| `flow_lm_flow_int8` | 0.0629 | correct |
| `mimi_decoder_int8` | **0.0036** | **near-silent (sole culprit)** |
| all GPU (int8 or FP32) | 0.0035-0.0065 | broken; FP32 does not fix it |

Two automatic fallbacks when `auto`/`gpu` is used:

1. **At load:** `pipeline_probe` synthesizes with a deterministic sine
   reference; if RMS < 0.015 or EOS is never reached, every graph is rebuilt
   on CPU.
2. **At request time:** if the GPU flow LM hits the frame limit without EOS
   (upstream treats this as an error), all graphs are rebuilt on CPU and the
   request is regenerated once. Measured: 10.00 s / RMS 0.058 became 7.68 s /
   RMS 0.034.

**Making Vulkan-first the default required three more fixes:**

1. **Dawn is not thread-safe.** Two concurrent Pocket loads segfaulted inside
   Dawn on AMD RADV. A session-creation mutex was not enough; the whole bundle
   open (all five sessions, both probes and any rebuild) now runs under one
   global `gpu_session_lock()`. With that, two concurrent real-bundle tests
   pass 3/3 runs (previously 0/3).
2. **GPU can be slower than CPU.** The load-time probe now measures the real
   synthesis time and downgrades to CPU when RTF > 1.0. On this machine the
   probe reports RTF 2.88-4.26, so the load finishes on CPU and real requests
   stay fast (1.31 s / 2.71 s for the two reference texts, RMS 0.062 / 0.032).
3. **EOS drift is reference-dependent.** The synthetic sine probe cannot
   reproduce every voice's failure, so the request-time retry remains as the
   final safety net.

## Supertonic upstream parity (2026-11-20)

`preprocess_chunk` was a simplified copy of upstream `_preprocess_text`. It now
ports the full cleanup (emoji/pictograph removal, dash/curly-quote/separator
mapping, removal of `♥☆♡©\`, spacing before punctuation, duplicate quotes,
CJK/guillemet closers as terminators; `@`/`e.g.,`/`i.e.,` for English only).
Upstream defaults are also applied: speaking rate `1.05` (`duration / speed`)
and `0.3 s` between chunks (was `1.0` and `0.2 s`). Spot check with Whisper
small: Spanish and English sentences (with an emoji and an em dash)
transcribed word-for-word.

Qwen3-TTS GGUF: the `balanced` sampling preset already equals the pinned
upstream defaults (talker 0.9/50/1.0/1.05, predictor 0.9/50/1.0/1.0); no
change.

## Reverb-TTS provider (2026-11-20)

`reverb-tts` runs Software Mansion's masked-diffusion TTS from ONNX exports
(`melser/Reverb-TTS-onnx` + `melser/Higgs-Audio-2-Tokenizer-onnx`, conversion repo
<https://github.com/nglmercer/Reverb-TTS-onnx>). Measured on the AMD BC-250 (Whisper small
word error rate, 4 Spanish texts x 6 seeds, clean 7 s reference): Reverb 0.031, Pocket 0.173;
preset voices 0.000. The Rust sampler/tokenizer/text code is validated against the official
Python package (tokenizer ids identical in es/en/de/pl; the ONNX graphs reproduce the official
codes 100 %).

| Path | 4.8 s of audio |
|---|---|
| CPU (6 threads) | 7.8 s |
| Vulkan, transformer only (codec on CPU) | 2.8 s |

Done since: Supertonic's loads and runs take the same GPU lock; the codec graphs use all cores when
the transformer is on the GPU; sessions load concurrently; the reference/output resampler is
tabulated per phase (about 55x faster, same samples); `/api/providers` no longer waits for a
running take. Vulkan now measures 4.5 s of audio in about 2.1 s warm. Profiling the CPU path
(ORT profile, 22 runs) put 81 % of the core graph in fp32 `MatMul` and showed no effect from
thread counts above 4, memory-pattern, flush-to-zero or spinning options (measured on a loaded
machine, load average 6-8): it is compute-bound.

Open items, not done: IO-binding
to keep the KV cache on the device between passes; fp16 weights (halves the 1.9 GB download,
untested on RADV); the 16 upstream preset voices (codes, not audio, so they need a profile
path that accepts codes); Verbalis-grade text verbalization for the languages SonicBoom's
normalizer does not cover (`pl`).
