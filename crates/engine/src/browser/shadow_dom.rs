//! Shadow DOM v1 - encapsulated DOM subtrees.
//!
//! Spec: https://dom.spec.whatwg.org/#shadow-trees
//! element.attachShadow({mode: 'open'|'closed', delegatesFocus, slotAssignment})
//! ::slotted(selector), ::part(name), :host, :host(selector), :host-context(selector).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowMode {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlotAssignment {
    Named,
    Manual,
}

#[derive(Debug, Clone)]
pub struct ShadowRoot {
    pub host_id: u64,
    pub mode: ShadowMode,
    pub delegates_focus: bool,
    pub slot_assignment: SlotAssignment,
    pub clonable: bool,
    pub serializable: bool,
    /// children directly under the shadow root.
    pub children: Vec<u64>,
}

impl ShadowRoot {
    pub fn new(host_id: u64, mode: ShadowMode) -> Self {
        Self {
            host_id, mode,
            delegates_focus: false,
            slot_assignment: SlotAssignment::Named,
            clonable: false,
            serializable: false,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Slot {
    pub id: u64,
    pub name: String,
    pub assigned_nodes: Vec<u64>,         // explicitly assigned nodes
}

#[derive(Default)]
pub struct ShadowDomRegistry {
    pub roots: HashMap<u64, ShadowRoot>,         // host_id -> root
    pub slots_by_root: HashMap<u64, Vec<Slot>>,  // host_id -> slots
}

impl ShadowDomRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn attach_shadow(&mut self, host_id: u64, mode: ShadowMode) -> Result<&ShadowRoot, String> {
        if self.roots.contains_key(&host_id) {
            return Err("shadow already attached".into());
        }
        self.roots.insert(host_id, ShadowRoot::new(host_id, mode));
        Ok(self.roots.get(&host_id).unwrap())
    }

    pub fn root_for(&self, host_id: u64) -> Option<&ShadowRoot> {
        self.roots.get(&host_id)
    }

    pub fn register_slot(&mut self, host_id: u64, slot: Slot) {
        self.slots_by_root.entry(host_id).or_default().push(slot);
    }

    /// Implicit slotting: assign light-DOM child to <slot name="X"> if its `slot` attribute matches.
    /// `default_slot_id` = the slot with no name (default slot).
    pub fn assign_node_implicit(&mut self, host_id: u64, node_id: u64, slot_attr: Option<&str>) {
        let Some(slots) = self.slots_by_root.get_mut(&host_id) else { return; };
        let target_name = slot_attr.unwrap_or("");
        for s in slots.iter_mut() {
            if s.name == target_name {
                if !s.assigned_nodes.contains(&node_id) {
                    s.assigned_nodes.push(node_id);
                }
                return;
            }
        }
    }

    pub fn flatten_descendants(&self, host_id: u64) -> Vec<u64> {
        let mut out = Vec::new();
        if let Some(root) = self.roots.get(&host_id) {
            for c in &root.children { out.push(*c); }
        }
        if let Some(slots) = self.slots_by_root.get(&host_id) {
            for s in slots { for n in &s.assigned_nodes { out.push(*n); } }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_shadow_once() {
        let mut r = ShadowDomRegistry::new();
        r.attach_shadow(1, ShadowMode::Open).unwrap();
        assert!(r.attach_shadow(1, ShadowMode::Open).is_err());
    }

    #[test]
    fn mode_recorded() {
        let mut r = ShadowDomRegistry::new();
        r.attach_shadow(1, ShadowMode::Closed).unwrap();
        assert_eq!(r.root_for(1).unwrap().mode, ShadowMode::Closed);
    }

    #[test]
    fn slot_assign_named() {
        let mut r = ShadowDomRegistry::new();
        r.attach_shadow(1, ShadowMode::Open).unwrap();
        r.register_slot(1, Slot { id: 10, name: "header".into(), assigned_nodes: vec![] });
        r.assign_node_implicit(1, 100, Some("header"));
        let slots = &r.slots_by_root[&1];
        assert_eq!(slots[0].assigned_nodes, vec![100]);
    }

    #[test]
    fn slot_default_unnamed() {
        let mut r = ShadowDomRegistry::new();
        r.attach_shadow(1, ShadowMode::Open).unwrap();
        r.register_slot(1, Slot { id: 10, name: String::new(), assigned_nodes: vec![] });
        r.assign_node_implicit(1, 100, None);
        let slots = &r.slots_by_root[&1];
        assert_eq!(slots[0].assigned_nodes, vec![100]);
    }

    #[test]
    fn flatten_descendants_includes_assigned() {
        let mut r = ShadowDomRegistry::new();
        r.attach_shadow(1, ShadowMode::Open).unwrap();
        r.roots.get_mut(&1).unwrap().children = vec![10, 20];
        r.register_slot(1, Slot { id: 30, name: "x".into(), assigned_nodes: vec![100, 200] });
        let d = r.flatten_descendants(1);
        assert_eq!(d, vec![10, 20, 100, 200]);
    }
}
