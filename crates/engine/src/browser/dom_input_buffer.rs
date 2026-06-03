//! TextBuffer adapter nad DOM <input>/<textarea>. Cursor + anchor zije v
//! Document.selection registry (per-element InputState mapa). Text se cte/zapise
//! pres `value` attr.
//!
//! Drzi cached String kopii value attr + InputState aby slo provadet
//! replace_range bez RefCell re-borrow konfliktu. Pri commit_back() (a Drop)
//! flush zpet do attru + Document.selection registry.

use std::rc::Rc;
use std::cell::RefCell;
use crate::browser::dom::{NodeData, Document};
use crate::browser::selection::InputState;
use crate::devtools::model::text_buffer::TextBuffer;

pub struct DomInputBuffer {
    node: Rc<NodeData>,
    document: Rc<RefCell<Document>>,
    text_cache: String,
    state_cache: InputState,
}

impl DomInputBuffer {
    pub fn new(node: Rc<NodeData>, document: Rc<RefCell<Document>>) -> Self {
        let text_cache = node.attributes.borrow().get("value").cloned().unwrap_or_default();
        let node_id = Rc::as_ptr(&node) as usize;
        let state_cache = {
            let doc = document.borrow();
            doc.selection.borrow().input_state(node_id).cloned()
                .unwrap_or_else(|| InputState { cursor: text_cache.len(), anchor: None })
        };
        let mut s = state_cache;
        if s.cursor > text_cache.len() { s.cursor = text_cache.len(); }
        if let Some(a) = s.anchor { if a > text_cache.len() { s.anchor = None; } }
        Self { node, document, text_cache, state_cache: s }
    }

    fn node_id(&self) -> usize { Rc::as_ptr(&self.node) as usize }

    pub fn commit_back(&self) {
        self.node.attributes.borrow_mut().insert("value".to_string(), self.text_cache.clone());
        let doc = self.document.borrow();
        let mut sel = doc.selection.borrow_mut();
        let st = sel.input_state_mut(self.node_id());
        st.cursor = self.state_cache.cursor;
        st.anchor = self.state_cache.anchor;
    }
}

impl Drop for DomInputBuffer {
    fn drop(&mut self) { self.commit_back(); }
}

impl TextBuffer for DomInputBuffer {
    fn text(&self) -> &str { &self.text_cache }
    fn cursor(&self) -> usize { self.state_cache.cursor }
    fn set_cursor(&mut self, byte: usize) {
        let mut i = byte.min(self.text_cache.len());
        while i > 0 && !self.text_cache.is_char_boundary(i) { i -= 1; }
        self.state_cache.cursor = i;
    }
    fn anchor(&self) -> Option<usize> { self.state_cache.anchor }
    fn set_anchor(&mut self, byte: Option<usize>) {
        let snapped = byte.map(|b| {
            let mut i = b.min(self.text_cache.len());
            while i > 0 && !self.text_cache.is_char_boundary(i) { i -= 1; }
            i
        });
        self.state_cache.anchor = snapped;
    }
    fn replace_range(&mut self, range: std::ops::Range<usize>, with: &str) {
        self.text_cache.replace_range(range, with);
    }
}
