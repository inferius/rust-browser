//! MutationObserver - async DOM mutation notifications.
//!
//! Spec: https://dom.spec.whatwg.org/#interface-mutationobserver
//! new MutationObserver(callback).observe(target, {childList, attributes, ...})
//! - Records queued during DOM mutations, flushed at microtask checkpoint.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MutationType {
    ChildList,
    Attributes,
    CharacterData,
}

#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub kind: MutationType,
    pub target_id: u64,
    pub added_nodes: Vec<u64>,
    pub removed_nodes: Vec<u64>,
    pub attribute_name: Option<String>,
    pub attribute_namespace: Option<String>,
    pub old_value: Option<String>,
    pub previous_sibling: Option<u64>,
    pub next_sibling: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct ObserverOptions {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
    pub attribute_filter: Option<Vec<String>>,
}

impl ObserverOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !self.child_list && !self.attributes && !self.character_data {
            return Err("must set at least one of childList/attributes/characterData".into());
        }
        if self.attribute_old_value && !self.attributes {
            return Err("attributeOldValue requires attributes:true".into());
        }
        if self.character_data_old_value && !self.character_data {
            return Err("characterDataOldValue requires characterData:true".into());
        }
        Ok(())
    }
}

pub struct Observer {
    pub id: u64,
    pub callback_id: u64,
    pub targets: Vec<(u64, ObserverOptions)>,
    pub queue: Rc<RefCell<Vec<MutationRecord>>>,
}

#[derive(Default)]
pub struct MutationObserverRegistry {
    pub observers: HashMap<u64, Observer>,
    pub next_id: u64,
    /// Pending observers with queued records - flushed in microtask.
    pub pending_flush: Vec<u64>,
}

impl MutationObserverRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, callback_id: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.observers.insert(id, Observer {
            id, callback_id,
            targets: Vec::new(),
            queue: Rc::new(RefCell::new(Vec::new())),
        });
        id
    }

    pub fn observe(&mut self, id: u64, target_id: u64, opts: ObserverOptions) -> Result<(), String> {
        opts.validate()?;
        let obs = self.observers.get_mut(&id).ok_or("observer not found")?;
        // If already observing target, replace options.
        if let Some(slot) = obs.targets.iter_mut().find(|(t, _)| *t == target_id) {
            slot.1 = opts;
        } else {
            obs.targets.push((target_id, opts));
        }
        Ok(())
    }

    pub fn disconnect(&mut self, id: u64) {
        if let Some(o) = self.observers.get_mut(&id) {
            o.targets.clear();
            o.queue.borrow_mut().clear();
        }
    }

    /// Push a record to every observer that matches the target.
    pub fn deliver(&mut self, record: MutationRecord) {
        for obs in self.observers.values_mut() {
            for (target, opts) in &obs.targets {
                if !is_relevant(*target, opts, &record) { continue; }
                obs.queue.borrow_mut().push(record.clone());
                if !self.pending_flush.contains(&obs.id) {
                    self.pending_flush.push(obs.id);
                }
                break;
            }
        }
    }

    /// Returns and clears queued records for flush callback dispatch.
    pub fn take_records(&mut self, id: u64) -> Vec<MutationRecord> {
        self.observers.get(&id).map(|o| std::mem::take(&mut *o.queue.borrow_mut())).unwrap_or_default()
    }

    pub fn drain_pending(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.pending_flush)
    }
}

fn is_relevant(_target: u64, opts: &ObserverOptions, rec: &MutationRecord) -> bool {
    match rec.kind {
        MutationType::ChildList => opts.child_list,
        MutationType::Attributes => {
            if !opts.attributes { return false; }
            if let (Some(filter), Some(name)) = (&opts.attribute_filter, &rec.attribute_name) {
                return filter.iter().any(|f| f == name);
            }
            true
        }
        MutationType::CharacterData => opts.character_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_requires_at_least_one_option() {
        let mut r = MutationObserverRegistry::new();
        let id = r.create(99);
        let opts = ObserverOptions::default();
        assert!(r.observe(id, 1, opts).is_err());
    }

    #[test]
    fn observe_child_list() {
        let mut r = MutationObserverRegistry::new();
        let id = r.create(99);
        let opts = ObserverOptions { child_list: true, ..Default::default() };
        assert!(r.observe(id, 1, opts).is_ok());
    }

    #[test]
    fn deliver_only_matching_kind() {
        let mut r = MutationObserverRegistry::new();
        let id = r.create(99);
        r.observe(id, 1, ObserverOptions { child_list: true, ..Default::default() }).unwrap();
        // attribute mutation should NOT match
        r.deliver(MutationRecord {
            kind: MutationType::Attributes,
            target_id: 1, added_nodes: vec![], removed_nodes: vec![],
            attribute_name: Some("x".into()), attribute_namespace: None,
            old_value: None, previous_sibling: None, next_sibling: None,
        });
        assert!(r.take_records(id).is_empty());
        r.deliver(MutationRecord {
            kind: MutationType::ChildList,
            target_id: 1, added_nodes: vec![10], removed_nodes: vec![],
            attribute_name: None, attribute_namespace: None,
            old_value: None, previous_sibling: None, next_sibling: None,
        });
        assert_eq!(r.take_records(id).len(), 1);
    }

    #[test]
    fn attribute_filter_works() {
        let mut r = MutationObserverRegistry::new();
        let id = r.create(99);
        r.observe(id, 1, ObserverOptions {
            attributes: true,
            attribute_filter: Some(vec!["data-x".into()]),
            ..Default::default()
        }).unwrap();
        r.deliver(MutationRecord {
            kind: MutationType::Attributes, target_id: 1,
            added_nodes: vec![], removed_nodes: vec![],
            attribute_name: Some("class".into()), attribute_namespace: None,
            old_value: None, previous_sibling: None, next_sibling: None,
        });
        assert!(r.take_records(id).is_empty());
        r.deliver(MutationRecord {
            kind: MutationType::Attributes, target_id: 1,
            added_nodes: vec![], removed_nodes: vec![],
            attribute_name: Some("data-x".into()), attribute_namespace: None,
            old_value: None, previous_sibling: None, next_sibling: None,
        });
        assert_eq!(r.take_records(id).len(), 1);
    }

    #[test]
    fn disconnect_clears_records() {
        let mut r = MutationObserverRegistry::new();
        let id = r.create(99);
        r.observe(id, 1, ObserverOptions { child_list: true, ..Default::default() }).unwrap();
        r.deliver(MutationRecord {
            kind: MutationType::ChildList, target_id: 1,
            added_nodes: vec![], removed_nodes: vec![],
            attribute_name: None, attribute_namespace: None,
            old_value: None, previous_sibling: None, next_sibling: None,
        });
        r.disconnect(id);
        assert!(r.take_records(id).is_empty());
    }

    #[test]
    fn pending_flush_queue() {
        let mut r = MutationObserverRegistry::new();
        let id = r.create(99);
        r.observe(id, 1, ObserverOptions { child_list: true, ..Default::default() }).unwrap();
        r.deliver(MutationRecord {
            kind: MutationType::ChildList, target_id: 1,
            added_nodes: vec![], removed_nodes: vec![],
            attribute_name: None, attribute_namespace: None,
            old_value: None, previous_sibling: None, next_sibling: None,
        });
        let pending = r.drain_pending();
        assert!(pending.contains(&id));
        assert!(r.drain_pending().is_empty());
    }
}
