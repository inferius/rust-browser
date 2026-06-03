//! Favicon discovery + cache.
//!
//! Spec: HTML5 link[rel=icon] + fallback /favicon.ico.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FaviconLink {
    pub rel: String,                // "icon", "shortcut icon", "apple-touch-icon", "mask-icon"
    pub href: String,
    pub size: Option<(u32, u32)>,   // sizes="32x32" attribute
    pub mime: Option<String>,
    pub purpose: Option<String>,    // manifest "any" | "maskable" | "monochrome"
}

impl FaviconLink {
    pub fn parse_size(value: &str) -> Option<(u32, u32)> {
        let lower = value.to_ascii_lowercase();
        if lower == "any" { return Some((0, 0)); }
        let mut parts = lower.split('x');
        let w: u32 = parts.next()?.parse().ok()?;
        let h: u32 = parts.next()?.parse().ok()?;
        Some((w, h))
    }
}

#[derive(Default)]
pub struct FaviconRegistry {
    pub links_by_doc: HashMap<u64, Vec<FaviconLink>>,
}

impl FaviconRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, doc_id: u64, link: FaviconLink) {
        self.links_by_doc.entry(doc_id).or_default().push(link);
    }

    /// Pick the best match for a requested rendering size.
    pub fn pick_for(&self, doc_id: u64, target: u32) -> Option<&FaviconLink> {
        let links = self.links_by_doc.get(&doc_id)?;
        let mut best: Option<&FaviconLink> = None;
        let mut best_diff = u32::MAX;
        for l in links {
            let candidate_size = l.size.map(|(w, h)| w.max(h)).unwrap_or(16);
            let diff = candidate_size.abs_diff(target);
            if diff < best_diff {
                best_diff = diff;
                best = Some(l);
            }
        }
        best
    }

    /// Returns the implicit /favicon.ico fallback URL given base page URL.
    pub fn fallback_url(page_url: &str) -> String {
        // Strip path + query, keep scheme + host.
        if let Some(rest) = page_url.split("://").nth(1) {
            let host = rest.split('/').next().unwrap_or(rest);
            let scheme = &page_url[..page_url.find("://").unwrap_or(0)];
            return format!("{}://{}/favicon.ico", scheme, host);
        }
        "/favicon.ico".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_wxh() {
        assert_eq!(FaviconLink::parse_size("32x32"), Some((32, 32)));
        assert_eq!(FaviconLink::parse_size("16X16"), Some((16, 16)));
        assert_eq!(FaviconLink::parse_size("any"), Some((0, 0)));
    }

    #[test]
    fn register_and_pick_best() {
        let mut r = FaviconRegistry::new();
        r.register(1, FaviconLink {
            rel: "icon".into(), href: "/16.png".into(),
            size: Some((16, 16)), mime: None, purpose: None,
        });
        r.register(1, FaviconLink {
            rel: "icon".into(), href: "/32.png".into(),
            size: Some((32, 32)), mime: None, purpose: None,
        });
        r.register(1, FaviconLink {
            rel: "icon".into(), href: "/64.png".into(),
            size: Some((64, 64)), mime: None, purpose: None,
        });
        // Want 30px -> pick closest = 32.
        let best = r.pick_for(1, 30).unwrap();
        assert_eq!(best.href, "/32.png");
    }

    #[test]
    fn fallback_url_strip_path() {
        let url = FaviconRegistry::fallback_url("https://x.com/some/page?q=1");
        assert_eq!(url, "https://x.com/favicon.ico");
    }
}
