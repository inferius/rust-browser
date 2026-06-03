//! Autoscroll - middle-click drag scrolling (Chrome behavior).
//!
//! Activated by middle-mouse press; scroll velocity = distance from anchor.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AutoscrollState {
    Inactive,
    Active,
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
pub struct Autoscroll {
    pub state: AutoscrollState,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub deadzone_px: f32,             // small radius where velocity = 0
    pub max_velocity_px_per_sec: f32,
    pub last_tick: Option<Instant>,
    pub scroll_x: f32,
    pub scroll_y: f32,
}

impl Default for Autoscroll {
    fn default() -> Self {
        Self {
            state: AutoscrollState::Inactive,
            anchor_x: 0.0, anchor_y: 0.0,
            current_x: 0.0, current_y: 0.0,
            deadzone_px: 20.0,
            max_velocity_px_per_sec: 2400.0,
            last_tick: None,
            scroll_x: 0.0, scroll_y: 0.0,
        }
    }
}

impl Autoscroll {
    pub fn new() -> Self { Self::default() }

    pub fn start(&mut self, x: f32, y: f32) {
        self.state = AutoscrollState::Active;
        self.anchor_x = x;
        self.anchor_y = y;
        self.current_x = x;
        self.current_y = y;
        self.last_tick = Some(Instant::now());
    }

    pub fn cancel(&mut self) {
        self.state = AutoscrollState::Cancelled;
        self.last_tick = None;
    }

    pub fn cursor(&mut self, x: f32, y: f32) {
        self.current_x = x;
        self.current_y = y;
    }

    /// Compute scroll delta for this frame.
    pub fn tick(&mut self, now: Instant) -> (f32, f32) {
        if self.state != AutoscrollState::Active { return (0.0, 0.0); }
        let dt = match self.last_tick {
            Some(t) => (now - t).as_secs_f32(),
            None => 0.016,
        };
        self.last_tick = Some(now);
        let dx = self.current_x - self.anchor_x;
        let dy = self.current_y - self.anchor_y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= self.deadzone_px { return (0.0, 0.0); }
        let effective = dist - self.deadzone_px;
        let velocity = (effective * 4.0).min(self.max_velocity_px_per_sec);
        let scale = velocity * dt / dist;
        (dx * scale, dy * scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn idle_returns_zero_delta() {
        let mut a = Autoscroll::new();
        let (dx, dy) = a.tick(Instant::now());
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn within_deadzone_no_scroll() {
        let mut a = Autoscroll::new();
        a.start(500.0, 500.0);
        a.cursor(505.0, 505.0);
        let (dx, dy) = a.tick(Instant::now());
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn beyond_deadzone_scrolls_proportionally() {
        let mut a = Autoscroll::new();
        a.start(500.0, 500.0);
        let t0 = Instant::now();
        a.last_tick = Some(t0);
        a.cursor(700.0, 500.0);
        let (dx, dy) = a.tick(t0 + Duration::from_millis(100));
        assert!(dx > 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn cancel_disables() {
        let mut a = Autoscroll::new();
        a.start(0.0, 0.0);
        a.cancel();
        assert_eq!(a.state, AutoscrollState::Cancelled);
        let (dx, dy) = a.tick(Instant::now());
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn velocity_capped() {
        let mut a = Autoscroll::new();
        a.start(0.0, 0.0);
        let t0 = Instant::now();
        a.last_tick = Some(t0);
        a.cursor(10000.0, 0.0);
        let (dx, _) = a.tick(t0 + Duration::from_secs(1));
        // dx should be capped to max_velocity_px_per_sec * 1.0s = 2400
        assert!(dx <= a.max_velocity_px_per_sec + 1.0);
    }
}
