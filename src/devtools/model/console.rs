//! Console panel: log entries + input field s cursor/selection/history + autocomplete.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    InputEcho,
    Result,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text: String,
}

/// Input pole s cursor pos + selection + historie.
/// Cursor je byte offset v `text`. Selection je (anchor, cursor) - pri active selection
/// jde rozsirit pres Shift+Arrow / mouse drag.
#[derive(Debug, Clone, Default)]
pub struct ConsoleInput {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub history_pending: String,
    /// Pixel scroll offset pri input prekroci sirku - jen pro horizontal scroll.
    pub scroll_x: f32,
}

impl ConsoleInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_selection(&self) -> bool {
        self.anchor.map(|a| a != self.cursor).unwrap_or(false)
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        if a == self.cursor { return None; }
        Some((a.min(self.cursor), a.max(self.cursor)))
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Vlozi text na cursor pos (nebo nahradi selection). Posune cursor.
    pub fn insert(&mut self, s: &str) {
        if let Some((start, end)) = self.selection_range() {
            self.text.replace_range(start..end, s);
            self.cursor = start + s.len();
        } else {
            self.text.insert_str(self.cursor, s);
            self.cursor += s.len();
        }
        self.clear_selection();
        self.history_idx = None;
    }

    /// Smaze char vlevo od cursoru (Backspace).
    pub fn backspace(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.text.replace_range(start..end, "");
            self.cursor = start;
            self.clear_selection();
            return;
        }
        if self.cursor == 0 { return; }
        let new_cursor = prev_char_boundary(&self.text, self.cursor);
        self.text.replace_range(new_cursor..self.cursor, "");
        self.cursor = new_cursor;
    }

    pub fn delete_forward(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.text.replace_range(start..end, "");
            self.cursor = start;
            self.clear_selection();
            return;
        }
        if self.cursor >= self.text.len() { return; }
        let next = next_char_boundary(&self.text, self.cursor);
        self.text.replace_range(self.cursor..next, "");
    }

    pub fn move_left(&mut self, extend_selection: bool) {
        if !extend_selection && self.has_selection() {
            self.cursor = self.selection_range().unwrap().0;
            self.clear_selection();
            return;
        }
        if extend_selection && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        if !extend_selection {
            self.clear_selection();
        }
        self.cursor = prev_char_boundary(&self.text, self.cursor);
    }

    pub fn move_right(&mut self, extend_selection: bool) {
        if !extend_selection && self.has_selection() {
            self.cursor = self.selection_range().unwrap().1;
            self.clear_selection();
            return;
        }
        if extend_selection && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        if !extend_selection {
            self.clear_selection();
        }
        self.cursor = next_char_boundary(&self.text, self.cursor);
    }

    pub fn move_home(&mut self, extend_selection: bool) {
        if extend_selection && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        if !extend_selection {
            self.clear_selection();
        }
        self.cursor = 0;
    }

    pub fn move_end(&mut self, extend_selection: bool) {
        if extend_selection && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        if !extend_selection {
            self.clear_selection();
        }
        self.cursor = self.text.len();
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }

    pub fn selected_text(&self) -> Option<String> {
        let (s, e) = self.selection_range()?;
        Some(self.text[s..e].to_string())
    }

    /// Vyrize selection do clipboardu - vraci text do callera (caller posli na clipboard).
    pub fn cut(&mut self) -> Option<String> {
        let s = self.selected_text()?;
        if let Some((start, end)) = self.selection_range() {
            self.text.replace_range(start..end, "");
            self.cursor = start;
            self.clear_selection();
        }
        Some(s)
    }

    /// Vrati current text + reset.
    pub fn submit(&mut self) -> String {
        let cmd = std::mem::take(&mut self.text);
        if !cmd.trim().is_empty() {
            self.history.push(cmd.clone());
            if self.history.len() > 200 { self.history.remove(0); }
        }
        self.cursor = 0;
        self.anchor = None;
        self.history_idx = None;
        self.history_pending.clear();
        cmd
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() { return; }
        match self.history_idx {
            None => {
                self.history_pending = self.text.clone();
                self.history_idx = Some(self.history.len() - 1);
            }
            Some(0) => return,
            Some(i) => self.history_idx = Some(i - 1),
        }
        if let Some(i) = self.history_idx {
            self.text = self.history[i].clone();
            self.cursor = self.text.len();
            self.clear_selection();
        }
    }

    pub fn history_next(&mut self) {
        let Some(i) = self.history_idx else { return };
        if i + 1 >= self.history.len() {
            // Konec historie - vrat se na rozpracovany text.
            self.text = std::mem::take(&mut self.history_pending);
            self.history_idx = None;
        } else {
            self.history_idx = Some(i + 1);
            self.text = self.history[i + 1].clone();
        }
        self.cursor = self.text.len();
        self.clear_selection();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
        self.history_idx = None;
        self.history_pending.clear();
    }
}

fn prev_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx == 0 { return 0; }
    idx -= 1;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn next_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() { return s.len(); }
    idx += 1;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// Autocomplete navrhy. `kind` = co se completuje (global ident / member access).
#[derive(Debug, Clone)]
pub struct AutocompleteHit {
    pub text: String,
    pub kind: HitKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Variable,
    Property,
    Function,
    Keyword,
}

#[derive(Debug, Default)]
pub struct AutocompleteState {
    pub hits: Vec<AutocompleteHit>,
    pub selected: usize,
    /// Byte offset v console_input.text kde zacina prefix (tj. kam se text vlozi).
    pub prefix_start: usize,
}

impl AutocompleteState {
    pub fn open(hits: Vec<AutocompleteHit>, prefix_start: usize) -> Option<Self> {
        if hits.is_empty() { return None; }
        Some(AutocompleteState { hits, selected: 0, prefix_start })
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.hits.len() {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn current(&self) -> Option<&AutocompleteHit> {
        self.hits.get(self.selected)
    }
}

#[cfg(test)]
#[path = "../tests/console_input_tests.rs"]
mod tests;
