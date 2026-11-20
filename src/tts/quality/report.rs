//! Structured quality report shared by the worker, the job status, the API,
//! and the offline audit harness.

use serde::{Deserialize, Serialize};

use super::audio_health::AudioHealth;
use super::reference_echo::EchoAnalysis;

/// Severity of a single quality finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One quality finding. `confidence` stays `None` until a detector is
/// calibrated — never invent a probability.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QualityIssue {
    pub kind: String,
    pub severity: Severity,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

impl QualityIssue {
    pub fn new(kind: impl Into<String>, severity: Severity, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            severity,
            detail: detail.into(),
            confidence: None,
        }
    }

    pub fn warning(kind: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(kind, Severity::Warning, detail)
    }

    pub fn info(kind: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(kind, Severity::Info, detail)
    }

    pub fn error(kind: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(kind, Severity::Error, detail)
    }
}

/// Overall disposition of a generation's quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityStatus {
    Ok,
    Warning,
    Failed,
}

/// Complete quality report. Serialized into job diagnostics; never carries
/// reference-transcript text or raw audio.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QualityReport {
    pub quality_status: QualityStatus,
    pub issues: Vec<QualityIssue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<EchoAnalysis>,
    /// Detection mode that produced this report.
    pub mode: String,
}

impl QualityReport {
    pub fn ok() -> Self {
        Self {
            quality_status: QualityStatus::Ok,
            issues: Vec::new(),
            audio: None,
            echo: None,
            mode: "off".to_string(),
        }
    }

    /// Assemble a report and derive the status from its findings.
    pub fn from_parts(
        mode: &str,
        audio: Option<AudioHealth>,
        echo: Option<EchoAnalysis>,
        mut issues: Vec<QualityIssue>,
    ) -> Self {
        if echo.as_ref().is_some_and(|e| e.detected) {
            issues.push(QualityIssue::warning(
                "possible_reference_echo",
                "generated speech begins with reference-transcript words absent from the target text",
            ));
        }
        let status = if issues.iter().any(|i| i.severity == Severity::Error) {
            QualityStatus::Failed
        } else if issues.iter().any(|i| i.severity == Severity::Warning) {
            QualityStatus::Warning
        } else {
            QualityStatus::Ok
        };
        Self {
            quality_status: status,
            issues,
            audio,
            echo,
            mode: mode.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::quality::audio_health;

    #[test]
    fn status_derives_from_findings() {
        let ok = QualityReport::from_parts("report", None, None, vec![]);
        assert_eq!(ok.quality_status, QualityStatus::Ok);

        let warned =
            QualityReport::from_parts("report", None, None, vec![QualityIssue::warning("x", "y")]);
        assert_eq!(warned.quality_status, QualityStatus::Warning);

        let failed =
            QualityReport::from_parts("report", None, None, vec![QualityIssue::error("x", "y")]);
        assert_eq!(failed.quality_status, QualityStatus::Failed);
    }

    #[test]
    fn confirmed_echo_raises_warning() {
        let echo = crate::tts::quality::reference_echo::analyze(
            "the quick brown fox",
            "hello there",
            Some("quick brown fox hello there"),
        );
        let report = QualityReport::from_parts("report", None, Some(echo), vec![]);
        assert_eq!(report.quality_status, QualityStatus::Warning);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.kind == "possible_reference_echo")
        );
    }

    #[test]
    fn report_serializes_without_nulls_or_content() {
        let health = audio_health::analyze(&[0.1, -0.1, 0.2], 24_000);
        let report = QualityReport::from_parts("report", Some(health), None, vec![]);
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["quality_status"], "ok");
        assert_eq!(value["mode"], "report");
        assert!(value.get("echo").is_none());
        assert!(value["audio"].get("peak").is_some());
    }
}
