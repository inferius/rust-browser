//! Crash reporter foundation - structured crash dumps + upload queue.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrashKind {
    BrowserMainProcess,
    Renderer,
    GpuProcess,
    UtilityProcess,
    AudioService,
    NetworkService,
}

#[derive(Debug, Clone)]
pub struct CrashReport {
    pub id: u64,
    pub kind: CrashKind,
    pub process_id: u32,
    pub thread_id: u32,
    pub signal: Option<String>,            // SIGSEGV / EXCEPTION_ACCESS_VIOLATION
    pub stack_hash: String,
    pub url: Option<String>,
    pub channel: String,                   // stable / beta / dev / canary
    pub version: String,
    pub timestamp_unix_ms: u64,
    pub annotations: HashMap<String, String>,
    pub minidump_size_bytes: u64,
    pub uploaded: bool,
}

#[derive(Default)]
pub struct CrashStore {
    pub reports: Vec<CrashReport>,
    pub upload_consent: bool,
    pub max_pending: usize,
}

impl CrashStore {
    pub fn new() -> Self {
        Self { max_pending: 100, ..Self::default() }
    }

    pub fn record(&mut self, report: CrashReport) {
        self.reports.push(report);
        while self.reports.len() > self.max_pending {
            self.reports.remove(0);
        }
    }

    pub fn next_for_upload(&mut self) -> Option<&mut CrashReport> {
        if !self.upload_consent { return None; }
        self.reports.iter_mut().find(|r| !r.uploaded)
    }

    pub fn mark_uploaded(&mut self, id: u64) {
        if let Some(r) = self.reports.iter_mut().find(|r| r.id == id) {
            r.uploaded = true;
        }
    }

    pub fn pending_count(&self) -> usize {
        self.reports.iter().filter(|r| !r.uploaded).count()
    }

    /// Group by stack_hash to find frequent crashers.
    pub fn group_by_signature(&self) -> HashMap<String, u32> {
        let mut out = HashMap::new();
        for r in &self.reports {
            *out.entry(r.stack_hash.clone()).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: u64, hash: &str) -> CrashReport {
        CrashReport {
            id, kind: CrashKind::Renderer, process_id: 1, thread_id: 1,
            signal: Some("SIGSEGV".into()),
            stack_hash: hash.into(),
            url: None,
            channel: "stable".into(),
            version: "1.0".into(),
            timestamp_unix_ms: 0,
            annotations: HashMap::new(),
            minidump_size_bytes: 1024,
            uploaded: false,
        }
    }

    #[test]
    fn record_appends() {
        let mut s = CrashStore::new();
        s.record(report(1, "hash1"));
        assert_eq!(s.reports.len(), 1);
    }

    #[test]
    fn capped_at_max() {
        let mut s = CrashStore::new();
        s.max_pending = 3;
        for i in 0..10 { s.record(report(i, "h")); }
        assert!(s.reports.len() <= 3);
    }

    #[test]
    fn upload_requires_consent() {
        let mut s = CrashStore::new();
        s.record(report(1, "h"));
        assert!(s.next_for_upload().is_none());
        s.upload_consent = true;
        assert!(s.next_for_upload().is_some());
    }

    #[test]
    fn mark_uploaded() {
        let mut s = CrashStore::new();
        s.upload_consent = true;
        s.record(report(1, "h"));
        s.mark_uploaded(1);
        assert_eq!(s.pending_count(), 0);
    }

    #[test]
    fn group_by_hash() {
        let mut s = CrashStore::new();
        s.record(report(1, "h1"));
        s.record(report(2, "h1"));
        s.record(report(3, "h2"));
        let groups = s.group_by_signature();
        assert_eq!(groups.get("h1"), Some(&2));
        assert_eq!(groups.get("h2"), Some(&1));
    }
}
