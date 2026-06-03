//! Storage Buckets API - per-bucket quota + eviction policies.
//!
//! Spec: https://wicg.github.io/storage-buckets/
//! navigator.storageBuckets.open(name, opts) - bucket dostane vlastni IDB/Caches/...

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BucketDurability {
    Relaxed,    // pri OS crash mozno ztratit pendings
    Strict,     // fsync per write
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BucketPersistence {
    BestEffort, // muze byt evicted pri storage pressure
    Persistent, // nesmi byt evicted (vyzaduje user permission)
}

#[derive(Debug, Clone)]
pub struct StorageBucket {
    pub name: String,
    pub quota_bytes: u64,
    pub usage_bytes: u64,
    pub durability: BucketDurability,
    pub persistence: BucketPersistence,
    pub expires_unix_ms: Option<u64>,
}

impl StorageBucket {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            quota_bytes: 0,
            usage_bytes: 0,
            durability: BucketDurability::Relaxed,
            persistence: BucketPersistence::BestEffort,
            expires_unix_ms: None,
        }
    }

    pub fn allocate(&mut self, bytes: u64) -> Result<u64, String> {
        if self.quota_bytes > 0 && self.usage_bytes + bytes > self.quota_bytes {
            return Err(format!("bucket '{}' quota exceeded ({} + {} > {})", self.name, self.usage_bytes, bytes, self.quota_bytes));
        }
        self.usage_bytes += bytes;
        Ok(self.usage_bytes)
    }

    pub fn free(&mut self, bytes: u64) {
        self.usage_bytes = self.usage_bytes.saturating_sub(bytes);
    }

    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        self.expires_unix_ms.map(|t| now_unix_ms >= t).unwrap_or(false)
    }
}

#[derive(Default)]
pub struct StorageBucketManager {
    pub buckets: HashMap<String, StorageBucket>,
}

impl StorageBucketManager {
    pub fn new() -> Self { Self::default() }

    pub fn open(&mut self, name: &str) -> &mut StorageBucket {
        self.buckets.entry(name.into()).or_insert_with(|| StorageBucket::new(name))
    }

    pub fn delete(&mut self, name: &str) -> bool {
        self.buckets.remove(name).is_some()
    }

    pub fn keys(&self) -> Vec<String> { self.buckets.keys().cloned().collect() }

    /// LRU-ish eviction: drop best-effort buckets pri pressure.
    pub fn evict_best_effort(&mut self) -> Vec<String> {
        let to_drop: Vec<String> = self.buckets.iter()
            .filter(|(_, b)| b.persistence == BucketPersistence::BestEffort)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &to_drop { self.buckets.remove(k); }
        to_drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_or_returns() {
        let mut m = StorageBucketManager::new();
        m.open("inbox");
        m.open("inbox").quota_bytes = 1024;
        assert_eq!(m.open("inbox").quota_bytes, 1024);
    }

    #[test]
    fn allocate_under_quota() {
        let mut b = StorageBucket::new("b");
        b.quota_bytes = 1000;
        assert!(b.allocate(500).is_ok());
        assert_eq!(b.usage_bytes, 500);
    }

    #[test]
    fn quota_exceeded_errors() {
        let mut b = StorageBucket::new("b");
        b.quota_bytes = 100;
        assert!(b.allocate(200).is_err());
    }

    #[test]
    fn evict_best_effort_drops_only_those() {
        let mut m = StorageBucketManager::new();
        m.open("a");
        m.open("b").persistence = BucketPersistence::Persistent;
        let dropped = m.evict_best_effort();
        assert!(dropped.contains(&"a".to_string()));
        assert!(m.buckets.contains_key("b"));
    }

    #[test]
    fn expiry_check() {
        let mut b = StorageBucket::new("b");
        b.expires_unix_ms = Some(1000);
        assert!(b.is_expired(2000));
        assert!(!b.is_expired(500));
    }
}
