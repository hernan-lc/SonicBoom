//! Cooperative execution control shared by HTTP admission and native workers.
use crate::tts::quality::QualityReport;
use serde::Serialize;
use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionContext {
    pub provider: String,
    pub model: String,
    pub voice: String,
    pub effective_device: String,
    /// Decimal string preserves all u64 bits in browser JSON clients.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub context: Option<ExecutionContext>,
    pub stage: String,
    pub frames: usize,
    pub elapsed_secs: f64,
    pub audio_secs: Option<f64>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub stage_secs: std::collections::BTreeMap<String, f64>,
    /// Deterministic quality report for the produced audio, when detection ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityReport>,
    /// Long-form fragment progress: total fragments of the request, when the
    /// request was segmented, and how many fragments have completed audio.
    /// `None` total means the request was never segmented (single unit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragments_total: Option<usize>,
    pub fragments_done: usize,
}

#[derive(Debug)]
pub struct InferenceControl {
    pub id: String,
    cancelled: AtomicBool,
    started: Instant,
    deadline: Instant,
    progress: Mutex<Progress>,
}

impl InferenceControl {
    pub fn new(timeout: Duration) -> Arc<Self> {
        let started = Instant::now();
        Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            cancelled: AtomicBool::new(false),
            started,
            deadline: started + timeout.min(Duration::from_secs(86400)),
            progress: Mutex::new(Progress {
                context: None,
                stage: "queued".into(),
                frames: 0,
                elapsed_secs: 0.0,
                audio_secs: None,
                updated_at: chrono::Utc::now(),
                stage_secs: std::collections::BTreeMap::new(),
                quality: None,
                fragments_total: None,
                fragments_done: 0,
            }),
        })
    }
    /// Publish the deterministic quality report for the produced audio.
    pub fn set_quality(&self, report: QualityReport) {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quality = Some(report);
    }
    pub fn set_audio_duration(&self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .audio_secs = Some(seconds);
        }
    }
    pub fn set_context(&self, provider: &str, model: &str, voice: &str, device: &str) {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .context = Some(ExecutionContext {
            provider: provider.into(),
            model: model.into(),
            voice: voice.into(),
            effective_device: device.into(),
            seed: None,
        });
    }
    pub fn set_seed(&self, seed: u64) {
        if let Some(context) = &mut self
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .context
        {
            context.seed = Some(seed.to_string());
        }
    }
    /// Declare how many independently completable fragments the request was
    /// split into. Published before fragment synthesis starts so pollers and
    /// SSE consumers can render total progress while audio is still pending.
    pub fn set_fragments_total(&self, total: usize) {
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        p.fragments_total = Some(total);
        p.updated_at = chrono::Utc::now();
    }
    /// Mark fragment `index` (`0`-based) as completed. `fragments_done`
    /// always tracks the highest completed prefix (`index + 1`), so a slow
    /// consumer never observes reordered or duplicated completion.
    pub fn set_fragment_done(&self, index: usize) {
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        p.fragments_done = p.fragments_done.max(index.saturating_add(1));
        if let Some(total) = p.fragments_total {
            p.fragments_done = p.fragments_done.min(total);
        }
        p.updated_at = chrono::Utc::now();
    }
    /// Add a measured duration to a stage bucket without changing the current
    /// stage. Used for work that happens outside staged inference, such as
    /// text normalization at enqueue time and queue waiting before the
    /// blocking task starts. Never carries user text.
    pub fn record_stage_secs(&self, stage: &str, secs: f64) {
        if !secs.is_finite() || secs <= 0.0 {
            return;
        }
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        *p.stage_secs.entry(stage.into()).or_default() += secs;
        p.updated_at = chrono::Utc::now();
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn cancellation_requested(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn deadline_exceeded(&self) -> bool {
        Instant::now() >= self.deadline
    }
    pub fn stopped(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline
    }
    pub fn update(&self, stage: &str, frames: usize) -> bool {
        if stage != "acoustic" {
            let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
            let elapsed = self.started.elapsed().as_secs_f64();
            let previous_stage = p.stage.clone();
            let delta = elapsed - p.elapsed_secs;
            *p.stage_secs.entry(previous_stage).or_default() += delta;
            p.stage = stage.into();
            p.frames = frames;
            p.elapsed_secs = elapsed;
            p.updated_at = chrono::Utc::now();
        }
        !self.stopped()
    }
    pub fn snapshot(&self) -> Progress {
        let mut p = self
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        p.elapsed_secs = self.started.elapsed().as_secs_f64();
        p
    }
}
thread_local! { static CURRENT: RefCell<Option<Arc<InferenceControl>>> = const { RefCell::new(None) }; }
pub fn current() -> Option<Arc<InferenceControl>> {
    CURRENT.with(|c| c.borrow().clone())
}
pub fn scope<T>(control: Arc<InferenceControl>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<InferenceControl>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|c| {
                c.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(CURRENT.with(|c| c.replace(Some(control))));
    work()
}

/// Model serialization without an uninterruptible mutex wait. Lock ordering:
/// lifecycle -> model/worker -> short-lived state/config locks.
pub fn lock_cancellable<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    loop {
        if current().is_some_and(|c| c.stopped()) {
            return Err("inference cancelled or deadline exceeded".into());
        }
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err("model lock poisoned".into()),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(10))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_seed_preserves_full_u64_precision() {
        let control = InferenceControl::new(Duration::from_secs(1));
        control.set_context("pocket-tts", "pocket-tts-en", "voice", "cpu");
        control.set_seed(u64::MAX);
        let value = serde_json::to_value(control.snapshot()).unwrap();
        assert_eq!(value["context"]["seed"], "18446744073709551615");
    }

    #[test]
    fn fragment_progress_is_ordered_and_clamped() {
        let control = InferenceControl::new(Duration::from_secs(60));
        assert_eq!(control.snapshot().fragments_total, None);
        assert_eq!(control.snapshot().fragments_done, 0);
        control.set_fragments_total(3);
        control.set_fragment_done(1);
        // Highest completed prefix wins; out-of-order completion never
        // moves the counter backwards.
        control.set_fragment_done(0);
        let snapshot = control.snapshot();
        assert_eq!(snapshot.fragments_total, Some(3));
        assert_eq!(snapshot.fragments_done, 2);
        control.set_fragment_done(99);
        assert_eq!(control.snapshot().fragments_done, 3);
    }

    #[test]
    fn stage_recording_ignores_non_positive_or_non_finite_durations() {
        let control = InferenceControl::new(Duration::from_secs(60));
        control.record_stage_secs("normalization", 0.5);
        control.record_stage_secs("normalization", 0.0);
        control.record_stage_secs("queue_wait", -1.0);
        control.record_stage_secs("queue_wait", f64::NAN);
        let snapshot = control.snapshot();
        assert_eq!(snapshot.stage_secs.get("normalization"), Some(&0.5));
        assert!(!snapshot.stage_secs.contains_key("queue_wait"));
    }

    #[test]
    fn cancellation_and_deadline_are_observed_at_checkpoints() {
        let control = InferenceControl::new(Duration::from_secs(1));
        assert!(control.update("generating", 1));
        control.cancel();
        assert!(!control.update("generating", 2));
        let expired = InferenceControl::new(Duration::ZERO);
        assert!(expired.stopped());
    }
    #[test]
    fn scoped_control_is_restored_after_panics() {
        let outer = InferenceControl::new(Duration::from_secs(1));
        scope(outer.clone(), || {
            let _ = std::panic::catch_unwind(|| {
                scope(InferenceControl::new(Duration::ZERO), || {
                    panic!("test panic")
                })
            });
            assert!(Arc::ptr_eq(&current().unwrap(), &outer));
        });
        assert!(current().is_none());
    }
}
