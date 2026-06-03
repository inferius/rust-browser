//! Trusted Types API - XSS prevention pres policy-bound sanitizers.
//!
//! Spec: https://w3c.github.io/trusted-types/
//! window.trustedTypes.createPolicy(name, {createHTML, createScript, createScriptURL})
//! - innerHTML/etc accept jen TrustedHTML/TrustedScript/etc.

use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrustedTypeKind {
    Html,
    Script,
    ScriptUrl,
}

#[derive(Debug, Clone)]
pub struct TrustedValue {
    pub kind: TrustedTypeKind,
    pub value: String,
    pub policy: String,
}

pub struct TrustedTypePolicy {
    pub name: String,
    pub create_html: Option<Box<dyn Fn(&str) -> String>>,
    pub create_script: Option<Box<dyn Fn(&str) -> String>>,
    pub create_script_url: Option<Box<dyn Fn(&str) -> String>>,
}

impl TrustedTypePolicy {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            create_html: None,
            create_script: None,
            create_script_url: None,
        }
    }
}

#[derive(Default)]
pub struct TrustedTypesFactory {
    pub policies: HashMap<String, Rc<TrustedTypePolicy>>,
    /// CSP: 'require-trusted-types-for' enforces.
    pub enforce: bool,
    /// Allowed policy names (CSP trusted-types directive).
    pub allowed_names: Option<Vec<String>>,
}

impl TrustedTypesFactory {
    pub fn new() -> Self { Self::default() }

    pub fn create_policy(&mut self, name: &str, policy: TrustedTypePolicy) -> Result<Rc<TrustedTypePolicy>, String> {
        if let Some(allowed) = &self.allowed_names {
            if !allowed.iter().any(|n| n == name || n == "*") {
                return Err(format!("Policy '{}' not in CSP trusted-types allowlist", name));
            }
        }
        if self.policies.contains_key(name) && name != "default" {
            return Err(format!("Policy '{}' already exists", name));
        }
        let rc = Rc::new(policy);
        self.policies.insert(name.into(), Rc::clone(&rc));
        Ok(rc)
    }

    pub fn get_policy(&self, name: &str) -> Option<&Rc<TrustedTypePolicy>> {
        self.policies.get(name)
    }

    /// Pri enforce: vse non-TrustedValue rejecten. Foundation = check.
    pub fn validate(&self, value: &TrustedValue, expected_kind: TrustedTypeKind) -> bool {
        if value.kind != expected_kind { return false; }
        self.policies.contains_key(&value.policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_policy_succeeds() {
        let mut t = TrustedTypesFactory::new();
        let p = TrustedTypePolicy::new("default");
        assert!(t.create_policy("default", p).is_ok());
    }

    #[test]
    fn duplicate_policy_rejected() {
        let mut t = TrustedTypesFactory::new();
        t.create_policy("p1", TrustedTypePolicy::new("p1")).unwrap();
        assert!(t.create_policy("p1", TrustedTypePolicy::new("p1")).is_err());
    }

    #[test]
    fn default_can_be_replaced() {
        let mut t = TrustedTypesFactory::new();
        t.create_policy("default", TrustedTypePolicy::new("default")).unwrap();
        // Default policy = exception, can replace.
        assert!(t.create_policy("default", TrustedTypePolicy::new("default")).is_ok());
    }

    #[test]
    fn csp_allowlist_enforced() {
        let mut t = TrustedTypesFactory::new();
        t.allowed_names = Some(vec!["safe-policy".into()]);
        assert!(t.create_policy("evil", TrustedTypePolicy::new("evil")).is_err());
        assert!(t.create_policy("safe-policy", TrustedTypePolicy::new("safe-policy")).is_ok());
    }

    #[test]
    fn validate_kind_match() {
        let mut t = TrustedTypesFactory::new();
        t.create_policy("p", TrustedTypePolicy::new("p")).unwrap();
        let v = TrustedValue {
            kind: TrustedTypeKind::Html,
            value: "<b>safe</b>".into(),
            policy: "p".into(),
        };
        assert!(t.validate(&v, TrustedTypeKind::Html));
        assert!(!t.validate(&v, TrustedTypeKind::Script));
    }
}
