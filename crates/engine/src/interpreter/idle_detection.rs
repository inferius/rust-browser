//! Idle Detection API.
//!
//! Spec: https://wicg.github.io/idle-detection/
//! detect uzivatelske idle (no input X seconds) + screen lock.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserIdleState {
    Active,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenIdleState {
    Locked,
    Unlocked,
}

pub struct IdleDetector {
    pub threshold_seconds: u64,
    pub last_input_unix_secs: u64,
    pub screen_locked: bool,
}

impl Default for IdleDetector {
    fn default() -> Self {
        Self {
            threshold_seconds: 60,
            last_input_unix_secs: now_secs(),
            screen_locked: false,
        }
    }
}

impl IdleDetector {
    pub fn new(threshold_seconds: u64) -> Self {
        Self { threshold_seconds, ..Default::default() }
    }

    pub fn record_input(&mut self) {
        self.last_input_unix_secs = now_secs();
    }

    pub fn user_state(&self) -> UserIdleState {
        let idle_for = now_secs().saturating_sub(self.last_input_unix_secs);
        if idle_for >= self.threshold_seconds { UserIdleState::Idle }
        else { UserIdleState::Active }
    }

    pub fn screen_state(&self) -> ScreenIdleState {
        if self.screen_locked { ScreenIdleState::Locked }
        else { ScreenIdleState::Unlocked }
    }

    pub fn set_screen_locked(&mut self, locked: bool) {
        self.screen_locked = locked;
    }
}

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_after_recent_input() {
        let mut d = IdleDetector::new(60);
        d.record_input();
        assert_eq!(d.user_state(), UserIdleState::Active);
    }

    #[test]
    fn idle_when_past_threshold() {
        let mut d = IdleDetector::new(1);
        d.last_input_unix_secs = now_secs().saturating_sub(10);
        assert_eq!(d.user_state(), UserIdleState::Idle);
    }

    #[test]
    fn screen_state_toggle() {
        let mut d = IdleDetector::new(60);
        assert_eq!(d.screen_state(), ScreenIdleState::Unlocked);
        d.set_screen_locked(true);
        assert_eq!(d.screen_state(), ScreenIdleState::Locked);
    }
}
