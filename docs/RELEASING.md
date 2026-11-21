# Releases and runtime requirements

Release binaries use the default features plus `gui`: Supertonic,
Reverb-TTS, playback, HTTP server and tray support are retained.
Models are downloaded separately on normal startup; no credentials, user
configuration, downloaded voices/models or caches belong in release archives.

## Supported release targets

* Linux x86_64: built/tested on Ubuntu 24.04 (glibc 2.39). Install
  `libasound2t64 libssl3t64 libstdc++6 libgcc-s1 ca-certificates` using apt.
  Other distributions need equivalent libraries and a compatible glibc; they
  are not separately certified. The tray uses a D-Bus StatusNotifier host;
  a missing host falls back to server operation. Launch `./sonicboom` to
  include any packaged ONNX shared libraries in the loader search path.
* Windows x86_64: Windows 10 (2004 or newer)/11 with the current Microsoft Visual C++
  2015–2022 x64 Redistributable (https://aka.ms/vs/17/release/vc_redist.x64.exe).
  The pinned Windows ORT static distribution also links DirectML/D3D12/DXGI/
  DXCore imports even though SonicBoom selects CPU inference. The archive must carry the pinned DirectML 1.15.4 DLL and its
  separate Microsoft redistribution terms. Keep the DLL with the executable; do not infer GPU support from those imports.
  Launch `SonicBoom.exe`. CI uses Windows Server 2022; consumer desktop
  tray interaction and physical audio devices still need manual checks.
* macOS is not a release target. The source has Apple feature gates, but no
  macOS archive is advertised until native CI, signing/notarization, runtime
  dependencies and hardware testing are established.

These archives are portable application bundles, **not fully standalone**.
System libraries, the Windows redistributable, certificates, audio devices,
and models remain external requirements. Packaging carries Cargo-copied native
DLL/SO files and dependency license files. `ldd` on Linux, PE import inspection
on Windows, and isolated startup with a restricted Windows DLL search PATH
check native loading. The model-free probe explicitly initializes ONNX Runtime.
The locked CPU ORT distribution may link statically; do not invent a DLL
requirement if the produced executable has no ONNX dynamic import. If future
ORT builds use shared libraries they must appear beside the built executable
(via `ort/copy-dylibs`), be packaged with their notices, and pass the probe.

Vulkan acceleration is included in these packages: Supertonic uses ORT's
WebGPU EP (Vulkan on Linux) with automatic CPU fallback, and Reverb-TTS can
opt into it. CUDA/ROCm/CoreML builds require separate SDKs,
provider libraries, driver compatibility and hardware validation. CI does not
certify model inference quality, GPU execution, tray interaction or audio playback.

## Create a release

1. Fork the project (or push a clone to a GitHub repository you can write to),
   enable Actions if needed, and merge the desired changes into that repository's
   default branch.
2. In **Actions → Release → Run workflow**, select the default branch. Choose
   `build` to test and download temporary workflow artifacts without publishing.
   For a release, choose `publish`, enter a new version such as `v1.2.3` or
   `v1.2.3-rc.1`, and select `prerelease` if appropriate. Start the workflow.
3. Both Linux and Windows builds must pass their tests and package smoke checks.
   The publish job checks the commit is on the default branch, checks the release
   does not already exist, and verifies the final checksums. For manual
   publication it then creates the new tag at the tested commit. It stages a
   draft, verifies its downloaded assets, publishes it with generated release
   notes, and verifies the published assets again.
4. Confirm the release in your repository contains
   `sonicboom-linux-x86_64.tar.gz`, `sonicboom-windows-x86_64.zip`, and
   `SHA256SUMS`. Download and verify them.

Alternatively, pushing a new `v*` tag on a commit in the default branch starts
the same build and publication flow. Tag-triggered releases are marked stable;
use manual publication to mark a prerelease. A manual build run on a branch or
tag only uploads workflow artifacts, retained for 14 days.

Publishing uses the workflow repository's `GITHUB_TOKEN` and `contents: write`
only in the publish job; no PAT or cross-repository upload is needed. Repository
or organization Actions policies must allow the workflow and its write access.
Existing releases with assets or notes are not overwritten. For tag-triggered
runs, the existing preflight removes an empty placeholder release before staging
the draft. If publication fails after creating a tag, inspect the run and any
draft before retrying. Rerun the failed publication job if it is safe to do so;
if a draft with assets or notes remains, inspect and deliberately delete that
unpublished draft before retrying. Do not move the tag or overwrite a published
release; use a new version if the commit or published assets need to change.

A tag created by the manual workflow's `GITHUB_TOKEN` does not trigger the
separate CUDA workflow. Run **CUDA full build** manually when that validation is
needed. A user-pushed `v*` tag triggers it separately.

## Local packaging and checksum verification

Use the same target as the native host (MSVC Windows cannot be validated by
Linux cross-compilation):

```sh
cargo test --locked --features gui --target x86_64-unknown-linux-gnu
cargo build --release --locked --features gui --target x86_64-unknown-linux-gnu
python scripts/release/package.py --target x86_64-unknown-linux-gnu
python scripts/release/smoke.py dist/sonicboom-linux-x86_64.tar.gz
```

Substitute `x86_64-pc-windows-msvc` and the `.zip` name on Windows. Python 3.12+
is required. With both packages present:

```sh
python scripts/release/package.py --checksums
python scripts/release/package.py --checksums --verify
cd dist
sha256sum -c SHA256SUMS
```

Windows PowerShell: compare `(Get-FileHash .\sonicboom-windows-x86_64.zip
-Algorithm SHA256).Hash` with the corresponding entry in `SHA256SUMS`.
Checksums cover the **final archive bytes**, not temporary extracted files.
Archive entry order, ownership, modes, gzip timestamps and ZIP timestamps are
normalized. Identical inputs yield identical archive bytes on the same packaging
toolchain; this does not claim bit-for-bit Rust builds across toolchain versions
or hosts. BUILD-INFO.json records the commit, target, features and rustc version.

`SonicBoom --smoke-test` is a diagnostic mode: use an isolated
`SONICBOOM_CONFIG` directory, loopback bind, and temporary working directory.
It starts the real server without the tray/browser or model downloads,
initializes native ORT, serves HTTP for 15 seconds and exits gracefully. It
rejects non-loopback configuration. This is not model inference validation.
The packaging probe enforces startup/health and shutdown deadlines and kills
any remaining child on failure. It never uses your personal config or models.

## Troubleshooting

* LNK2038 / LNK4098 / LNK1319 mentioning `MT_StaticRelease` versus
  `MD_DynamicRelease`: verify `cargo tree --locked --features gui -i esaxx-rs`
  points at `vendor/esaxx-rs`. Both tokenizers versions use the same patch.
  Delete stale target outputs or use a fresh target directory. Do not set
  `KNF_STATIC_CRT=1`, `+crt-static`, `/NODEFAULTLIB` or suppress the mismatch.
  See `vendor/esaxx-rs/README.sonicboom.md` for upstream provenance and removal
  criteria. Native Windows test and release links are the regression checks.
* Missing DLL / 0xc0000135: install the x64 MSVC redistributable, extract the
  whole archive, inspect the printed PE imports, and keep supplied DLLs beside
  SonicBoom.exe. Do not copy arbitrary DLLs from another machine.
* Missing `.so`: run `ldd ./SonicBoom` and on each bundled SO, install the
  documented OS packages, and use the `./sonicboom` launcher. Packaging/smoke
  failures must be fixed before publishing; never mask `not found` output.
* Model-dependent failure: inspect model trust/revision, disk/network access,
  provider enablement and logs separately. A healthy model-free server does
  not prove successful speech synthesis. Exercise every provider with
  real models and reference audio before claiming full inference validation.

## Rollback

Mark a faulty published release as prerelease or remove its public release
entry after recording the incident. Leave its Git tag immutable. Recommend
installation of the previous verified version and restore user data from a
backup if a migration occurred. Revert the change through a PR and publish a
new tag after validation. Never replace existing assets silently: checksums and
consumer downloads must continue to identify the same bytes.

### Native and container preflight

Run `python3 scripts/ci/preflight.py`. It checks the local path manifests and
each Docker builder's manifest inputs and feature names. CI runs it before
compilation on Linux and Windows.
Official release binaries (exactly `sonicboom-windows-x86_64.zip` and
`sonicboom-linux-x86_64.tar.gz`) include all providers, built with
`cargo build --release --locked --features gui` (the defaults already carry
the `webgpu` execution provider). No Vulkan SDK is needed to build; end users
need only the system Vulkan loader/driver. ONNX Runtime opens the loader at
runtime, so the executable starts with no usable Vulkan device and falls back
to the CPU. ONNX Runtime's `cuda` feature selects a separate stack.
`BUILD-INFO.json` reports the actual `features` queried from the built
executable (`--build-info`); packaging, smoke and publish verification reject
metadata that lacks the release features. Hosted CI proves compilation only;
real GPU inference needs hardware validation.

Docker's CPU and playback targets select their features explicitly. The CUDA
target selects ONNX Runtime CUDA. The locked
ort-sys rc.13 distribution ships CUDA 13 only, so the CUDA images use matching
CUDA 13/cuDNN runtime and development bases; a CUDA 13 compatible host driver
is required. CPU images retain their independent SDK-free build. The CUDA build
needs no GPU during compilation. At runtime use NVIDIA Container Toolkit (`docker run --gpus all`):
`libcuda.so.1` must come from the host driver. Image checks permit only that
specific driver library to be absent and never ship linker stubs. Hardware
inference still requires a supported GPU and installed models.

`TOKEN_STORE_PATH=/app/data/tokens.json` in Docker/Compose is a storage path,
not an access token. Keep it writable by the non-root `sonicboom` user and mount
persistent data as needed. Docker's name-based secret warning does not indicate
embedded credentials here; do not place access tokens in build arguments or
image environment defaults. Model weights, credential files, nested Git data
and local build caches are excluded from the allowlisted build context.

### CI structure

Push/pull-request CI (`.github/workflows/ci.yml`) runs formatting, Clippy, tests,
the security-regression manifest guard, the feature matrix and Linux and Windows
GUI packaging. It builds no Docker images and runs no CUDA checks: the Docker
CPU/playback jobs, the Dockerfile structure checks, the static CUDA check and the
`cuda-full.yml` workflow were removed from Actions. `Dockerfile`, `docker-compose.yml`
and `scripts/ci/cuda_structure.py` remain for local use and are not validated in CI.

Use `python3 scripts/ci/container_smoke.py sonicboom:cpu` (or
`sonicboom:playback`) to check real HTTP health and graceful shutdown without
model downloads. The production images set `BIND=0.0.0.0`, so the probe starts
the container with an explicit `BIND=127.0.0.1` override; the application
deliberately refuses `--smoke-test` on a non-loopback listener.

