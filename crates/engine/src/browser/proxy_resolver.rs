//! Proxy resolver - PAC scripts + system proxy + per-scheme overrides.
//!
//! Foundation only - real impl evaluates JS PAC scripts via the interpreter.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProxyMode {
    Direct,
    Auto,                  // OS auto-detect (WPAD)
    Pac,                   // PAC URL
    Manual,                // per-scheme overrides
    System,                // ICS / GNOME / macOS system
}

#[derive(Debug, Clone)]
pub struct ProxyServer {
    pub scheme: ProxyScheme,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProxyScheme {
    Http,
    Https,
    Socks4,
    Socks5,
    Quic,
    Direct,
}

#[derive(Debug, Clone, Default)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    pub pac_url: Option<String>,
    pub http_proxy: Option<ProxyServer>,
    pub https_proxy: Option<ProxyServer>,
    pub ftp_proxy: Option<ProxyServer>,
    pub socks_proxy: Option<ProxyServer>,
    pub bypass_rules: Vec<String>,
}

impl Default for ProxyMode {
    fn default() -> Self { ProxyMode::Direct }
}

impl ProxyConfig {
    pub fn resolve(&self, url: &str) -> Vec<ProxyServer> {
        if self.should_bypass(url) || self.mode == ProxyMode::Direct {
            return vec![ProxyServer { scheme: ProxyScheme::Direct, host: String::new(), port: 0 }];
        }
        match self.mode {
            ProxyMode::Manual => {
                let scheme = url.split(':').next().unwrap_or("");
                let candidate = match scheme {
                    "http" => self.http_proxy.clone(),
                    "https" => self.https_proxy.clone(),
                    "ftp" => self.ftp_proxy.clone(),
                    _ => self.socks_proxy.clone(),
                };
                if let Some(p) = candidate { vec![p] }
                else { vec![ProxyServer { scheme: ProxyScheme::Direct, host: String::new(), port: 0 }] }
            }
            _ => vec![ProxyServer { scheme: ProxyScheme::Direct, host: String::new(), port: 0 }],
        }
    }

    pub fn should_bypass(&self, url: &str) -> bool {
        for rule in &self.bypass_rules {
            if rule_matches(rule, url) { return true; }
        }
        false
    }
}

fn rule_matches(rule: &str, url: &str) -> bool {
    let rule = rule.trim().to_ascii_lowercase();
    if rule == "<local>" {
        return url.contains("//localhost") || url.contains("//127.")
               || url.contains("//[::1]") || url.contains("//[0:0:0:0:0:0:0:1]");
    }
    if rule == "<-loopback>" { return false; }
    let lower = url.to_ascii_lowercase();
    if let Some(host) = lower.split("://").nth(1) {
        let host = host.split('/').next().unwrap_or("").split(':').next().unwrap_or("");
        if rule.starts_with('.') {
            return host.ends_with(&rule) || host.ends_with(&rule[1..]);
        }
        if rule.contains('*') {
            return wildcard_match(&rule, host);
        }
        return host == rule || host.ends_with(&format!(".{}", rule));
    }
    false
}

fn wildcard_match(pat: &str, host: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    let mut idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !host.starts_with(part) { return false; }
            idx = part.len();
        } else if i == parts.len() - 1 {
            return host[idx..].ends_with(part);
        } else {
            if let Some(pos) = host[idx..].find(part) {
                idx += pos + part.len();
            } else { return false; }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_when_mode_direct() {
        let c = ProxyConfig::default();
        let r = c.resolve("https://x.com");
        assert_eq!(r[0].scheme, ProxyScheme::Direct);
    }

    #[test]
    fn manual_uses_http_proxy() {
        let mut c = ProxyConfig::default();
        c.mode = ProxyMode::Manual;
        c.http_proxy = Some(ProxyServer { scheme: ProxyScheme::Http, host: "proxy".into(), port: 3128 });
        let r = c.resolve("http://x.com");
        assert_eq!(r[0].host, "proxy");
    }

    #[test]
    fn bypass_localhost() {
        let mut c = ProxyConfig::default();
        c.mode = ProxyMode::Manual;
        c.bypass_rules.push("<local>".into());
        assert!(c.should_bypass("http://localhost:3000"));
        assert!(!c.should_bypass("https://x.com"));
    }

    #[test]
    fn bypass_dot_rule() {
        let mut c = ProxyConfig::default();
        c.bypass_rules.push(".example.com".into());
        assert!(c.should_bypass("https://api.example.com/x"));
        assert!(c.should_bypass("https://example.com"));
    }

    #[test]
    fn bypass_wildcard() {
        let mut c = ProxyConfig::default();
        c.bypass_rules.push("*.internal".into());
        assert!(c.should_bypass("https://service.internal/x"));
    }
}
