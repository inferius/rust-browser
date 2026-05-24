//! Memory inspector - snapshot Rc counts + heap walk pro devtools Memory tab.
//!
//! Inspired by Chromium `third_party/blink/renderer/core/inspector/inspector_memory_agent.cc`.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use crate::interpreter::{JsObject, JsValue};

/// Memory snapshot - per object type pocet instances + total bytes.
#[derive(Debug, Default, Clone)]
pub struct MemorySnapshot {
    pub objects: usize,
    pub strings: usize,
    pub arrays: usize,
    pub functions: usize,
    pub dom_nodes: usize,
    pub total_bytes: usize,
    /// Top retainers - object type + count.
    pub by_type: HashMap<String, usize>,
}

impl MemorySnapshot {
    pub fn new() -> Self { Self::default() }

    /// Walk JS object graph z root + count instances per type.
    pub fn capture(&mut self, root: &Rc<RefCell<JsObject>>) {
        let mut visited = std::collections::HashSet::new();
        self.walk_object(root, &mut visited);
    }

    fn walk_object(
        &mut self,
        obj: &Rc<RefCell<JsObject>>,
        visited: &mut std::collections::HashSet<*const RefCell<JsObject>>,
    ) {
        let ptr = Rc::as_ptr(obj);
        if !visited.insert(ptr) { return; }
        self.objects += 1;
        // Approx bytes: 8 bytes per prop entry overhead + value size.
        let borrowed = match obj.try_borrow() { Ok(b) => b, Err(_) => return };
        for (k, v) in &borrowed.props {
            self.total_bytes += k.len() + std::mem::size_of_val(v);
            match v {
                JsValue::Str(s) => {
                    self.strings += 1;
                    self.total_bytes += s.len();
                }
                JsValue::Array(arr) => {
                    self.arrays += 1;
                    self.total_bytes += arr.borrow().len() * 16;
                }
                JsValue::Object(child) => {
                    self.walk_object(child, visited);
                }
                JsValue::DomNode(_) => {
                    self.dom_nodes += 1;
                }
                _ => {}
            }
        }
    }

    pub fn report(&self) -> String {
        format!(
            "Memory: {} objects, {} strings, {} arrays, {} dom_nodes, ~{} KB total",
            self.objects, self.strings, self.arrays, self.dom_nodes,
            self.total_bytes / 1024,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot() {
        let root = Rc::new(RefCell::new(JsObject::new()));
        let mut snap = MemorySnapshot::new();
        snap.capture(&root);
        assert_eq!(snap.objects, 1); // root
        assert_eq!(snap.strings, 0);
    }

    #[test]
    fn count_strings_and_arrays() {
        let root = Rc::new(RefCell::new(JsObject::new()));
        root.borrow_mut().set("name".into(), JsValue::Str("hello".into()));
        root.borrow_mut().set("items".into(),
            JsValue::Array(Rc::new(RefCell::new(vec![]))));
        let mut snap = MemorySnapshot::new();
        snap.capture(&root);
        assert_eq!(snap.strings, 1);
        assert_eq!(snap.arrays, 1);
    }

    #[test]
    fn cycle_safe() {
        let a = Rc::new(RefCell::new(JsObject::new()));
        let b = Rc::new(RefCell::new(JsObject::new()));
        a.borrow_mut().set("b".into(), JsValue::Object(Rc::clone(&b)));
        b.borrow_mut().set("a".into(), JsValue::Object(Rc::clone(&a)));
        let mut snap = MemorySnapshot::new();
        snap.capture(&a);
        assert_eq!(snap.objects, 2); // ne loop
    }

    #[test]
    fn report_format() {
        let root = Rc::new(RefCell::new(JsObject::new()));
        root.borrow_mut().set("x".into(), JsValue::Str("test".into()));
        let mut snap = MemorySnapshot::new();
        snap.capture(&root);
        let r = snap.report();
        assert!(r.contains("Memory:"));
        assert!(r.contains("strings"));
    }
}
