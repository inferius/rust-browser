//! HTML `<template>` element - inert document fragment.
//!
//! Spec: https://html.spec.whatwg.org/multipage/scripting.html#the-template-element
//! Template content stored in a separate DocumentFragment; not active until cloned.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TemplateContent {
    pub id: u64,
    pub inert_root_id: u64,                // node id of the inert fragment root
    pub shadow_root_mode: Option<String>,  // for declarative shadow DOM
    pub shadow_root_delegates_focus: bool,
    pub shadow_root_clonable: bool,
}

#[derive(Default)]
pub struct TemplateRegistry {
    pub templates: HashMap<u64, TemplateContent>,
    pub next_id: u64,
}

impl TemplateRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, inert_root_id: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.templates.insert(id, TemplateContent {
            id, inert_root_id,
            shadow_root_mode: None,
            shadow_root_delegates_focus: false,
            shadow_root_clonable: false,
        });
        id
    }

    /// Mark template as declarative shadow root template per spec.
    pub fn set_declarative_shadow(&mut self, id: u64, mode: &str, delegates: bool, clonable: bool) {
        if let Some(t) = self.templates.get_mut(&id) {
            t.shadow_root_mode = Some(mode.into());
            t.shadow_root_delegates_focus = delegates;
            t.shadow_root_clonable = clonable;
        }
    }

    /// Clone content - returns a copy of the inert root id (real impl deep-clones DOM).
    pub fn clone_content(&self, id: u64) -> Option<u64> {
        self.templates.get(&id).map(|t| t.inert_root_id)
    }

    pub fn is_declarative_shadow(&self, id: u64) -> bool {
        self.templates.get(&id).map(|t| t.shadow_root_mode.is_some()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_returns_id() {
        let mut r = TemplateRegistry::new();
        let id = r.create(100);
        assert!(r.templates.contains_key(&id));
    }

    #[test]
    fn declarative_shadow_mode() {
        let mut r = TemplateRegistry::new();
        let id = r.create(100);
        r.set_declarative_shadow(id, "open", true, false);
        let t = r.templates.get(&id).unwrap();
        assert_eq!(t.shadow_root_mode.as_deref(), Some("open"));
        assert!(t.shadow_root_delegates_focus);
    }

    #[test]
    fn clone_returns_root_id() {
        let mut r = TemplateRegistry::new();
        let id = r.create(42);
        assert_eq!(r.clone_content(id), Some(42));
    }

    #[test]
    fn is_declarative_check() {
        let mut r = TemplateRegistry::new();
        let id = r.create(1);
        assert!(!r.is_declarative_shadow(id));
        r.set_declarative_shadow(id, "open", false, false);
        assert!(r.is_declarative_shadow(id));
    }
}
