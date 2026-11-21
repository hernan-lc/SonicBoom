//! Keeps native audio libraries from writing probe noise into the console.
//!
//! While cpal looks for an output device, ALSA and JACK print lines such as
//! `Cannot open device /dev/dsp` or `connect(2) call to /dev/shm/jack-1000/... failed`
//! straight to the process's stderr. They are expected on most desktops (ALSA's default
//! chain probes the OSS and JACK plugins and rejects them) and bury SonicBoom's own log.
//! Failures that matter are still reported through `tracing` by the callers.
//!
//! [`quiet`] points stderr at `/dev/null` for the duration of the closure. Set
//! `SONICBOOM_NATIVE_LOGS=1` to show the native output again.

use std::sync::Mutex;

/// Serializes redirections: stderr is process-wide.
static REDIRECT: Mutex<()> = Mutex::new(());

fn native_logs_enabled() -> bool {
    std::env::var("SONICBOOM_NATIVE_LOGS").is_ok_and(|v| !matches!(v.trim(), "" | "0" | "false"))
}

/// Run `f` with stderr silenced (Unix). Other platforms and
/// `SONICBOOM_NATIVE_LOGS=1` run `f` unchanged.
pub fn quiet<T>(f: impl FnOnce() -> T) -> T {
    if native_logs_enabled() {
        return f();
    }
    #[cfg(unix)]
    {
        let _lock = REDIRECT.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = unix::Redirect::to_dev_null();
        f()
    }
    #[cfg(not(unix))]
    {
        let _ = &REDIRECT;
        f()
    }
}

#[cfg(unix)]
mod unix {
    /// Restores the original stderr when dropped, including on panic.
    pub struct Redirect {
        saved: Option<libc::c_int>,
    }

    impl Redirect {
        pub fn to_dev_null() -> Self {
            // SAFETY: plain descriptor calls on descriptors this function opened or
            // that are always valid (2); every failure leaves stderr untouched.
            unsafe {
                let saved = libc::dup(2);
                let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
                if saved < 0 || null < 0 {
                    if saved >= 0 {
                        libc::close(saved);
                    }
                    if null >= 0 {
                        libc::close(null);
                    }
                    return Self { saved: None };
                }
                libc::dup2(null, 2);
                libc::close(null);
                Self { saved: Some(saved) }
            }
        }
    }

    impl Drop for Redirect {
        fn drop(&mut self) {
            if let Some(saved) = self.saved.take() {
                // SAFETY: `saved` is the descriptor duplicated in `to_dev_null`.
                unsafe {
                    libc::dup2(saved, 2);
                    libc::close(saved);
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The tests compare the process-wide stderr, so they must not overlap.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn stderr_identity() -> (u64, u64) {
        // SAFETY: fstat on fd 2 with a zeroed out-parameter.
        unsafe {
            let mut stat: libc::stat = std::mem::zeroed();
            assert_eq!(libc::fstat(2, &mut stat), 0);
            (stat.st_dev as u64, stat.st_ino as u64)
        }
    }

    #[test]
    fn quiet_returns_the_value_and_restores_stderr() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let before = stderr_identity();
        let inside = quiet(stderr_identity);
        assert_ne!(
            inside, before,
            "stderr must point elsewhere inside the closure"
        );
        assert_eq!(
            stderr_identity(),
            before,
            "stderr must be restored afterwards"
        );
        assert_eq!(quiet(|| 7), 7);
    }

    #[test]
    fn stderr_is_restored_when_the_closure_panics() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let before = stderr_identity();
        let result = std::panic::catch_unwind(|| quiet(|| panic!("boom")));
        assert!(result.is_err());
        assert_eq!(stderr_identity(), before);
    }
}
