# WebGPU / D3D12 runtime notices

Windows release builds link ONNX Runtime's pinned
`x86_64-pc-windows-msvc+webgpu` artifact (`ms@1.28.0`). It ships no
`DirectML.dll`: the WebGPU EP uses Dawn on D3D12 and carries three DLLs, which
the packager verifies byte-for-byte before adding them to the archive:

| DLL | SHA-256 | Upstream |
|---|---|---|
| `webgpu_dawn.dll` | `e501c2a450ec81bf85f34081570ac2242b1e507101bdeb576f792b41f21e34a7` | [Dawn](https://dawn.googlesource.com/dawn/) — BSD-style, `LICENSE` |
| `dxcompiler.dll` | `0e34fed27b131a851ea6118c328d9ab441d7d08a43e043368e31f2e9d529fd45` | [DirectX Shader Compiler](https://github.com/microsoft/DirectXShaderCompiler) — LLVM/NCSA, `LICENSE.TXT` |
| `dxil.dll` | `fda4dde92222ddb8564c3173ce48faa56f7e1c7b46b0b67dada1182ee07a4123` | [DirectX Shader Compiler](https://github.com/microsoft/DirectXShaderCompiler) — LLVM/NCSA, `LICENSE.TXT` |

## Linux

The Linux `webgpu` artifact (`x86_64-unknown-linux-gnu+webgpu`, SHA-256
`68406bc32de516ee8baeaa4c5f2de2bb0031269f19ce8b76d64988c0498b93be`) ships
`libwebgpu_dawn.so` with SHA-256
`e07cc47ed362fbc24d1aa305a39419ef025332b00973581bdbcbd66e7621a44a`.
`ort/copy-dylibs` symlinks it next to the executable and `build.rs` adds an
`$ORIGIN` RUNPATH; the packager dereferences the symlink, verifies the hash and
ships this `LICENSE` as `licenses/webgpu-dawn-LICENSE`.

The license texts are copied unmodified from the upstream projects. Hashes were
computed from the artifact pinned in `vendor/ort-sys/build/download/dist.tsv`
(SHA-256 `7e21d6e157b80e910ef6bc336846ed7958f13b9dbdd0b0369d6225a59aebd825`).
If the ORT artifact changes, update the pin, these hashes and the notices in
the same commit. No Windows DLL execution is validated by this static check.
