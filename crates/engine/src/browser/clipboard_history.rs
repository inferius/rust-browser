//! Clipboard history - recent copies, formatted types, per-app provenance.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipboardFormat {
    Text,
    Html,
    Rtf,
    Markdown,
    Image,
    Files,
}

#[derive(Debug, Clone)]
pub struct ClipboardItem {
    pub id: u64,
    pub format: ClipboardFormat,
    pub text: Option<String>,
    pub html: Option<String>,
    pub data_bytes: Option<Vec<u8>>,
    pub mime: Option<String>,
    pub source_app: Option<String>,
    pub timestamp_unix_ms: u64,
    pub redacted: bool,                  // password manager fills set this
}

#[derive(Default)]
pub struct ClipboardHistory {
    pub items: VecDeque<ClipboardItem>,
    pub max_items: usize,
    pub next_id: u64,
}

impl ClipboardHistory {
    pub fn new() -> Self {
        Self { max_items: 25, ..Self::default() }
    }

    pub fn push(&mut self, mut item: ClipboardItem) {
        self.next_id += 1;
        item.id = self.next_id;
        // Dedupe: if last item has same text, replace timestamp.
        if let Some(prev) = self.items.front_mut() {
            if prev.text == item.text && prev.format == item.format && !item.redacted {
                prev.timestamp_unix_ms = item.timestamp_unix_ms;
                return;
            }
        }
        self.items.push_front(item);
        while self.items.len() > self.max_items {
            self.items.pop_back();
        }
    }

    pub fn current(&self) -> Option<&ClipboardItem> {
        self.items.front()
    }

    pub fn list(&self) -> impl Iterator<Item = &ClipboardItem> {
        self.items.iter()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn purge_redacted(&mut self) {
        self.items.retain(|i| !i.redacted);
    }

    pub fn search(&self, query: &str) -> Vec<&ClipboardItem> {
        let q = query.to_ascii_lowercase();
        self.items.iter()
            .filter(|i| i.text.as_deref().unwrap_or("").to_ascii_lowercase().contains(&q))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str, ts: u64) -> ClipboardItem {
        ClipboardItem {
            id: 0, format: ClipboardFormat::Text,
            text: Some(text.into()), html: None,
            data_bytes: None, mime: None,
            source_app: None, timestamp_unix_ms: ts,
            redacted: false,
        }
    }

    #[test]
    fn push_and_current() {
        let mut h = ClipboardHistory::new();
        h.push(item("hello", 1));
        assert_eq!(h.current().unwrap().text.as_deref(), Some("hello"));
    }

    #[test]
    fn dedupe_keeps_one() {
        let mut h = ClipboardHistory::new();
        h.push(item("hello", 1));
        h.push(item("hello", 2));
        assert_eq!(h.items.len(), 1);
    }

    #[test]
    fn capped_at_max() {
        let mut h = ClipboardHistory::new();
        h.max_items = 3;
        for i in 0..10 {
            h.push(item(&format!("text{}", i), i));
        }
        assert_eq!(h.items.len(), 3);
    }

    #[test]
    fn purge_redacted() {
        let mut h = ClipboardHistory::new();
        h.push(item("plain", 1));
        let mut r = item("secret", 2);
        r.redacted = true;
        h.push(r);
        h.purge_redacted();
        assert_eq!(h.items.len(), 1);
    }

    #[test]
    fn search_finds() {
        let mut h = ClipboardHistory::new();
        h.push(item("hello world", 1));
        h.push(item("foobar", 2));
        assert_eq!(h.search("world").len(), 1);
        assert_eq!(h.search("foo").len(), 1);
    }
}
