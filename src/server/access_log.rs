//! One access-log line per request, at a level that keeps the console readable.
//!
//! The web UI loads assets and polls `/api/info`, `/api/providers` and friends every few
//! seconds, so logging every request at INFO drowned everything else. The level now says
//! how interesting the request was:
//!
//! | outcome | level |
//! |---|---|
//! | 5xx | `ERROR` |
//! | 4xx | `WARN` (a missing favicon or a stale asset hash is `DEBUG`) |
//! | slow (>= 1 s) | `INFO` |
//! | successful `GET`/`HEAD`/`OPTIONS` | `DEBUG` |
//! | successful change (`POST`, `PUT`, `PATCH`, `DELETE`) | `INFO` |
//!
//! Only the path is logged, never the query string (it can carry text or tokens).

use std::time::{Duration, Instant};

use axum::{
    extract::Request, http::Method, http::StatusCode, middleware::Next, response::Response,
};
use tracing::Level;

const TARGET: &str = "SonicBoom::access";
/// Requests at least this slow are always worth an INFO line.
const SLOW: Duration = Duration::from_secs(1);

/// Level for one finished request (see the module table).
pub fn level_for(method: &Method, path: &str, status: StatusCode, elapsed: Duration) -> Level {
    let read_only = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
    if status.is_server_error() {
        Level::ERROR
    } else if status.is_client_error() {
        let browser_noise = read_only && (path == "/favicon.ico" || path.starts_with("/assets/"));
        if browser_noise {
            Level::DEBUG
        } else {
            Level::WARN
        }
    } else if elapsed >= SLOW || !read_only {
        Level::INFO
    } else {
        Level::DEBUG
    }
}

/// `<1ms`, `42ms` or `1.25s`.
pub fn format_latency(elapsed: Duration) -> String {
    if elapsed < Duration::from_millis(1) {
        "<1ms".to_string()
    } else if elapsed < Duration::from_secs(1) {
        format!("{}ms", elapsed.as_millis())
    } else {
        format!("{:.2}s", elapsed.as_secs_f64())
    }
}

macro_rules! emit {
    ($level:expr, $($arg:tt)+) => {
        match $level {
            Level::ERROR => tracing::error!(target: TARGET, $($arg)+),
            Level::WARN => tracing::warn!(target: TARGET, $($arg)+),
            Level::INFO => tracing::info!(target: TARGET, $($arg)+),
            _ => tracing::debug!(target: TARGET, $($arg)+),
        }
    };
}

/// Axum middleware: logs `METHOD /path STATUS latency` once the response headers are ready
/// (streaming bodies such as job events are measured to their first byte).
pub async fn access_log(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    let elapsed = started.elapsed();
    let status = response.status();
    emit!(
        level_for(&method, &path, status, elapsed),
        "{method} {path} {} {}",
        status.as_u16(),
        format_latency(elapsed)
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Duration = Duration::from_millis(3);

    #[test]
    fn successful_reads_are_debug_so_ui_polling_stays_quiet() {
        for path in [
            "/",
            "/assets/index-abc.js",
            "/api/info",
            "/api/providers",
            "/v1/voices",
        ] {
            assert_eq!(
                level_for(&Method::GET, path, StatusCode::OK, FAST),
                Level::DEBUG,
                "{path}"
            );
        }
        assert_eq!(
            level_for(&Method::OPTIONS, "/api/tts", StatusCode::NO_CONTENT, FAST),
            Level::DEBUG
        );
    }

    #[test]
    fn successful_changes_are_info() {
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert_eq!(
                level_for(&method, "/api/tts/jobs", StatusCode::ACCEPTED, FAST),
                Level::INFO
            );
        }
    }

    #[test]
    fn slow_requests_are_info_even_when_they_are_reads() {
        assert_eq!(
            level_for(
                &Method::GET,
                "/api/voices",
                StatusCode::OK,
                Duration::from_millis(1500)
            ),
            Level::INFO
        );
        assert_eq!(
            level_for(
                &Method::GET,
                "/api/voices",
                StatusCode::OK,
                Duration::from_millis(999)
            ),
            Level::DEBUG
        );
    }

    #[test]
    fn failures_are_loud_except_browser_noise() {
        assert_eq!(
            level_for(&Method::POST, "/api/tts", StatusCode::UNAUTHORIZED, FAST),
            Level::WARN
        );
        assert_eq!(
            level_for(&Method::GET, "/api/voices/x", StatusCode::NOT_FOUND, FAST),
            Level::WARN
        );
        assert_eq!(
            level_for(&Method::GET, "/favicon.ico", StatusCode::NOT_FOUND, FAST),
            Level::DEBUG
        );
        assert_eq!(
            level_for(
                &Method::GET,
                "/assets/old-hash.js",
                StatusCode::NOT_FOUND,
                FAST
            ),
            Level::DEBUG
        );
        assert_eq!(
            level_for(&Method::POST, "/assets/x", StatusCode::NOT_FOUND, FAST),
            Level::WARN
        );
        assert_eq!(
            level_for(
                &Method::POST,
                "/api/tts",
                StatusCode::INTERNAL_SERVER_ERROR,
                FAST
            ),
            Level::ERROR
        );
        assert_eq!(
            level_for(&Method::GET, "/", StatusCode::SERVICE_UNAVAILABLE, FAST),
            Level::ERROR
        );
    }

    #[test]
    fn latency_is_compact() {
        assert_eq!(format_latency(Duration::from_micros(400)), "<1ms");
        assert_eq!(format_latency(Duration::from_millis(42)), "42ms");
        assert_eq!(format_latency(Duration::from_millis(1250)), "1.25s");
    }
}
