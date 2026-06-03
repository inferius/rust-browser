//! View Transitions API foundation.
//!
//! `document.startViewTransition(updateCallback)` - capture DOM snapshot, run
//! update callback, capture new snapshot, animate cross-fade default (custom
//! pres CSS @view-transition pseudo-elementu).
//!
//! Inspired by Chromium `core/view_transition/`.

use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewTransitionState {
    /// Pre-capture: skip transition kdyz no update.
    Initial,
    /// Snapshot pred update.
    OldCaptured,
    /// Update callback runs.
    Updating,
    /// Snapshot po update.
    NewCaptured,
    /// Cross-fade anim running.
    Animating,
    /// Done.
    Finished,
    /// Skipped (uzivatel zavolal skipTransition).
    Skipped,
}

pub struct ViewTransition {
    pub state: ViewTransitionState,
    /// Snapshot pred update - real impl by capture layer textures.
    pub old_snapshot_layer_id: Option<usize>,
    pub new_snapshot_layer_id: Option<usize>,
    /// Start time pro anim.
    pub start_time: Option<std::time::Instant>,
    pub duration_secs: f32,
}

impl Default for ViewTransition {
    fn default() -> Self {
        Self {
            state: ViewTransitionState::Initial,
            old_snapshot_layer_id: None,
            new_snapshot_layer_id: None,
            start_time: None,
            duration_secs: 0.25,
        }
    }
}

impl ViewTransition {
    pub fn new() -> Self { Self::default() }

    /// Spustit transition. Real impl by:
    /// 1. Capture all layers do "old" snapshot textures
    /// 2. Run update_callback - DOM mutace
    /// 3. Layout, paint
    /// 4. Capture "new" snapshot
    /// 5. Animate cross-fade s @view-transition pseudo
    pub fn start(&mut self) {
        self.state = ViewTransitionState::OldCaptured;
        self.start_time = Some(std::time::Instant::now());
    }

    pub fn skip(&mut self) {
        self.state = ViewTransitionState::Skipped;
    }

    /// Sample current progress 0..1 pri animating state.
    pub fn progress(&self, now: std::time::Instant) -> f32 {
        let st = match self.start_time { Some(t) => t, None => return 0.0 };
        if self.state != ViewTransitionState::Animating { return 0.0; }
        let elapsed = now.duration_since(st).as_secs_f32();
        (elapsed / self.duration_secs.max(0.001)).clamp(0.0, 1.0)
    }
}

/// Registry per-document view transitions.
#[derive(Default)]
pub struct ViewTransitionRegistry {
    pub active: Option<Rc<RefCell<ViewTransition>>>,
}

impl ViewTransitionRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn start_new(&mut self) -> Rc<RefCell<ViewTransition>> {
        let vt = Rc::new(RefCell::new(ViewTransition::new()));
        vt.borrow_mut().start();
        self.active = Some(Rc::clone(&vt));
        vt
    }

    pub fn is_active(&self) -> bool {
        self.active.as_ref()
            .map(|vt| !matches!(vt.borrow().state, ViewTransitionState::Finished | ViewTransitionState::Skipped))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_transition_initial_state() {
        let vt = ViewTransition::new();
        assert_eq!(vt.state, ViewTransitionState::Initial);
    }

    #[test]
    fn start_transitions_state() {
        let mut vt = ViewTransition::new();
        vt.start();
        assert_eq!(vt.state, ViewTransitionState::OldCaptured);
        assert!(vt.start_time.is_some());
    }

    #[test]
    fn registry_tracks_active() {
        let mut r = ViewTransitionRegistry::new();
        assert!(!r.is_active());
        let _vt = r.start_new();
        assert!(r.is_active());
    }

    #[test]
    fn skip_marks_state() {
        let mut vt = ViewTransition::new();
        vt.start();
        vt.skip();
        assert_eq!(vt.state, ViewTransitionState::Skipped);
    }
}
