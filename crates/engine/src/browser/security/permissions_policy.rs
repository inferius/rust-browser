//! Permissions Policy (drive Feature Policy) - controls powerful API access per origin.
//!
//! Spec: https://www.w3.org/TR/permissions-policy/
//! `Permissions-Policy: camera=(), geolocation=(self "https://trusted.com")`

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum AllowList {
    None,
    All,                              // *
    Origins(Vec<String>),             // explicit list
}

#[derive(Debug, Clone)]
pub struct PermissionsPolicy {
    /// feature -> allowlist.
    pub directives: HashMap<String, AllowList>,
}

impl PermissionsPolicy {
    pub fn new() -> Self { Self { directives: HashMap::new() } }

    /// Parse `feature=(self "https://a.com"), feature2=*` syntax.
    pub fn parse(header: &str) -> Self {
        let mut p = Self::new();
        for entry in split_top_level(header, ',') {
            let entry = entry.trim();
            let Some((name, value)) = entry.split_once('=') else { continue; };
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            let allow = parse_allow(value);
            p.directives.insert(name, allow);
        }
        p
    }

    pub fn is_allowed(&self, feature: &str, origin: &str, document_origin: &str) -> bool {
        let Some(allow) = self.directives.get(feature) else {
            // No directive -> use feature default (varies; safe default = self only).
            return origin == document_origin;
        };
        match allow {
            AllowList::None => false,
            AllowList::All => true,
            AllowList::Origins(list) => list.iter().any(|o| {
                o == "self" && origin == document_origin || o == origin
            }),
        }
    }
}

impl Default for PermissionsPolicy {
    fn default() -> Self { Self::new() }
}

fn parse_allow(value: &str) -> AllowList {
    let s = value.trim();
    if s == "*" { return AllowList::All; }
    let inside = s.trim_start_matches('(').trim_end_matches(')').trim();
    if inside.is_empty() { return AllowList::None; }
    let mut origins = Vec::new();
    for tok in inside.split_ascii_whitespace() {
        let tok = tok.trim_matches('"').to_string();
        origins.push(tok);
    }
    AllowList::Origins(origins)
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut paren = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => paren += 1,
            ')' => paren -= 1,
            x if x == sep && paren == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if start < s.len() { out.push(&s[start..]); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_none_empty_parens() {
        let p = PermissionsPolicy::parse("camera=()");
        assert_eq!(p.directives.get("camera"), Some(&AllowList::None));
    }

    #[test]
    fn parse_wildcard() {
        let p = PermissionsPolicy::parse("geolocation=*");
        assert_eq!(p.directives.get("geolocation"), Some(&AllowList::All));
    }

    #[test]
    fn parse_origin_list() {
        let p = PermissionsPolicy::parse("camera=(self \"https://a.com\")");
        if let Some(AllowList::Origins(list)) = p.directives.get("camera") {
            assert!(list.iter().any(|o| o == "self"));
            assert!(list.iter().any(|o| o == "https://a.com"));
        } else { panic!("expected origins"); }
    }

    #[test]
    fn allowed_self() {
        let p = PermissionsPolicy::parse("camera=(self)");
        assert!(p.is_allowed("camera", "https://x.com", "https://x.com"));
        assert!(!p.is_allowed("camera", "https://y.com", "https://x.com"));
    }

    #[test]
    fn allowed_wildcard() {
        let p = PermissionsPolicy::parse("camera=*");
        assert!(p.is_allowed("camera", "https://y.com", "https://x.com"));
    }

    #[test]
    fn allowed_blocked_empty() {
        let p = PermissionsPolicy::parse("camera=()");
        assert!(!p.is_allowed("camera", "https://x.com", "https://x.com"));
    }

    #[test]
    fn missing_directive_falls_to_self() {
        let p = PermissionsPolicy::new();
        assert!(p.is_allowed("camera", "https://x.com", "https://x.com"));
        assert!(!p.is_allowed("camera", "https://y.com", "https://x.com"));
    }

    #[test]
    fn parse_multiple_directives() {
        let p = PermissionsPolicy::parse("camera=(), geolocation=*, microphone=(self)");
        assert_eq!(p.directives.len(), 3);
    }
}
