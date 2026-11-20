//! Quality detection mode: whether generated audio is analyzed at all.
//!
//! Detection is always non-destructive: it reports findings and never trims,
//! filters or regenerates audio.

/// Detection mode for generated audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetectionMode {
    /// No detection.
    Off,
    /// Detect and report suspicious results; never modify audio. Default.
    #[default]
    Report,
}

impl DetectionMode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Some(Self::Off),
            "report" => Some(Self::Report),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Report => "report",
        }
    }

    /// Whether any detection work runs at all.
    pub fn detects(self) -> bool {
        !matches!(self, Self::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_parse_and_stringify_round_trip() {
        for mode in [DetectionMode::Off, DetectionMode::Report] {
            assert_eq!(DetectionMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(DetectionMode::parse("REPORT"), Some(DetectionMode::Report));
        assert_eq!(DetectionMode::parse("none"), Some(DetectionMode::Off));
        assert_eq!(DetectionMode::parse("retry"), None);
        assert_eq!(DetectionMode::parse("banana"), None);
        assert!(!DetectionMode::Off.detects());
        assert!(DetectionMode::Report.detects());
    }
}
