//! Screen Wake Lock API - prevent screen sleep.
//!
//! Spec: https://w3c.github.io/screen-wake-lock/
//! navigator.wakeLock.request('screen') -> sentinel object. Released auto pri
//! tab background / page unload.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WakeLockKind {
    Screen,
}

#[derive(Default)]
pub struct WakeLockRegistry {
    pub active_sentinels: HashSet<u64>,
    pub next_id: u64,
    pub screen_active: bool,
}

impl WakeLockRegistry {
    pub fn new() -> Self { Self::default() }

    /// Request lock - vraci sentinel id. None pri denied (permission).
    pub fn request(&mut self, kind: WakeLockKind) -> Option<u64> {
        let _ = kind; // jen Screen aktualne
        self.next_id += 1;
        let id = self.next_id;
        self.active_sentinels.insert(id);
        self.screen_active = true;
        // Real: SetThreadExecutionState (Win) / IOPMAssertionCreateWithName (mac) /
        //       systemd-inhibit (Linux).
        Some(id)
    }

    /// Release sentinel.
    pub fn release(&mut self, id: u64) -> bool {
        let removed = self.active_sentinels.remove(&id);
        if self.active_sentinels.is_empty() {
            self.screen_active = false;
        }
        removed
    }

    pub fn release_all(&mut self) {
        self.active_sentinels.clear();
        self.screen_active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_activates_screen() {
        let mut r = WakeLockRegistry::new();
        r.request(WakeLockKind::Screen);
        assert!(r.screen_active);
    }

    #[test]
    fn release_last_deactivates() {
        let mut r = WakeLockRegistry::new();
        let id = r.request(WakeLockKind::Screen).unwrap();
        r.release(id);
        assert!(!r.screen_active);
    }

    #[test]
    fn release_one_of_many_keeps_active() {
        let mut r = WakeLockRegistry::new();
        let id1 = r.request(WakeLockKind::Screen).unwrap();
        let _id2 = r.request(WakeLockKind::Screen).unwrap();
        r.release(id1);
        assert!(r.screen_active);
    }

    #[test]
    fn release_all_clears() {
        let mut r = WakeLockRegistry::new();
        r.request(WakeLockKind::Screen);
        r.request(WakeLockKind::Screen);
        r.release_all();
        assert!(!r.screen_active);
        assert_eq!(r.active_sentinels.len(), 0);
    }
}
