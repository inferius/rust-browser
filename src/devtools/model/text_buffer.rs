//! Sjednoceny text edit interface. Vsechna textova vstupni pole (devtools console,
//! elements search, inline edit, page form input, address bar, find bar) implementuji
//! `TextBuffer` -> jeden centralni dispatch_text_key + dispatch_text_click v render
//! handleru, zadny duplicitni Backspace/Space/Char match per pole.
//!
//! Cursor a anchor jsou byte offsety v textu (snap na char boundary). Anchor =
//! Some(x) -> selection range (anchor, cursor); None -> caret only.

use std::ops::Range;

/// Primitiva, default impls pokryvaji insert/backspace/move atd.
pub trait TextBuffer {
    fn text(&self) -> &str;
    fn cursor(&self) -> usize;
    /// Nastavi cursor na byte offset, snap na char boundary.
    fn set_cursor(&mut self, byte: usize);
    fn anchor(&self) -> Option<usize>;
    fn set_anchor(&mut self, byte: Option<usize>);
    /// Nahrad range textem. Implementor smazat range a vlozit `with` na pozici start.
    fn replace_range(&mut self, range: Range<usize>, with: &str);

    // ─── Default odvozene operace ──────────────────────────────────

    fn has_selection(&self) -> bool {
        self.anchor().map(|a| a != self.cursor()).unwrap_or(false)
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        let a = self.anchor()?;
        let c = self.cursor();
        if a == c { return None; }
        Some((a.min(c), a.max(c)))
    }

    fn clear_selection(&mut self) {
        self.set_anchor(None);
    }

    fn insert(&mut self, s: &str) {
        if let Some((a, b)) = self.selection_range() {
            self.replace_range(a..b, s);
            self.set_cursor(a + s.len());
        } else {
            let c = self.cursor();
            self.replace_range(c..c, s);
            self.set_cursor(c + s.len());
        }
        self.clear_selection();
    }

    fn backspace(&mut self) {
        if let Some((a, b)) = self.selection_range() {
            self.replace_range(a..b, "");
            self.set_cursor(a);
            self.clear_selection();
            return;
        }
        let c = self.cursor();
        if c == 0 { return; }
        let new_c = prev_char_boundary(self.text(), c);
        self.replace_range(new_c..c, "");
        self.set_cursor(new_c);
    }

    fn delete_forward(&mut self) {
        if let Some((a, b)) = self.selection_range() {
            self.replace_range(a..b, "");
            self.set_cursor(a);
            self.clear_selection();
            return;
        }
        let c = self.cursor();
        let len = self.text().len();
        if c >= len { return; }
        let next = next_char_boundary(self.text(), c);
        self.replace_range(c..next, "");
    }

    fn move_left(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            let (a, _) = self.selection_range().unwrap();
            self.set_cursor(a);
            self.clear_selection();
            return;
        }
        if extend && self.anchor().is_none() {
            let c = self.cursor();
            self.set_anchor(Some(c));
        }
        if !extend { self.clear_selection(); }
        let new_c = prev_char_boundary(self.text(), self.cursor());
        self.set_cursor(new_c);
    }

    fn move_right(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            let (_, b) = self.selection_range().unwrap();
            self.set_cursor(b);
            self.clear_selection();
            return;
        }
        if extend && self.anchor().is_none() {
            let c = self.cursor();
            self.set_anchor(Some(c));
        }
        if !extend { self.clear_selection(); }
        let new_c = next_char_boundary(self.text(), self.cursor());
        self.set_cursor(new_c);
    }

    fn move_home(&mut self, extend: bool) {
        if extend && self.anchor().is_none() {
            let c = self.cursor();
            self.set_anchor(Some(c));
        }
        if !extend { self.clear_selection(); }
        self.set_cursor(0);
    }

    fn move_end(&mut self, extend: bool) {
        if extend && self.anchor().is_none() {
            let c = self.cursor();
            self.set_anchor(Some(c));
        }
        if !extend { self.clear_selection(); }
        let len = self.text().len();
        self.set_cursor(len);
    }

    fn select_all(&mut self) {
        self.set_anchor(Some(0));
        let len = self.text().len();
        self.set_cursor(len);
    }

    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection_range()?;
        Some(self.text()[a..b].to_string())
    }

    fn cut(&mut self) -> Option<String> {
        let s = self.selected_text()?;
        if let Some((a, b)) = self.selection_range() {
            self.replace_range(a..b, "");
            self.set_cursor(a);
            self.clear_selection();
        }
        Some(s)
    }

    /// Pripoj k textu (na konec) - vyuzite kdyz nektere implementace nemaji
    /// cursor (jen append-only, napr. find_query historicky byl takovy).
    fn append(&mut self, s: &str) {
        let len = self.text().len();
        self.replace_range(len..len, s);
        self.set_cursor(len + s.len());
    }
}

pub fn prev_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx == 0 { return 0; }
    idx -= 1;
    while idx > 0 && !s.is_char_boundary(idx) { idx -= 1; }
    idx
}

pub fn next_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() { return s.len(); }
    idx += 1;
    while idx < s.len() && !s.is_char_boundary(idx) { idx += 1; }
    idx
}

// ─── SimpleStringBuffer: wraps String + cursor/anchor ─────────────────────
//
// Pouzite pro elements search query, address bar input, find query.
// Pred trait extraction byly tyhle pole jen `String` + ad-hoc push/pop. Ted
// dostavaji full text edit feature (cursor / selection / Home/End / Shift+Arrow
// pres centralni dispatch).

#[derive(Debug, Clone, Default)]
pub struct SimpleStringBuffer {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
}

impl SimpleStringBuffer {
    pub fn new() -> Self { Self::default() }
    pub fn with_text(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor, anchor: None }
    }
    /// Init s textem + full selection (anchor=0, cursor=end). Prvni typed char
    /// pretrhne selection a nahradi cely text. Address bar UX.
    pub fn with_text_selected(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor, anchor: Some(0) }
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
    }
}

impl TextBuffer for SimpleStringBuffer {
    fn text(&self) -> &str { &self.text }
    fn cursor(&self) -> usize { self.cursor }
    fn set_cursor(&mut self, byte: usize) {
        let mut i = byte.min(self.text.len());
        while i > 0 && !self.text.is_char_boundary(i) { i -= 1; }
        self.cursor = i;
    }
    fn anchor(&self) -> Option<usize> { self.anchor }
    fn set_anchor(&mut self, byte: Option<usize>) {
        self.anchor = byte.map(|b| {
            let mut i = b.min(self.text.len());
            while i > 0 && !self.text.is_char_boundary(i) { i -= 1; }
            i
        });
    }
    fn replace_range(&mut self, range: Range<usize>, with: &str) {
        self.text.replace_range(range, with);
    }
}
