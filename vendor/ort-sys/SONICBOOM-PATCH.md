# ort-sys 2.0.0-rc.13 Cargo build-directory fix

Vendored from the crates.io `ort-sys` 2.0.0-rc.13 source (MIT / Apache-2.0).
Only `build/dynamic_link.rs` is patched; all other files remain upstream.

Recent Cargo versions changed the build-script output directory from
`<target>/<profile>/build/<crate>-<hash>/out` to
`<target>/<profile>/build/<crate>/<hash>/out`. `copy_dylibs()` counted a fixed
number of path ancestors to find the target directory, so with the new layout
it resolved to `<target>/<profile>/build` and tried to symlink ONNX Runtime
dylibs into non-existent `build/deps` and `build/examples` directories. That
made every build using ort's default `copy-dylibs` feature fail with
`No such file or directory` (or `The system cannot find the path specified`
on Windows) in the `ort-sys` build script.

The patch walks up to the `build` component and takes its parent (with the
old calculation as a fallback), and skips candidate directories that do not
exist. This is the same fix shipped upstream in `ort-sys` 2.0.0-rc.15,
backported here so SonicBoom can stay on the rc.13 API it is pinned to.

No ONNX Runtime linkage or feature behavior changes. Remove this vendored
crate and the `[patch.crates-io]` entry when upgrading to an ort release that
includes the upstream fix.
