//! Reload strategy - F5 vs Ctrl+F5 vs Ctrl+Shift+R semantics.
//!
//! Chrome behavior:
//! - F5 / location.reload(): same-origin, cache may be hit
//! - Ctrl+F5 / location.reload(true): bypass HTTP cache (Cache-Control: no-cache)
//! - Ctrl+Shift+R: also bypass service worker

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReloadKind {
    Normal,                // F5 - cache-friendly
    BypassCache,           // Ctrl+F5
    BypassServiceWorker,   // Ctrl+Shift+R
}

#[derive(Debug, Clone)]
pub struct ReloadRequest {
    pub kind: ReloadKind,
    pub url: String,
    pub origin: String,
    pub keep_scroll: bool,
}

impl ReloadKind {
    /// Per-spec cache mode for the navigation request.
    pub fn cache_mode(&self) -> &'static str {
        match self {
            Self::Normal => "default",
            Self::BypassCache => "no-cache",
            Self::BypassServiceWorker => "reload",
        }
    }

    pub fn bypass_service_worker(&self) -> bool {
        matches!(self, Self::BypassServiceWorker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_mode_per_kind() {
        assert_eq!(ReloadKind::Normal.cache_mode(), "default");
        assert_eq!(ReloadKind::BypassCache.cache_mode(), "no-cache");
        assert_eq!(ReloadKind::BypassServiceWorker.cache_mode(), "reload");
    }

    #[test]
    fn bypass_sw_only_on_shift() {
        assert!(!ReloadKind::Normal.bypass_service_worker());
        assert!(!ReloadKind::BypassCache.bypass_service_worker());
        assert!(ReloadKind::BypassServiceWorker.bypass_service_worker());
    }
}
