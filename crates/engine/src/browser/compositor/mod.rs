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
    /// CSS mix-blend-mode - aplikovan pri kompozici layeru pres backdrop
    /// (= akumulovany target). Normal = bezny alpha-over compose.
    pub blend_mode: super::computed_style::BlendMode,
    pub transform: Option<super::layout::TransformOp>,
    /// Plny transform chain (transform: A() B() C()). `transform` (singular) je
    /// jen prvni op pres parse_transform - rozbity pro multi-op chainy (napr.
    /// `rotateX(30deg) rotateY(30deg)` nebo `perspective(600px) rotateY(35deg)`,
    /// kde parse_transform vrati garbage). Outer compose pouziva TENTO Vec pres
    /// compute_transform_matrix(&transforms) = spravny 3D chain.
    pub transforms: Vec<super::layout::TransformOp>,
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
    /// Overflow clip od ANCESTOR boxu (world/layout coords, intersekce vsech
    /// overflow != visible predku). Compose vrstvy nastavi GPU scissor na tento
    /// rect (scroll-adjusted) - bez toho transformovany obsah (marquee translateX
    /// anim) utekl z overflow:hidden rodice (CPU clip bezi PRED transformem,
    /// compose transform PO nem). None = zadny clipping predek.
    pub clip_rect: Option<super::layout::Rect>,
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
    /// `mix-blend-mode != normal` - blend pres backdrop.
    MixBlend,
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
        blend_mode: super::computed_style::BlendMode::Normal,
        transform: None,
        transforms: Vec::new(),
        reason: LayerReason::Root,
        children: Vec::new(),
        content_box_ids: Vec::new(),
        fingerprint: 0,
        structural_fp: 0,
        damage_rect: None,
        clip_rect: None,
        tiles: Vec::new(),
    };
    let _p0 = std::time::Instant::now();
    walk_box(layout_root, &mut root, None);
    root.children.sort_by_key(|l| l.z_index.unwrap_or(0));
    let _p1 = std::time::Instant::now();
    // Spocti fingerprint kazdy layer pres post-walk (children jiz finalized).
    compute_fingerprints(&mut root, layout_root);
    let _p2 = std::time::Instant::now();
    if std::env::var("RWE_LAYPROF").is_ok() {
        eprintln!("[EXTPROF] walk_box={:.2} fingerprints+tiles={:.2}",
            _p1.duration_since(_p0).as_secs_f32()*1000.0,
            _p2.duration_since(_p1).as_secs_f32()*1000.0);
    }
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
    let _q0 = std::time::Instant::now();
    let mut box_map: std::collections::HashMap<usize, &LayoutBox> =
        std::collections::HashMap::new();
    collect_boxes(layout_root, &mut box_map);
    let _q1 = std::time::Instant::now();
    TILES_PROF_US.with(|c| c.set(0));
    compute_fingerprints_inner(layer, &box_map);
    let _q2 = std::time::Instant::now();
    if std::env::var("RWE_LAYPROF").is_ok() {
        let tiles_us = TILES_PROF_US.with(|c| c.get());
        eprintln!("[FPPROF] box_map_build={:.2} fp_only={:.2} tiles={:.2} boxes={}",
            _q1.duration_since(_q0).as_secs_f32()*1000.0,
            (_q2.duration_since(_q1).as_micros() as f32 - tiles_us as f32)/1000.0,
            tiles_us as f32/1000.0,
            box_map.len());
    }
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
    // VIEWPORT CULL: off-screen content boxy NEhashujeme (s bufferem). Bez tohoto
    // off-screen animace (napr. colorCycle bg v sekci mimo viewport) meni
    // structural_fp kazdy frame -> layer damaged -> full root re-paint (~9ms tree
    // walk) i kdyz neni videt. Browsery off-screen damage ignoruji dokud
    // nescrollujes. Scroll meni cull bounds -> box se "objevi" v fp -> damage ->
    // re-paint s aktualni hodnotou (spravne).
    let cull = crate::browser::paint::viewport_cull_bounds();
    const CULL_BUF: f32 = 600.0;
    for id in &layer.content_box_ids {
        if let Some(bx) = box_map.get(id) {
            if let Some((vt, vb)) = cull {
                let always = matches!(bx.position,
                    super::layout::Position::Fixed | super::layout::Position::Sticky);
                if !always {
                    let top = bx.rect.y;
                    let bot = bx.rect.y + bx.rect.height;
                    if bot < vt - CULL_BUF || top > vb + CULL_BUF { continue; }
                }
            }
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
            // border-color: paint property - musi byt v structural_fp jinak
            // border-color transition (napr. .tf-box:hover border-color) NEdamaguje
            // layer -> nedojde k re-paintu -> border se neanimuje ("zjevi se na
            // konci"). Plus border_width.
            if let Some(c) = bx.border_color { c.hash(&mut h_full); c.hash(&mut h_struct); }
            bx.border_width.to_bits().hash(&mut h_full);
            bx.border_width.to_bits().hash(&mut h_struct);
            if let Some(t) = &bx.text { t.hash(&mut h_full); t.hash(&mut h_struct); }
            // Form control obsah (input value / select option) - bez nej psani
            // do inputu nedamaguje layer -> stary obraz (placeholder) zustal.
            if let Some(t) = &bx.control_text { t.hash(&mut h_full); t.hash(&mut h_struct); }
            // Checked stav (checkbox/radio) - paint kresli checkmark/dot primo
            // z attru, bez hashe klik na checkbox nedamagoval layer.
            if bx.tag.as_deref() == Some("input") {
                if let Some(n) = &bx.node {
                    n.attr("checked").is_some().hash(&mut h_full);
                    n.attr("checked").is_some().hash(&mut h_struct);
                    // Range value - thumb pozice se kresli z value attru.
                    if let Some(v) = n.attr("value") { v.hash(&mut h_full); v.hash(&mut h_struct); }
                }
            }
            // Outline (:focus-visible ring) - paint prop mimo border. Bez
            // hashe focus zmena NEdamagovala layer/tile -> outline se kreslil
            // jen castecne (jen v nahodou-dirty tiles) a "rozprostrel se" az
            // po mouse-leave full repaintu.
            if let Some(c) = bx.outline_color { c.hash(&mut h_full); c.hash(&mut h_struct); }
            bx.outline_width.to_bits().hash(&mut h_full);
            bx.outline_width.to_bits().hash(&mut h_struct);
            // Box-shadow (hover/focus shadow zmeny).
            for &(sx, sy, sb, ss, sc, inset) in &bx.box_shadow {
                let mut hp = |v: u32| { v.hash(&mut h_full); v.hash(&mut h_struct); };
                hp(sx.to_bits()); hp(sy.to_bits()); hp(sb.to_bits()); hp(ss.to_bits());
                sc.hash(&mut h_full); sc.hash(&mut h_struct);
                inset.hash(&mut h_full); inset.hash(&mut h_struct);
            }
            // background-position (animated gradient posouva pozici per frame)
            // - meni OBSAH textury, takze patri do structural_fp (damage +
            // tile raster), ne jen full. Bez toho anim layer texture cache
            // drzela staticky obraz.
            for l in &bx.backgrounds {
                if l.gradient.is_some() {
                    let mut hp = |v: u32| { v.hash(&mut h_full); v.hash(&mut h_struct); };
                    match &l.position {
                        crate::browser::layout::BgPosition::Px(px, py) => {
                            hp(px.to_bits()); hp(py.to_bits());
                        }
                        crate::browser::layout::BgPosition::Pct(px, py) => {
                            hp(px.to_bits()); hp(py.to_bits());
                        }
                        crate::browser::layout::BgPosition::Mixed { x_px, x_pct, y_px, y_pct } => {
                            hp(x_px.map(|v| v.to_bits()).unwrap_or(u32::MAX));
                            hp(x_pct.map(|v| v.to_bits()).unwrap_or(u32::MAX));
                            hp(y_px.map(|v| v.to_bits()).unwrap_or(u32::MAX));
                            hp(y_pct.map(|v| v.to_bits()).unwrap_or(u32::MAX));
                        }
                    }
                }
            }
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
    let _t0 = std::time::Instant::now();
    compute_layer_tiles(layer, box_map);
    TILES_PROF_US.with(|c| c.set(c.get() + _t0.elapsed().as_micros() as u64));
}

thread_local! {
    static TILES_PROF_US: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

/// Spocita konzervativni paint-extent boxu (outline kresli VNE boxu +
/// box-shadow reach). Sdileno mezi tile assignment + (historicky) intersection
/// test. Vraci 0 pro boxy bez outline/shadow.
#[inline]
fn box_paint_ext(bx: &LayoutBox) -> f32 {
    let mut ext = if bx.outline_width > 0.0 {
        bx.outline_width + bx.outline_offset.max(0.0)
    } else { 0.0 };
    for &(sx, sy, sb, ss, _, inset) in &bx.box_shadow {
        if !inset {
            ext = ext.max(sx.abs().max(sy.abs()) + sb + ss);
        }
    }
    ext
}

/// Hash content boxu do tile hasheru. MUSI byt bit-identicke s historickym
/// inline blokem (jinak by se zmenily vsechny tile fingerprinty = false damage).
#[inline]
fn hash_box_into_tile<H: std::hash::Hasher>(h: &mut H, id: usize, bx: &LayoutBox) {
    use std::hash::Hash;
    id.hash(h);
    bx.rect.x.to_bits().hash(h);
    bx.rect.y.to_bits().hash(h);
    bx.rect.width.to_bits().hash(h);
    bx.rect.height.to_bits().hash(h);
    if let Some(c) = bx.bg_color { c.hash(h); }
    if let Some(c) = bx.text_color { c.hash(h); }
    // border-color/width: paint property - bez nej border-color
    // transition (.tf-box:hover) nezmeni tile fp -> tile se
    // neoznaci dirty -> NEprepaintuje -> border se "zjevi az na
    // konci" animace (layer structural_fp se menil ale tile ne).
    if let Some(c) = bx.border_color { c.hash(h); }
    bx.border_width.to_bits().hash(h);
    // Outline + box-shadow: focus ring / hover stin (viz layer fp).
    if let Some(c) = bx.outline_color { c.hash(h); }
    bx.outline_width.to_bits().hash(h);
    for &(sx, sy, sb, ss, sc, inset) in &bx.box_shadow {
        sx.to_bits().hash(h); sy.to_bits().hash(h);
        sb.to_bits().hash(h); ss.to_bits().hash(h);
        sc.hash(h); inset.hash(h);
    }
    if let Some(t) = &bx.text { t.hash(h); }
    // Form control obsah - stejny duvod jako v layer fp (psani
    // do inputu jinak nedirti tile -> stary obraz).
    if let Some(t) = &bx.control_text { t.hash(h); }
    // Checked + range value (checkbox/radio/range paint z attru).
    if bx.tag.as_deref() == Some("input") {
        if let Some(n) = &bx.node {
            n.attr("checked").is_some().hash(h);
            if let Some(v) = n.attr("value") { v.hash(h); }
        }
    }
}

/// Rozdeli layer.root_rect na NxM grid tiles velikost TILE_SIZE. Per tile
/// vypocti fingerprint z content boxes ktere intersect tile rect. Foundation
/// pro tile-level damage tracking (per-tile re-paint misto cely layer).
///
/// PERF: drive O(tiles * boxes) (per-tile loop pres VSECHNY content boxy +
/// intersection test). Root layer ~9000px = ~9 tile rows * 1079 boxu = ~9700
/// iteraci/frame (~0.5ms debug). Ted O(boxes): per box urci rozsah
/// [col0..col1]x[row0..row1] ktery protina, append do tech tiles' seznamu
/// (poradi pruchodu content_box_ids zachovano -> tile hash BIT-identicky).
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
    let ntiles = cols * rows;
    // Per-tile seznam (box_id, box ref) v poradi pruchodu content_box_ids.
    // Single tile (typicke pro male layery) = fast path bez per-box bucketingu.
    let mut buckets: Vec<Vec<(usize, &LayoutBox)>> = vec![Vec::new(); ntiles];
    for id in &layer.content_box_ids {
        if let Some(bx) = box_map.get(id) {
            let ext = box_paint_ext(bx);
            let bx_x0 = bx.rect.x - ext;
            let bx_y0 = bx.rect.y - ext;
            let bx_x1 = bx.rect.x + bx.rect.width + ext;
            let bx_y1 = bx.rect.y + bx.rect.height + ext;
            // Urci rozsah tile-bunek ktere box protina. Box mimo layer = skip.
            // col/row index z box pozice relativne k layer originu.
            if bx_x1 <= lr.x || bx_x0 >= lr.x + lr.width
               || bx_y1 <= lr.y || bx_y0 >= lr.y + lr.height { continue; }
            // Rozsah col/row bunek ktere box (vc. ext) muze protinat. floor pro
            // start, ceil pro konec - nadmnozina, presny `if ix` test nize
            // filtruje hranicni bunky (zachova puvodni > / < semantiku).
            let c0 = (((bx_x0 - lr.x) / TILE_SIZE).floor() as isize).max(0) as usize;
            let c1 = ((((bx_x1 - lr.x) / TILE_SIZE).ceil() as isize).max(0) as usize).min(cols);
            let r0 = (((bx_y0 - lr.y) / TILE_SIZE).floor() as isize).max(0) as usize;
            let r1 = ((((bx_y1 - lr.y) / TILE_SIZE).ceil() as isize).max(0) as usize).min(rows);
            for row in r0..r1 {
                for col in c0..c1 {
                    // Presny intersection test vs tile rect (stejny jako puvodni
                    // inline kod) - col/row rozsah je konzervativne sirsi.
                    let tx = lr.x + col as f32 * TILE_SIZE;
                    let ty = lr.y + row as f32 * TILE_SIZE;
                    let tw = TILE_SIZE.min(lr.x + lr.width - tx);
                    let th = TILE_SIZE.min(lr.y + lr.height - ty);
                    let ix = bx_x1 > tx && bx_x0 < tx + tw
                          && bx_y1 > ty && bx_y0 < ty + th;
                    if ix { buckets[row * cols + col].push((*id, bx)); }
                }
            }
        }
    }
    let mut tiles = Vec::with_capacity(ntiles);
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
            for (id, bx) in &buckets[row * cols + col] {
                hash_box_into_tile(&mut h, *id, bx);
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

thread_local! {
    /// Node ids s aktivni @keyframes animaci (z webview.active_animations). Set
    /// pred extract_layer_tree. Promote je na vlastni layer aby paint-animace
    /// (colorCycle bg/border = NE transform/opacity) NEdamagovaly ROOT layer.
    /// Bez tohoto: colorCycle v rootu -> root damaged -> paint_layer_into
    /// prekresli CELY root content (vsechny off-screen prvky) = 17ms/frame
    /// (35 FPS). S promote: jen maly colorCycle layer re-paint = root reuse.
    static FORCE_LAYER_NODES: std::cell::RefCell<std::collections::HashSet<usize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Set node ids ktere maji byt vzdy vlastni layer (animated paint props).
pub fn set_force_layer_nodes(ids: std::collections::HashSet<usize>) {
    FORCE_LAYER_NODES.with(|c| *c.borrow_mut() = ids);
}

fn is_force_layer_node(id: usize) -> bool {
    FORCE_LAYER_NODES.with(|c| c.borrow().contains(&id))
}

/// Vraci true pokud element vytvori novou layer (stacking context boundary).
pub fn is_layer_boundary(b: &LayoutBox) -> bool {
    // position: fixed / sticky
    if matches!(b.position, Position::Fixed | Position::Sticky) {
        return true;
    }
    // Aktivni @keyframes animace (vc. paint-only jako colorCycle) -> vlastni
    // layer = damage izolovany, root se neprekresluje.
    if let Some(node) = b.node.as_ref() {
        let id = std::rc::Rc::as_ptr(node) as usize;
        if is_force_layer_node(id) { return true; }
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
    // mix-blend-mode != normal -> blend pres backdrop = vlastni layer
    // (per CSS Compositing-1: mix-blend-mode vytvori stacking context).
    if !matches!(b.mix_blend_mode, super::computed_style::BlendMode::Normal) {
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
    if !matches!(b.mix_blend_mode, super::computed_style::BlendMode::Normal) {
        return LayerReason::MixBlend;
    }
    LayerReason::Root
}

/// Recursivne walks LayoutBox a buduje LayerTree.
/// Box patri do `current` layer pokud nesi nova hranice. Jinak vytvori child layer.
/// Prunik dvou rectu; degenerovany (w/h <= 0) vraci zero-size rect na miste
/// pruniku - compose ho interpretuje jako "vse cliple" (skip draw).
fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect { x: x0, y: y0, width: (x1 - x0).max(0.0), height: (y1 - y0).max(0.0) }
}

fn walk_box(b: &LayoutBox, current: &mut LayerNode, clip: Option<Rect>) {
    // Aktualni box content - registruj do current layer.
    if let Some(node) = b.node.as_ref() {
        let id = std::rc::Rc::as_ptr(node) as usize;
        current.content_box_ids.push(id);
    } else if let Some(pid) = b.pseudo_id {
        // Pseudo-element box (::before/::after) - synteticke id, aby layer
        // fingerprint/damage pokryl i pseudo content.
        current.content_box_ids.push(pid);
    }
    // Overflow clip TOHOTO boxu se aplikuje na deti (vc. sub-layeru v nich).
    // CSS: overflow != visible na jedne ose computuje druhou na auto -> clip
    // obou os pri clips() na kterekoliv. Rect = border box (aproximace padding
    // boxu; 1px border overlap akceptovan).
    let child_clip = if b.overflow_x.clips() || b.overflow_y.clips() {
        Some(match clip {
            Some(c) => intersect_rects(c, b.rect),
            None => b.rect,
        })
    } else {
        clip
    };
    // Pro kazdeho childa: pokud je layer boundary -> novy sub-layer, walks tam.
    // Jinak pokracuje v current layer.
    for child in &b.children {
        if is_layer_boundary(child) {
            // layer_box_id: node ptr / synteticke pseudo_id (::before s
            // transform) / 0. Drive pseudo layer dostal id 0 -> paint pass ho
            // pres find_box_by_node_id nenasel -> nikdy se nevykreslil.
            let layer_id = crate::browser::paint::layer_box_id(child);
            // Layer.root_rect = orig rect (NE AABB-expanded). Compose pres
            // transform pipeline rotuje quad sized orig rect (80x60) pres orig
            // center pivot -> rotated quad shape v page = TIGHT rotated rect's
            // parallelogram = NO corner extension beyond rotated rect = NO bily
            // "padding" area mimo section bg. Border-radius pres SDF v src tex
            // = rounded corners visible v rotated direction (= correct chrome
            // behavior).
            // Snap layer origin na cele px (Chrome raster-origin snapping).
            // Sub-px layer pozice (text span 103.73,54.24) + nearest compose
            // sampler = glyph pixely "preskakuji" = rozsypany maly text
            // (mix-blend .blend-text). Floor origin + ceil size s frakci aby
            // pravy/dolni okraj porad pokryl cely content; sub-px frakce
            // zustava v layer-LOCAL coords kde ji glyph raster snapne.
            let snapped_rect = {
                let fx = child.rect.x.floor();
                let fy = child.rect.y.floor();
                crate::browser::layout::Rect {
                    x: fx,
                    y: fy,
                    width: (child.rect.width + (child.rect.x - fx)).ceil(),
                    height: (child.rect.height + (child.rect.y - fy)).ceil(),
                }
            };
            let mut sub = LayerNode {
                id: layer_id,
                root_rect: snapped_rect,
                z_index: child.z_index,
                opacity: child.opacity,
                blend_mode: child.mix_blend_mode,
                transform: child.transform.clone(),
                transforms: child.transforms.clone(),
                reason: classify_layer_reason(child),
                children: Vec::new(),
                content_box_ids: Vec::new(),
                fingerprint: 0,
                structural_fp: 0,
                damage_rect: None,
                clip_rect: child_clip,
                tiles: Vec::new(),
            };
            walk_box(child, &mut sub, child_clip);
            sub.children.sort_by_key(|l| l.z_index.unwrap_or(0));
            current.children.push(sub);
        } else {
            walk_box(child, current, child_clip);
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

    // Pomocna factory s mix-blend-mode hodnotou.
    fn box_with_blend(mode: &str) -> crate::browser::layout::LayoutBox {
        let html = format!(
            "<html><body><div style=\"mix-blend-mode:{};\">x</div></body></html>", mode);
        let doc = crate::browser::html_parser::parse_html(&html, "about:blank");
        let sheets: Vec<crate::browser::css_parser::Stylesheet> = Vec::new();
        let style_map = std::rc::Rc::new(crate::browser::cascade::cascade(&doc.root, &sheets));
        let layout = crate::browser::layout::layout_tree(&doc.root, &style_map, 800.0, 600.0);
        fn find_div(b: &crate::browser::layout::LayoutBox) -> Option<&crate::browser::layout::LayoutBox> {
            if b.tag.as_deref() == Some("div") { return Some(b); }
            for c in &b.children { if let Some(f) = find_div(c) { return Some(f); } }
            None
        }
        find_div(&layout).cloned().expect("test fixture has div")
    }

    #[test]
    fn mix_blend_creates_layer_with_reason_and_propagates_mode() {
        use crate::browser::computed_style::BlendMode;
        // multiply -> layer boundary + reason MixBlend.
        let b = box_with_blend("multiply");
        assert!(matches!(b.mix_blend_mode, BlendMode::Multiply), "parsing mix-blend-mode");
        assert!(is_layer_boundary(&b), "mix-blend element musi byt vlastni layer");
        assert_eq!(classify_layer_reason(&b), LayerReason::MixBlend);
        // normal -> NENI layer boundary (kvuli blendu).
        let n = box_with_blend("normal");
        assert!(matches!(n.mix_blend_mode, BlendMode::Normal));
        assert!(!is_layer_boundary(&n), "mix-blend:normal nesmi sam o sobe delat layer");
    }

    #[test]
    fn extract_layer_tree_carries_blend_mode() {
        use crate::browser::computed_style::BlendMode;
        // Stranka: rodic s difference dite -> child layer s blend_mode=Difference.
        let html = "<html><body><div style=\"position:relative\"><span style=\"mix-blend-mode:difference\">y</span></div></body></html>";
        let doc = crate::browser::html_parser::parse_html(html, "about:blank");
        let sheets: Vec<crate::browser::css_parser::Stylesheet> = Vec::new();
        let style_map = std::rc::Rc::new(crate::browser::cascade::cascade(&doc.root, &sheets));
        let layout = crate::browser::layout::layout_tree(&doc.root, &style_map, 800.0, 600.0);
        let tree = extract_layer_tree(&layout);
        // Najdi nekde v tree layer s blend_mode != Normal.
        fn find_blend(l: &LayerNode) -> Option<BlendMode> {
            if !matches!(l.blend_mode, BlendMode::Normal) { return Some(l.blend_mode); }
            for c in &l.children { if let Some(b) = find_blend(c) { return Some(b); } }
            None
        }
        assert_eq!(find_blend(&tree), Some(BlendMode::Difference),
            "blend_mode se musi propagovat do LayerNode");
    }

    // ---------- Tile-based rasterization tests (priority 5) ----------

    fn build_layer(width: f32, height: f32) -> LayerNode {
        LayerNode {
            id: 1,
            root_rect: Rect { x: 0.0, y: 0.0, width, height },
            z_index: None,
            opacity: 1.0,
            blend_mode: crate::browser::computed_style::BlendMode::Normal,
            transform: None,
            transforms: Vec::new(),
            reason: LayerReason::Root,
            children: Vec::new(),
            content_box_ids: Vec::new(),
            fingerprint: 0,
            structural_fp: 0,
            damage_rect: None,
            clip_rect: None,
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

    // Naivni referenci compute_layer_tiles (puvodni O(tiles*boxes) algoritmus) -
    // per tile iteruj VSECHNY content boxy. Slouzi k overeni ze nova bucket
    // verze produkuje BIT-IDENTICKE tile fingerprinty.
    fn ref_tile_fingerprints(
        layer: &LayerNode,
        box_map: &std::collections::HashMap<usize, &LayoutBox>,
    ) -> Vec<u64> {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let lr = layer.root_rect;
        if lr.width < 1.0 || lr.height < 1.0 { return Vec::new(); }
        let cols = ((lr.width / TILE_SIZE).ceil() as usize).max(1);
        let rows = ((lr.height / TILE_SIZE).ceil() as usize).max(1);
        let mut out = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let tx = lr.x + col as f32 * TILE_SIZE;
                let ty = lr.y + row as f32 * TILE_SIZE;
                let tw = TILE_SIZE.min(lr.x + lr.width - tx);
                let th = TILE_SIZE.min(lr.y + lr.height - ty);
                let tile_rect = Rect { x: tx, y: ty, width: tw, height: th };
                let mut h = DefaultHasher::new();
                (tile_rect.x.to_bits()).hash(&mut h);
                (tile_rect.y.to_bits()).hash(&mut h);
                (tile_rect.width.to_bits()).hash(&mut h);
                (tile_rect.height.to_bits()).hash(&mut h);
                for id in &layer.content_box_ids {
                    if let Some(bx) = box_map.get(id) {
                        let ext = box_paint_ext(bx);
                        let bx_x0 = bx.rect.x - ext;
                        let bx_y0 = bx.rect.y - ext;
                        let bx_x1 = bx.rect.x + bx.rect.width + ext;
                        let bx_y1 = bx.rect.y + bx.rect.height + ext;
                        let ix = bx_x1 > tile_rect.x && bx_x0 < tile_rect.x + tile_rect.width
                              && bx_y1 > tile_rect.y && bx_y0 < tile_rect.y + tile_rect.height;
                        if !ix { continue; }
                        hash_box_into_tile(&mut h, *id, bx);
                    }
                }
                out.push(h.finish());
            }
        }
        out
    }

    // Postavi layer tree z velke stranky (vice tiles vysoke) a overi ze nova
    // bucketovana compute_layer_tiles da stejne tile fingerprinty jako naivni
    // referenci. Klic korektnosti PERF refactoru (bucket misto per-tile scan).
    #[test]
    fn tile_fingerprints_match_naive_reference() {
        // Mnoho ruznych bloku -> page vyssi nez nekolik tiles (TILE_SIZE=1024).
        let mut body = String::from("<html><body>");
        for i in 0..120 {
            body.push_str(&format!(
                "<div style=\"height:80px;border:2px solid #f0{0:02x}0;background:#{0:02x}3344;outline:3px solid #00f;box-shadow:4px 4px 8px #000;\">blk {0}</div>",
                i % 200));
        }
        body.push_str("</body></html>");
        let doc = crate::browser::html_parser::parse_html(&body, "about:blank");
        let sheets: Vec<crate::browser::css_parser::Stylesheet> = Vec::new();
        let style_map = std::rc::Rc::new(crate::browser::cascade::cascade(&doc.root, &sheets));
        let layout = crate::browser::layout::layout_tree(&doc.root, &style_map, 1265.0, 900.0);
        let tree = extract_layer_tree(&layout);
        // Postav box_map jako compute_fingerprints (pres celou layout tree).
        fn collect<'a>(bx: &'a LayoutBox, out: &mut std::collections::HashMap<usize, &'a LayoutBox>) {
            if let Some(n) = &bx.node { out.insert(std::rc::Rc::as_ptr(n) as usize, bx); }
            for ch in &bx.children { collect(ch, out); }
        }
        let mut map = std::collections::HashMap::new();
        collect(&layout, &mut map);
        // Root layer musi mit > 1 tile (jinak test nic neoveri).
        assert!(tree.tiles.len() > 1, "page musi byt vice tiles vysoka, je {}", tree.tiles.len());
        let reference = ref_tile_fingerprints(&tree, &map);
        let actual: Vec<u64> = tree.tiles.iter().map(|t| t.fingerprint).collect();
        assert_eq!(actual.len(), reference.len(), "stejny pocet tiles");
        assert_eq!(actual, reference,
            "bucketovana compute_layer_tiles musi dat BIT-identicke fingerprinty jako naivni reference");
    }
}
