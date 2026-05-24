//! Lightweight ad-blocker style filter rule engine.
//!
//! Inspired by uBlock Origin / ABP filter syntax. Two rule kinds:
//! - Block rules: `||example.com/ads^`
//! - Exception rules: `@@||example.com/ads`

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleKind {
    Block,
    Exception,
}

#[derive(Debug, Clone)]
pub struct FilterRule {
    pub raw: String,
    pub kind: RuleKind,
    pub pattern: String,
    pub domains: Vec<String>,
    pub element_hide: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterDecision {
    Allow,
    Block,
}

#[derive(Default)]
pub struct FilterEngine {
    pub rules: Vec<FilterRule>,
    pub plain_block_hosts: HashSet<String>,    // fast-path for plain hostname rules
}

impl FilterEngine {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, rule: &str) {
        let raw = rule.trim();
        if raw.is_empty() || raw.starts_with('!') { return; }
        let (kind, body) = if let Some(rest) = raw.strip_prefix("@@") {
            (RuleKind::Exception, rest)
        } else {
            (RuleKind::Block, raw)
        };
        let element_hide = body.contains("##");
        let pattern = body.trim_start_matches("||").trim_end_matches('^').to_string();
        if kind == RuleKind::Block && !pattern.contains('/') && !pattern.contains('*') && !element_hide {
            self.plain_block_hosts.insert(pattern.to_ascii_lowercase());
        }
        self.rules.push(FilterRule {
            raw: raw.into(),
            kind,
            pattern,
            domains: Vec::new(),
            element_hide,
        });
    }

    /// Check URL against rules. Exceptions override blocks.
    pub fn check(&self, url: &str) -> FilterDecision {
        let url_lower = url.to_ascii_lowercase();
        // Exception first.
        for r in self.rules.iter().filter(|r| r.kind == RuleKind::Exception) {
            if url_lower.contains(&r.pattern) {
                return FilterDecision::Allow;
            }
        }
        // Plain host fast-path.
        if let Some(host) = extract_host(&url_lower) {
            if self.plain_block_hosts.contains(&host) { return FilterDecision::Block; }
            // Sub-host
            let mut parts: Vec<&str> = host.split('.').collect();
            while parts.len() > 1 {
                parts.remove(0);
                let parent = parts.join(".");
                if self.plain_block_hosts.contains(&parent) { return FilterDecision::Block; }
            }
        }
        for r in self.rules.iter().filter(|r| r.kind == RuleKind::Block) {
            if url_lower.contains(&r.pattern) {
                return FilterDecision::Block;
            }
        }
        FilterDecision::Allow
    }
}

fn extract_host(url: &str) -> Option<String> {
    let after = url.split("://").nth(1)?;
    let end = after.find(|c: char| c == '/' || c == '?' || c == '#').unwrap_or(after.len());
    Some(after[..end].split(':').next().unwrap_or("").to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_host_block() {
        let mut e = FilterEngine::new();
        e.add("||ads.example.com");
        assert_eq!(e.check("https://ads.example.com/banner.png"), FilterDecision::Block);
        assert_eq!(e.check("https://safe.com"), FilterDecision::Allow);
    }

    #[test]
    fn exception_overrides() {
        let mut e = FilterEngine::new();
        e.add("||ads.example.com");
        e.add("@@||ads.example.com/safe.js");
        assert_eq!(e.check("https://ads.example.com/safe.js"), FilterDecision::Allow);
    }

    #[test]
    fn pattern_in_path() {
        let mut e = FilterEngine::new();
        e.add("/tracker/");
        assert_eq!(e.check("https://x.com/tracker/pixel.gif"), FilterDecision::Block);
    }

    #[test]
    fn comment_ignored() {
        let mut e = FilterEngine::new();
        e.add("! comment");
        e.add("||ads.com");
        assert_eq!(e.rules.len(), 1);
    }

    #[test]
    fn subdomain_block() {
        let mut e = FilterEngine::new();
        e.add("||ads.com");
        assert_eq!(e.check("https://api.ads.com/track"), FilterDecision::Block);
    }
}
