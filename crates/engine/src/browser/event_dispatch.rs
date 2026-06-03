//! DOM Event dispatch - capture/target/bubble phases.
//!
//! Spec: https://dom.spec.whatwg.org/#interface-event
//! event.composedPath() returns flat list from target up.
//! addEventListener({capture, once, passive, signal}).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventPhase {
    None,
    Capturing,
    AtTarget,
    Bubbling,
}

#[derive(Debug, Clone)]
pub struct EventListener {
    pub id: u64,
    pub callback_id: u64,
    pub event_type: String,
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
    pub signal_id: Option<u64>,            // AbortSignal
    pub removed: bool,
}

#[derive(Debug, Clone)]
pub struct DispatchedEvent {
    pub event_type: String,
    pub target_id: u64,
    pub bubbles: bool,
    pub cancelable: bool,
    pub composed: bool,
    pub propagation_stopped: bool,
    pub immediate_stopped: bool,
    pub default_prevented: bool,
    pub phase: EventPhase,
    pub current_target: u64,
}

#[derive(Default)]
pub struct EventTarget {
    pub id: u64,
    pub parent: Option<u64>,
    pub listeners: Vec<EventListener>,
}

#[derive(Default)]
pub struct EventDispatcher {
    pub targets: HashMap<u64, EventTarget>,
    pub next_listener_id: u64,
}

impl EventDispatcher {
    pub fn new() -> Self { Self::default() }

    pub fn add_listener(&mut self, target: u64, event_type: &str, callback_id: u64, capture: bool, once: bool, passive: bool, signal_id: Option<u64>) -> u64 {
        self.next_listener_id += 1;
        let id = self.next_listener_id;
        let entry = self.targets.entry(target).or_insert_with(|| EventTarget { id: target, ..Default::default() });
        // Spec: duplicate (same type + callback + capture) is no-op.
        if entry.listeners.iter().any(|l| l.event_type == event_type && l.callback_id == callback_id && l.capture == capture && !l.removed) {
            return id;
        }
        entry.listeners.push(EventListener {
            id, callback_id,
            event_type: event_type.into(),
            capture, once, passive, signal_id,
            removed: false,
        });
        id
    }

    pub fn remove_listener(&mut self, target: u64, event_type: &str, callback_id: u64, capture: bool) {
        if let Some(t) = self.targets.get_mut(&target) {
            for l in t.listeners.iter_mut() {
                if l.event_type == event_type && l.callback_id == callback_id && l.capture == capture {
                    l.removed = true;
                }
            }
        }
    }

    pub fn set_parent(&mut self, target: u64, parent: Option<u64>) {
        self.targets.entry(target).or_insert_with(|| EventTarget { id: target, ..Default::default() }).parent = parent;
    }

    /// Returns ordered path from root to target (for capture); reversed for bubble.
    pub fn composed_path(&self, target: u64) -> Vec<u64> {
        let mut path = vec![target];
        let mut cur = target;
        while let Some(t) = self.targets.get(&cur) {
            if let Some(p) = t.parent {
                path.push(p);
                cur = p;
            } else { break; }
        }
        path.reverse();
        path
    }

    /// Dispatch event; returns ordered list of (listener_id, callback_id, phase) to invoke.
    /// Caller is responsible for invoking each callback and updating event state between.
    pub fn dispatch(&mut self, event_type: &str, target: u64, bubbles: bool) -> Vec<(u64, u64, EventPhase, u64)> {
        let path = self.composed_path(target);
        let mut callbacks = Vec::new();

        // Capture phase: root..target-1 (exclusive of target during capture).
        for &node in path.iter().take(path.len().saturating_sub(1)) {
            if let Some(t) = self.targets.get(&node) {
                for l in &t.listeners {
                    if l.removed || !l.capture || l.event_type != event_type { continue; }
                    callbacks.push((l.id, l.callback_id, EventPhase::Capturing, node));
                }
            }
        }
        // At target: capture-flagged listeners use Capturing phase, others use AtTarget.
        if let Some(t) = self.targets.get(&target) {
            for l in &t.listeners {
                if l.removed || l.event_type != event_type { continue; }
                callbacks.push((l.id, l.callback_id, EventPhase::AtTarget, target));
            }
        }
        // Bubble phase: target+1..root (exclusive of target).
        if bubbles {
            for &node in path.iter().rev().skip(1) {
                if let Some(t) = self.targets.get(&node) {
                    for l in &t.listeners {
                        if l.removed || l.capture || l.event_type != event_type { continue; }
                        callbacks.push((l.id, l.callback_id, EventPhase::Bubbling, node));
                    }
                }
            }
        }

        // Mark once listeners removed.
        for (lid, _, _, node) in &callbacks {
            if let Some(t) = self.targets.get_mut(node) {
                if let Some(l) = t.listeners.iter_mut().find(|l| l.id == *lid) {
                    if l.once { l.removed = true; }
                }
            }
        }
        callbacks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_path_returns_chain() {
        let mut d = EventDispatcher::new();
        d.set_parent(3, Some(2));
        d.set_parent(2, Some(1));
        d.set_parent(1, None);
        let p = d.composed_path(3);
        assert_eq!(p, vec![1, 2, 3]);
    }

    #[test]
    fn dispatch_visits_capture_then_target() {
        let mut d = EventDispatcher::new();
        d.set_parent(2, Some(1));
        let cap = d.add_listener(1, "click", 100, true, false, false, None);
        let at = d.add_listener(2, "click", 200, false, false, false, None);
        let invokes = d.dispatch("click", 2, false);
        assert_eq!(invokes[0].0, cap);
        assert_eq!(invokes[0].2, EventPhase::Capturing);
        assert_eq!(invokes[1].0, at);
        assert_eq!(invokes[1].2, EventPhase::AtTarget);
    }

    #[test]
    fn dispatch_bubbles_when_set() {
        let mut d = EventDispatcher::new();
        d.set_parent(2, Some(1));
        d.add_listener(1, "click", 100, false, false, false, None);
        d.add_listener(2, "click", 200, false, false, false, None);
        let invokes = d.dispatch("click", 2, true);
        let phases: Vec<_> = invokes.iter().map(|x| x.2).collect();
        assert!(phases.contains(&EventPhase::AtTarget));
        assert!(phases.contains(&EventPhase::Bubbling));
    }

    #[test]
    fn duplicate_listener_skipped() {
        let mut d = EventDispatcher::new();
        d.add_listener(1, "click", 100, false, false, false, None);
        d.add_listener(1, "click", 100, false, false, false, None);
        let t = &d.targets[&1];
        assert_eq!(t.listeners.iter().filter(|l| !l.removed).count(), 1);
    }

    #[test]
    fn once_self_removes() {
        let mut d = EventDispatcher::new();
        d.add_listener(1, "click", 100, false, true, false, None);
        d.dispatch("click", 1, false);
        d.dispatch("click", 1, false);
        let t = &d.targets[&1];
        // First dispatch invoked it; once flag marks removed.
        assert!(t.listeners.iter().find(|l| l.id == 1).unwrap().removed);
    }

    #[test]
    fn remove_listener_skips() {
        let mut d = EventDispatcher::new();
        d.add_listener(1, "click", 100, false, false, false, None);
        d.remove_listener(1, "click", 100, false);
        let invokes = d.dispatch("click", 1, false);
        assert!(invokes.is_empty());
    }
}
