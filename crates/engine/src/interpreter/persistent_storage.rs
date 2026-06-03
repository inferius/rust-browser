//! localStorage / sessionStorage backed by per-origin disk store.
//!
//! Spec: https://html.spec.whatwg.org/multipage/webstorage.html
//! Storage event fires across same-origin documents.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StorageRecord {
    pub origin: String,
    pub entries: HashMap<String, String>,
    pub kind: StorageKind,
    pub last_modified_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageKind {
    Local,
    Session,
}

#[derive(Default)]
pub struct PersistentStorage {
    /// (origin, kind) -> record
    pub stores: HashMap<(String, StorageKind), StorageRecord>,
    /// Quota in bytes (spec: ~10 MB per origin).
    pub origin_quota_bytes: usize,
    pub events_pending: Vec<StorageEvent>,
}

#[derive(Debug, Clone)]
pub struct StorageEvent {
    pub origin: String,
    pub kind: StorageKind,
    pub key: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub url: String,
}

impl PersistentStorage {
    pub fn new() -> Self {
        Self { origin_quota_bytes: 10 * 1024 * 1024, ..Self::default() }
    }

    fn record_mut(&mut self, origin: &str, kind: StorageKind, now: u64) -> &mut StorageRecord {
        let key = (origin.to_string(), kind);
        self.stores.entry(key).or_insert_with(|| StorageRecord {
            origin: origin.into(),
            entries: HashMap::new(),
            kind,
            last_modified_unix_ms: now,
        })
    }

    pub fn set(&mut self, origin: &str, kind: StorageKind, key: &str, value: &str, url: &str, now: u64) -> Result<(), String> {
        let quota = self.origin_quota_bytes;
        let event = {
            let rec = self.record_mut(origin, kind, now);
            // Check quota.
            let mut size: usize = rec.entries.iter().map(|(k, v)| k.len() + v.len()).sum();
            if let Some(existing) = rec.entries.get(key) {
                size -= key.len() + existing.len();
            }
            size += key.len() + value.len();
            if size > quota {
                return Err("storage quota exceeded".into());
            }
            let old = rec.entries.insert(key.into(), value.into());
            rec.last_modified_unix_ms = now;
            StorageEvent {
                origin: origin.into(),
                kind,
                key: Some(key.into()),
                old_value: old,
                new_value: Some(value.into()),
                url: url.into(),
            }
        };
        self.events_pending.push(event);
        Ok(())
    }

    pub fn get(&self, origin: &str, kind: StorageKind, key: &str) -> Option<&str> {
        self.stores.get(&(origin.to_string(), kind))?.entries.get(key).map(|s| s.as_str())
    }

    pub fn remove(&mut self, origin: &str, kind: StorageKind, key: &str, url: &str, now: u64) -> bool {
        let event = {
            let Some(rec) = self.stores.get_mut(&(origin.to_string(), kind)) else { return false; };
            let Some(old) = rec.entries.remove(key) else { return false; };
            rec.last_modified_unix_ms = now;
            StorageEvent {
                origin: origin.into(),
                kind,
                key: Some(key.into()),
                old_value: Some(old),
                new_value: None,
                url: url.into(),
            }
        };
        self.events_pending.push(event);
        true
    }

    pub fn clear(&mut self, origin: &str, kind: StorageKind, url: &str, now: u64) {
        if let Some(rec) = self.stores.get_mut(&(origin.to_string(), kind)) {
            rec.entries.clear();
            rec.last_modified_unix_ms = now;
        }
        self.events_pending.push(StorageEvent {
            origin: origin.into(), kind,
            key: None, old_value: None, new_value: None,
            url: url.into(),
        });
    }

    pub fn keys(&self, origin: &str, kind: StorageKind) -> Vec<String> {
        self.stores.get(&(origin.to_string(), kind))
            .map(|r| r.entries.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn drain_events(&mut self) -> Vec<StorageEvent> {
        std::mem::take(&mut self.events_pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let mut s = PersistentStorage::new();
        s.set("https://x.com", StorageKind::Local, "k", "v", "https://x.com/page", 0).unwrap();
        assert_eq!(s.get("https://x.com", StorageKind::Local, "k"), Some("v"));
    }

    #[test]
    fn isolated_per_origin() {
        let mut s = PersistentStorage::new();
        s.set("https://a.com", StorageKind::Local, "k", "1", "", 0).unwrap();
        s.set("https://b.com", StorageKind::Local, "k", "2", "", 0).unwrap();
        assert_eq!(s.get("https://a.com", StorageKind::Local, "k"), Some("1"));
        assert_eq!(s.get("https://b.com", StorageKind::Local, "k"), Some("2"));
    }

    #[test]
    fn remove_returns_status() {
        let mut s = PersistentStorage::new();
        s.set("o", StorageKind::Local, "k", "v", "", 0).unwrap();
        assert!(s.remove("o", StorageKind::Local, "k", "", 0));
        assert!(!s.remove("o", StorageKind::Local, "k", "", 0));
    }

    #[test]
    fn quota_enforced() {
        let mut s = PersistentStorage::new();
        s.origin_quota_bytes = 10;
        // 6 bytes ok
        assert!(s.set("o", StorageKind::Local, "k", "vvv", "", 0).is_ok());
        // 20 bytes -> exceed
        assert!(s.set("o", StorageKind::Local, "k2", &"x".repeat(50), "", 0).is_err());
    }

    #[test]
    fn events_emitted() {
        let mut s = PersistentStorage::new();
        s.set("o", StorageKind::Local, "k", "v1", "page", 0).unwrap();
        s.set("o", StorageKind::Local, "k", "v2", "page", 0).unwrap();
        let events = s.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].old_value.as_deref(), Some("v1"));
        assert_eq!(events[1].new_value.as_deref(), Some("v2"));
    }

    #[test]
    fn clear_fires_null_event() {
        let mut s = PersistentStorage::new();
        s.set("o", StorageKind::Local, "k", "v", "", 0).unwrap();
        s.clear("o", StorageKind::Local, "", 0);
        let events = s.drain_events();
        let last = events.last().unwrap();
        assert!(last.key.is_none());
    }
}
