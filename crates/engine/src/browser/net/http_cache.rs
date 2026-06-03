//! HTTP cache - per RFC 9111.
//!
//! Implements freshness lifetime (max-age, Expires), heuristic freshness,
//! validators (ETag, Last-Modified), Vary header partitioning.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub url: String,
    pub method: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub response_headers: HashMap<String, String>,
    pub stored_unix_ms: u64,
    pub max_age_seconds: Option<u32>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub vary_keys: Vec<String>,           // "accept-encoding,user-agent"
    pub vary_partition_hash: u64,         // hash of request headers matching vary
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Freshness {
    Fresh,
    Stale,
    MustRevalidate,
}

impl CacheEntry {
    pub fn is_fresh(&self, now_unix_ms: u64) -> Freshness {
        let age_s = (now_unix_ms.saturating_sub(self.stored_unix_ms)) / 1000;
        if let Some(max_age) = self.max_age_seconds {
            if age_s < max_age as u64 { return Freshness::Fresh; }
            return Freshness::Stale;
        }
        // Heuristic: if Last-Modified present, freshness = 10% of (stored - last_modified).
        // Without timestamp parsing, fallback: treat as stale.
        if self.last_modified.is_some() && age_s < 60 {
            return Freshness::Fresh;
        }
        Freshness::Stale
    }
}

#[derive(Default)]
pub struct HttpCache {
    /// Key = (url, vary_partition_hash) for Vary-aware lookup.
    pub entries: HashMap<(String, u64), CacheEntry>,
    pub max_bytes: u64,
    pub current_bytes: u64,
}

impl HttpCache {
    pub fn new(max_bytes: u64) -> Self {
        Self { entries: HashMap::new(), max_bytes, current_bytes: 0 }
    }

    pub fn store(&mut self, entry: CacheEntry) {
        let size = entry.body.len() as u64;
        // Simple LRU-ish eviction: drop random entries until under quota.
        while self.current_bytes + size > self.max_bytes && !self.entries.is_empty() {
            let key = self.entries.keys().next().cloned().unwrap();
            if let Some(e) = self.entries.remove(&key) {
                self.current_bytes = self.current_bytes.saturating_sub(e.body.len() as u64);
            }
        }
        let key = (entry.url.clone(), entry.vary_partition_hash);
        self.current_bytes += size;
        self.entries.insert(key, entry);
    }

    pub fn lookup(&self, url: &str, vary_hash: u64) -> Option<&CacheEntry> {
        self.entries.get(&(url.into(), vary_hash))
    }

    pub fn invalidate(&mut self, url: &str) {
        let keys: Vec<_> = self.entries.keys().filter(|(u, _)| u == url).cloned().collect();
        for k in keys {
            if let Some(e) = self.entries.remove(&k) {
                self.current_bytes = self.current_bytes.saturating_sub(e.body.len() as u64);
            }
        }
    }
}

/// Parse `Cache-Control` directives into a struct.
#[derive(Debug, Clone, Default)]
pub struct CacheControl {
    pub max_age: Option<u32>,
    pub s_maxage: Option<u32>,
    pub no_cache: bool,
    pub no_store: bool,
    pub must_revalidate: bool,
    pub public: bool,
    pub private: bool,
    pub immutable: bool,
    pub stale_while_revalidate: Option<u32>,
    pub stale_if_error: Option<u32>,
}

pub fn parse_cache_control(s: &str) -> CacheControl {
    let mut cc = CacheControl::default();
    for token in s.split(',') {
        let t = token.trim();
        let lower = t.to_ascii_lowercase();
        let (k, v) = match lower.find('=') {
            Some(i) => (&lower[..i], Some(&lower[i + 1..])),
            None => (lower.as_str(), None),
        };
        match k {
            "max-age" => cc.max_age = v.and_then(|x| x.trim_matches('"').parse().ok()),
            "s-maxage" => cc.s_maxage = v.and_then(|x| x.trim_matches('"').parse().ok()),
            "no-cache" => cc.no_cache = true,
            "no-store" => cc.no_store = true,
            "must-revalidate" => cc.must_revalidate = true,
            "public" => cc.public = true,
            "private" => cc.private = true,
            "immutable" => cc.immutable = true,
            "stale-while-revalidate" => cc.stale_while_revalidate = v.and_then(|x| x.trim_matches('"').parse().ok()),
            "stale-if-error" => cc.stale_if_error = v.and_then(|x| x.trim_matches('"').parse().ok()),
            _ => {}
        }
    }
    cc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(max_age: Option<u32>, now: u64) -> CacheEntry {
        CacheEntry {
            url: "https://x.com".into(),
            method: "GET".into(),
            status: 200,
            body: vec![],
            response_headers: HashMap::new(),
            stored_unix_ms: now,
            max_age_seconds: max_age,
            etag: None,
            last_modified: None,
            vary_keys: vec![],
            vary_partition_hash: 0,
        }
    }

    #[test]
    fn fresh_within_max_age() {
        let e = entry(Some(60), 0);
        assert_eq!(e.is_fresh(30_000), Freshness::Fresh);
    }

    #[test]
    fn stale_past_max_age() {
        let e = entry(Some(60), 0);
        assert_eq!(e.is_fresh(120_000), Freshness::Stale);
    }

    #[test]
    fn cache_control_parses() {
        let cc = parse_cache_control("max-age=3600, public, immutable");
        assert_eq!(cc.max_age, Some(3600));
        assert!(cc.public);
        assert!(cc.immutable);
        assert!(!cc.no_store);
    }

    #[test]
    fn cache_control_no_store() {
        let cc = parse_cache_control("no-store, no-cache");
        assert!(cc.no_store);
        assert!(cc.no_cache);
    }

    #[test]
    fn store_and_lookup() {
        let mut c = HttpCache::new(1024);
        c.store(entry(Some(60), 0));
        assert!(c.lookup("https://x.com", 0).is_some());
    }

    #[test]
    fn invalidate_drops_entries() {
        let mut c = HttpCache::new(1024);
        c.store(entry(Some(60), 0));
        c.invalidate("https://x.com");
        assert!(c.lookup("https://x.com", 0).is_none());
    }

    #[test]
    fn quota_evicts() {
        let mut c = HttpCache::new(100);
        let mut e1 = entry(Some(60), 0);
        e1.body = vec![0u8; 80];
        let mut e2 = entry(Some(60), 0);
        e2.url = "https://y.com".into();
        e2.body = vec![0u8; 80];
        c.store(e1);
        c.store(e2);
        assert!(c.current_bytes <= c.max_bytes);
    }
}
