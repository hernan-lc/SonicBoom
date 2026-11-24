# Speech-to-text (ASR)

Local transcription with [Fluid Speech Pro](https://huggingface.co/FluidVoice-Labs/fluid-speech-pro-onnx), a FastConformer-TDT model (0.63B, 25 European languages, automatic language detection, punctuation and capitals). Model revision `fa588e9f87f1b9ad0ca2e2d8f7eaf84dc3803581`, int8 graphs only.

**License:** CC BY 4.0, commercial use allowed with attribution: **Fluid Speech Pro by FluidVoice**. See `tests/fixtures/ldc93s1.wav` (LDC93S1, LibriSpeech, CC BY 4.0), used only by the ignored integration test.

## Routes

| Method | Endpoint | Auth | Body |
| --- | --- | --- | --- |
| `POST` | `/api/asr` | Bearer | multipart `file` (WAV) + optional `language` |
| `POST` | `/v1/audio/transcriptions` | Bearer | multipart `file`, `model`, `language`, `response_format` |
| `GET` | `/api/asr/status` | Bearer | — |
| `POST` | `/api/asr/prepare` | Bearer | — (warm-up / download) |

Raw `Content-Type: audio/wav` is also accepted on `/api/asr`. WAV aliases: `audio/wav`, `audio/x-wav`, `audio/wave`, `audio/vnd.wave`.

Responses: `{"text":"...","language":"es","duration":5.9,"model":"fluid-speech-pro"}`. `response_format`: `json` (default), `text`, `verbose_json` (adds `task` and an empty `segments` array). `model` accepts `fluid-speech-pro` or OpenAI aliases (`whisper-1`, `whisper-large-v3`, `whisper-large-v3-turbo`). Innocuous extra fields (`prompt`, `user`, `timestamp_granularities*`) are ignored; `temperature != 0`, `stream=true` and word/segment timestamps return `400`.

`language` is validated against the 25 supported codes and echoed in the response; the model detects language internally and does not emit a language id. When omitted, the response contains an approximate hint (`auto` if undecidable).

## Limits and lifecycle

- WAV input: PCM 8/16/24/32-bit or float, mono/stereo, any sample rate; resampled to 16 kHz with the existing Blackman-windowed sinc resampler.
- Max 30 s per request (`SONICBOOM_ASR_MAX_SECONDS` overrides); longer audio returns `400`. No chunking in this phase.
- Multipart body cap: 8 MiB (`413` above it). Standard Bearer auth (`401`), token-bucket rate limit (`429`) and inference admission (`429`) apply.
- Weights download on first transcription or on `/api/asr/prepare`, from the pinned revision with SHA-256 checks; `SONICBOOM_ASR_MODEL_DIR` sets the directory (default `models/fluid-speech-pro`), `HF_TOKEN` authenticates downloads.
- Download and load run as **one background task owned by the ASR manager**, not inside an HTTP request. `POST /api/asr/prepare` schedules it and returns the current status immediately; `GET /api/asr/status` exposes `progress` and the terminal state, and the same shape is available as `asr` in `GET /api/providers`. Cancelling or timing out a request never cancels the download, and repeated prepare calls share the same task. No inference admission permit is held while downloading.
- One shared instance, one transcription at a time. Inference is always CPU: the model's convolutions return silence on the user's Vulkan GPU. Status states: `absent`, `downloading`, `loading`, `ready`, `failed`.

`fluid-speech-pro` also appears in `GET /v1/models` with `capabilities: ["asr"]`; it is not added to the TTS model list, so speech clients are unaffected.

## CPU measurements

Debug build, AMD BC-250 Linux host, 4 ORT intra-op threads, one request at a time; from `cargo test --test asr_real -- --ignored --nocapture` (last run):

| Clip | Wall time | Peak RSS (whole process) |
| --- | ---: | ---: |
| 5 s | 1.69 s | ~1.07 GiB |
| 30 s | 7.29 s | ~1.07 GiB |

Disk: **800,847,271 bytes** (801 MB) for the six pinned files; the two `.data` weight files are 799,789,056 bytes. The fixture transcript matches onnx-asr 0.12.0 token-for-token.
