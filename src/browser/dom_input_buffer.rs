//! TextBuffer adapter nad DOM <input>/<textarea>. Cursor + anchor zije v
//! NodeData::input_cursor / input_anchor. Text se cte/zapise pres `value` attr.
//!
//! Drzi cached String kopii value attr aby slo provadet replace_range bez RefCell
//! re-borrow konfliktu (replace_range je &mut self na bufferu, ale zapis do attr
//! by vyzadoval borrow_mut). Pri commit_back() flush kopie zpet do attru.

use std::rc::Rc;
use crate::browser::dom::NodeData;
use crate::devtools::model::text_buffer::TextBuffer;

pub struct DomInputBuffer {
    node: Rc<NodeData>,
    text_cache: String,
}

impl DomInputBuffer {
    pub fn new(node: Rc<NodeData>) -> Self {
        let text_cache = node.attributes.borrow().get("value").cloned().unwrap_or_default();
        // Snap cursor na length pri prvnim use kdyz uninit (default 0).
        // Snap anchor na char boundary kdyz Some.
        let len = text_cache.len();
        if node.input_cursor.get() > len {
            node.input_cursor.set(len);
        }
        Self { node, text_cache }
    }

    /// Flush text_cache zpet do `value` attr. Volat pred drop / pred render.
    pub fn commit_back(&self) {
        self.node.attributes.borrow_mut().insert("value".to_string(), self.text_cache.clone());
    }
}

impl Drop for DomInputBuffer {
    fn drop(&mut self) {
        self.commit_back();
    }
}

impl TextBuffer for DomInputBuffer {
    fn text(&self) -> &str { &self.text_cache }
    fn cursor(&self) -> usize { self.node.input_cursor.get() }
    fn set_cursor(&mut self, byte: usize) {
        let mut i = byte.min(self.text_cache.len());
        while i > 0 && !self.text_cache.is_char_boundary(i) { i -= 1; }
        self.node.input_cursor.set(i);
    }
    fn anchor(&self) -> Option<usize> { self.node.input_anchor.get() }
    fn set_anchor(&mut self, byte: Option<usize>) {
        let snapped = byte.map(|b| {
            let mut i = b.min(self.text_cache.len());
            while i > 0 && !self.text_cache.is_char_boundary(i) { i -= 1; }
            i
        });
        self.node.input_anchor.set(snapped);
    }
    fn replace_range(&mut self, range: std::ops::Range<usize>, with: &str) {
        self.text_cache.replace_range(range, with);
    }
}
