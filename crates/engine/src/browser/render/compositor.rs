//! Compositor - layer tree + transform/opacity props running on compositor thread.
//!
//! Inspired by Chromium cc::LayerTreeHost.
//! Main thread mutates DOM/style/layout -> commits layer tree to compositor.
//! Compositor independently animates transform/opacity (cheap, no relayout).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerKind {
    Content,        // rasterized tiles
    Solid,          // background_color only
    Mask,
    Filter,
    Scroll,
    Sticky,
    Overlay,        // OS surface (video, canvas accelerated)
}

#[derive(Debug, Clone, Copy)]
pub struct LayerTransform {
    pub tx: f32,
    pub ty: f32,
    pub scale: f32,
    pub rotation_rad: f32,
}

impl Default for LayerTransform {
    fn default() -> Self { Self { tx: 0.0, ty: 0.0, scale: 1.0, rotation_rad: 0.0 } }
}

#[derive(Debug, Clone)]
pub struct CompositorLayer {
    pub id: u64,
    pub kind: LayerKind,
    pub bounds: (f32, f32, f32, f32),       // x, y, w, h
    pub transform: LayerTransform,
    pub opacity: f32,
    pub will_change_transform: bool,
    pub backface_visible: bool,
    pub blend_mode: u32,                    // shader id
    pub children: Vec<u64>,
    pub damage_rect: Option<(f32, f32, f32, f32)>,
}

impl CompositorLayer {
    pub fn new(id: u64, kind: LayerKind) -> Self {
        Self {
            id, kind,
            bounds: (0.0, 0.0, 0.0, 0.0),
            transform: LayerTransform::default(),
            opacity: 1.0,
            will_change_transform: false,
            backface_visible: true,
            blend_mode: 0,
            children: Vec::new(),
            damage_rect: None,
        }
    }
}

#[derive(Default)]
pub struct LayerTree {
    pub layers: HashMap<u64, CompositorLayer>,
    pub root: Option<u64>,
    pub next_id: u64,
}

impl LayerTree {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, kind: LayerKind) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.layers.insert(id, CompositorLayer::new(id, kind));
        id
    }

    pub fn append_child(&mut self, parent: u64, child: u64) -> Result<(), String> {
        if !self.layers.contains_key(&parent) { return Err("parent missing".into()); }
        if !self.layers.contains_key(&child) { return Err("child missing".into()); }
        self.layers.get_mut(&parent).unwrap().children.push(child);
        Ok(())
    }

    pub fn set_root(&mut self, id: u64) { self.root = Some(id); }

    /// Update layer transform - compositor-only animation, no main-thread tick.
    pub fn update_transform(&mut self, id: u64, t: LayerTransform) -> bool {
        if let Some(l) = self.layers.get_mut(&id) {
            l.transform = t;
            return true;
        }
        false
    }

    pub fn update_opacity(&mut self, id: u64, opacity: f32) -> bool {
        if let Some(l) = self.layers.get_mut(&id) {
            l.opacity = opacity.clamp(0.0, 1.0);
            return true;
        }
        false
    }

    pub fn mark_damage(&mut self, id: u64, rect: (f32, f32, f32, f32)) {
        if let Some(l) = self.layers.get_mut(&id) {
            l.damage_rect = Some(match l.damage_rect {
                Some(d) => union_rect(d, rect),
                None => rect,
            });
        }
    }

    pub fn clear_damage(&mut self) {
        for l in self.layers.values_mut() { l.damage_rect = None; }
    }

    /// Walk tree depth-first, returns vec of visited ids.
    pub fn traverse(&self) -> Vec<u64> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            self.walk(root, &mut out);
        }
        out
    }

    fn walk(&self, id: u64, out: &mut Vec<u64>) {
        out.push(id);
        if let Some(l) = self.layers.get(&id) {
            for c in &l.children { self.walk(*c, out); }
        }
    }
}

fn union_rect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let x1 = a.0.min(b.0);
    let y1 = a.1.min(b.1);
    let x2 = (a.0 + a.2).max(b.0 + b.2);
    let y2 = (a.1 + a.3).max(b.1 + b.3);
    (x1, y1, x2 - x1, y2 - y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_returns_unique_ids() {
        let mut t = LayerTree::new();
        let a = t.create(LayerKind::Content);
        let b = t.create(LayerKind::Content);
        assert_ne!(a, b);
    }

    #[test]
    fn child_append_and_traverse() {
        let mut t = LayerTree::new();
        let root = t.create(LayerKind::Content);
        let a = t.create(LayerKind::Content);
        let b = t.create(LayerKind::Content);
        t.append_child(root, a).unwrap();
        t.append_child(root, b).unwrap();
        t.set_root(root);
        let order = t.traverse();
        assert_eq!(order, vec![root, a, b]);
    }

    #[test]
    fn transform_update() {
        let mut t = LayerTree::new();
        let id = t.create(LayerKind::Content);
        t.update_transform(id, LayerTransform { tx: 10.0, ty: 20.0, scale: 2.0, rotation_rad: 0.0 });
        let l = t.layers.get(&id).unwrap();
        assert_eq!(l.transform.tx, 10.0);
        assert_eq!(l.transform.scale, 2.0);
    }

    #[test]
    fn opacity_clamped() {
        let mut t = LayerTree::new();
        let id = t.create(LayerKind::Content);
        t.update_opacity(id, 2.5);
        assert_eq!(t.layers.get(&id).unwrap().opacity, 1.0);
        t.update_opacity(id, -0.5);
        assert_eq!(t.layers.get(&id).unwrap().opacity, 0.0);
    }

    #[test]
    fn damage_union() {
        let mut t = LayerTree::new();
        let id = t.create(LayerKind::Content);
        t.mark_damage(id, (0.0, 0.0, 10.0, 10.0));
        t.mark_damage(id, (5.0, 5.0, 20.0, 20.0));
        let d = t.layers.get(&id).unwrap().damage_rect.unwrap();
        assert_eq!(d.0, 0.0);
        assert_eq!(d.1, 0.0);
        assert_eq!(d.2, 25.0);
        assert_eq!(d.3, 25.0);
    }

    #[test]
    fn missing_parent_errors() {
        let mut t = LayerTree::new();
        let c = t.create(LayerKind::Content);
        assert!(t.append_child(999, c).is_err());
    }
}
