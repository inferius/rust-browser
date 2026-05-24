//! Content Security Policy (CSP) parser + enforcer.
//!
//! Per spec [CSP3](https://www.w3.org/TR/CSP3/). Parse `Content-Security-Policy`
//! HTTP header / `<meta http-equiv>` content. Enforce pri:
//! - Script load (script-src)
//! - Style load (style-src)
//! - Image load (img-src)
//! - Network fetch (connect-src)
//! - Frame load (frame-src)
//! - Default fallback (default-src)
//!
//! Source list keywords: 'self', 'unsafe-inline', 'unsafe-eval', 'none', 'strict-dynamic'.
//! Pattern: scheme (https:), host (example.com), wildcard (*.example.com).
//!
//! Inspired by Chromium `content/browser/renderer_host/csp_context.cc`.

use std::collections::HashMap;

/// Parsed CSP directive - typ + source list.
#[derive(Debug, Clone, Default)]
pub struct CspPolicy {
    pub directives: HashMap<String, Vec<CspSource>>,
    /// Report-only mode: log violations bez enforce.
    pub report_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CspSource {
    None,
    Self_,
    UnsafeInline,
    UnsafeEval,
    StrictDynamic,
    Scheme(String),       // "https:"
    Host(String),         // "example.com"
    HostWildcard(String), // "*.example.com" -> stored without "*"
    Nonce(String),        // 'nonce-XYZ'
    Hash(String),         // 'sha256-...'
}

impl CspPolicy {
    /// Parse Content-Security-Policy header value.
    /// Format: "directive1 source1 source2; directive2 source3; ..."
    pub fn parse(header: &str) -> Self {
        let mut policy = CspPolicy::default();
        for directive in header.split(';') {
            let directive = directive.trim();
            if directive.is_empty() { continue; }
            let mut parts = directive.split_whitespace();
            let name = match parts.next() { Some(n) => n.to_lowercase(), None => continue };
            let sources: Vec<CspSource> = parts.map(parse_source).collect();
            policy.directives.insert(name, sources);
        }
        policy
    }

    /// Vraci true kdyz URL je allowed pres directive (s default-src fallback).
    pub fn allows(&self, directive: &str, url: &str, origin: &str) -> bool {
        let sources = self.directives.get(directive)
            .or_else(|| self.directives.get("default-src"));
        let sources = match sources { Some(s) => s, None => return true };
        if sources.iter().any(|s| matches!(s, CspSource::None)) {
            return false;
        }
        sources.iter().any(|s| source_matches(s, url, origin))
    }

    /// Check inline script execution allowed (style equivalent stejne).
    pub fn allows_inline_script(&self) -> bool {
        let sources = self.directives.get("script-src")
            .or_else(|| self.directives.get("default-src"));
        let sources = match sources { Some(s) => s, None => return true };
        sources.iter().any(|s| matches!(s, CspSource::UnsafeInline))
    }

    /// Check eval() allowed.
    pub fn allows_eval(&self) -> bool {
        let sources = self.directives.get("script-src")
            .or_else(|| self.directives.get("default-src"));
        let sources = match sources { Some(s) => s, None => return true };
        sources.iter().any(|s| matches!(s, CspSource::UnsafeEval))
    }
}

fn parse_source(s: &str) -> CspSource {
    let s = s.trim();
    match s.to_lowercase().as_str() {
        "'none'" => CspSource::None,
        "'self'" => CspSource::Self_,
        "'unsafe-inline'" => CspSource::UnsafeInline,
        "'unsafe-eval'" => CspSource::UnsafeEval,
        "'strict-dynamic'" => CspSource::StrictDynamic,
        _ => {
            if let Some(nonce) = s.strip_prefix("'nonce-").and_then(|x| x.strip_suffix('\'')) {
                CspSource::Nonce(nonce.to_string())
            } else if let Some(hash) = s.strip_prefix("'sha")
                .and_then(|_| s.strip_prefix('\''))
                .and_then(|x| x.strip_suffix('\''))
            {
                CspSource::Hash(hash.to_string())
            } else if s.ends_with(':') {
                CspSource::Scheme(s.to_string())
            } else if let Some(host) = s.strip_prefix("*.") {
                CspSource::HostWildcard(host.to_string())
            } else {
                CspSource::Host(s.to_string())
            }
        }
    }
}

fn source_matches(src: &CspSource, url: &str, origin: &str) -> bool {
    match src {
        CspSource::None => false,
        CspSource::Self_ => url_origin(url) == origin,
        CspSource::UnsafeInline | CspSource::UnsafeEval | CspSource::StrictDynamic => false,
        CspSource::Scheme(s) => url.starts_with(s.as_str()),
        CspSource::Host(h) => url_host(url).map(|u| u == *h).unwrap_or(false),
        CspSource::HostWildcard(h) => {
            // *.example.com matchne sub.example.com, NEMEL by match example.com.
            url_host(url).map(|u| u.ends_with(&format!(".{}", h))).unwrap_or(false)
        }
        CspSource::Nonce(_) | CspSource::Hash(_) => false, // matching pres script attr
    }
}

fn url_origin(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) {
        let host = rest.split('/').next().unwrap_or("");
        let scheme = if url.starts_with("https:") { "https" } else { "http" };
        format!("{}://{}", scheme, host)
    } else { String::new() }
}

fn url_host(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) {
        Some(rest.split('/').next()?.split(':').next()?.to_string())
    } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let p = CspPolicy::parse("default-src 'self'; script-src 'self' https://cdn.example.com");
        assert_eq!(p.directives.len(), 2);
        assert!(matches!(p.directives.get("default-src").unwrap()[0], CspSource::Self_));
    }

    #[test]
    fn allows_self_origin() {
        let p = CspPolicy::parse("default-src 'self'");
        assert!(p.allows("script-src", "https://example.com/app.js", "https://example.com"));
        assert!(!p.allows("script-src", "https://evil.com/x.js", "https://example.com"));
    }

    #[test]
    fn allows_scheme() {
        let p = CspPolicy::parse("img-src https:");
        assert!(p.allows("img-src", "https://example.com/img.png", "https://other.com"));
        assert!(!p.allows("img-src", "http://example.com/img.png", "https://other.com"));
    }

    #[test]
    fn allows_wildcard_host() {
        let p = CspPolicy::parse("script-src *.example.com");
        assert!(p.allows("script-src", "https://cdn.example.com/lib.js", "https://other.com"));
        assert!(p.allows("script-src", "https://api.example.com/v1.js", "https://other.com"));
        assert!(!p.allows("script-src", "https://example.com/x.js", "https://other.com")); // exact not subdomain
    }

    #[test]
    fn none_blocks_all() {
        let p = CspPolicy::parse("script-src 'none'");
        assert!(!p.allows("script-src", "https://example.com/x.js", "https://example.com"));
    }

    #[test]
    fn default_src_fallback() {
        let p = CspPolicy::parse("default-src 'self'");
        // img-src not specified -> fallback to default-src
        assert!(p.allows("img-src", "https://example.com/img.png", "https://example.com"));
        assert!(!p.allows("img-src", "https://evil.com/img.png", "https://example.com"));
    }

    #[test]
    fn unsafe_inline_detected() {
        let p = CspPolicy::parse("script-src 'self' 'unsafe-inline'");
        assert!(p.allows_inline_script());
        let p2 = CspPolicy::parse("script-src 'self'");
        assert!(!p2.allows_inline_script());
    }

    #[test]
    fn unsafe_eval_detected() {
        let p = CspPolicy::parse("script-src 'unsafe-eval'");
        assert!(p.allows_eval());
        let p2 = CspPolicy::parse("script-src 'self'");
        assert!(!p2.allows_eval());
    }
}
