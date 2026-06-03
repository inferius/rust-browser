//! WebDriver classic + BiDi protocol foundation.
//!
//! Specs:
//! - WebDriver Classic: https://www.w3.org/TR/webdriver2/
//! - WebDriver BiDi: https://w3c.github.io/webdriver-bidi/

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WebDriverSession {
    pub session_id: String,
    pub browser_name: String,
    pub browser_version: String,
    pub platform_name: String,
    pub accept_insecure_certs: bool,
    pub page_load_strategy: PageLoadStrategy,
    pub strict_file_interactability: bool,
    pub timeouts: SessionTimeouts,
    pub unhandled_prompt_behavior: PromptBehavior,
    pub current_window_handle: String,
    pub windows: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageLoadStrategy {
    None,
    Eager,           // DOMContentLoaded
    Normal,          // load event
}

impl Default for PageLoadStrategy { fn default() -> Self { PageLoadStrategy::Normal } }

#[derive(Debug, Clone)]
pub struct SessionTimeouts {
    pub script_ms: u64,
    pub page_load_ms: u64,
    pub implicit_wait_ms: u64,
}

impl Default for SessionTimeouts {
    fn default() -> Self {
        Self { script_ms: 30_000, page_load_ms: 300_000, implicit_wait_ms: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromptBehavior {
    Dismiss,
    Accept,
    DismissAndNotify,
    AcceptAndNotify,
    Ignore,
}

impl Default for PromptBehavior { fn default() -> Self { PromptBehavior::DismissAndNotify } }

#[derive(Default)]
pub struct WebDriverServer {
    pub sessions: HashMap<String, WebDriverSession>,
    pub next_session_idx: u64,
}

impl WebDriverServer {
    pub fn new() -> Self { Self::default() }

    pub fn new_session(&mut self, browser: &str, version: &str, platform: &str) -> String {
        self.next_session_idx += 1;
        let id = format!("sess-{:016x}", self.next_session_idx);
        let win_id = format!("win-{:016x}", self.next_session_idx);
        let session = WebDriverSession {
            session_id: id.clone(),
            browser_name: browser.into(),
            browser_version: version.into(),
            platform_name: platform.into(),
            accept_insecure_certs: false,
            page_load_strategy: PageLoadStrategy::Normal,
            strict_file_interactability: false,
            timeouts: SessionTimeouts::default(),
            unhandled_prompt_behavior: PromptBehavior::DismissAndNotify,
            current_window_handle: win_id.clone(),
            windows: vec![win_id],
        };
        self.sessions.insert(id.clone(), session);
        id
    }

    pub fn delete_session(&mut self, id: &str) -> bool {
        self.sessions.remove(id).is_some()
    }

    pub fn switch_window(&mut self, session_id: &str, handle: &str) -> Result<(), String> {
        let s = self.sessions.get_mut(session_id).ok_or("no session")?;
        if !s.windows.iter().any(|w| w == handle) {
            return Err("window not found".into());
        }
        s.current_window_handle = handle.into();
        Ok(())
    }

    pub fn new_window(&mut self, session_id: &str) -> Result<String, String> {
        let s = self.sessions.get_mut(session_id).ok_or("no session")?;
        self.next_session_idx += 1;
        let handle = format!("win-{:016x}", self.next_session_idx);
        s.windows.push(handle.clone());
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_returns_unique_id() {
        let mut s = WebDriverServer::new();
        let a = s.new_session("chrome", "1", "linux");
        let b = s.new_session("chrome", "1", "linux");
        assert_ne!(a, b);
    }

    #[test]
    fn delete_session() {
        let mut s = WebDriverServer::new();
        let id = s.new_session("c", "1", "lin");
        assert!(s.delete_session(&id));
        assert!(!s.delete_session(&id));
    }

    #[test]
    fn new_window_appends() {
        let mut s = WebDriverServer::new();
        let id = s.new_session("c", "1", "lin");
        let win = s.new_window(&id).unwrap();
        assert!(s.sessions[&id].windows.contains(&win));
    }

    #[test]
    fn switch_window() {
        let mut s = WebDriverServer::new();
        let id = s.new_session("c", "1", "lin");
        let win = s.new_window(&id).unwrap();
        s.switch_window(&id, &win).unwrap();
        assert_eq!(s.sessions[&id].current_window_handle, win);
    }

    #[test]
    fn switch_unknown_window_fails() {
        let mut s = WebDriverServer::new();
        let id = s.new_session("c", "1", "lin");
        assert!(s.switch_window(&id, "missing").is_err());
    }
}
