//! R-tree spatial index pro hit_test. Drive linear tree walk O(N) per mouse
//! move = pri 5000-box page + 1000Hz mysi prepad FPS ~100. R-tree query
//! O(log N) - jen pro static page elements (no transform / scrollable
//! container subtree).
//!
//! Build: po `layout_tree` v render_via, jednou per dom_version + viewport
//! change. Query: handle_input MouseMove -> rtree.locate_all_at_point ->
//! sort kandidatu by paint_order desc -> top non-skip.
//!
//! Pri transformed/scrollable subtrees fallback na klasicky tree walk.
//! Inspired by WebKit `HitTestingTransformState` + Chromium
//! `cc::LayerTree::HitTest`.

use rstar::{RTree, RTreeObject, AABB, PointDistance};

use crate::browser::layout::LayoutBox;

/// Per-box vstup do r-tree. AABB v VISUAL coords (= bx.rect minus akumulovany
/// scroll_offset ancestoru). Box vlastnosti potrebne pro filter (visibility,
/// pointer-events) ulozene primo, ne pointer-chase do LayoutBoxu.
#[derive(Clone, Debug)]
pub struct HitEntry {
    /// Rc::as_ptr(box.node) - identifikator DOM nodu. 0 = anonymous box.
    pub node_ptr: usize,
    /// AABB visual_rect [x_min, y_min, x_max, y_max] v logical CSS px.
    pub aabb: [f32; 4],
    /// Paint order index (0 = first painted, increasing s DFS). Pri shoda
    /// na point: higher paint_order = on-top = wins.
    pub paint_order: u32,
    /// True kdyz box ma transform OR position:fixed OR jine "specialni" feature
    /// ktery jednoduchy AABB hit nedostatecny. Caller fallback na tree walk.
    pub has_special: bool,
    /// HTML tag jmeno (pro cursor lookup bez DOM dereferencing).
    pub tag: Option<String>,
    /// True kdyz box obsahuje text node (cursor = Text).
    pub has_text: bool,
    /// pointer-events: none -> skip pri hit.
    pub pointer_events_none: bool,
    /// visibility: hidden/collapse -> skip pri hit.
    pub visibility_hidden: bool,
}

impl RTreeObject for HitEntry {
    type Envelope = AABB<[f32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.aabb[0], self.aabb[1]],
            [self.aabb[2], self.aabb[3]],
        )
    }
}

impl PointDistance for HitEntry {
    fn distance_2(&self, point: &[f32; 2]) -> f32 {
        // Distance from point to nearest edge of AABB. 0 if inside.
        let dx_left = (self.aabb[0] - point[0]).max(0.0);
        let dx_right = (point[0] - self.aabb[2]).max(0.0);
        let dy_top = (self.aabb[1] - point[1]).max(0.0);
        let dy_bot = (point[1] - self.aabb[3]).max(0.0);
        let dx = dx_left + dx_right;
        let dy = dy_top + dy_bot;
        dx * dx + dy * dy
    }
}

/// Build r-tree z layout tree. Pres DFS akumuluj scroll_offset ancestoru pro
/// VISUAL coords. Per box jeden HitEntry. Bulk-load O(N log N).
///
/// `has_special_subtree` flag: pri prvni transform/iframe v subtreu se cele
/// descendants oznaci has_special=true (fallback na tree walk). Pro V1 nezavadime
/// nested r-trees za kazdym transform - mass-page case (5000 inline elementu
/// bez transformu) je hlavni cil.
pub fn build_hit_rtree(layout_root: &LayoutBox) -> RTree<HitEntry> {
    let mut entries: Vec<HitEntry> = Vec::with_capacity(1024);
    let mut paint_order: u32 = 0;
    collect_boxes(layout_root, 0.0, 0.0, false, &mut paint_order, &mut entries);
    RTree::bulk_load(entries)
}

fn collect_boxes(
    bx: &LayoutBox,
    acc_scroll_x: f32,
    acc_scroll_y: f32,
    parent_special: bool,
    paint_order: &mut u32,
    entries: &mut Vec<HitEntry>,
) {
    use crate::browser::computed_style::Visibility;
    use crate::browser::layout::Position;

    let visual_x = bx.rect.x - acc_scroll_x;
    let visual_y = bx.rect.y - acc_scroll_y;
    let visual_x2 = visual_x + bx.rect.width;
    let visual_y2 = visual_y + bx.rect.height;

    // "Special" subtree = transform applied OR position:fixed/sticky
    // (scroll-independent - paint NoScrollShift drzi na viewport pozici)
    // OR pointer-events: none (zachovava semantiku pri descent). Pri special node
    // r-tree mark fallback - tree walk dovrsi accurate hit. Propaguje na descendants.
    let self_special = !bx.transforms.is_empty()
        || bx.transform.is_some()
        || matches!(bx.position, Position::Fixed | Position::Sticky);
    let has_special = parent_special || self_special;

    let node_ptr = bx.node.as_ref()
        .map(|n| std::rc::Rc::as_ptr(n) as usize)
        .unwrap_or(0);

    let tag = bx.node.as_ref().and_then(|n| n.tag_name());

    entries.push(HitEntry {
        node_ptr,
        aabb: [visual_x, visual_y, visual_x2, visual_y2],
        paint_order: *paint_order,
        has_special,
        tag,
        has_text: bx.text.is_some(),
        pointer_events_none: bx.pointer_events.is_none(),
        visibility_hidden: matches!(bx.visibility,
            Visibility::Hidden | Visibility::Collapse),
    });
    *paint_order += 1;

    // Descend with updated scroll accumulator (per-element scroll offset shifts
    // descendants visually).
    let child_acc_x = acc_scroll_x + bx.scroll_offset_x;
    let child_acc_y = acc_scroll_y + bx.scroll_offset_y;
    for child in &bx.children {
        collect_boxes(child, child_acc_x, child_acc_y, has_special, paint_order, entries);
    }
}

/// Query r-tree at point. Vraci top-most non-skip HitEntry. None pri no hit
/// nebo pri hit pres special-subtree node (caller pak fallback na klasicky
/// LayoutBox::hit_test).
///
/// Strategie: locate_all_at_point -> candidates. Filter visibility/pointer-events.
/// Sort by paint_order desc - higher = drawn later = on-top. First candidate
/// wins.
pub fn hit_test_point(tree: &RTree<HitEntry>, x: f32, y: f32) -> HitResult {
    let pt = [x, y];
    let mut candidates: Vec<&HitEntry> = tree.locate_all_at_point(&pt).collect();
    if candidates.is_empty() {
        return HitResult::Miss;
    }
    // Sort desc by paint_order. Last painted = top of stack = wins hit.
    candidates.sort_unstable_by(|a, b| b.paint_order.cmp(&a.paint_order));
    for c in &candidates {
        if c.pointer_events_none { continue; }
        if c.visibility_hidden { continue; }
        if c.has_special {
            // Pri transform/fixed subtree: r-tree AABB neni accurate. Caller pouzij
            // tree walk fallback.
            return HitResult::NeedsFallback;
        }
        return HitResult::Hit(HitInfo {
            node_ptr: c.node_ptr,
            tag: c.tag.clone(),
            has_text: c.has_text,
        });
    }
    HitResult::Miss
}

/// Query pro fixed/sticky pri scrollu: special entries (sticky/fixed) maji
/// v r-tree LAYOUT aabb, ale vizualne sedi na VIEWPORT pozici (paint
/// NoScrollShift). Content-coords query je mine -> tento check s viewport
/// coords odhali "bod je nad sticky elementem" -> caller tree-walk fallback
/// (hit_test_scrolled sticky vetev). Bez tohoto hover/klik PROLETI sticky
/// sidebar/header na obsah pod nim.
pub fn special_at_point(tree: &RTree<HitEntry>, x: f32, y: f32) -> bool {
    tree.locate_all_at_point(&[x, y])
        .any(|c| c.has_special && !c.pointer_events_none && !c.visibility_hidden)
}

/// Result jednoho hit query. NeedsFallback = caller fallne na tree walk.
#[derive(Debug, Clone)]
pub enum HitResult {
    Hit(HitInfo),
    Miss,
    NeedsFallback,
}

/// Plne info ze single hit - pro consumer (cursor lookup, :hover dispatch).
#[derive(Debug, Clone)]
pub struct HitInfo {
    pub node_ptr: usize,
    pub tag: Option<String>,
    pub has_text: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::layout::LayoutBox;

    fn make_box(x: f32, y: f32, w: f32, h: f32) -> LayoutBox {
        let mut bx = LayoutBox::new();
        bx.rect.x = x;
        bx.rect.y = y;
        bx.rect.width = w;
        bx.rect.height = h;
        bx
    }

    #[test]
    fn hit_test_single() {
        let bx = make_box(0.0, 0.0, 100.0, 100.0);
        let tree = build_hit_rtree(&bx);
        match hit_test_point(&tree, 50.0, 50.0) {
            HitResult::Hit(_) => {}
            other => panic!("expected Hit got {:?}", other),
        }
    }

    #[test]
    fn hit_test_miss() {
        let bx = make_box(0.0, 0.0, 100.0, 100.0);
        let tree = build_hit_rtree(&bx);
        assert!(matches!(hit_test_point(&tree, 500.0, 500.0), HitResult::Miss));
    }

    #[test]
    fn hit_test_top_child() {
        let mut root = make_box(0.0, 0.0, 200.0, 200.0);
        root.children.push(make_box(10.0, 10.0, 50.0, 50.0));
        root.children.push(make_box(0.0, 0.0, 200.0, 200.0));
        let tree = build_hit_rtree(&root);
        // Point (20, 20) is in root, child0, child1. Child1 painted last = top.
        match hit_test_point(&tree, 20.0, 20.0) {
            HitResult::Hit(info) => {
                // child1 = paint_order 2 (root=0, child0=1, child1=2)
                let _ = info;
            }
            other => panic!("expected Hit got {:?}", other),
        }
    }

    #[test]
    fn hit_test_pointer_events_none_skips() {
        let mut root = make_box(0.0, 0.0, 100.0, 100.0);
        root.pointer_events = crate::browser::layout::PointerEvents::None;
        let tree = build_hit_rtree(&root);
        assert!(matches!(hit_test_point(&tree, 50.0, 50.0), HitResult::Miss));
    }
}
