//! Input Method Editor (IME) composition state.
//!
//! Spec: https://www.w3.org/TR/ime-api/
//! compositionstart, compositionupdate, compositionend events.
//! Real impl talks to OS IME (IMM32 / IBus / TSF / Cocoa Input Method Kit).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImeState {
    Idle,
    Composing,
    Committed,
}

#[derive(Debug, Clone)]
pub struct ImeComposition {
    pub state: ImeState,
    pub text: String,            // currently visible pre-edit text
    pub caret: usize,            // caret position within composition
    pub committed_text: String,  // accumulated committed text
    pub clauses: Vec<(usize, usize)>, // [start, end) clause ranges for underline rendering
}

impl ImeComposition {
    pub fn new() -> Self {
        Self {
            state: ImeState::Idle,
            text: String::new(),
            caret: 0,
            committed_text: String::new(),
            clauses: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.state = ImeState::Composing;
        self.text.clear();
        self.caret = 0;
        self.clauses.clear();
    }

    pub fn update(&mut self, text: &str, caret: usize, clauses: Vec<(usize, usize)>) {
        self.text = text.to_string();
        self.caret = caret.min(text.chars().count());
        self.clauses = clauses;
        self.state = ImeState::Composing;
    }

    pub fn commit(&mut self, final_text: &str) {
        self.committed_text.push_str(final_text);
        self.text.clear();
        self.caret = 0;
        self.clauses.clear();
        self.state = ImeState::Committed;
    }

    pub fn cancel(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.clauses.clear();
        self.state = ImeState::Idle;
    }
}

impl Default for ImeComposition {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_marks_composing() {
        let mut c = ImeComposition::new();
        c.start();
        assert_eq!(c.state, ImeState::Composing);
    }

    #[test]
    fn update_text() {
        let mut c = ImeComposition::new();
        c.start();
        c.update("\u{4F60}\u{597D}", 2, vec![(0, 2)]);
        assert_eq!(c.text, "\u{4F60}\u{597D}");
        assert_eq!(c.caret, 2);
        assert_eq!(c.clauses, vec![(0, 2)]);
    }

    #[test]
    fn commit_accumulates() {
        let mut c = ImeComposition::new();
        c.start();
        c.commit("hello");
        c.start();
        c.commit(" world");
        assert_eq!(c.committed_text, "hello world");
    }

    #[test]
    fn cancel_resets() {
        let mut c = ImeComposition::new();
        c.start();
        c.update("x", 1, vec![]);
        c.cancel();
        assert!(c.text.is_empty());
        assert_eq!(c.state, ImeState::Idle);
    }

    #[test]
    fn caret_clamped_to_length() {
        let mut c = ImeComposition::new();
        c.start();
        c.update("ab", 100, vec![]);
        assert_eq!(c.caret, 2);
    }
}
