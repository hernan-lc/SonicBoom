//! The one place where HTTP synthesis routes prepare user text.
//!
//! Every synthesis path (`/api/tts`, `/api/tts/play`, `/api/tts/jobs`,
//! `/v1/audio/speech`, voice previews) turns its text into a
//! [`SpeechText`] through [`prepare_speech`] exactly once, before any
//! provider or inference call. The core rules live in
//! [`sonicboom::engine::prepare_speech_text`], which the reusable engine
//! uses too, so library and server semantics cannot drift apart.
//!
//! Errors never contain user text: they map [`NormalizeError`] variants to
//! fixed `400` messages.

use sonicboom::tts::normalization::{
    NormalizationMode, NormalizeError, NormalizeOptions, ReadingStyle,
};

pub use sonicboom::tts::normalization::SpeechText;

use crate::{config::AppConfig, error::AppError};

/// Effective normalization mode: the requested or configured mode, unless
/// the global switch is disabled. Invalid request values are still rejected.
pub(crate) fn resolve_mode(
    config: &AppConfig,
    requested: Option<&str>,
) -> Result<NormalizationMode, AppError> {
    let mode = match requested.map(str::trim).filter(|raw| !raw.is_empty()) {
        None => Ok(config.tts.normalization.effective_mode()),
        Some(raw) => NormalizationMode::parse(raw).ok_or_else(|| {
            AppError::BadRequest("normalization must be one of: auto, off".to_string())
        }),
    }?;
    Ok(if config.tts.normalization.enabled {
        mode
    } else {
        NormalizationMode::Off
    })
}

/// Effective reading style: the requested style, otherwise the configured
/// default. Invalid request values are rejected.
pub(crate) fn resolve_reading_style(
    config: &AppConfig,
    requested: Option<&str>,
) -> Result<ReadingStyle, AppError> {
    match requested.map(str::trim).filter(|raw| !raw.is_empty()) {
        None => Ok(config.tts.normalization.effective_reading_style()),
        Some(raw) => ReadingStyle::parse(raw).ok_or_else(|| {
            AppError::BadRequest(
                "reading_style must be one of: precise, natural, digits".to_string(),
            )
        }),
    }
}

/// Fixed, content-free `400` message for a normalization failure.
pub(crate) fn normalize_error(error: NormalizeError) -> AppError {
    AppError::BadRequest(format!("text normalization: {error}"))
}

/// Resolve full normalizer options for `language` with the request's
/// `normalization`/`locale` values and the configured defaults. Shared by
/// [`prepare_speech`] and the `POST /api/tts/normalize` preview.
pub(crate) fn resolve_options(
    config: &AppConfig,
    normalization: Option<&str>,
    locale: Option<&str>,
    reading_style: Option<&str>,
    language: &str,
) -> Result<NormalizeOptions, AppError> {
    let mode = resolve_mode(config, normalization)?;
    let style = resolve_reading_style(config, reading_style)?;
    let settings = &config.tts.normalization;
    sonicboom::engine::resolve_speech_options_with_style(
        mode,
        language,
        sonicboom::engine::requested_locale(
            language,
            locale,
            Some(settings.default_locale.as_str()),
        ),
        settings.limits(config.tts.max_text_length),
        settings.report_ambiguities,
        style,
    )
    .map_err(normalize_error)
}

/// Prepare user text for synthesis. `text` is exactly what the legacy code
/// path would have handed to the provider (legacy trimming already
/// applied), so `off` reproduces the pre-normalization input byte for byte.
///
/// - mode: `normalization` when present (`400` if invalid), else the
///   configured effective mode;
/// - locale: `locale` when present (`auto` = language default), else the
///   configured `default_locale` when it belongs to `language`, else the
///   language default;
/// - limits: `[tts.normalization]` with `tts.max_text_length` as input bound.
pub(crate) fn prepare_speech(
    config: &AppConfig,
    normalization: Option<&str>,
    locale: Option<&str>,
    reading_style: Option<&str>,
    language: &str,
    text: &str,
) -> Result<SpeechText, AppError> {
    let mode = resolve_mode(config, normalization)?;
    let style = resolve_reading_style(config, reading_style)?;
    let settings = &config.tts.normalization;
    sonicboom::engine::prepare_speech_text_with_style(
        text,
        language,
        mode,
        sonicboom::engine::requested_locale(
            language,
            locale,
            Some(settings.default_locale.as_str()),
        ),
        settings.limits(config.tts.max_text_length),
        style,
    )
    .map_err(normalize_error)
}

/// The optional `normalization` / `locale` request fields of one synthesis
/// request, unparsed. Validated by [`SpeechOptions::prepare`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SpeechOptions {
    pub normalization: Option<String>,
    pub locale: Option<String>,
    pub reading_style: Option<String>,
}

impl SpeechOptions {
    /// [`prepare_speech`] with these options.
    pub(crate) fn prepare(
        &self,
        config: &AppConfig,
        language: &str,
        text: &str,
    ) -> Result<SpeechText, AppError> {
        prepare_speech(
            config,
            self.normalization.as_deref(),
            self.locale.as_deref(),
            self.reading_style.as_deref(),
            language,
            text,
        )
    }
}

/// Provider request carrying already prepared text (and its identity).
pub(crate) fn provider_request(
    model: impl Into<String>,
    voice: impl Into<String>,
    speech: SpeechText,
    language: impl Into<String>,
) -> sonicboom::tts::providers::ProviderRequest {
    sonicboom::engine::provider_request_for(model, voice, speech, language)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AppConfig {
        crate::api::tests::test_config()
    }

    #[test]
    fn mode_defaults_to_config_and_rejects_unknown_values() {
        let mut config = config();
        assert_eq!(
            resolve_mode(&config, None).unwrap(),
            NormalizationMode::Auto
        );
        assert_eq!(
            resolve_mode(&config, Some("")).unwrap(),
            NormalizationMode::Auto
        );
        assert_eq!(
            resolve_mode(&config, Some(" OFF ")).unwrap(),
            NormalizationMode::Off
        );
        assert!(matches!(
            resolve_mode(&config, Some("strict")),
            Err(AppError::BadRequest(_))
        ));
        config.tts.normalization.enabled = false;
        assert_eq!(resolve_mode(&config, None).unwrap(), NormalizationMode::Off);
        assert_eq!(
            resolve_mode(&config, Some("auto")).unwrap(),
            NormalizationMode::Off
        );
    }

    #[test]
    fn off_is_byte_identical_and_auto_normalizes() {
        let config = config();
        let text = "Pago el 26/10/2026, ¿sí?";
        let off = prepare_speech(&config, Some("off"), None, None, "es", text).unwrap();
        assert_eq!(off.as_str(), text);
        assert!(!off.changed());
        let auto = prepare_speech(&config, None, None, None, "es", text).unwrap();
        assert!(auto.changed());
        assert!(auto.as_str().contains("veintiséis de octubre"));
        assert_eq!(auto.identity(), "tn-v2.1|auto|es|es-ES|precise");
    }

    #[test]
    fn locale_resolution_and_errors_are_content_free() {
        let mut config = config();
        let secret = "secreto 123";
        let peru = prepare_speech(&config, None, Some("es-pe"), None, "es", secret).unwrap();
        assert_eq!(peru.identity(), "tn-v2.1|auto|es|es-PE|precise");
        for (locale, language) in [("es-AR", "es"), ("en-US", "es"), ("not a tag", "es")] {
            let error =
                prepare_speech(&config, None, Some(locale), None, language, secret).unwrap_err();
            assert!(matches!(error, AppError::BadRequest(_)));
            assert!(!error.to_string().contains("secreto"));
        }
        // Configured default applies only to its own language.
        config.tts.normalization.default_locale = "es-PE".into();
        let es = prepare_speech(&config, None, None, None, "es", secret).unwrap();
        assert_eq!(es.identity(), "tn-v2.1|auto|es|es-PE|precise");
        let en = prepare_speech(&config, None, None, None, "en", "1 cat").unwrap();
        assert_eq!(en.identity(), "tn-v2.1|auto|en|en-US|precise");
        // `auto` in the request selects the language default, not the config.
        let auto = prepare_speech(&config, None, Some("auto"), None, "es", secret).unwrap();
        assert_eq!(auto.identity(), "tn-v2.1|auto|es|es-ES|precise");
    }

    #[test]
    fn reading_style_uses_config_default_and_request_override() {
        let mut config = config();
        let text = "12:30";
        let precise = prepare_speech(&config, None, None, None, "es", text).unwrap();
        assert_eq!(precise.as_str(), "doce treinta");
        assert_eq!(precise.identity(), "tn-v2.1|auto|es|es-ES|precise");
        let natural = prepare_speech(&config, None, None, Some("natural"), "es", text).unwrap();
        assert_eq!(natural.as_str(), "doce y media");
        assert!(natural.identity().ends_with("|natural"));
        config.tts.normalization.reading_style = "digits".into();
        let digits = prepare_speech(&config, None, None, None, "es", text).unwrap();
        assert_eq!(digits.as_str(), "uno dos tres cero");
        assert!(digits.identity().ends_with("|digits"));
        // An invalid request value is rejected without echoing text.
        let error = prepare_speech(&config, None, None, Some("loud"), "es", text).unwrap_err();
        assert!(matches!(error, AppError::BadRequest(_)));
    }

    #[test]
    fn input_and_output_budgets_are_enforced() {
        let mut config = config();
        config.tts.max_text_length = 5;
        assert!(matches!(
            prepare_speech(&config, Some("off"), None, None, "en", "too long"),
            Err(AppError::BadRequest(_))
        ));
        config.tts.max_text_length = 100;
        config.tts.normalization.max_normalized_chars = 4;
        assert!(matches!(
            prepare_speech(&config, None, None, None, "en", "1000000"),
            Err(AppError::BadRequest(_))
        ));
    }
}
