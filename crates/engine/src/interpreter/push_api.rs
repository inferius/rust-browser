//! Push API stub - `PushManager.subscribe` + service worker `push` event.
//!
//! Spec: https://w3c.github.io/push-api/
//!
//! Foundation: subscribe registry + permission state. Real impl by vyzadoval
//! VAPID keys, push server protocol (Web Push Protocol RFC 8030), system
//! notification daemon integration.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PushPermissionState {
    Default,    // not yet asked
    Granted,
    Denied,
}

#[derive(Debug, Clone)]
pub struct PushSubscription {
    pub endpoint: String,
    pub p256dh_key: String,    // base64
    pub auth_key: String,      // base64
    pub origin: String,
}

#[derive(Default)]
pub struct PushRegistry {
    pub subscriptions: HashMap<String, PushSubscription>, // origin -> sub
    pub permissions: HashMap<String, PushPermissionState>,
}

impl PushRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn permission_state(&self, origin: &str) -> PushPermissionState {
        self.permissions.get(origin).copied().unwrap_or(PushPermissionState::Default)
    }

    pub fn grant(&mut self, origin: &str) {
        self.permissions.insert(origin.into(), PushPermissionState::Granted);
    }

    pub fn deny(&mut self, origin: &str) {
        self.permissions.insert(origin.into(), PushPermissionState::Denied);
    }

    pub fn subscribe(&mut self, origin: &str) -> Option<PushSubscription> {
        if self.permission_state(origin) != PushPermissionState::Granted { return None; }
        let sub = PushSubscription {
            endpoint: format!("https://push.example.com/sub/{}", uid()),
            p256dh_key: "stub_p256dh".into(),
            auth_key: "stub_auth".into(),
            origin: origin.into(),
        };
        self.subscriptions.insert(origin.into(), sub.clone());
        Some(sub)
    }

    pub fn unsubscribe(&mut self, origin: &str) -> bool {
        self.subscriptions.remove(origin).is_some()
    }
}

fn uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_permission() {
        let r = PushRegistry::new();
        assert_eq!(r.permission_state("https://x.com"), PushPermissionState::Default);
    }

    #[test]
    fn subscribe_requires_grant() {
        let mut r = PushRegistry::new();
        assert!(r.subscribe("https://x.com").is_none());
        r.grant("https://x.com");
        assert!(r.subscribe("https://x.com").is_some());
    }

    #[test]
    fn unsubscribe_removes() {
        let mut r = PushRegistry::new();
        r.grant("https://x.com");
        r.subscribe("https://x.com");
        assert!(r.unsubscribe("https://x.com"));
        assert!(!r.unsubscribe("https://x.com")); // already removed
    }
}
