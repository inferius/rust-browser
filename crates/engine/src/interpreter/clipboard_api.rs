//! Clipboard API - read/write text, HTML, image.
//!
//! Spec: https://w3c.github.io/clipboard-apis/
//! Foundation pres existing arboard. ClipboardItem multi-format support.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ClipboardItem {
    /// MIME -> bytes (text/plain, text/html, image/png).
    pub formats: HashMap<String, Vec<u8>>,
}

impl ClipboardItem {
    pub fn new() -> Self { Self { formats: HashMap::new() } }
    pub fn with_text(text: &str) -> Self {
        let mut item = Self::new();
        item.formats.insert("text/plain".into(), text.as_bytes().to_vec());
        item
    }
    pub fn with_html(html: &str) -> Self {
        let mut item = Self::new();
        item.formats.insert("text/html".into(), html.as_bytes().to_vec());
        item
    }
    pub fn types(&self) -> Vec<String> {
        self.formats.keys().cloned().collect()
    }
    pub fn get(&self, mime: &str) -> Option<&[u8]> {
        self.formats.get(mime).map(|v| v.as_slice())
    }
}

impl Default for ClipboardItem {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipboardPermission {
    Default,
    Granted,
    Denied,
}

#[derive(Default)]
pub struct ClipboardManager {
    pub current: Option<ClipboardItem>,
    pub read_permission: ClipboardPermission,
    pub write_permission: ClipboardPermission,
}

impl ClipboardManager {
    pub fn new() -> Self { Self::default() }

    pub fn write(&mut self, item: ClipboardItem) -> bool {
        if self.write_permission == ClipboardPermission::Denied { return false; }
        self.current = Some(item);
        true
    }

    pub fn write_text(&mut self, text: &str) -> bool {
        self.write(ClipboardItem::with_text(text))
    }

    pub fn read(&self) -> Option<&ClipboardItem> {
        if self.read_permission == ClipboardPermission::Denied { return None; }
        self.current.as_ref()
    }

    pub fn read_text(&self) -> Option<String> {
        let item = self.read()?;
        let bytes = item.get("text/plain")?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    pub fn grant_read(&mut self) { self.read_permission = ClipboardPermission::Granted; }
    pub fn grant_write(&mut self) { self.write_permission = ClipboardPermission::Granted; }
}

impl Default for ClipboardPermission {
    fn default() -> Self { ClipboardPermission::Default }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_text() {
        let mut m = ClipboardManager::new();
        m.grant_write();
        m.grant_read();
        m.write_text("hello");
        assert_eq!(m.read_text().as_deref(), Some("hello"));
    }

    #[test]
    fn multi_format_item() {
        let mut item = ClipboardItem::with_text("hi");
        item.formats.insert("text/html".into(), b"<b>hi</b>".to_vec());
        assert_eq!(item.types().len(), 2);
        assert_eq!(item.get("text/html").unwrap(), b"<b>hi</b>");
    }

    #[test]
    fn denied_blocks_write() {
        let mut m = ClipboardManager::new();
        m.write_permission = ClipboardPermission::Denied;
        assert!(!m.write_text("x"));
    }

    #[test]
    fn denied_blocks_read() {
        let mut m = ClipboardManager::new();
        m.grant_write();
        m.write_text("x");
        m.read_permission = ClipboardPermission::Denied;
        assert!(m.read_text().is_none());
    }
}
