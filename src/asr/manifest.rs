//! Pinned download manifest for the Fluid Speech Pro ONNX graphs.
//!
//! Revision `fa588e9f87f1b9ad0ca2e2d8f7eaf84dc3803581` (HF API, 2026-11-20).
//! Only the `int8` variants without `.clean` are used; sizes and SHA-256
//! digests were computed from the downloaded bytes and match the HF LFS OIDs.
//! `decoder*.onnx` / `joiner*.onnx` are the separate sherpa-onnx graphs and
//! are deliberately not fetched: this engine uses the onnx-asr path
//! (`encoder-model` + `decoder_joint-model`), as in the upstream model card.

use crate::tts::providers::assets::PinnedFile;

pub const REPO: &str = "FluidVoice-Labs/fluid-speech-pro-onnx";
pub const REVISION: &str = "fa588e9f87f1b9ad0ca2e2d8f7eaf84dc3803581";

/// Every file needed at runtime, with exact expected size and digest.
pub const FILES: &[(&str, u64, &str)] = &[
    (
        "config.json",
        98,
        "21565538716b1cb134a096e3df9fa3f27f0470bc09c596d9faa578dc27d945d5",
    ),
    (
        "vocab.txt",
        93939,
        "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
    ),
    (
        "encoder-model.int8.onnx",
        955592,
        "07ad77459bd3bb165c2c53e978ed1dfd64c6e129b18af2ec5ee38343be1d4451",
    ),
    (
        "encoder-model.int8.onnx.data",
        745971712,
        "d2f8bfbb1fa399f207cde85aa3fabee0d67f6730cb31cbf31832c10de69f9c3c",
    ),
    (
        "decoder_joint-model.int8.onnx",
        8586,
        "59e3ff4b22b6629bc5a23e627daff7697ec232d2d4e6de50597c907f4038bd29",
    ),
    (
        "decoder_joint-model.int8.onnx.data",
        53817344,
        "3c7a1e90be06acc16d858b989700cdb51f761c6129a45b0bdc0378f2b40a28a4",
    ),
];

/// Total bytes downloaded on a cold install (801 MB).
pub const TOTAL_BYTES: u64 = 800_847_271;

pub fn pinned() -> Vec<PinnedFile> {
    FILES
        .iter()
        .map(|(path, size, sha256)| PinnedFile {
            path,
            url: PinnedFile::huggingface(REPO, REVISION, path),
            sha256,
            size: *size,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_complete_and_sizes_add_up() {
        assert_eq!(FILES.len(), 6);
        assert_eq!(FILES.iter().map(|f| f.1).sum::<u64>(), TOTAL_BYTES);
        assert!(FILES.iter().all(|(_, _, hash)| hash.len() == 64));
        let files = pinned();
        assert_eq!(files.len(), 6);
        assert!(files.iter().all(|f| f.url.contains(REVISION) && !f.path.contains("clean")));
    }
}
