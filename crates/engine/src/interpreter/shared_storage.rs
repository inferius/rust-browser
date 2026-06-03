//! Shared Storage API - cross-site write-once, read-in-fenced-frame.
//!
//! Spec: https://wicg.github.io/shared-storage/
//! sharedStorage.set/append/delete (any site, write only).
//! sharedStorage.selectURL([urls], runOperation) - vraci opaque URN -> fenced frame.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SharedStorageEntry {
    pub value: String,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
}

#[derive(Default)]
pub struct SharedStorageOrigin {
    pub entries: HashMap<String, SharedStorageEntry>,
    /// Max entries per origin (spec: 10000).
    pub max_entries: usize,
    pub budget_bits: f64, // 12 daily, replenished
}

impl SharedStorageOrigin {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), max_entries: 10_000, budget_bits: 12.0 }
    }

    pub fn set(&mut self, key: &str, value: &str, now: u64) -> Result<(), String> {
        if !self.entries.contains_key(key) && self.entries.len() >= self.max_entries {
            return Err(format!("shared storage origin max entries {} reached", self.max_entries));
        }
        let entry = self.entries.entry(key.into()).or_insert_with(|| SharedStorageEntry {
            value: String::new(), created_unix_ms: now, updated_unix_ms: now,
        });
        entry.value = value.into();
        entry.updated_unix_ms = now;
        Ok(())
    }

    pub fn append(&mut self, key: &str, value: &str, now: u64) -> Result<(), String> {
        let cur = self.entries.get(key).map(|e| e.value.clone()).unwrap_or_default();
        self.set(key, &format!("{}{}", cur, value), now)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn select_url(&mut self, urls: Vec<String>, index_hint: usize) -> Result<String, String> {
        if urls.is_empty() { return Err("urls empty".into()); }
        let log2 = (urls.len() as f64).log2();
        if log2 > self.budget_bits {
            return Err(format!("budget {:.2} bits insufficient for {} entries", self.budget_bits, urls.len()));
        }
        self.budget_bits -= log2;
        let urn = format!("urn:uuid:opaque-{}", index_hint % urls.len());
        Ok(urn)
    }
}

#[derive(Default)]
pub struct SharedStorageRegistry {
    pub origins: HashMap<String, SharedStorageOrigin>,
}

impl SharedStorageRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn for_origin(&mut self, origin: &str) -> &mut SharedStorageOrigin {
        self.origins.entry(origin.into()).or_insert_with(SharedStorageOrigin::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_append() {
        let mut o = SharedStorageOrigin::new();
        o.set("k", "v", 1000).unwrap();
        o.append("k", "v2", 2000).unwrap();
        assert_eq!(o.entries.get("k").unwrap().value, "vv2");
    }

    #[test]
    fn delete_returns_status() {
        let mut o = SharedStorageOrigin::new();
        o.set("k", "v", 1000).unwrap();
        assert!(o.delete("k"));
        assert!(!o.delete("k"));
    }

    #[test]
    fn select_url_returns_urn() {
        let mut o = SharedStorageOrigin::new();
        let urn = o.select_url(vec!["a".into(), "b".into()], 0).unwrap();
        assert!(urn.starts_with("urn:uuid:"));
    }

    #[test]
    fn select_url_budget_check() {
        let mut o = SharedStorageOrigin::new();
        // log2(10000) ~ 13.3 > 12 bits -> exceed
        let urls: Vec<String> = (0..10000).map(|i| format!("u{}", i)).collect();
        assert!(o.select_url(urls, 0).is_err());
    }

    #[test]
    fn max_entries_enforced() {
        let mut o = SharedStorageOrigin::new();
        o.max_entries = 2;
        o.set("a", "1", 1).unwrap();
        o.set("b", "2", 1).unwrap();
        assert!(o.set("c", "3", 1).is_err());
        // update existing OK
        assert!(o.set("a", "1b", 1).is_ok());
    }
}
