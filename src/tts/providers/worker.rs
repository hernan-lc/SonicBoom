//! Shared building blocks for in-process cloning providers: a process-wide
//! guard that serializes heavyweight model construction, and a bounded
//! conditioning cache keyed by the reference identity.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Serialize heavyweight construction across providers. Already resident
/// models and inference remain independent; a failed load cannot poison
/// this resource guard for another provider.
pub fn model_load_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOAD: Mutex<()> = Mutex::new(());
    LOAD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Bounded least-recently-used cache for voice conditioning artifacts,
/// keyed by `(provider, model revision, reference hash, transcript, mode)`.
#[derive(Debug)]
pub struct ConditioningCache {
    capacity: usize,
    entries: VecDeque<(String, Vec<u8>)>,
}

impl ConditioningCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.clamp(1, 1024),
            entries: VecDeque::new(),
        }
    }

    pub fn cache_key(
        provider: &str,
        model_revision: &str,
        reference_hash: &str,
        transcript: Option<&str>,
        mode: &str,
    ) -> String {
        use sha2::Digest;
        let identity =
            serde_json::to_vec(&(provider, model_revision, reference_hash, transcript, mode))
                .expect("string identity serializes");
        format!(
            "{provider}|{model_revision}|{}",
            hex::encode(sha2::Sha256::digest(identity))
        )
    }

    pub fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        let pos = self.entries.iter().position(|(k, _)| k == key)?;
        let (k, v) = self.entries.remove(pos)?;
        self.entries.push_back((k, v.clone()));
        Some(v)
    }

    pub fn insert(&mut self, key: String, value: Vec<u8>) {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == &key) {
            self.entries.remove(pos);
        }
        if key.len().saturating_add(value.len()) > 64 * 1024 * 1024 {
            return;
        }
        self.entries.push_back((key, value));
        while self.entries.len() > self.capacity
            || self
                .entries
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
            self.entries.pop_front();
        }
    }

    /// Drop a single entry (voice-profile deletion/rotation). Missing keys
    /// are a no-op so double deletion stays idempotent.
    pub fn remove(&mut self, key: &str) {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
            self.entries.remove(pos);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditioning_cache_evicts_least_recently_used() {
        let mut cache = ConditioningCache::new(2);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]);
        assert_eq!(cache.get("a"), Some(vec![1]));
        cache.insert("c".into(), vec![3]);
        assert_eq!(cache.len(), 2);
        assert!(cache.get("b").is_none());
        assert!(cache.get("a").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn distinct_references_have_distinct_keys() {
        let a = ConditioningCache::cache_key("reverb-tts", "r", "hash-a", None, "icl");
        let b = ConditioningCache::cache_key("reverb-tts", "r", "hash-b", None, "icl");
        assert_ne!(a, b);
    }

    #[test]
    fn single_key_removal_is_idempotent() {
        let mut cache = ConditioningCache::new(8);
        cache.insert("voice-a".into(), vec![1]);
        cache.insert("voice-b".into(), vec![2]);
        cache.remove("voice-a");
        assert!(cache.get("voice-a").is_none());
        assert_eq!(cache.get("voice-b"), Some(vec![2]));
        cache.remove("voice-a");
        assert_eq!(cache.len(), 1);
    }
}
