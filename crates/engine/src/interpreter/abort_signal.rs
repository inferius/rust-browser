//! AbortController / AbortSignal - cancellation primitive for fetch, streams, etc.
//!
//! Spec: https://dom.spec.whatwg.org/#aborting-ongoing-activities
//! AbortController.signal -> passed to fetch / event listener / etc.
//! AbortSignal.timeout(ms), AbortSignal.any([s1, s2]).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AbortSignal {
    pub id: u64,
    pub aborted: bool,
    pub reason: Option<String>,         // serialized reason value
    pub listener_ids: Vec<u64>,         // callback ids fired on abort
    pub source_signals: Vec<u64>,       // AbortSignal.any() composes from these
}

impl AbortSignal {
    pub fn new(id: u64) -> Self {
        Self { id, aborted: false, reason: None, listener_ids: Vec::new(), source_signals: Vec::new() }
    }
}

#[derive(Default)]
pub struct AbortRegistry {
    pub signals: HashMap<u64, AbortSignal>,
    pub next_id: u64,
    /// Composed signals: parent_id -> [composed_signal_ids]
    pub composed_by: HashMap<u64, Vec<u64>>,
    /// Pending firings - drained at microtask checkpoint to dispatch onabort.
    pub pending: Vec<u64>,
}

impl AbortRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create_signal(&mut self) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.signals.insert(id, AbortSignal::new(id));
        id
    }

    /// AbortSignal.any([sigs]) - returns new signal that aborts as soon as any source does.
    pub fn create_any(&mut self, sources: Vec<u64>) -> u64 {
        let new_id = self.create_signal();
        let mut any_already_aborted: Option<u64> = None;
        for s in &sources {
            self.composed_by.entry(*s).or_default().push(new_id);
            if self.signals.get(s).map(|sig| sig.aborted).unwrap_or(false) {
                any_already_aborted = Some(*s);
            }
        }
        let sig = self.signals.get_mut(&new_id).unwrap();
        sig.source_signals = sources;
        if let Some(src) = any_already_aborted {
            let reason = self.signals.get(&src).and_then(|s| s.reason.clone());
            let signal = self.signals.get_mut(&new_id).unwrap();
            signal.aborted = true;
            signal.reason = reason;
        }
        new_id
    }

    pub fn add_listener(&mut self, id: u64, callback_id: u64) {
        if let Some(s) = self.signals.get_mut(&id) {
            s.listener_ids.push(callback_id);
        }
    }

    pub fn abort(&mut self, id: u64, reason: Option<String>) {
        // Collect composed ids first to avoid borrow conflict.
        let composed = self.composed_by.get(&id).cloned().unwrap_or_default();
        if let Some(s) = self.signals.get_mut(&id) {
            if s.aborted { return; }
            s.aborted = true;
            s.reason = reason.clone();
            if !self.pending.contains(&id) { self.pending.push(id); }
        }
        for cid in composed {
            self.abort(cid, reason.clone());
        }
    }

    pub fn drain_pending(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.pending)
    }

    pub fn is_aborted(&self, id: u64) -> bool {
        self.signals.get(&id).map(|s| s.aborted).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_marks_signal() {
        let mut r = AbortRegistry::new();
        let id = r.create_signal();
        r.abort(id, Some("user cancel".into()));
        assert!(r.is_aborted(id));
        assert_eq!(r.signals.get(&id).unwrap().reason.as_deref(), Some("user cancel"));
    }

    #[test]
    fn double_abort_no_op() {
        let mut r = AbortRegistry::new();
        let id = r.create_signal();
        r.abort(id, Some("a".into()));
        r.abort(id, Some("b".into()));
        assert_eq!(r.signals.get(&id).unwrap().reason.as_deref(), Some("a"));
        assert_eq!(r.drain_pending().len(), 1);
    }

    #[test]
    fn any_propagates_abort() {
        let mut r = AbortRegistry::new();
        let s1 = r.create_signal();
        let s2 = r.create_signal();
        let composed = r.create_any(vec![s1, s2]);
        r.abort(s1, None);
        assert!(r.is_aborted(composed));
    }

    #[test]
    fn any_already_aborted_source() {
        let mut r = AbortRegistry::new();
        let s1 = r.create_signal();
        r.abort(s1, Some("done".into()));
        let composed = r.create_any(vec![s1]);
        assert!(r.is_aborted(composed));
    }

    #[test]
    fn listener_registered() {
        let mut r = AbortRegistry::new();
        let id = r.create_signal();
        r.add_listener(id, 42);
        assert_eq!(r.signals.get(&id).unwrap().listener_ids, vec![42]);
    }

    #[test]
    fn pending_drains_once() {
        let mut r = AbortRegistry::new();
        let id = r.create_signal();
        r.abort(id, None);
        assert_eq!(r.drain_pending(), vec![id]);
        assert!(r.drain_pending().is_empty());
    }
}
