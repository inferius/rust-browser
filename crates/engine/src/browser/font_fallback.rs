//! Font fallback chain - per-script + per-codepoint selection.
//!
//! Chromium reference: third_party/blink/renderer/platform/fonts/FontFallbackList.
//! When the primary font doesn't have a glyph for a codepoint, walk the fallback
//! list (system + WebFont @font-face) until one covers it.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontScript {
    Latin,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Devanagari,
    Han,            // CJK Unified Ideographs
    Hiragana,
    Katakana,
    Hangul,
    Thai,
    Emoji,
    Math,
    Symbol,
    Unknown,
}

/// Classify a codepoint to a script. Coarse-grained.
pub fn script_for_codepoint(cp: u32) -> FontScript {
    match cp {
        0x0000..=0x007F | 0x0080..=0x024F => FontScript::Latin,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => FontScript::Greek,
        0x0400..=0x04FF | 0x0500..=0x052F => FontScript::Cyrillic,
        0x0590..=0x05FF | 0xFB1D..=0xFB4F => FontScript::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFC => FontScript::Arabic,
        0x0900..=0x097F => FontScript::Devanagari,
        0x0E00..=0x0E7F => FontScript::Thai,
        0x2200..=0x22FF | 0x27C0..=0x27EF | 0x2980..=0x29FF | 0x2A00..=0x2AFF => FontScript::Math,
        0x3040..=0x309F => FontScript::Hiragana,
        0x30A0..=0x30FF | 0x31F0..=0x31FF => FontScript::Katakana,
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => FontScript::Hangul,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF => FontScript::Han,
        0x1F300..=0x1F9FF | 0x1FA70..=0x1FAFF | 0x2600..=0x26FF | 0x2700..=0x27BF => FontScript::Emoji,
        _ => FontScript::Symbol,
    }
}

#[derive(Debug, Clone)]
pub struct FontEntry {
    pub family: String,
    pub source_path: Option<String>,
    pub supports: Vec<(u32, u32)>,        // codepoint ranges this font covers
    pub priority: i32,                     // higher = preferred
}

impl FontEntry {
    pub fn covers(&self, cp: u32) -> bool {
        self.supports.iter().any(|(lo, hi)| cp >= *lo && cp <= *hi)
    }
}

#[derive(Default)]
pub struct FontFallbackChain {
    /// Per-script fallback lists (default order to try).
    pub by_script: HashMap<FontScript, Vec<FontEntry>>,
    /// Global UA fallback (Last-resort font).
    pub last_resort: Option<FontEntry>,
}

impl FontFallbackChain {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, script: FontScript, font: FontEntry) {
        let list = self.by_script.entry(script).or_default();
        list.push(font);
        list.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    pub fn find_for_codepoint(&self, cp: u32) -> Option<&FontEntry> {
        let script = script_for_codepoint(cp);
        if let Some(list) = self.by_script.get(&script) {
            for f in list { if f.covers(cp) { return Some(f); } }
        }
        // Fall through to last-resort.
        self.last_resort.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, lo: u32, hi: u32) -> FontEntry {
        FontEntry {
            family: name.into(),
            source_path: None,
            supports: vec![(lo, hi)],
            priority: 0,
        }
    }

    #[test]
    fn script_classification() {
        assert_eq!(script_for_codepoint('A' as u32), FontScript::Latin);
        assert_eq!(script_for_codepoint('А' as u32), FontScript::Cyrillic);
        assert_eq!(script_for_codepoint(0x4E2D), FontScript::Han);
        assert_eq!(script_for_codepoint(0x1F600), FontScript::Emoji);
    }

    #[test]
    fn font_entry_covers_range() {
        let f = entry("Latin", 0x0020, 0x007F);
        assert!(f.covers('A' as u32));
        assert!(!f.covers(0x0500));
    }

    #[test]
    fn fallback_picks_matching() {
        let mut chain = FontFallbackChain::new();
        chain.register(FontScript::Latin, entry("Sans", 0x0020, 0x007F));
        let f = chain.find_for_codepoint('B' as u32).unwrap();
        assert_eq!(f.family, "Sans");
    }

    #[test]
    fn fallback_last_resort() {
        let mut chain = FontFallbackChain::new();
        chain.last_resort = Some(entry("LastResort", 0, 0x10FFFF));
        let f = chain.find_for_codepoint(0xE000).unwrap();
        assert_eq!(f.family, "LastResort");
    }

    #[test]
    fn priority_ordering() {
        let mut chain = FontFallbackChain::new();
        let mut hi = entry("Hi", 0x0020, 0x007F); hi.priority = 100;
        let lo = entry("Lo", 0x0020, 0x007F);
        chain.register(FontScript::Latin, lo);
        chain.register(FontScript::Latin, hi);
        let f = chain.find_for_codepoint('A' as u32).unwrap();
        assert_eq!(f.family, "Hi");
    }
}
