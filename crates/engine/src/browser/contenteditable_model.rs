//! contenteditable + execCommand state model.
//!
//! Spec: https://w3c.github.io/editing/
//! Tracks selection, insertion mode, undo history, command queue.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditCommand {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    InsertText,
    InsertLineBreak,
    InsertParagraph,
    Delete,
    ForwardDelete,
    Indent,
    Outdent,
    JustifyLeft,
    JustifyCenter,
    JustifyRight,
    JustifyFull,
    InsertOrderedList,
    InsertUnorderedList,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
pub struct EditAction {
    pub command: EditCommand,
    pub value: Option<String>,
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    pub undo: VecDeque<EditAction>,
    pub redo: VecDeque<EditAction>,
    pub max_depth: usize,
}

impl UndoStack {
    pub fn new() -> Self {
        Self { undo: VecDeque::new(), redo: VecDeque::new(), max_depth: 100 }
    }

    pub fn record(&mut self, action: EditAction) {
        self.undo.push_back(action);
        self.redo.clear();
        if self.undo.len() > self.max_depth {
            self.undo.pop_front();
        }
    }

    pub fn undo(&mut self) -> Option<EditAction> {
        let a = self.undo.pop_back()?;
        self.redo.push_back(a.clone());
        Some(a)
    }

    pub fn redo(&mut self) -> Option<EditAction> {
        let a = self.redo.pop_back()?;
        self.undo.push_back(a.clone());
        Some(a)
    }
}

#[derive(Debug, Clone)]
pub struct EditableSelection {
    pub anchor_node: u64,
    pub anchor_offset: usize,
    pub focus_node: u64,
    pub focus_offset: usize,
    pub is_collapsed: bool,
    pub direction: SelectionDirection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectionDirection {
    Forward,
    Backward,
    None,
}

impl EditableSelection {
    pub fn collapsed(node: u64, offset: usize) -> Self {
        Self {
            anchor_node: node, anchor_offset: offset,
            focus_node: node, focus_offset: offset,
            is_collapsed: true,
            direction: SelectionDirection::None,
        }
    }
}

#[derive(Default)]
pub struct EditorState {
    pub history: UndoStack,
    pub selection: Option<EditableSelection>,
    pub composition_active: bool,
    pub query_state_cache: std::collections::HashMap<EditCommand, bool>,
}

impl EditorState {
    pub fn new() -> Self {
        Self { history: UndoStack::new(), ..Self::default() }
    }

    pub fn exec(&mut self, command: EditCommand, value: Option<&str>, now: u64) {
        let action = EditAction { command, value: value.map(|s| s.into()), timestamp_us: now };
        self.history.record(action);
    }

    pub fn query_state(&self, command: EditCommand) -> bool {
        self.query_state_cache.get(&command).copied().unwrap_or(false)
    }

    pub fn set_state(&mut self, command: EditCommand, active: bool) {
        self.query_state_cache.insert(command, active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_works() {
        let mut u = UndoStack::new();
        u.record(EditAction { command: EditCommand::Bold, value: None, timestamp_us: 0 });
        u.record(EditAction { command: EditCommand::Italic, value: None, timestamp_us: 1 });
        assert!(u.undo().is_some());
        assert!(u.redo().is_some());
    }

    #[test]
    fn new_action_clears_redo() {
        let mut u = UndoStack::new();
        u.record(EditAction { command: EditCommand::Bold, value: None, timestamp_us: 0 });
        u.undo();
        u.record(EditAction { command: EditCommand::Italic, value: None, timestamp_us: 1 });
        assert!(u.redo().is_none());
    }

    #[test]
    fn depth_limited() {
        let mut u = UndoStack::new();
        u.max_depth = 3;
        for i in 0..10 {
            u.record(EditAction { command: EditCommand::Bold, value: None, timestamp_us: i });
        }
        assert!(u.undo.len() <= 3);
    }

    #[test]
    fn collapsed_selection() {
        let s = EditableSelection::collapsed(1, 5);
        assert!(s.is_collapsed);
        assert_eq!(s.anchor_offset, 5);
        assert_eq!(s.focus_offset, 5);
    }

    #[test]
    fn exec_appends_to_history() {
        let mut e = EditorState::new();
        e.exec(EditCommand::Bold, None, 0);
        assert_eq!(e.history.undo.len(), 1);
    }

    #[test]
    fn query_state_cache() {
        let mut e = EditorState::new();
        e.set_state(EditCommand::Bold, true);
        assert!(e.query_state(EditCommand::Bold));
        assert!(!e.query_state(EditCommand::Italic));
    }
}
