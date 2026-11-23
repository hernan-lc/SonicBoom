# SonicBoom

A high-performance web server that generates Text-to-Speech (TTS) audio using the Supertonic 3 ONNX model and delivers it via HTTP.

## Documentation Index

This README serves as an index to all SonicBoom documentation:

| Document                                | Description                         |
| --------------------------------------- | ----------------------------------- |
| [API Reference](docs/api.md)            | Complete API documentation          |
| [OpenAI-Compatible API](docs/openai.md) | OpenAI TTS API compatible endpoints |
| [Admin Panel](docs/admin.md)            | Admin panel guide                   |
| [Configuration](docs/config.md)         | TOML, first-run setup and live settings |
| [Security](SECURITY.md)                   | Security policy and credential rotation |

---

## Frontend development

The interface uses Vue, TypeScript and Vite, with English and Spanish localization
and dedicated speech, voice library, engine and settings views. See the [frontend guide](docs/frontend.md)
for localhost hot reload and embedded production builds. Building the server
requires Node.js 22.12+ and npm; deployed binaries need neither.

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/nglmercer/SonicBoom.git
cd SonicBoom

# Default build: Supertonic 3 + Reverb-TTS (the cloning provider is disabled
# until enabled in the configuration). ONNX inference uses the
# WebGPU execution provider (Vulkan on Linux) with automatic CPU fallback.
cargo build --release

# Desktop system-tray GUI (included in the defaults; official releases add it
# explicitly with `--features gui`):
cargo build --release --features gui
```

> Windows x64: the prebuilt ONNX Runtime library resolves its MSVC STL
> vectorized-algorithm symbols from the dynamically linked CRT (`/MD`) shipped
> with the `windows-2025` toolchain. Do not add Rust `__std_*` replacements:
> they collide with `msvcprt.lib` and break the link with LNK2005. Native
> dependencies are built with the dynamic CRT (`KNF_STATIC_CRT=0`,
> `vendor/esaxx-rs` uses `static_crt(false)`); see `docs/RELEASING.md`.

### GPU acceleration

ONNX inference tries the WebGPU execution provider first (Vulkan on Linux,
D3D12 on Windows, Metal on macOS) and falls back to the CPU automatically; end
users need only a system Vulkan loader and a compatible GPU driver. Force the
CPU with `SONICBOOM_EP=cpu`. No Vulkan SDK is needed to build. Reverb-TTS uses
the same GPU-first behavior by default (`[providers.reverb_tts] device = "auto"`);
if no usable GPU is found, either engine falls back to CPU automatically and
`/api/providers` reports the device actually in use.

A vendor execution provider replaces WebGPU and needs `--no-default-features`
because no prebuilt ONNX Runtime combines them. Enable at most ONE per
platform:

```bash
cargo build --release --no-default-features --features server,web-ui,reverb-tts,cuda    # NVIDIA CUDA (Linux/Windows)
cargo build --release --no-default-features --features server,web-ui,reverb-tts,rocm    # AMD ROCm (Linux)
cargo build --release --features coreml                                                            # Apple Silicon CoreML (macOS)
```

### Run

Desktop builds open a first-run setup wizard. No `.env` file is needed:

```bash
cargo run --release --features gui
```

Choose local or LAN mode, an audio output, model storage, and admin
credentials. Local mode binds loopback without an API token. LAN mode
requires bearer authentication and shows its initial token once.
Configuration lives in the platform application directory as `config.toml`.

For an unattended server, keep using environment variables:

```bash
export PORT=17842
export BIND=127.0.0.1       # use 0.0.0.0 for deliberate LAN exposure
export SONICBOOM_ADMIN_ID=admin
export SONICBOOM_ADMIN_PW=<GENERATE-A-RANDOM-PASSWORD>
export ALLOWED_AUDIO_DIR=./audio
cargo run --release
```

Use a unique password of at least 12 characters. API token hashes remain
in `tokens.json`, which is initialized safely when missing. The admin panel
(`/admin`) displays each newly created token once.

Edit `config.toml` or use `SonicBoom config set` to apply settings live.
See the [configuration guide](docs/config.md) for location overrides,
revisions, migration, source tracking and runtime apply status.

The server will:

1. Start listening on port 17842
2. Download the Supertonic 3 model (first run)
3. Load the model
4. Be ready to serve TTS requests

---

## Features

- **ONNX Runtime Inference** - Supertonic 3 with hardware acceleration (CoreML on Apple Silicon, CUDA/ROCm on Linux via `--features cuda`/`--features rocm`)
- **Buffered Audio Output** - Opus/OGG, WAV, MP3, and FLAC audio output (each response is one fully encoded buffer, not an incremental stream)
- **Token-Based Authentication** - API access control
- **Verified Model Supply Chain** - Pinned revision with compiled-in SHA-256 integrity checks
- **Admin Panel** - Web-based management interface
- **OpenAI-Compatible API** - Drop-in replacement for OpenAI TTS
- **Speech-to-Text** - Fluid Speech Pro ASR (25 languages, CPU, lazy download; [docs/asr.md](docs/asr.md))
- **Audio Queue System** - Play audio files directly on the server with queue management
- **Output Device Selection** - Discover and switch server audio outputs (speakers, HDMI, virtual cables) via API
- **Session Management** - Secure admin sessions with lockout protection
- **Desktop Tray (optional)** - System tray GUI via the `gui` feature (macOS/Windows/Linux; Linux uses StatusNotifierItem over D-Bus, no GTK build dependencies)

---

## API Endpoints

### Original TTS API

| Method | Endpoint        | Description                   |
| ------ | --------------- | ----------------------------- |
| `POST` | `/api/tts`      | Generate TTS audio            |
| `POST` | `/api/tts/play` | Synthesize and play on server |
| `GET`  | `/api/status`   | Check model status            |

### Speech-to-Text

| Method | Endpoint                     | Description                       |
| ------ | ---------------------------- | --------------------------------- |
| `POST` | `/api/asr`                   | Transcribe a WAV file             |
| `GET`  | `/api/asr/status`            | ASR model lifecycle status        |
| `POST` | `/api/asr/prepare`           | Download/load the ASR model       |

### Audio Queue API

| Method | Endpoint            | Description                   |
| ------ | ------------------- | ----------------------------- |
| `POST` | `/api/queue`        | Add file to playback queue    |
| `POST` | `/api/queue/next`   | Play next item in queue       |
| `POST` | `/api/queue/pause`  | Pause playback                |
| `POST` | `/api/queue/resume` | Resume playback               |
| `POST` | `/api/queue/stop`   | Stop playback and clear queue |
| `POST` | `/api/queue/volume` | Set playback volume           |
| `GET`  | `/api/queue/status` | Get current queue status      |

### Audio Output Device API (playback builds)

| Method | Endpoint            | Description                        |
| ------ | ------------------- | ---------------------------------- |
| `GET`  | `/api/audio/devices`| List output devices + selection    |
| `GET`  | `/api/audio/output` | Get active output device           |
| `POST` | `/api/audio/output` | Switch output device at runtime    |

### OpenAI-Compatible API

| Method | Endpoint                  | Description                  |
| ------ | ------------------------- | ---------------------------- |
| `POST` | `/v1/audio/speech`        | Generate TTS (OpenAI format) |
| `POST` | `/v1/audio/transcriptions` | Transcribe WAV (OpenAI format) |
| `GET`  | `/v1/models`              | List models                  |
| `GET`  | `/v1/models/list`         | List models (alias)          |
| `GET`  | `/v1/voices`              | List voices                  |

### Voice Cloning (optional native providers)

| Method   | Endpoint                      | Description                              |
| -------- | ----------------------------- | ---------------------------------------- |
| `GET`    | `/api/providers`              | List providers, capabilities, and status |
| `GET`    | `/api/providers/{id}/status`  | One provider's lifecycle status          |
| `POST`   | `/api/voices/clones`          | Upload reference WAV, create a profile   |
| `GET`    | `/api/voices`                 | List your voice profiles                 |
| `GET`    | `/api/voices/{id}`            | One voice profile                        |
| `DELETE` | `/api/voices/{id}`            | Delete a profile and its reference audio |
| `POST`   | `/api/voices/{id}/preview`    | Synthesize text with a profile           |

See [docs/providers.md](docs/providers.md) for Reverb-TTS (7 languages, ICL
cloning, non-commercial CC BY-NC weights) setup (disabled by default; CPU works
without GPU). Qwen3-TTS, Pocket TTS and GPT-SoVITS were removed; their stored
voice profiles stay listed but cannot synthesize (recreate them as
`reverb-tts` profiles).

### Admin Panel

| Method   | Endpoint                       | Description      |
| -------- | ------------------------------ | ---------------- |
| `GET`    | `/admin`                       | Admin dashboard  |
| `GET`    | `/admin/login`                 | Login page       |
| `POST`   | `/admin/login`                 | Admin login      |
| `POST`   | `/admin/logout`                | Admin logout     |
| `POST`   | `/admin/tokens`                | Create new token |
| `POST`   | `/admin/tokens/{id}/revoke`    | Revoke token     |

### Web Routes

| Method | Endpoint  | Description                        |
| ------ | --------- | ---------------------------------- |
| `GET`  | `/`       | Home page (TTS demo, needs token)  |
| `GET`  | `/health` | Liveness probe                     |
| `GET`  | `/ready`  | Readiness probe (model loaded)     |

---

## Usage

### Generate TTS Audio

```bash
# Using original API
# Audio is encoded as Opus inside an OGG container (Content-Type: audio/ogg; codecs=opus).
curl -X POST "http://localhost:17842/api/tts?voice=F1" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d "Hello, world!" \
  --output audio.ogg

# Using OpenAI-compatible API
curl -X POST http://localhost:17842/v1/audio/speech \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"input": "Hello, world!", "voice": "alloy"}' \
  --output audio.ogg
```

---

## Technology Stack

| Component      | Technology                               |
| -------------- | ---------------------------------------- |
| Runtime        | [Tokio](https://tokio.rs/)               |
| Web Framework  | [Axum](https://github.com/tokio-rs/axum) |
| ONNX Inference | [ORT](https://github.com/DBDi/ort)       |
| Audio Encoding | [Opus](https://opus-codec.org/)          |
| Serialization  | [Serde](https://serde.rs/)               |
| Logging        | [Tracing](https://tokio.rs/blog/tracing) |

---

## Project Structure

```textplain
SonicBoom/
├── src/
│   ├── main.rs              # Application entry point
│   ├── config/             # Central ConfigManager, TOML, validation and watcher
│   ├── runtime/            # Subsystems and live reconfiguration
│   ├── server/             # Restartable HTTP listener
│   ├── setup/              # First-run wizard
│   ├── error.rs             # Error types
│   ├── logging.rs           # Logging setup
│   ├── admin/               # Admin panel
│   │   ├── handlers.rs      # Admin HTTP handlers
│   │   ├── lockout.rs       # Failed-login IP lockout
│   │   ├── session.rs       # Admin session helpers
│   │   └── templates.rs     # Session-protected Vue page data
│   ├── api/                 # API handlers
│   │   ├── tts.rs           # Original TTS API
│   │   ├── openai.rs        # OpenAI-compatible API
│   │   ├── queue.rs         # Audio queue API
│   │   └── audio.rs         # Output device API
│   ├── auth/                # Authentication
│   │   ├── store.rs         # Token storage
│   │   └── token.rs         # Token types/validation
│   ├── tts/                 # TTS engine
│   │   ├── audio.rs         # Audio encoding (Opus/OGG/MP3/FLAC/WAV)
│   │   ├── download.rs      # HuggingFace model download
│   │   ├── inference.rs     # ONNX inference
│   │   ├── model.rs         # Model loading
│   │   ├── queue.rs         # Server-side playback queue
│   │   ├── devices.rs       # Output device discovery
│   │   └── text.rs          # Text normalization
│   └── web/                 # Web frontend
│       └── index.rs         # Home page
├── docs/
│   ├── api.md              # API reference
│   ├── openai.md           # OpenAI API guide
│   ├── admin.md            # Admin panel guide
│   └── config.md           # Configuration guide
├── frontend/               # Vue components, TypeScript controllers, Vite
├── build.rs                # Build and embed production UI
└── Cargo.toml
```

---

## Acknowledgments

- [Supertonic 3](https://huggingface.co/Supertone/supertonic-3) - The TTS model
- [Fluid Speech Pro by FluidVoice](https://huggingface.co/FluidVoice-Labs/fluid-speech-pro) - ASR model (CC BY 4.0)
- [ONNX Runtime](https://onnxruntime.ai/) - Cross-platform ML inference
- Upstream project: [daramkun/SonicBoom](https://github.com/daramkun/SonicBoom)
  (this repository, [nglmercer/SonicBoom](https://github.com/nglmercer/SonicBoom),
  is a hardened fork)

## Reliable native synthesis jobs

Use the authenticated [background job API](docs/inference-jobs.md) for long CPU
inference, with progress and cooperative cancellation. The synchronous routes
remain supported. See [ownership/cache architecture](docs/inference-architecture.md)
for model residency, integrity verification, reloads and cache bounds.

## Binary releases

See [release instructions and runtime requirements](docs/RELEASING.md) for
supported Linux/Windows x64 archives, checksums, smoke tests and troubleshooting.

### Native Termux / Android ARM64

See [native Termux installation, runtime ABI and testing](docs/termux/README.md) for the CPU API-only build without Node.js, a desktop or an audio server.
