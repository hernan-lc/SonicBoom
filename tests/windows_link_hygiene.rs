//! Windows native-link hygiene guards.
//!
//! These are source/configuration checks, so they run on every platform and
//! never invoke the linker. They lock in two rules that the Windows GUI job
//! depends on:
//!
//!   * the MSVC STL is supplied by the dynamically linked CRT — Rust must not
//!     redefine its `__std_*` vectorized algorithms (LNK2005/LNK1169);
//!   * native C++ dependencies and CI keep the dynamic CRT, without linker
//!     suppressions such as `/FORCE:MULTIPLE` (LNK2038/LNK4098).
use std::path::{Path, PathBuf};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Recursively collect `.rs` files under a repository-relative directory.
fn rust_sources(relative: &str) -> Vec<PathBuf> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    visit(&root().join(relative), &mut out);
    out
}

#[test]
fn no_rust_definitions_of_msvc_stl_symbols() {
    // msvcprt.lib(vector_algorithms.obj) exports these in the windows-2025
    // toolchain. Defining them in Rust duplicates the symbols at link time.
    let mut offenders = Vec::new();
    for path in rust_sources("src") {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for (line_number, line) in source.lines().enumerate() {
            let names_symbol = line.contains("__std_");
            let exports_symbol = line.contains("no_mangle") || line.contains("unsafe(no_mangle)");
            if names_symbol && exports_symbol {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.strip_prefix(root()).unwrap_or(&path).display(),
                    line_number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "Rust must not export MSVC STL symbols (LNK2005/LNK1169); remove the shim:\n{}",
        offenders.join("\n")
    );

    let old_shim = root().join("src/msvc_stl_compat.rs");
    assert!(
        !old_shim.exists(),
        "src/msvc_stl_compat.rs duplicates msvcprt.lib symbols and must stay removed"
    );
}

#[test]
fn native_cpp_builds_use_dynamic_crt() {
    // esaxx-rs is the only vendored native C++ dependency with a build script
    // here; its /MT -> /MD patch is the documented fix for LNK2038.
    let build = read("vendor/esaxx-rs/build.rs");
    assert!(
        build.contains(".static_crt(false)"),
        "vendor/esaxx-rs must compile with the dynamic CRT (/MD)"
    );
    assert!(
        !build.contains(".static_crt(true)") && !build.contains("\"/MT"),
        "vendor/esaxx-rs must not select the static CRT"
    );

    // The vendored README records the provenance and the /MD rule.
    let notes = read("vendor/esaxx-rs/README.sonicboom.md");
    assert!(
        notes.contains("static_crt(false)") && notes.contains("MSVC"),
        "vendored esaxx-rs CRT configuration must stay documented"
    );

    for workflow in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let config = read(workflow);
        assert!(
            config.contains("KNF_STATIC_CRT: '0'"),
            "{workflow} must keep knf-rs-sys on the dynamic CRT"
        );
        assert!(
            !config.contains("/FORCE:MULTIPLE") && !config.contains("/FORCE "),
            "{workflow} must not suppress duplicate symbols with /FORCE"
        );
    }
}
