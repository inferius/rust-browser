//! Storage Manager API - quota + estimate + persistent storage.
//!
//! Spec: https://storage.spec.whatwg.org/
//! navigator.storage.estimate() + persist() + persisted().

use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct StorageEstimate {
    pub usage_bytes: u64,
    pub quota_bytes: u64,
}

#[derive(Default)]
pub struct StorageManager {
    /// Per-origin usage + quota.
    pub estimates: HashMap<String, StorageEstimate>,
    /// Persistent flag per-origin (persist() granted = NE evicted).
    pub persistent: HashMap<String, bool>,
}

impl StorageManager {
    pub fn new() -> Self { Self::default() }

    pub fn estimate(&self, origin: &str) -> StorageEstimate {
        self.estimates.get(origin).copied().unwrap_or(StorageEstimate {
            usage_bytes: 0,
            quota_bytes: 100 * 1024 * 1024, // 100MB default
        })
    }

    pub fn set_usage(&mut self, origin: &str, usage: u64) {
        let entry = self.estimates.entry(origin.into()).or_insert(StorageEstimate {
            usage_bytes: 0,
            quota_bytes: 100 * 1024 * 1024,
        });
        entry.usage_bytes = usage;
    }

    pub fn set_quota(&mut self, origin: &str, quota: u64) {
        let entry = self.estimates.entry(origin.into()).or_insert(StorageEstimate {
            usage_bytes: 0,
            quota_bytes: 0,
        });
        entry.quota_bytes = quota;
    }

    pub fn persist(&mut self, origin: &str) -> bool {
        self.persistent.insert(origin.into(), true);
        true
    }

    pub fn persisted(&self, origin: &str) -> bool {
        self.persistent.get(origin).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_estimate() {
        let m = StorageManager::new();
        let e = m.estimate("https://x.com");
        assert_eq!(e.usage_bytes, 0);
        assert!(e.quota_bytes > 0);
    }

    #[test]
    fn persist_marks() {
        let mut m = StorageManager::new();
        assert!(!m.persisted("https://x.com"));
        m.persist("https://x.com");
        assert!(m.persisted("https://x.com"));
    }

    #[test]
    fn usage_update() {
        let mut m = StorageManager::new();
        m.set_usage("https://x.com", 50_000);
        assert_eq!(m.estimate("https://x.com").usage_bytes, 50_000);
    }
}
