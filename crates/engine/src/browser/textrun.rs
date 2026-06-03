//! TextRun - foundation pro per-glyph selection (Phase 6).
//!
//! Aktualni stav: rect-drag selection s flow-based copy (compute_selection_text
//! v render mod.rs). Per-glyph anchor/focus by vyzadoval velky refactor:
//! - Paint emit musi tracker per-run rect + glyph advances
//! - Hit-test (x,y) -> (run_idx, byte_offset) pres binary search
//! - Selection paint highlight per-run partial rect
//! - W3C Selection API: anchorNode/focusNode/anchorOffset/focusOffset
//! - Keyboard nav (Shift+Arrow extend)
//!
//! Tato data structura definuje cilove API. Real implementace by nahradila
//! `PageSelection { anchor: (f32,f32), current: (f32,f32) }` na
//! `PageSelection { anchor: SelectionPos, focus: SelectionPos }`.

/// Pozice v selection - identifikuje konkretni glyf v konkretnim TextRun.
/// Stable across layout reflow (anchor zustava na glyfu, ne pixelu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SelectionPos {
    /// Index do TextRun pole (per-paint TextRuns vector).
    pub run_idx: usize,
    /// Byte offset v TextRun.text. UTF-8 boundary.
    pub byte_offset: usize,
}

/// Painted text run - 1:1 s DisplayCommand::Text v paint.
/// Drzi reference na DOM node + text + advance widths pro hit-test.
#[derive(Debug, Clone)]
pub struct TextRun {
    /// DOM node ktery text obsahuje (pro Selection API anchorNode/focusNode).
    /// Rc<NodeData> pointer cast as usize - stable identifier.
    pub node_id: usize,
    /// Plain text content (UTF-8).
    pub text: String,
    /// Origin position v document-logical px (kde rendering zacina).
    pub origin_x: f32,
    pub origin_y: f32,
    /// Per-glyph advance widths (cumulative pro binary-search hit-test).
    /// Velikost: 1 + text.chars().count(). [0] = 0.0, [n] = total_width.
    pub cumulative_advances: Vec<f32>,
    /// Vyska glyf radku (pro y range).
    pub line_height: f32,
}

impl TextRun {
    /// Hit-test x souradnice v ramci tohoto runu -> byte_offset.
    /// Standard text editing: tie-break >= half snap to NEXT glyf.
    pub fn byte_offset_at_x(&self, x: f32) -> usize {
        let local_x = (x - self.origin_x).max(0.0);
        let n_chars = self.text.chars().count();
        let mut closest_char = n_chars; // default end
        for i in 0..n_chars {
            let mid = (self.cumulative_advances[i] + self.cumulative_advances[i + 1]) * 0.5;
            if local_x < mid {
                closest_char = i;
                break;
            }
        }
        self.text.char_indices()
            .nth(closest_char)
            .map(|(b, _)| b)
            .unwrap_or(self.text.len())
    }

    /// X pozice byte_offset v ramci tohoto runu.
    pub fn x_at_byte_offset(&self, byte_offset: usize) -> f32 {
        let char_idx = self.text[..byte_offset.min(self.text.len())].chars().count();
        self.origin_x + self.cumulative_advances.get(char_idx).copied().unwrap_or(0.0)
    }

    /// True pokud (x, y) je v rectu tohoto runu.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let total_w = self.cumulative_advances.last().copied().unwrap_or(0.0);
        x >= self.origin_x && x < self.origin_x + total_w
            && y >= self.origin_y && y < self.origin_y + self.line_height
    }
}

/// Selection range pres TextRun pole - source of truth pri Phase 6.
/// Aktualne neaktivni; rect-drag pres PageSelection v selection.rs zustava.
#[derive(Debug, Clone)]
pub struct TextSelection {
    pub anchor: SelectionPos,
    pub focus: SelectionPos,
}

impl TextSelection {
    /// Vraci normalizovany start/end (pres run_idx, pak byte_offset).
    pub fn ordered(&self) -> (SelectionPos, SelectionPos) {
        let a = self.anchor;
        let b = self.focus;
        if a.run_idx < b.run_idx
            || (a.run_idx == b.run_idx && a.byte_offset <= b.byte_offset) {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Extract text z runs anchor->focus. Walk runs[anchor..focus], concat
    /// substrings. Pri stejnem run_idx jen single substring.
    pub fn extract_text(&self, runs: &[TextRun]) -> String {
        let (s, e) = self.ordered();
        if s.run_idx == e.run_idx {
            if let Some(r) = runs.get(s.run_idx) {
                return r.text.get(s.byte_offset..e.byte_offset)
                    .unwrap_or("").to_string();
            }
            return String::new();
        }
        let mut out = String::new();
        // First run: od s.byte_offset do konce.
        if let Some(r) = runs.get(s.run_idx) {
            if let Some(slice) = r.text.get(s.byte_offset..) {
                out.push_str(slice);
                out.push('\n');
            }
        }
        // Middle runs: cely text.
        for ri in (s.run_idx + 1)..e.run_idx {
            if let Some(r) = runs.get(ri) {
                out.push_str(&r.text);
                out.push('\n');
            }
        }
        // Last run: od 0 do e.byte_offset.
        if let Some(r) = runs.get(e.run_idx) {
            if let Some(slice) = r.text.get(..e.byte_offset) {
                out.push_str(slice);
            }
        }
        out
    }
}

/// Hit-test (x, y) na pole TextRunů -> SelectionPos. None pokud ne v zadnem runu.
pub fn hit_test_runs(runs: &[TextRun], x: f32, y: f32) -> Option<SelectionPos> {
    for (idx, run) in runs.iter().enumerate() {
        if run.contains(x, y) {
            return Some(SelectionPos {
                run_idx: idx,
                byte_offset: run.byte_offset_at_x(x),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, x: f32, y: f32, char_advances: &[f32]) -> TextRun {
        let mut cum = vec![0.0];
        let mut acc = 0.0;
        for a in char_advances { acc += a; cum.push(acc); }
        TextRun {
            node_id: 0, text: text.to_string(),
            origin_x: x, origin_y: y,
            cumulative_advances: cum,
            line_height: 16.0,
        }
    }

    #[test]
    fn hit_test_within_run() {
        let r = run("abc", 100.0, 100.0, &[10.0, 10.0, 10.0]);
        assert!(r.contains(105.0, 105.0));
        assert!(!r.contains(50.0, 105.0));
        assert!(!r.contains(105.0, 200.0));
    }

    #[test]
    fn byte_offset_at_x() {
        let r = run("abc", 100.0, 100.0, &[10.0, 10.0, 10.0]);
        // x = 100 -> offset 0 (zacatek).
        assert_eq!(r.byte_offset_at_x(100.0), 0);
        // x = 105 -> closest na 100 nebo 110, vyber 110 (offset 1).
        assert_eq!(r.byte_offset_at_x(105.0), 1);
        // x = 130 -> end (offset 3).
        assert_eq!(r.byte_offset_at_x(130.0), 3);
    }

    #[test]
    fn extract_single_run() {
        let runs = vec![run("Hello world", 0.0, 0.0, &[7.0; 11])];
        let sel = TextSelection {
            anchor: SelectionPos { run_idx: 0, byte_offset: 0 },
            focus: SelectionPos { run_idx: 0, byte_offset: 5 },
        };
        assert_eq!(sel.extract_text(&runs), "Hello");
    }

    #[test]
    fn extract_multi_run() {
        let runs = vec![
            run("Line one", 0.0, 0.0, &[5.0; 8]),
            run("Line two", 0.0, 20.0, &[5.0; 8]),
        ];
        let sel = TextSelection {
            anchor: SelectionPos { run_idx: 0, byte_offset: 5 },
            focus: SelectionPos { run_idx: 1, byte_offset: 4 },
        };
        assert_eq!(sel.extract_text(&runs), "one\nLine");
    }

    #[test]
    fn ordered_handles_reverse() {
        let sel = TextSelection {
            anchor: SelectionPos { run_idx: 1, byte_offset: 5 },
            focus: SelectionPos { run_idx: 0, byte_offset: 2 },
        };
        let (s, e) = sel.ordered();
        assert_eq!(s.run_idx, 0);
        assert_eq!(e.run_idx, 1);
    }
}
