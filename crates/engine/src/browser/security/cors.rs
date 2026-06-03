//! CORS (Cross-Origin Resource Sharing) per Fetch spec §3.
//!
//! Browser blokuje cross-origin response read pres JS unless origin v
//! `Access-Control-Allow-Origin` header. Pri non-simple requests (custom
//! headers, PUT/DELETE, etc.) preflight OPTIONS check.
//!
//! Inspired by Chromium `services/network/cors/cors_url_loader.cc`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CorsMode {
    SameOrigin,    // jen same-origin, jinak fail
    Cors,          // cross-origin OK kdyz allowed
    NoCors,        // cross-origin allow ale opaque response
    Navigate,      // top-level navigation
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CorsCredentials {
    Omit,          // no cookies/auth
    SameOrigin,    // jen pro same-origin
    Include,       // vzdy
}

/// CORS check pred request - rozhoduje, jestli preflight needed.
pub fn needs_preflight(method: &str, headers: &[(&str, &str)]) -> bool {
    let safe_methods = matches!(method.to_uppercase().as_str(),
        "GET" | "HEAD" | "POST");
    if !safe_methods { return true; }
    // Pripustne headers bez preflight:
    // Accept, Accept-Language, Content-Language, Content-Type (jen application/x-www-form-urlencoded,
    // multipart/form-data, text/plain), Range.
    for (name, value) in headers {
        let n = name.to_lowercase();
        let safe_header = match n.as_str() {
            "accept" | "accept-language" | "content-language" | "range" => true,
            "content-type" => {
                let v = value.to_lowercase();
                v.starts_with("application/x-www-form-urlencoded")
                || v.starts_with("multipart/form-data")
                || v.starts_with("text/plain")
            }
            _ => false,
        };
        if !safe_header { return true; }
    }
    false
}

/// Check Access-Control-Allow-Origin response header. Vraci true pokud read
/// allowed pro origin.
pub fn check_allow_origin(
    response_header: Option<&str>,
    request_origin: &str,
    credentials: CorsCredentials,
) -> bool {
    let allow = match response_header { Some(h) => h.trim(), None => return false };
    if allow == "*" {
        // S credentials Include: wildcard NEni allowed (spec).
        return !matches!(credentials, CorsCredentials::Include);
    }
    allow == request_origin || allow == "null"
}

/// Check exposed headers - response headers viditelne JS jen kdyz v
/// `Access-Control-Expose-Headers`. Default exposed: Cache-Control,
/// Content-Language, Content-Length, Content-Type, Expires, Last-Modified, Pragma.
pub fn is_exposed_header(name: &str, expose_list: Option<&str>) -> bool {
    let default_exposed = matches!(name.to_lowercase().as_str(),
        "cache-control" | "content-language" | "content-length" |
        "content-type" | "expires" | "last-modified" | "pragma");
    if default_exposed { return true; }
    if let Some(list) = expose_list {
        for entry in list.split(',') {
            if entry.trim().eq_ignore_ascii_case(name) { return true; }
        }
    }
    false
}

/// Get origin z URL (scheme://host[:port]).
pub fn origin_of(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split('/').next().unwrap_or("");
        format!("http://{}", host)
    } else if let Some(rest) = url.strip_prefix("https://") {
        let host = rest.split('/').next().unwrap_or("");
        format!("https://{}", host)
    } else if url.starts_with("file://") || url.starts_with("about:") {
        "null".into() // opaque origin
    } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_get_no_preflight() {
        assert!(!needs_preflight("GET", &[]));
        assert!(!needs_preflight("POST", &[("Content-Type", "text/plain")]));
    }

    #[test]
    fn put_needs_preflight() {
        assert!(needs_preflight("PUT", &[]));
        assert!(needs_preflight("DELETE", &[]));
    }

    #[test]
    fn custom_header_needs_preflight() {
        assert!(needs_preflight("GET", &[("X-Custom", "value")]));
        assert!(needs_preflight("POST", &[("Content-Type", "application/json")]));
    }

    #[test]
    fn allow_origin_exact_match() {
        assert!(check_allow_origin(Some("https://example.com"),
            "https://example.com", CorsCredentials::Omit));
        assert!(!check_allow_origin(Some("https://other.com"),
            "https://example.com", CorsCredentials::Omit));
    }

    #[test]
    fn allow_origin_wildcard_no_credentials() {
        assert!(check_allow_origin(Some("*"),
            "https://example.com", CorsCredentials::Omit));
    }

    #[test]
    fn allow_origin_wildcard_blocked_with_credentials() {
        assert!(!check_allow_origin(Some("*"),
            "https://example.com", CorsCredentials::Include));
    }

    #[test]
    fn exposed_header_default() {
        assert!(is_exposed_header("Content-Type", None));
        assert!(is_exposed_header("cache-control", None));
        assert!(!is_exposed_header("X-Custom", None));
    }

    #[test]
    fn exposed_header_explicit() {
        assert!(is_exposed_header("X-Custom", Some("X-Custom, X-Other")));
    }

    #[test]
    fn origin_extraction() {
        assert_eq!(origin_of("https://example.com/path/to/file"), "https://example.com");
        assert_eq!(origin_of("http://localhost:8080/x"), "http://localhost:8080");
        assert_eq!(origin_of("file:///tmp/x.html"), "null");
    }
}
