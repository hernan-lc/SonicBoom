# Native inference ownership and cache policy

SonicBoom remains Rust-native: Supertonic, Pocket TTS and Reverb-TTS use ONNX
Runtime. No Python/PyTorch runtime or model subprocess is introduced. (The
former Qwen3-TTS backends, Candle and GGUF/llama.cpp, were removed together with
their vendored dependencies.)

## Ownership and lock ordering

HTTP handlers reserve admission before cloning/decoding references. Pending
requests wait asynchronously in the shared semaphore queue; its acquire future
is retained to preserve FIFO ordering. Running permits are moved into blocking
native work. Admission/reference preparation permits are shared leases retained
by actual blocking preparation and encoding, so dropping an HTTP future cannot
release capacity while that work continues.

Pocket TTS owns its model and prompts on one serialized model transaction, and
Reverb-TTS on its own ONNX sessions. Worker reply waits are off Tokio executors
and retain permits until acknowledged computation ends. Cancellation/deadlines
reach the generation loops. Panics in synthesis retire the damaged model. Release
builds unwind panics so the job owner can report terminal failure. Normal
inference errors and cancellation preserve healthy shared weights.

Provider lock order is lifecycle -> heavyweight construction resource ->
model/worker -> short state/config locks. Conditioning and synthesis are one
serialized transaction. Job-store locks never cover awaits. Voice-store locks
are acquired only on blocking threads; a bounded two-second advisory file lock
serializes manifest mutations across server instances.

Large optional model construction is serialized across providers. Each provider
loads once. A failed load of one provider does not poison construction for
another. Keep sufficient free RAM
for every enabled resident model; this is not an automatic offloading policy.

## Configuration versions and reloads

The server snapshots configuration and revision together. ProviderPool rejects
older versions, preventing an old request from reverting a newer model choice.
Busy reconfiguration is recorded and retried by one coordinator; active model
work finishes before retirement. Background loaders compare model identity
(model directory, model ID/revision, enablement and device), reject obsolete
candidates, and do not discard a compatible load for a cache-only
change. Cache changes preserve loaded weights.

Providers are serialized: `providers.pocket_tts.max_concurrent` must be **1**. Older values that implied
parallel model execution were never effective; set them to 1 before upgrading.
Global admission can still allow distinct providers to run concurrently.

## Disk verification

SHA-256 is computed with a 256 KiB buffered reader and 64 KiB scratch buffer,
on blocking threads, rather than allocating an entire multi-GB file. Verification
progress reports actual processed bytes. Downloaded bytes must match both the
pinned digest and exact size before atomic installation; staging is cleaned on
errors/dropped futures. Linux process-tagged staging files left by dead processes
are reaped without deleting another live server's downloads/reference staging.
Legacy staging without a process tag is not blindly deleted.

Private verification records contain format version, pinned source/revision
URL, expected digest/size and verified file identity. On Unix, an unchanged
regular file's device/inode/size/mtime/ctime (including nanoseconds), together
with owner-private metadata, permits skipping repeat hashing. Changed, corrupt,
untrusted or missing records trigger pinned hashing. Platforms without that
identity policy conservatively stream-hash. Metadata is an optimization of a
prior pinned verification, not trust on first use. No tokens or voice audio are
written into these records. Native offline loads use the same integrity pins.

## Voice caches

Each native provider promotes conditioning on preparation/synthesis access and
evicts least recently used entries. Entries are bounded by configured count (1..=1024)
and 64 MiB. Identity includes provider, reviewed implementation/model revision,
model ID, decoded reference hash, sample-rate handling, transcript and cloning
mode. Profile-scoped keys isolate voices; ownership is checked before cache
retrieval. An eagerly prepared identical prompt can be rebound without encoding
again. Deletion logically invalidates the profile and retires artifacts at a
safe worker boundary.

The server caches one manifest plus a decoded-reference LRU (16 entries/64 MiB).
Manifest and WAV identities detect changes, and create/delete operations keep
invalidation coherent. Reference lookup holds the voices lock only for metadata
(probe, identity re-check); disk reads and WAV decoding run without the lock,
then re-verify profile identity and file stamp before publishing, so parallel
reads no longer serialize on the slow path and a mid-decode deletion,
replacement or reconfiguration can never publish stale audio. Profile
directories are owner-private; manifests and
reference writes remain atomic. Pocket TTS retains only its last conditioned
profile/reference under its serialized model transaction and invalidates that
reuse on changes/deletion. Its native data is replaced at the next safe reference
preparation rather than unloading healthy weights. Prefilled Pocket flow states
stay per-request (rebuilt from cached embeddings): sharing mutable
request-local KV across voices would be unsound, and per-request cloning
(which isolation would still require) has no measured win over re-prefill —
warm-request benchmarking against pinned weights is outstanding (see
benchmarks), so the embeddings-only cache stands.

Server-generated playback files transfer ownership atomically on admission:
the audio thread registers the temp file before it can complete, stop, or be
replaced, so no interleaving orphans it and no separate registration round-trip
exists. Failed admissions are never registered (the caller deletes its file),
caller-owned library files are never registered (cleanup never deletes them),
and a deletion that fails because the backend still holds the file keeps its
registration for a later pass instead of orphaning the path. The healthy idle
audio thread blocks indefinitely on the command channel (zero wakeups);
it polls only while a sink is active (completion/controls) or while a throttled
device-recovery retry is pending.

Supertonic duration-predictor outputs are validated before any tensor
allocation (finite, positive, per-chunk 60 s / latent-byte budgets, plus a
per-request sample budget with checked arithmetic); valid inputs derive
bit-identical lengths to the previous formula. The reusable engine releases
its inference lock after native synthesis and before WAV encoding, so encoding
never blocks the next inference.

## Instrumentation and profiling

Authenticated `/api/providers` includes actual selected device, model residency,
load count and conditioning counters. Jobs expose measured stage durations (`generating`/`decoding`/`encoding` plus
`normalization` at enqueue and `queue_wait` until the executor starts),
fragment progress (`fragments_total`/`fragments_done` in original order),
frames, elapsed time, timestamps and actual provider/model/voice/device context.
Logs identify request paths and job IDs, not request bodies, query transcripts,
credentials or recordings. No provider logs raw reference audio or transcripts
independently of configured verbosity.

Providers log generated frames, EOS/max-frame reason, audio duration and RTF.
Audio encoding executes off the async executor. Reproducible Rust benchmark
tools and measurements are in `benchmarks/providers.md`; ordinary CI never needs
multi-GB model downloads. The generation loops have EOS and frame limits;
slowness is not an infinite loop. A max-frame result is explicitly withheld,
with an actionable error, instead of silently truncating speech.


### Reproducible profiling commands

```sh
# Offline per-provider audit (see benchmarks/tts-quality/README.md):
cargo run --release --bin tts-quality-audit -- --help
# Linux perf needs host permission and symbols; profile the built binary:
perf record -g target/release/SonicBoom
perf report
```

Use the exact pinned model, build features, owned reference, text, seed and
thread settings for comparisons. Run without concurrent compilation or other
inference, repeat runs, and capture `/usr/bin/time -v` plus JSON results.
Do not compare the historical two-CPU audit numbers with this machine as if
only software changed.


## Quality detection (offline tooling)

Deterministic quality detection (`src/tts/quality/`) is two O(n) passes over
already-resident PCM (health measurements plus a first-difference scan for
impulses and chunk seams). It no longer runs in the request path: the server
uses only the audio-health analysis for the non-destructive reference-health
report returned when a voice profile is created, and `tts-quality-audit` runs
the full pass (including reference-echo detection when an offline ASR transcript
is supplied) over directly synthesized audio. ASR is a benchmark/offline
dependency and is never run during generation.
