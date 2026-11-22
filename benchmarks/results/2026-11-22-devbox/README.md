# 2026-11-22 devbox — Supertonic 3 ONNX vs GGUF

x86_64 Linux, 2 logical CPUs, Radeon (Vulkan ICD), ORT 2.0.0-rc.13 vs
audio.cpp 0.10.0 (ggml) with the pinned F32 `orig` GGUF. Text: "A gentle breeze
moved through the open window while everyone listened." Voice F1, seed 42,
5 flow steps, 2 warm-ups. Raw rows in `supertonic-gguf.jsonl`.

| Backend | Device | Threads | Median | RTF | Peak RSS |
|---|---|---:|---:|---:|---:|
| ONNX (ORT) | CPU | — | 1.34 s | 0.293 | ~766 MB |
| GGUF (audio.cpp) | CPU | 4 | 1.74 s | 0.363 | ~1.23 GB (2-thread run) |
| ONNX (ORT WebGPU) | Vulkan | — | 0.180 s | 0.039 | 767 MB |
| GGUF (audio.cpp ggml) | Vulkan | 4 | 0.365 s | 0.076 | 909 MB |

GGUF thread count matters a lot on CPU: with the default (`available_parallelism`)
the same run took 10.8 s; 2 threads took 2.5 s and 4 threads 1.7 s. Pin
`GGUF_THREADS` for any further measurement.

## Conclusion

**Do not migrate to GGUF for performance.** ORT is ~1.2–1.8x faster on CPU and
~2x faster on Vulkan, with lower peak RSS. GGUF's value is access to models
with no ONNX distribution, not speed. Keep the harness as an opt-in tool; do
not add a native runtime to the default build.

Quality was not measured here. GGUF output was ~5% longer (211k vs 201k
samples), so before any future decision run the `tts-quality-audit` WER gate.
Do not compare waveform hashes (ggml/ORT numerics differ).
