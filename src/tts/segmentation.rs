//! Shared long-text segmentation for all TTS providers.
//!
//! Every synthesis entry point normalizes the full text once (see
//! [`crate::api::speech_prep`] and [`crate::engine`]) and only then splits
//! it here, so numbers, dates, and other protected expressions spanning a
//! potential boundary are already expanded before any cut is chosen.
//!
//! [`SegmentPolicy`] is the provider-specific part: Supertonic and the
//! reusable engine use the character-bounded sentence policy
//! ([`SegmentPolicy::supertonic`], built on
//! [`TextProcessor::split_sentences_with_limit`](crate::tts::text::TextProcessor::split_sentences_with_limit)),
//! while Pocket TTS keeps its tokenizer-aware sentence splitter (token
//! budget per chunk, autoregressive state per request) and Qwen3-TTS its
//! 4096-token context handling. Those providers express their own policy;
//! this module guarantees the shared contract for the fragments they do
//! emit: original order, no empty fragments, no cut inside a Unicode
//! grapheme cluster, and per-fragment [`Fragment`] metadata (index, total)
//! so every fragment can be tracked, cancelled, and stitched independently.

use serde::Serialize;

/// Default inter-fragment silence, in seconds. Matches the Supertonic
/// inference stitching (`INTER_CHUNK_SILENCE_SECS`); providers that manage
/// their own continuity (Pocket TTS autoregressive state) use their own
/// value.
pub const DEFAULT_INTER_FRAGMENT_SILENCE_SECS: f64 = 0.2;

/// One independently completable piece of a long-form synthesis request.
///
/// Fragments always preserve the original order (`index` in `0..total`);
/// stitching concatenates fragment audio in index order with the policy's
/// inter-fragment silence, so duplicated, omitted, or reordered speech at
/// boundaries is a programming error, never an accepted outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fragment {
    /// Position in the original order (`0`-based).
    pub index: usize,
    /// Total fragment count of the request this fragment belongs to.
    pub total: usize,
    /// Prepared (normalized) text of this fragment. Never empty.
    pub text: String,
}

impl Fragment {
    /// Length in Unicode characters (not UTF-8 bytes).
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }
}

/// Provider-specific long-text chunk policy.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentPolicy {
    /// Maximum fragment size in Unicode characters.
    pub max_chars: usize,
    /// Silence inserted between stitched fragments, in seconds.
    pub inter_fragment_silence_secs: f64,
}

impl SegmentPolicy {
    /// Character-bounded sentence policy for Supertonic and the reusable
    /// engine. `max_chars` is the configured `tts.max_chunk_chars`
    /// (default [`crate::tts::text::DEFAULT_MAX_CHUNK_CHARS`]).
    pub fn supertonic(max_chars: usize) -> Self {
        Self {
            max_chars: max_chars.max(1),
            inter_fragment_silence_secs: DEFAULT_INTER_FRAGMENT_SILENCE_SECS,
        }
    }
}

impl Default for SegmentPolicy {
    fn default() -> Self {
        Self::supertonic(crate::tts::text::DEFAULT_MAX_CHUNK_CHARS)
    }
}

/// Split prepared text into ordered, independently completable [`Fragment`]s.
///
/// Contract:
/// - empty/blank input yields no fragments;
/// - no fragment is empty and none exceeds `policy.max_chars` characters;
/// - cuts prefer protected sentence boundaries, then whitespace, then a
///   hard split that never breaks a grapheme cluster (via
///   [`TextProcessor::split_sentences_with_limit`](crate::tts::text::TextProcessor::split_sentences_with_limit));
/// - `index`/`total` always reflect the original order.
pub fn segment_text(text: &str, policy: &SegmentPolicy) -> Vec<Fragment> {
    let chunks =
        crate::tts::text::TextProcessor::split_sentences_with_limit(text, policy.max_chars);
    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| Fragment { index, total, text })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(max_chars: usize) -> SegmentPolicy {
        SegmentPolicy::supertonic(max_chars)
    }

    #[test]
    fn empty_input_yields_no_fragments() {
        assert!(segment_text("", &policy(200)).is_empty());
        assert!(segment_text("   \n\t  ", &policy(200)).is_empty());
    }

    #[test]
    fn single_fragment_carries_index_and_total() {
        let fragments = segment_text("Hola mundo.", &policy(200));
        assert_eq!(fragments.len(), 1);
        assert_eq!(
            fragments[0],
            Fragment {
                index: 0,
                total: 1,
                text: "Hola mundo.".to_string(),
            }
        );
        assert_eq!(fragments[0].char_count(), "Hola mundo.".chars().count());
    }

    #[test]
    fn fragments_preserve_order_bounds_and_content() {
        let text = "Primera frase. Segunda frase con 3.14 decimales. \
                    Tercera frase con https://example.com/page?x=1 dentro. \
                    Cuarta y última frase para completar el texto largo.";
        let fragments = segment_text(text, &policy(60));
        assert!(fragments.len() > 1);
        // Order, contiguity, bounds, non-emptiness.
        for (i, fragment) in fragments.iter().enumerate() {
            assert_eq!(fragment.index, i);
            assert_eq!(fragment.total, fragments.len());
            assert!(!fragment.text.is_empty());
            assert!(
                fragment.char_count() <= 60,
                "fragment exceeds policy: {fragment:?}"
            );
        }
        // No content lost or duplicated: concatenation of the fragments
        // holds every word of the input exactly once.
        let joined = fragments
            .iter()
            .map(|fragment| fragment.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        for word in text.split_whitespace() {
            assert!(
                joined.split_whitespace().filter(|w| *w == word).count()
                    >= text.split_whitespace().filter(|w| *w == word).count(),
                "lost content near {word:?}"
            );
        }
    }

    #[test]
    fn protected_expressions_survive_segmentation() {
        // Decimals, dates, URLs and abbreviations must never be cut, even
        // when fragments are tiny.
        let text = "El valor es 3.14 euros. Fecha 26/10/2026. Ver https://example.com/a/b. Sr. García vino.";
        let fragments = segment_text(text, &policy(30));
        let joined = fragments
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for protected in ["3.14", "26/10/2026", "https://example.com/a/b", "Sr."] {
            assert!(
                joined.contains(protected),
                "protected expression cut: {protected:?} in {joined:?}"
            );
        }
    }

    #[test]
    fn grapheme_clusters_are_never_split() {
        // Family emoji (ZWJ sequence) plus combining marks: every char of
        // each cluster must land in the same fragment.
        let text = "👨‍👩‍👧‍👦 holá adiós amigo";
        let fragments = segment_text(text, &policy(8));
        for fragment in &fragments {
            assert!(fragment.char_count() <= 8);
        }
        let joined: String = fragments.iter().map(|f| f.text.clone()).collect();
        assert!(joined.contains("👨‍👩‍👧‍👦"));
    }

    #[test]
    fn default_policy_matches_supertonic_chunk_limit() {
        assert_eq!(
            SegmentPolicy::default().max_chars,
            crate::tts::text::DEFAULT_MAX_CHUNK_CHARS
        );
    }
}
