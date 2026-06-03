//! Download manager - track in-progress + completed downloads.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DownloadState {
    Pending,
    InProgress,
    Paused,
    Cancelled,
    Failed,
    Completed,
}

#[derive(Debug, Clone)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub suggested_filename: String,
    pub final_path: Option<String>,
    pub mime: String,
    pub state: DownloadState,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub started_unix_ms: u64,
    pub completed_unix_ms: Option<u64>,
    pub failure_reason: Option<String>,
    pub from_referrer: Option<String>,
    pub source_origin: String,
}

#[derive(Default)]
pub struct DownloadManager {
    pub downloads: HashMap<u64, Download>,
    pub next_id: u64,
}

impl DownloadManager {
    pub fn new() -> Self { Self::default() }

    pub fn start(&mut self, url: &str, suggested: &str, mime: &str, source_origin: &str, now: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.downloads.insert(id, Download {
            id, url: url.into(),
            suggested_filename: suggested.into(),
            final_path: None,
            mime: mime.into(),
            state: DownloadState::Pending,
            bytes_received: 0, total_bytes: None,
            started_unix_ms: now,
            completed_unix_ms: None,
            failure_reason: None,
            from_referrer: None,
            source_origin: source_origin.into(),
        });
        id
    }

    pub fn set_total_bytes(&mut self, id: u64, total: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            d.total_bytes = Some(total);
            d.state = DownloadState::InProgress;
        }
    }

    pub fn record_progress(&mut self, id: u64, bytes: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            d.bytes_received += bytes;
            if let Some(total) = d.total_bytes {
                if d.bytes_received >= total {
                    d.state = DownloadState::InProgress;
                }
            }
        }
    }

    pub fn pause(&mut self, id: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            if d.state == DownloadState::InProgress { d.state = DownloadState::Paused; }
        }
    }

    pub fn resume(&mut self, id: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            if d.state == DownloadState::Paused { d.state = DownloadState::InProgress; }
        }
    }

    pub fn cancel(&mut self, id: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            d.state = DownloadState::Cancelled;
        }
    }

    pub fn complete(&mut self, id: u64, final_path: &str, now: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            d.state = DownloadState::Completed;
            d.final_path = Some(final_path.into());
            d.completed_unix_ms = Some(now);
        }
    }

    pub fn fail(&mut self, id: u64, reason: &str, now: u64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            d.state = DownloadState::Failed;
            d.failure_reason = Some(reason.into());
            d.completed_unix_ms = Some(now);
        }
    }

    pub fn progress_ratio(&self, id: u64) -> Option<f32> {
        let d = self.downloads.get(&id)?;
        let total = d.total_bytes?;
        if total == 0 { return Some(0.0); }
        Some((d.bytes_received as f32 / total as f32).clamp(0.0, 1.0))
    }
}

/// Sanitize a Content-Disposition filename or URL-derived name.
pub fn sanitize_filename(name: &str) -> String {
    let mut s: String = name.chars().map(|c| match c {
        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0' => '_',
        c if (c as u32) < 0x20 => '_',
        c => c,
    }).collect();
    // Strip trailing dots + spaces (Windows-incompatible).
    while s.ends_with('.') || s.ends_with(' ') { s.pop(); }
    if s.is_empty() { s = "download".into(); }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_returns_id() {
        let mut m = DownloadManager::new();
        let id = m.start("https://x.com/f.zip", "f.zip", "application/zip", "https://x.com", 0);
        assert!(m.downloads.contains_key(&id));
    }

    #[test]
    fn progress_tracks() {
        let mut m = DownloadManager::new();
        let id = m.start("u", "f", "m", "o", 0);
        m.set_total_bytes(id, 1000);
        m.record_progress(id, 500);
        assert_eq!(m.progress_ratio(id), Some(0.5));
    }

    #[test]
    fn pause_and_resume() {
        let mut m = DownloadManager::new();
        let id = m.start("u", "f", "m", "o", 0);
        m.set_total_bytes(id, 1000);
        m.pause(id);
        assert_eq!(m.downloads[&id].state, DownloadState::Paused);
        m.resume(id);
        assert_eq!(m.downloads[&id].state, DownloadState::InProgress);
    }

    #[test]
    fn complete_sets_path() {
        let mut m = DownloadManager::new();
        let id = m.start("u", "f", "m", "o", 0);
        m.complete(id, "/tmp/f", 1000);
        assert_eq!(m.downloads[&id].final_path.as_deref(), Some("/tmp/f"));
        assert_eq!(m.downloads[&id].state, DownloadState::Completed);
    }

    #[test]
    fn sanitize_strips_disallowed() {
        let s = sanitize_filename("a/b\\c:d|e.txt");
        assert!(!s.contains('/'));
        assert!(!s.contains('\\'));
    }

    #[test]
    fn sanitize_empty_returns_default() {
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("...   "), "download");
    }
}
