//! Hit-test tree - per-layer hit testable region (z-ordered).
//!
//! Chromium reference: cc::HitTestRegionList.
//! Each layer publishes a list of rects that should respond to pointer events.
//! Hit testing walks layers in painted-z order; the first match wins.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HitRegion {
    pub element_id: u64,
    pub rect: (f32, f32, f32, f32),
    pub clip_rects: Vec<(f32, f32, f32, f32)>,
    /// Higher z-index = drawn later = hit first.
    pub z_index: i32,
    pub pointer_events_enabled: bool,
}

#[derive(Default)]
pub struct HitTestTree {
    pub regions_by_layer: HashMap<u64, Vec<HitRegion>>,
    /// Layer painted order - last in list = topmost.
    pub layer_paint_order: Vec<u64>,
}

impl HitTestTree {
    pub fn new() -> Self { Self::default() }

    pub fn set_paint_order(&mut self, order: Vec<u64>) {
        self.layer_paint_order = order;
    }

    pub fn add_region(&mut self, layer_id: u64, region: HitRegion) {
        self.regions_by_layer.entry(layer_id).or_default().push(region);
    }

    pub fn clear_layer(&mut self, layer_id: u64) {
        self.regions_by_layer.remove(&layer_id);
    }

    /// Walk top-most layer downward; return first matching region.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&HitRegion> {
        for &layer in self.layer_paint_order.iter().rev() {
            let Some(regions) = self.regions_by_layer.get(&layer) else { continue; };
            // Within layer, higher z first.
            let mut sorted: Vec<&HitRegion> = regions.iter().collect();
            sorted.sort_by(|a, b| b.z_index.cmp(&a.z_index));
            for r in sorted {
                if !r.pointer_events_enabled { continue; }
                if !contains(r.rect, x, y) { continue; }
                if r.clip_rects.iter().any(|c| !contains(*c, x, y)) { continue; }
                return Some(r);
            }
        }
        None
    }
}

pub fn contains(rect: (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= rect.0 && x <= rect.0 + rect.2 && y >= rect.1 && y <= rect.1 + rect.3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(id: u64, rect: (f32, f32, f32, f32), z: i32) -> HitRegion {
        HitRegion { element_id: id, rect, clip_rects: vec![], z_index: z, pointer_events_enabled: true }
    }

    #[test]
    fn hit_test_single() {
        let mut t = HitTestTree::new();
        t.set_paint_order(vec![1]);
        t.add_region(1, reg(100, (0.0, 0.0, 50.0, 50.0), 0));
        let r = t.hit_test(10.0, 10.0).unwrap();
        assert_eq!(r.element_id, 100);
    }

    #[test]
    fn hit_test_top_layer_wins() {
        let mut t = HitTestTree::new();
        t.set_paint_order(vec![1, 2]);
        t.add_region(1, reg(100, (0.0, 0.0, 50.0, 50.0), 0));
        t.add_region(2, reg(200, (0.0, 0.0, 50.0, 50.0), 0));
        let r = t.hit_test(10.0, 10.0).unwrap();
        assert_eq!(r.element_id, 200); // top layer (last in paint order)
    }

    #[test]
    fn hit_test_z_within_layer() {
        let mut t = HitTestTree::new();
        t.set_paint_order(vec![1]);
        t.add_region(1, reg(100, (0.0, 0.0, 50.0, 50.0), 0));
        t.add_region(1, reg(200, (0.0, 0.0, 50.0, 50.0), 5));
        let r = t.hit_test(10.0, 10.0).unwrap();
        assert_eq!(r.element_id, 200);
    }

    #[test]
    fn miss_returns_none() {
        let mut t = HitTestTree::new();
        t.set_paint_order(vec![1]);
        t.add_region(1, reg(100, (0.0, 0.0, 50.0, 50.0), 0));
        assert!(t.hit_test(100.0, 100.0).is_none());
    }

    #[test]
    fn pointer_events_none_skips() {
        let mut t = HitTestTree::new();
        t.set_paint_order(vec![1]);
        let mut r = reg(100, (0.0, 0.0, 50.0, 50.0), 0);
        r.pointer_events_enabled = false;
        t.add_region(1, r);
        assert!(t.hit_test(10.0, 10.0).is_none());
    }

    #[test]
    fn clip_rect_excludes() {
        let mut t = HitTestTree::new();
        t.set_paint_order(vec![1]);
        let mut r = reg(100, (0.0, 0.0, 100.0, 100.0), 0);
        r.clip_rects = vec![(0.0, 0.0, 20.0, 20.0)];
        t.add_region(1, r);
        // Outside clip -> miss
        assert!(t.hit_test(50.0, 50.0).is_none());
        // Inside both -> hit
        assert!(t.hit_test(10.0, 10.0).is_some());
    }
}
