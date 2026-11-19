# Future optimization backlog

Current state (validated 2026-11-18, AMD BC-250 / RADV, Supertonic 3, voice F1):

- ONNX inference uses ORT's WebGPU EP by default — Vulkan on Linux — with automatic CPU fallback (`SONICBOOM_EP=cpu` forces CPU).
- Supertonic, Pocket TTS and the Qwen ONNX codecs share this policy; Qwen GGUF uses llama.cpp Vulkan.
- Measured Supertonic latency, 5 denoising steps: CPU ~1.6 s vs Vulkan ~0.36–0.44 s (~3.7–4.4×). CPU/Vulkan waveform difference: `max 7.4e-5`, `mean 1e-6` with a fixed seed.
- Manual end-to-end RTF across all models is acceptable, so the items below are
  **improvements, not blockers**. Benchmark each one before enabling it.

## Candidates

| Idea | Why | How to validate | Status |
|---|---|---|---|
| Use OnnxSlim / int8 Supertonic assets | Upstream slim assets exist; community int8 drops weights 398→102 MB and peak RSS ~1026→327 MB | Compare RTF and waveform similarity vs the current fp32 baseline; check language coverage | Not tried |
| GPU graph capture | Removes per-run CPU submission overhead | Already tested via `SONICBOOM_WEBGPU_GRAPH_CAPTURE=1`: no gain (0.383→0.380 s) — CPU-fallback nodes and dynamic shapes prevent capture | Measured, no gain |
| I/O binding (GPU-resident tensors) | Avoids host↔device copies of latents/token ids; `ort` exposes `IoBinding` | Profile copy time, then A/B end-to-end RTF | Not tried |
| Move remaining CPU nodes to GPU | Profile shows ~42% of `duration_predictor` and ~7% of `vector_estimator` nodes still on CPU (shape/control ops) | Re-run `SONICBOOM_PROFILE` after each opset/graph rewrite; verify output equivalence | Tracked |
| fp16 / NHWC layout options | WebGPU EP supports `preferredLayout`; fp16 can cut memory bandwidth | A/B accuracy and RTF; watch for drift in flow-matching latent | Not tried |
| Honor `model.inference_steps` in native `SupertonicProvider` | It currently hard-codes 5 steps while the API/config default is 5 and allows 1–50 | Set steps 1/5/10 via config and confirm latency/quality changes | Known mismatch |
| CPU-thread tuning for fallback paths | `SONICBOOM_CPU_THREADS` is only read on Android; desktop uses ORT defaults | Sweep 1/2/4/8 threads on CPU and on the fallback nodes | Not tried |
| Parallel chunk synthesis | Long text is split at 200 chars and synthesized sequentially | Measure RTF for multi-chunk inputs; verify chunk boundary audio | Not tried |
| Vendor EPs for NVIDIA images | CUDA/TensorRT can beat WebGPU on NVIDIA; build with `--no-default-features --features cuda` | Compare RTF and output vs Vulkan on the same GPU | Build available, unbenchmarked |
| Native Vulkan EP (`onnx-vulkan-rs`) | Avoids the WebGPU/Dawn abstraction; ~9× vs CPU reported on some graphs | Check operator/opset coverage for all four Supertonic graphs; A/B RTF and quality | Experimental, not integrated |

## Reproducing the baseline

```sh
cargo build --release --no-default-features --features webgpu,server,pocket-tts \
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
