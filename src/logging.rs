//! Logging for SonicBoom.
//!
//! * `LOG_LEVEL` (`logging.level`) applies to SonicBoom's own targets and to
//!   the `tower_http` access log only. Every other crate stays at `warn`, so
//!   `info` no longer streams ONNX Runtime / WebGPU internals (one line per GPU
//!   dispatch, ~50k lines per day before this change).
//! * `logging.filter` / `RUST_LOG` replace those default directives entirely,
//!   e.g. `RUST_LOG=warn,sonicboom=debug,ort=info` for deep debugging.
//! * ONNX Runtime severity is set at the source by `onnx_runtime` (see
//!   `SONICBOOM_ORT_LOG`), so messages below it never reach this pipeline.
//! * The level is hot-reloadable through [`set_level`].

use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter, fmt, fmt::format::Writer, layer::SubscriberExt, reload, util::SubscriberInitExt,
};

/// Local wall-clock time with milliseconds, e.g. `14:19:50.123`.
/// (Replaces a UTC-only HH:MM:SS timer that disagreed with the local clock.)
struct LocalTime;

impl fmt::time::FormatTime for LocalTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%H:%M:%S%.3f"))
    }
}

/// Keeps the file writer's worker thread alive for the process lifetime.
static LOG_GUARDS: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

/// Type-erased level setter. The reload handle is generic over the (unnameable)
/// layered subscriber type, so it is captured in a closure and erased.
type LevelSetter = dyn Fn(&str) -> Result<(), String> + Send + Sync;

static LEVEL_HANDLE: OnceLock<Box<LevelSetter>> = OnceLock::new();

/// Expands a plain level (`debug`) into directives that scope it to SonicBoom
/// and its access log. Anything else is already an `EnvFilter` directive list
/// (`logging.filter`, `RUST_LOG`) and is used unchanged.
///
/// The binary crate logs under `SonicBoom` and the library under `sonicboom`,
/// so both targets are listed.
fn directives(spec: &str) -> String {
    let spec = spec.trim();
    let lower = spec.to_ascii_lowercase();
    match lower.as_str() {
        "trace" | "debug" | "info" | "warn" | "error" | "off" => {
            format!("warn,SonicBoom={lower},sonicboom={lower},tower_http={lower}")
        }
        _ => spec.to_owned(),
    }
}

/// Initialize the logging system. Call once at startup; a second call only
/// prints a warning.
pub fn init(
    log_dir: &str,
    log_level: &str,
    filter: Option<&str>,
    log_to_file: bool,
    log_to_stdout: bool,
) {
    // `logging.filter` / `RUST_LOG` take precedence over `LOG_LEVEL`, and both
    // go through `directives` so a bare level scopes the same way on startup
    // and on hot reload.
    let env_filter =
        EnvFilter::try_new(directives(filter.unwrap_or(log_level))).unwrap_or_else(|e| {
            eprintln!("Warning: invalid log filter ({e}); falling back to info");
            EnvFilter::new(directives("info"))
        });
    let (reloadable_filter, reload_handle) = reload::Layer::new(env_filter);

    // File: full detail, daily rotation, written by a background thread.
    let mut guards = Vec::new();
    let file_layer = log_to_file.then(|| {
        if let Err(e) = std::fs::create_dir_all(log_dir) {
            eprintln!("Warning: could not create log directory: {e}");
        }
        let appender = tracing_appender::rolling::daily(log_dir, "sonicboom.log");
        let (writer, guard) = tracing_appender::non_blocking(appender);
        guards.push(guard);
        fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_ansi(false)
            .with_writer(writer)
    });

    // Console: compact, local time.
    let stdout_layer = log_to_stdout.then(|| {
        fmt::layer()
            .with_target(true)
            .with_ansi(false)
            .with_timer(LocalTime)
            .compact()
    });

    let _ = LOG_GUARDS.set(guards);
    let subscriber = tracing_subscriber::registry()
        .with(reloadable_filter)
        .with(file_layer)
        .with(stdout_layer);
    if let Err(e) = subscriber.try_init() {
        eprintln!("Warning: logging was already initialized: {e}");
    }

    let setter: Box<LevelSetter> = Box::new(move |spec: &str| {
        let filter = EnvFilter::try_new(directives(spec)).map_err(|e| e.to_string())?;
        reload_handle
            .modify(|f| *f = filter)
            .map_err(|e| e.to_string())
    });
    let _ = LEVEL_HANDLE.set(setter);
}

/// Hot-reload the log level or a directive list (spec §44). Accepts the same
/// values as `LOG_LEVEL` and `logging.filter`; ConfigManager resolves precedence.
pub fn set_level(spec: &str) -> Result<(), String> {
    match LEVEL_HANDLE.get() {
        Some(set) => set(spec),
        None => Err("logging is not initialized".to_string()),
    }
}

/// Log startup banner
pub fn log_startup(port: u16, log_dir: &str) {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        port = port,
        log_dir = log_dir,
        "SonicBoom TTS Server starting"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_levels_are_scoped_to_sonicboom_and_access_log() {
        assert_eq!(
            directives("info"),
            "warn,SonicBoom=info,sonicboom=info,tower_http=info"
        );
        assert_eq!(
            directives(" DEBUG "),
            "warn,SonicBoom=debug,sonicboom=debug,tower_http=debug"
        );
    }

    #[test]
    fn explicit_directives_pass_through_unchanged() {
        assert_eq!(directives("warn,ort=info"), "warn,ort=info");
        assert!(EnvFilter::try_new(directives("info")).is_ok());
        assert!(EnvFilter::try_new(directives("warn,sonicboom=debug")).is_ok());
    }

    #[test]
    fn set_level_before_init_is_an_error_or_ok() {
        // The test process may already have initialized logging, so only
        // assert that the call is safe and returns a Result.
        let _ = set_level("debug");
    }

    #[test]
    fn init_then_set_level_round_trips() {
        // Stdout only, no files. A second init in the same process only
        // prints a warning (try_init), so this is safe under the test harness.
        init("unused", "info", None, false, false);
        assert!(set_level("debug").is_ok());
        assert!(set_level("warn").is_ok());
        assert!(set_level("warn,ort=info").is_ok());
    }
}
