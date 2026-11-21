# SonicBoom Native Voice-Cloning Providers

Local, in-process voice cloning with no Python, PyTorch, `tch`, upstream
CLI, network TTS service, or sidecar process at runtime. Supertonic 3
(`ort`-based ONNX) stays the default provider; Reverb-TTS (ONNX Runtime) is the
optional cloning provider and is disabled by default. The Qwen3-TTS backends
(Candle and GGUF/ONNX), Pocket TTS and GPT-SoVITS were removed; see
[Retired providers](#retired-providers).

Upstream implementations under review (revalidate before pinning a
dependency revision):

| Provider | Source | Reviewed revision | Reality |
|---|---|---|---|
| Reverb-TTS | https://huggingface.co/melser/Reverb-TTS-onnx (converted from https://huggingface.co/software-mansion/Reverb-TTS) | pinned in `src/tts/providers/reverb_tts.rs` | Pure-Rust driver over the transformer ONNX graphs (core, embeddings, heads) plus the Higgs Audio v2 codec encoder/decoder; masked-diffusion ICL cloning (reference WAV plus its exact transcript); CPU, optional Vulkan for the transformer |

ONNX Runtime's packaged native library is the only native-code exception
and is shared by all providers (initialized once). Python is used **offline
only** for model conversion/validation, never by SonicBoom.

## What is implemented in this tree

- `src/tts/providers/`: object-safe `TtsProvider` contract (`id`,
  capabilities, `health/status`, `load/unload`, `prepare_reference`,
  `synthesize` → normalized `PcmAudio`), `TtsProviderRegistry` keyed by
  provider ID + model ID, typed errors (`unsupported_model`,
  `unsupported_language`, `invalid_voice`, `model_missing`, `loading`,
  `busy`, `device_unavailable`, `inference_failed`) mapped to the existing
  HTTP policy (`400`/`413`/`429`/`503`/sanitized `500`).
- `SupertonicProvider`: adapter preserving all ten style voices, `alloy`
  aliases, formats, auth, rate limits, and the engine API. No second
  synthesis path: the registry routes to the same inference code.
- `ReverbTtsProvider`: asset validation, language/mode
  validation, device resolution with effective-device reporting,
  per-model serialization so concurrent voices never swap reference data,
  and a bounded conditioning cache keyed by
  `(provider, model revision, voice identity, reference hash, transcript, mode)`.
- `src/tts/voices.rs`: persistent `VoiceProfileStore` (opaque UUID, consent
  metadata, `<uuid>.wav` references, no arbitrary client filenames, no
  transcript/audio logging).
- Config `[providers.reverb_tts]` (`enabled=false`
  by default) and `[voices]`; schema, validation, hot reload (per-request
  reads, last-healthy state retained on failure). Leftover
  `[providers.qwen3]`, `[providers.qwen3_gguf]`, `[providers.pocket_tts]` and
  `[providers.gpt_sovits]` sections are tolerated for boot compatibility but ignored (with a startup
  warning): delete them from old files. Voice profiles of retired providers
  are kept on disk and listed, but cannot synthesize (see
  [Retired providers](#retired-providers)).
- Endpoints: `GET /api/providers`, `GET /api/providers/{id}/status`,
  `POST /api/voices/clones`, `GET /api/voices`, `GET /api/voices/{id}`,
  `DELETE /api/voices/{id}`, `POST /api/voices/{id}/preview`; extended
  `/v1/audio/speech` (new model IDs, profile voices, optional `language`)
  and dynamic `/v1/models`. Legacy `/api/tts` and OpenAI aliases behave
  exactly as before.
- Web UI provider table (server-rendered, no new framework) and an
  optional TikTools `provider`/`model` selection that preserves the
  Supertonic default.

## Build features

Performance work planned after this Vulkan rollout is tracked in
[future-optimizations.md](future-optimizations.md).

| Feature | Effect |
|---|---|
| (default) | Supertonic 3 + Reverb-TTS compiled with Vulkan-first ONNX inference (`webgpu`); no weights fetched |
| `webgpu` | ORT WebGPU EP: Vulkan on Linux, D3D12 on Windows, Metal on macOS. Tried first for Supertonic; Reverb-TTS opts into it with `device = "auto"`; CPU is the automatic fallback. Set `SONICBOOM_EP=cpu` to force CPU only; `SONICBOOM_WARMUP=0` skips the post-load GPU warm-up. Ships in `default` |
| `reverb-tts` | Reverb-TTS ONNX provider (ships in `default`, disabled by default; **weights are CC BY-NC 4.0, non-commercial**). CPU by default, `device = "auto"` runs the transformer on Vulkan |
| `cuda` / `rocm` / `coreml` | Alternative ORT EPs for Supertonic. `cuda`/`rocm` require `--no-default-features` (no combined prebuilt with WebGPU); `coreml` is chosen automatically on macOS |

AMD BC-250 (`gfx1013`) is CPU-only for the acceptance criteria. Do not
imply CUDA runs on AMD, and do not claim ROCm/MIGraphX support without
proof. `coreml`/`cuda`/`rocm` stay mutually exclusive.

## Language support (explicit, no silent substitution)

- Reverb-TTS: `de`, `en`, `es`, `fr`, `it`, `pl`, `pt` with one multilingual
  model (`reverb-tts`). Other languages (including Japanese and Chinese)
  return an explicit unsupported-language error.
- Retired providers (GPT-SoVITS, Qwen3-TTS, Pocket TTS): their model IDs return
  an explicit retired-model error and their old voice profiles are unavailable
  (recreate as Reverb-TTS). No language falls back to English or another provider.
- Supertonic: unchanged (English synthesis; legacy voice aliases).

Unknown provider/model/voice/language, incompatible variants (any mode other than `icl`), missing assets, and unavailable devices all return explicit,
actionable errors. Configured `cpu` fallback from `auto` is allowed where
supported, and the effective device is always reported.

### Text normalization coverage

Text normalization (`tn-v2.1`, see [normalization.md](normalization.md)) runs
before every provider. It is separate from provider language support: a
provider that cannot synthesize a language still rejects it, whatever the
normalizer can render. Each language uses its own rules. No language is
normalized with English or Spanish rules. `reading_style` (`precise`, `natural`
or `digits`) is applied once by the shared normalizer, so every provider
receives identical prepared text for the same request.

| Language | Locales | What `auto` converts | Preserved (unchanged, often with a warning) |
|---|---|---|---|
| `pt` | pt-BR, pt-PT | cardinals, decimals, `%`, `R$`/`€`/`US$`/codes, slash and ISO dates, `HH:MM`, units, `1º`/`1ª`, ranges, Dr./Sr./Av. | `$`, `¥`, nouns that need gender (`2 casas`), `S/` (PEN), pt-PT space-grouped decimals |
| `fr` | fr-FR | cardinals, decimals, `%` (also `12,5 %`), `€`/`US$`/codes, dates, `HH:MM`, units, ranges, Dr. | space-grouped decimals (`1 234,50 €`), `1er`/`2e`, `15h30`, `un`/`une` before nouns |
| `de` | de-DE | decimals, `%`, `€`/`US$`/codes, ISO and slash dates, units, ranges, Dr./Nr. | standalone integers (they may be ordinals such as `3.`), clock times (`15:30 Uhr`), dot dates (`26.10.2026`) |
| `it` | it-IT | cardinals, decimals, `%`, `€`/`US$`/codes, dates, `HH:MM`, units, `1º`/`1ª`, Dott./Sig. | `15.30`, `uno`/`una` before nouns, ranges |
| `ru` | ru-RU | cardinals before verified nouns (`5 рублей`, `21 год`), decimals, `%`, `€`/`US$`/codes, km/kg | other nouns and cases, dates, times, Cyrillic unit abbreviations (`км`) |
| `zh` | zh-CN, zh-TW | Hanzi cardinals, decimals, `%`, ISO dates, `HH:MM`; zh-CN only: currency codes and units | numbers attached to Hanzi (`2026年10月26日`, `3个`), measure words, `¥` |
| `ja` | ja-JP | kanji cardinals, decimals, `%`, ISO dates, `HH:MM`, `JPY`/`USD`/`EUR`/`GBP`, units | numbers attached to kana/kanji (`500円`), counters (`3 人`), `¥`, amounts with cents |
| `ko` | ko-KR | Sino-Korean cardinals, decimals, `%`, ISO dates, clock hours 1–12, currency codes, units, `10000 원` | numbers attached to Hangul (`2026년`), native counters (`3 개`), 24-hour times above 12 |

All of these expectations were written by an agent and have not yet been
reviewed by fluent speakers. Coverage per feature is reported by
`/api/info` and in each `/api/tts/normalize` response.

## Pinned download manifests

Optional-provider downloads retry transient network failures, interrupted bodies,
HTTP 408/429 and HTTP 5xx responses up to five total attempts per file, waiting
1, 2, 4 and 8 seconds between attempts. Each retry restarts that file; completed,
verified files remain cached. Connections time out after 10 seconds and reads
after 60 seconds without data, allowing large files to continue while data arrives.
Permanent HTTP errors, oversized bodies, checksum mismatches and local disk errors
fail immediately. Staged files are removed on failure; only size- and
SHA-256-verified files are installed.

Reverb-TTS files (three pinned repositories, full hashes in
`src/tts/providers/reverb_tts.rs` `MANIFEST`): the transformer graphs
(`melser/Reverb-TTS-onnx`), the Higgs Audio v2 codec encoder/decoder
(`melser/Higgs-Audio-2-Tokenizer-onnx`) and the original tokenizer
(`software-mansion/Reverb-TTS`).

## Reverb-TTS setup (non-commercial)

Reverb-TTS (Software Mansion, derived from OmniVoice) clones a voice from a
**3-10 s reference recording plus its exact transcript** (`icl` mode) in seven languages.

> **License.** The Reverb weights are **CC BY-NC 4.0** (non-commercial, attribution) and the
> audio codec follows the **Boson Higgs Audio 2 Community License** (Meta Llama 3 terms;
> products above 100,000 annual active users need an expanded license). The provider is
> disabled by default and nothing is downloaded until you enable it. Do not enable it in a
> commercial product.

1. Set `[providers.reverb_tts] enabled = true` (or the **Engines** toggle). SonicBoom downloads
   about 1.9 GB from three pinned Hugging Face revisions and verifies each file's SHA-256:
   `melser/Reverb-TTS-onnx` (transformer), `melser/Higgs-Audio-2-Tokenizer-onnx` (codec) and
   the original `software-mansion/Reverb-TTS` (tokenizer). Conversion scripts and parity checks:
   <https://github.com/nglmercer/Reverb-TTS-onnx>.
2. Watch `/api/providers/reverb-tts/status` until `ready`.
3. Create a profile with the reference WAV **and its transcript** (`mode=icl` is the only mode;
   the transcript must be the exact words spoken, in the reference's language):

```bash
curl -X POST 'http://127.0.0.1:17842/api/voices/clones?name=ana&provider=reverb-tts&model=reverb-tts&language=es&mode=icl&consent_basis=owner&transcript=Hola%2C%20me%20llamo%20Ana.' \
  -H 'Authorization: Bearer <token>' -H 'Content-Type: audio/wav' \
  --data-binary @reference.wav
```

```json
{ "model": "reverb-tts", "voice": "<profile-id>", "language": "es",
  "input": "El tren sale a las ocho de la mañana.", "response_format": "wav" }
```

Notes:
- **Reference quality dominates.** Use 3-10 s of clean speech from one speaker in the language
  you will synthesize (longer than 15 s makes worse takes). A short or cross-language clip is the
  most common cause of poor output, for every cloning provider.
- **Devices.** `device = "cpu"` (default) runs about 0.5-0.65x real time on a 12-thread APU:
  the transformer is plain fp32 matrix multiplication (81 % of the time in an ORT profile), so
  neither more threads nor session options help. `device = "auto"` runs the embedding,
  transformer and head graphs on Vulkan (ORT WebGPU EP): about 2x real time on an AMD BC-250
  (4.5 s of audio in 2.1 s, warm) with a bit-identical take. **Use `auto` when a Vulkan GPU is
  available**; if the GPU self-check fails the graphs are rebuilt on the CPU. The codec
  encoder/decoder always run on the CPU because the convolution decoder returns near-silence on
  RADV; with `auto` they use every core (up to 12) rather than `cpu_threads`.
  `SONICBOOM_REVERB_EP=cpu` forces the CPU. Reverb and Supertonic share one process-wide GPU lock
  (loads and inference runs), so a Reverb pass and a Supertonic run never overlap on the GPU;
  they simply wait for each other (the archived Pocket TTS startup crash inside Dawn is described
  in `docs/future-optimizations.md`).
- **Latency breakdown.** Each take logs `encode_ms` (reference, first use only), `prefix_ms`,
  `sample_ms` (`embed_ms` + `core_ms` + `heads_ms` over `passes`), and `decode_ms`. Warm, 4.5 s of
  audio, BC-250: Vulkan `prefix` 0.1 s, `sample` 1.2 s, `decode` 0.6 s; CPU `prefix` 0.7 s,
  `sample` 5.3 s, `decode` 0.8 s. The first take after a restart also encodes the reference
  (about 1-2 s). Reproduce with `examples/reverb_bench.rs`.
- **Load.** The six graphs (embeddings, core, heads, codec encoder and decoder) and the tokenizer are created concurrently
  (GPU graphs one after another, since Dawn dislikes overlapping creation): about 2.6 s instead of
  5.9 s on the CPU. Pinned files are re-hashed on every load unless they are regular files with a
  `*.verified.json` sidecar: a symlinked `onnx/` directory (a development layout) is never
  trusted and costs about 4 s per load: hard-link or copy the files instead.
- **Cost.** Peak resident memory measured at about 2.9 GB on the CPU (1.9 GB with Vulkan before the codec threads change; not re-measured). `steps` (default 8, upstream's tuned value) trades
  speed for fidelity; seeds are reproducible per request.
- **Text.** The request text is normalized by SonicBoom's own normalizer (not Verbalis).
  Texts estimated over 30 s are split at sentence ends into pieces of about 15 s; a single
  sentence over about 90 s is rejected: add punctuation.
- **Not provided.** No built-in preset voices yet (upstream ships 16 as code files) and no
  per-request speed control.

## Retired providers

GPT-SoVITS, Pocket TTS and Qwen3-TTS (both the Candle and the GGUF/ONNX
backends) were removed. SonicBoom never reinterprets their data as another engine's:

- Their model IDs (`gpt-sovits-v2-onnx`, `gpt-sovits-v2pro-onnx`,
  `gpt-sovits-v2proplus-onnx`, `qwen3-tts-0.6b-base`, `qwen3-tts-1.7b-base`, `pocket-tts-en|es|pt|de|it|fr-24l`)
  return `400` with an explicit "retired" message that suggests `reverb-tts`,
  never a silent reroute and never a generic unknown-model error.
- Leftover `[providers.qwen3]`, `[providers.qwen3_gguf]`, `[providers.pocket_tts]`
  and `[providers.gpt_sovits]` config sections boot with a warning and are
  otherwise ignored. They are dropped the next time the configuration is saved.
- Stored voice profiles of the retired providers (`gpt-sovits`,
  `qwen3-candle`, `qwen3-gguf`, `pocket-tts`) still load, appear in `GET /api/voices`, and can
  be deleted, but any synthesis with them returns `400` explaining that they
  must be recreated as `reverb-tts` profiles (mode `icl`, with the exact
  transcript) from the reference audio. There is no automatic migration.
- Leftover model caches (`models/qwen3-tts-1.7b-gguf`, `models/pocket-tts`, `models/gpt-sovits-v2pro`)
  are left inert for manual removal.

## Configuration reference

```toml
[voices]
store_dir = "./voices"
max_upload_bytes = 8388608
```

### Reverb-TTS (`reverb-tts`, non-commercial weights)

```toml
[providers.reverb_tts]
enabled = false
model_dir = "./models/reverb-tts"
model_id = "reverb-tts"
revision = "ca95b327a5624f0988511e192510970885430421"
device = "cpu"           # cpu | auto (Vulkan for the transformer; codec stays on the CPU)
cache_entries = 16       # reference-code cache (1-1024)
steps = 8                # main sampler passes (1-64); upstream default 8
cpu_threads = 4          # 1-32
```

No passwords or tokens live in TOML. Invalid provider updates keep the
last healthy state and surface actionable status via
`/api/providers/{id}/status` and `/api/config/status`.

## Security notes

- All speech endpoints keep Bearer-token auth, rate limits, inference
  admission limits, body limits, and timeout semantics. `/v1/models` and
  `/v1/voices` stay public metadata without private clone data; voice
  profiles require authorization and are scoped to the owning token
  (foreign IDs report `404`, never existence-revealing `403`).
- Reference uploads validate duration, decoded sample count, channels,
  sample rate, MIME **and actual format** (WAV contents required;
  FLAC/MP3 uploads return `415`), transcript presence, and consent basis.
  Oversize uploads return `413`. Paths are UUID-derived; traversal IDs
  are rejected.
- Only user-owned/authorized recordings are supported; store the
  permission/consent basis on the profile. Do not bundle proprietary or
  unlicensed character voice checkpoints, and do not claim official
  Hatsune Miku support.

## Open limitations (honest, not silent)

- Native weight execution is wired end-to-end
  (serialized model ownership, reference-code caching, atomic
  reference+synthesis, SHA-256-verified downloads). CPU smoke-test results
  are recorded in `benchmarks/providers.md`; rows stay pending until
  measured. No fake/silent fallbacks exist anywhere in the new code.
- Non-`icl` modes, ROCm/CUDA on BC-250, and FLAC/MP3 reference uploads are
  explicitly unsupported and rejected with actionable errors.

## Reliable native synthesis jobs

Use the authenticated [background job API](inference-jobs.md) for long CPU
inference, with progress and cooperative cancellation. The synchronous routes
remain supported. See [ownership/cache architecture](inference-architecture.md)
for model residency, integrity verification, reloads and cache bounds.

Cache-only changes retain weights; genuine model/device changes coordinate with
active work. Cloning engines can
be slower than real time on the CPU: use jobs rather than raising an HTTP
timeout alone. Measured resident/peak RAM can be several GB per model; see the
current benchmark rows before enabling multiple backends.

### Audio isolation and quality follow-up (2026-10-16)

Use `tts-quality-audit --help` (Rust binary) and the commands in
`benchmarks/tts-quality/README.md` for independent direct-provider WAV manifests.
The UI now separates new submission from explicitly resuming the stored previous
input; a changed text/model/voice cannot silently resume an older job. A new
submission clears and releases old audio, and disposal invalidates callbacks.
Cancellation acknowledgement does not imply native completion.

Only reference conditioning uses the new centered Blackman windowed-sinc
resampler (`sinc-blackman-96-zero-phase-v1`), with integer ceil duration and
versioned conditioning identity. Same-rate audio is copied unchanged. The
filter does not denoise, trim, stretch or normalize waveforms. Interpolation
can ring at sharp discontinuities; there is no hidden clipping/limiting step.
Playback and output-format resampling retain their existing implementation.
