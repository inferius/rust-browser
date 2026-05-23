//! ResizeObserver - dispatched when element content-box / border-box changes size.
//!
//! Spec: https://www.w3.org/TR/resize-observer/
//! new ResizeObserver(callback).observe(target, {box: 'content-box'|'border-box'|'device-pixel-content-box'})
//! Fires on layout phase end; deep-observer-loop detection truncates after first
//! depth at which only shallower nodes have new sizes.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResizeBox {
    ContentBox,
    BorderBox,
    DevicePixelContentBox,
}

#[derive(Debug, Clone)]
pub struct ResizeObserverEntry {
    pub target_id: u64,
    pub content_rect: (f32, f32, f32, f32),
    pub content_box_size: (f32, f32),
    pub border_box_size: (f32, f32),
    pub device_pixel_content_box_size: (f32, f32),
}

#[derive(Debug, Clone, Copy)]
pub struct LastSize {
    pub content: (f32, f32),
    pub border: (f32, f32),
    pub device: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub target_id: u64,
    pub box_type: ResizeBox,
    pub last_reported: Option<LastSize>,
}

pub struct ResizeObserver {
    pub id: u64,
    pub callback_id: u64,
    pub observations: Vec<Observation>,
    pub pending_entries: Vec<ResizeObserverEntry>,
}

#[derive(Default)]
pub struct ResizeObserverRegistry {
    pub observers: HashMap<u64, ResizeObserver>,
    pub next_id: u64,
}

impl ResizeObserverRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, callback_id: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.observers.insert(id, ResizeObserver {
            id, callback_id,
            observations: Vec::new(),
            pending_entries: Vec::new(),
        });
        id
    }

    pub fn observe(&mut self, observer_id: u64, target: u64, box_type: ResizeBox) -> Result<(), String> {
        let o = self.observers.get_mut(&observer_id).ok_or("observer missing")?;
        if let Some(slot) = o.observations.iter_mut().find(|ob| ob.target_id == target) {
            slot.box_type = box_type;
            slot.last_reported = None;
        } else {
            o.observations.push(Observation { target_id: target, box_type, last_reported: None });
        }
        Ok(())
    }

    pub fn unobserve(&mut self, observer_id: u64, target: u64) {
        if let Some(o) = self.observers.get_mut(&observer_id) {
            o.observations.retain(|ob| ob.target_id != target);
        }
    }

    pub fn disconnect(&mut self, observer_id: u64) {
        if let Some(o) = self.observers.get_mut(&observer_id) {
            o.observations.clear();
            o.pending_entries.clear();
        }
    }

    /// Called by layout: pass current sizes per element. Generates entries for changed sizes.
    /// Returns observer IDs that have new entries to dispatch.
    pub fn gather(&mut self, sizes: &HashMap<u64, LastSize>) -> Vec<u64> {
        let mut changed = Vec::new();
        for (oid, obs) in self.observers.iter_mut() {
            let mut new_entries = Vec::new();
            for o in obs.observations.iter_mut() {
                let Some(now) = sizes.get(&o.target_id).copied() else { continue; };
                let prev = o.last_reported;
                let differs = match (prev, o.box_type) {
                    (None, _) => true,
                    (Some(p), ResizeBox::ContentBox) => p.content != now.content,
                    (Some(p), ResizeBox::BorderBox) => p.border != now.border,
                    (Some(p), ResizeBox::DevicePixelContentBox) => p.device != now.device,
                };
                if differs {
                    o.last_reported = Some(now);
                    new_entries.push(ResizeObserverEntry {
                        target_id: o.target_id,
                        content_rect: (0.0, 0.0, now.content.0, now.content.1),
                        content_box_size: now.content,
                        border_box_size: now.border,
                        device_pixel_content_box_size: now.device,
                    });
                }
            }
            if !new_entries.is_empty() {
                obs.pending_entries.extend(new_entries);
                changed.push(*oid);
            }
        }
        changed
    }

    pub fn take_entries(&mut self, observer_id: u64) -> Vec<ResizeObserverEntry> {
        self.observers.get_mut(&observer_id)
            .map(|o| std::mem::take(&mut o.pending_entries))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(w: f32, h: f32) -> LastSize {
        LastSize { content: (w, h), border: (w + 4.0, h + 4.0), device: (w * 2.0, h * 2.0) }
    }

    #[test]
    fn first_observation_dispatches() {
        let mut r = ResizeObserverRegistry::new();
        let id = r.create(1);
        r.observe(id, 100, ResizeBox::ContentBox).unwrap();
        let mut sizes = HashMap::new();
        sizes.insert(100, size(50.0, 50.0));
        let changed = r.gather(&sizes);
        assert!(changed.contains(&id));
        assert_eq!(r.take_entries(id).len(), 1);
    }

    #[test]
    fn unchanged_does_not_dispatch() {
        let mut r = ResizeObserverRegistry::new();
        let id = r.create(1);
        r.observe(id, 100, ResizeBox::ContentBox).unwrap();
        let mut sizes = HashMap::new();
        sizes.insert(100, size(50.0, 50.0));
        r.gather(&sizes); r.take_entries(id);
        let changed = r.gather(&sizes);
        assert!(!changed.contains(&id));
    }

    #[test]
    fn change_dispatches_again() {
        let mut r = ResizeObserverRegistry::new();
        let id = r.create(1);
        r.observe(id, 100, ResizeBox::ContentBox).unwrap();
        let mut sizes = HashMap::new();
        sizes.insert(100, size(50.0, 50.0));
        r.gather(&sizes); r.take_entries(id);
        sizes.insert(100, size(60.0, 50.0));
        let changed = r.gather(&sizes);
        assert!(changed.contains(&id));
    }

    #[test]
    fn unobserve_stops_dispatch() {
        let mut r = ResizeObserverRegistry::new();
        let id = r.create(1);
        r.observe(id, 100, ResizeBox::ContentBox).unwrap();
        r.unobserve(id, 100);
        let mut sizes = HashMap::new();
        sizes.insert(100, size(50.0, 50.0));
        assert!(!r.gather(&sizes).contains(&id));
    }

    #[test]
    fn border_box_differs_from_content_box() {
        let mut r = ResizeObserverRegistry::new();
        let id = r.create(1);
        r.observe(id, 100, ResizeBox::BorderBox).unwrap();
        let mut sizes = HashMap::new();
        sizes.insert(100, LastSize { content: (50.0, 50.0), border: (60.0, 60.0), device: (100.0, 100.0) });
        r.gather(&sizes); r.take_entries(id);
        // border change only -> still dispatch
        sizes.insert(100, LastSize { content: (50.0, 50.0), border: (70.0, 60.0), device: (100.0, 100.0) });
        assert!(r.gather(&sizes).contains(&id));
    }
}
