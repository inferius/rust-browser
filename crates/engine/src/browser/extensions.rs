//! WebExtensions / browser extensions API foundation.
//!
//! Spec: https://wicg.github.io/webextensions/ + Chrome extension manifests.
//! Loads extension manifest, registers content scripts, registers background scripts.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ExtensionManifest {
    pub manifest_version: u32,        // 2 or 3
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub host_permissions: Vec<String>,
    pub content_scripts: Vec<ContentScript>,
    pub background: Option<BackgroundScript>,
    pub action: Option<BrowserAction>,
    pub commands: HashMap<String, ExtensionCommand>,
}

#[derive(Debug, Clone, Default)]
pub struct ContentScript {
    pub matches: Vec<String>,
    pub exclude_matches: Vec<String>,
    pub js_files: Vec<String>,
    pub css_files: Vec<String>,
    pub run_at: RunAt,
    pub world: ScriptWorld,
    pub all_frames: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RunAt { DocumentStart, DocumentEnd, DocumentIdle }
impl Default for RunAt { fn default() -> Self { RunAt::DocumentIdle } }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScriptWorld { Isolated, Main }
impl Default for ScriptWorld { fn default() -> Self { ScriptWorld::Isolated } }

#[derive(Debug, Clone)]
pub enum BackgroundScript {
    ServiceWorker { script: String, script_type: String },
    EventPage { scripts: Vec<String>, persistent: bool }, // v2
}

#[derive(Debug, Clone, Default)]
pub struct BrowserAction {
    pub default_title: String,
    pub default_icon: HashMap<String, String>, // "16" -> path
    pub default_popup: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtensionCommand {
    pub suggested_key: HashMap<String, String>,     // "default" -> "Ctrl+Shift+X"
    pub description: String,
}

#[derive(Default)]
pub struct ExtensionRegistry {
    pub extensions: HashMap<String, ExtensionManifest>,
}

impl ExtensionRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, ext_id: &str, m: ExtensionManifest) {
        self.extensions.insert(ext_id.into(), m);
    }

    /// Find content scripts whose match pattern applies to a URL.
    pub fn content_scripts_for(&self, url: &str) -> Vec<(&str, &ContentScript)> {
        let mut out = Vec::new();
        for (id, m) in &self.extensions {
            for cs in &m.content_scripts {
                if cs.matches.iter().any(|p| pattern_matches(p, url))
                   && !cs.exclude_matches.iter().any(|p| pattern_matches(p, url)) {
                    out.push((id.as_str(), cs));
                }
            }
        }
        out
    }

    pub fn has_permission(&self, ext_id: &str, perm: &str) -> bool {
        self.extensions.get(ext_id).map(|m| m.permissions.iter().any(|p| p == perm)).unwrap_or(false)
    }
}

/// Match a Chrome extension match pattern like "https://*.example.com/*".
pub fn pattern_matches(pattern: &str, url: &str) -> bool {
    if pattern == "<all_urls>" {
        return url.starts_with("http") || url.starts_with("file://") || url.starts_with("ftp:");
    }
    // <scheme>://<host>/<path>
    let Some((p_scheme, rest)) = pattern.split_once("://") else { return false; };
    let Some((u_scheme, u_rest)) = url.split_once("://") else { return false; };
    if p_scheme != "*" && p_scheme != u_scheme { return false; }
    let Some((p_host, p_path)) = rest.split_once('/') else { return false; };
    let Some((u_host, u_path)) = u_rest.split_once('/') else { return false; };
    if !host_matches(p_host, u_host) { return false; }
    path_glob(&format!("/{}", p_path), &format!("/{}", u_path))
}

fn host_matches(pattern: &str, host: &str) -> bool {
    if pattern == "*" { return true; }
    if let Some(rest) = pattern.strip_prefix("*.") {
        return host == rest || host.ends_with(&format!(".{}", rest));
    }
    pattern == host
}

fn path_glob(pattern: &str, path: &str) -> bool {
    if !pattern.contains('*') { return pattern == path; }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) { return false; }
            idx = part.len();
        } else if i == parts.len() - 1 {
            return path[idx..].ends_with(part);
        } else {
            if let Some(pos) = path[idx..].find(part) {
                idx += pos + part.len();
            } else {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_glob_matches_subdomain() {
        assert!(host_matches("*.example.com", "www.example.com"));
        assert!(host_matches("*.example.com", "example.com"));
        assert!(!host_matches("*.example.com", "other.com"));
    }

    #[test]
    fn pattern_all_urls() {
        assert!(pattern_matches("<all_urls>", "https://x.com/y"));
    }

    #[test]
    fn pattern_full_match() {
        assert!(pattern_matches("https://*.example.com/*", "https://www.example.com/page"));
    }

    #[test]
    fn pattern_scheme_mismatch() {
        assert!(!pattern_matches("https://x.com/*", "http://x.com/"));
    }

    #[test]
    fn register_and_lookup_scripts() {
        let mut r = ExtensionRegistry::new();
        let mut m = ExtensionManifest::default();
        m.manifest_version = 3;
        m.content_scripts.push(ContentScript {
            matches: vec!["https://*.x.com/*".into()],
            ..Default::default()
        });
        r.register("my-ext", m);
        let scripts = r.content_scripts_for("https://www.x.com/page");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].0, "my-ext");
    }

    #[test]
    fn permission_check() {
        let mut r = ExtensionRegistry::new();
        let mut m = ExtensionManifest::default();
        m.permissions = vec!["storage".into(), "tabs".into()];
        r.register("ext1", m);
        assert!(r.has_permission("ext1", "tabs"));
        assert!(!r.has_permission("ext1", "history"));
    }
}
