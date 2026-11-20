//! Feature-resolution regression guards.
//!
//! These read the manifest as data and never invoke Cargo, so they are cheap
//! and cannot deadlock against the outer `cargo test`. They lock down the
//! rules for the ONNX acceleration features:
//!
//!   * the root `default` set keeps the core providers and never selects more
//!     than one mutually-exclusive ONNX execution provider;
//!   * `cuda`/`rocm`/`coreml` each forward only to the matching `ort` feature.
use std::path::Path;

fn manifest(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Dependency/feature list of `name = [ ... ]` inside a manifest, tolerant of
/// single-line and multi-line arrays. Entries are unquoted and trimmed.
fn feature_list(manifest: &str, name: &str) -> Vec<String> {
    let marker = format!("\n{name} = [");
    let start = manifest
        .find(&marker)
        .unwrap_or_else(|| panic!("feature `{name}` not declared"))
        + marker.len();
    let end = start + manifest[start..].find(']').expect("unterminated array");
    manifest[start..end]
        .split(',')
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

#[test]
fn root_defaults_keep_core_providers_and_one_onnx_execution_provider() {
    let default = feature_list(&manifest("Cargo.toml"), "default");
    for required in [
        "server",
        "web-ui",
        "playback",
        "pocket-tts",
        "reverb-tts",
        "gui",
    ] {
        assert!(
            default.contains(&required.to_string()),
            "`default` unexpectedly dropped `{required}`: {default:?}"
        );
    }
    // `cuda`/`rocm`/`coreml` are mutually-exclusive ONNX execution providers;
    // a default build must never select two at once.
    let onnx_eps: Vec<&str> = ["cuda", "rocm", "coreml"]
        .into_iter()
        .filter(|feature| default.contains(&feature.to_string()))
        .collect();
    assert!(
        onnx_eps.len() <= 1,
        "`default` selects conflicting ONNX execution providers: {onnx_eps:?}"
    );
}

#[test]
fn onnx_gpu_features_enable_only_the_matching_ort_feature() {
    let root = manifest("Cargo.toml");
    for (feature, expected) in [
        ("cuda", "ort/cuda"),
        ("rocm", "ort/rocm"),
        ("coreml", "ort/coreml"),
    ] {
        let deps = feature_list(&root, feature);
        assert!(
            deps.contains(&expected.to_string()),
            "`{feature}` must enable `{expected}`: {deps:?}"
        );
        assert_eq!(
            deps.len(),
            1,
            "`{feature}` is an ONNX-only flag and must not enable other features: {deps:?}"
        );
    }
}
