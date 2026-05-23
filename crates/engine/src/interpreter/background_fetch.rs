//! Background Fetch API - resumable, browser-managed downloads.
//!
//! Spec: https://wicg.github.io/background-fetch/
//! navigator.serviceWorker.ready.then(reg => reg.backgroundFetch.fetch(id, requests, opts))

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgFetchResult {
    Pending,
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgFetchFailureReason {
    None,
    Aborted,
    BadStatus,
    FetchError,
    QuotaExceeded,
    DownloadTotalExceeded,
}

#[derive(Debug, Clone)]
pub struct BackgroundFetchRegistration {
    pub id: String,
    pub upload_total: u64,
    pub uploaded: u64,
    pub download_total: u64,
    pub downloaded: u64,
    pub result: BgFetchResult,
    pub failure_reason: BgFetchFailureReason,
    pub recorded_requests: Vec<String>, // URLs
}

impl BackgroundFetchRegistration {
    pub fn new(id: &str, requests: Vec<String>) -> Self {
        Self {
            id: id.into(),
            upload_total: 0,
            uploaded: 0,
            download_total: 0,
            downloaded: 0,
            result: BgFetchResult::Pending,
            failure_reason: BgFetchFailureReason::None,
            recorded_requests: requests,
        }
    }

    pub fn abort(&mut self) {
        self.result = BgFetchResult::Failure;
        self.failure_reason = BgFetchFailureReason::Aborted;
    }

    pub fn record_progress(&mut self, bytes: u64) {
        self.downloaded += bytes;
        if self.download_total > 0 && self.downloaded > self.download_total {
            self.result = BgFetchResult::Failure;
            self.failure_reason = BgFetchFailureReason::DownloadTotalExceeded;
        }
    }

    pub fn complete(&mut self, success: bool) {
        self.result = if success { BgFetchResult::Success } else { BgFetchResult::Failure };
        if !success && self.failure_reason == BgFetchFailureReason::None {
            self.failure_reason = BgFetchFailureReason::FetchError;
        }
    }
}

#[derive(Default)]
pub struct BackgroundFetchManager {
    pub registrations: HashMap<String, BackgroundFetchRegistration>,
}

impl BackgroundFetchManager {
    pub fn new() -> Self { Self::default() }

    pub fn fetch(&mut self, id: &str, requests: Vec<String>) -> Result<&BackgroundFetchRegistration, String> {
        if self.registrations.contains_key(id) {
            return Err(format!("background fetch '{}' already exists", id));
        }
        self.registrations.insert(id.into(), BackgroundFetchRegistration::new(id, requests));
        Ok(self.registrations.get(id).unwrap())
    }

    pub fn get(&self, id: &str) -> Option<&BackgroundFetchRegistration> {
        self.registrations.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut BackgroundFetchRegistration> {
        self.registrations.get_mut(id)
    }

    pub fn get_ids(&self) -> Vec<String> {
        self.registrations.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_creates_registration() {
        let mut m = BackgroundFetchManager::new();
        let reg = m.fetch("dl1", vec!["https://x.com/a.zip".into()]).unwrap();
        assert_eq!(reg.id, "dl1");
        assert_eq!(reg.recorded_requests.len(), 1);
    }

    #[test]
    fn duplicate_rejected() {
        let mut m = BackgroundFetchManager::new();
        m.fetch("x", vec![]).unwrap();
        assert!(m.fetch("x", vec![]).is_err());
    }

    #[test]
    fn abort_sets_failure() {
        let mut m = BackgroundFetchManager::new();
        m.fetch("x", vec![]).unwrap();
        m.get_mut("x").unwrap().abort();
        assert_eq!(m.get("x").unwrap().failure_reason, BgFetchFailureReason::Aborted);
    }

    #[test]
    fn download_total_exceeded() {
        let mut m = BackgroundFetchManager::new();
        m.fetch("x", vec![]).unwrap();
        let r = m.get_mut("x").unwrap();
        r.download_total = 100;
        r.record_progress(150);
        assert_eq!(r.failure_reason, BgFetchFailureReason::DownloadTotalExceeded);
    }

    #[test]
    fn complete_success() {
        let mut m = BackgroundFetchManager::new();
        m.fetch("x", vec![]).unwrap();
        m.get_mut("x").unwrap().complete(true);
        assert_eq!(m.get("x").unwrap().result, BgFetchResult::Success);
    }
}
