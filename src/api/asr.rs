//! HTTP endpoints for local speech-to-text (Fluid Speech Pro).
//!
//! Multipart is the wire format for both `/api/asr` and the OpenAI-compatible
//! `/v1/audio/transcriptions`. WAV aliases and actual WAV decoding are both
//! validated; errors use the standard `AppError` envelope.

use axum::{
    Json,
    extract::{FromRequest, Multipart, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::AppState;
use crate::error::AppError;
use sonicboom::asr::{AsrError, AsrStatus, SUPPORTED_LANGUAGES};
use crate::auth::AuthenticatedToken;

/// Multipart envelope: 30 s of 16-bit mono WAV is ~1 MB, so 8 MiB leaves room
/// for 32-bit float input and form fields.
pub const ASR_BODY_LIMIT: usize = 8 * 1024 * 1024;

const IGNORED_OPENAI_FIELDS: &[&str] = &[
    "prompt",
    "timestamp_granularities[]",
    "timestamp_granularities",
    "user",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseFormat {
    Json,
    Text,
    VerboseJson,
}

struct AsrRequest {
    wav: Vec<u8>,
    language: Option<String>,
    format: ResponseFormat,
}

/// Check only that the model id is one we serve; OpenAI clients commonly
/// send `whisper-1`, which is accepted as an alias and reported truthfully.
fn validate_model(value: Option<&str>) -> Result<(), AppError> {
    match value.map(str::trim) {
        None | Some("") | Some("fluid-speech-pro") | Some("whisper-1")
        | Some("whisper-large-v3") | Some("whisper-large-v3-turbo") => Ok(()),
        Some(other) => Err(AppError::BadRequest(format!(
            "unsupported ASR model '{other}' (expected fluid-speech-pro)"
        ))),
    }
}

fn language_code(value: &str) -> String {
    let base = value.trim().to_ascii_lowercase();
    base.split('-').next().unwrap_or("auto").to_string()
}

fn validate_language(value: Option<String>) -> Result<Option<String>, AppError> {
    let Some(value) = value else { return Ok(None) };
    let code = language_code(&value);
    if code == "auto" {
        return Ok(None);
    }
    if SUPPORTED_LANGUAGES.contains(&code.as_str()) {
        Ok(Some(code))
    } else {
        Err(AppError::BadRequest(format!(
            "unsupported language '{value}' (supported: {})",
            SUPPORTED_LANGUAGES.join(", ")
        )))
    }
}

fn response_format(value: Option<String>) -> Result<ResponseFormat, AppError> {
    match value.as_deref().map(str::trim) {
        None | Some("") | Some("json") => Ok(ResponseFormat::Json),
        Some("text") => Ok(ResponseFormat::Text),
        Some("verbose_json") => Ok(ResponseFormat::VerboseJson),
        Some(other) => Err(AppError::BadRequest(format!(
            "unsupported response_format '{other}' (expected json, text or verbose_json)"
        ))),
    }
}

fn asr_error(error: AsrError) -> AppError {
    match error {
        AsrError::TooLong { .. } | AsrError::Decode(_) | AsrError::Io(_) => {
            AppError::BadRequest(error.to_string())
        }
        AsrError::Busy => AppError::TooManyRequests(error.to_string()),
        AsrError::Model(_) | AsrError::Failed(_) => AppError::ServiceUnavailable(
            "Fluid Speech Pro is not ready. Check GET /api/asr/status.".into(),
        ),
    }
}

async fn parse_request(
    state: &AppState,
    request: Request,
    openai: bool,
) -> Result<AsrRequest, AppError> {
    let (parts, body) = request.into_parts();
    let header_value = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = axum::body::to_bytes(body, ASR_BODY_LIMIT)
        .await
        .map_err(|_| AppError::PayloadTooLarge("ASR upload exceeds 8 MiB".into()))?;
    if !header_value.starts_with("multipart/form-data") {
        if !super::is_wav_media_type(&header_value) {
            return Err(AppError::UnsupportedMediaType(
                "expected multipart/form-data with a WAV file, or Content-Type: audio/wav".into(),
            ));
        }
        let wav = bytes.to_vec();
        sonicboom::asr::validate_wav(&wav).map_err(asr_error)?;
        return Ok(AsrRequest {
            wav,
            language: None,
            format: ResponseFormat::Json,
        });
    }

    let request = Request::from_parts(parts, axum::body::Body::from(bytes));
    let mut form = Multipart::from_request(request, state)
        .await
        .map_err(|_| AppError::BadRequest("invalid multipart upload".into()))?;
    let mut wav = None;
    let mut language = None;
    let mut format = None;
    let mut model = None;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("invalid multipart field".into()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" && wav.is_none() {
            if let Some(kind) = field.content_type()
                && !super::is_wav_media_type(kind)
                && !kind.eq_ignore_ascii_case("application/octet-stream")
            {
                return Err(AppError::UnsupportedMediaType(
                    "the file part must contain WAV audio".into(),
                ));
            }
            wav = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| AppError::BadRequest("invalid WAV part".into()))?,
            );
            continue;
        }
        let value = field
            .bytes()
            .await
            .map_err(|_| AppError::BadRequest("invalid multipart data".into()))?;
        let text = String::from_utf8_lossy(&value).into_owned();
        match name.as_str() {
            "language" if language.is_none() => language = Some(text),
            "model" if openai && model.is_none() => model = Some(text),
            "response_format" if openai && format.is_none() => format = Some(text),
            "temperature" if openai => {
                let temperature: f32 = text.trim().parse().map_err(|_| {
                    AppError::BadRequest("temperature must be 0 (greedy decoding only)".into())
                })?;
                if temperature != 0.0 {
                    return Err(AppError::BadRequest(
                        "temperature other than 0 is not supported (greedy decoding only)".into(),
                    ));
                }
            }
            "stream" if openai => {
                if text.trim().eq_ignore_ascii_case("true") {
                    return Err(AppError::BadRequest("streaming transcription is not supported".into()));
                }
            }
            "timestamp_granularities[]" | "timestamp_granularities" if openai => {
                if text.to_ascii_lowercase().contains("word")
                    || text.to_ascii_lowercase().contains("segment")
                {
                    return Err(AppError::BadRequest(
                        "word/segment timestamps are not supported".into(),
                    ));
                }
            }
            _ if IGNORED_OPENAI_FIELDS.contains(&name.as_str()) => {}
            _ => {
                return Err(AppError::BadRequest(
                    "unexpected or duplicate multipart field".into(),
                ));
            }
        }
    }
    if openai {
        validate_model(model.as_deref())?;
    }
    let wav = wav
        .ok_or_else(|| AppError::BadRequest("file is required".into()))?
        .to_vec();
    // Reject malformed/oversized audio before a 800 MB model is fetched.
    sonicboom::asr::validate_wav(&wav).map_err(asr_error)?;
    Ok(AsrRequest {
        wav,
        language: validate_language(language)?,
        format: response_format(format)?,
    })
}

fn render(result: sonicboom::asr::Transcription, format: ResponseFormat) -> Response {
    match format {
        ResponseFormat::Text => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            result.text,
        )
            .into_response(),
        ResponseFormat::VerboseJson => Json(serde_json::json!({
            "text": result.text,
            "language": result.language,
            "duration": result.duration,
            "model": sonicboom::asr::MODEL_ID,
            "task": "transcribe",
            "segments": [],
        }))
        .into_response(),
        ResponseFormat::Json => Json(serde_json::json!({
            "text": result.text,
            "language": result.language,
            "duration": result.duration,
            "model": sonicboom::asr::MODEL_ID,
        }))
        .into_response(),
    }
}

async fn transcribe(
    state: AppState,
    request: Request,
    openai: bool,
) -> Result<Response, AppError> {
    let parsed = parse_request(&state, request, openai).await?;
    let result = sonicboom::asr::manager()
        .transcribe(parsed.wav, parsed.language)
        .await
        .map_err(asr_error)?;
    Ok(render(result, parsed.format))
}

/// `POST /api/asr`: multipart `file` (WAV) plus optional `language`.
pub async fn post_asr(
    token: AuthenticatedToken,
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, AppError> {
    super::tts::check_rate_limit(&state, &token)?;
    let inner = state.clone();
    super::jobs::with_admission(&state, transcribe(inner, request, false)).await
}

/// `POST /v1/audio/transcriptions`: OpenAI-compatible fields. `model`,
/// `language`, `response_format` are honored; harmless extras are ignored and
/// incompatible ones return 400.
pub async fn post_transcription(
    token: AuthenticatedToken,
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, AppError> {
    super::tts::check_rate_limit(&state, &token)?;
    let inner = state.clone();
    super::jobs::with_admission(&state, transcribe(inner, request, true)).await
}

/// `GET /api/asr/status`: lifecycle mirroring `/api/providers/{id}/status`.
pub async fn get_status(_token: AuthenticatedToken) -> Json<AsrStatus> {
    Json(sonicboom::asr::manager().status().await)
}

/// `POST /api/asr/prepare`: activation hook for the Engines view / warm-up.
pub async fn post_prepare(
    token: AuthenticatedToken,
    State(state): State<AppState>,
) -> Result<Json<AsrStatus>, AppError> {
    super::tts::check_rate_limit(&state, &token)?;
    super::jobs::with_admission(&state, async {
        sonicboom::asr::manager()
            .prepare()
            .await
            .map(Json)
            .map_err(asr_error)
    })
    .await
}
