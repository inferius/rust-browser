//! View Transitions API - document.startViewTransition(callback).
//!
//! Spec: https://www.w3.org/TR/css-view-transitions-1/
//! Snapshot old DOM, run callback (DOM update), snapshot new DOM, cross-fade.
//! ::view-transition pseudo-elements pro per-element animation.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionPhase {
    Pending,
    Capturing,        // old snapshot
    Updating,         // callback runs
    Animating,        // pseudo tree displayed
    Finished,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct CapturedElement {
    pub name: String,                // view-transition-name property
    pub rect: (f32, f32, f32, f32),  // x, y, w, h v viewport coords
    pub snapshot_id: u64,            // textura id v atlasu
}

#[derive(Debug)]
pub struct ViewTransition {
    pub phase: TransitionPhase,
    pub old_captures: Vec<CapturedElement>,
    pub new_captures: Vec<CapturedElement>,
    pub start_unix_ms: u64,
    pub duration_ms: f64,
    pub skipped_reason: Option<String>,
}

impl ViewTransition {
    pub fn new() -> Self {
        Self {
            phase: TransitionPhase::Pending,
            old_captures: Vec::new(),
            new_captures: Vec::new(),
            start_unix_ms: 0,
            duration_ms: 250.0,
            skipped_reason: None,
        }
    }

    pub fn capture_old(&mut self, elements: Vec<CapturedElement>) {
        self.phase = TransitionPhase::Capturing;
        self.old_captures = elements;
    }

    pub fn capture_new(&mut self, elements: Vec<CapturedElement>) {
        self.phase = TransitionPhase::Animating;
        self.new_captures = elements;
    }

    pub fn skip(&mut self, reason: &str) {
        self.phase = TransitionPhase::Skipped;
        self.skipped_reason = Some(reason.into());
    }

    pub fn finish(&mut self) {
        self.phase = TransitionPhase::Finished;
    }

    /// Pair old+new captures by name for cross-fade rendering.
    pub fn paired(&self) -> Vec<(Option<&CapturedElement>, Option<&CapturedElement>)> {
        let mut by_name: HashMap<&str, (Option<&CapturedElement>, Option<&CapturedElement>)> = HashMap::new();
        for e in &self.old_captures { by_name.entry(&e.name).or_insert((None, None)).0 = Some(e); }
        for e in &self.new_captures { by_name.entry(&e.name).or_insert((None, None)).1 = Some(e); }
        by_name.into_values().collect()
    }
}

impl Default for ViewTransition {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(name: &str, snap: u64) -> CapturedElement {
        CapturedElement { name: name.into(), rect: (0.0, 0.0, 100.0, 100.0), snapshot_id: snap }
    }

    #[test]
    fn capture_progresses_phase() {
        let mut t = ViewTransition::new();
        assert_eq!(t.phase, TransitionPhase::Pending);
        t.capture_old(vec![el("root", 1)]);
        assert_eq!(t.phase, TransitionPhase::Capturing);
        t.capture_new(vec![el("root", 2)]);
        assert_eq!(t.phase, TransitionPhase::Animating);
    }

    #[test]
    fn skip_records_reason() {
        let mut t = ViewTransition::new();
        t.skip("hidden");
        assert_eq!(t.phase, TransitionPhase::Skipped);
        assert_eq!(t.skipped_reason.as_deref(), Some("hidden"));
    }

    #[test]
    fn pair_by_name() {
        let mut t = ViewTransition::new();
        t.capture_old(vec![el("a", 1), el("b", 2)]);
        t.capture_new(vec![el("a", 3), el("c", 4)]);
        let pairs = t.paired();
        assert_eq!(pairs.len(), 3); // a (old+new), b (old only), c (new only)
        // exactly one entry should have both sides populated
        let both = pairs.iter().filter(|(o, n)| o.is_some() && n.is_some()).count();
        assert_eq!(both, 1);
    }
}
