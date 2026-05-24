//! Cookies s SameSite policy per RFC 6265bis.
//!
//! SameSite values:
//! - Strict: jen pri same-site navigation
//! - Lax: + top-level GET navigation (default modern)
//! - None: cross-site OK (vyzaduje Secure)
//!
//! Inspired by Chromium `net/cookies/cookie_monster.cc`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    pub expires_unix: Option<u64>,
    pub max_age_seconds: Option<i64>,
}

impl Cookie {
    /// Parse Set-Cookie header value.
    /// Format: "name=value; Domain=...; Path=...; Secure; HttpOnly; SameSite=Lax"
    pub fn parse(header: &str, default_domain: &str, default_path: &str) -> Option<Cookie> {
        let mut parts = header.split(';');
        let name_value = parts.next()?.trim();
        let eq_idx = name_value.find('=')?;
        let name = name_value[..eq_idx].trim().to_string();
        let value = name_value[eq_idx+1..].trim().to_string();
        let mut cookie = Cookie {
            name, value,
            domain: default_domain.to_string(),
            path: default_path.to_string(),
            secure: false,
            http_only: false,
            same_site: SameSite::Lax, // modern default
            expires_unix: None,
            max_age_seconds: None,
        };
        for attr in parts {
            let a = attr.trim();
            let (k, v) = match a.find('=') {
                Some(i) => (&a[..i], Some(a[i+1..].trim())),
                None => (a, None),
            };
            match k.to_lowercase().as_str() {
                "domain" => if let Some(v) = v { cookie.domain = v.trim_start_matches('.').to_string(); }
                "path" => if let Some(v) = v { cookie.path = v.to_string(); }
                "secure" => cookie.secure = true,
                "httponly" => cookie.http_only = true,
                "samesite" => if let Some(v) = v {
                    cookie.same_site = match v.to_lowercase().as_str() {
                        "strict" => SameSite::Strict,
                        "lax" => SameSite::Lax,
                        "none" => SameSite::None,
                        _ => SameSite::Lax,
                    };
                }
                "max-age" => if let Some(v) = v {
                    cookie.max_age_seconds = v.parse().ok();
                }
                _ => {}
            }
        }
        // SameSite=None vyzaduje Secure (chrome spec).
        if matches!(cookie.same_site, SameSite::None) && !cookie.secure {
            return None;
        }
        Some(cookie)
    }

    /// Check zda cookie posla v request pres given context.
    /// `same_site_context`: true kdyz request je same-site, false cross-site.
    /// `is_top_level_navigation`: true pri top-level GET navigation.
    pub fn should_send(&self, request_url: &str, same_site_context: bool, is_top_level_navigation: bool) -> bool {
        // Domain match.
        let host = match url_host(request_url) { Some(h) => h, None => return false };
        let domain_match = host == self.domain || host.ends_with(&format!(".{}", self.domain));
        if !domain_match { return false; }
        // Secure cookie jen pres https.
        if self.secure && !request_url.starts_with("https://") { return false; }
        // SameSite check.
        match self.same_site {
            SameSite::Strict => same_site_context,
            SameSite::Lax => same_site_context || is_top_level_navigation,
            SameSite::None => true,
        }
    }
}

#[derive(Default)]
pub struct CookieJar {
    pub cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, c: Cookie) {
        // Replace existing same-key (name+domain+path).
        self.cookies.retain(|x| !(x.name == c.name && x.domain == c.domain && x.path == c.path));
        self.cookies.push(c);
    }

    /// Get Cookie header value pro URL.
    pub fn header_for(&self, url: &str, same_site_context: bool, top_level_nav: bool) -> String {
        let mut parts = Vec::new();
        for c in &self.cookies {
            if c.should_send(url, same_site_context, top_level_nav) {
                parts.push(format!("{}={}", c.name, c.value));
            }
        }
        parts.join("; ")
    }
}

fn url_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://"))?;
    Some(rest.split('/').next()?.split(':').next()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_cookie() {
        let c = Cookie::parse("foo=bar; Path=/; SameSite=Lax",
            "example.com", "/").unwrap();
        assert_eq!(c.name, "foo");
        assert_eq!(c.value, "bar");
        assert_eq!(c.same_site, SameSite::Lax);
    }

    #[test]
    fn samesite_none_requires_secure() {
        let c = Cookie::parse("foo=bar; SameSite=None", "example.com", "/");
        assert!(c.is_none()); // missing Secure
        let c2 = Cookie::parse("foo=bar; SameSite=None; Secure", "example.com", "/");
        assert!(c2.is_some());
    }

    #[test]
    fn samesite_strict_blocks_cross_site() {
        let c = Cookie::parse("foo=bar; SameSite=Strict", "example.com", "/").unwrap();
        assert!(c.should_send("https://example.com/x", true, false));
        assert!(!c.should_send("https://example.com/x", false, true)); // cross-site
    }

    #[test]
    fn samesite_lax_allows_top_level_nav() {
        let c = Cookie::parse("foo=bar; SameSite=Lax", "example.com", "/").unwrap();
        assert!(c.should_send("https://example.com/x", false, true)); // top-level cross-site = allow
        assert!(!c.should_send("https://example.com/x", false, false)); // sub-resource cross-site = block
    }

    #[test]
    fn jar_replaces_same_key() {
        let mut j = CookieJar::new();
        j.add(Cookie::parse("k=v1; SameSite=Lax", "example.com", "/").unwrap());
        j.add(Cookie::parse("k=v2; SameSite=Lax", "example.com", "/").unwrap());
        assert_eq!(j.cookies.len(), 1);
        assert_eq!(j.cookies[0].value, "v2");
    }

    #[test]
    fn jar_header_skips_secure_on_http() {
        let mut j = CookieJar::new();
        j.add(Cookie::parse("k=v; Secure; SameSite=Lax", "example.com", "/").unwrap());
        assert_eq!(j.header_for("http://example.com/", true, false), "");
        assert_eq!(j.header_for("https://example.com/", true, false), "k=v");
    }
}
