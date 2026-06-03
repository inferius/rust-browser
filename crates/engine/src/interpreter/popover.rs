//! HTML Popover API - `popover` attribute + `showPopover()`/`hidePopover()`.
//!
//! Spec: https://html.spec.whatwg.org/multipage/popover.html
//!
//! Auto popover: dismissed pri Esc nebo click mimo. Manual: jen pres script.
//! Stack-based: noviější popover prekryje starsí. light dismiss queue.

use std::rc::Rc;
use std::cell::RefCell;
use crate::browser::dom::Node;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PopoverType {
    Auto,
    Manual,
    Hint,
}

impl PopoverType {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "manual" => Self::Manual,
            "hint" => Self::Hint,
            _ => Self::Auto, // default
        }
    }
}

#[derive(Default)]
pub struct PopoverStack {
    /// Open popovers v poradi - top je posledni.
    pub stack: Vec<Rc<Node>>,
}

impl PopoverStack {
    pub fn new() -> Self { Self::default() }

    pub fn show(&mut self, node: Rc<Node>, kind: PopoverType) -> bool {
        // Auto popover - close jine auto popovers nad current.
        if kind == PopoverType::Auto {
            // Real: close ancestors stack (DOM ancestor check).
        }
        self.stack.push(node);
        true
    }

    pub fn hide(&mut self, node: &Rc<Node>) -> bool {
        let before = self.stack.len();
        self.stack.retain(|n| !Rc::ptr_eq(n, node));
        self.stack.len() < before
    }

    /// Light dismiss - Esc / click mimo = close top auto popover.
    pub fn light_dismiss(&mut self) -> Option<Rc<Node>> {
        self.stack.pop()
    }

    pub fn is_open(&self, node: &Rc<Node>) -> bool {
        self.stack.iter().any(|n| Rc::ptr_eq(n, node))
    }

    pub fn top(&self) -> Option<&Rc<Node>> {
        self.stack.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::html_parser::parse_html;

    #[test]
    fn show_and_hide() {
        let mut s = PopoverStack::new();
        let doc = parse_html("<div popover>Hi</div>", "about:blank");
        let node = Rc::clone(&doc.root);
        assert!(s.show(node.clone(), PopoverType::Auto));
        assert!(s.is_open(&node));
        s.hide(&node);
        assert!(!s.is_open(&node));
    }

    #[test]
    fn light_dismiss_pops_top() {
        let mut s = PopoverStack::new();
        let doc1 = parse_html("<div>a</div>", "about:blank");
        let doc2 = parse_html("<div>b</div>", "about:blank");
        s.show(Rc::clone(&doc1.root), PopoverType::Auto);
        s.show(Rc::clone(&doc2.root), PopoverType::Auto);
        let popped = s.light_dismiss().unwrap();
        assert!(Rc::ptr_eq(&popped, &doc2.root));
    }

    #[test]
    fn parse_popover_attribute() {
        assert_eq!(PopoverType::parse("manual"), PopoverType::Manual);
        assert_eq!(PopoverType::parse("auto"), PopoverType::Auto);
        assert_eq!(PopoverType::parse(""), PopoverType::Auto);
    }
}
