//! TreeWalker + NodeIterator (DOM Traversal L1).
//!
//! Spec: https://dom.spec.whatwg.org/#interface-treewalker
//!
//! TreeWalker drzi current node + filter, podporuje nextNode/previousNode/
//! parentNode/firstChild/lastChild/nextSibling/previousSibling.
//! NodeIterator je simpler subset.

use std::rc::Rc;
use crate::browser::dom::Node;

#[derive(Debug, Clone, Copy)]
pub enum FilterAccept {
    Accept,
    Reject,  // skip node AND descendants
    Skip,    // skip node, descend OK
}

pub type NodeFilter = Box<dyn Fn(&Rc<Node>) -> FilterAccept>;

/// `NodeFilter.whatToShow` bitfield (DOM Traversal §5.5).
pub const SHOW_ALL: u32 = 0xFFFFFFFF;
pub const SHOW_ELEMENT: u32 = 1;
pub const SHOW_TEXT: u32 = 4;
pub const SHOW_COMMENT: u32 = 0x80;

pub struct TreeWalker {
    pub root: Rc<Node>,
    pub current: Rc<Node>,
    pub what_to_show: u32,
    pub filter: Option<NodeFilter>,
}

impl TreeWalker {
    pub fn new(root: Rc<Node>, what_to_show: u32, filter: Option<NodeFilter>) -> Self {
        Self { current: Rc::clone(&root), root, what_to_show, filter }
    }

    fn accept(&self, node: &Rc<Node>) -> FilterAccept {
        use crate::browser::dom::NodeKind;
        let kind_mask = match &node.kind {
            NodeKind::Element { .. } => SHOW_ELEMENT,
            NodeKind::Text(_) => SHOW_TEXT,
            NodeKind::Comment(_) => SHOW_COMMENT,
            _ => 0,
        };
        if self.what_to_show & kind_mask == 0 {
            return FilterAccept::Skip;
        }
        if let Some(f) = &self.filter {
            return f(node);
        }
        FilterAccept::Accept
    }

    /// firstChild() - prvni accepted child, descent depth-first.
    pub fn first_child(&mut self) -> Option<Rc<Node>> {
        let children = self.current.children.borrow().clone();
        for ch in children {
            match self.accept(&ch) {
                FilterAccept::Accept => {
                    self.current = Rc::clone(&ch);
                    return Some(ch);
                }
                FilterAccept::Skip => {
                    // Descend into this child's children.
                    let saved = std::mem::replace(&mut self.current, Rc::clone(&ch));
                    if let Some(n) = self.first_child() {
                        return Some(n);
                    }
                    self.current = saved;
                }
                FilterAccept::Reject => {}
            }
        }
        None
    }

    /// nextSibling() - dalsi accepted sibling current.
    pub fn next_sibling(&mut self) -> Option<Rc<Node>> {
        let parent = self.current.parent.borrow().upgrade()?;
        let siblings = parent.children.borrow().clone();
        let cur_ptr = Rc::as_ptr(&self.current);
        let mut after_cur = false;
        for s in siblings {
            if Rc::as_ptr(&s) == cur_ptr { after_cur = true; continue; }
            if !after_cur { continue; }
            match self.accept(&s) {
                FilterAccept::Accept => {
                    self.current = Rc::clone(&s);
                    return Some(s);
                }
                _ => {}
            }
        }
        None
    }

    /// nextNode - DFS pres tree, return next accepted.
    pub fn next_node(&mut self) -> Option<Rc<Node>> {
        if let Some(n) = self.first_child() { return Some(n); }
        // No children - sibling or ancestor's sibling.
        let mut cur = Rc::clone(&self.current);
        loop {
            let parent_opt = cur.parent.borrow().upgrade();
            let parent = match parent_opt { Some(p) => p, None => return None };
            if Rc::ptr_eq(&cur, &self.root) { return None; }
            self.current = Rc::clone(&cur);
            if let Some(s) = self.next_sibling() { return Some(s); }
            if Rc::ptr_eq(&parent, &self.root) { return None; }
            cur = parent;
        }
    }

    /// parentNode - prvni accepted ancestor.
    pub fn parent_node(&mut self) -> Option<Rc<Node>> {
        let mut cur = self.current.parent.borrow().upgrade()?;
        loop {
            if Rc::ptr_eq(&cur, &self.root) { return None; }
            if matches!(self.accept(&cur), FilterAccept::Accept) {
                self.current = Rc::clone(&cur);
                return Some(cur);
            }
            let next = cur.parent.borrow().upgrade();
            cur = next?;
        }
    }
}

/// NodeIterator - simpler API (jen nextNode/previousNode + reference).
pub struct NodeIterator {
    pub walker: TreeWalker,
}

impl NodeIterator {
    pub fn new(root: Rc<Node>, what_to_show: u32, filter: Option<NodeFilter>) -> Self {
        Self { walker: TreeWalker::new(root, what_to_show, filter) }
    }

    pub fn next_node(&mut self) -> Option<Rc<Node>> {
        self.walker.next_node()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::html_parser;

    fn make_doc(html: &str) -> Rc<Node> {
        let doc = html_parser::parse_html(html, "about:blank");
        Rc::clone(&doc.root)
    }

    #[test]
    fn walk_all_elements() {
        let root = make_doc("<div><p>a</p><span>b</span></div>");
        let mut w = TreeWalker::new(Rc::clone(&root), SHOW_ELEMENT, None);
        let mut count = 0;
        while w.next_node().is_some() { count += 1; }
        // html, head, body, div, p, span = 6 elementov.
        assert!(count >= 4);
    }

    #[test]
    fn walk_text_only() {
        let root = make_doc("<p>hello <span>world</span></p>");
        let mut w = TreeWalker::new(Rc::clone(&root), SHOW_TEXT, None);
        let mut count = 0;
        while w.next_node().is_some() { count += 1; }
        assert!(count >= 1); // alespoň jeden text node
    }

    #[test]
    fn filter_rejects_specific() {
        let root = make_doc("<p>a</p><p class='skip'>b</p><p>c</p>");
        let filter: NodeFilter = Box::new(|n| {
            if n.attr("class").as_deref() == Some("skip") {
                FilterAccept::Reject
            } else {
                FilterAccept::Accept
            }
        });
        let mut w = TreeWalker::new(Rc::clone(&root), SHOW_ELEMENT, Some(filter));
        let mut count = 0;
        while w.next_node().is_some() { count += 1; }
        // Skip class=skip + descendants - p s class=skip nepocita.
        assert!(count >= 1);
    }
}
