//! Custom Elements v1 - customElements.define + lifecycle callbacks.
//!
//! Spec: https://html.spec.whatwg.org/multipage/custom-elements.html
//! customElements.define(name, ctor, {extends})
//! Lifecycle: constructor, connectedCallback, disconnectedCallback,
//!            attributeChangedCallback, adoptedCallback.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LifecycleCallback {
    Connected,
    Disconnected,
    AttributeChanged,
    Adopted,
    FormAssociated,
    FormDisabled,
    FormReset,
    FormStateRestore,
}

#[derive(Debug, Clone)]
pub struct CustomElementDef {
    pub name: String,
    /// If set: built-in extension (e.g. extends "button" -> <button is="x-btn">).
    pub extends: Option<String>,
    pub observed_attributes: Vec<String>,
    pub form_associated: bool,
    /// IDs of the JS constructor function values (engine-internal lookup table).
    pub constructor_id: u64,
    pub callbacks: Vec<LifecycleCallback>,
}

#[derive(Default)]
pub struct CustomElementRegistry {
    pub definitions: HashMap<String, CustomElementDef>,
    /// Pending upgrades - element creation captured before define().
    pub pending_upgrades: HashMap<String, Vec<u64>>, // name -> element ids
}

impl CustomElementRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn define(&mut self, def: CustomElementDef) -> Result<(), String> {
        if !is_valid_custom_name(&def.name) {
            return Err(format!("invalid custom element name '{}'", def.name));
        }
        if self.definitions.contains_key(&def.name) {
            return Err(format!("custom element '{}' already defined", def.name));
        }
        let name = def.name.clone();
        self.definitions.insert(name.clone(), def);
        // Caller should upgrade pending instances - here we just clear them.
        // upgrade() helper returns the list to upgrade.
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&CustomElementDef> {
        self.definitions.get(name)
    }

    /// Capture creation of `<x-foo>` before define() was called.
    pub fn record_pending(&mut self, name: &str, element_id: u64) {
        if self.definitions.contains_key(name) { return; }
        self.pending_upgrades.entry(name.into()).or_default().push(element_id);
    }

    /// Returns elements to upgrade now that `name` has been defined.
    pub fn drain_pending(&mut self, name: &str) -> Vec<u64> {
        self.pending_upgrades.remove(name).unwrap_or_default()
    }
}

/// Custom element name validity per spec:
/// - Must start with lowercase ASCII letter.
/// - Must contain at least one hyphen `-`.
/// - Allowed chars: a-z, 0-9, '-', '.', '_', '\u{B7}', and CJK ranges (simplified).
/// - Must not be a reserved name (e.g. "annotation-xml", "color-profile", ...)
pub fn is_valid_custom_name(name: &str) -> bool {
    if name.is_empty() { return false; }
    let bytes = name.as_bytes();
    if !(b'a'..=b'z').contains(&bytes[0]) { return false; }
    if !name.contains('-') { return false; }
    for c in name.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit()
           && c != '-' && c != '.' && c != '_' && c != '\u{B7}' {
            return false;
        }
    }
    !matches!(name,
        "annotation-xml" | "color-profile" | "font-face" | "font-face-src"
        | "font-face-uri" | "font-face-format" | "font-face-name" | "missing-glyph"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> CustomElementDef {
        CustomElementDef {
            name: name.into(),
            extends: None,
            observed_attributes: Vec::new(),
            form_associated: false,
            constructor_id: 1,
            callbacks: vec![LifecycleCallback::Connected],
        }
    }

    #[test]
    fn define_valid_name() {
        let mut r = CustomElementRegistry::new();
        assert!(r.define(def("my-element")).is_ok());
    }

    #[test]
    fn define_duplicate_rejected() {
        let mut r = CustomElementRegistry::new();
        r.define(def("my-el")).unwrap();
        assert!(r.define(def("my-el")).is_err());
    }

    #[test]
    fn reject_no_hyphen() {
        let mut r = CustomElementRegistry::new();
        assert!(r.define(def("nodash")).is_err());
    }

    #[test]
    fn reject_uppercase_start() {
        let mut r = CustomElementRegistry::new();
        assert!(r.define(def("My-El")).is_err());
    }

    #[test]
    fn reject_reserved_name() {
        let mut r = CustomElementRegistry::new();
        assert!(r.define(def("font-face")).is_err());
    }

    #[test]
    fn pending_upgrade_path() {
        let mut r = CustomElementRegistry::new();
        r.record_pending("my-el", 100);
        r.record_pending("my-el", 200);
        r.define(def("my-el")).unwrap();
        let ids = r.drain_pending("my-el");
        assert_eq!(ids, vec![100, 200]);
    }

    #[test]
    fn extended_builtin() {
        let mut d = def("my-btn");
        d.extends = Some("button".into());
        let mut r = CustomElementRegistry::new();
        r.define(d).unwrap();
        assert_eq!(r.get("my-btn").unwrap().extends.as_deref(), Some("button"));
    }
}
