//! Referrer Policy - controls Referer header on navigations + subresource requests.
//!
//! Spec: https://www.w3.org/TR/referrer-policy/
//! Default: strict-origin-when-cross-origin.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    SameOrigin,
    Origin,
    StrictOrigin,
    OriginWhenCrossOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

impl ReferrerPolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "no-referrer" => Some(Self::NoReferrer),
            "no-referrer-when-downgrade" => Some(Self::NoReferrerWhenDowngrade),
            "same-origin" => Some(Self::SameOrigin),
            "origin" => Some(Self::Origin),
            "strict-origin" => Some(Self::StrictOrigin),
            "origin-when-cross-origin" => Some(Self::OriginWhenCrossOrigin),
            "strict-origin-when-cross-origin" => Some(Self::StrictOriginWhenCrossOrigin),
            "unsafe-url" => Some(Self::UnsafeUrl),
            _ => None,
        }
    }

    /// Default per spec.
    pub fn default() -> Self { Self::StrictOriginWhenCrossOrigin }
}

/// Compute the Referer header value for a request.
/// from_url = page making request, to_url = target.
/// Returns None if no referrer should be sent.
pub fn compute_referrer(policy: ReferrerPolicy, from_url: &str, to_url: &str) -> Option<String> {
    let same_origin = same_origin(from_url, to_url);
    let downgrade = is_downgrade(from_url, to_url);
    let origin_only = origin_of(from_url);
    let full = strip_fragment_and_credentials(from_url);

    match policy {
        ReferrerPolicy::NoReferrer => None,
        ReferrerPolicy::NoReferrerWhenDowngrade => {
            if downgrade { None } else { Some(full) }
        }
        ReferrerPolicy::SameOrigin => {
            if same_origin { Some(full) } else { None }
        }
        ReferrerPolicy::Origin => Some(origin_only),
        ReferrerPolicy::StrictOrigin => {
            if downgrade { None } else { Some(origin_only) }
        }
        ReferrerPolicy::OriginWhenCrossOrigin => {
            if same_origin { Some(full) } else { Some(origin_only) }
        }
        ReferrerPolicy::StrictOriginWhenCrossOrigin => {
            if same_origin { Some(full) }
            else if downgrade { None }
            else { Some(origin_only) }
        }
        ReferrerPolicy::UnsafeUrl => Some(full),
    }
}

fn origin_of(url: &str) -> String {
    // scheme://host[:port]
    let after_scheme = match url.find("://") {
        Some(i) => &url[..i + 3],
        None => return url.to_string(),
    };
    let rest = &url[after_scheme.len()..];
    let host_end = rest.find(|c: char| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    format!("{}{}", after_scheme, &rest[..host_end])
}

fn same_origin(a: &str, b: &str) -> bool {
    origin_of(a) == origin_of(b)
}

fn is_downgrade(from: &str, to: &str) -> bool {
    from.starts_with("https://") && to.starts_with("http://")
}

fn strip_fragment_and_credentials(url: &str) -> String {
    let no_frag = match url.find('#') {
        Some(i) => &url[..i],
        None => url,
    };
    // Strip user:pass@ from authority component.
    let scheme_end = no_frag.find("://");
    if let Some(s) = scheme_end {
        let after = &no_frag[s + 3..];
        if let Some(at) = after.find('@') {
            let auth_end = after.find('/').unwrap_or(after.len());
            if at < auth_end {
                return format!("{}{}", &no_frag[..s + 3], &after[at + 1..]);
            }
        }
    }
    no_frag.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_referrer_returns_none() {
        let r = compute_referrer(ReferrerPolicy::NoReferrer, "https://x.com/a", "https://y.com");
        assert!(r.is_none());
    }

    #[test]
    fn unsafe_url_returns_full() {
        let r = compute_referrer(ReferrerPolicy::UnsafeUrl, "https://x.com/a?q=1", "http://y.com");
        assert_eq!(r.as_deref(), Some("https://x.com/a?q=1"));
    }

    #[test]
    fn strict_origin_downgrade_drops() {
        let r = compute_referrer(ReferrerPolicy::StrictOrigin, "https://x.com/a", "http://y.com");
        assert!(r.is_none());
    }

    #[test]
    fn strict_origin_when_cross_origin_same_origin_full() {
        let r = compute_referrer(ReferrerPolicy::StrictOriginWhenCrossOrigin, "https://x.com/a", "https://x.com/b");
        assert_eq!(r.as_deref(), Some("https://x.com/a"));
    }

    #[test]
    fn strict_origin_when_cross_origin_cross_origin_https() {
        let r = compute_referrer(ReferrerPolicy::StrictOriginWhenCrossOrigin, "https://x.com/a", "https://y.com");
        assert_eq!(r.as_deref(), Some("https://x.com"));
    }

    #[test]
    fn origin_strips_path() {
        assert_eq!(origin_of("https://x.com:8080/path?q"), "https://x.com:8080");
    }

    #[test]
    fn strips_fragment_and_creds() {
        let r = strip_fragment_and_credentials("https://user:pass@x.com/path#frag");
        assert_eq!(r, "https://x.com/path");
    }

    #[test]
    fn parse_known() {
        assert_eq!(ReferrerPolicy::parse("no-referrer"), Some(ReferrerPolicy::NoReferrer));
        assert_eq!(ReferrerPolicy::parse("Strict-Origin"), Some(ReferrerPolicy::StrictOrigin));
        assert_eq!(ReferrerPolicy::parse("garbage"), None);
    }
}
