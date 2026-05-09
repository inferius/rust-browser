//! Multi-tab state pro shell mode. Kazdy tab drzi vlastni Document URL +
//! scroll state + history. Pri switch_to(idx) se aktivni tab nahraje a
//! zbyle se zachovaji (deferred load - kazdy tab ma svuj snapshot html/css).
//!
//! Pro minimum viable: tab zna jen URL/path + cached html/css. Aktivni
//! tab se loaduje pri switch (potrebuje re-parse). Future: per-tab
//! interpreter + document instance + layout cache pro fast switch.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Tab {
    pub url: Option<String>,
    pub path: Option<PathBuf>,
    pub html: String,
    pub css: String,
    pub title: String,
    pub scroll_y: f32,
    pub scroll_x: f32,
    pub history: Vec<String>,
    pub history_idx: usize,
}

impl Tab {
    pub fn new(html: String, css: String, url: Option<String>, path: Option<PathBuf>) -> Self {
        let title = url.clone()
            .map(|u| u.split('/').last().unwrap_or(&u).to_string())
            .unwrap_or_else(|| "Nova zalozka".to_string());
        Self {
            url, path, html, css, title,
            scroll_y: 0.0, scroll_x: 0.0,
            history: Vec::new(),
            history_idx: 0,
        }
    }

    pub fn empty() -> Self {
        Self {
            url: None,
            path: None,
            html: "<html><body><h1>Nova zalozka</h1></body></html>".to_string(),
            css: String::new(),
            title: "Nova zalozka".to_string(),
            scroll_y: 0.0, scroll_x: 0.0,
            history: Vec::new(),
            history_idx: 0,
        }
    }
}

#[derive(Debug)]
pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl Default for TabManager {
    fn default() -> Self {
        Self { tabs: vec![Tab::empty()], active: 0 }
    }
}

impl TabManager {
    pub fn new(initial: Tab) -> Self {
        Self { tabs: vec![initial], active: 0 }
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.tabs.len() { self.active = idx; }
    }

    pub fn open(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    pub fn close(&mut self, idx: usize) {
        if self.tabs.len() <= 1 { return; }
        if idx >= self.tabs.len() { return; }
        self.tabs.remove(idx);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if self.active > idx {
            self.active -= 1;
        }
    }

    pub fn next(&mut self) {
        if self.tabs.len() > 0 {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn prev(&mut self) {
        if self.tabs.len() > 0 {
            self.active = if self.active == 0 { self.tabs.len() - 1 } else { self.active - 1 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_default_empty_je_jeden_tab() {
        let m = TabManager::default();
        assert_eq!(m.tabs.len(), 1);
        assert_eq!(m.active, 0);
    }

    #[test]
    fn tab_open_zvysi_active() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        assert_eq!(m.tabs.len(), 2);
        assert_eq!(m.active, 1);
    }

    #[test]
    fn tab_close_active_posune_active_dolu() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        assert_eq!(m.active, 2);
        m.close(2);
        assert_eq!(m.tabs.len(), 2);
        assert_eq!(m.active, 1);
    }

    #[test]
    fn tab_close_neumozni_jediny_tab() {
        let mut m = TabManager::default();
        m.close(0);
        assert_eq!(m.tabs.len(), 1, "Posledni tab nelze zavrit");
    }

    #[test]
    fn tab_next_wraparound() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        m.switch_to(0);
        m.next();
        assert_eq!(m.active, 1);
        m.next();
        assert_eq!(m.active, 2);
        m.next();
        assert_eq!(m.active, 0, "Wraparound");
    }

    #[test]
    fn tab_prev_wraparound() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.switch_to(0);
        m.prev();
        assert_eq!(m.active, 1, "Wrap z 0 na last");
    }

    #[test]
    fn tab_close_after_active_neposune_active() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        m.switch_to(1);
        m.close(2);
        assert_eq!(m.active, 1, "Close vyssi nez active = beze zmeny");
    }

    #[test]
    fn tab_close_below_active_posune_active() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        m.switch_to(2);
        m.close(0);
        assert_eq!(m.active, 1, "Close pod active = active - 1");
    }
}
