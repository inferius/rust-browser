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

use super::layout::{LayoutBox, Position};

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
    };
    walk_box(layout_root, &mut root);
    // Sort children layers by z-index (None = 0).
    root.children.sort_by_key(|l| l.z_index.unwrap_or(0));
    root
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
            let mut sub = LayerNode {
                id: layer_id,
                root_rect: child.rect,
                z_index: child.z_index,
                opacity: child.opacity,
                transform: child.transform.clone(),
                reason: classify_layer_reason(child),
                children: Vec::new(),
                content_box_ids: Vec::new(),
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
}
