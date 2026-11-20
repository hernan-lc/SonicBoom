# Request isolation remediation

> Historical note: the Qwen3-TTS and Pocket TTS providers measured in the archived
> evidence below were removed, together with their audit runner modes, grid scripts
> and fixtures. Only the `mock` and `reverb` runner modes remain. The archived
> results are kept as-is.

Baseline: fetched `origin/main` = `35416e8` on 2026-10-16; clean worktree.

Confirmed client defect: `useSynthesis.generate` used any retained job ID without
binding it to the submitted input. A transport failure followed by changing text,
voice or model could resume the previous job and publish its audio. The previous
object URL also stayed visible throughout a new submission. This proves a client
identity failure, not acoustic concatenation or model KV leakage.

Backend inspection: jobs have independent control UUIDs, token-derived owners,
move-owned request bodies, completed-only audio downloads, cancellation checks
before publication and `private, no-store` responses. This inspection alone does
not prove HTTP integration or speech-content correctness.

Native inspection (archived, removed providers): no model-state cause was
confirmed.

Current completed runs, numerical runtime-build failures and remaining gates are
reported in [REPORT.md](REPORT.md) and [verification.json](verification.json).
Real CPU weights were available and executed; model directories alone were not
counted as successful validation. Human listening and GPU acceptance remain open.

## Rust diagnostic runner

```
cargo test --no-default-features --test tts_request_isolation
cargo run --no-default-features --bin tts-quality-audit -- --help
# Create the parent, but not NEW_OUTPUT_DIR; all destinations are create-new.
REVERB_REFERENCE_TRANSCRIPT='exact transcript' cargo run --release --no-default-features --features reverb-tts --bin tts-quality-audit -- reverb reverb-tts es models/reverb-tts NEW_OUTPUT_DIR AUTHORIZED_REFERENCE.wav
```

Runner sequence: hola, test, mundo, cold test (engine dropped/reloaded), warm
test, punctuation, short words, combined text and multiple sentences. Each call
uses a fresh UUID and WAV destination. JSONL records hashes, effective CPU,
revision, seed, timing/RTF, sample count, health statistics and exposed metrics.
`job_id=null` explicitly means this is the direct-provider boundary, not HTTP.
Unexposed provider metrics are null/unknown, not invented. `listen.html` provides
playable clips. Same WAV hashes establish determinism only, never word accuracy.
The mock test verifies independent files, identities, WAV lengths and no overwrite;
it does not exercise real HTTP jobs or infer speech correctness from mock signals.

## Offline parameter and content evaluation

`score_asr.py MANIFEST TARGETS_JSON ASR_JSONL NEW_OUTPUT` accepts explicitly
provided offline ASR results (schema in its docstring), requires an ASR model and
revision, checks exact target hashes, joins by request UUID and reports NFC,
case-folded, punctuation-separated WER/CER with insertions/deletions/substitutions.
Accents remain significant. It explicitly flags `hola test` for a `test` request
and requires manual review of suspect clips; it never turns ASR guesses into a
passing content assertion. No production Python or ASR dependency is added.

Local baseline references use the already documented eSpeak NG synthetic clip
as `fixtures/reference-espeak-en.wav`; no human recording is included.
It does not contain `hola`. Its exact transcript and provenance are recorded in
`fixtures/README.md`. The synthetic Spanish pair in `fixtures/` supplies both the containing-`hola`
and non-containing-`hola` cases without a human recording.

For alternating references, explicitly set `AUDIT_ALT_REFERENCE_WAV` and (Reverb)
`AUDIT_ALT_REFERENCE_TRANSCRIPT`. Four extra requests run A/B/A/B at fixed seed
with reference hashes and independent voice IDs. A and B never come from generated
audio. `fixtures/README.md` identifies the synthetic `hola`-containing fixture.
The manifest includes the executable SHA-256 and ORT build, so runtime git HEAD
alone is not mistaken for a precise inference-build identity.

The optional offline ASR helper is `transcribe_whisper.py LOCAL_MODEL AUDIT_ROOT
NEW_JSONL`. Development environment used: faster-whisper 1.2.1, CTranslate2 4.8.3,
PyAV **16.0.1** (PyAV 19 fails this faster-whisper decoder API), CPU INT8,
2 threads, beam 5, forced fixture language, no VAD filtering and no previous-text
conditioning. Model: `Systran/faster-whisper-tiny` revision
`d90ca5fe260221311c53c58e660288d3deb8d356`; actual binary SHA-256 is recorded in
every ASR row. Download this development-only model explicitly before running;
the helper only opens local files. No ASR or Python package enters SonicBoom's
Cargo dependency graph or default CI.

### Runner knobs

The `tts-quality-audit` binary accepts environment overrides. Each row carries a
`quality` object (audio-health measurements, plus echo analysis when ASR is
supplied):

- `AUDIT_SEED=<decimal-u64>` overrides the default 42 and is recorded in each
  row; it reproduces the exact decimal seed shown by job diagnostics. Use a new
  output directory for another seed; never overwrite an earlier run.
- `AUDIT_QUALITY=off|report` — quality pass mode (default `report`; the runner
  records evidence, it never trims or regenerates).
- `AUDIT_DEVICE=cpu|vulkan` — explicit device for Reverb-TTS. CPU/Vulkan
  comparisons require separate runs into separate fresh output directories.
- `AUDIT_ASR_JSON='["transcript 1", ...]'` — one offline ASR transcript per
  text, aligned by index, enabling reference-echo confirmation. This is
  benchmark-only; the production request path never runs ASR.
- `AUDIT_TEXTS_JSON` overrides the text matrix.

`AUDIT_ALT_REFERENCE_WAV` / `AUDIT_ALT_REFERENCE_TRANSCRIPT` drive the A/B/A/B
reference alternation for boundary experiments. The runner writes hashed inputs
only and never logs raw reference audio or transcript text.
