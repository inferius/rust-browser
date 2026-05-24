//! Layer compositor (L1-L5 plan).
//!
//! Chrome/Firefox model: dokument != flat tree. Pri rendrovani je rozdelen
//! do LAYERS. Kazda layer = vlastni offscreen bitmap. Compositor pak
//! mixuje layers do final framu pres GPU pass s transform/opacity/blend
//! per layer.
//!
//! Win: pri zmene jedne layer (hover na button v devtools) jen ji repaint.
//! Stable layers (toolbar, side panel) reuse cached texture. Compositor
//! pass je cheap (jen quads s texture sample, par ms na GPU).
//!
//! L1 (tento modul): layer detection z LayoutBox.
//! L2: per-layer wgpu::Texture allocator (TODO).
//! L3: compositor shader + present pass (TODO).
//! L4: composite-only animations (transform/opacity = jen uniform update) (TODO).
//! L5: dirty rect tracking pro partial repaint (TODO).
//!
//! ## Layer boundary kriteria (per CSS Stacking Context spec)
//!
//! Element vytvori novou layer kdyz:
//! - `position: fixed` nebo `position: sticky`
//! - `z-index != auto` (na positioned element)
//! - `opacity < 1`
//! - `transform != none`
//! - `filter != none`
//! - `will-change: transform | opacity` (hint)
//! - `isolation: isolate`
//! - `mix-blend-mode != normal`
//! - `clip-path != none`
//!
//! Vsechny child elementy bez layer boundary patri do parent layer.

pub mod thread;
pub mod anim;

use super::layout::{LayoutBox, Position, Rect};

/// Tile = sub-layer caching unit (WebRender pattern). Per-layer rozdelena na
/// grid 256x256 tiles. Damage detection per tile - pri zmene jen 1 tile re-paint,
/// ostatni reuse. Pri composite blit per tile s vlastni position.
///
/// Inspired by WebRender `tile_cache.rs::Tile`. Bez tile granularity by celý
/// layer texture musel byt re-painted pri jakekoliv zmene v jeho subtree.
#[derive(Debug, Clone)]
pub struct Tile {
    /// Tile rect v layer-local coords (origin layer top-left).
    pub local_rect: Rect,
    /// Fingerprint content boxes uvnitr tohoto tile rect.
    pub fingerprint: u64,
    /// Dirty flag - true kdyz fingerprint diff vs prev frame.
    pub dirty: bool,
}

pub const TILE_SIZE: f32 = 1024.0;


/// Layer = offscreen "vrstva" content. Maps na CSS Stacking Context.
#[derive(Debug, Clone)]
pub struct LayerNode {
    /// Stable ID pres frames - root box node ptr (Rc::as_ptr usize). 0 pro
    /// root layer (no associated node). Pouziti: HashMap klic pro texture
    /// cache (host alokuje wgpu::Texture per layer_id, pri zmene size
    /// realokuje, jinak reuse mezi frames).
    pub id: usize,
    /// LayoutBox ktery layer "owns" (root teto vrstvy).
    pub root_rect: super::layout::Rect,
    /// Z-index pro sort order (None = auto = treat jako 0).
    pub z_index: Option<i32>,
    /// Compositing properties - applied at compositor pass:
    pub opacity: f32,
    pub transform: Option<super::layout::TransformOp>,
    /// Reason proc je layer (debug + selectivni invalidation).
    pub reason: LayerReason,
    /// Child layers. Sortovany pres z_index pri compositor pass.
    pub children: Vec<LayerNode>,
    /// Boxes content - LayoutBox patrici do TETO vrstvy (NE descendant layers).
    /// Pri repaint teto vrstvy emit jen tyhle boxy.
    /// Box ids jako (node_ptr usize) pro lookup v layout tree.
    pub content_box_ids: Vec<usize>,
    /// Fingerprint sub-tree obsah (style + rect hash). Pri match s prev frame
    /// = no damage = mozne reuse cached texture.
    /// Inspired by Chromium cc/trees/damage_tracker.cc::UpdateDamage.
    pub fingerprint: u64,
    /// Structural fingerprint - bez transform / opacity hash. Pri zmene jen
    /// transform/opacity (compositor-only anim) structural_fp ZUSTANE stejny.
    /// Damage rect je flagged jen pri structural change (real paint potreba).
    /// Inspired by WebRender Picture cache invalidation policy.
    pub structural_fp: u64,
    /// Dirty area - bbox toho co se zmenilo od posledniho framu. None = no
    /// damage (cached texture reuse). Some(rect) = re-paint potreba.
    pub damage_rect: Option<super::layout::Rect>,
    /// Tile grid - sub-layer cache units. Damage tracked per tile pres
    /// fingerprint - bez tile granularity by celý layer musel byt re-painted.
    /// Inspired by WebRender Picture cache tile model.
    pub tiles: Vec<Tile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerReason {
    /// Root document layer (always).
    Root,
    /// `position: fixed` - layer follows viewport, ne scroll.
    PositionFixed,
    /// `position: sticky` - hybrid scroll-anchor.
    PositionSticky,
    /// `z-index` set na positioned element.
    ZIndex,
    /// `opacity < 1` - blend pres parent.
    Opacity,
    /// `transform != none` - GPU compositor target.
    Transform,
    /// `will-change` hint - lazy layer.
    WillChange,
}

/// Build LayerTree z layout_root. Walk LayoutBox tree, identify layer
/// boundaries pres CSS Stacking Context kriteria.
pub fn extract_layer_tree(layout_root: &LayoutBox) -> LayerNode {
    let root_id = layout_root.node.as_ref()
        .map(|n| std::rc::Rc::as_ptr(n) as usize).unwrap_or(0);
    let mut root = LayerNode {
        id: root_id,
        root_rect: layout_root.rect,
        z_index: None,
        opacity: 1.0,
        transform: None,
        reason: LayerReason::Root,
        children: Vec::new(),
        content_box_ids: Vec::new(),
        fingerprint: 0,
        structural_fp: 0,
        damage_rect: None,
        tiles: Vec::new(),
    };
    walk_box(layout_root, &mut root);
    root.children.sort_by_key(|l| l.z_index.unwrap_or(0));
    // Spocti fingerprint kazdy layer pres post-walk (children jiz finalized).
    compute_fingerprints(&mut root, layout_root);
    root
}

/// Vypocti fingerprint kazdy layer = hash content boxes' rect + opacity +
/// transform + child layer fingerprints. Stable mezi frames pokud nic se nemenilo.
fn compute_fingerprints(layer: &mut LayerNode, layout_root: &LayoutBox) {
    // Build node_id -> LayoutBox lookup (1x pres root).
    fn collect_boxes<'a>(
        bx: &'a LayoutBox,
        out: &mut std::collections::HashMap<usize, &'a LayoutBox>,
    ) {
        if let Some(n) = &bx.node {
            out.insert(std::rc::Rc::as_ptr(n) as usize, bx);
        }
        for ch in &bx.children { collect_boxes(ch, out); }
    }
    let mut box_map: std::collections::HashMap<usize, &LayoutBox> =
        std::collections::HashMap::new();
    collect_boxes(layout_root, &mut box_map);
    compute_fingerprints_inner(layer, &box_map);
}

fn compute_fingerprints_inner(
    layer: &mut LayerNode,
    box_map: &std::collections::HashMap<usize, &LayoutBox>,
) {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    // Recurse first (children fingerprints prispivaji do parent hash).
    for child in &mut layer.children {
        compute_fingerprints_inner(child, box_map);
    }
    let mut h_full = DefaultHasher::new();
    let mut h_struct = DefaultHasher::new();
    // Layer's own props.
    layer.id.hash(&mut h_full);
    layer.id.hash(&mut h_struct);
    (layer.root_rect.x.to_bits()).hash(&mut h_full);
    (layer.root_rect.x.to_bits()).hash(&mut h_struct);
    (layer.root_rect.y.to_bits()).hash(&mut h_full);
    (layer.root_rect.y.to_bits()).hash(&mut h_struct);
    (layer.root_rect.width.to_bits()).hash(&mut h_full);
    (layer.root_rect.width.to_bits()).hash(&mut h_struct);
    (layer.root_rect.height.to_bits()).hash(&mut h_full);
    (layer.root_rect.height.to_bits()).hash(&mut h_struct);
    layer.z_index.unwrap_or(0).hash(&mut h_full);
    layer.z_index.unwrap_or(0).hash(&mut h_struct);
    // opacity + transform: only into full hash (compositor-only props).
    layer.opacity.to_bits().hash(&mut h_full);
    // Content boxes rect + key style props.
    for id in &layer.content_box_ids {
        if let Some(bx) = box_map.get(id) {
            id.hash(&mut h_full);
            id.hash(&mut h_struct);
            bx.rect.x.to_bits().hash(&mut h_full);
            bx.rect.x.to_bits().hash(&mut h_struct);
            bx.rect.y.to_bits().hash(&mut h_full);
            bx.rect.y.to_bits().hash(&mut h_struct);
            bx.rect.width.to_bits().hash(&mut h_full);
            bx.rect.width.to_bits().hash(&mut h_struct);
            bx.rect.height.to_bits().hash(&mut h_full);
            bx.rect.height.to_bits().hash(&mut h_struct);
            // opacity: full only (compositor-friendly).
            bx.opacity.to_bits().hash(&mut h_full);
            if let Some(c) = bx.bg_color { c.hash(&mut h_full); c.hash(&mut h_struct); }
            if let Some(c) = bx.text_color { c.hash(&mut h_full); c.hash(&mut h_struct); }
            if let Some(t) = &bx.text { t.hash(&mut h_full); t.hash(&mut h_struct); }
        }
    }
    // Children layer fingerprints (rekursivni signal).
    for child in &layer.children {
        child.fingerprint.hash(&mut h_full);
        child.structural_fp.hash(&mut h_struct);
    }
    layer.fingerprint = h_full.finish();
    layer.structural_fp = h_struct.finish();
    // Build tile grid + per-tile fingerprint.
    compute_layer_tiles(layer, box_map);
}

/// Rozdeli layer.root_rect na NxM grid tiles velikost TILE_SIZE. Per tile
/// vypocti fingerprint z content boxes ktere intersect tile rect. Foundation
/// pro tile-level damage tracking (per-tile re-paint misto cely layer).
fn compute_layer_tiles(
    layer: &mut LayerNode,
    box_map: &std::collections::HashMap<usize, &LayoutBox>,
) {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let lr = layer.root_rect;
    if lr.width < 1.0 || lr.height < 1.0 {
        layer.tiles.clear();
        return;
    }
    let cols = ((lr.width / TILE_SIZE).ceil() as usize).max(1);
    let rows = ((lr.height / TILE_SIZE).ceil() as usize).max(1);
    let mut tiles = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let tx = lr.x + col as f32 * TILE_SIZE;
            let ty = lr.y + row as f32 * TILE_SIZE;
            let tw = TILE_SIZE.min(lr.x + lr.width - tx);
            let th = TILE_SIZE.min(lr.y + lr.height - ty);
            let tile_rect = Rect { x: tx, y: ty, width: tw, height: th };
            // FP per tile - content boxes intersecting tile.
            let mut h = DefaultHasher::new();
            (tile_rect.x.to_bits()).hash(&mut h);
            (tile_rect.y.to_bits()).hash(&mut h);
            (tile_rect.width.to_bits()).hash(&mut h);
            (tile_rect.height.to_bits()).hash(&mut h);
            for id in &layer.content_box_ids {
                if let Some(bx) = box_map.get(id) {
                    // Bx rect intersect tile?
                    let bx_x0 = bx.rect.x;
                    let bx_y0 = bx.rect.y;
                    let bx_x1 = bx.rect.x + bx.rect.width;
                    let bx_y1 = bx.rect.y + bx.rect.height;
                    let ix = bx_x1 > tile_rect.x && bx_x0 < tile_rect.x + tile_rect.width
                          && bx_y1 > tile_rect.y && bx_y0 < tile_rect.y + tile_rect.height;
                    if !ix { continue; }
                    id.hash(&mut h);
                    bx.rect.x.to_bits().hash(&mut h);
                    bx.rect.y.to_bits().hash(&mut h);
                    bx.rect.width.to_bits().hash(&mut h);
                    bx.rect.height.to_bits().hash(&mut h);
                    if let Some(c) = bx.bg_color { c.hash(&mut h); }
                    if let Some(c) = bx.text_color { c.hash(&mut h); }
                    if let Some(t) = &bx.text { t.hash(&mut h); }
                }
            }
            tiles.push(Tile {
                local_rect: Rect {
                    x: tile_rect.x - lr.x,
                    y: tile_rect.y - lr.y,
                    width: tile_rect.width,
                    height: tile_rect.height,
                },
                fingerprint: h.finish(),
                dirty: false, // mark v separate pass vs prev
            });
        }
    }
    layer.tiles = tiles;
}

/// Vraci true pokud element vytvori novou layer (stacking context boundary).
pub fn is_layer_boundary(b: &LayoutBox) -> bool {
    // position: fixed / sticky
    if matches!(b.position, Position::Fixed | Position::Sticky) {
        return true;
    }
    // z-index != auto on positioned element
    if b.z_index.is_some() && b.position != Position::Static {
        return true;
    }
    // opacity < 1
    if b.opacity < 1.0 {
        return true;
    }
    // transform != none
    if b.transform.is_some() {
        return true;
    }
    false
}

/// Layer reason klasifikator - debug + selectivni invalidation.
pub fn classify_layer_reason(b: &LayoutBox) -> LayerReason {
    if b.position == Position::Fixed {
        return LayerReason::PositionFixed;
    }
    if b.position == Position::Sticky {
        return LayerReason::PositionSticky;
    }
    if b.z_index.is_some() {
        return LayerReason::ZIndex;
    }
    if b.opacity < 1.0 {
        return LayerReason::Opacity;
    }
    if b.transform.is_some() {
        return LayerReason::Transform;
    }
    LayerReason::Root
}

/// Recursivne walks LayoutBox a buduje LayerTree.
/// Box patri do `current` layer pokud nesi nova hranice. Jinak vytvori child layer.
fn walk_box(b: &LayoutBox, current: &mut LayerNode) {
    // Aktualni box content - registruj do current layer.
    if let Some(node) = b.node.as_ref() {
        let id = std::rc::Rc::as_ptr(node) as usize;
        current.content_box_ids.push(id);
    }
    // Pro kazdeho childa: pokud je layer boundary -> novy sub-layer, walks tam.
    // Jinak pokracuje v current layer.
    for child in &b.children {
        if is_layer_boundary(child) {
            let layer_id = child.node.as_ref()
                .map(|n| std::rc::Rc::as_ptr(n) as usize).unwrap_or(0);
            // Layer.root_rect = orig rect (NE AABB-expanded). Compose pres
            // transform pipeline rotuje quad sized orig rect (80x60) pres orig
            // center pivot -> rotated quad shape v page = TIGHT rotated rect's
            // parallelogram = NO corner extension beyond rotated rect = NO bily
            // "padding" area mimo section bg. Border-radius pres SDF v src tex
            // = rounded corners visible v rotated direction (= correct chrome
            // behavior).
            let mut sub = LayerNode {
                id: layer_id,
                root_rect: child.rect,
                z_index: child.z_index,
                opacity: child.opacity,
                transform: child.transform.clone(),
                reason: classify_layer_reason(child),
                children: Vec::new(),
                content_box_ids: Vec::new(),
                fingerprint: 0,
                structural_fp: 0,
                damage_rect: None,
                tiles: Vec::new(),
            };
            walk_box(child, &mut sub);
            sub.children.sort_by_key(|l| l.z_index.unwrap_or(0));
            current.children.push(sub);
        } else {
            walk_box(child, current);
        }
    }
}

/// Spocita celkove pocet layer v tree (root + child layers recursive).
/// Diagnostika - kolik layer dokument vyrabi.
pub fn count_layers(root: &LayerNode) -> usize {
    let mut count = 1;
    for child in &root.children {
        count += count_layers(child);
    }
    count
}

/// Spocita celkove pocet boxes (content) v cele tree.
pub fn count_content_boxes(root: &LayerNode) -> usize {
    let mut count = root.content_box_ids.len();
    for child in &root.children {
        count += count_content_boxes(child);
    }
    count
}

/// Walk LayerTree + collect vsechny layer ids do HashSet. Pouziti:
/// WebView::gc_layer_textures - drop entries pres set membership check.
pub fn collect_layer_ids(root: &LayerNode, out: &mut std::collections::HashSet<usize>) {
    out.insert(root.id);
    for child in &root.children {
        collect_layer_ids(child, out);
    }
}

/// Walk LayerTree + flat list (root + all descendants). Pouziti pri render:
/// process kazdou layer separately.
pub fn flatten_layers<'a>(root: &'a LayerNode, out: &mut Vec<&'a LayerNode>) {
    out.push(root);
    for child in &root.children {
        flatten_layers(child, out);
    }
}

/// Mark damage_rect na layers ktere zmenily STRUCTURAL fingerprint vs prev
/// frame. Compositor-only props (opacity, transform) NEspusti damage = layer
/// texture reused, jen composite uniforms update.
/// Inspired by Chromium cc/trees/damage_tracker.cc + WebRender Picture cache.
pub fn mark_damage(
    layer: &mut LayerNode,
    prev_fps: &mut std::collections::HashMap<usize, u64>,
) {
    let prev = prev_fps.get(&layer.id).copied();
    let structural_changed = match prev {
        Some(p) => p != layer.structural_fp,
        None => true, // new layer = full damage
    };
    layer.damage_rect = if structural_changed { Some(layer.root_rect) } else { None };
    prev_fps.insert(layer.id, layer.structural_fp);
    for child in &mut layer.children {
        mark_damage(child, prev_fps);
    }
}

/// Spocita kolik layeru ma damage_rect = Some. Diagnostika pro damage tracking.
pub fn count_damaged_layers(root: &LayerNode) -> usize {
    let mut c = if root.damage_rect.is_some() { 1 } else { 0 };
    for child in &root.children { c += count_damaged_layers(child); }
    c
}

/// Mark tile.dirty pres porovnani s prev frame tile fingerprints.
/// prev_tile_fps: HashMap<(layer_id, tile_index), tile_fingerprint>.
pub fn mark_tile_damage(
    layer: &mut LayerNode,
    prev_tile_fps: &mut std::collections::HashMap<(usize, usize), u64>,
) {
    for (idx, tile) in layer.tiles.iter_mut().enumerate() {
        let key = (layer.id, idx);
        let prev = prev_tile_fps.get(&key).copied();
        tile.dirty = match prev {
            Some(p) => p != tile.fingerprint,
            None => true,
        };
        prev_tile_fps.insert(key, tile.fingerprint);
    }
    for child in &mut layer.children {
        mark_tile_damage(child, prev_tile_fps);
    }
}

/// Spocita tile damage statistiku: (dirty_tiles, total_tiles).
pub fn count_tile_damage(root: &LayerNode) -> (usize, usize) {
    let mut d = root.tiles.iter().filter(|t| t.dirty).count();
    let mut t = root.tiles.len();
    for child in &root.children {
        let (cd, ct) = count_tile_damage(child);
        d += cd; t += ct;
    }
    (d, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::layout::Position;

    // Pomocna LayoutBox factory pres Default::default - vyplni vsechny non-Option
    // fields. Pak override jen relevant fields.
    fn box_with(position: Position, z_index: Option<i32>, opacity: f32) -> crate::browser::layout::LayoutBox {
        // Layout_tree pres test fixture HTML by bylo cistejsi ale slozite.
        // Pouzit real HTML parsing + cascade pres minimal sample.
        let html = format!(
            "<html><body><div style=\"position:{}; opacity:{}; {}\">x</div></body></html>",
            match position {
                Position::Static => "static",
                Position::Relative => "relative",
                Position::Absolute => "absolute",
                Position::Fixed => "fixed",
                Position::Sticky => "sticky",
            },
            opacity,
            if let Some(z) = z_index { format!("z-index:{};", z) } else { String::new() }
        );
        let doc = crate::browser::html_parser::parse_html(&html, "about:blank");
        let sheets: Vec<crate::browser::css_parser::Stylesheet> = Vec::new();
        let style_map = std::rc::Rc::new(crate::browser::cascade::cascade(&doc.root, &sheets));
        let layout = crate::browser::layout::layout_tree(&doc.root, &style_map, 800.0, 600.0);
        // Najdi nejhlubsi div.
        fn find_div(b: &crate::browser::layout::LayoutBox) -> Option<&crate::browser::layout::LayoutBox> {
            if b.tag.as_deref() == Some("div") { return Some(b); }
            for c in &b.children {
                if let Some(f) = find_div(c) { return Some(f); }
            }
            None
        }
        find_div(&layout).cloned().expect("test fixture has div")
    }

    #[test]
    fn static_box_no_layer() {
        let b = box_with(Position::Static, None, 1.0);
        assert!(!is_layer_boundary(&b));
    }

    #[test]
    fn opacity_creates_layer() {
        let b = box_with(Position::Static, None, 0.5);
        assert!(is_layer_boundary(&b));
        assert_eq!(classify_layer_reason(&b), LayerReason::Opacity);
    }

    #[test]
    fn position_fixed_creates_layer() {
        let b = box_with(Position::Fixed, None, 1.0);
        assert!(is_layer_boundary(&b));
        assert_eq!(classify_layer_reason(&b), LayerReason::PositionFixed);
    }

    #[test]
    fn z_index_creates_layer_when_positioned() {
        let b = box_with(Position::Relative, Some(5), 1.0);
        assert!(is_layer_boundary(&b));
    }

    // ---------- Tile-based rasterization tests (priority 5) ----------

    fn build_layer(width: f32, height: f32) -> LayerNode {
        LayerNode {
            id: 1,
            root_rect: Rect { x: 0.0, y: 0.0, width, height },
            z_index: None,
            opacity: 1.0,
            transform: None,
            reason: LayerReason::Root,
            children: Vec::new(),
            content_box_ids: Vec::new(),
            fingerprint: 0,
            structural_fp: 0,
            damage_rect: None,
            tiles: Vec::new(),
        }
    }

    #[test]
    fn tile_grid_size_matches_layer() {
        // 1024x768 -> 4x3 tiles pri TILE_SIZE=256.
        let mut layer = build_layer(1024.0, 768.0);
        let map = std::collections::HashMap::new();
        compute_layer_tiles(&mut layer, &map);
        let expected = ((1024.0_f32 / TILE_SIZE).ceil() as usize)
                     * ((768.0_f32 / TILE_SIZE).ceil() as usize);
        assert_eq!(layer.tiles.len(), expected);
    }

    #[test]
    fn tile_first_frame_all_dirty() {
        let mut layer = build_layer(512.0, 256.0);
        let map = std::collections::HashMap::new();
        compute_layer_tiles(&mut layer, &map);
        let mut prev: std::collections::HashMap<(usize, usize), u64>
            = std::collections::HashMap::new();
        mark_tile_damage(&mut layer, &mut prev);
        for tile in &layer.tiles {
            assert!(tile.dirty, "first frame all dirty");
        }
        let (dirty, total) = count_tile_damage(&layer);
        assert_eq!(dirty, total);
    }

    #[test]
    fn tile_second_frame_no_damage_when_stable() {
        let mut prev: std::collections::HashMap<(usize, usize), u64>
            = std::collections::HashMap::new();
        let mut layer = build_layer(512.0, 256.0);
        let map = std::collections::HashMap::new();
        // Frame 1.
        compute_layer_tiles(&mut layer, &map);
        mark_tile_damage(&mut layer, &mut prev);
        // Frame 2 - same content.
        let mut layer2 = build_layer(512.0, 256.0);
        compute_layer_tiles(&mut layer2, &map);
        mark_tile_damage(&mut layer2, &mut prev);
        let (dirty, _) = count_tile_damage(&layer2);
        assert_eq!(dirty, 0, "stable content -> 0 dirty tiles");
    }

    #[test]
    fn tile_local_rect_origin_at_zero() {
        let mut layer = build_layer(512.0, 256.0);
        // Posun layer mimo origin - tile rect.x/y local origin musi byt 0.
        layer.root_rect.x = 100.0;
        layer.root_rect.y = 200.0;
        let map = std::collections::HashMap::new();
        compute_layer_tiles(&mut layer, &map);
        assert!(layer.tiles[0].local_rect.x.abs() < 0.01);
        assert!(layer.tiles[0].local_rect.y.abs() < 0.01);
    }
}
