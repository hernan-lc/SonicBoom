# Supertonic 3: ONNX vs GGUF latency evaluation

Opt-in A/B harness comparing the shipped ONNX Runtime (ORT) path with a GGUF
model through audio.cpp's ggml runtime. It is **not** wired into the server or
default features: this is measurement scaffolding, not a migration.

## Why this file exists

`pykeio/ort#675` closed as *not planned*: ORT's Windows WebGPU binaries are
built against VS 2026, so Windows CI uses `windows-2025`. That is unrelated to
this benchmark, but it shows the inference backend is an externally constrained
dependency. GGUF would give the project a second backend and a path to models
that have no ONNX distribution (Kokoro, Qwen3-TTS, Pocket-TTS, …).

## Model

Pinned in `benchmarks/gguf/pin.json` (HF repo, immutable revision, SHA-256,
size). We use the **F32 `orig`** package because audio.cpp's `f16` package
crashes in `ggml_concat` and its `q8_0` is an unsupported weight dtype. The
weights are OpenRAIL-M (commercial use allowed with restrictions); review the
upstream license before redistributing.

The harness downloads and verifies the file on first run into
`models/gguf/supertonic-3-orig.gguf` (git-ignored). `GGUF_MODEL` overrides the
path; the pin is still enforced.

## Run

```sh
AUDIOCPP_MODELS=supertonic cargo run --release --features supertonic-gguf \
  --example supertonic_gguf_bench -- \
  "A gentle breeze moved through the open window." 5 5
```

`AUDIOCPP_MODELS=supertonic` is required at build time: the default `core-models`
set of audio.cpp does not compile the Supertonic loader. Requires `cmake`, a C++
toolchain and network access on first build (the `audio-cpp-sys` build script
clones and builds the audio.cpp runtime).

GPU/Vulkan comparison (needs `glslc`, Vulkan headers and an ICD):

```sh
AUDIOCPP_MODELS=supertonic BENCH_DEVICE=vulkan GGUF_THREADS=4 \
  cargo run --release --features supertonic-gguf-vulkan \
  --example supertonic_gguf_bench -- \
  "A gentle breeze moved through the open window." 5 5
```

Knobs:

- `BENCH_DEVICE` — `cpu` (default, ORT CPU + ggml CPU) or `vulkan` (ORT WebGPU
  EP + ggml Vulkan). Both sides use the same device.
- `GGUF_THREADS` — ggml threads (defaults to available parallelism). This
  matters a lot on CPU: on the dev box, 4 threads was ~6x faster than the
  default for the GGUF side. Pin it when comparing.
- `GGUF_MODEL` — alternate GGUF path.

Both backends use seed 42, voice F1, the same text and flow-step count, 2
warm-ups and a configurable number of timed trials. Reported metrics: load
time, warm-up time, median synthesis time, `RTF = median / audio_seconds`,
peak RSS (`VmHWM`) and PCM statistics. WAVs land in `/tmp/opencode/gguf-bench/`.

## Measured result (2026-11-22, devbox)

See `benchmarks/results/2026-11-22-devbox/`. ORT was faster everywhere:
CPU 1.34 s vs 1.74 s (RTF 0.293 vs 0.363), Vulkan 0.180 s vs 0.365 s
(RTF 0.039 vs 0.076), with lower peak RSS. **Do not migrate for speed**; GGUF's
value is model variety. GGUF CPU timings are extremely thread-sensitive, so pin
`GGUF_THREADS` when repeating.

## Decision criteria

Do **not** migrate on speed alone. Compare on the same machine and require all
of:

1. GGUF warm RTF at least **1.5x** better than ORT on CPU (the portable case),
   or a clear memory reduction with no meaningful RTF regression.
2. Quality parity via the `tts-quality-audit` protocol (WER + listening check).
   Do not compare waveform hashes: ggml and ORT differ numerically even when
   the speech is equivalent (the retired Qwen GGUF work saw near-zero waveform
   correlation with identical content).
3. Maintenance cost accepted: a second native runtime (cmake + C++ toolchain,
   per-model family support) in the release and cross-compile matrix.

Reverb-TTS is deliberately out of scope: there is no ggml backend for its
architecture (297M params + Higgs codec), so a "conversion" would mean writing
a new backend, and its weights are CC BY-NC 4.0 (non-commercial).
