//! Text preparation and target-length estimation, ported from `reverb_tts/text.py`.
//!
//! The upstream package verbalizes numbers, dates and symbols with Verbalis. SonicBoom
//! already normalizes the request text before it reaches a provider
//! ([`crate::tts::normalization`]), so only the final cleanup is applied here.

use unicode_normalization::char::is_combining_mark;

/// Languages the released weights were trained on.
pub const REVERB_LANGUAGES: &[&str] = &["de", "en", "es", "fr", "it", "pl", "pt"];

/// Codec frames per second of audio (Higgs Audio v2 tokenizer).
pub const FRAME_RATE: usize = 25;
/// Estimated target above which a text is spoken in pieces (frames, 30 s).
pub const LONGEST_FRAMES: usize = 30 * FRAME_RATE;
/// Approximate size of each piece of a long text (frames, 15 s).
pub const PIECE_FRAMES: usize = 15 * FRAME_RATE;

/// Text as the model was trained to read it: brackets dropped, single spaces.
pub fn prepare_text(text: &str) -> String {
    let without: String = text
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | '[' | ']' | '{' | '}'))
        .collect();
    without.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Spoken weight of one character (upstream `unicodedata.category` table:
/// separators 0.2, marks 0, punctuation and symbols 0.5, numbers 3.5, rest 1).
fn char_weight(c: char) -> f64 {
    if c.is_whitespace() {
        0.2
    } else if is_combining_mark(c) {
        0.0
    } else if c.is_numeric() {
        3.5
    } else if c.is_alphabetic() || c.is_control() {
        1.0
    } else {
        0.5
    }
}

fn weight(text: &str) -> f64 {
    text.chars().map(char_weight).sum()
}

/// The target's length in codec frames: the text's spoken weight, at the rate
/// the reference speaks its transcript. Short texts get relatively more time.
pub fn estimate_frames(text: &str, reference_text: &str, reference_frames: usize) -> usize {
    let reference_weight = weight(reference_text).max(1e-6);
    let mut frames = weight(text) / (reference_weight / reference_frames.max(1) as f64);
    if frames < 50.0 {
        frames = 50.0 * (frames / 50.0).cbrt();
    }
    (frames as usize).max(1)
}

/// Split on whitespace runs that follow `.`, `!` or `?` (upstream `(?<=[.!?])\s+`).
fn sentences(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut start = 0;
    let mut previous: Option<char> = None;
    let mut k = 0;
    while k < chars.len() {
        let (index, c) = chars[k];
        if c.is_whitespace() && matches!(previous, Some('.' | '!' | '?')) {
            let mut end = k;
            while end < chars.len() && chars[end].1.is_whitespace() {
                end += 1;
            }
            out.push(&text[start..index]);
            start = chars.get(end).map_or(text.len(), |&(i, _)| i);
            previous = None;
            k = end;
        } else {
            previous = Some(c);
            k += 1;
        }
    }
    out.push(&text[start..]);
    out
}

/// The text whole when estimated at most `longest` frames, else cut at sentence
/// ends into pieces of about `piece` frames. A sentence longer than a piece
/// stays whole.
pub fn split_text(
    text: &str,
    reference_text: &str,
    reference_frames: usize,
    longest: usize,
    piece: usize,
) -> Vec<String> {
    if estimate_frames(text, reference_text, reference_frames) <= longest {
        return vec![text.to_string()];
    }
    let mut pieces: Vec<String> = Vec::new();
    for sentence in sentences(text) {
        if let Some(last) = pieces.last_mut() {
            let joined = format!("{last} {sentence}");
            if estimate_frames(&joined, reference_text, reference_frames) <= piece {
                *last = joined;
                continue;
            }
        }
        pieces.push(sentence.to_string());
    }
    pieces
}

/// Prompt text for the tokenizer (upstream `_prompt`).
pub fn prompt(text: &str, reference_text: &str, language: &str) -> String {
    format!(
        "<|denoise|><|lang_start|>{language}<|lang_end|><|instruct_start|>None<|instruct_end|>\
         <|text_start|>{reference_text} {text}<|text_end|>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_are_dropped_and_spaces_collapse() {
        assert_eq!(
            prepare_text("  Hola (mundo)\n [x]  {y}  fin "),
            "Hola mundo x y fin"
        );
    }

    #[test]
    fn weights_follow_the_unicode_category_table() {
        assert!((weight("ab") - 2.0).abs() < 1e-9);
        assert!((weight("a b") - 2.2).abs() < 1e-9);
        assert!((weight("a,") - 1.5).abs() < 1e-9);
        assert!((weight("a1") - 4.5).abs() < 1e-9);
        // "é" decomposed: letter + combining acute (weight 0).
        assert!((weight("e\u{301}") - 1.0).abs() < 1e-9);
        assert!((weight("€") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn short_texts_get_relatively_more_time() {
        // Same speaking rate as the reference: 100 weight over 100 frames.
        let reference = "a".repeat(100);
        assert_eq!(estimate_frames(&"a".repeat(100), &reference, 100), 100);
        let short = estimate_frames(&"a".repeat(10), &reference, 100);
        assert!(short > 10, "{short}");
        assert_eq!(short, (50.0 * (10.0f64 / 50.0).cbrt()) as usize);
        assert_eq!(estimate_frames("", &reference, 100), 1);
    }

    #[test]
    fn sentence_split_matches_the_lookbehind_regex() {
        assert_eq!(
            sentences("Hola. Que tal? Bien"),
            vec!["Hola.", "Que tal?", "Bien"]
        );
        assert_eq!(
            sentences("Uno,  dos. \n Tres!"),
            vec!["Uno,  dos.", "Tres!"]
        );
        assert_eq!(sentences("sin fin"), vec!["sin fin"]);
        assert_eq!(sentences("3.14 es pi"), vec!["3.14 es pi"]);
    }

    #[test]
    fn long_text_is_cut_at_sentence_ends() {
        let reference = "a".repeat(100);
        let sentence = format!("{}.", "b".repeat(300)); // ~301 weight -> ~301 frames
        let text = format!("{sentence} {sentence} {sentence}");
        let pieces = split_text(&text, &reference, 100, 500, 400);
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        assert!(pieces.iter().all(|p| p == &sentence));
        // Short enough: untouched.
        assert_eq!(
            split_text("Hola.", &reference, 100, 500, 400),
            vec!["Hola."]
        );
        // Small sentences are joined up to `piece` frames.
        let small = "Hola mundo. ".repeat(60);
        let pieces = split_text(small.trim(), &reference, 100, 300, 200);
        assert!(pieces.len() > 1);
        assert!(
            pieces
                .iter()
                .all(|p| estimate_frames(p, &reference, 100) <= 200)
        );
    }

    #[test]
    fn prompt_layout_matches_upstream() {
        assert_eq!(
            prompt("Texto.", "Ref.", "es"),
            "<|denoise|><|lang_start|>es<|lang_end|><|instruct_start|>None<|instruct_end|><|text_start|>Ref. Texto.<|text_end|>"
        );
    }
}
