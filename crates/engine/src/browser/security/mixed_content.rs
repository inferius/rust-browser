//! Mixed Content blocking - HTTPS pages must not load HTTP subresources.
//!
//! Spec: https://www.w3.org/TR/mixed-content/
//! - Optionally-blockable: img, audio, video, prefetch -> upgrade or warn
//! - Blockable: script, link[stylesheet], iframe, XHR, fetch, WebSocket, EventSource, font, SVG <use>
//! - Same-origin or local (file/data/blob) is always allowed.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixedContentClass {
    Blockable,
    OptionallyBlockable,
    Upgradeable,         // ws: -> wss: after Upgrade Insecure Requests
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixedContentDecision {
    Allow,
    Block,
    UpgradeToHttps,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResourceKind {
    Script,
    Stylesheet,
    Image,
    Audio,
    Video,
    Iframe,
    Xhr,
    Fetch,
    WebSocket,
    EventSource,
    Font,
    SvgUse,
    Other,
}

pub fn classify(kind: ResourceKind) -> MixedContentClass {
    match kind {
        ResourceKind::Image | ResourceKind::Audio | ResourceKind::Video => MixedContentClass::OptionallyBlockable,
        ResourceKind::WebSocket => MixedContentClass::Upgradeable,
        _ => MixedContentClass::Blockable,
    }
}

/// Returns decision per spec.
pub fn check(top_level_url: &str, resource_url: &str, kind: ResourceKind, upgrade_insecure: bool) -> MixedContentDecision {
    if !is_secure_context(top_level_url) {
        // Non-HTTPS page - mixed content rules do not apply.
        return MixedContentDecision::Allow;
    }
    if is_potentially_trustworthy(resource_url) {
        return MixedContentDecision::Allow;
    }
    let class = classify(kind);
    if upgrade_insecure {
        return MixedContentDecision::UpgradeToHttps;
    }
    match class {
        MixedContentClass::Blockable => MixedContentDecision::Block,
        MixedContentClass::OptionallyBlockable => MixedContentDecision::Allow,
        MixedContentClass::Upgradeable => MixedContentDecision::UpgradeToHttps,
    }
}

pub fn is_secure_context(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("wss://") ||
    url.starts_with("file://") || url.starts_with("data:") ||
    url.starts_with("blob:") || url.starts_with("about:") ||
    url.starts_with("chrome:") || url.starts_with("chrome-extension:")
}

/// Per https://www.w3.org/TR/secure-contexts/#potentially-trustworthy-url.
pub fn is_potentially_trustworthy(url: &str) -> bool {
    if is_secure_context(url) { return true; }
    if url.starts_with("http://localhost") || url.starts_with("http://127.")
        || url.starts_with("http://[::1]") || url.starts_with("http://[0:0:0:0:0:0:0:1]") {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockable_script_blocked() {
        let d = check("https://x.com", "http://evil.com/a.js", ResourceKind::Script, false);
        assert_eq!(d, MixedContentDecision::Block);
    }

    #[test]
    fn image_optionally_allowed() {
        let d = check("https://x.com", "http://cdn.com/i.png", ResourceKind::Image, false);
        assert_eq!(d, MixedContentDecision::Allow);
    }

    #[test]
    fn websocket_upgraded() {
        let d = check("https://x.com", "ws://x.com/sock", ResourceKind::WebSocket, false);
        assert_eq!(d, MixedContentDecision::UpgradeToHttps);
    }

    #[test]
    fn upgrade_insecure_requests_upgrades_all() {
        let d = check("https://x.com", "http://cdn.com/a.js", ResourceKind::Script, true);
        assert_eq!(d, MixedContentDecision::UpgradeToHttps);
    }

    #[test]
    fn http_page_unaffected() {
        let d = check("http://x.com", "http://cdn.com/a.js", ResourceKind::Script, false);
        assert_eq!(d, MixedContentDecision::Allow);
    }

    #[test]
    fn localhost_trustworthy() {
        assert!(is_potentially_trustworthy("http://localhost:3000"));
        assert!(is_potentially_trustworthy("http://127.0.0.1:8080"));
    }

    #[test]
    fn https_trustworthy() {
        assert!(is_potentially_trustworthy("https://x.com"));
    }
}
