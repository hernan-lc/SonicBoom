//! NeMo sentencepiece-style vocabulary (`vocab.txt`) decoding.
//!
//! The file has one `token id` pair per line (ids are dense, 0..=8192).
//! `▁` (U+2581) is the word-boundary marker; `<...>` entries are special
//! tokens and are never emitted as text. Whitespace cleanup mirrors
//! onnx-asr's `DECODE_SPACE_PATTERN` (`\A\s|\s\B|(\s)\b`): one space before
//! a word, none at the start or before punctuation/end of text.

use super::AsrError;

#[derive(Debug, Clone)]
pub struct Vocab {
    tokens: Vec<String>,
    pub blank: usize,
}

impl Vocab {
    pub fn parse(text: &str) -> Result<Self, AsrError> {
        let mut entries: Vec<(usize, String)> = Vec::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let (token, id) = line
                .rsplit_once(' ')
                .ok_or_else(|| AsrError::Model(format!("invalid vocab.txt line: {line:?}")))?;
            let id: usize = id
                .trim()
                .parse()
                .map_err(|_| AsrError::Model(format!("invalid vocab.txt id in line: {line:?}")))?;
            if entries.iter().any(|(seen, _)| *seen == id) {
                return Err(AsrError::Model(format!("duplicate vocab id {id}")));
            }
            entries.push((id, token.to_string()));
        }
        let size = entries.len().max(1);
        let mut tokens = vec![String::new(); size];
        let mut blank = None;
        for (id, token) in entries {
            if token == "<blk>" {
                blank = Some(id);
            }
            tokens[id] = token;
        }
        Ok(Self {
            tokens,
            blank: blank.ok_or_else(|| AsrError::Model("vocab.txt has no <blk> token".into()))?,
        })
    }

    pub fn size(&self) -> usize {
        self.tokens.len()
    }

    pub fn blank(&self) -> usize {
        self.blank
    }

    /// Decode token ids into text with punctuation and capitalization intact.
    pub fn decode(&self, ids: &[usize]) -> String {
        let mut joined = String::new();
        for &id in ids {
            let Some(token) = self.tokens.get(id) else {
                continue;
            };
            if token.starts_with('<') && token.ends_with('>') {
                continue;
            }
            for ch in token.chars() {
                joined.push(if ch == '\u{2581}' { ' ' } else { ch });
            }
        }
        collapse_spaces(&joined)
    }
}

/// Keep a single space only when it precedes a word character.
fn collapse_spaces(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if !ch.is_whitespace() {
            out.push(ch);
            continue;
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if !out.is_empty()
            && chars
                .peek()
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            out.push(' ');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> Vocab {
        Vocab::parse("<unk> 0\n▁hello 1\n▁world 2\n, 3\n▁ 4\n<blk> 5\n").expect("vocab")
    }

    #[test]
    fn parses_ids_and_finds_blank() {
        let v = vocab();
        assert_eq!(v.size(), 6);
        assert_eq!(v.blank(), 5);
    }

    #[test]
    fn joins_word_pieces_and_hides_special_tokens() {
        let v = vocab();
        assert_eq!(v.decode(&[1, 2, 3]), "hello world,");
        assert_eq!(v.decode(&[5, 1, 5, 2]), "hello world");
        assert_eq!(v.decode(&[1, 4, 2]), "hello world");
    }

    #[test]
    fn rejects_malformed_lines_and_missing_blank() {
        assert!(Vocab::parse("no-id\n<blk> 0\n").is_err());
        assert!(Vocab::parse("a 0\na 0\n<blk> 1\n").is_err());
        assert!(Vocab::parse("a 0\n").is_err());
    }
}
