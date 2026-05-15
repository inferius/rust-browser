//! Unified text editor model - single source of truth pro caret + selection.
//!
//! Architektura (Phase 1 + 2 z Session N+22b):
//!
//! `shape_text` wrapuje `layout::shape_text_advances` na per-glyph data
//! (byte_offset + advance + cumulative x). Pouziva SAME font resolve path
//! jako `measure_text_width_full` (layout canonical) - pri zoom=1 glyph
//! advance shoduje s renderem (atlas rasterize-time bere stejny font).
//!
//! `EditorState` drzi text + caret byte offset + selection anchor byte
//! offset. Operace insert/delete/move/hit_test pres glyph_run geometry.
//! WebView pak vlastni `HashMap<node_id, EditorState>` pro input/textarea/
//! contenteditable.
//!
//! Cleanup target: nahradit ad-hoc `webview.input_caret` HashMap + 3
//! separate measure paths (paint text emit, caret blink, collect_text_lines)
//! jednim modelem.

use crate::browser::layout::{shape_text_advances, ShapedText};

/// Per-glyph data v shaped text runu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphRun {
    /// Char (UTF-32 codepoint).
    pub ch: char,
    /// Byte offset zacatku znaku v UTF-8 source textu.
    pub byte_offset: usize,
    /// X pozice zacatku (relative k text origin, logical px).
    pub x: f32,
    /// Advance po tomto glyfu (vc. letter_spacing).
    pub advance: f32,
}

/// Shape text -> Vec<GlyphRun> + ShapedText (low-level data).
///
/// Per-char x je `cumulative[i]`, advance je `advances[i]`. Vraci pole o
/// delce `text.chars().count()` (kazdy znak = 1 GlyphRun, bez ligature merge
/// - layout pouziva fontdue advance per codepoint).
pub fn shape_text(
    text: &str,
    font_size: f32,
    weight: u32,
    italic: bool,
    family: &str,
    letter_spacing: f32,
) -> (Vec<GlyphRun>, ShapedText) {
    let shaped = shape_text_advances(text, font_size, weight, italic, family, letter_spacing);
    let mut runs: Vec<GlyphRun> = Vec::with_capacity(shaped.advances.len());
    for (i, (ch_idx, ch)) in text.char_indices().zip(text.chars()).enumerate() {
        // ch_idx je (byte_offset, char) z char_indices, ale zip s chars by
        // dal redundantni - opravim na char_indices:
        let _ = ch;
        let byte_offset = ch_idx.0;
        let ch = ch_idx.1;
        let x = *shaped.cumulative.get(i).unwrap_or(&0.0);
        let advance = *shaped.advances.get(i).unwrap_or(&0.0);
        runs.push(GlyphRun { ch, byte_offset, x, advance });
    }
    (runs, shaped)
}

/// Najde byte offset N-teho charu v textu (caret position v charoch ->
/// UTF-8 byte). char_idx >= chars() vraci text.len().
pub fn char_to_byte_offset(text: &str, char_idx: usize) -> usize {
    text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(text.len())
}

/// Inverze - byte offset -> char index (pro hit_test->char->caret).
pub fn byte_to_char_offset(text: &str, byte_offset: usize) -> usize {
    let mut count = 0;
    for (b, _) in text.char_indices() {
        if b >= byte_offset { return count; }
        count += 1;
    }
    count
}

/// Stav single-line/multi-line text editoru. Drzi text + caret (byte offset
/// v textu) + optional selection anchor (byte offset, focus = caret).
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    pub text: String,
    /// Caret pozice v BYTE offset (UTF-8 boundary). Always <= text.len().
    pub caret: usize,
    /// Selection anchor byte offset. None = no selection.
    /// Selection range = (min(caret, anchor), max(...)).
    pub selection_anchor: Option<usize>,
}

impl EditorState {
    pub fn new(initial: &str) -> Self {
        EditorState {
            text: initial.to_string(),
            caret: initial.len(),
            selection_anchor: None,
        }
    }

    /// Nahradi text. Caret clamp na konec, selection clear.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        if self.caret > self.text.len() {
            self.caret = self.text.len();
        }
        self.selection_anchor = None;
    }

    /// Vraci normalizovany range (start_byte, end_byte) selection, nebo None.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.caret { return None; }
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    pub fn has_selection(&self) -> bool { self.selection_range().is_some() }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Insert string. Pokud selection, nejprve smaze.
    pub fn insert(&mut self, s: &str) {
        if let Some((a, b)) = self.selection_range() {
            self.text.replace_range(a..b, "");
            self.caret = a;
            self.selection_anchor = None;
        }
        // Caret clamp na valid UTF-8 boundary.
        let pos = self.caret.min(self.text.len());
        self.text.insert_str(pos, s);
        self.caret = pos + s.len();
    }

    /// Backspace. Pokud selection, smaze selection. Jinak smaze char pred caret.
    pub fn delete_backward(&mut self) {
        if let Some((a, b)) = self.selection_range() {
            self.text.replace_range(a..b, "");
            self.caret = a;
            self.selection_anchor = None;
            return;
        }
        if self.caret == 0 { return; }
        // Najdi predchozi char boundary.
        let prev = self.prev_char_boundary(self.caret);
        self.text.replace_range(prev..self.caret, "");
        self.caret = prev;
    }

    /// Delete forward.
    pub fn delete_forward(&mut self) {
        if let Some((a, b)) = self.selection_range() {
            self.text.replace_range(a..b, "");
            self.caret = a;
            self.selection_anchor = None;
            return;
        }
        if self.caret >= self.text.len() { return; }
        let next = self.next_char_boundary(self.caret);
        self.text.replace_range(self.caret..next, "");
    }

    /// Move caret left. extend=true -> rozsiruje selection (zachova anchor).
    /// by_word=true -> Ctrl-Left: skip pres word.
    pub fn move_left(&mut self, by_word: bool, extend: bool) {
        let start_anchor = if extend { self.selection_anchor.or(Some(self.caret)) } else { None };
        if !extend && self.has_selection() {
            // Pri no-extend left s selection: skoc na zacatek selection.
            if let Some((a, _)) = self.selection_range() {
                self.caret = a;
                self.selection_anchor = None;
                return;
            }
        }
        if by_word {
            self.caret = self.prev_word_boundary(self.caret);
        } else if self.caret > 0 {
            self.caret = self.prev_char_boundary(self.caret);
        }
        self.selection_anchor = start_anchor;
    }

    pub fn move_right(&mut self, by_word: bool, extend: bool) {
        let start_anchor = if extend { self.selection_anchor.or(Some(self.caret)) } else { None };
        if !extend && self.has_selection() {
            if let Some((_, b)) = self.selection_range() {
                self.caret = b;
                self.selection_anchor = None;
                return;
            }
        }
        if by_word {
            self.caret = self.next_word_boundary(self.caret);
        } else if self.caret < self.text.len() {
            self.caret = self.next_char_boundary(self.caret);
        }
        self.selection_anchor = start_anchor;
    }

    pub fn move_home(&mut self, extend: bool) {
        let start_anchor = if extend { self.selection_anchor.or(Some(self.caret)) } else { None };
        // Single-line: 0. Multi-line: zacatek aktualniho radku.
        let line_start = self.text[..self.caret].rfind('\n')
            .map(|p| p + 1).unwrap_or(0);
        self.caret = line_start;
        self.selection_anchor = start_anchor;
    }

    pub fn move_end(&mut self, extend: bool) {
        let start_anchor = if extend { self.selection_anchor.or(Some(self.caret)) } else { None };
        let line_end = self.text[self.caret..].find('\n')
            .map(|p| self.caret + p).unwrap_or(self.text.len());
        self.caret = line_end;
        self.selection_anchor = start_anchor;
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.caret = self.text.len();
    }

    /// Hit-test X koord (relative k text origin) -> nastav caret na byte
    /// offset nejblizsiho glyph mid.
    pub fn hit_test(&mut self, shaped: &ShapedText, x: f32, extend: bool) {
        let char_idx = shaped.char_at_x(x);
        let new_caret = char_to_byte_offset(&self.text, char_idx);
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.caret);
            }
        } else {
            self.selection_anchor = None;
        }
        self.caret = new_caret;
    }

    /// Caret -> char index (pro shaped.x_at_char).
    pub fn caret_char_index(&self) -> usize {
        byte_to_char_offset(&self.text, self.caret)
    }

    fn prev_char_boundary(&self, pos: usize) -> usize {
        if pos == 0 { return 0; }
        let mut p = pos - 1;
        while p > 0 && !self.text.is_char_boundary(p) { p -= 1; }
        p
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        let mut p = (pos + 1).min(self.text.len());
        while p < self.text.len() && !self.text.is_char_boundary(p) { p += 1; }
        p
    }

    fn prev_word_boundary(&self, pos: usize) -> usize {
        if pos == 0 { return 0; }
        let bytes = self.text.as_bytes();
        let mut p = pos;
        // Skip pres whitespace zpet.
        while p > 0 {
            let prev = self.prev_char_boundary(p);
            if !is_word_break_byte(bytes[prev]) { break; }
            p = prev;
        }
        // Skip pres word chars zpet.
        while p > 0 {
            let prev = self.prev_char_boundary(p);
            if is_word_break_byte(bytes[prev]) { break; }
            p = prev;
        }
        p
    }

    fn next_word_boundary(&self, pos: usize) -> usize {
        let bytes = self.text.as_bytes();
        let mut p = pos;
        // Skip pres word chars dopredu.
        while p < self.text.len() && !is_word_break_byte(bytes[p]) {
            p = self.next_char_boundary(p);
        }
        // Skip pres whitespace dopredu.
        while p < self.text.len() && is_word_break_byte(bytes[p]) {
            p = self.next_char_boundary(p);
        }
        p
    }
}

fn is_word_break_byte(b: u8) -> bool {
    // ASCII whitespace + punctuation. Multi-byte UTF-8 chars (CJK, etc.)
    // mahji bytes >= 0x80 = considered word chars (conservative).
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'.' | b',' | b';' | b':' | b'!' | b'?'
        | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b'"' | b'\''
        | b'/' | b'\\' | b'-' | b'_' | b'+' | b'=' | b'*' | b'&' | b'^' | b'%'
        | b'$' | b'#' | b'@' | b'~' | b'`')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_text_ascii_advances() {
        let (runs, shaped) = shape_text("abc", 16.0, 400, false, "", 0.0);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].ch, 'a');
        assert_eq!(runs[0].byte_offset, 0);
        assert_eq!(runs[1].byte_offset, 1);
        assert_eq!(runs[2].byte_offset, 2);
        // Cumulative monotonic.
        assert!(runs[0].x <= runs[1].x);
        assert!(runs[1].x <= runs[2].x);
        // total_width = poslední cumulative + advance.
        let last_end = runs[2].x + runs[2].advance;
        assert!((last_end - shaped.total_width).abs() < 0.01);
    }

    #[test]
    fn shape_text_utf8_byte_offsets() {
        let (runs, _) = shape_text("aá", 16.0, 400, false, "", 0.0);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].byte_offset, 0); // 'a' = 1 byte
        assert_eq!(runs[1].byte_offset, 1); // 'á' starts at byte 1 (2-byte UTF-8)
    }

    #[test]
    fn shaped_char_at_x_basic() {
        let (_, shaped) = shape_text("abcd", 16.0, 400, false, "", 0.0);
        // X = 0 -> char 0.
        assert_eq!(shaped.char_at_x(0.0), 0);
        // X za polovinu prvniho glyfu -> char 1.
        let mid1 = (shaped.cumulative[0] + shaped.cumulative[1]) * 0.5;
        assert_eq!(shaped.char_at_x(mid1 + 0.5), 1);
        // X >= total_width -> n_chars.
        assert_eq!(shaped.char_at_x(shaped.total_width + 100.0), 4);
    }

    #[test]
    fn editor_insert_at_caret() {
        let mut ed = EditorState::new("");
        ed.insert("hello");
        assert_eq!(ed.text, "hello");
        assert_eq!(ed.caret, 5);
    }

    #[test]
    fn editor_insert_mid() {
        let mut ed = EditorState::new("ad");
        ed.caret = 1;
        ed.insert("bc");
        assert_eq!(ed.text, "abcd");
        assert_eq!(ed.caret, 3);
    }

    #[test]
    fn editor_backspace() {
        let mut ed = EditorState::new("abc");
        assert_eq!(ed.caret, 3);
        ed.delete_backward();
        assert_eq!(ed.text, "ab");
        assert_eq!(ed.caret, 2);
    }

    #[test]
    fn editor_backspace_at_zero() {
        let mut ed = EditorState::new("abc");
        ed.caret = 0;
        ed.delete_backward();
        assert_eq!(ed.text, "abc");
        assert_eq!(ed.caret, 0);
    }

    #[test]
    fn editor_delete_forward() {
        let mut ed = EditorState::new("abc");
        ed.caret = 1;
        ed.delete_forward();
        assert_eq!(ed.text, "ac");
        assert_eq!(ed.caret, 1);
    }

    #[test]
    fn editor_move_left_right() {
        let mut ed = EditorState::new("abc");
        ed.caret = 3;
        ed.move_left(false, false);
        assert_eq!(ed.caret, 2);
        ed.move_right(false, false);
        assert_eq!(ed.caret, 3);
    }

    #[test]
    fn editor_move_left_utf8() {
        // 'á' = 2 byte UTF-8. Caret musi krokovat o char boundary, ne byte.
        let mut ed = EditorState::new("aá");
        ed.caret = 3; // konec
        ed.move_left(false, false);
        assert_eq!(ed.caret, 1); // pred 'á'
        ed.move_left(false, false);
        assert_eq!(ed.caret, 0);
    }

    #[test]
    fn editor_select_all_then_delete() {
        let mut ed = EditorState::new("hello");
        ed.select_all();
        assert!(ed.has_selection());
        ed.delete_backward();
        assert_eq!(ed.text, "");
        assert_eq!(ed.caret, 0);
    }

    #[test]
    fn editor_word_boundary() {
        let mut ed = EditorState::new("hello world test");
        ed.caret = ed.text.len();
        ed.move_left(true, false);
        // Po Ctrl+Left z konce: skok pred "test".
        assert_eq!(&ed.text[ed.caret..], "test");
    }

    #[test]
    fn editor_hit_test_basic() {
        let mut ed = EditorState::new("abcd");
        let (_, shaped) = shape_text(&ed.text, 16.0, 400, false, "", 0.0);
        // Hit-test pred zacatkem -> caret 0.
        ed.hit_test(&shaped, -10.0, false);
        assert_eq!(ed.caret, 0);
        // Hit-test za koncem -> caret na end (4 byte = 4 char).
        ed.hit_test(&shaped, 9999.0, false);
        assert_eq!(ed.caret, 4);
    }

    #[test]
    fn editor_hit_test_extend_creates_selection() {
        let mut ed = EditorState::new("abcd");
        ed.caret = 0;
        let (_, shaped) = shape_text(&ed.text, 16.0, 400, false, "", 0.0);
        ed.hit_test(&shaped, shaped.total_width, true);
        assert_eq!(ed.caret, 4);
        assert_eq!(ed.selection_anchor, Some(0));
        assert_eq!(ed.selection_range(), Some((0, 4)));
    }

    #[test]
    fn editor_move_home_end() {
        let mut ed = EditorState::new("abcdef");
        ed.caret = 3;
        ed.move_home(false);
        assert_eq!(ed.caret, 0);
        ed.move_end(false);
        assert_eq!(ed.caret, 6);
    }

    #[test]
    fn editor_hit_test_then_caret_char_index() {
        // Simulace MouseDown na vnitrek inputu "hello": klik priblizne na
        // pozici 'l' (3. char) -> caret musi byt na 2 nebo 3 dle snap.
        let mut ed = EditorState::new("hello");
        let (_, shaped) = shape_text(&ed.text, 16.0, 400, false, "", 0.0);
        // X = stred 3. glyfu ('l' index 2).
        let target_x = (shaped.cumulative[2] + shaped.cumulative[3]) * 0.5 - 0.5;
        ed.hit_test(&shaped, target_x, false);
        // Char index pred 3. glyf = 2.
        assert_eq!(ed.caret_char_index(), 2);
        // Byte = 2 (ASCII).
        assert_eq!(ed.caret, 2);
    }

    #[test]
    fn editor_set_text_clamps_caret() {
        let mut ed = EditorState::new("hello world");
        ed.caret = 11;
        ed.set_text("hi");
        assert_eq!(ed.text, "hi");
        assert_eq!(ed.caret, 2, "caret musi clamp na novy text.len()");
    }

    #[test]
    fn editor_extend_selection_anchors() {
        // Move s extend=true po caret advancuje - selection_anchor zustava
        // na initial position.
        let mut ed = EditorState::new("hello");
        ed.caret = 1;
        ed.move_right(false, true); // extend=true
        assert_eq!(ed.caret, 2);
        assert_eq!(ed.selection_anchor, Some(1));
        ed.move_right(false, true);
        assert_eq!(ed.caret, 3);
        assert_eq!(ed.selection_anchor, Some(1), "anchor musi zustat");
        assert_eq!(ed.selection_range(), Some((1, 3)));
    }

    #[test]
    fn editor_byte_char_roundtrip() {
        let txt = "aá本";
        // 'a' = 1B, 'á' = 2B, '本' = 3B. Total = 6B, 3 chars.
        assert_eq!(char_to_byte_offset(txt, 0), 0);
        assert_eq!(char_to_byte_offset(txt, 1), 1);
        assert_eq!(char_to_byte_offset(txt, 2), 3);
        assert_eq!(char_to_byte_offset(txt, 3), 6);
        assert_eq!(byte_to_char_offset(txt, 0), 0);
        assert_eq!(byte_to_char_offset(txt, 1), 1);
        assert_eq!(byte_to_char_offset(txt, 3), 2);
        assert_eq!(byte_to_char_offset(txt, 6), 3);
    }
}
