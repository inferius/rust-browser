//! Compositor-driven animations (priority 4 z RENDER_RETROSPECTIVE).
//!
//! Inspired by Chromium `cc/animation/animation_host.cc` + Servo
//! `animation/animation.rs`.
//!
//! ## Principy
//!
//! V tradicnim browseru animace prochazi celym pipelinem:
//! `JS / @keyframes -> cascade -> apply_paint_animations -> layout/paint
//!  -> render -> compose`. Pri animaci 60 fps = 60x cascade + paint per second.
//!
//! Compositor-driven shortcut: pokud anim mutuje JEN `transform` nebo `opacity`,
//! tyto property se aplikuji az pri composite pass (LayerNode.opacity /
//! LayerNode.transform). Style/layout/paint zustanou stable -> texture cache
//! hit -> jen GPU compose s novym uniform.
//!
//! ## API
//!
//! - `CompositorAnimStore`: registry aktivnich anim (klic = node_ptr usize)
//! - `CompositorAnim::Opacity { from, to, ... }` / `Transform { from, to, ... }`
//! - `tick(now) -> bool`: posune progress, vraci true pokud jakekoli anim
//!   tikla (volajici muze skip paint pipeline)
//! - `apply_to_layer_tree(root)`: prepise LayerNode.opacity/transform podle
//!   currentValue
//!
//! ## Co tohle NEPRINASI
//!
//! - Off-main-thread compose (compositor thread split). Stale single-thread.
//! - JS access do running anim (`Element.animate()` API). To by sel pres
//!   navazani na tento store.
//!
//! Win: pri pure transform/opacity animaci (button hover scale, fade-in)
//! 100x rychlejsi nez full re-cascade per frame.

use super::LayerNode;
use crate::browser::layout::TransformOp;
use std::collections::HashMap;
use std::time::Instant;

/// Single compositor-driven animation entry.
#[derive(Debug, Clone)]
pub enum CompositorAnim {
    /// Anim opacity od `from` do `to`.
    Opacity {
        from: f32,
        to: f32,
        start: Instant,
        duration_ms: f32,
        easing: Easing,
        /// Pocet iteraci (f32::INFINITY = forever).
        iterations: f32,
        /// alternate = lichá iterace forward, suda reverse.
        alternate: bool,
        /// Aktualni hodnota (cache pro apply).
        current: f32,
        /// Done flag - po vsech iteracich.
        done: bool,
    },
    /// Anim transform op (TransformOp je clone-able enum).
    Transform {
        from: TransformOp,
        to: TransformOp,
        start: Instant,
        duration_ms: f32,
        easing: Easing,
        iterations: f32,
        alternate: bool,
        current: TransformOp,
        done: bool,
    },
}

/// Casovaci funkce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// t in [0,1] -> eased t in [0,1].
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 { 2.0 * t * t }
                else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
            }
        }
    }
}

/// Store mapping node_id (Rc::as_ptr usize) -> list aktivnich animaci.
/// Jeden node muze mit 2 souc. anim (jednu opacity + jednu transform).
#[derive(Debug, Default, Clone)]
pub struct CompositorAnimStore {
    entries: HashMap<usize, Vec<CompositorAnim>>,
}

impl CompositorAnimStore {
    pub fn new() -> Self { Self::default() }

    /// Vlozit novou anim. Existujici se SAME varianty (Opacity/Transform)
    /// pro stejny node se nahradi (chrome-style: novy keyframe override).
    pub fn insert(&mut self, node_id: usize, anim: CompositorAnim) {
        let list = self.entries.entry(node_id).or_default();
        let is_opacity = matches!(anim, CompositorAnim::Opacity { .. });
        list.retain(|a| matches!(a, CompositorAnim::Opacity { .. }) != is_opacity);
        list.push(anim);
    }

    /// Remove all anim pro node (e.g. animation-name: none).
    pub fn remove(&mut self, node_id: usize) {
        self.entries.remove(&node_id);
    }

    /// Pocet aktivnich animaci.
    pub fn active_count(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    /// Ma node aktivni compositor anim? Gate check: fast path smi bezet jen
    /// kdyz store pokryva VSECHNY aktivni animace (jinak neregistrovane
    /// zamrznou - tick maji jen pri full framu).
    pub fn has_node(&self, node_id: usize) -> bool {
        self.entries.contains_key(&node_id)
    }

    /// Tick - posune progress kazde anim na now. Vraci true pokud
    /// jakakoli anim aktivni (still ticking, ne done).
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut any_active = false;
        let mut to_remove: Vec<usize> = Vec::new();
        for (&node_id, list) in self.entries.iter_mut() {
            for anim in list.iter_mut() {
                if tick_anim(anim, now) {
                    any_active = true;
                }
            }
            // Drop done anim per-node (Vec::retain).
            list.retain(|a| !is_done(a));
            if list.is_empty() {
                to_remove.push(node_id);
            }
        }
        for id in to_remove {
            self.entries.remove(&id);
        }
        any_active
    }

    /// Aplikovat aktualni values na LayerNode strom. Walk recursive,
    /// pri match layer.id == anim node_id prepis layer.opacity / transform.
    pub fn apply_to_layer_tree(&self, root: &mut LayerNode) {
        if let Some(list) = self.entries.get(&root.id) {
            for anim in list {
                match anim {
                    CompositorAnim::Opacity { current, .. } => {
                        root.opacity = *current;
                    }
                    CompositorAnim::Transform { current, .. } => {
                        root.transform = Some(current.clone());
                        // Compose pouziva PLURAL chain (layer.transforms) -
                        // bez sync by gate fast path kreslil stale/prazdny
                        // chain a anim stala (single-op anim = vec o 1).
                        root.transforms = vec![current.clone()];
                    }
                }
            }
        }
        for child in root.children.iter_mut() {
            self.apply_to_layer_tree(child);
        }
    }

    /// Vraci aktualni opacity pro node (Some), pokud anim aktivni.
    pub fn opacity_for(&self, node_id: usize) -> Option<f32> {
        self.entries.get(&node_id)?.iter().find_map(|a| {
            if let CompositorAnim::Opacity { current, .. } = a { Some(*current) }
            else { None }
        })
    }

    /// Vraci aktualni transform pro node (Some), pokud anim aktivni.
    pub fn transform_for(&self, node_id: usize) -> Option<TransformOp> {
        self.entries.get(&node_id)?.iter().find_map(|a| {
            if let CompositorAnim::Transform { current, .. } = a { Some(current.clone()) }
            else { None }
        })
    }
}

fn is_done(a: &CompositorAnim) -> bool {
    match a {
        CompositorAnim::Opacity { done, .. } => *done,
        CompositorAnim::Transform { done, .. } => *done,
    }
}

fn tick_anim(anim: &mut CompositorAnim, now: Instant) -> bool {
    match anim {
        CompositorAnim::Opacity { from, to, start, duration_ms, easing, iterations, alternate, current, done } => {
            if *done { return false; }
            let elapsed = now.saturating_duration_since(*start).as_secs_f32() * 1000.0;
            let raw_iter = elapsed / duration_ms.max(0.001);
            if raw_iter >= *iterations {
                // Done - apply final value.
                let final_iter = (*iterations).floor() as i32;
                let direction_reverse = *alternate && (final_iter % 2 == 1);
                *current = if direction_reverse { *from } else { *to };
                *done = true;
                return false;
            }
            let iter_idx = raw_iter.floor() as i32;
            let local_t = raw_iter - iter_idx as f32;
            let reverse = *alternate && (iter_idx % 2 == 1);
            let t = if reverse { 1.0 - local_t } else { local_t };
            let eased = easing.apply(t);
            *current = *from + (*to - *from) * eased;
            true
        }
        CompositorAnim::Transform { from, to, start, duration_ms, easing, iterations, alternate, current, done } => {
            if *done { return false; }
            let elapsed = now.saturating_duration_since(*start).as_secs_f32() * 1000.0;
            let raw_iter = elapsed / duration_ms.max(0.001);
            if raw_iter >= *iterations {
                let final_iter = (*iterations).floor() as i32;
                let direction_reverse = *alternate && (final_iter % 2 == 1);
                *current = if direction_reverse { from.clone() } else { to.clone() };
                *done = true;
                return false;
            }
            let iter_idx = raw_iter.floor() as i32;
            let local_t = raw_iter - iter_idx as f32;
            let reverse = *alternate && (iter_idx % 2 == 1);
            let t = if reverse { 1.0 - local_t } else { local_t };
            let eased = easing.apply(t);
            *current = lerp_transform(from, to, eased);
            true
        }
    }
}

/// Linearni lerp dvou TransformOp - vyzaduje stejnou variantu.
/// Pri non-matching variants vraci `to` (no-op).
fn lerp_transform(from: &TransformOp, to: &TransformOp, t: f32) -> TransformOp {
    use TransformOp::*;
    match (from, to) {
        (Translate(fx, fy), Translate(tx, ty)) => {
            Translate(fx + (tx - fx) * t, fy + (ty - fy) * t)
        }
        (Scale(fx, fy), Scale(tx, ty)) => {
            Scale(fx + (tx - fx) * t, fy + (ty - fy) * t)
        }
        // translateX(100%) -> translateX(-100%) (marquee): % i px slozky lerp.
        (TranslateMixed { x_px: fxp, x_pct: fxc, y_px: fyp, y_pct: fyc },
         TranslateMixed { x_px: txp, x_pct: txc, y_px: typ, y_pct: tyc }) => TranslateMixed {
            x_px: fxp + (txp - fxp) * t,
            x_pct: fxc + (txc - fxc) * t,
            y_px: fyp + (typ - fyp) * t,
            y_pct: fyc + (tyc - fyc) * t,
        },
        (Rotate(fr), Rotate(tr)) => Rotate(fr + (tr - fr) * t),
        (Translate3D { x: fx, y: fy, z: fz },
         Translate3D { x: tx, y: ty, z: tz }) => Translate3D {
            x: fx + (tx - fx) * t,
            y: fy + (ty - fy) * t,
            z: fz + (tz - fz) * t,
        },
        (Scale3D { x: fx, y: fy, z: fz },
         Scale3D { x: tx, y: ty, z: tz }) => Scale3D {
            x: fx + (tx - fx) * t,
            y: fy + (ty - fy) * t,
            z: fz + (tz - fz) * t,
        },
        (Rotate3D { x: fx, y: fy, z: fz, angle_rad: fa },
         Rotate3D { x: tx, y: ty, z: tz, angle_rad: ta }) => Rotate3D {
            // Axis interpolation simple-naive (full spec by mel normalize).
            x: fx + (tx - fx) * t,
            y: fy + (ty - fy) * t,
            z: fz + (tz - fz) * t,
            angle_rad: fa + (ta - fa) * t,
        },
        (Perspective(fd), Perspective(td)) => Perspective(fd + (td - fd) * t),
        (Matrix3D(fm), Matrix3D(tm)) => {
            let mut out = [0.0f32; 16];
            for i in 0..16 { out[i] = fm[i] + (tm[i] - fm[i]) * t; }
            Matrix3D(out)
        }
        _ => to.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_lerp_halfway() {
        let mut store = CompositorAnimStore::new();
        let start = Instant::now();
        store.insert(123, CompositorAnim::Opacity {
            from: 0.0, to: 1.0,
            start,
            duration_ms: 100.0,
            easing: Easing::Linear,
            iterations: 1.0,
            alternate: false,
            current: 0.0,
            done: false,
        });
        // Tick at 50ms.
        let now = start + std::time::Duration::from_millis(50);
        let active = store.tick(now);
        assert!(active);
        let cur = store.opacity_for(123).unwrap();
        // Tolerance pres sub-ms casovani.
        assert!((cur - 0.5).abs() < 0.05, "expect ~0.5, got {}", cur);
    }

    #[test]
    fn opacity_done_after_duration() {
        let mut store = CompositorAnimStore::new();
        let start = Instant::now();
        store.insert(1, CompositorAnim::Opacity {
            from: 0.0, to: 1.0,
            start, duration_ms: 50.0,
            easing: Easing::Linear,
            iterations: 1.0, alternate: false,
            current: 0.0, done: false,
        });
        let now = start + std::time::Duration::from_millis(100);
        let _ = store.tick(now);
        // Po done = entry odstraneno (cleanup).
        assert_eq!(store.active_count(), 0);
    }

    #[test]
    fn transform_translate_lerp() {
        let mut store = CompositorAnimStore::new();
        let start = Instant::now();
        store.insert(7, CompositorAnim::Transform {
            from: TransformOp::Translate(0.0, 0.0),
            to: TransformOp::Translate(100.0, 50.0),
            start, duration_ms: 100.0,
            easing: Easing::Linear,
            iterations: 1.0, alternate: false,
            current: TransformOp::Translate(0.0, 0.0),
            done: false,
        });
        let now = start + std::time::Duration::from_millis(50);
        store.tick(now);
        let tr = store.transform_for(7).unwrap();
        if let TransformOp::Translate(x, y) = tr {
            assert!((x - 50.0).abs() < 5.0);
            assert!((y - 25.0).abs() < 5.0);
        } else {
            panic!("expect Translate, got {:?}", tr);
        }
    }

    #[test]
    fn apply_to_layer_tree_updates_opacity() {
        let mut root = LayerNode {
            id: 42,
            root_rect: crate::browser::layout::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            z_index: None,
            opacity: 1.0,
            blend_mode: crate::browser::computed_style::BlendMode::Normal,
            transform: None,
            transforms: Vec::new(),
            reason: super::super::LayerReason::Root,
            children: Vec::new(),
            content_box_ids: Vec::new(),
            fingerprint: 0,
            structural_fp: 0,
            damage_rect: None,
            clip_rect: None,
            tiles: Vec::new(),
        };
        let mut store = CompositorAnimStore::new();
        store.insert(42, CompositorAnim::Opacity {
            from: 1.0, to: 0.0,
            start: Instant::now(),
            duration_ms: 100.0,
            easing: Easing::Linear,
            iterations: 1.0, alternate: false,
            current: 0.3,
            done: false,
        });
        store.apply_to_layer_tree(&mut root);
        assert!((root.opacity - 0.3).abs() < 1e-6);
    }

    #[test]
    fn alternate_iteration_reverses() {
        let mut store = CompositorAnimStore::new();
        let start = Instant::now();
        store.insert(9, CompositorAnim::Opacity {
            from: 0.0, to: 1.0,
            start, duration_ms: 100.0,
            easing: Easing::Linear,
            iterations: 4.0, alternate: true,
            current: 0.0, done: false,
        });
        // Druha iterace = reverse = at 150ms (mid iter 1) -> t local=0.5,
        // reversed = 0.5 -> value = 1.0 + (0-1)*0.5 = 0.5? actually
        // from=0,to=1 + reversed t = 1-0.5 = 0.5 -> 0 + (1-0)*0.5 = 0.5.
        let now = start + std::time::Duration::from_millis(150);
        store.tick(now);
        let cur = store.opacity_for(9).unwrap();
        // Pri 150ms iter_idx=1 (raw=1.5), local=0.5, reverse=true,
        // t = 1-0.5=0.5, lerp 0+1*0.5 = 0.5.
        assert!((cur - 0.5).abs() < 0.05, "got {}", cur);
    }

    #[test]
    fn remove_clears_node() {
        let mut store = CompositorAnimStore::new();
        store.insert(5, CompositorAnim::Opacity {
            from: 0.0, to: 1.0, start: Instant::now(),
            duration_ms: 100.0, easing: Easing::Linear,
            iterations: 1.0, alternate: false,
            current: 0.0, done: false,
        });
        assert_eq!(store.active_count(), 1);
        store.remove(5);
        assert_eq!(store.active_count(), 0);
    }
}
