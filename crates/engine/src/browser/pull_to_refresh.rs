//! Pull-to-refresh - mobile-style overscroll gesture to trigger reload.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtrState {
    Idle,
    Pulling,
    Releasing,
    Refreshing,
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
pub struct PullToRefresh {
    pub state: PtrState,
    pub pull_distance: f32,
    pub trigger_distance_px: f32,
    pub max_distance_px: f32,
    pub start_y: f32,
    pub current_y: f32,
}

impl Default for PullToRefresh {
    fn default() -> Self {
        Self {
            state: PtrState::Idle,
            pull_distance: 0.0,
            trigger_distance_px: 80.0,
            max_distance_px: 200.0,
            start_y: 0.0,
            current_y: 0.0,
        }
    }
}

impl PullToRefresh {
    pub fn new() -> Self { Self::default() }

    pub fn touch_start(&mut self, y: f32, scroll_y: f32) {
        if scroll_y <= 0.0 {
            self.state = PtrState::Pulling;
            self.start_y = y;
            self.current_y = y;
            self.pull_distance = 0.0;
        }
    }

    pub fn touch_move(&mut self, y: f32) {
        if self.state != PtrState::Pulling { return; }
        self.current_y = y;
        let raw = y - self.start_y;
        // Add resistance: feels less linear past trigger.
        if raw > self.trigger_distance_px {
            let extra = raw - self.trigger_distance_px;
            self.pull_distance = self.trigger_distance_px + extra * 0.5;
        } else {
            self.pull_distance = raw.max(0.0);
        }
        self.pull_distance = self.pull_distance.min(self.max_distance_px);
    }

    pub fn touch_end(&mut self) -> bool {
        let triggered = self.pull_distance >= self.trigger_distance_px;
        if self.state == PtrState::Pulling {
            self.state = if triggered { PtrState::Refreshing } else { PtrState::Releasing };
        }
        triggered
    }

    pub fn finish(&mut self) {
        self.state = PtrState::Idle;
        self.pull_distance = 0.0;
    }

    pub fn cancel(&mut self) {
        self.state = PtrState::Cancelled;
        self.pull_distance = 0.0;
    }

    pub fn progress(&self) -> f32 {
        (self.pull_distance / self.trigger_distance_px).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_only_at_top() {
        let mut p = PullToRefresh::new();
        p.touch_start(100.0, 50.0);
        assert_eq!(p.state, PtrState::Idle);
        p.touch_start(100.0, 0.0);
        assert_eq!(p.state, PtrState::Pulling);
    }

    #[test]
    fn move_accumulates_pull() {
        let mut p = PullToRefresh::new();
        p.touch_start(0.0, 0.0);
        p.touch_move(50.0);
        assert_eq!(p.pull_distance, 50.0);
    }

    #[test]
    fn resistance_past_trigger() {
        let mut p = PullToRefresh::new();
        p.trigger_distance_px = 80.0;
        p.touch_start(0.0, 0.0);
        p.touch_move(160.0);
        // 80 + (80 * 0.5) = 120
        assert_eq!(p.pull_distance, 120.0);
    }

    #[test]
    fn capped_at_max() {
        let mut p = PullToRefresh::new();
        p.touch_start(0.0, 0.0);
        p.touch_move(10_000.0);
        assert!(p.pull_distance <= p.max_distance_px);
    }

    #[test]
    fn touch_end_triggers_when_far_enough() {
        let mut p = PullToRefresh::new();
        p.touch_start(0.0, 0.0);
        p.touch_move(100.0);
        assert!(p.touch_end());
        assert_eq!(p.state, PtrState::Refreshing);
    }

    #[test]
    fn touch_end_releases_when_short() {
        let mut p = PullToRefresh::new();
        p.touch_start(0.0, 0.0);
        p.touch_move(30.0);
        assert!(!p.touch_end());
        assert_eq!(p.state, PtrState::Releasing);
    }
}
