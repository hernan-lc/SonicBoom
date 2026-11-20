//! Persistent optional-provider singletons.
//!
//! The Pocket TTS and Reverb-TTS models are large (hundreds of megabytes
//! to gigabytes of weights); they must load once and stay resident
//! between requests — never reload per request. This pool owns the
//! singletons and syncs their configuration from the live `AppConfig` on
//! every registry build, so hot-reload applies without restarts and a
//! failed reload keeps the last healthy state.

use std::sync::Arc;

use super::TtsProviderRegistry;
use super::pocket_tts::{PocketTtsConfig, PocketTtsProvider};
use super::reverb_tts::{ReverbTtsConfig, ReverbTtsProvider};
use super::supertonic::SupertonicProvider;

type DesiredProviders = Option<(u64, PocketTtsConfig)>;

/// Resident optional providers shared by all requests.
pub struct ProviderPool {
    desired: Arc<std::sync::Mutex<DesiredProviders>>,
    retrying: Arc<std::sync::atomic::AtomicBool>,
    /// Pocket TTS singleton (sessions + voice embeddings behind one lock).
    pub pocket: Arc<PocketTtsProvider>,
    /// Reverb-TTS singleton (ONNX sessions + cached reference codes behind one lock).
    pub reverb: Arc<ReverbTtsProvider>,
}

impl Default for ProviderPool {
    fn default() -> Self {
        Self {
            desired: Arc::new(std::sync::Mutex::new(None)),
            retrying: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pocket: PocketTtsProvider::new(PocketTtsConfig::default()),
            reverb: ReverbTtsProvider::new(ReverbTtsConfig::default()),
        }
    }
}

impl ProviderPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sync the Pocket TTS singleton configuration (reconfiguration drops
    /// its loaded sessions; unchanged configs keep serving). Only a
    /// genuinely changed config reconfigures; unchanged providers keep
    /// serving.
    pub fn sync_pocket_config(&self, pocket: PocketTtsConfig) {
        self.pocket.sync_config(pocket);
    }

    /// Sync the Reverb-TTS singleton configuration. A reload deferred because a
    /// synthesis holds the model is retried by the next registry build.
    pub fn sync_reverb_config(&self, reverb: ReverbTtsConfig) {
        self.reverb.sync_config(reverb);
    }

    /// Reject request-scoped snapshots older than the most recently adopted
    /// server configuration. Equal versions retry deferred busy-model changes.
    pub fn sync_versioned(&self, generation: u64, pocket: PocketTtsConfig) {
        let mut current = self.desired.lock().unwrap_or_else(|e| e.into_inner());
        if current
            .as_ref()
            .is_some_and(|(current, _)| generation < *current)
        {
            return;
        }
        *current = Some((generation, pocket.clone()));
        self.sync_pocket_config(pocket);
        drop(current);
        self.retry_deferred();
    }
    fn retry_deferred(&self) {
        use std::sync::atomic::Ordering;
        if !self.pocket.reload_pending() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        if self.retrying.swap(true, Ordering::AcqRel) {
            return;
        }
        let desired = self.desired.clone();
        let retrying = self.retrying.clone();
        let pocket = self.pocket.clone();
        runtime.spawn(async move {
            struct Reset(Arc<std::sync::atomic::AtomicBool>, bool);
            impl Drop for Reset {
                fn drop(&mut self) {
                    if self.1 {
                        self.0.store(false, Ordering::Release);
                    }
                }
            }
            let mut reset = Reset(retrying, true);
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let current = desired.lock().unwrap_or_else(|e| e.into_inner());
                if let Some((_, p)) = current.as_ref() {
                    pocket.sync_config(p.clone());
                }
                pocket.kick_ensure_if_needed(None);
                if !pocket.reload_pending() {
                    // Publish idle while holding the same lock used by new
                    // versions; a concurrent update cannot lose its retry.
                    reset.0.store(false, Ordering::Release);
                    reset.1 = false;
                    break;
                }
            }
        });
    }

    /// Kick background download + load for enabled providers that have no
    /// loaded model and no recorded error (startup and hot-reload path).
    /// Never blocks. `hf_token` covers gated repositories; the pinned
    /// public bundles need none.
    pub fn kick_ensure_if_needed(&self, hf_token: Option<String>) {
        self.pocket.kick_ensure_if_needed(hf_token.clone());
        self.reverb.kick_ensure_if_needed(hf_token);
    }

    /// Build a request-scoped registry around the resident singletons plus
    /// a Supertonic adapter bound to the current handle (if any).
    pub fn registry(&self, supertonic: Arc<SupertonicProvider>) -> TtsProviderRegistry {
        let registry = TtsProviderRegistry::new();
        registry.register(supertonic);
        registry.register(Arc::clone(&self.pocket) as Arc<dyn super::TtsProvider>);
        registry.register(Arc::clone(&self.reverb) as Arc<dyn super::TtsProvider>);
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::providers::TtsProvider;

    #[test]
    fn older_configuration_cannot_restore_a_retired_provider() {
        let pool = ProviderPool::new();
        let enabled = PocketTtsConfig {
            enabled: true,
            ..Default::default()
        };
        pool.sync_versioned(10, enabled.clone());
        pool.sync_versioned(11, PocketTtsConfig::default());
        pool.sync_versioned(10, enabled);
        assert!(!pool.pocket.status().enabled);
    }
    #[test]
    fn pool_registers_all_providers() {
        let pool = ProviderPool::new();
        let supertonic = Arc::new(SupertonicProvider::new(std::path::PathBuf::from(
            "./models",
        )));
        let registry = pool.registry(supertonic);
        // Retired providers must never be registered.
        for retired in ["qwen3", "qwen3-gguf", "gpt-sovits"] {
            assert!(!registry.provider_ids().contains(&retired.to_string()));
        }
        assert_eq!(
            registry.provider_ids(),
            vec!["pocket-tts", "reverb-tts", "supertonic"]
        );
    }

    #[test]
    fn sync_keeps_healthy_singletons_when_config_is_unchanged() {
        let pool = ProviderPool::new();
        pool.sync_pocket_config(PocketTtsConfig::default());
        // No loaded model, no error, disabled by default: sync is a no-op
        // that must not record failures.
        assert!(!pool.pocket.status().enabled);
    }
}
