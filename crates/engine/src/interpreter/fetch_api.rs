//! fetch() / Request / Response - WHATWG Fetch.
//!
//! Spec: https://fetch.spec.whatwg.org/
//! High-level state model. Real impl plumbs through ureq/isahc; this captures
//! the JS-facing structure (headers, body kind, redirect/cache modes).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestMode {
    Cors,
    NoCors,
    SameOrigin,
    Navigate,
    Websocket,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestCredentials {
    Omit,
    SameOrigin,
    Include,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestCache {
    Default,
    NoStore,
    Reload,
    NoCache,
    ForceCache,
    OnlyIfCached,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestRedirect {
    Follow,
    Error,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestDestination {
    None,
    Document,
    Embed,
    Font,
    Image,
    Manifest,
    Object,
    Report,
    Script,
    ServiceWorker,
    SharedWorker,
    Style,
    Track,
    Video,
    Audio,
    Worker,
    Xslt,
    Fencedframe,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub mode: RequestMode,
    pub credentials: RequestCredentials,
    pub cache: RequestCache,
    pub redirect: RequestRedirect,
    pub destination: RequestDestination,
    pub referrer: String,
    pub referrer_policy: String,
    pub integrity: String,
    pub keepalive: bool,
    pub signal_id: Option<u64>,
    pub priority: RequestPriority,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RequestPriority {
    Auto,
    High,
    Low,
}

impl Request {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(), method: "GET".into(),
            headers: HashMap::new(), body: None,
            mode: RequestMode::Cors,
            credentials: RequestCredentials::SameOrigin,
            cache: RequestCache::Default,
            redirect: RequestRedirect::Follow,
            destination: RequestDestination::None,
            referrer: "about:client".into(),
            referrer_policy: String::new(),
            integrity: String::new(),
            keepalive: false,
            signal_id: None,
            priority: RequestPriority::Auto,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub url: String,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub kind: ResponseKind,
    pub redirected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResponseKind {
    Basic,
    Cors,
    Default,
    Error,
    Opaque,
    OpaqueRedirect,
}

impl Response {
    pub fn ok(&self) -> bool { (200..300).contains(&self.status) }

    pub fn header(&self, name: &str) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.headers.iter().find(|(k, _)| k.to_ascii_lowercase() == key).map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_defaults() {
        let r = Request::new("https://x.com");
        assert_eq!(r.method, "GET");
        assert_eq!(r.mode, RequestMode::Cors);
    }

    #[test]
    fn response_ok_in_2xx() {
        let r = Response {
            url: "x".into(), status: 200, status_text: "OK".into(),
            headers: HashMap::new(), body: vec![],
            kind: ResponseKind::Basic, redirected: false,
        };
        assert!(r.ok());
    }

    #[test]
    fn response_not_ok_404() {
        let r = Response {
            url: "x".into(), status: 404, status_text: "NF".into(),
            headers: HashMap::new(), body: vec![],
            kind: ResponseKind::Basic, redirected: false,
        };
        assert!(!r.ok());
    }

    #[test]
    fn response_header_case_insensitive() {
        let mut h = HashMap::new();
        h.insert("Content-Type".into(), "text/html".into());
        let r = Response {
            url: "x".into(), status: 200, status_text: "OK".into(),
            headers: h, body: vec![],
            kind: ResponseKind::Basic, redirected: false,
        };
        assert_eq!(r.header("content-type"), Some("text/html"));
    }

    #[test]
    fn request_priority_default_auto() {
        let r = Request::new("x");
        assert_eq!(r.priority, RequestPriority::Auto);
    }
}
