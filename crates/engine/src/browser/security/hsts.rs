//! HTTP Strict Transport Security (HSTS) per RFC 6797.
//!
//! Server posila `Strict-Transport-Security: max-age=N; includeSubDomains`.
//! Browser pak vse pro tu domenu upgraduje http -> https po dobu max-age.
//!
//! Persistent storage (preload list + dynamic entries). Inspired by Chromium
//! `net/http/transport_security_state.cc`.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct HstsEntry {
    pub expires_unix: u64,
    pub include_subdomains: bool,
}

#[derive(Default)]
pub struct HstsStore {
    pub entries: HashMap<String, HstsEntry>,
}

impl HstsStore {
    pub fn new() -> Self { Self::default() }

    /// Parse Strict-Transport-Security header + register entry.
    pub fn add_from_header(&mut self, host: &str, header: &str) {
        let mut max_age: u64 = 0;
        let mut include_subdomains = false;
        for part in header.split(';') {
            let p = part.trim().to_lowercase();
            if let Some(rest) = p.strip_prefix("max-age=") {
                let rest = rest.trim_matches('"');
                max_age = rest.parse().unwrap_or(0);
            } else if p == "includesubdomains" {
                include_subdomains = true;
            }
        }
        if max_age == 0 {
            // max-age=0 = remove entry (spec).
            self.entries.remove(host);
            return;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.entries.insert(host.to_lowercase(), HstsEntry {
            expires_unix: now + max_age,
            include_subdomains,
        });
    }

    /// Check zdali URL musi byt upgraded na https.
    pub fn should_upgrade(&self, url: &str) -> bool {
        if !url.starts_with("http://") { return false; }
        let host = match url_host(url) { Some(h) => h, None => return false };
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        // Direct match.
        if let Some(entry) = self.entries.get(&host) {
            if entry.expires_unix > now { return true; }
        }
        // Subdomain match (includeSubDomains).
        let mut parts: Vec<&str> = host.split('.').collect();
        while parts.len() > 1 {
            parts.remove(0);
            let parent = parts.join(".");
            if let Some(entry) = self.entries.get(&parent) {
                if entry.include_subdomains && entry.expires_unix > now {
                    return true;
                }
            }
        }
        false
    }

    /// Upgrade URL z http:// na https://.
    pub fn upgrade(url: &str) -> String {
        if let Some(rest) = url.strip_prefix("http://") {
            format!("https://{}", rest)
        } else { url.to_string() }
    }
}

fn url_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://"))?;
    Some(rest.split('/').next()?.split(':').next()?.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_upgrade() {
        let mut s = HstsStore::new();
        s.add_from_header("example.com", "max-age=31536000");
        assert!(s.should_upgrade("http://example.com/path"));
        assert_eq!(HstsStore::upgrade("http://example.com/path"), "https://example.com/path");
    }

    #[test]
    fn no_upgrade_when_not_registered() {
        let s = HstsStore::new();
        assert!(!s.should_upgrade("http://example.com"));
    }

    #[test]
    fn subdomain_match() {
        let mut s = HstsStore::new();
        s.add_from_header("example.com", "max-age=31536000; includeSubDomains");
        assert!(s.should_upgrade("http://api.example.com/x"));
        assert!(s.should_upgrade("http://deep.sub.example.com/x"));
    }

    #[test]
    fn subdomain_no_match_without_flag() {
        let mut s = HstsStore::new();
        s.add_from_header("example.com", "max-age=31536000");
        // includeSubDomains chybi = jen exact match.
        assert!(!s.should_upgrade("http://api.example.com/x"));
    }

    #[test]
    fn max_age_zero_removes() {
        let mut s = HstsStore::new();
        s.add_from_header("example.com", "max-age=31536000");
        assert!(s.should_upgrade("http://example.com"));
        s.add_from_header("example.com", "max-age=0");
        assert!(!s.should_upgrade("http://example.com"));
    }
}
