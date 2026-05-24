//! Back-Forward Cache (BFCache) - keep recently navigated-away pages alive.
//!
//! Spec: https://web.dev/bfcache/ + WHATWG HTML "history traversal" steps.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CacheState {
    Eligible,
    Cached,
    Restored,
    Evicted,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct BfCacheEntry {
    pub history_index: u32,
    pub url: String,
    pub state: CacheState,
    pub stored_unix_ms: u64,
    pub estimated_memory_kb: u64,
    pub block_reason: Option<BlockReason>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockReason {
    HasUnloadHandler,           // unloadlistener attached
    HasBeforeUnloadDialog,
    UsingWebRTC,
    UsingWebSocket,
    InflightFetchOrXhr,
    ContainsPlugin,
    HasStreams,                 // ReadableStream/WritableStream
    HoldsServiceWorkerClient,
    Disabled,
    InternalError,
}

#[derive(Default)]
pub struct BfCache {
    pub entries: HashMap<u32, BfCacheEntry>,
    pub max_entries: usize,
    pub max_memory_kb: u64,
    pub eviction_age_ms: u64,
}

impl BfCache {
    pub fn new() -> Self {
        Self {
            max_entries: 5,
            max_memory_kb: 300_000,
            eviction_age_ms: 10 * 60 * 1000,        // 10 min
            ..Self::default()
        }
    }

    pub fn store(&mut self, entry: BfCacheEntry) -> bool {
        if entry.state == CacheState::Blocked { return false; }
        self.maybe_evict(entry.estimated_memory_kb);
        self.entries.insert(entry.history_index, entry);
        true
    }

    pub fn restore(&mut self, history_index: u32, now: u64) -> Option<BfCacheEntry> {
        let mut entry = self.entries.remove(&history_index)?;
        if now > entry.stored_unix_ms + self.eviction_age_ms {
            entry.state = CacheState::Evicted;
            return None;
        }
        entry.state = CacheState::Restored;
        Some(entry)
    }

    pub fn block(&mut self, history_index: u32, reason: BlockReason) {
        if let Some(e) = self.entries.get_mut(&history_index) {
            e.state = CacheState::Blocked;
            e.block_reason = Some(reason);
        }
    }

    fn maybe_evict(&mut self, incoming_kb: u64) {
        while self.entries.len() >= self.max_entries {
            let oldest = self.entries.values().min_by_key(|e| e.stored_unix_ms).map(|e| e.history_index);
            if let Some(idx) = oldest { self.entries.remove(&idx); } else { break; }
        }
        let total: u64 = self.entries.values().map(|e| e.estimated_memory_kb).sum();
        if total + incoming_kb > self.max_memory_kb {
            while {
                let total: u64 = self.entries.values().map(|e| e.estimated_memory_kb).sum();
                total + incoming_kb > self.max_memory_kb && !self.entries.is_empty()
            } {
                let oldest = self.entries.values().min_by_key(|e| e.stored_unix_ms).map(|e| e.history_index);
                if let Some(idx) = oldest { self.entries.remove(&idx); } else { break; }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(idx: u32, ts: u64, kb: u64) -> BfCacheEntry {
        BfCacheEntry {
            history_index: idx, url: "u".into(),
            state: CacheState::Eligible,
            stored_unix_ms: ts, estimated_memory_kb: kb,
            block_reason: None,
        }
    }

    #[test]
    fn store_and_restore() {
        let mut c = BfCache::new();
        c.store(entry(1, 100, 1000));
        let e = c.restore(1, 200).unwrap();
        assert_eq!(e.state, CacheState::Restored);
    }

    #[test]
    fn blocked_not_stored() {
        let mut c = BfCache::new();
        let mut e = entry(1, 0, 100);
        e.state = CacheState::Blocked;
        assert!(!c.store(e));
    }

    #[test]
    fn restore_expired_returns_none() {
        let mut c = BfCache::new();
        c.eviction_age_ms = 1000;
        c.store(entry(1, 0, 100));
        assert!(c.restore(1, 5000).is_none());
    }

    #[test]
    fn evicts_when_too_many() {
        let mut c = BfCache::new();
        c.max_entries = 2;
        c.store(entry(1, 100, 50));
        c.store(entry(2, 200, 50));
        c.store(entry(3, 300, 50));
        assert!(c.entries.len() <= 2);
    }

    #[test]
    fn block_marks_state() {
        let mut c = BfCache::new();
        c.store(entry(1, 0, 100));
        c.block(1, BlockReason::UsingWebRTC);
        assert_eq!(c.entries[&1].state, CacheState::Blocked);
        assert_eq!(c.entries[&1].block_reason, Some(BlockReason::UsingWebRTC));
    }
}
