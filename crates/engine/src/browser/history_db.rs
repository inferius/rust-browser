//! Browser history database - visit log + URL ranking.
//!
//! Tracks per-URL visits with frecency score (frequency + recency).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VisitRecord {
    pub url: String,
    pub title: String,
    pub visit_count: u32,
    pub typed_count: u32,            // user-typed (vs clicked) - higher rank
    pub last_visit_unix_ms: u64,
    pub favicon_url: Option<String>,
    pub hidden: bool,
}

impl VisitRecord {
    /// Firefox-style frecency: log(visits) + recency boost.
    pub fn frecency(&self, now_unix_ms: u64) -> f64 {
        if self.hidden { return 0.0; }
        let visits = (self.visit_count as f64).max(1.0);
        let typed_boost = (self.typed_count as f64).log2().max(0.0) * 10.0;
        let age_days = (now_unix_ms.saturating_sub(self.last_visit_unix_ms)) as f64 / 86_400_000.0;
        let recency_factor = (1.0 / (1.0 + age_days * 0.1)).max(0.1);
        visits.log2() * 50.0 + typed_boost + recency_factor * 100.0
    }
}

#[derive(Default)]
pub struct HistoryDb {
    /// URL -> record.
    pub records: HashMap<String, VisitRecord>,
    pub max_entries: usize,
}

impl HistoryDb {
    pub fn new() -> Self {
        Self { max_entries: 100_000, ..Self::default() }
    }

    pub fn record_visit(&mut self, url: &str, title: &str, typed: bool, now: u64) {
        let entry = self.records.entry(url.into()).or_insert(VisitRecord {
            url: url.into(), title: title.into(),
            visit_count: 0, typed_count: 0,
            last_visit_unix_ms: now,
            favicon_url: None, hidden: false,
        });
        entry.visit_count += 1;
        if typed { entry.typed_count += 1; }
        entry.last_visit_unix_ms = now;
        if !title.is_empty() { entry.title = title.into(); }
        self.evict_if_needed();
    }

    fn evict_if_needed(&mut self) {
        if self.records.len() <= self.max_entries { return; }
        // Drop oldest, lowest-frecency entries until under cap.
        let now = self.records.values().map(|r| r.last_visit_unix_ms).max().unwrap_or(0);
        let mut candidates: Vec<(String, f64)> = self.records.iter().map(|(k, v)| (k.clone(), v.frecency(now))).collect();
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        while self.records.len() > self.max_entries {
            if let Some((k, _)) = candidates.pop() { /* don't evict best */ let _ = k; }
            let (k, _) = candidates.remove(0);
            self.records.remove(&k);
        }
    }

    /// Search by query against title + URL.
    pub fn search(&self, query: &str, now: u64) -> Vec<&VisitRecord> {
        let q = query.to_ascii_lowercase();
        let mut results: Vec<&VisitRecord> = self.records.values()
            .filter(|r| !r.hidden &&
                       (r.title.to_ascii_lowercase().contains(&q) || r.url.to_ascii_lowercase().contains(&q)))
            .collect();
        results.sort_by(|a, b| b.frecency(now).partial_cmp(&a.frecency(now)).unwrap());
        results
    }

    pub fn delete(&mut self, url: &str) -> bool {
        self.records.remove(url).is_some()
    }

    pub fn clear(&mut self) { self.records.clear(); }

    pub fn delete_since(&mut self, since_unix_ms: u64) {
        self.records.retain(|_, v| v.last_visit_unix_ms < since_unix_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_increments_visits() {
        let mut h = HistoryDb::new();
        h.record_visit("https://x.com", "X", false, 1000);
        h.record_visit("https://x.com", "X", false, 2000);
        assert_eq!(h.records["https://x.com"].visit_count, 2);
    }

    #[test]
    fn typed_count_separate() {
        let mut h = HistoryDb::new();
        h.record_visit("https://x.com", "X", true, 1000);
        h.record_visit("https://x.com", "X", false, 2000);
        assert_eq!(h.records["https://x.com"].typed_count, 1);
    }

    #[test]
    fn search_returns_matches() {
        let mut h = HistoryDb::new();
        h.record_visit("https://example.com", "Example", false, 0);
        h.record_visit("https://x.com", "X", false, 0);
        assert_eq!(h.search("example", 0).len(), 1);
    }

    #[test]
    fn delete_since() {
        let mut h = HistoryDb::new();
        h.record_visit("https://a", "A", false, 100);
        h.record_visit("https://b", "B", false, 1000);
        h.delete_since(500);
        assert!(h.records.contains_key("https://a"));
        assert!(!h.records.contains_key("https://b"));
    }

    #[test]
    fn frecency_ranking() {
        let mut h = HistoryDb::new();
        h.record_visit("https://old", "old", false, 1);
        for _ in 0..10 {
            h.record_visit("https://recent", "recent", true, 1_000_000_000);
        }
        let results = h.search("", 1_000_000_000);
        // "recent" should rank higher
        assert_eq!(results[0].url, "https://recent");
    }
}
