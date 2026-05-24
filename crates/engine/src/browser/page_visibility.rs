//! Page Visibility API + Lifecycle API.
//!
//! Spec: https://www.w3.org/TR/page-visibility/
//!       https://wicg.github.io/page-lifecycle/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisibilityState {
    Visible,
    Hidden,
    Prerender,         // legacy
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LifecycleState {
    Active,            // user focused this tab
    Passive,           // visible but not focused
    Hidden,            // tab in background
    Frozen,            // resources released; can no longer run task queues
    Discarded,         // tab killed; only meta survives
    Terminated,        // window close in progress
}

#[derive(Debug, Clone)]
pub struct VisibilityTracker {
    pub state: VisibilityState,
    pub lifecycle_state: LifecycleState,
    pub last_change_unix_ms: u64,
}

impl Default for VisibilityTracker {
    fn default() -> Self {
        Self {
            state: VisibilityState::Visible,
            lifecycle_state: LifecycleState::Active,
            last_change_unix_ms: 0,
        }
    }
}

impl VisibilityTracker {
    pub fn new() -> Self { Self::default() }

    /// Caller signals window focus + tab visibility changes.
    pub fn update(&mut self, has_focus: bool, is_visible: bool, now: u64) -> Option<TransitionEvent> {
        let new_state = if is_visible { VisibilityState::Visible } else { VisibilityState::Hidden };
        let mut new_lifecycle = self.lifecycle_state;
        if !is_visible {
            new_lifecycle = LifecycleState::Hidden;
        } else if !has_focus {
            new_lifecycle = LifecycleState::Passive;
        } else {
            new_lifecycle = LifecycleState::Active;
        }
        // Don't downgrade Frozen/Discarded automatically.
        if matches!(self.lifecycle_state, LifecycleState::Frozen | LifecycleState::Discarded) {
            new_lifecycle = self.lifecycle_state;
        }
        if new_state == self.state && new_lifecycle == self.lifecycle_state {
            return None;
        }
        let from = self.lifecycle_state;
        let to = new_lifecycle;
        self.state = new_state;
        self.lifecycle_state = new_lifecycle;
        self.last_change_unix_ms = now;
        Some(TransitionEvent { from, to, visibility: new_state })
    }

    /// Freeze: called when OS suggests tab is idle and resources should be released.
    pub fn freeze(&mut self, now: u64) -> Option<TransitionEvent> {
        if self.lifecycle_state != LifecycleState::Hidden { return None; }
        let from = self.lifecycle_state;
        self.lifecycle_state = LifecycleState::Frozen;
        self.last_change_unix_ms = now;
        Some(TransitionEvent { from, to: self.lifecycle_state, visibility: self.state })
    }

    pub fn resume(&mut self, has_focus: bool, now: u64) -> Option<TransitionEvent> {
        if !matches!(self.lifecycle_state, LifecycleState::Frozen) { return None; }
        let from = self.lifecycle_state;
        self.lifecycle_state = if has_focus { LifecycleState::Active } else { LifecycleState::Passive };
        self.last_change_unix_ms = now;
        Some(TransitionEvent { from, to: self.lifecycle_state, visibility: self.state })
    }
}

#[derive(Debug, Clone)]
pub struct TransitionEvent {
    pub from: LifecycleState,
    pub to: LifecycleState,
    pub visibility: VisibilityState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_change() {
        let mut v = VisibilityTracker::new();
        let ev = v.update(false, true, 1000);
        assert!(ev.is_some());
        assert_eq!(v.lifecycle_state, LifecycleState::Passive);
    }

    #[test]
    fn hidden_when_invisible() {
        let mut v = VisibilityTracker::new();
        v.update(false, false, 1000);
        assert_eq!(v.state, VisibilityState::Hidden);
        assert_eq!(v.lifecycle_state, LifecycleState::Hidden);
    }

    #[test]
    fn freeze_from_hidden_only() {
        let mut v = VisibilityTracker::new();
        v.update(false, false, 1000);
        let ev = v.freeze(2000);
        assert!(ev.is_some());
        assert_eq!(v.lifecycle_state, LifecycleState::Frozen);
    }

    #[test]
    fn freeze_from_visible_no_op() {
        let mut v = VisibilityTracker::new();
        assert!(v.freeze(0).is_none());
    }

    #[test]
    fn resume_from_frozen() {
        let mut v = VisibilityTracker::new();
        v.update(false, false, 0);
        v.freeze(1);
        let ev = v.resume(true, 2);
        assert!(ev.is_some());
        assert_eq!(v.lifecycle_state, LifecycleState::Active);
    }
}
