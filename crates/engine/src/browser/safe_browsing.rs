//! Safe Browsing - URL reputation check + warning page.
//!
//! Real impl talks to Google Safe Browsing v4 API; here foundation only.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreatType {
    Malware,
    SocialEngineering,        // phishing
    UnwantedSoftware,
    PotentiallyHarmfulApplication,
}

#[derive(Debug, Clone)]
pub struct ThreatMatch {
    pub threat_type: ThreatType,
    pub url: String,
    pub matched_pattern: String,
}

#[derive(Default)]
pub struct SafeBrowsingDb {
    /// Hash prefixes for local lookup (real impl uses SHA-256 prefix lists).
    pub url_blacklist: HashSet<String>,
    pub host_blacklist: HashSet<String>,
}

impl SafeBrowsingDb {
    pub fn new() -> Self { Self::default() }

    pub fn add_url(&mut self, url: &str) {
        self.url_blacklist.insert(canonicalize(url));
    }

    pub fn add_host(&mut self, host: &str) {
        self.host_blacklist.insert(host.to_ascii_lowercase());
    }

    pub fn check(&self, url: &str) -> Option<ThreatMatch> {
        let canon = canonicalize(url);
        if self.url_blacklist.contains(&canon) {
            return Some(ThreatMatch {
                threat_type: ThreatType::Malware,
                url: url.into(),
                matched_pattern: canon,
            });
        }
        if let Some(host) = extract_host(url) {
            if self.host_blacklist.contains(&host) {
                return Some(ThreatMatch {
                    threat_type: ThreatType::SocialEngineering,
                    url: url.into(),
                    matched_pattern: host,
                });
            }
            // Also check parent domains.
            let mut parts: Vec<&str> = host.split('.').collect();
            while parts.len() > 2 {
                parts.remove(0);
                let parent = parts.join(".");
                if self.host_blacklist.contains(&parent) {
                    return Some(ThreatMatch {
                        threat_type: ThreatType::SocialEngineering,
                        url: url.into(),
                        matched_pattern: parent,
                    });
                }
            }
        }
        None
    }
}

fn canonicalize(url: &str) -> String {
    // Lower-case host part, strip fragment.
    let no_frag = url.split('#').next().unwrap_or(url);
    no_frag.to_ascii_lowercase()
}

fn extract_host(url: &str) -> Option<String> {
    let after = url.split("://").nth(1)?;
    let host_end = after.find(|c: char| c == '/' || c == '?' || c == '#').unwrap_or(after.len());
    let host = &after[..host_end];
    Some(host.split(':').next().unwrap_or(host).to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_exact_url() {
        let mut db = SafeBrowsingDb::new();
        db.add_url("https://evil.com/x");
        let m = db.check("https://EVIL.com/x");
        assert!(m.is_some());
    }

    #[test]
    fn blocks_host_match() {
        let mut db = SafeBrowsingDb::new();
        db.add_host("malware.com");
        let m = db.check("https://malware.com/page");
        assert_eq!(m.unwrap().threat_type, ThreatType::SocialEngineering);
    }

    #[test]
    fn blocks_subdomain_via_parent_host() {
        let mut db = SafeBrowsingDb::new();
        db.add_host("evil.com");
        let m = db.check("https://api.evil.com/x");
        assert!(m.is_some());
    }

    #[test]
    fn clean_url_passes() {
        let db = SafeBrowsingDb::new();
        assert!(db.check("https://x.com").is_none());
    }
}
