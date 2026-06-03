//! Cookie Store API - async cookies access (replaces document.cookie sync).
//!
//! Spec: https://wicg.github.io/cookie-store/

use std::collections::HashMap;
use crate::browser::security::cookies::Cookie;

#[derive(Default)]
pub struct CookieStore {
    /// Per-origin/path cookies.
    pub cookies: Vec<Cookie>,
}

#[derive(Debug, Clone, Default)]
pub struct CookieListItem {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<u64>,
    pub secure: bool,
    pub same_site: String,
    pub partitioned: bool,
}

impl CookieStore {
    pub fn new() -> Self { Self::default() }

    /// `cookieStore.get(name)`.
    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.iter().find(|c| c.name == name)
    }

    /// `cookieStore.getAll(filter)` - vsechny match.
    pub fn get_all(&self, name: Option<&str>) -> Vec<&Cookie> {
        self.cookies.iter().filter(|c| {
            name.map(|n| c.name == n).unwrap_or(true)
        }).collect()
    }

    /// `cookieStore.set(...)`.
    pub fn set(&mut self, cookie: Cookie) {
        self.cookies.retain(|c| !(c.name == cookie.name && c.path == cookie.path));
        self.cookies.push(cookie);
    }

    /// `cookieStore.delete(name)`.
    pub fn delete(&mut self, name: &str) -> bool {
        let before = self.cookies.len();
        self.cookies.retain(|c| c.name != name);
        self.cookies.len() < before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::security::cookies::SameSite;

    fn c(name: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: "x.com".into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
            expires_unix: None,
            max_age_seconds: None,
        }
    }

    #[test]
    fn set_and_get() {
        let mut s = CookieStore::new();
        s.set(c("k"));
        assert!(s.get("k").is_some());
    }

    #[test]
    fn set_replaces_same_name_path() {
        let mut s = CookieStore::new();
        s.set(c("k"));
        s.set(c("k"));
        assert_eq!(s.cookies.len(), 1);
    }

    #[test]
    fn get_all_filtered() {
        let mut s = CookieStore::new();
        s.set(c("a"));
        s.set(c("b"));
        assert_eq!(s.get_all(None).len(), 2);
        assert_eq!(s.get_all(Some("a")).len(), 1);
    }

    #[test]
    fn delete_removes() {
        let mut s = CookieStore::new();
        s.set(c("k"));
        assert!(s.delete("k"));
        assert!(!s.delete("k"));
    }
}
