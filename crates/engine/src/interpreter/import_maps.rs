//! Import maps - bare specifier resolution per HTML spec.
//!
//! Spec: https://html.spec.whatwg.org/multipage/webappapis.html#import-maps
//! `<script type="importmap">{"imports": {"lodash": "/lodash.mjs"}}</script>`

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ImportMap {
    pub imports: HashMap<String, String>,
    /// Scope-restricted maps - keyed by URL prefix.
    pub scopes: HashMap<String, HashMap<String, String>>,
    /// Integrity hashes per resolved URL.
    pub integrity: HashMap<String, String>,
}

impl ImportMap {
    pub fn new() -> Self { Self::default() }

    /// Parse JSON-like import map. Real impl pipes via serde_json; here a tiny manual parser.
    pub fn parse_minimal(json: &str) -> Result<Self, String> {
        let mut map = Self::new();
        // Expecting object {"imports": {...}, "scopes": {...}}.
        // For simplicity, support flat "imports" only via regex-free string scan.
        if let Some(imports_start) = json.find("\"imports\"") {
            let after = &json[imports_start..];
            if let Some(open) = after.find('{') {
                let chunk = &after[open + 1..];
                if let Some(close) = chunk.find('}') {
                    let entries = &chunk[..close];
                    for entry in entries.split(',') {
                        let entry = entry.trim();
                        if entry.is_empty() { continue; }
                        let Some(colon) = entry.find(':') else { continue; };
                        let k = entry[..colon].trim().trim_matches('"').to_string();
                        let v = entry[colon + 1..].trim().trim_matches('"').to_string();
                        if !k.is_empty() && !v.is_empty() {
                            map.imports.insert(k, v);
                        }
                    }
                }
            }
        }
        Ok(map)
    }

    /// Resolve bare specifier (per spec):
    /// 1. Check scopes from most-specific to least.
    /// 2. Check top-level imports map.
    /// 3. Match longest prefix that ends with `/`.
    pub fn resolve(&self, specifier: &str, referrer_url: &str) -> Option<String> {
        // Scopes
        let mut scope_urls: Vec<&String> = self.scopes.keys().collect();
        scope_urls.sort_by_key(|u| std::cmp::Reverse(u.len()));
        for scope in scope_urls {
            if referrer_url.starts_with(scope) {
                if let Some(target) = self.scopes[scope].get(specifier) {
                    return Some(target.clone());
                }
                if let Some(target) = longest_prefix_match(specifier, &self.scopes[scope]) {
                    return Some(target);
                }
            }
        }
        // Top-level
        if let Some(target) = self.imports.get(specifier) {
            return Some(target.clone());
        }
        longest_prefix_match(specifier, &self.imports)
    }
}

fn longest_prefix_match(specifier: &str, map: &HashMap<String, String>) -> Option<String> {
    let mut best: Option<(&String, &String)> = None;
    for (k, v) in map {
        if k.ends_with('/') && specifier.starts_with(k) {
            if best.map(|(bk, _)| k.len() > bk.len()).unwrap_or(true) {
                best = Some((k, v));
            }
        }
    }
    best.map(|(k, v)| format!("{}{}", v, &specifier[k.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ImportMap {
        let mut m = ImportMap::new();
        m.imports.insert("lodash".into(), "/cdn/lodash.mjs".into());
        m.imports.insert("@org/".into(), "/vendor/org/".into());
        m
    }

    #[test]
    fn exact_match() {
        let r = map().resolve("lodash", "https://x.com/main.mjs");
        assert_eq!(r.as_deref(), Some("/cdn/lodash.mjs"));
    }

    #[test]
    fn prefix_match() {
        let r = map().resolve("@org/utils", "https://x.com/main.mjs");
        assert_eq!(r.as_deref(), Some("/vendor/org/utils"));
    }

    #[test]
    fn no_match_returns_none() {
        let r = map().resolve("unknown", "https://x.com/main.mjs");
        assert!(r.is_none());
    }

    #[test]
    fn scope_overrides_top_level() {
        let mut m = map();
        let mut scope = HashMap::new();
        scope.insert("lodash".to_string(), "/scope/lodash.mjs".to_string());
        m.scopes.insert("https://x.com/sub/".to_string(), scope);
        let r = m.resolve("lodash", "https://x.com/sub/page.mjs");
        assert_eq!(r.as_deref(), Some("/scope/lodash.mjs"));
    }

    #[test]
    fn longest_prefix_wins() {
        let mut m = ImportMap::new();
        m.imports.insert("a/".into(), "/short/".into());
        m.imports.insert("a/b/".into(), "/long/".into());
        let r = m.resolve("a/b/c", "");
        assert_eq!(r.as_deref(), Some("/long/c"));
    }

    #[test]
    fn parse_simple_json() {
        let m = ImportMap::parse_minimal(r#"{"imports": {"lodash": "/cdn/lodash.mjs"}}"#).unwrap();
        assert_eq!(m.imports.get("lodash").map(|s| s.as_str()), Some("/cdn/lodash.mjs"));
    }
}
