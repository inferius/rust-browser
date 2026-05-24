//! Cookie jar - per-host store + Set-Cookie parsing.
//!
//! Spec: RFC 6265 + draft-ietf-httpbis-rfc6265bis (SameSite, partitioned cookies).
//! Stored separately from `browser::security::cookies::Cookie` (which models the
//! single-cookie attribute set); this jar handles persistence + match-on-request.

use std::collections::HashMap;

use crate::browser::security::cookies::{Cookie, SameSite};

#[derive(Default)]
pub struct CookieJar {
    /// Key by (domain, path, name) for uniqueness.
    pub entries: Vec<Cookie>,
}

impl CookieJar {
    pub fn new() -> Self { Self::default() }

    /// Add or replace a cookie.
    pub fn set(&mut self, cookie: Cookie) {
        self.entries.retain(|c| !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path));
        self.entries.push(cookie);
    }

    /// Get the cookies that match a request.
    /// `secure_request` = HTTPS scheme. `same_site_request` = true if request URL same-site to top-level.
    pub fn cookies_for_request(&self, host: &str, path: &str, secure_request: bool, same_site_request: bool) -> Vec<&Cookie> {
        self.entries.iter().filter(|c| {
            domain_matches(host, &c.domain)
            && path_matches(path, &c.path)
            && (!c.secure || secure_request)
            && match c.same_site {
                SameSite::Strict => same_site_request,
                SameSite::Lax => same_site_request,
                SameSite::None => true,
            }
        }).collect()
    }

    pub fn remove_expired(&mut self, now_unix: u64) {
        self.entries.retain(|c| c.expires_unix.map(|e| e > now_unix).unwrap_or(true));
    }

    /// Parse Set-Cookie header value into a Cookie.
    pub fn parse_set_cookie(header: &str, default_domain: &str, default_path: &str) -> Option<Cookie> {
        let mut parts = header.split(';');
        let first = parts.next()?.trim();
        let (name, value) = first.split_once('=')?;
        let mut cookie = Cookie {
            name: name.trim().into(),
            value: value.trim().into(),
            domain: default_domain.into(),
            path: default_path.into(),
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
            expires_unix: None,
            max_age_seconds: None,
        };
        for attr in parts {
            let attr = attr.trim();
            let (k, v) = match attr.find('=') {
                Some(i) => (attr[..i].trim().to_ascii_lowercase(), Some(attr[i + 1..].trim().to_string())),
                None => (attr.to_ascii_lowercase(), None),
            };
            match (k.as_str(), v.as_deref()) {
                ("domain", Some(v)) => cookie.domain = v.trim_start_matches('.').to_ascii_lowercase(),
                ("path", Some(v)) => cookie.path = v.to_string(),
                ("secure", _) => cookie.secure = true,
                ("httponly", _) => cookie.http_only = true,
                ("samesite", Some(v)) => cookie.same_site = match v.to_ascii_lowercase().as_str() {
                    "strict" => SameSite::Strict,
                    "none" => SameSite::None,
                    _ => SameSite::Lax,
                },
                ("max-age", Some(v)) => cookie.max_age_seconds = v.parse().ok(),
                _ => {}
            }
        }
        Some(cookie)
    }
}

pub fn domain_matches(request_host: &str, cookie_domain: &str) -> bool {
    let h = request_host.to_ascii_lowercase();
    let d = cookie_domain.to_ascii_lowercase();
    h == d || h.ends_with(&format!(".{}", d))
}

pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path { return true; }
    if cookie_path.ends_with('/') && request_path.starts_with(cookie_path) { return true; }
    if request_path.starts_with(cookie_path) && request_path.as_bytes().get(cookie_path.len()) == Some(&b'/') {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(name: &str, dom: &str, path: &str, secure: bool, ss: SameSite) -> Cookie {
        Cookie {
            name: name.into(), value: "v".into(),
            domain: dom.into(), path: path.into(),
            secure, http_only: false, same_site: ss,
            expires_unix: None, max_age_seconds: None,
        }
    }

    #[test]
    fn set_replaces_same_name() {
        let mut j = CookieJar::new();
        j.set(make("a", "x.com", "/", false, SameSite::Lax));
        j.set(make("a", "x.com", "/", false, SameSite::Lax));
        assert_eq!(j.entries.len(), 1);
    }

    #[test]
    fn cookies_for_request_filters_secure() {
        let mut j = CookieJar::new();
        j.set(make("a", "x.com", "/", true, SameSite::Lax));
        assert!(j.cookies_for_request("x.com", "/", false, true).is_empty());
        assert_eq!(j.cookies_for_request("x.com", "/", true, true).len(), 1);
    }

    #[test]
    fn same_site_none_passes_cross_site() {
        let mut j = CookieJar::new();
        j.set(make("a", "x.com", "/", false, SameSite::None));
        assert_eq!(j.cookies_for_request("x.com", "/", false, false).len(), 1);
    }

    #[test]
    fn parse_attributes() {
        let c = CookieJar::parse_set_cookie(
            "id=abc; Domain=x.com; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=3600",
            "x.com", "/",
        ).unwrap();
        assert_eq!(c.name, "id");
        assert_eq!(c.value, "abc");
        assert!(c.secure);
        assert!(c.http_only);
        assert_eq!(c.same_site, SameSite::Strict);
        assert_eq!(c.max_age_seconds, Some(3600));
    }

    #[test]
    fn domain_match_includes_subdomain() {
        assert!(domain_matches("a.b.x.com", "x.com"));
        assert!(domain_matches("x.com", "x.com"));
        assert!(!domain_matches("xx.com", "x.com"));
    }

    #[test]
    fn path_match_segments() {
        assert!(path_matches("/a/b", "/a"));
        assert!(path_matches("/a/", "/a/"));
        assert!(path_matches("/", "/"));
        assert!(!path_matches("/ax", "/a"));
    }
}
