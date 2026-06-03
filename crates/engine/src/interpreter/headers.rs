//! WHATWG Headers - case-insensitive ordered name/value pairs + forbidden name guards.
//!
//! Spec: https://fetch.spec.whatwg.org/#headers-class

#[derive(Debug, Clone, Default)]
pub struct Headers {
    /// Ordered, lower-cased name + original-cased value.
    pub entries: Vec<(String, String)>,
    pub guard: HeadersGuard,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadersGuard {
    None,
    Immutable,
    Request,
    RequestNoCors,
    Response,
}

impl Default for HeadersGuard {
    fn default() -> Self { HeadersGuard::None }
}

pub const FORBIDDEN_REQUEST_NAMES: &[&str] = &[
    "accept-charset", "accept-encoding", "access-control-request-headers",
    "access-control-request-method", "connection", "content-length",
    "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
    "origin", "referer", "te", "trailer", "transfer-encoding",
    "upgrade", "via",
];

pub const FORBIDDEN_RESPONSE_NAMES: &[&str] = &["set-cookie", "set-cookie2"];

impl Headers {
    pub fn new() -> Self { Self::default() }

    /// Append (does NOT dedupe — Headers preserves multiple values).
    pub fn append(&mut self, name: &str, value: &str) -> Result<(), String> {
        let lname = name.to_ascii_lowercase();
        if !is_token(name) { return Err(format!("invalid header name '{}'", name)); }
        if !is_value(value) { return Err(format!("invalid header value")); }
        if self.is_forbidden(&lname) { return Ok(()); /* silently ignored per spec */ }
        self.entries.push((lname, value.trim().to_string()));
        Ok(())
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let lname = name.to_ascii_lowercase();
        if !is_token(name) { return Err(format!("invalid header name '{}'", name)); }
        if !is_value(value) { return Err(format!("invalid header value")); }
        if self.is_forbidden(&lname) { return Ok(()); }
        self.entries.retain(|(n, _)| n != &lname);
        self.entries.push((lname, value.trim().to_string()));
        Ok(())
    }

    pub fn delete(&mut self, name: &str) {
        let lname = name.to_ascii_lowercase();
        if self.is_forbidden(&lname) { return; }
        self.entries.retain(|(n, _)| n != &lname);
    }

    /// Comma-joined value list per spec (Set-Cookie excepted).
    pub fn get(&self, name: &str) -> Option<String> {
        let lname = name.to_ascii_lowercase();
        let values: Vec<&str> = self.entries.iter().filter(|(n, _)| n == &lname).map(|(_, v)| v.as_str()).collect();
        if values.is_empty() { None } else { Some(values.join(", ")) }
    }

    pub fn get_set_cookie(&self) -> Vec<String> {
        self.entries.iter().filter(|(n, _)| n == "set-cookie").map(|(_, v)| v.clone()).collect()
    }

    pub fn has(&self, name: &str) -> bool {
        let lname = name.to_ascii_lowercase();
        self.entries.iter().any(|(n, _)| n == &lname)
    }

    fn is_forbidden(&self, lname: &str) -> bool {
        match self.guard {
            HeadersGuard::Immutable => true,
            HeadersGuard::Request => FORBIDDEN_REQUEST_NAMES.contains(&lname),
            HeadersGuard::Response => FORBIDDEN_RESPONSE_NAMES.contains(&lname),
            _ => false,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.entries.iter()
    }
}

fn is_token(s: &str) -> bool {
    if s.is_empty() { return false; }
    s.bytes().all(|b| matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.'
        | b'^' | b'_' | b'`' | b'|' | b'~'
        | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z'))
}

fn is_value(s: &str) -> bool {
    !s.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_get() {
        let mut h = Headers::new();
        h.append("Content-Type", "text/html").unwrap();
        assert_eq!(h.get("content-type").as_deref(), Some("text/html"));
    }

    #[test]
    fn append_joins_on_get() {
        let mut h = Headers::new();
        h.append("X", "a").unwrap();
        h.append("X", "b").unwrap();
        assert_eq!(h.get("x").as_deref(), Some("a, b"));
    }

    #[test]
    fn set_replaces() {
        let mut h = Headers::new();
        h.append("X", "a").unwrap();
        h.set("X", "z").unwrap();
        assert_eq!(h.get("x").as_deref(), Some("z"));
    }

    #[test]
    fn delete_removes_all() {
        let mut h = Headers::new();
        h.append("X", "a").unwrap();
        h.append("X", "b").unwrap();
        h.delete("X");
        assert!(!h.has("x"));
    }

    #[test]
    fn request_guard_forbids_cookie() {
        let mut h = Headers { guard: HeadersGuard::Request, ..Default::default() };
        h.set("Cookie", "x=1").unwrap();
        assert!(!h.has("cookie"));
    }

    #[test]
    fn invalid_name_errors() {
        let mut h = Headers::new();
        assert!(h.append("Bad Name", "v").is_err());
    }

    #[test]
    fn set_cookie_separate() {
        let mut h = Headers::new();
        h.append("Set-Cookie", "a=1").unwrap();
        h.append("Set-Cookie", "b=2").unwrap();
        let list = h.get_set_cookie();
        assert_eq!(list, vec!["a=1", "b=2"]);
    }
}
