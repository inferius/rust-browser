//! Speculation Rules API - prerender/prefetch link hints.
//!
//! Spec: https://wicg.github.io/nav-speculation/speculation-rules.html
//! `<script type="speculationrules">{"prerender":[{"source":"list","urls":["/a"]}]}</script>`

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeculationAction {
    Prefetch,
    PrefetchWithSubresources,
    Prerender,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeculationSource {
    List,
    DocumentLinks,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeculationEagerness {
    Immediate,        // Trigger now (high cost, only known-needed)
    Eager,            // Hover + 100ms
    Moderate,         // Hover + 200ms (default)
    Conservative,     // Pointerdown
}

#[derive(Debug, Clone)]
pub struct SpeculationRule {
    pub action: SpeculationAction,
    pub source: SpeculationSource,
    pub urls: Vec<String>,
    pub where_predicate: Option<String>,   // CSS selector for "document" source
    pub eagerness: SpeculationEagerness,
    pub requires_anonymous_client_ip: bool,
}

#[derive(Default)]
pub struct SpeculationRuleSet {
    pub rules: Vec<SpeculationRule>,
    /// URL -> last triggered timestamp; debounce repeat triggers.
    pub triggered: HashMap<String, u64>,
}

impl SpeculationRuleSet {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, rule: SpeculationRule) {
        self.rules.push(rule);
    }

    pub fn urls_for_action(&self, action: SpeculationAction) -> Vec<&str> {
        self.rules.iter()
            .filter(|r| r.action == action)
            .flat_map(|r| r.urls.iter().map(|u| u.as_str()))
            .collect()
    }

    /// Returns true if `url` already triggered within debounce window.
    pub fn was_recently_triggered(&self, url: &str, now: u64, window_ms: u64) -> bool {
        self.triggered.get(url).map(|t| now - t < window_ms).unwrap_or(false)
    }

    pub fn mark_triggered(&mut self, url: &str, now: u64) {
        self.triggered.insert(url.into(), now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(urls: &[&str], action: SpeculationAction) -> SpeculationRule {
        SpeculationRule {
            action, source: SpeculationSource::List,
            urls: urls.iter().map(|s| s.to_string()).collect(),
            where_predicate: None,
            eagerness: SpeculationEagerness::Moderate,
            requires_anonymous_client_ip: false,
        }
    }

    #[test]
    fn urls_filtered_by_action() {
        let mut s = SpeculationRuleSet::new();
        s.add(rule(&["/a", "/b"], SpeculationAction::Prerender));
        s.add(rule(&["/c"], SpeculationAction::Prefetch));
        assert_eq!(s.urls_for_action(SpeculationAction::Prerender).len(), 2);
        assert_eq!(s.urls_for_action(SpeculationAction::Prefetch).len(), 1);
    }

    #[test]
    fn debounce_works() {
        let mut s = SpeculationRuleSet::new();
        s.mark_triggered("/a", 1000);
        assert!(s.was_recently_triggered("/a", 1500, 1000));
        assert!(!s.was_recently_triggered("/a", 3000, 1000));
    }

    #[test]
    fn eagerness_defaults_moderate() {
        let r = rule(&["/x"], SpeculationAction::Prerender);
        assert_eq!(r.eagerness, SpeculationEagerness::Moderate);
    }
}
