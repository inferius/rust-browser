//! Badging API - navigator.setAppBadge(count) / clearAppBadge().
//!
//! Spec: https://w3c.github.io/badging/
//! Shows small badge nad app icon (taskbar/dock). Per origin.

use std::collections::HashMap;

#[derive(Default)]
pub struct BadgingService {
    /// Per-origin badge count.
    pub badges: HashMap<String, BadgeValue>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BadgeValue {
    Number(u64),
    Flag,           // showAppBadge() bez argumentu = unread indicator
    Cleared,
}

impl BadgingService {
    pub fn new() -> Self { Self::default() }

    pub fn set(&mut self, origin: &str, count: Option<u64>) -> bool {
        let value = match count {
            Some(n) if n > 0 => BadgeValue::Number(n),
            Some(_) => BadgeValue::Cleared,
            None => BadgeValue::Flag,
        };
        self.badges.insert(origin.into(), value);
        // Real: native API - Windows ITaskbarList3::SetOverlayIcon /
        //       macOS NSApp.dockTile.badgeLabel / Linux libunity.
        true
    }

    pub fn clear(&mut self, origin: &str) {
        self.badges.insert(origin.into(), BadgeValue::Cleared);
    }

    pub fn get(&self, origin: &str) -> BadgeValue {
        self.badges.get(origin).copied().unwrap_or(BadgeValue::Cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_count() {
        let mut b = BadgingService::new();
        b.set("https://x.com", Some(5));
        assert_eq!(b.get("https://x.com"), BadgeValue::Number(5));
    }

    #[test]
    fn set_flag_without_value() {
        let mut b = BadgingService::new();
        b.set("https://x.com", None);
        assert_eq!(b.get("https://x.com"), BadgeValue::Flag);
    }

    #[test]
    fn zero_clears() {
        let mut b = BadgingService::new();
        b.set("https://x.com", Some(3));
        b.set("https://x.com", Some(0));
        assert_eq!(b.get("https://x.com"), BadgeValue::Cleared);
    }

    #[test]
    fn explicit_clear() {
        let mut b = BadgingService::new();
        b.set("https://x.com", Some(10));
        b.clear("https://x.com");
        assert_eq!(b.get("https://x.com"), BadgeValue::Cleared);
    }
}
