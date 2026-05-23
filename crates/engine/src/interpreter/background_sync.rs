//! Background Sync API + Periodic Background Sync.
//!
//! Specs:
//! - https://wicg.github.io/background-sync/spec/
//! - https://wicg.github.io/periodic-background-sync/
//!
//! Tag-based one-shot sync (retry pri network restoration) +
//! periodic recurring sync (registered v service worker).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SyncRegistration {
    pub tag: String,
    pub last_chance_attempted: bool,
}

#[derive(Debug, Clone)]
pub struct PeriodicSyncRegistration {
    pub tag: String,
    pub min_interval_ms: u64,
    pub last_fired_unix_ms: u64,
}

#[derive(Default)]
pub struct BackgroundSyncRegistry {
    /// One-shot pending syncs per scope (= SW registration).
    pub one_shot: HashMap<String, Vec<SyncRegistration>>,
    pub periodic: HashMap<String, Vec<PeriodicSyncRegistration>>,
}

impl BackgroundSyncRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register_one_shot(&mut self, scope: &str, tag: &str) {
        let regs = self.one_shot.entry(scope.into()).or_default();
        if !regs.iter().any(|r| r.tag == tag) {
            regs.push(SyncRegistration { tag: tag.into(), last_chance_attempted: false });
        }
    }

    pub fn register_periodic(&mut self, scope: &str, tag: &str, min_interval_ms: u64) {
        let regs = self.periodic.entry(scope.into()).or_default();
        if !regs.iter().any(|r| r.tag == tag) {
            regs.push(PeriodicSyncRegistration {
                tag: tag.into(),
                min_interval_ms,
                last_fired_unix_ms: 0,
            });
        }
    }

    pub fn unregister(&mut self, scope: &str, tag: &str) -> bool {
        let mut removed = false;
        if let Some(regs) = self.one_shot.get_mut(scope) {
            let before = regs.len();
            regs.retain(|r| r.tag != tag);
            if regs.len() < before { removed = true; }
        }
        if let Some(regs) = self.periodic.get_mut(scope) {
            let before = regs.len();
            regs.retain(|r| r.tag != tag);
            if regs.len() < before { removed = true; }
        }
        removed
    }

    /// Drain ready syncs - online + one-shot pending nebo periodic interval elapsed.
    pub fn drain_ready(&mut self, scope: &str, now_unix_ms: u64, online: bool) -> Vec<String> {
        let mut ready = Vec::new();
        if !online { return ready; }
        if let Some(regs) = self.one_shot.get_mut(scope) {
            for r in regs.drain(..) { ready.push(r.tag); }
        }
        if let Some(regs) = self.periodic.get_mut(scope) {
            for r in regs.iter_mut() {
                // First fire = last_fired=0. Pak >= interval pres prev.
                let fire = r.last_fired_unix_ms == 0
                    || now_unix_ms.saturating_sub(r.last_fired_unix_ms) >= r.min_interval_ms;
                if fire {
                    r.last_fired_unix_ms = now_unix_ms;
                    ready.push(r.tag.clone());
                }
            }
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_drain_when_online() {
        let mut r = BackgroundSyncRegistry::new();
        r.register_one_shot("https://x.com/", "upload");
        let ready = r.drain_ready("https://x.com/", 1000, true);
        assert_eq!(ready, vec!["upload"]);
        // Drained - second call empty.
        let ready2 = r.drain_ready("https://x.com/", 2000, true);
        assert!(ready2.is_empty());
    }

    #[test]
    fn no_drain_when_offline() {
        let mut r = BackgroundSyncRegistry::new();
        r.register_one_shot("/", "upload");
        let ready = r.drain_ready("/", 0, false);
        assert!(ready.is_empty());
    }

    #[test]
    fn periodic_interval_respected() {
        let mut r = BackgroundSyncRegistry::new();
        r.register_periodic("/", "refresh", 60000);
        // First call (last_fired=0): now - 0 = now >= interval - fires.
        let ready1 = r.drain_ready("/", 1000, true);
        assert_eq!(ready1, vec!["refresh"]);
        // Pred interval pase - prazdne (now - last_fired_1000 = 29000 < 60000).
        let ready2 = r.drain_ready("/", 30000, true);
        assert!(ready2.is_empty());
        // Po interval (now=100000 - last_fired=1000 = 99000 >= 60000).
        let ready3 = r.drain_ready("/", 100000, true);
        assert_eq!(ready3, vec!["refresh"]);
    }

    #[test]
    fn unregister_removes() {
        let mut r = BackgroundSyncRegistry::new();
        r.register_one_shot("/", "x");
        assert!(r.unregister("/", "x"));
        assert!(!r.unregister("/", "x")); // already gone
    }
}
