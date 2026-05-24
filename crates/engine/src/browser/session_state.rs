//! Session restore - tabs + scroll positions saved across restarts.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SavedTab {
    pub tab_index: u32,
    pub url: String,
    pub title: String,
    pub history: Vec<SavedHistoryEntry>,
    pub history_index: usize,
    pub scroll_y: f32,
    pub pinned: bool,
    pub muted: bool,
    pub group_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SavedHistoryEntry {
    pub url: String,
    pub title: String,
    pub scroll_y: f32,
    pub form_state: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SavedWindow {
    pub window_index: u32,
    pub focused_tab_index: u32,
    pub tabs: Vec<SavedTab>,
    pub bounds: (i32, i32, u32, u32),  // x, y, w, h on display
    pub maximized: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionSnapshot {
    pub windows: Vec<SavedWindow>,
    pub saved_at_unix_ms: u64,
    pub last_active_window: u32,
}

impl SessionSnapshot {
    pub fn new() -> Self { Self::default() }

    /// Add a window snapshot.
    pub fn record_window(&mut self, w: SavedWindow) {
        self.windows.push(w);
    }

    /// Restore a single tab by (window_idx, tab_idx).
    pub fn get_tab(&self, window_idx: u32, tab_idx: u32) -> Option<&SavedTab> {
        let w = self.windows.iter().find(|w| w.window_index == window_idx)?;
        w.tabs.iter().find(|t| t.tab_index == tab_idx)
    }

    /// Compute restore size (number of tabs total).
    pub fn tab_count(&self) -> usize {
        self.windows.iter().map(|w| w.tabs.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(idx: u32, url: &str) -> SavedTab {
        SavedTab {
            tab_index: idx, url: url.into(), title: "".into(),
            history: Vec::new(), history_index: 0, scroll_y: 0.0,
            pinned: false, muted: false, group_id: None,
        }
    }

    #[test]
    fn record_and_get() {
        let mut s = SessionSnapshot::new();
        let w = SavedWindow {
            window_index: 0, focused_tab_index: 0,
            tabs: vec![tab(0, "https://x.com"), tab(1, "https://y.com")],
            bounds: (0, 0, 1280, 800), maximized: false,
        };
        s.record_window(w);
        assert!(s.get_tab(0, 1).is_some());
        assert_eq!(s.tab_count(), 2);
    }

    #[test]
    fn history_index_within_bounds() {
        let mut t = tab(0, "https://x.com");
        t.history = vec![
            SavedHistoryEntry { url: "https://a".into(), title: "A".into(), scroll_y: 0.0, form_state: HashMap::new() },
            SavedHistoryEntry { url: "https://b".into(), title: "B".into(), scroll_y: 0.0, form_state: HashMap::new() },
        ];
        t.history_index = 1;
        assert!(t.history_index < t.history.len());
    }
}
