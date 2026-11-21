# SonicBoom

Rust/Axum TTS server (Supertonic 3 via ONNX Runtime; optional Reverb-TTS). Default port `17842`.

## Docs

Read the relevant doc before changing code:

- [README.md](README.md) — overview, build, run, Docker
- [docs/api.md](docs/api.md) — `/api/tts`, status codes, auth
- [docs/openai.md](docs/openai.md), [docs/admin.md](docs/admin.md), [docs/config.md](docs/config.md), [docs/providers.md](docs/providers.md), [docs/frontend.md](docs/frontend.md)
- [docs/](docs/) — everything else

## Notes

- Commits are fine (ask if unsure); always ask before `git push`.
- Keep docs in sync when behavior changes.
- UI text: no explanatory paragraphs. Use short labels plus an interactive guide (`frontend/src/guides/registry.ts`, Driver.js). Error, validation and diagnostic messages stay as text.
