//! IntersectionObserver - async visibility detection (viewport intersection).
//!
//! Spec: https://www.w3.org/TR/intersection-observer/
//! new IntersectionObserver(callback, {root, rootMargin, threshold: [0, 0.5, 1]})
//! Fires when target's intersection ratio crosses a threshold.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct IntersectionObserverEntry {
    pub target_id: u64,
    pub bounding_client_rect: (f32, f32, f32, f32),
    pub intersection_rect: (f32, f32, f32, f32),
    pub root_bounds: (f32, f32, f32, f32),
    pub intersection_ratio: f32,
    pub is_intersecting: bool,
    pub time_ms: f64,
}

#[derive(Debug, Clone)]
pub struct ObserverConfig {
    pub root_id: Option<u64>,                 // None = viewport
    pub root_margin: (f32, f32, f32, f32),    // t r b l in px
    pub thresholds: Vec<f32>,                 // sorted, in [0,1]
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self { root_id: None, root_margin: (0.0, 0.0, 0.0, 0.0), thresholds: vec![0.0] }
    }
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub target_id: u64,
    pub last_ratio: f32,
    pub last_threshold_index: Option<usize>,
}

pub struct IntersectionObserver {
    pub id: u64,
    pub callback_id: u64,
    pub config: ObserverConfig,
    pub observations: Vec<Observation>,
    pub pending_entries: Vec<IntersectionObserverEntry>,
}

#[derive(Default)]
pub struct IntersectionObserverRegistry {
    pub observers: HashMap<u64, IntersectionObserver>,
    pub next_id: u64,
}

impl IntersectionObserverRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, callback_id: u64, config: ObserverConfig) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.observers.insert(id, IntersectionObserver {
            id, callback_id, config,
            observations: Vec::new(),
            pending_entries: Vec::new(),
        });
        id
    }

    pub fn observe(&mut self, observer_id: u64, target_id: u64) -> Result<(), String> {
        let o = self.observers.get_mut(&observer_id).ok_or("observer missing")?;
        if !o.observations.iter().any(|ob| ob.target_id == target_id) {
            o.observations.push(Observation { target_id, last_ratio: 0.0, last_threshold_index: None });
        }
        Ok(())
    }

    pub fn unobserve(&mut self, observer_id: u64, target_id: u64) {
        if let Some(o) = self.observers.get_mut(&observer_id) {
            o.observations.retain(|ob| ob.target_id != target_id);
        }
    }

    pub fn disconnect(&mut self, observer_id: u64) {
        if let Some(o) = self.observers.get_mut(&observer_id) {
            o.observations.clear();
            o.pending_entries.clear();
        }
    }

    /// Called per frame with target rects + root rect.
    /// rects: target_id -> bounding_client_rect
    /// root: viewport or root element rect after applying rootMargin.
    pub fn gather(&mut self, rects: &HashMap<u64, (f32, f32, f32, f32)>, root: (f32, f32, f32, f32), now_ms: f64) -> Vec<u64> {
        let mut changed = Vec::new();
        for (oid, obs) in self.observers.iter_mut() {
            let mut new_entries = Vec::new();
            let root_with_margin = apply_margin(root, obs.config.root_margin);
            for o in obs.observations.iter_mut() {
                let Some(rect) = rects.get(&o.target_id).copied() else { continue; };
                let inter = intersect(rect, root_with_margin);
                let ratio = if rect.2 > 0.0 && rect.3 > 0.0 {
                    (inter.2 * inter.3) / (rect.2 * rect.3)
                } else { 0.0 };
                let idx = threshold_index(&obs.config.thresholds, ratio);
                if Some(idx) != o.last_threshold_index {
                    o.last_threshold_index = Some(idx);
                    o.last_ratio = ratio;
                    let is_intersecting = ratio > 0.0 || obs.config.thresholds.iter().any(|t| *t == 0.0 && ratio == 0.0);
                    // is_intersecting per spec: target is intersecting if it intersects root (any overlap)
                    let is_intersecting = inter.2 > 0.0 && inter.3 > 0.0;
                    new_entries.push(IntersectionObserverEntry {
                        target_id: o.target_id,
                        bounding_client_rect: rect,
                        intersection_rect: inter,
                        root_bounds: root_with_margin,
                        intersection_ratio: ratio,
                        is_intersecting,
                        time_ms: now_ms,
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

    pub fn take_entries(&mut self, observer_id: u64) -> Vec<IntersectionObserverEntry> {
        self.observers.get_mut(&observer_id)
            .map(|o| std::mem::take(&mut o.pending_entries))
            .unwrap_or_default()
    }
}

fn apply_margin(r: (f32, f32, f32, f32), m: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    // m = top, right, bottom, left
    (r.0 - m.3, r.1 - m.0, r.2 + m.1 + m.3, r.3 + m.0 + m.2)
}

fn intersect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = (a.0 + a.2).min(b.0 + b.2);
    let y2 = (a.1 + a.3).min(b.1 + b.3);
    if x2 <= x1 || y2 <= y1 { return (x1, y1, 0.0, 0.0); }
    (x1, y1, x2 - x1, y2 - y1)
}

fn threshold_index(thresholds: &[f32], ratio: f32) -> usize {
    // index = number of thresholds that ratio has crossed.
    thresholds.iter().filter(|t| ratio >= **t).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_intersecting_ratio_1() {
        let mut r = IntersectionObserverRegistry::new();
        let id = r.create(1, ObserverConfig::default());
        r.observe(id, 100).unwrap();
        let mut rects = HashMap::new();
        rects.insert(100, (10.0, 10.0, 50.0, 50.0));
        let changed = r.gather(&rects, (0.0, 0.0, 200.0, 200.0), 0.0);
        assert!(changed.contains(&id));
        let entries = r.take_entries(id);
        assert!((entries[0].intersection_ratio - 1.0).abs() < 0.01);
        assert!(entries[0].is_intersecting);
    }

    #[test]
    fn no_overlap_ratio_0() {
        let mut r = IntersectionObserverRegistry::new();
        let id = r.create(1, ObserverConfig::default());
        r.observe(id, 100).unwrap();
        let mut rects = HashMap::new();
        rects.insert(100, (-200.0, -200.0, 10.0, 10.0));
        r.gather(&rects, (0.0, 0.0, 100.0, 100.0), 0.0);
        let entries = r.take_entries(id);
        assert_eq!(entries[0].intersection_ratio, 0.0);
        assert!(!entries[0].is_intersecting);
    }

    #[test]
    fn threshold_crossing() {
        let mut r = IntersectionObserverRegistry::new();
        let config = ObserverConfig { thresholds: vec![0.5], ..Default::default() };
        let id = r.create(1, config);
        r.observe(id, 100).unwrap();
        let mut rects = HashMap::new();
        // 40% visible -> below 0.5
        rects.insert(100, (-30.0, 0.0, 50.0, 50.0));
        r.gather(&rects, (0.0, 0.0, 200.0, 200.0), 0.0);
        r.take_entries(id);
        // 100% visible -> crosses 0.5
        rects.insert(100, (10.0, 10.0, 50.0, 50.0));
        let changed = r.gather(&rects, (0.0, 0.0, 200.0, 200.0), 0.0);
        assert!(changed.contains(&id));
    }

    #[test]
    fn root_margin_expands_root() {
        let mut r = IntersectionObserverRegistry::new();
        let config = ObserverConfig {
            root_margin: (50.0, 50.0, 50.0, 50.0),
            ..Default::default()
        };
        let id = r.create(1, config);
        r.observe(id, 100).unwrap();
        let mut rects = HashMap::new();
        // target just outside viewport - root margin pulls it in
        rects.insert(100, (-30.0, -30.0, 20.0, 20.0));
        r.gather(&rects, (0.0, 0.0, 200.0, 200.0), 0.0);
        let entries = r.take_entries(id);
        assert!(entries[0].is_intersecting);
    }

    #[test]
    fn unobserve_stops() {
        let mut r = IntersectionObserverRegistry::new();
        let id = r.create(1, ObserverConfig::default());
        r.observe(id, 100).unwrap();
        r.unobserve(id, 100);
        let mut rects = HashMap::new();
        rects.insert(100, (10.0, 10.0, 50.0, 50.0));
        assert!(!r.gather(&rects, (0.0, 0.0, 200.0, 200.0), 0.0).contains(&id));
    }
}
