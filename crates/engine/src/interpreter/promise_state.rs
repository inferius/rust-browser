//! Promise spec primitive - fulfilled/rejected/pending tristate + reactions.
//!
//! ECMA-262 27.2 - Promise objects.
//! Promise resolution: deduplicate, follow thenable chain, schedule microtask reactions.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct PromiseRecord {
    pub id: u64,
    pub state: PromiseState,
    pub value_ref: Option<u64>,            // opaque value/error reference
    pub fulfill_reactions: Vec<Reaction>,
    pub reject_reactions: Vec<Reaction>,
    pub is_handled: bool,                  // unhandled rejection tracking
}

#[derive(Debug, Clone)]
pub struct Reaction {
    pub handler_callback_id: u64,
    pub resulting_promise_id: u64,
}

#[derive(Default)]
pub struct PromiseStore {
    pub promises: HashMap<u64, PromiseRecord>,
    pub next_id: u64,
    /// Promises that became rejected without a handler attached -
    /// fire unhandledrejection at next microtask checkpoint.
    pub pending_unhandled: Vec<u64>,
}

impl PromiseStore {
    pub fn new() -> Self { Self::default() }

    pub fn create_pending(&mut self) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.promises.insert(id, PromiseRecord {
            id,
            state: PromiseState::Pending,
            value_ref: None,
            fulfill_reactions: Vec::new(),
            reject_reactions: Vec::new(),
            is_handled: false,
        });
        id
    }

    pub fn resolve(&mut self, id: u64, value_ref: u64) -> Vec<u64> {
        let mut to_invoke = Vec::new();
        if let Some(p) = self.promises.get_mut(&id) {
            if p.state != PromiseState::Pending { return to_invoke; }
            p.state = PromiseState::Fulfilled;
            p.value_ref = Some(value_ref);
            for r in p.fulfill_reactions.drain(..) {
                to_invoke.push(r.handler_callback_id);
            }
            p.reject_reactions.clear();
        }
        to_invoke
    }

    pub fn reject(&mut self, id: u64, reason_ref: u64) -> Vec<u64> {
        let mut to_invoke = Vec::new();
        let mut became_unhandled = false;
        if let Some(p) = self.promises.get_mut(&id) {
            if p.state != PromiseState::Pending { return to_invoke; }
            p.state = PromiseState::Rejected;
            p.value_ref = Some(reason_ref);
            for r in p.reject_reactions.drain(..) {
                to_invoke.push(r.handler_callback_id);
            }
            p.fulfill_reactions.clear();
            if p.reject_reactions.is_empty() && !p.is_handled && to_invoke.is_empty() {
                became_unhandled = true;
            }
        }
        if became_unhandled { self.pending_unhandled.push(id); }
        to_invoke
    }

    /// Attach .then(fulfill, reject) - returns the new promise id.
    pub fn then(&mut self, id: u64, fulfill_cb: u64, reject_cb: Option<u64>) -> u64 {
        let new_id = self.create_pending();
        if let Some(p) = self.promises.get_mut(&id) {
            p.is_handled = true;
            match p.state {
                PromiseState::Pending => {
                    p.fulfill_reactions.push(Reaction { handler_callback_id: fulfill_cb, resulting_promise_id: new_id });
                    if let Some(rcb) = reject_cb {
                        p.reject_reactions.push(Reaction { handler_callback_id: rcb, resulting_promise_id: new_id });
                    }
                }
                PromiseState::Fulfilled => {
                    // Schedule fulfill_cb via microtask (caller handles).
                }
                PromiseState::Rejected => {
                    // Schedule reject_cb via microtask (caller handles).
                    if reject_cb.is_some() {
                        // Promise was rejected, but we've now handled it.
                        if let Some(idx) = self.pending_unhandled.iter().position(|x| *x == id) {
                            self.pending_unhandled.swap_remove(idx);
                        }
                    }
                }
            }
        }
        new_id
    }

    pub fn state(&self, id: u64) -> Option<PromiseState> {
        self.promises.get(&id).map(|p| p.state)
    }

    pub fn drain_unhandled(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.pending_unhandled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_starts_pending() {
        let mut s = PromiseStore::new();
        let id = s.create_pending();
        assert_eq!(s.state(id), Some(PromiseState::Pending));
    }

    #[test]
    fn resolve_fires_fulfill_reactions() {
        let mut s = PromiseStore::new();
        let id = s.create_pending();
        s.then(id, 100, None);
        let cbs = s.resolve(id, 42);
        assert_eq!(cbs, vec![100]);
        assert_eq!(s.state(id), Some(PromiseState::Fulfilled));
    }

    #[test]
    fn double_resolve_ignored() {
        let mut s = PromiseStore::new();
        let id = s.create_pending();
        s.resolve(id, 1);
        let cbs = s.resolve(id, 2);
        assert!(cbs.is_empty());
    }

    #[test]
    fn reject_fires_reject_reactions() {
        let mut s = PromiseStore::new();
        let id = s.create_pending();
        s.then(id, 100, Some(200));
        let cbs = s.reject(id, 5);
        assert_eq!(cbs, vec![200]);
    }

    #[test]
    fn unhandled_rejection_tracked() {
        let mut s = PromiseStore::new();
        let id = s.create_pending();
        s.reject(id, 1);
        // No reactions -> queued as unhandled.
        let pending = s.drain_unhandled();
        assert_eq!(pending, vec![id]);
    }

    #[test]
    fn then_clears_unhandled_for_already_rejected() {
        let mut s = PromiseStore::new();
        let id = s.create_pending();
        s.reject(id, 1);
        s.then(id, 0, Some(99));
        // We attached handler after rejection - unhandled cleared.
        assert!(s.pending_unhandled.is_empty());
    }
}
