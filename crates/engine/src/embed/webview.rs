//! `WebView` - per-stranka (per-tab) embeddable view.
//!
//! Drzi DOM, CSS stylesheets, JS interpreter, layout tree, scroll state +
//! offscreen render target. Hostujici aplikace dostane handle na texturu po
//! `render()` a kompozituje ji do sve swap chain spolu s chrome UI.
//!
//! V Phase 2 je vetsina API stub - `todo!()` v body. Phase 3 sem migruje
//! state z `browser::render::App` (DOM, CSS, interp, layout, scroll). Phase 5
//! pripoji offscreen RT + render pass.

use std::path::PathBuf;
use std::sync::Arc;

use crate::browser::dom::Document;
use crate::browser::css_parser::Stylesheet;
use crate::interpreter::Interpreter;
use crate::lexer::base::Lexer;
use crate::parser::Parser;
use crate::tokens::TokenKind;

use super::engine::Engine;
use super::event::{EventResponse, InputEvent, NavigationResult};
use super::loader;

/// Stav per-tab page. Hostujici aplikace drzi jeden `WebView` per logicky tab.
///
/// # Lifecycle
///
/// 1. `WebView::new(engine, width, height)` - prazdny webview
/// 2. `load_html(...)` nebo `load_url(...)` - nahraj stranku, parse HTML/CSS,
///     spust pocatecni JS
/// 3. `handle_input(event)` po kazdem user inputu z hostujici aplikace
/// 4. `render() -> &wgpu::TextureView` kdyz host chce frame (typicky kazdy
///     redraw event); WebView interne skipne pokud nic nezmenilo (dirty flag)
pub use crate::browser::scroll_anim::ScrollAnimState;

/// 5. `resize(w, h)` na window/tab resize
/// 6. Drop pri zavreni tabu
pub struct WebView {
    /// Sdilene engine resources (GPU device + atlas + font registry).
    pub(crate) engine: Arc<Engine>,

    /// Raw HTML source - preserved pres `load_html` pro re-parse / view-source / save.
    pub(crate) raw_html: String,
    /// Raw CSS source (agregat <link>/<style>/<imports>) - preserved pres `load_html`.
    pub(crate) raw_css: String,
    /// Aktualni DOM po HTML5 parse.
    pub(crate) document: Option<Document>,
    /// Vsechny stylesheets (link rel=stylesheet + inline <style> + UA defaults).
    pub(crate) stylesheets: Vec<Stylesheet>,
    /// JS interpreter - drzi globaly, timery, workers, console_log, network_log.
    pub(crate) interpreter: Option<Interpreter>,
    /// Base URL pro relative resolve (file:// nebo http://).
    pub(crate) base_url: Option<String>,
    /// Local path pri file:// navigaci - pro relative file lookup.
    pub(crate) local_path: Option<PathBuf>,
    /// Page title (z <title> nebo `document.title = ...`).
    pub(crate) title: String,

    /// Viewport sirka v logickych CSS px.
    pub(crate) viewport_w: f32,
    /// Viewport vyska v logickych CSS px.
    pub(crate) viewport_h: f32,
    /// HiDPI scale factor (1.0 / 1.5 / 2.0 / ...).
    pub(crate) scale_factor: f32,
    /// Zoom level (Ctrl++ / Ctrl+-).
    pub(crate) zoom: f32,

    /// Vertikalni scroll v CSS px.
    pub(crate) scroll_y: f32,
    /// Horizontalni scroll v CSS px.
    pub(crate) scroll_x: f32,
    /// Smooth scroll target Y - render_via lerp scroll_y -> scroll_target_y
    /// 25 %% per frame pro plynulou animaci wheel scroll.
    pub(crate) scroll_target_y: f32,
    /// Smooth scroll target X.
    pub(crate) scroll_target_x: f32,

    /// Per-element scroll offset - node_id -> (x, y) px. Wheel event hit-testne
    /// scrollable ancestora, modifikuje tento map. Pred paint walk layout_root +
    /// shift_subtree(child, -sx, -sy) pre kazdy scrollable box. Bez tohoto
    /// nested overflow:auto containers nelze scrollovat (drive jen viewport).
    pub(crate) element_scroll: std::collections::HashMap<usize, (f32, f32)>,

    /// Smooth scroll animation state (viewport). Drive jednoduchy lerp 25%/frame
    /// = frame-rate dependent (30 fps 2x pomalejsi nez 60). Nyni cubic-bezier
    /// (ease-in-out 0.42, 0, 0.58, 1) s duration-based timing.
    /// Inspired by Chromium cc/animation/scroll_offset_animation_curve.cc.
    /// None = no active animation, scroll_y == scroll_target_y.
    pub(crate) scroll_anim_y: Option<ScrollAnimState>,
    pub(crate) scroll_anim_x: Option<ScrollAnimState>,

    /// Frame pacing tracker - mereni per-frame stage timings (style/layout/paint/
    /// composite). Pres `browser::render::frame_pacing::FramePacer` foundation.
    pub(crate) frame_pacer: crate::browser::render::frame_pacing::FramePacer,
    /// Web Vitals collector - LCP/CLS/INP. Feed pres paint commands +
    /// layout shift detection.
    pub(crate) web_vitals: crate::browser::web_vitals::WebVitalsCollector,

    /// Offscreen render target texture - vytvori se v `new` (Phase 5).
    /// Phase 2 placeholder `None`.
    pub(crate) target_texture: Option<wgpu::Texture>,
    /// View handle vraceny z `render()`.
    pub(crate) target_view: Option<wgpu::TextureView>,

    /// Dirty flag - `render()` skipne pokud false. Set true pri handle_input
    /// kdyz neco zmenilo viditelne (hover, scroll, JS DOM mutation).
    pub(crate) dirty: bool,

    /// CSS @keyframes animation origin time. Effective_anim_time =
    /// (now - origin) * speed. Reset pri load_html (kazda stranka fresh
    /// animation context).
    pub(crate) animation_origin: std::time::Instant,
    /// Per-element prev frame styles - foundation pro CSS transitions
    /// detection (diff before/after, tween mezi old + new value pres
    /// `transition-duration`).
    pub(crate) prev_style_map: Option<std::rc::Rc<crate::browser::cascade::StyleMap>>,
    /// CSS transitions aktualne tweenujici. Detect z diff prev vs cur
    /// style_map + apply per frame dle elapsed time.
    pub(crate) active_transitions: Vec<crate::browser::cascade::ActiveTransition>,
    /// Aktivni @keyframes anim - (node_id, anim_name). Diff per frame ->
    /// animationstart / animationend events.
    pub(crate) active_animations: std::collections::HashSet<(usize, String)>,
    /// Iteration counter per (node_id, anim_name) - animationiteration event
    /// pri inkrementu.
    pub(crate) animation_iterations: std::collections::HashMap<(usize, String), i32>,
    /// Painted text runs - per-glyph cumulative advances. Foundation pro
    /// per-glyph text selection (hit-test mouse pos -> SelectionPos).
    pub(crate) painted_text_runs: Vec<crate::browser::textrun::TextRun>,
    /// Open <select> dropdown - Some((node_id, anchor_x, anchor_y, anchor_w))
    /// emit popup z option children pres render_via.
    pub(crate) open_select: Option<(usize, f32, f32, f32)>,
    /// Mouse position v CSS px (logical, viewport-relative). Updateuje
    /// `handle_input MouseMove`. Pouzity pro select option hover detect.
    pub(crate) mouse_x: f32,
    pub(crate) mouse_y: f32,
    /// Mouse down position - pro click-vs-drag distinguish pri MouseUp.
    /// Some pri MouseDown, None po MouseUp dispatch.
    pub(crate) mouse_down_at: Option<(f32, f32, std::rc::Rc<crate::browser::dom::Node>)>,
    /// Caret position per <input>/<textarea> node_id (char index 0..value.len()).
    /// TextInput insertne na caret pos + advance. Backspace delete pos-1.
    /// Arrow keys posunou. Render_via emit blinkajici Rect kdy focused input.
    ///
    /// LEGACY: postupne migruje do `editors` (Phase 3 z Session N+22b). Po
    /// uplne migraci selecta + contenteditable smazat.
    pub(crate) input_caret: std::collections::HashMap<usize, usize>,
    /// Unified editor state per <input>/<textarea>/contenteditable node_id.
    /// Drzi text + caret_byte + selection_anchor. WebView synchronizuje
    /// `value` attr <-> EditorState.text pri TextInput/KeyDown.
    /// Hit-test pres editor::shape_text vola hit_test_input -> set caret.
    pub(crate) editors: std::collections::HashMap<usize, crate::browser::editor::EditorState>,
    /// Volitelny overlay painter - hostujici aplikace registruje closure ktera
    /// po build_display_list emit DODATECNE DisplayCommands (inspector
    /// highlight, devtools panel, custom badges). Volana s layout_root +
    /// scroll_y + push prazdneho cmd_buf.
    #[allow(clippy::type_complexity)]
    pub(crate) overlay_painter: Option<Box<dyn FnMut(
        &crate::browser::layout::LayoutBox,
        f32,
        &mut Vec<crate::browser::paint::DisplayCommand>,
    )>>,
    /// Last synced scroll_pos (po sync z interpreteru). Diff vs interp =
    /// detekce ze JS scrollTo modify -> apply. Bez toho by stale-zero
    /// interp.scroll_pos prepisoval scroll_y po host-side drag.
    pub(crate) last_synced_scroll_pos: (f32, f32),
    /// Per-WebView focused DOM node id (Rc::as_ptr). Replaces global
    /// cascade::FOCUSED_NODE thread_local - to slo sdileny pres vsechny
    /// WebViews (chrome/page/devtools) co vedlo k mismatch.
    pub(crate) focused_node_local: Option<usize>,
    /// Range slider drag - Some(node) pri drag thumb (mousedown az mouseup).
    /// MouseMove pak nastavuje value dle x pozice.
    pub(crate) range_drag_node: Option<std::rc::Rc<crate::browser::dom::Node>>,
    /// Element resize drag (CSS resize): (node, start_mouse_x, start_mouse_y,
    /// start_w, start_h, axis "both"/"horizontal"/"vertical").
    pub(crate) resize_drag: Option<(std::rc::Rc<crate::browser::dom::Node>, f32, f32, f32, f32, String)>,
    pub(crate) layout_dumped: bool,
    /// Scrollbar drag state - Some(grab_offset_y) pri V thumb drag.
    /// Inner element scrollbar drag: (node_id, grab_y_offset, max_scroll, bar_y, bar_h).
    /// Drz dostatek info pro mouse move = scroll update bez relookup boxu.
    pub(crate) inner_v_drag: Option<(usize, f32, f32, f32, f32)>,
    pub(crate) inner_h_drag: Option<(usize, f32, f32, f32, f32)>,
    pub(crate) v_scrollbar_drag: Option<f32>,
    /// Scrollbar drag state - Some(grab_offset_x) pri H thumb drag.
    pub(crate) h_scrollbar_drag: Option<f32>,
    /// Last layout_root vyrobeny v render_via - getter pro hostujici aplikaci
    /// (App emits inspector overlay nad webview RT pres dalsi draw_segments
    /// pass; shell nepouziva).
    pub(crate) last_layout_root: Option<crate::browser::layout::LayoutBox>,
    /// Layout rects per node ptr (klic = Rc::as_ptr as usize). Vytvoreny
    /// po kazdem render_via z layout_root. Sdileny do interpreter
    /// layout_lookup callback - JS getBoundingClientRect / offsetXY read.
    pub(crate) layout_rects: std::rc::Rc<std::cell::RefCell<
        std::collections::HashMap<usize, (f32, f32, f32, f32)>
    >>,
    /// Cascade props per node ptr. Po cascade pass je tu Rc<StyleMap> = sdilene
    /// pres interpreter cascade_lookup callback. PERF: drive clone vsech entries
    /// per frame (3434 * HashMap clone = 30ms). Ted Rc::clone (1us).
    pub(crate) cascade_props: std::rc::Rc<std::cell::RefCell<
        Option<std::rc::Rc<crate::browser::cascade::StyleMap>>
    >>,
    /// Stylesheets ve formatu pro document.styleSheets JS API.
    /// Vec<sheet>, kazdy sheet Vec<(selector_text, Vec<(prop, val)>)>.
    /// Rebuild po kazdem load_html z self.stylesheets.
    pub(crate) stylesheets_data: std::rc::Rc<std::cell::RefCell<
        Vec<Vec<(String, Vec<(String, String)>)>>
    >>,
    /// Async jobs registry - background work (image lazy load, file IO).
    /// Drain per render_via vola pending callbacks v main thread.
    pub(crate) async_jobs: crate::browser::async_jobs::AsyncJobsRegistry,
    /// Navigation counter. Inkrementuje pri kazdem `load_html` startu.
    /// DevTools host porovnava proti svemu `last_nav_id`; pri zmene drainne
    /// `collected_sources` do Sources panelu + clearne predchozi console/network.
    pub(crate) nav_id: u64,
    /// Buffer scriptu/stylu sebranych behem load_html / run_scripts.
    /// Each entry = (url, body, language_marker: "js" | "css" | "html").
    /// Vyplneny v `run_scripts` + `load_dom`. Drainuje host pres `take_collected_sources`.
    pub(crate) collected_sources: Vec<(String, String, &'static str)>,
    /// Last seen interp.dom_version() pri render_via. Diff -> dirty=true
    /// (JS DOM mutation pres setAttribute/appendChild/innerHTML potrebuje
    /// repaint i bez explicitniho input event).
    pub(crate) last_render_dom_version: u64,
    /// Canvas generation pri poslednim renderu - detekce canvas kresleni
    /// (canvas ops nebumpaji dom_version) -> dirty -> repaint.
    pub(crate) last_render_canvas_gen: u64,
    /// Cascade cache: hash key (dom_version, hovered_id, focused_id, viewport)
    /// -> resolved StyleMap. Pri shode reuse Rc clone. Bez cache by hover
    /// pohyb mysi vyvolal cascade walk celeho DOMu kazdy frame (2400 LOC HTML
    /// + 28 :hover selectoru = 100% CPU pri pohybu).
    pub(crate) cascade_cache_key: Option<u64>,
    pub(crate) cascade_cache_value: Option<std::rc::Rc<crate::browser::cascade::StyleMap>>,
    /// Cache layout_fingerprint per StyleMap Rc identity. Klic = Rc::as_ptr(style_map).
    /// Pri cascade-level HIT je style_map SAME Rc -> layout_fp identical = reuse.
    /// Sni hash compute z 8-10ms na O(1) lookup.
    pub(crate) layout_fp_cache: Option<(usize, u64)>,
    /// Cache paint_fingerprint per StyleMap Rc identity - stejne jako layout_fp.
    pub(crate) paint_fp_cache: Option<(usize, u64)>,
    /// Hit-test cache pri mouse_move - klic = (x rounded 2px grid, y rounded,
    /// dom_version) -> Option<hovered_id>. Pri stejnem klici reuse posledni
    /// hovered (bez tree walk). Mouse pohyb pres 1px = stejna mrizka.
    pub(crate) hit_test_cache: Option<((i32, i32, u64), Option<usize>)>,
    /// Per-WebView hovered DOM node. Drive bylo jen thread_local v
    /// cascade::HOVERED_NODE (sdileny pres WV) - pohyb mysi v jedne WV
    /// invalidoval cascade cache vsech ostatnich. Per-WV stav fixuje.
    /// Pred cascade_with_viewport call se thread_local set z tohoto fieldu.
    pub(crate) hovered_node_local: Option<usize>,
    /// R-tree spatial hit index. Rebuilt per layout pass. Pri mouse move
    /// O(log N) lookup misto O(N) tree walk = 100 FPS gain pri 5000-box page.
    pub(crate) hit_rtree: Option<rstar::RTree<crate::browser::spatial_hit::HitEntry>>,
    /// P2 hover invalidation set - per node_ptr ktery je ovlivnen :hover rule.
    /// Pri mouse_move check zda prev/new hovered v setu. Pokud NEN0 -
    /// skip dirty=true -> cascade cache hit -> 0 work.
    /// Rebuild po load_html (CSS muze prinest nove :hover).
    pub(crate) hover_affected_set: std::collections::HashSet<usize>,
    /// Profilovaci timery posledniho render_via (ms). Sledovat ktera faze
    /// je drahy. Public read pro host (shell title bar).
    pub(crate) prof_cascade_ms: f32,
    pub(crate) prof_layout_ms: f32,
    pub(crate) prof_paint_ms: f32,
    pub(crate) prof_gpu_ms: f32,
    /// Layout cache klic = (layout_fingerprint hash, viewport_w u32, viewport_h
    /// u32, scroll_y rounded - pro sticky). Fingerprint pres LAYOUT_RELEVANT_PROPS
    /// jen - color/background change neinvaliduje. Pri shode reuse
    /// last_layout_root + skip layout_tree call (363ms drop na <1ms v debug).
    pub(crate) layout_cache_key: Option<(u64, u64, u32, u32)>,
    /// Cache PseudoStyleMap (::before/::after/::selection/::marker). cascade_pseudo
    /// je ~6ms (re-match vsech pseudo selektoru pro vsechny nody) a bezel na KAZDEM
    /// layout cache miss - i kdyz hover meni jen layout prop a pseudo styly se
    /// nezmenily. Klic = (dom_style_version, stylesheets_sig) -> reuse kdyz se DOM
    /// strukturalne/tridami nemenil. (`:hover::before` edge case nepokryt - vzacne.)
    pub(crate) pseudo_map_cache: Option<crate::browser::cascade::PseudoStyleMap>,
    pub(crate) pseudo_map_cache_key: Option<(u64, usize)>,
    /// Per-element matched_decls cache invalidation tracker. Pres dom_version
    /// change clear cache (node_ptrs mohou byt mrtvych po DOM mutaci).
    pub(crate) last_matched_cache_dom_ver: u64,
    /// Pocet emitnutych LAYOUT OVERFLOW logu - rate-limit ze spamu pri opakovanych
    /// layout cache miss. Reset pri load_html.
    pub(crate) layout_overflow_log_count: u32,
    /// Per-layer texture cache (L2 compositor). Klic = layer_id (root box
    /// node_ptr). Hodnota = (texture, view, width_px, height_px, last_paint_fp).
    /// Pri layer texture cache hit: paint do existing texture skip (last_paint_fp
    /// matches) -> compositor pass jen sample. Pri size change: realloc texture.
    /// Pri layer remove (rebuild layer_tree): GC unreferenced entries.
    pub(crate) layer_textures: std::collections::HashMap<usize, LayerTextureSlot>,
    /// Per-tile texture cache (priority 5 - tile-based rasterization).
    /// Klic = (layer_id, tile_idx). Hodnota = TileTextureSlot.
    /// Pri damage tile = re-raster jen tato sub-region. Compose: walk tiles
    /// per layer, blit kazdou. Bez tile granularity = whole layer re-paint
    /// at any change = wasted GPU pri velkych pages.
    /// Inspired by WebRender Picture cache tiles + Chromium cc tile_manager.
    pub(crate) tile_textures: std::collections::HashMap<(usize, usize), TileTextureSlot>,
    /// Posledni LayerTree z extract_layer_tree. Hostujici code muze sample.
    /// Diagnostika + invalidation tracking.
    pub(crate) last_layer_tree: Option<crate::browser::compositor::LayerNode>,
    /// Prev frame fingerprint per layer_id - pro damage detection. mark_damage
    /// porovna current fingerprint vs entry, marknek damage_rect kdyz diff.
    pub(crate) prev_layer_fingerprints: std::collections::HashMap<usize, u64>,
    /// Prev frame tile fingerprints - per (layer_id, tile_idx). Tile-level
    /// damage detection (sub-layer granular). Inspired by WebRender tile cache.
    pub(crate) prev_tile_fingerprints: std::collections::HashMap<(usize, usize), u64>,
    /// Per-layer paint commands cache. Layer s damage_rect=None reuse jeji
    /// last frame commands. Plne wired v D3/D4 (per-layer texture caching).
    /// Aktualne foundation - render_via zatim paintuje monolitne.
    pub(crate) layer_paint_cache: std::collections::HashMap<usize, Vec<crate::browser::paint::DisplayCommand>>,
    /// Paint cache: hash style_map full content. Pri shode (cascade vraci
    /// novy Rc ale identicky content - hover bez :hover effect) skip paint
    /// + gpu submit, reuse cached target_view. Klicova win pres hover bez
    /// vizualni odezvy = 0ms frame.
    pub(crate) last_paint_fingerprint: Option<u64>,
    /// Compositor-driven animations - transform/opacity bez re-cascade.
    /// JS / @keyframes registruje pres `register_compositor_anim`. Tick
    /// per frame v render_via pred extract_layer_tree. Apply na LayerNode
    /// po extract (override layer.opacity/transform). Pri animaci tickou
    /// = structural_fp NE menil = layer texture cache hit = skip raster.
    pub(crate) compositor_anims: crate::browser::compositor::anim::CompositorAnimStore,
}

/// Pomocnik pro debug log node count v layout slow path.
fn count_nodes(node: &std::rc::Rc<crate::browser::dom::NodeData>) -> usize {
    let mut c = 1;
    for child in node.children.borrow().iter() {
        c += count_nodes(child);
    }
    c
}

/// Per-layer texture slot v WebView cache. L2 compositor foundation.
pub struct LayerTextureSlot {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    /// Fingerprint posledniho paintu - hash style + layout boxu v teto layer.
    /// Pri shode skip paint, reuse texture. 0 = nikdy nepaintnuto.
    pub last_paint_fp: u64,
}

/// Per-tile texture slot - sub-layer granular cache (priority 5 z RENDER
/// retrospective: tile-based rasterization). Inspired by WebRender
/// `picture_textures.rs`.
///
/// Klic v HashMap = `(layer_id, tile_idx)`. Pri damage tile = realloc/reuse
/// tato texture + re-raster jen tile region. Pri compose: blit tile texture
/// na parent layer texture na lokalni pozici.
///
/// 256x256 per tile - vlastni overflow `compositor::TILE_SIZE`. Memory
/// overhead vs whole-layer texture: scaling factor depending na page size.
/// Pri 1920x1080 viewport = 8x5 = 40 tiles. Pro velky scroll page = stovky.
/// Pro context: WebRender pouziva 512x512 default tile.
pub struct TileTextureSlot {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    /// Fingerprint tile content - hash boxes intersecting tile rect.
    /// Pri shode skip per-tile raster (texture reuse).
    pub last_paint_fp: u64,
}

/// Layer paint cache statistika za frame: (reused_z_cache, repainted, total).
/// Diagnostika pro damage tracking efficiency.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayerCacheStats {
    pub cached: u32,
    pub repainted: u32,
    pub total: u32,
}

thread_local! {
    static LAYER_CACHE_STATS: std::cell::Cell<LayerCacheStats> =
        const { std::cell::Cell::new(LayerCacheStats { cached: 0, repainted: 0, total: 0 }) };
}

/// Mapuje computed_style::BlendMode na shader mode discriminant (shodne s
/// paint.rs mode_tag + BLEND_COMPOSE_SHADER: 1=Multiply..15=Luminosity).
fn blend_mode_discriminant(m: crate::browser::computed_style::BlendMode) -> u8 {
    use crate::browser::computed_style::BlendMode as B;
    match m {
        B::Normal => 0,
        B::Multiply => 1,
        B::Screen => 2,
        B::Overlay => 3,
        B::Darken => 4,
        B::Lighten => 5,
        B::ColorDodge => 6,
        B::ColorBurn => 7,
        B::HardLight => 8,
        B::SoftLight => 9,
        B::Difference => 10,
        B::Exclusion => 11,
        B::Hue => 12,
        B::Saturation => 13,
        B::Color => 14,
        B::Luminosity => 15,
    }
}

/// Postavi display list per-layer s cache reuse pri no damage. Walk LayerTree
/// in tree order (parent layer first, then children layers - matches paint order).
/// Per layer: damage=None -> reuse cache; damage=Some -> repaint do cache.
/// Inspired by WebRender Picture cache.
fn build_layered_display_list(
    layer_tree: &crate::browser::compositor::LayerNode,
    layout_root: &crate::browser::layout::LayoutBox,
    scroll_y: f32,
    viewport_h: f32,
    cache: &mut std::collections::HashMap<usize, Vec<crate::browser::paint::DisplayCommand>>,
) -> Vec<crate::browser::paint::DisplayCommand> {
    LAYER_CACHE_STATS.with(|c| c.set(LayerCacheStats::default()));
    // Set viewport cull bounds (thread_local).
    crate::browser::paint::set_viewport_cull(scroll_y, scroll_y + viewport_h);
    let mut out: Vec<crate::browser::paint::DisplayCommand> = Vec::with_capacity(1024);
    walk_layer_paint(layer_tree, layout_root, &mut out, cache);
    crate::browser::paint::clear_viewport_cull();
    out
}

fn walk_layer_paint(
    layer: &crate::browser::compositor::LayerNode,
    layout_root: &crate::browser::layout::LayoutBox,
    out: &mut Vec<crate::browser::paint::DisplayCommand>,
    cache: &mut std::collections::HashMap<usize, Vec<crate::browser::paint::DisplayCommand>>,
) {
    let layer_box = crate::browser::paint::find_box_by_node_id(layout_root, layer.id);
    // V MONOLITHIC mode se transform baked do vertexu (geometry post-process).
    // Layer cache ale structural_fp drzi STEJNY pri transform-only zmene (zamerne
    // - layer compose to resi bez re-paintu). To by v monolithic zamrzlo animovany
    // transform (cache vraci zapeceny prvni frame). Proto force re-paint layeru s
    // transformem v monolithic = re-bake aktualniho transformu kazdy frame.
    let force_repaint_tf = crate::browser::paint::is_monolithic_paint()
        && !layer.transforms.is_empty();
    let layer_cmds: Vec<crate::browser::paint::DisplayCommand> = if !force_repaint_tf
        && layer.damage_rect.is_none()
        && cache.contains_key(&layer.id)
    {
        // No damage - reuse cached commands.
        LAYER_CACHE_STATS.with(|c| {
            let mut s = c.get(); s.cached += 1; s.total += 1; c.set(s);
        });
        cache.get(&layer.id).cloned().unwrap_or_default()
    } else if let Some(bx) = layer_box {
        // Damage or first paint - repaint subtree (layer-aware).
        let mut tmp = Vec::new();
        crate::browser::paint::paint_layer_into(bx, &mut tmp);
        // Force-repaint (transform) NEcachujeme - musi se re-bakovat kazdy frame.
        if !force_repaint_tf {
            cache.insert(layer.id, tmp.clone());
        }
        LAYER_CACHE_STATS.with(|c| {
            let mut s = c.get(); s.repainted += 1; s.total += 1; c.set(s);
        });
        tmp
    } else {
        Vec::new()
    };
    // POZN: v monolithic mode (MONOLITHIC_PAINT=true) paint_layer_into uz
    // aplikoval layer 2D transform pres CPU geometry (paint.rs ~2491) -> layer_cmds
    // maji transform zapeceny do vertexu (BEZ clipu). Tady jen flatten.
    out.extend(layer_cmds);
    // Recurse child layers in z-index order.
    for child in &layer.children {
        walk_layer_paint(child, layout_root, out, cache);
    }
}

/// Per-layer cache pro GPU rendering (D4 plne pipeline). Cmds jsou v
/// layer-local coords (origin = layer.root_rect top-left). Pri composite pass
/// renderer kresli layer textures at viewport position (po scroll shift).
/// Cache klic = layer_id. Hodnota = Vec<DisplayCommand> v local coords.
/// Scale faktor z transform chainu layeru - pro raster boost (ostry text u
/// scale(N)). Product Scale ops (max sx/sy), 1.0 pro rotate-only / 3D / bez
/// transformu. Cap [1.0, 4.0]: nikdy neshrinkuje, limit aby velke scale
/// neudelalo obri texturu.
pub(crate) fn layer_raster_scale(transforms: &[crate::browser::layout::TransformOp]) -> f32 {
    use crate::browser::layout::TransformOp;
    let mut s = 1.0_f32;
    for op in transforms {
        match op {
            TransformOp::Scale(sx, sy) => s *= sx.abs().max(sy.abs()),
            TransformOp::Scale3D { x, y, .. } => s *= x.abs().max(y.abs()),
            _ => {}
        }
    }
    // KVANTIZACE na 0.25 kroky. Bez toho scale TRANSITION interpoluje raster
    // spojite (1.299 vs 1.301) -> phys texture size osciluje o 1px (116<->117) ->
    // ensure_layer_texture re-creates texturu KAZDY frame -> prazdna texture ->
    // scale-hover box MIZEL. Kvantizace = stabilni size mezi blizkymi scale =
    // zadne re-creation = box viditelny + plynula animace (compose scaluje quad).
    let q = (s * 4.0).round() / 4.0;
    q.clamp(1.0, 4.0)
}

pub(crate) fn build_layer_local_cache(
    layer_tree: &crate::browser::compositor::LayerNode,
    layout_root: &crate::browser::layout::LayoutBox,
    cache: &mut std::collections::HashMap<usize, Vec<crate::browser::paint::DisplayCommand>>,
) {
    walk_layer_local(layer_tree, layout_root, cache);
}

fn walk_layer_local(
    layer: &crate::browser::compositor::LayerNode,
    layout_root: &crate::browser::layout::LayoutBox,
    cache: &mut std::collections::HashMap<usize, Vec<crate::browser::paint::DisplayCommand>>,
) {
    if layer.damage_rect.is_some() || !cache.contains_key(&layer.id) {
        if let Some(bx) = crate::browser::paint::find_box_by_node_id(layout_root, layer.id) {
            let mut tmp = Vec::new();
            crate::browser::paint::paint_layer_into(bx, &mut tmp);
            // Shift cmds to layer-local: subtract layer.root_rect origin.
            let dx = -layer.root_rect.x;
            let dy = -layer.root_rect.y;
            for cmd in tmp.iter_mut() {
                crate::browser::render::segments::shift_command_x(cmd, dx);
                crate::browser::render::segments::shift_command_y(cmd, dy);
            }
            cache.insert(layer.id, tmp);
        }
    }
    for child in &layer.children {
        walk_layer_local(child, layout_root, cache);
    }
}

/// Get last frame layer cache statistika.
pub fn last_layer_cache_stats() -> LayerCacheStats {
    LAYER_CACHE_STATS.with(|c| c.get())
}

/// DEBUG: najdi prvni node s danou CSS tridou (pro RWE_FORCE_HOVER mereni).
fn find_first_node_by_class(bx: &crate::browser::layout::LayoutBox, class: &str) -> Option<usize> {
    if let Some(n) = &bx.node {
        if let Some(c) = n.attr("class") {
            if c.split_whitespace().any(|cls| cls == class) {
                return Some(std::rc::Rc::as_ptr(n) as usize);
            }
        }
    }
    for ch in &bx.children {
        if let Some(id) = find_first_node_by_class(ch, class) { return Some(id); }
    }
    None
}

/// Path lookup LayoutBox dle node_id. Vraci (rect_w, rect_h, content_w, content_h).
fn find_box_dims(bx: &crate::browser::layout::LayoutBox, node_id: usize)
    -> Option<(f32, f32, f32, f32)>
{
    if let Some(n) = &bx.node {
        if std::rc::Rc::as_ptr(n) as usize == node_id {
            return Some((bx.rect.width, bx.rect.height, bx.inner_content_w, bx.inner_content_h));
        }
    }
    for ch in &bx.children {
        if let Some(d) = find_box_dims(ch, node_id) { return Some(d); }
    }
    None
}

/// Klik na <label> aktivuje jeho asociovany control (browser chovani). Vrati
/// control pro label, jinak puvodni node. for="id" nebo prvni form-control
/// descendant (label obaluje input). Bez tohoto klik na text labelu (napr.
/// "Polozka 1") nic nedelal = checkbox/radio "nereaguji".
fn resolve_label_target(node: &std::rc::Rc<crate::browser::dom::Node>)
    -> std::rc::Rc<crate::browser::dom::Node>
{
    if node.tag_name().as_deref() != Some("label") {
        return std::rc::Rc::clone(node);
    }
    // for="id" -> dohledej element v dokumentu (walk od rootu).
    if let Some(for_id) = node.attr("for") {
        if !for_id.is_empty() {
            let mut root = std::rc::Rc::clone(node);
            loop {
                let parent = root.parent.borrow().upgrade();
                match parent { Some(p) => root = p, None => break }
            }
            if let Some(ctrl) = find_node_by_id(&root, &for_id) {
                return ctrl;
            }
        }
    }
    // Jinak prvni form-control descendant (label obaluje input/select/...).
    if let Some(ctrl) = first_form_control(node) {
        return ctrl;
    }
    std::rc::Rc::clone(node)
}

fn first_form_control(node: &std::rc::Rc<crate::browser::dom::Node>)
    -> Option<std::rc::Rc<crate::browser::dom::Node>>
{
    for child in node.children.borrow().iter() {
        if matches!(child.tag_name().as_deref(),
            Some("input") | Some("select") | Some("textarea") | Some("button")) {
            return Some(std::rc::Rc::clone(child));
        }
        if let Some(c) = first_form_control(child) { return Some(c); }
    }
    None
}

fn find_node_by_id(node: &std::rc::Rc<crate::browser::dom::Node>, id: &str)
    -> Option<std::rc::Rc<crate::browser::dom::Node>>
{
    if node.attr("id").as_deref() == Some(id) { return Some(std::rc::Rc::clone(node)); }
    for child in node.children.borrow().iter() {
        if let Some(f) = find_node_by_id(child, id) { return Some(f); }
    }
    None
}

/// Vraci true pokud element je focusable (nativne nebo pres tabindex >= -... ).
/// tabindex="-1" je focusable programaticky/klikem, ne pres Tab - pro klik OK.
fn is_focusable_element(node: &std::rc::Rc<crate::browser::dom::Node>) -> bool {
    let native = matches!(node.tag_name().as_deref(),
        Some("input") | Some("textarea") | Some("button")
        | Some("a") | Some("select"));
    if native {
        // <a> bez href neni focusable.
        if node.tag_name().as_deref() == Some("a") {
            return node.attr("href").is_some();
        }
        return true;
    }
    // [tabindex] na libovolnem elementu = focusable klikem.
    node.attr("tabindex").is_some()
}

/// Walk od `node` nahoru, vrati nejblizsi focusable element (vc. node samotneho).
/// Prohlizece pri kliku na potomka tabindex elementu focusuji ten predka -
/// klik na <span> uvnitr <div tabindex=0> focusuje div, ne span.
fn nearest_focusable(node: &std::rc::Rc<crate::browser::dom::Node>)
    -> Option<std::rc::Rc<crate::browser::dom::Node>>
{
    let mut cur = Some(std::rc::Rc::clone(node));
    while let Some(n) = cur {
        if is_focusable_element(&n) { return Some(n); }
        cur = n.parent.borrow().upgrade();
    }
    None
}

/// Postavi jednoduchy focus/blur event objekt (type + target). Focus/blur
/// jsou non-bubbling; fire_inline cte `onfocus`/`onblur` atribut na targetu.
fn make_focus_event(ev_type: &str, node: &std::rc::Rc<crate::browser::dom::Node>)
    -> crate::interpreter::JsValue
{
    let mut event = crate::interpreter::JsObject::new();
    event.set("type".into(), crate::interpreter::JsValue::Str(ev_type.into()));
    event.set("target".into(), crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(node)));
    crate::interpreter::JsValue::Object(std::rc::Rc::new(std::cell::RefCell::new(event)))
}

/// Nastav/nahrad jednu CSS property v inline `style` atributu node (merge s
/// existujicimi). Pouzito pro element resize override (width/height).
fn set_inline_style_prop(node: &std::rc::Rc<crate::browser::dom::Node>, prop: &str, value: &str) {
    let existing = node.attr("style").unwrap_or_default();
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut found = false;
    for decl in existing.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        if let Some((k, v)) = decl.split_once(':') {
            let key = k.trim().to_string();
            if key.eq_ignore_ascii_case(prop) {
                pairs.push((key, value.to_string()));
                found = true;
            } else {
                pairs.push((key, v.trim().to_string()));
            }
        }
    }
    if !found { pairs.push((prop.to_string(), value.to_string())); }
    let s = pairs.iter().map(|(k, v)| format!("{}:{}", k, v)).collect::<Vec<_>>().join(";");
    node.set_attr("style", &s);
}

/// Najde nejhlubsi box s resize != none jehoz grip zona (pravy dolni roh ~16px)
/// obsahuje (cx, cy) content coords. Vrati (node, rect_w, rect_h, axis).
fn find_resize_grip(bx: &crate::browser::layout::LayoutBox, cx: f32, cy: f32)
    -> Option<(std::rc::Rc<crate::browser::dom::Node>, f32, f32, String)>
{
    // Deti maji prioritu (nejhlubsi).
    for ch in &bx.children {
        if let Some(r) = find_resize_grip(ch, cx, cy) { return Some(r); }
    }
    if !bx.resize.is_empty() {
        let gx0 = bx.rect.x + bx.rect.width - 16.0;
        let gy0 = bx.rect.y + bx.rect.height - 16.0;
        let gx1 = bx.rect.x + bx.rect.width;
        let gy1 = bx.rect.y + bx.rect.height;
        if cx >= gx0 && cx <= gx1 && cy >= gy0 && cy <= gy1 {
            if let Some(n) = bx.node.as_ref() {
                return Some((std::rc::Rc::clone(n), bx.rect.width, bx.rect.height, bx.resize.clone()));
            }
        }
    }
    None
}

impl WebView {
    /// Vytvori prazdny WebView s viewportem dane velikosti. Offscreen RT
    /// alokovan az v Phase 5 - Phase 2 nech `target_texture = None`.
    pub fn new(engine: Arc<Engine>, viewport_w: u32, viewport_h: u32) -> Self {
        Self {
            engine,
            raw_html: String::new(),
            raw_css: String::new(),
            document: None,
            stylesheets: Vec::new(),
            interpreter: None,
            base_url: None,
            local_path: None,
            title: String::new(),
            viewport_w: viewport_w as f32,
            viewport_h: viewport_h as f32,
            scale_factor: 1.0,
            zoom: 1.0,
            scroll_y: 0.0,
            scroll_x: 0.0,
            scroll_target_y: 0.0,
            scroll_target_x: 0.0,
            scroll_anim_y: None,
            scroll_anim_x: None,
            frame_pacer: crate::browser::render::frame_pacing::FramePacer::new(60),
            web_vitals: crate::browser::web_vitals::WebVitalsCollector::new(),
            element_scroll: std::collections::HashMap::new(),
            target_texture: None,
            target_view: None,
            dirty: true,
            animation_origin: std::time::Instant::now(),
            prev_style_map: None,
            active_transitions: Vec::new(),
            active_animations: std::collections::HashSet::new(),
            animation_iterations: std::collections::HashMap::new(),
            painted_text_runs: Vec::new(),
            open_select: None,
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_down_at: None,
            input_caret: std::collections::HashMap::new(),
            editors: std::collections::HashMap::new(),
            overlay_painter: None,
            last_synced_scroll_pos: (0.0, 0.0),
            focused_node_local: None,
            range_drag_node: None,
            resize_drag: None,
            layout_dumped: false,
            v_scrollbar_drag: None,
            h_scrollbar_drag: None,
            inner_v_drag: None,
            inner_h_drag: None,
            last_layout_root: None,
            layout_rects: std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
            cascade_props: std::rc::Rc::new(std::cell::RefCell::new(None)),
            stylesheets_data: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            async_jobs: crate::browser::async_jobs::AsyncJobsRegistry::new(),
            nav_id: 0,
            collected_sources: Vec::new(),
            last_render_dom_version: 0,
            last_render_canvas_gen: 0,
            cascade_cache_key: None,
            cascade_cache_value: None,
            pseudo_map_cache: None,
            pseudo_map_cache_key: None,
            layout_fp_cache: None,
            paint_fp_cache: None,
            hit_test_cache: None,
            hovered_node_local: None,
            hit_rtree: None,
            hover_affected_set: std::collections::HashSet::new(),
            prof_cascade_ms: 0.0,
            prof_layout_ms: 0.0,
            prof_paint_ms: 0.0,
            prof_gpu_ms: 0.0,
            layout_cache_key: None,
            last_matched_cache_dom_ver: 0,
            layout_overflow_log_count: 0,
            layer_textures: std::collections::HashMap::new(),
            last_layer_tree: None,
            prev_layer_fingerprints: std::collections::HashMap::new(),
            prev_tile_fingerprints: std::collections::HashMap::new(),
            layer_paint_cache: std::collections::HashMap::new(),
            last_paint_fingerprint: None,
            compositor_anims: crate::browser::compositor::anim::CompositorAnimStore::new(),
            tile_textures: std::collections::HashMap::new(),
        }
    }

    /// Registruj compositor-driven opacity anim.
    /// `node_id` = Rc::as_ptr(node) as usize. Anim tikla per render_via tick,
    /// override layer.opacity v compose pass bez re-cascade/repaint.
    pub fn register_opacity_anim(
        &mut self,
        node_id: usize,
        from: f32,
        to: f32,
        duration_ms: f32,
        easing: crate::browser::compositor::anim::Easing,
        iterations: f32,
        alternate: bool,
    ) {
        self.compositor_anims.insert(node_id, crate::browser::compositor::anim::CompositorAnim::Opacity {
            from, to,
            start: std::time::Instant::now(),
            duration_ms, easing, iterations, alternate,
            current: from,
            done: false,
        });
        self.dirty = true;
    }

    /// Registruj compositor-driven transform anim.
    pub fn register_transform_anim(
        &mut self,
        node_id: usize,
        from: crate::browser::layout::TransformOp,
        to: crate::browser::layout::TransformOp,
        duration_ms: f32,
        easing: crate::browser::compositor::anim::Easing,
        iterations: f32,
        alternate: bool,
    ) {
        let cur = from.clone();
        self.compositor_anims.insert(node_id, crate::browser::compositor::anim::CompositorAnim::Transform {
            from, to,
            start: std::time::Instant::now(),
            duration_ms, easing, iterations, alternate,
            current: cur,
            done: false,
        });
        self.dirty = true;
    }

    /// Odstran vsechny compositor anim pro node.
    pub fn remove_compositor_anim(&mut self, node_id: usize) {
        self.compositor_anims.remove(node_id);
    }

    /// Diagnostika pocet aktivnich compositor anim.
    pub fn compositor_anim_count(&self) -> usize {
        self.compositor_anims.active_count()
    }

    /// Diagnostika: pocet aktivnich layers v posledni render. L1 compositor
    /// foundation - vystavena hodnota umoznuje shell title bar zobrazit
    /// "L:N" pocet layers per WebView.
    pub fn layer_count(&self) -> usize {
        self.last_layer_tree.as_ref()
            .map(crate::browser::compositor::count_layers)
            .unwrap_or(0)
    }

    /// True pokud WV ma aktivni setInterval (cdp.js poll, anim loops).
    /// Shell::redraw pak schedule next redraw - bez tohoto idle WV by
    /// pollEvents nikdy nepokracoval (host stop request_redraw po prvnim
    /// dirty=false render).
    pub fn has_pending_intervals(&self) -> bool {
        self.interpreter.as_ref()
            .map(|i| !i.interval_queue.borrow().is_empty())
            .unwrap_or(false)
    }

    /// Pending requestAnimationFrame callbacky. Bez tohoto check by RAF-driven
    /// canvas animace (particles/wave) provedly max 1 frame a zamrzly (event
    /// loop usnul, callbacky se nedrainovaly).
    pub fn has_pending_raf(&self) -> bool {
        self.interpreter.as_ref()
            .map(|i| !i.raf_callbacks.borrow().is_empty())
            .unwrap_or(false)
    }

    /// Posledni render_via per-phase timing (ms): (cascade, layout, paint, gpu).
    /// Pro diagnostiku - shell title bar nebo overlay.
    pub fn render_phase_times(&self) -> (f32, f32, f32, f32) {
        (self.prof_cascade_ms, self.prof_layout_ms, self.prof_paint_ms, self.prof_gpu_ms)
    }

    /// Layer cache stats z posledniho frame: (cached_z_prev, repainted, total).
    /// Cached high = damage tracking efficient. Repainted high = page se hodne meni.
    pub fn layer_cache_stats(&self) -> (u32, u32, u32) {
        let s = last_layer_cache_stats();
        (s.cached, s.repainted, s.total)
    }

    /// Painted text runs z posledniho `render_via` (per-glyph cumulative
    /// advances). Foundation pro text selection hit-test.
    pub fn text_runs(&self) -> &[crate::browser::textrun::TextRun] {
        &self.painted_text_runs
    }

    /// Hit-test (x, y) na painted_text_runs - vrati SelectionPos pres mouse.
    pub fn hit_test_text(&self, x: f32, y: f32) -> Option<crate::browser::textrun::SelectionPos> {
        crate::browser::textrun::hit_test_runs(&self.painted_text_runs, x, y)
    }

    /// Aktualni layout tree z posledniho `render_via`. None pred prvnim render.
    /// Pouziti: hostujici aplikace emit custom overlay (inspector highlight,
    /// devtools devtools_panel, ...) pres dalsi `Renderer::draw_segments_into_
    /// view_clipped` pass nad `target_view()` PRED `present_external_to_swap_chain`.
    pub fn last_layout_root(&self) -> Option<&crate::browser::layout::LayoutBox> {
        self.last_layout_root.as_ref()
    }

    /// Nahraj HTML + CSS string + spust inline/external `<script>` tagy.
    /// `base_url` se pouzije pro relative `<link rel=stylesheet>` a
    /// `<img src=...>` resolve.
    pub fn load_html(&mut self, html: &str, css: &str, base_url: Option<String>) -> NavigationResult {
        let result = self.load_dom(html, css, base_url);
        self.run_scripts();
        // Po JS muze byt title prepsany pres `document.title = ...`. Refresh.
        if let Some(interp) = &self.interpreter {
            let doc_title = interp.document.borrow().title.clone();
            if !doc_title.is_empty() {
                self.title = doc_title;
            }
        }
        // Dispatch DOMContentLoaded + load events.
        if let Some(interp) = self.interpreter.as_mut() {
            interp.dispatch_window_event("DOMContentLoaded", crate::interpreter::JsValue::Undefined);
            interp.dispatch_window_event("load", crate::interpreter::JsValue::Undefined);
        }
        // Race fix: force bump dom_version po load+scripts aby DevTools host
        // pri pristim redraw zaregistroval novy DOM (bez nutnosti klik uvnitr).
        if let Some(interp) = &self.interpreter {
            interp.bump_dom_version();
        }
        result
    }

    /// DOM mutation counter z interpreteru. DevTools host porovnava proti
    /// vlastnimu snapshotu a pri zmene rebuilds Elements tree. Vraci 0
    /// pokud interpreter neexistuje (no document loaded).
    pub fn dom_version(&self) -> u64 {
        self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0)
    }

    /// Style/strukturalni DOM mutation counter (class/id/style/add/remove,
    /// NE textContent/SVG geometry). DevTools to pouziva pro DOM.documentUpdated
    /// - tree re-fetch jen pri strukturalni zmene, ne pri SVG points animaci
    /// (jinak by se tree re-fetchoval kazdych 500ms = 1s render = <1 FPS).
    pub fn dom_style_version(&self) -> u64 {
        self.interpreter.as_ref().map(|i| i.dom_style_version()).unwrap_or(0)
    }

    /// Navigation counter. Inkrementuje pri kazdem `load_html` startu.
    /// DevTools host porovnava proti svemu `last_nav_id` snapshotu.
    pub fn nav_id(&self) -> u64 {
        self.nav_id
    }

    /// Drainne nasbirane HTML/CSS/JS sources do (url, body, lang_marker) Vec.
    /// Volat po detekci nav_id zmeny - DevTools registruje do Sources panelu.
    /// `lang_marker`: "html" | "css" | "js".
    pub fn take_collected_sources(&mut self) -> Vec<(String, String, &'static str)> {
        std::mem::take(&mut self.collected_sources)
    }

    /// Stejne jako `load_html` ale BEZ behu `<script>` tagu. Pouziti:
    /// `App::sync_webview_from_app` (Phase 4a) kde App.interpreter je primary
    /// + uz scripts probehl - mirror WebView ma DOM/stylesheets identicke ale
    /// JS by se nesmel spustit podruhe (dvojite fetch / console / DOM mutace).
    pub fn load_dom(&mut self, html: &str, css: &str, base_url: Option<String>) -> NavigationResult {
        let base = base_url.clone().unwrap_or_else(|| "about:blank".to_string());
        // Preserve raw sources pred parse - app/devtools/save je mohou potrebovat.
        self.raw_html = html.to_string();
        self.raw_css = css.to_string();
        // Inkrementuj nav counter + reset sources buffer. DevTools host
        // pri pristim drain registruje fresh sady scriptu pro novou stranku.
        self.nav_id = self.nav_id.wrapping_add(1);
        self.collected_sources.clear();
        // Pre-collect HTML + CSS jako Sources entries.
        self.collected_sources.push((base.clone(), html.to_string(), "html"));
        if !css.trim().is_empty() {
            self.collected_sources.push((format!("{}#inline-styles", base), css.to_string(), "css"));
        }
        let doc = crate::browser::html_parser::parse_html(html, &base);

        let stylesheet = crate::browser::css_parser::parse_stylesheet(css);
        let stylesheet_count = if stylesheet.rules.is_empty() { 0 } else { 1 };

        // Init interpreter + set document. Bez run_scripts (volaci kod o ne stoji).
        // SDILENY root Rc<Node> mezi self.document a interp.document - bez
        // sdileni mela hit_test pres self.document.root jiny ptr nez JS lookup
        // pres interp.document.root (focused_node nikdy nesedi).
        let mut interp = Interpreter::new();
        let interp_doc = crate::browser::dom::Document {
            root: std::rc::Rc::clone(&doc.root),
            url: doc.url.clone(),
            title: doc.title.clone(),
            selection: std::cell::RefCell::new(
                crate::browser::selection::SelectionRegistry::new()),
        };
        interp.set_document(interp_doc);

        // Wire-up lookups - layout_rects + cascade_props sdilene s host.
        // Po kazdem render_via webview rebuilds tyto mapy, interpreter
        // closures je read pres Rc<RefCell> clone.
        let rects_clone = std::rc::Rc::clone(&self.layout_rects);
        interp.set_layout_lookup(move |ptr| {
            rects_clone.borrow().get(&(ptr as usize)).copied()
        });
        let cascade_clone = std::rc::Rc::clone(&self.cascade_props);
        interp.set_cascade_lookup(move |ptr| {
            cascade_clone.borrow().as_ref()
                .and_then(|m| m.get(&(ptr as usize)).map(|rc| rc.as_ref().clone()))
                .unwrap_or_default()
        });
        let sheets_clone = std::rc::Rc::clone(&self.stylesheets_data);
        interp.set_stylesheets_lookup(move || {
            sheets_clone.borrow().clone()
        });

        // Pre-build stylesheets_data ze stylesheet pro document.styleSheets API.
        let mut sheet_data: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for rule in &stylesheet.rules {
            let selector_text = rule.selectors.iter()
                .map(|s| s.parts.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>().join(", ");
            let decls = rule.declarations.iter()
                .map(|d| (d.property.clone(), d.value.clone()))
                .collect();
            sheet_data.push((selector_text, decls));
        }
        *self.stylesheets_data.borrow_mut() = if sheet_data.is_empty() {
            Vec::new()
        } else {
            vec![sheet_data]
        };

        self.title = doc.title.clone();
        let doc_root_clone = std::rc::Rc::clone(&doc.root);
        self.document = Some(doc);
        self.stylesheets = vec![stylesheet];
        // Stylesheets uplne nove pres load_html - drop cache (per-WV keys obsahuji
        // host_id, ale stary host_id zustal v thread_local pres jine WV - bezpecne
        // clearovat protoze nova pagina = nove node_ptrs).
        // POZOR: clear smaze i entries jinych WV ale ty se rebuilduji rychle (cold start).
        crate::browser::cascade::clear_matched_decls_cache();
        // P2 hover invalidation set - build pres new stylesheets + DOM.
        // Pri mouse_move host kontroluje zda hovered v set. Pokud NE - skip
        // dirty -> cascade reuse cache. Pres mass :hover v devtools-frontend
        // (24x) mass hover events ne-affected nodes = no cascade walks.
        self.hover_affected_set = crate::browser::cascade::collect_hover_affected_set(
            &doc_root_clone, &self.stylesheets);
        eprintln!("[HOVER SET] {} affected nodes", self.hover_affected_set.len());
        // Reset overflow log counter pro novou stranku (max 3 logy per page).
        self.layout_overflow_log_count = 0;
        self.base_url = base_url.clone();
        self.interpreter = Some(interp);
        self.dirty = true;
        // Animation origin reset - fresh stranka start = anim elapsed 0.
        self.animation_origin = std::time::Instant::now();
        // Transitions / animations state cleanup pri nove strance.
        self.prev_style_map = None;
        self.active_transitions.clear();
        self.active_animations.clear();
        self.animation_iterations.clear();

        NavigationResult {
            url: base,
            status: 200,
            stylesheet_count,
            local_path: self.local_path.clone(),
        }
    }

    /// Naviguj na URL. `http(s)://` jde pres ureq, lokalni paths cte z disku.
    /// Helper z `embed::loader` agregue CSS z `<link rel=stylesheet>`, `<style>`,
    /// co-located `.css`.
    ///
    /// Vrati `None` pokud fetch/read selze - WebView state se nemeni.
    pub fn load_url(&mut self, url: &str) -> Option<NavigationResult> {
        let loaded = loader::load_page(url)?;
        // Update local_path PRED load_html aby ho NavigationResult vratil.
        self.local_path = loaded.local_path.clone();
        let mut result = self.load_html(&loaded.html, &loaded.css, loaded.base_url);
        result.local_path = loaded.local_path;
        Some(result)
    }

    /// POST form submit + load response HTML. Pro `<form method=post>`.
    /// Pri uspechu nahradi current page response HTML + base_url = action URL.
    pub fn load_url_post(&mut self, url: &str, body: &str) -> Option<NavigationResult> {
        let html = crate::browser::render::forms::post_form(url, body)?;
        let css = String::new();
        let result = self.load_html(&html, &css, Some(url.to_string()));
        Some(result)
    }

    /// Spusti vsechny inline + external `<script>` tagy z dokumentu pres
    /// aktualni interpreter. Volane interne z `load_html` po set_document.
    pub fn run_scripts(&mut self) {
        if self.interpreter.is_none() { return; }
        let base = self.base_url.clone().unwrap_or_default();
        let fetch_external = std::env::var("RWE_NO_SCRIPTS")
            .map(|v| v != "1" && !v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);

        // Phase 1: collect scripts (fetch external) - krátký interp borrow.
        let mut scripts: Vec<(String, String)> = Vec::new();
        {
            let interp = self.interpreter.as_mut().unwrap();
            let doc_ref = interp.document.clone();
            let script_nodes = doc_ref.borrow().root.get_elements_by_tag("script");
            scripts.reserve(script_nodes.len());
            for (i, s) in script_nodes.iter().enumerate() {
                if let Some(src_attr) = s.attr("src") {
                    if !fetch_external { continue; }
                    let src_attr = src_attr.trim().to_string();
                    if src_attr.is_empty() { continue; }
                    let abs_url = if src_attr.starts_with("http://")
                        || src_attr.starts_with("https://")
                        || src_attr.starts_with("file://")
                    {
                        src_attr.clone()
                    } else if !base.is_empty() {
                        crate::browser::render::resolve_url(&base, &src_attr)
                    } else {
                        src_attr.clone()
                    };
                    match crate::browser::render::fetch_text_url(&abs_url) {
                        Some(body) => {
                            interp.network_log.borrow_mut().push((abs_url.clone(), 200));
                            scripts.push((abs_url, body));
                        }
                        None => {
                            interp.network_log.borrow_mut().push((abs_url.clone(), 0));
                            interp.console_log.borrow_mut().push((
                                "error".into(),
                                format!("[script fetch failed] {abs_url}"),
                            ));
                        }
                    }
                } else {
                    let url = format!("<inline #{}>", i + 1);
                    let body = s.text_content();
                    if !body.trim().is_empty() {
                        scripts.push((url, body));
                    }
                }
            }
        }

        // Phase 2: append do collected_sources (mut self).
        for (url, body) in &scripts {
            self.collected_sources.push((url.clone(), body.clone(), "js"));
        }

        // Phase 3: actual eval - znovu interp borrow.
        let interp = self.interpreter.as_mut().unwrap();
        for (url, src) in scripts {
            if src.trim().is_empty() { continue; }
            // Debug: log script header pred eval (DIAG bug: parser/script error
            // bez kontextu = nevime ktery script chyboval).
            let preview = src.lines().next().unwrap_or("").chars().take(80).collect::<String>();
            eprintln!("[run_script] url={} ({} bytes) line1: {}",
                url, src.len(), preview);
            match Lexer::parse_str(&src, "<inline>") {
                Ok(lex) => {
                    let tokens: Vec<_> = lex.tokens.into_iter()
                        .filter(|t| !matches!(t.kind,
                            TokenKind::Whitespace | TokenKind::Newline
                            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
                        .collect();
                    let mut parser = Parser::new(tokens);
                    match parser.parse() {
                        Ok(prog) => {
                            if let Err(e) = interp.run(&prog) {
                                let msg = format!("[script error] {e}");
                                eprintln!("{msg}");
                                interp.console_log.borrow_mut()
                                    .push(("error".into(), msg));
                            }
                        }
                        Err(e) => {
                            let msg = format!("[parser error] line {} col {}: {}",
                                e.line, e.column, e.msg);
                            eprintln!("{msg}");
                            interp.console_log.borrow_mut()
                                .push(("error".into(), msg));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[lexer error] {e}");
                    interp.console_log.borrow_mut()
                        .push(("error".into(), format!("[lexer error] {e}")));
                }
            }
        }
    }

    /// Zmena velikosti viewportu. Trigger relayout pri pristim `render()`.
    /// Pokud Engine ma GPU, realokuje offscreen RT na novou velikost.
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        let size_changed = (self.viewport_w as u32) != width || (self.viewport_h as u32) != height;
        self.viewport_w = width as f32;
        self.viewport_h = height as f32;
        self.scale_factor = scale_factor;
        self.dirty = true;
        if size_changed {
            self.ensure_target_texture();
            // BUG fix: realokace target_texture znamena ze stara texture s obsahem
            // je dropped + new EMPTY texture alokovana. Vsechny paint/layout caches
            // referenci ZASTARALY content -> seda plocha pri reuse.
            // Invalidate paint cache aby pristim render full pipeline naplnil
            // novou texturu.
            self.last_paint_fingerprint = None;
            self.layout_cache_key = None;
            self.cascade_cache_key = None;
            // Dispatch window 'resize' event do JS po skutecne zmene size.
            if let Some(interp) = self.interpreter.as_mut() {
                interp.dispatch_window_event("resize", crate::interpreter::JsValue::Undefined);
            }
        }
    }

    /// Realokuje `target_texture` + `target_view` na aktualni viewport.
    /// Pokud Engine je headless (no GPU), no-op (target_* zustanou None).
    /// Pouziti: vola se po `resize` + pri prvnim `render` pokud target chybi.
    pub(crate) fn ensure_target_texture(&mut self) {
        let device = match self.engine.device.as_ref() {
            Some(d) => d.clone(),
            None => return, // headless engine - skip
        };
        // viewport_w/h jsou ulozeny jako LOGICAL CSS px. RT velikost MUSI
        // byt v PHYSICAL px (= logical * scale_factor) aby match renderer
        // surface config (NDC mapping pouziva renderer.config.width physical).
        let w = ((self.viewport_w * self.scale_factor) as u32).max(1);
        let h = ((self.viewport_h * self.scale_factor) as u32).max(1);
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rwe-webview-offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.target_texture = Some(tex);
        self.target_view = Some(view);
    }

    /// L2 compositor: alokuj/reuse offscreen Texture pro danou layer.
    /// Klic = layer.id (root box node_ptr). Pri shode size reuse, jinak realloc.
    /// Velikost v PHYSICAL px (= logical * scale_factor).
    /// Vraci Option<()> = None pokud engine headless. Texture + view ulozeny
    /// v self.layer_textures pres klic.
    /// Vraci true kdyz texturu (re)vytvorila (size change) - caller pak MUSI
    /// vrstvu re-rastrovat i kdyz neni damaged (jinak nova prazdna texture =
    /// vanish; typicky pri scale spring overshoot co osciluje raster_scale).
    pub(crate) fn ensure_layer_texture(
        &mut self,
        layer_id: usize,
        logical_w: f32,
        logical_h: f32,
        raster_scale: f32,
    ) -> bool {
        let device = match self.engine.device.as_ref() {
            Some(d) => d.clone(),
            None => return false,
        };
        // Layer phys = logical * zoom * scale_factor. Bez zoom multiplier by
        // pri zoom 1.5x byl layer tex stored at base size. Atlas glyph raster
        // at zoom-scaled size -> compose downsamples to layer tex -> SOFT.
        // S zoom v scale: layer phys matches atlas raster scale = 1:1 sharp.
        // raster_scale (>=1) = boost pro layery se scale(N) transformem - texture
        // je N x vetsi aby compose (ktery quad scaluje N x) samploval 1:1 = ostry
        // text. Cap aby velke scale neudelaly nesmyslne velkou texturu.
        let combined = self.zoom * self.scale_factor * raster_scale.clamp(1.0, 4.0);
        let phys_w = ((logical_w * combined) as u32).max(1);
        let phys_h = ((logical_h * combined) as u32).max(1);

        // Reuse pri shode size.
        if let Some(slot) = self.layer_textures.get(&layer_id) {
            if slot.width == phys_w && slot.height == phys_h {
                return false;
            }
        }

        // Alloc nova (replace pripadnou starou).
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rwe-layer-offscreen"),
            size: wgpu::Extent3d { width: phys_w, height: phys_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.layer_textures.insert(layer_id, LayerTextureSlot {
            texture: tex,
            view,
            width: phys_w,
            height: phys_h,
            last_paint_fp: 0,
        });
        true
    }

    /// Garbage collect layer_textures - drop entries jejichz layer_id neni v
    /// current_layers set. Volat po extract_layer_tree v render_via.
    /// Bez GC: layer_textures roste pri DOM mutaci (smazane elementy ale
    /// jejich texture cache zustava). Take GC paint cache - smazani layer
    /// = uvolnit commands.
    pub(crate) fn gc_layer_textures(&mut self, alive_layer_ids: &std::collections::HashSet<usize>) {
        self.layer_textures.retain(|k, _| alive_layer_ids.contains(k));
        self.layer_paint_cache.retain(|k, _| alive_layer_ids.contains(k));
        self.prev_layer_fingerprints.retain(|k, _| alive_layer_ids.contains(k));
        // Tile textures take vazane na layer_id - drop entries kde layer mrtvy.
        self.tile_textures.retain(|(lid, _), _| alive_layer_ids.contains(lid));
    }

    /// Allokuje (nebo reuses) per-tile texture pro `(layer_id, tile_idx)`.
    /// Tile dimensions v logical CSS px (tile.local_rect.width/height).
    /// Pri shode size: no-op. Pri size change: realloc.
    ///
    /// Cast priority 5 (tile-based rasterization). Volana pres
    /// `ensure_tile_textures_for_layer` po extract_layer_tree.
    pub(crate) fn ensure_tile_texture(
        &mut self,
        layer_id: usize,
        tile_idx: usize,
        logical_w: f32,
        logical_h: f32,
    ) -> Option<()> {
        let device = self.engine.device.as_ref()?.clone();
        // Tile phys s zoom (same jako layer texture) - bez zoom by atlas raster
        // != tile tex scale = bilinear downsample = blur.
        let combined = self.zoom * self.scale_factor;
        let phys_w = ((logical_w * combined) as u32).max(1);
        let phys_h = ((logical_h * combined) as u32).max(1);
        let key = (layer_id, tile_idx);
        if let Some(slot) = self.tile_textures.get(&key) {
            if slot.width == phys_w && slot.height == phys_h {
                return Some(());
            }
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rwe-tile-offscreen"),
            size: wgpu::Extent3d { width: phys_w, height: phys_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.tile_textures.insert(key, TileTextureSlot {
            texture: tex,
            view,
            width: phys_w,
            height: phys_h,
            last_paint_fp: 0,
        });
        Some(())
    }

    /// Allokuje tile textures pro vsechny dirty tiles v layer. No-op pro
    /// tiles s `dirty=false` (texture reuse). Volana ve smycce per damaged
    /// layer v render_via.
    pub(crate) fn ensure_tile_textures_for_layer(
        &mut self,
        layer: &crate::browser::compositor::LayerNode,
    ) {
        for (idx, tile) in layer.tiles.iter().enumerate() {
            if !tile.dirty {
                // Hot path - reuse exist texture pokud size match. Pokud
                // texture neexistuje (prvni navsteva), allokuj.
                let key = (layer.id, idx);
                if self.tile_textures.contains_key(&key) {
                    continue;
                }
            }
            let _ = self.ensure_tile_texture(
                layer.id, idx,
                tile.local_rect.width.max(1.0),
                tile.local_rect.height.max(1.0),
            );
        }
    }

    /// Diagnostika - pocet tile textures v cache.
    pub fn tile_texture_count(&self) -> usize {
        self.tile_textures.len()
    }

    /// Diagnostika - aktualni VRAM footprint tile cache v bytes (4 bpp BGRA).
    pub fn tile_texture_bytes(&self) -> u64 {
        self.tile_textures.values()
            .map(|s| s.width as u64 * s.height as u64 * 4)
            .sum()
    }

    /// Zpracuj input event. Vrati `EventResponse` se zmenami pro hostujici
    /// aplikaci (dirty flag, cursor change, navigation request, ...).
    ///
    /// Phase 5 minimal implementacne: scroll + mouse move + resize. Click/key
    /// dispatch do JS event listeneru = Phase 99 (vyzaduje hit-test pres
    /// layout tree + DOM addEventListener registry).
    /// Spusti / retarget smooth scroll animation Y axis. Pri rapid wheel se
    /// velocity z prev anim preserves (= acceleration feel).
    /// Pres `scroll_anim::retarget_scroll`.
    fn start_scroll_anim_y(&mut self, target: f32) {
        let now = std::time::Instant::now();
        self.scroll_target_y = target;
        self.scroll_anim_y = crate::browser::scroll_anim::retarget_scroll(
            self.scroll_y, target, now, self.scroll_anim_y.as_ref());
        if self.scroll_anim_y.is_none() {
            self.scroll_y = target;
        }
    }
    fn start_scroll_anim_x(&mut self, target: f32) {
        let now = std::time::Instant::now();
        self.scroll_target_x = target;
        self.scroll_anim_x = crate::browser::scroll_anim::retarget_scroll(
            self.scroll_x, target, now, self.scroll_anim_x.as_ref());
        if self.scroll_anim_x.is_none() {
            self.scroll_x = target;
        }
    }

    /// Dispatch 'wheel' event do JS interpretu. Vrati true pokud listener
    /// zavolal preventDefault (= skip native scroll). Hit-testne target node
    /// pod kurzorem, vyrobi WheelEvent obj s deltaX/Y + clientX/Y, projde
    /// chain.
    fn dispatch_wheel_event(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        let content_x = x + self.scroll_x;
        let content_y = y + self.scroll_y;
        let target = match self.last_layout_root.as_ref()
            .and_then(|r| r.hit_test(content_x, content_y))
            .and_then(|bx| bx.node.clone()) {
            Some(n) => n,
            None => return false,
        };
        let interp = match self.interpreter.as_mut() { Some(i) => i, None => return false };
        let mut event = crate::interpreter::JsObject::new();
        event.set("type".into(), crate::interpreter::JsValue::Str("wheel".into()));
        event.set("deltaX".into(), crate::interpreter::JsValue::Number(dx as f64));
        event.set("deltaY".into(), crate::interpreter::JsValue::Number(dy as f64));
        event.set("deltaZ".into(), crate::interpreter::JsValue::Number(0.0));
        event.set("deltaMode".into(), crate::interpreter::JsValue::Number(0.0)); // 0 = pixel
        event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
        event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
        event.set("target".into(), crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(&target)));
        event.set("bubbles".into(), crate::interpreter::JsValue::Bool(true));
        event.set("cancelable".into(), crate::interpreter::JsValue::Bool(true));
        let event_rc = std::rc::Rc::new(std::cell::RefCell::new(event));
        let event_val = crate::interpreter::JsValue::Object(std::rc::Rc::clone(&event_rc));
        let _ = interp.dispatch_event(&target, "wheel", event_val);
        // Check defaultPrevented po dispatchu.
        matches!(event_rc.borrow().get("defaultPrevented"),
            crate::interpreter::JsValue::Bool(true))
    }

    /// Kbd scroll dispatcher - smerue dy do inner scrollable elementu pod
    /// kurzorem; pokud zadny match, fallback viewport scroll_target_y.
    /// Vola se z shell pri PageUp/Down, Arrow up/down, Home, End, Space.
    pub fn kbd_scroll_y(&mut self, dy: f32) -> bool {
        use crate::browser::scroll::{ScrollableMut, ElementScroll};
        let viewport_w = self.viewport_w / self.zoom.max(0.01);
        let target = self.find_scroll_target(self.mouse_x, self.mouse_y, 0.0, dy);
        if let Some(node_id) = target {
            if let Some(root) = &self.last_layout_root {
                if let Some((rw, rh, cw, ch)) = find_box_dims(root, node_id) {
                    let mut h = ElementScroll {
                        map: &mut self.element_scroll,
                        node_id, rect_w: rw, rect_h: rh,
                        content_w: cw, content_h: ch,
                    };
                    h.scroll_by(0.0, dy);
                    self.dirty = true;
                    return true;
                }
            }
        }
        let viewport_h = self.viewport_h / self.zoom.max(0.01);
        let _ = viewport_w;
        let (_content_w, content_h) = match &self.last_layout_root {
            Some(l) => (l.rect.width, l.rect.height),
            None => (f32::INFINITY, f32::INFINITY),
        };
        let max_y = (content_h - viewport_h).max(0.0);
        let new_target = (self.scroll_target_y + dy).clamp(0.0, max_y);
        self.start_scroll_anim_y(new_target);
        self.dirty = true;
        true
    }

    /// Wheel scroll: zkus scrollnout INNER overflow kontejner pod kurzorem.
    /// Vrati true = scrollnuto inner (App nesmi scrollovat stranku). false =
    /// zadny inner kontejner pod kurzorem (App scrolluje stranku). Bez tohoto
    /// wheel vzdy scrolloval stranku i nad overflow:auto sekci -> inner scrollbar
    /// thumb se nehybal + "scrolluju strankou misto sekce".
    pub fn try_inner_wheel_scroll(&mut self, mx: f32, my: f32, dy: f32) -> bool {
        use crate::browser::scroll::{ScrollableMut, ElementScroll};
        let target = self.find_scroll_target(mx, my, 0.0, dy);
        if std::env::var("RWE_SCROLL_DBG").is_ok() {
            eprintln!("[INNER] find_scroll_target(mx={:.0},my={:.0},dy={:.1}) = {:?}", mx, my, dy, target);
        }
        if let Some(node_id) = target {
            if let Some(root) = &self.last_layout_root {
                if let Some((rw, rh, cw, ch)) = find_box_dims(root, node_id) {
                    let mut h = ElementScroll {
                        map: &mut self.element_scroll,
                        node_id, rect_w: rw, rect_h: rh,
                        content_w: cw, content_h: ch,
                    };
                    h.scroll_by(0.0, dy);
                    self.dirty = true;
                    return true;
                }
            }
        }
        false
    }

    /// Hit-test layout pres mouse (logical px) + walk up ancestors. Vrati
    /// node_id prvniho scrollable predka kdyz scrolly v dany smer (dx/dy)
    /// jeste zbyva room. Pouziva trait `Scrollable::has_room`. Inak None =
    /// fallback viewport scroll.
    fn find_scroll_target(&self, mx: f32, my: f32, dx: f32, dy: f32) -> Option<usize> {
        use crate::browser::scroll::Scrollable;
        let root = self.last_layout_root.as_ref()?;
        let content_x = mx + self.scroll_x;
        let content_y = my + self.scroll_y;
        fn collect_path<'a>(bx: &'a crate::browser::layout::LayoutBox, x: f32, y: f32,
                            out: &mut Vec<&'a crate::browser::layout::LayoutBox>) {
            if x < bx.rect.x || y < bx.rect.y
                || x > bx.rect.x + bx.rect.width
                || y > bx.rect.y + bx.rect.height { return; }
            out.push(bx);
            let cx = x + bx.scroll_offset_x;
            let cy = y + bx.scroll_offset_y;
            for ch in &bx.children {
                collect_path(ch, cx, cy, out);
            }
        }
        let mut path: Vec<&crate::browser::layout::LayoutBox> = Vec::new();
        collect_path(root, content_x, content_y, &mut path);
        for bx in path.iter().rev() {
            if !bx.needs_scrollbar_y() && !bx.needs_scrollbar_x() { continue; }
            // `?` (early-return None) shodil cele hledani kdyz scrollable box nemel
            // DOM node (anonymni box) -> wheel pak scrolloval STRANKU misto inner
            // elementu. continue = preskoc jen tenhle box, hledej dal v ceste.
            let node = match bx.node.as_ref() { Some(n) => n, None => continue };
            let node_id = std::rc::Rc::as_ptr(node) as usize;
            // Construct read-only handle pres trait has_room. Map iz parent self,
            // pro read jen .get(node_id) - vsechno z snapshot.
            let (sx, sy) = self.element_scroll.get(&node_id).copied().unwrap_or((0.0, 0.0));
            let (mx, my) = (
                (bx.inner_content_w - bx.rect.width).max(0.0),
                (bx.inner_content_h - bx.rect.height).max(0.0),
            );
            let room_y = (dy > 0.0 && sy < my) || (dy < 0.0 && sy > 0.0);
            let room_x = (dx > 0.0 && sx < mx) || (dx < 0.0 && sx > 0.0);
            if (dy != 0.0 && bx.needs_scrollbar_y() && room_y)
                || (dx != 0.0 && bx.needs_scrollbar_x() && room_x) {
                return Some(node_id);
            }
        }
        None
    }

    pub fn handle_input(&mut self, event: InputEvent) -> EventResponse {
        let mut response = EventResponse::default();
        match event {
            InputEvent::Scroll { dx, dy, x, y, .. } => {
                use crate::browser::scroll::{ScrollableMut, ElementScroll};
                self.mouse_x = x;
                self.mouse_y = y;
                let viewport_h = self.viewport_h / self.zoom.max(0.01);
                let viewport_w = self.viewport_w / self.zoom.max(0.01);
                // Dispatch 'wheel' event do JS pred native scroll. Pokud listener
                // zavola event.preventDefault() -> skip scroll.
                // Inspired by Chromium core/input/event_handler.cc - wheel je JS
                // event first, native scroll second.
                let prevented = self.dispatch_wheel_event(x, y, dx, dy);
                if std::env::var("RWE_SCROLL_DBG").is_ok() {
                    eprintln!("[SCROLL] x={x} y={y} dx={dx} dy={dy} prevented={prevented}");
                }
                if prevented {
                    self.dirty = true;
                    response.dirty = true;
                    return response;
                }
                let target = self.find_scroll_target(x, y, dx, dy);
                if std::env::var("RWE_SCROLL_DBG").is_ok() {
                    eprintln!("[SCROLL] target={:?}", target);
                }
                if let Some(node_id) = target {
                    // Najdi bx pres path scan zatim - pro dim. Path lookup z layout_root.
                    if let Some(root) = &self.last_layout_root {
                        if let Some((rw, rh, cw, ch)) = find_box_dims(root, node_id) {
                            let mut handle = ElementScroll {
                                map: &mut self.element_scroll,
                                node_id,
                                rect_w: rw, rect_h: rh,
                                content_w: cw, content_h: ch,
                            };
                            handle.scroll_by(dx, dy);
                        }
                    }
                } else {
                    // Viewport smooth scroll - start cubic-bezier animation.
                    // Target = clamped(current_target + delta). Pri novem wheel
                    // pred dokoncenim animation = retarget z aktualniho scroll_y.
                    let (content_w, content_h) = match &self.last_layout_root {
                        Some(l) => (l.rect.width, l.rect.height),
                        None => (f32::INFINITY, f32::INFINITY),
                    };
                    let max_y = (content_h - viewport_h).max(0.0);
                    let max_x = (content_w - viewport_w).max(0.0);
                    let new_target_y = (self.scroll_target_y + dy).clamp(0.0, max_y);
                    let new_target_x = (self.scroll_target_x + dx).clamp(0.0, max_x);
                    if (new_target_y - self.scroll_y).abs() > 0.5 {
                        self.start_scroll_anim_y(new_target_y);
                    }
                    if (new_target_x - self.scroll_x).abs() > 0.5 {
                        self.start_scroll_anim_x(new_target_x);
                    }
                }
                self.dirty = true;
                response.dirty = true;
            }
            InputEvent::MouseMove { x, y, .. } => {
                if (self.mouse_x - x).abs() > 0.5 || (self.mouse_y - y).abs() > 0.5 {
                    self.mouse_x = x;
                    self.mouse_y = y;
                    // Inner scrollbar drag - prepocet thumb pos -> element_scroll.
                    let cx_inner = x + self.scroll_x;
                    let cy_inner = y + self.scroll_y;
                    // Element resize drag: prepocet novou sirku/vysku z mouse delta,
                    // zapis jako inline style override -> reflow -> ResizeObserver fire.
                    if let Some((node, sx, sy, sw, sh, axis)) = self.resize_drag.clone() {
                        let horiz = axis == "both" || axis == "horizontal";
                        let vert = axis == "both" || axis == "vertical";
                        if horiz {
                            let nw = (sw + (cx_inner - sx)).max(24.0);
                            set_inline_style_prop(&node, "width", &format!("{}px", nw.round()));
                        }
                        if vert {
                            let nh = (sh + (cy_inner - sy)).max(24.0);
                            set_inline_style_prop(&node, "height", &format!("{}px", nh.round()));
                        }
                        if let Some(interp) = self.interpreter.as_mut() { interp.bump_dom_version(); }
                        self.hit_rtree = None;
                        self.dirty = true;
                        response.dirty = true;
                        return response;
                    }
                    // Range drag: drzime-li range thumb (mousedown na range), nastav
                    // value dle x pozice (set_range_from_x fire input/change).
                    if let Some(rn) = self.range_drag_node.clone() {
                        self.set_range_from_x(&rn, cx_inner);
                    }
                    if let Some((node_id, grab_y, max_y, bar_y, bar_h)) = self.inner_v_drag {
                        // Thumb size invariant: vyresit z bar_h + max_scroll/vh - ale
                        // jednodusší: thumb_h dynamic, recompute. Pouzij stored max_y
                        // jako auth source; thumb_h = bar_h * (bar_h / (bar_h + max_y))
                        // (= viewport / content)... ale potreba viewport_h boxu.
                        // bar_h = rect.height (= visible viewport per element).
                        // content_h = max_y + bar_h -> thumb_h = bar_h * bar_h / (max_y + bar_h).
                        let content_h = max_y + bar_h;
                        let thumb_h = (bar_h * bar_h / content_h).max(30.0);
                        let track = (bar_h - thumb_h).max(1.0);
                        let new_thumb_top = (cy_inner - grab_y - bar_y).clamp(0.0, track);
                        let new_scroll = (new_thumb_top / track) * max_y;
                        let (sx, _) = self.element_scroll.get(&node_id).copied().unwrap_or((0.0, 0.0));
                        self.element_scroll.insert(node_id, (sx, new_scroll));
                        self.dirty = true;
                        response.dirty = true;
                        return response;
                    }
                    if let Some((node_id, grab_x, max_x, bar_x, bar_w)) = self.inner_h_drag {
                        let content_w = max_x + bar_w;
                        let thumb_w = (bar_w * bar_w / content_w).max(30.0);
                        let track = (bar_w - thumb_w).max(1.0);
                        let new_thumb_left = (cx_inner - grab_x - bar_x).clamp(0.0, track);
                        let new_scroll = (new_thumb_left / track) * max_x;
                        let (_, sy) = self.element_scroll.get(&node_id).copied().unwrap_or((0.0, 0.0));
                        self.element_scroll.insert(node_id, (new_scroll, sy));
                        self.dirty = true;
                        response.dirty = true;
                        return response;
                    }
                    // Scrollbar thumb drag - update scroll position pres
                    // mouse pos vs thumb grab offset.
                    let viewport_w = self.viewport_w / self.zoom.max(0.01);
                    let viewport_h = self.viewport_h / self.zoom.max(0.01);
                    if let (Some(grab_y), Some(layout)) = (self.v_scrollbar_drag, &self.last_layout_root) {
                        let total_h = layout.rect.height;
                        if total_h > viewport_h {
                            let thumb_h = (viewport_h * viewport_h / total_h).max(40.0);
                            let track_h = viewport_h - thumb_h;
                            let new_thumb_y = (y - grab_y).max(0.0).min(track_h);
                            let max_scroll = total_h - viewport_h;
                            let new_scroll = (new_thumb_y / track_h) * max_scroll;
                            self.scroll_y = new_scroll;
                            self.scroll_target_y = new_scroll;
                            self.dirty = true;
                            response.dirty = true;
                            return response;
                        }
                    }
                    if let (Some(grab_x), Some(layout)) = (self.h_scrollbar_drag, &self.last_layout_root) {
                        let total_w = layout.rect.width;
                        if total_w > viewport_w {
                            let thumb_w = (viewport_w * viewport_w / total_w).max(40.0);
                            let track_w = viewport_w - thumb_w;
                            let new_thumb_x = (x - grab_x).max(0.0).min(track_w);
                            let max_scroll_x = total_w - viewport_w;
                            let new_scroll = (new_thumb_x / track_w) * max_scroll_x;
                            self.scroll_x = new_scroll;
                            self.scroll_target_x = new_scroll;
                            self.dirty = true;
                            response.dirty = true;
                            return response;
                        }
                    }
                    // Hit-test layout_root pres content coords -> :hover state.
                    // Cache: pri stejne 2px-mrizce a dom_version reuse posledni
                    // hovered_id (bez tree walk). PERF: R-tree query O(log N) misto
                    // O(N) tree walk. Pri transformed/fixed subtree fallback na
                    // klasicky tree walk (NeedsFallback).
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    let dom_v = self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0);
                    let hit_key = ((content_x / 2.0) as i32, (content_y / 2.0) as i32, dom_v);
                    let (hit_id, hit_tag, hit_over_text) = {
                        use crate::browser::spatial_hit::{hit_test_point, HitResult};
                        // Lazy build R-tree pri prvnim dotazu po layout zmene
                        // (render invaliduje na None). Build z last_layout_root
                        // (clean clone, same jako build-time layout_root).
                        if self.hit_rtree.is_none() {
                            if let Some(root) = self.last_layout_root.as_ref() {
                                self.hit_rtree = Some(
                                    crate::browser::spatial_hit::build_hit_rtree(root));
                            }
                        }
                        let from_rtree = self.hit_rtree.as_ref()
                            .map(|t| hit_test_point(t, content_x, content_y));
                        match from_rtree {
                            Some(HitResult::Hit(info)) => (
                                Some(info.node_ptr),
                                info.tag,
                                info.has_text,
                            ),
                            Some(HitResult::Miss) => (None, None, false),
                            // Pri NeedsFallback (transform/fixed subtree) ANO nebo
                            // pri No-Rtree fallne na klasicky tree walk.
                            _ => {
                                let hit_box = self.last_layout_root.as_ref()
                                    .and_then(|root| root.hit_test(content_x, content_y));
                                let id = hit_box
                                    .and_then(|bx| bx.node.as_ref().map(|n|
                                        std::rc::Rc::as_ptr(n) as usize));
                                let tag = hit_box
                                    .and_then(|bx| bx.node.as_ref().map(|n| n.tag_name()))
                                    .flatten();
                                let over_text = hit_box.map(|bx| bx.text.is_some()).unwrap_or(false);
                                (id, tag, over_text)
                            }
                        }
                    };
                    let hovered_id = match &self.hit_test_cache {
                        Some((k, v)) if *k == hit_key => *v,
                        _ => {
                            self.hit_test_cache = Some((hit_key, hit_id));
                            hit_id
                        }
                    };
                    // Per-WebView hovered. Bez per-WV stav by mouse_move v
                    // jine WV invalidoval cascade cache teto WV (thread_local).
                    // P2 fix: dirty=true JEN POKUD prev nebo new hovered ma
                    // :hover effect (v hover_affected_set). Bez tohoto mass
                    // mouse_move pres devtools-frontend (5857 nodes) =
                    // mass cascade walks (60ms/walk) i kdyz hover effect je
                    // jen na nekolik elementu.
                    if self.hovered_node_local != hovered_id {
                        let prev_id = self.hovered_node_local;
                        self.hovered_node_local = hovered_id;
                        let prev_affected = prev_id
                            .map(|p| self.hover_affected_set.contains(&p))
                            .unwrap_or(false);
                        let new_affected = hovered_id
                            .map(|n| self.hover_affected_set.contains(&n))
                            .unwrap_or(false);
                        if prev_affected || new_affected {
                            self.dirty = true;
                            response.dirty = true;
                        }
                        // Fire mouseleave (prev) + mouseenter (cur) DOM events.
                        // Bez tohoto JS handlers (= devtools tree hover -> CDP
                        // Overlay.highlightNode) nikdy nevykonaji. Chrome/FF
                        // semantika: non-bubbling, fire pres target az root.
                        let make_evt = |x_pos: f32, y_pos: f32, ty: &str, t: &std::rc::Rc<crate::browser::dom::Node>| {
                            let mut event = crate::interpreter::JsObject::new();
                            event.set("type".into(), crate::interpreter::JsValue::Str(ty.into()));
                            event.set("clientX".into(), crate::interpreter::JsValue::Number(x_pos as f64));
                            event.set("clientY".into(), crate::interpreter::JsValue::Number(y_pos as f64));
                            event.set("target".into(), crate::interpreter::JsValue::DomNode(
                                std::rc::Rc::clone(t)));
                            crate::interpreter::JsValue::Object(
                                std::rc::Rc::new(std::cell::RefCell::new(event)))
                        };
                        if let Some(root) = self.last_layout_root.as_ref() {
                            let prev_target = prev_id.and_then(|p|
                                crate::browser::paint::find_box_by_node_id(root, p)
                                    .and_then(|bx| bx.node.clone()));
                            let cur_target = hovered_id.and_then(|n|
                                crate::browser::paint::find_box_by_node_id(root, n)
                                    .and_then(|bx| bx.node.clone()));
                            if let Some(interp) = self.interpreter.as_mut() {
                                if let Some(t) = prev_target {
                                    let e1 = make_evt(x, y, "mouseleave", &t);
                                    let e2 = make_evt(x, y, "mouseout", &t);
                                    let _ = interp.dispatch_event(&t, "mouseleave", e1);
                                    let _ = interp.dispatch_event(&t, "mouseout", e2);
                                }
                                if let Some(t) = cur_target {
                                    let e1 = make_evt(x, y, "mouseenter", &t);
                                    let e2 = make_evt(x, y, "mouseover", &t);
                                    let _ = interp.dispatch_event(&t, "mouseenter", e1);
                                    let _ = interp.dispatch_event(&t, "mouseover", e2);
                                }
                            }
                        }
                    }
                    // Fire mousemove pres cur target (= kazda mouse move pos).
                    // Vc. offsetX/offsetY (pozice v target boxu) - canvas kresleni
                    // + pozicni UI to ctou (engine-test). Box nalezen 1x, offset
                    // spocitan inline (scroll locals = bez double self borrow).
                    let (sxp, syp) = (self.scroll_x, self.scroll_y);
                    let mm = self.last_layout_root.as_ref().and_then(|root| {
                        hovered_id.and_then(|n|
                            crate::browser::paint::find_box_by_node_id(root, n).map(|bx| {
                                (bx.node.clone(),
                                 (x + sxp - bx.rect.x) as f64,
                                 (y + syp - bx.rect.y) as f64)
                            }))
                    });
                    if let Some((Some(t), ox, oy)) = mm {
                        if let Some(interp) = self.interpreter.as_mut() {
                            let mut event = crate::interpreter::JsObject::new();
                            event.set("type".into(), crate::interpreter::JsValue::Str("mousemove".into()));
                            event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                            event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
                            event.set("offsetX".into(), crate::interpreter::JsValue::Number(ox));
                            event.set("offsetY".into(), crate::interpreter::JsValue::Number(oy));
                            event.set("target".into(), crate::interpreter::JsValue::DomNode(
                                std::rc::Rc::clone(&t)));
                            let event_val = crate::interpreter::JsValue::Object(
                                std::rc::Rc::new(std::cell::RefCell::new(event)));
                            // Canvas freehand: handler pushne kreslici ops, ale ty
                            // nebumpnou dom_version -> bez dirty by se novy tah
                            // neprekreslil. Porovnej pocet ops pred/po dispatch.
                            let ops_before: usize = interp.canvas_ops.borrow()
                                .values().map(|v| v.len()).sum();
                            let _ = interp.dispatch_event(&t, "mousemove", event_val);
                            let ops_after: usize = interp.canvas_ops.borrow()
                                .values().map(|v| v.len()).sum();
                            if ops_after != ops_before {
                                self.dirty = true;
                                response.dirty = true;
                            }
                        }
                    }
                    if self.open_select.is_some() {
                        self.dirty = true;
                        response.dirty = true;
                    }
                    // Update text selection drag.
                    if self.sel_dragging() {
                        self.sel_update(content_x, content_y);
                        self.dirty = true;
                        response.dirty = true;
                    }
                    // Editor drag selection - pri mouse hold po MouseDown na
                    // input/textarea: update caret + extend selection anchor.
                    if let Some((_dx, _dy, down_node)) = self.mouse_down_at.clone() {
                        let is_input = matches!(down_node.tag_name().as_deref(),
                            Some("input") | Some("textarea"));
                        if is_input {
                            self.editor_hit_test_input(&down_node, x, true);
                            self.dirty = true;
                            response.dirty = true;
                        }
                    }
                    // Cursor icon: nejdriv CSS `cursor` property (LayoutBox.cursor,
                    // inherited - span pod divem dedi). Az fallback na tag/text.
                    // Drive se CSS cursor ignoroval -> `cursor:text` na divu se
                    // zahodil, I-beam jen nad spany s textem = "jen nekdy spravne".
                    let css_cursor = hit_id.and_then(|nid| self.last_layout_root.as_ref()
                        .and_then(|root| crate::browser::paint::find_box_by_node_id(root, nid))
                        .and_then(|bx| bx.cursor.clone()));
                    let fallback = |hit_tag: &Option<String>, hit_over_text: bool| {
                        match hit_tag.as_deref() {
                            Some("a") | Some("button") => crate::embed::CursorIcon::Pointer,
                            Some("input") | Some("textarea") => crate::embed::CursorIcon::Text,
                            _ => if hit_over_text { crate::embed::CursorIcon::Text }
                                 else { crate::embed::CursorIcon::Default },
                        }
                    };
                    response.cursor = Some(match css_cursor.as_deref() {
                        Some("pointer") => crate::embed::CursorIcon::Pointer,
                        Some("text") | Some("vertical-text") => crate::embed::CursorIcon::Text,
                        Some("wait") | Some("progress") => crate::embed::CursorIcon::Wait,
                        Some("help") => crate::embed::CursorIcon::Help,
                        Some("crosshair") | Some("cell") => crate::embed::CursorIcon::Crosshair,
                        Some("move") | Some("all-scroll") => crate::embed::CursorIcon::Move,
                        Some("not-allowed") | Some("no-drop") => crate::embed::CursorIcon::NotAllowed,
                        Some("grab") => crate::embed::CursorIcon::Grab,
                        Some("grabbing") => crate::embed::CursorIcon::Grabbing,
                        Some("ew-resize") | Some("col-resize") | Some("e-resize") | Some("w-resize")
                            => crate::embed::CursorIcon::ResizeEw,
                        Some("ns-resize") | Some("row-resize") | Some("n-resize") | Some("s-resize")
                            => crate::embed::CursorIcon::ResizeNs,
                        Some("nesw-resize") | Some("ne-resize") | Some("sw-resize")
                            => crate::embed::CursorIcon::ResizeNesw,
                        Some("nwse-resize") | Some("nw-resize") | Some("se-resize")
                            => crate::embed::CursorIcon::ResizeNwse,
                        Some("default") | Some("auto") | None => fallback(&hit_tag, hit_over_text),
                        _ => fallback(&hit_tag, hit_over_text),
                    });
                }
            }
            InputEvent::MouseDown { x, y, button, .. } => {
                if matches!(button, crate::embed::MouseButton::Left) {
                    // Otevreny <select> popup: vyhodnot klik PRED beznym hit-testem.
                    // Klik na option -> pick + zavri; klik mimo -> jen zavri. Popup
                    // geometrie musi sedet s render (find_node_by_ptr branch).
                    if let Some((sel_id, ax, ay, aw)) = self.open_select {
                        let opt_h = 24.0_f32;
                        let popup_x = ax;
                        let popup_y = ay + 24.0 - self.scroll_y;
                        let sel_node = self.interpreter.as_ref().and_then(|interp| {
                            let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                            crate::browser::render::find_node_by_ptr(&doc_root, sel_id)
                        });
                        let n_opts = sel_node.as_ref().map(|n| n.children.borrow().iter()
                            .filter(|c| c.tag_name().as_deref() == Some("option")).count()).unwrap_or(0);
                        let popup_h = opt_h * n_opts as f32;
                        if x >= popup_x && x < popup_x + aw && y >= popup_y && y < popup_y + popup_h {
                            let idx = ((y - popup_y) / opt_h).floor().max(0.0) as usize;
                            if let Some(sn) = sel_node { self.select_pick_option(&sn, idx); }
                        }
                        self.open_select = None;
                        response.dirty = true;
                        self.dirty = true;
                        return response;
                    }
                    // Scrollbar thumb hit-test PRED page hit-test.
                    let viewport_w = self.viewport_w / self.zoom.max(0.01);
                    let viewport_h = self.viewport_h / self.zoom.max(0.01);
                    if let Some(layout) = &self.last_layout_root {
                        let total_h = layout.rect.height;
                        let total_w = layout.rect.width;
                        // Vertical scrollbar - thumb drag nebo track jump.
                        if total_h > viewport_h && x >= viewport_w - 12.0 && x < viewport_w {
                            let thumb_h = (viewport_h * viewport_h / total_h).max(40.0);
                            let max_scroll = (total_h - viewport_h).max(1.0);
                            let thumb_y = (self.scroll_y / max_scroll) * (viewport_h - thumb_h);
                            if y >= thumb_y && y < thumb_y + thumb_h {
                                // Klik na thumb -> drag (instantni, bez smooth).
                                self.v_scrollbar_drag = Some(y - thumb_y);
                            } else {
                                // Klik na track mimo thumb -> page jump.
                                // y nad thumb = scroll up viewport_h, pod = down.
                                let delta = if y < thumb_y { -viewport_h } else { viewport_h };
                                let new_scroll = (self.scroll_y + delta).clamp(0.0, max_scroll);
                                self.scroll_y = new_scroll;
                                self.scroll_target_y = new_scroll;
                            }
                            response.dirty = true;
                            self.dirty = true;
                            return response;
                        }
                        // Horizontal scrollbar - thumb drag nebo track jump.
                        if total_w > viewport_w && y >= viewport_h - 12.0 && y < viewport_h {
                            let thumb_w = (viewport_w * viewport_w / total_w).max(40.0);
                            let max_scroll_x = (total_w - viewport_w).max(1.0);
                            let thumb_x = (self.scroll_x / max_scroll_x) * (viewport_w - thumb_w);
                            if x >= thumb_x && x < thumb_x + thumb_w {
                                self.h_scrollbar_drag = Some(x - thumb_x);
                            } else {
                                let delta = if x < thumb_x { -viewport_w } else { viewport_w };
                                let new_scroll = (self.scroll_x + delta).clamp(0.0, max_scroll_x);
                                self.scroll_x = new_scroll;
                                self.scroll_target_x = new_scroll;
                            }
                            response.dirty = true;
                            self.dirty = true;
                            return response;
                        }
                    }
                    // Inner scrollbar thumb/track drag - walk path k nejhlubsimu
                    // scrollable boxu pod kurzorem. Bar je vpravo dole rect.
                    use crate::browser::scroll::Scrollable;
                    let cx = x + self.scroll_x;
                    let cy = y + self.scroll_y;
                    let layout = self.last_layout_root.as_ref();
                    if let Some(root) = layout {
                        fn collect_path<'a>(bx: &'a crate::browser::layout::LayoutBox, x: f32, y: f32,
                                            out: &mut Vec<&'a crate::browser::layout::LayoutBox>) {
                            if x < bx.rect.x || y < bx.rect.y
                                || x > bx.rect.x + bx.rect.width
                                || y > bx.rect.y + bx.rect.height { return; }
                            out.push(bx);
                            let cx = x + bx.scroll_offset_x;
                            let cy = y + bx.scroll_offset_y;
                            for ch in &bx.children {
                                collect_path(ch, cx, cy, out);
                            }
                        }
                        let mut path: Vec<&crate::browser::layout::LayoutBox> = Vec::new();
                        collect_path(root, cx, cy, &mut path);
                        // Iter z nejhlubsiho - prvni match wins.
                        let mut handled = false;
                        for bx in path.iter().rev() {
                            let node = match bx.node.as_ref() { Some(n) => n, None => continue };
                            let node_id = std::rc::Rc::as_ptr(node) as usize;
                            // Current scroll offsets z mapy (NE z bx.scroll_offset
                            // - last_layout_root je clean snapshot).
                            let (cur_sx, cur_sy) = self.element_scroll
                                .get(&node_id).copied().unwrap_or((0.0, 0.0));
                            // V scrollbar bar
                            if bx.needs_scrollbar_y() {
                                let bar_w = bx.scrollbar_size.max(8.0).min(14.0);
                                let bar_x = bx.rect.x + bx.rect.width - bar_w;
                                let bar_y = bx.rect.y;
                                let bar_h = bx.rect.height;
                                if cx >= bar_x && cx < bar_x + bar_w
                                    && cy >= bar_y && cy < bar_y + bar_h
                                {
                                    let max_y = (bx.inner_content_h - bx.rect.height).max(0.0);
                                    let content_h = bx.inner_content_h.max(1.0);
                                    let thumb_h = (bar_h * bar_h / content_h).max(30.0);
                                    let track = (bar_h - thumb_h).max(0.0);
                                    let thumb_off = if max_y > 0.0 {
                                        (cur_sy / max_y).clamp(0.0, 1.0) * track
                                    } else { 0.0 };
                                    let thumb_top = bar_y + thumb_off;
                                    if cy >= thumb_top && cy < thumb_top + thumb_h {
                                        self.inner_v_drag = Some((node_id, cy - thumb_top, max_y, bar_y, bar_h));
                                    } else {
                                        let delta = if cy < thumb_top { -bx.rect.height } else { bx.rect.height };
                                        let new_y = (cur_sy + delta).clamp(0.0, max_y);
                                        self.element_scroll.insert(node_id, (cur_sx, new_y));
                                    }
                                    handled = true;
                                    break;
                                }
                            }
                            // H scrollbar bar
                            if bx.needs_scrollbar_x() {
                                let bar_h = bx.scrollbar_size.max(8.0).min(14.0);
                                let bar_y = bx.rect.y + bx.rect.height - bar_h;
                                let bar_x = bx.rect.x;
                                let bar_w = bx.rect.width;
                                if cy >= bar_y && cy < bar_y + bar_h
                                    && cx >= bar_x && cx < bar_x + bar_w
                                {
                                    let max_x = (bx.inner_content_w - bx.rect.width).max(0.0);
                                    let content_w = bx.inner_content_w.max(1.0);
                                    let thumb_w = (bar_w * bar_w / content_w).max(30.0);
                                    let track = (bar_w - thumb_w).max(0.0);
                                    let thumb_off = if max_x > 0.0 {
                                        (cur_sx / max_x).clamp(0.0, 1.0) * track
                                    } else { 0.0 };
                                    let thumb_left = bar_x + thumb_off;
                                    if cx >= thumb_left && cx < thumb_left + thumb_w {
                                        self.inner_h_drag = Some((node_id, cx - thumb_left, max_x, bar_x, bar_w));
                                    } else {
                                        let delta = if cx < thumb_left { -bx.rect.width } else { bx.rect.width };
                                        let new_x = (cur_sx + delta).clamp(0.0, max_x);
                                        self.element_scroll.insert(node_id, (new_x, cur_sy));
                                    }
                                    handled = true;
                                    break;
                                }
                            }
                        }
                        if handled {
                            response.dirty = true;
                            self.dirty = true;
                            return response;
                        }
                    }
                    // Hit-test layout_root pres content coords. Store target +
                    // pos pro MouseUp click-vs-drag distinguish.
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    // Resize grip (CSS resize): klik do praveho dolniho rohu
                    // resizable elementu -> start resize drag (PRED hit-testem).
                    if let Some((node, w, h, axis)) = self.last_layout_root.as_ref()
                        .and_then(|root| find_resize_grip(root, content_x, content_y)) {
                        self.resize_drag = Some((node, content_x, content_y, w, h, axis));
                        response.dirty = true;
                        self.dirty = true;
                        return response;
                    }
                    let target_node = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.clone())
                        // Klik na <label> -> aktivuj jeho control (focus + toggle).
                        .map(|n| resolve_label_target(&n));
                    // Focus / blur - per-WebView focused state.
                    // [tabindex] musi byt focusable taky - jinak klik na
                    // <div tabindex="0"> nenastavi focus a keydown/keyup
                    // (routovany na focused node) se nikdy nedispatchnou.
                    let old_focus_id = self.focused_node_local;
                    // Walk nahoru k nejblizsimu focusable predkovi - klik na <span>
                    // uvnitr <div tabindex=0> musi focusovat ten div (jinak keydown/
                    // keyup routovany na focused node se nikdy nedispatchnou).
                    let focus_target = target_node.as_ref().and_then(nearest_focusable);
                    let new_id = focus_target.as_ref()
                        .map(|t| std::rc::Rc::as_ptr(t) as usize);
                    self.focused_node_local = new_id;
                    // Cascade global = mirror per-WebView pro :focus styling
                    // (cascade.rs PSEUDO :focus check). Single thread,
                    // posledni MouseDown wins. Multi-WebView problem: posledni
                    // klik prepise styling pro vsechny - akceptace pri F12.
                    crate::browser::cascade::set_focused_node(new_id);
                    // Dispatch blur(stary) + focus(novy) JS event - bez toho se
                    // onfocus/onblur inline handlery (napr. border highlight)
                    // nikdy nezavolaji.
                    if old_focus_id != new_id {
                        if let Some(interp) = self.interpreter.as_mut() {
                            let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                            if let Some(oid) = old_focus_id {
                                if let Some(old_node) = crate::browser::render::find_node_by_ptr(&doc_root, oid) {
                                    let ev = make_focus_event("blur", &old_node);
                                    let _ = interp.dispatch_event(&old_node, "blur", ev);
                                }
                            }
                            if let Some(nid) = new_id {
                                if let Some(new_node) = crate::browser::render::find_node_by_ptr(&doc_root, nid) {
                                    let ev = make_focus_event("focus", &new_node);
                                    let _ = interp.dispatch_event(&new_node, "focus", ev);
                                }
                            }
                        }
                    }
                    // :active pseudo-class - target stisknute mysi (cleared v MouseUp).
                    // Bez toho :active styly (button click efekt, cursor:grabbing)
                    // nikdy nematchnou.
                    crate::browser::cascade::set_active_node(
                        target_node.as_ref().map(|t| std::rc::Rc::as_ptr(t) as usize));
                    // Klik na zavreny <select> -> otevri popup (anchor x=screen,
                    // y=content; sedi s render branch webview.rs find_node_by_ptr).
                    if let Some(target) = target_node.as_ref() {
                        if target.tag_name().as_deref() == Some("select") {
                            let sid = std::rc::Rc::as_ptr(target) as usize;
                            if let Some(bx) = self.last_layout_root.as_ref()
                                .and_then(|r| crate::browser::paint::find_box_by_node_id(r, sid)) {
                                self.open_select = Some((sid, bx.rect.x - self.scroll_x,
                                    bx.rect.y, bx.rect.width));
                                response.dirty = true;
                                self.dirty = true;
                                return response;
                            }
                        }
                    }
                    // Range slider: klik nastavi value dle x pozice + fire input/
                    // change (engine-test range-val span se updatne pres oninput).
                    // Bez tohoto byl range needovladatelny mysi. Nastav drag node
                    // pro nasledny drag v MouseMove.
                    if let Some(target) = target_node.as_ref() {
                        let is_range = target.tag_name().as_deref() == Some("input")
                            && target.attr("type").map(|t| t.eq_ignore_ascii_case("range")).unwrap_or(false);
                        if is_range {
                            self.range_drag_node = Some(std::rc::Rc::clone(target));
                            self.set_range_from_x(target, content_x);
                        }
                    }
                    // Editor hit-test pri klik na <input>/<textarea>: posun
                    // caret na glyph pod kurzorem. Bez tohoto by Click vzdy
                    // skociol jen na end (input_caret defaults).
                    // TODO: shift-click extend selection - aktualne nemame
                    // modifier z InputEvent::MouseDown propagated dolu.
                    if let Some(target) = target_node.as_ref() {
                        let is_input = matches!(target.tag_name().as_deref(),
                            Some("input") | Some("textarea"));
                        if is_input {
                            let target_clone = std::rc::Rc::clone(target);
                            self.editor_hit_test_input(&target_clone, x, false);
                        }
                    }
                    // mousedown event dispatch.
                    let md_off = target_node.as_ref().map(|t| self.event_offset(t, x, y));
                    if let (Some(target), Some(interp)) = (target_node.clone(), self.interpreter.as_mut()) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("mousedown".into()));
                        event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                        event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
                        if let Some((ox, oy)) = md_off {
                            event.set("offsetX".into(), crate::interpreter::JsValue::Number(ox));
                            event.set("offsetY".into(), crate::interpreter::JsValue::Number(oy));
                        }
                        event.set("target".into(), crate::interpreter::JsValue::DomNode(
                            std::rc::Rc::clone(&target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            std::rc::Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(&target, "mousedown", event_val);
                    }
                    // Begin text selection drag jen pri klik MIMO input/
                    // textarea. V inputu EditorState.selection_anchor handle
                    // selection self - bez tohoto by 2 paralelni selection
                    // states konfliktovali (Backspace clears EditorState, ale
                    // page_selection visible nad new insert).
                    // canvas: NEspoustet text-selection drag - canvas neni text +
                    // potrebuje mousemove behem dragu (kresleni). Bez vylouceni
                    // page_sel_dragging blokoval routing mousemove na canvas.
                    let click_on_noselect = target_node.as_ref()
                        .map(|n| matches!(n.tag_name().as_deref(),
                            Some("input") | Some("textarea") | Some("canvas")))
                        .unwrap_or(false);
                    if let Some(target) = target_node {
                        self.mouse_down_at = Some((x, y, target));
                    }
                    if !click_on_noselect {
                        self.sel_begin(content_x, content_y);
                    }
                    response.dirty = true;
                    self.dirty = true;
                }
            }
            InputEvent::MouseUp { x, y, button, .. } => {
                if matches!(button, crate::embed::MouseButton::Left) {
                    // Clear :active pseudo-class (set na MouseDown).
                    crate::browser::cascade::set_active_node(None);
                    // End element resize drag.
                    if self.resize_drag.take().is_some() {
                        response.dirty = true;
                        self.dirty = true;
                    }
                    // End range drag.
                    self.range_drag_node = None;
                    // End scrollbar drag.
                    if self.inner_v_drag.is_some() || self.inner_h_drag.is_some() {
                        self.inner_v_drag = None;
                        self.inner_h_drag = None;
                        response.dirty = true;
                        self.dirty = true;
                    }
                    if self.v_scrollbar_drag.is_some() || self.h_scrollbar_drag.is_some() {
                        self.v_scrollbar_drag = None;
                        self.h_scrollbar_drag = None;
                        response.dirty = true;
                        return response;
                    }
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    // End selection drag (collapse pri <3px movement).
                    self.sel_end();
                    let up_target = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.clone())
                        // Klik na <label> -> control (konzistentne s MouseDown aby
                        // same_target check + click toggle padly na control).
                        .map(|n| resolve_label_target(&n));
                    // mouseup event dispatch.
                    let mu_off = up_target.as_ref().map(|t| self.event_offset(t, x, y));
                    if let (Some(target), Some(interp)) = (up_target.as_ref(), self.interpreter.as_mut()) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("mouseup".into()));
                        event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                        event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
                        if let Some((ox, oy)) = mu_off {
                            event.set("offsetX".into(), crate::interpreter::JsValue::Number(ox));
                            event.set("offsetY".into(), crate::interpreter::JsValue::Number(oy));
                        }
                        event.set("target".into(), crate::interpreter::JsValue::DomNode(
                            std::rc::Rc::clone(target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            std::rc::Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(target, "mouseup", event_val);
                    }
                    // Click event: same target + distance < 5 px (jinak drag).
                    let down = std::mem::take(&mut self.mouse_down_at);
                    if let (Some((dx, dy, down_target)), Some(up)) = (down, up_target) {
                        let dist = ((dx - x).powi(2) + (dy - y).powi(2)).sqrt();
                        let same_target = std::rc::Rc::ptr_eq(&down_target, &up);
                        if dist < 5.0 && same_target {
                            let (cox, coy) = self.event_offset(&up, x, y);
                            let event_obj_rc = std::rc::Rc::new(std::cell::RefCell::new({
                                let mut event = crate::interpreter::JsObject::new();
                                event.set("type".into(), crate::interpreter::JsValue::Str("click".into()));
                                event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                                event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
                                event.set("offsetX".into(), crate::interpreter::JsValue::Number(cox));
                                event.set("offsetY".into(), crate::interpreter::JsValue::Number(coy));
                                event.set("target".into(), crate::interpreter::JsValue::DomNode(
                                    std::rc::Rc::clone(&up)));
                                event
                            }));
                            if let Some(interp) = self.interpreter.as_mut() {
                                let event_val = crate::interpreter::JsValue::Object(
                                    std::rc::Rc::clone(&event_obj_rc));
                                let _ = interp.dispatch_event(&up, "click", event_val);
                            }
                            let prevented = matches!(event_obj_rc.borrow().get("defaultPrevented"),
                                crate::interpreter::JsValue::Bool(true));
                            // <a href> navigation emit pri click + ne preventDefault.
                            if !prevented {
                                let mut cur = Some(up.clone());
                                while let Some(n) = cur {
                                    if n.tag_name().as_deref() == Some("a") {
                                        if let Some(href) = n.attr("href") {
                                            if !href.is_empty() && !href.starts_with('#') {
                                                let resolved = if let Some(base) = &self.base_url {
                                                    crate::browser::render::resolve_url(base, &href)
                                                } else { href.clone() };
                                                let target_kind = match n.attr("target").as_deref() {
                                                    Some("_blank") => crate::embed::event::NavigationTarget::NewTab,
                                                    Some(t) if !t.is_empty() => crate::embed::event::NavigationTarget::Named(t.to_string()),
                                                    _ => crate::embed::event::NavigationTarget::Self_,
                                                };
                                                response.navigation = Some(crate::embed::event::NavigationRequest {
                                                    url: resolved,
                                                    method: crate::embed::event::NavigationMethod::Get,
                                                    body: None,
                                                    target: target_kind,
                                                });
                                            }
                                        }
                                        break;
                                    }
                                    cur = n.parent.borrow().upgrade();
                                }
                            }
                        }
                    }
                    response.dirty = true;
                    self.dirty = true;
                }
            }
            InputEvent::MouseLeave => {
                // Clear :hover state pri opusteni viewport.
                if self.hovered_node_local.is_some() {
                    self.hovered_node_local = None;
                    crate::browser::cascade::set_hovered_node(None);
                    self.dirty = true;
                    response.dirty = true;
                }
            }
            InputEvent::KeyDown { ref key, .. } => {
                if let Some(target) = self.focused_dom_node() {
                    let is_input = matches!(target.tag_name().as_deref(),
                        Some("input") | Some("textarea"));
                    // Enter na focused input -> form submit: dispatch submit
                    // event + check defaultPrevented + emit NavigationRequest.
                    if is_input && key == "Enter" {
                        if let Some(form) = crate::browser::render::forms::find_ancestor_form(&target) {
                            let event_obj_rc = std::rc::Rc::new(std::cell::RefCell::new({
                                let mut event = crate::interpreter::JsObject::new();
                                event.set("type".into(), crate::interpreter::JsValue::Str("submit".into()));
                                event.set("target".into(), crate::interpreter::JsValue::DomNode(
                                    std::rc::Rc::clone(&form)));
                                event
                            }));
                            if let Some(interp) = self.interpreter.as_mut() {
                                let event_val = crate::interpreter::JsValue::Object(
                                    std::rc::Rc::clone(&event_obj_rc));
                                let _ = interp.dispatch_event(&form, "submit", event_val);
                            }
                            // Check defaultPrevented po dispatchu.
                            let prevented = matches!(event_obj_rc.borrow().get("defaultPrevented"),
                                crate::interpreter::JsValue::Bool(true));
                            if !prevented {
                                if let Some((url, method, body)) = crate::browser::render::forms::build_form_request(
                                    &form, self.base_url.as_deref())
                                {
                                    let nav_method = if method == "post" {
                                        crate::embed::event::NavigationMethod::Post
                                    } else {
                                        crate::embed::event::NavigationMethod::Get
                                    };
                                    response.navigation = Some(crate::embed::event::NavigationRequest {
                                        url,
                                        method: nav_method,
                                        body: body.map(|b| b.into_bytes()),
                                        target: crate::embed::event::NavigationTarget::Self_,
                                    });
                                }
                            }
                        }
                    }
                    if is_input {
                        let nid = std::rc::Rc::as_ptr(&target) as usize;
                        let cur = target.attr("value").unwrap_or_default();
                        // Ensure EditorState exists + synced s aktualnim value.
                        let entry = self.editors.entry(nid).or_insert_with(||
                            crate::browser::editor::EditorState::new(&cur));
                        if entry.text != cur { entry.set_text(&cur); }
                        let mut mutated = false;
                        let mut moved = false;
                        match key.as_str() {
                            "Backspace" => {
                                entry.delete_backward();
                                mutated = true;
                            }
                            "Delete" => {
                                entry.delete_forward();
                                mutated = true;
                            }
                            "ArrowLeft" => {
                                entry.move_left(false, false);
                                moved = true;
                            }
                            "ArrowRight" => {
                                entry.move_right(false, false);
                                moved = true;
                            }
                            "Home" => {
                                entry.move_home(false);
                                moved = true;
                            }
                            "End" => {
                                entry.move_end(false);
                                moved = true;
                            }
                            _ => {}
                        }
                        if mutated || moved {
                            let new_value = entry.text.clone();
                            let new_caret_byte = entry.caret;
                            if mutated {
                                target.set_attr("value", &new_value);
                            }
                            // Sync legacy char-idx caret pro back-compat caret blink.
                            let char_idx = crate::browser::editor::byte_to_char_offset(
                                &new_value, new_caret_byte);
                            self.input_caret.insert(nid, char_idx);
                            if mutated {
                                if let Some(interp) = self.interpreter.as_mut() {
                                    let mut event = crate::interpreter::JsObject::new();
                                    event.set("type".into(), crate::interpreter::JsValue::Str("input".into()));
                                    event.set("target".into(), crate::interpreter::JsValue::DomNode(
                                        std::rc::Rc::clone(&target)));
                                    let event_val = crate::interpreter::JsValue::Object(
                                        std::rc::Rc::new(std::cell::RefCell::new(event)));
                                    let _ = interp.dispatch_event(&target, "input", event_val);
                                }
                            }
                            response.dirty = true;
                            self.dirty = true;
                        }
                    }
                    if let Some(interp) = self.interpreter.as_mut() {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("keydown".into()));
                        event.set("key".into(), crate::interpreter::JsValue::Str(key.clone()));
                        event.set("target".into(), crate::interpreter::JsValue::DomNode(
                            std::rc::Rc::clone(&target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            std::rc::Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(&target, "keydown", event_val);
                        response.dirty = true;
                        self.dirty = true;
                    }
                }
            }
            InputEvent::KeyUp { ref key, .. } => {
                if let Some(target) = self.focused_dom_node() {
                    if let Some(interp) = self.interpreter.as_mut() {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("keyup".into()));
                        event.set("key".into(), crate::interpreter::JsValue::Str(key.clone()));
                        event.set("target".into(), crate::interpreter::JsValue::DomNode(
                            std::rc::Rc::clone(&target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            std::rc::Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(&target, "keyup", event_val);
                    }
                }
            }
            InputEvent::TextInput { ref text } => {
                if let Some(target) = self.focused_dom_node() {
                    let is_input = matches!(target.tag_name().as_deref(),
                        Some("input") | Some("textarea"));
                    if is_input {
                        // Skip control chars (Enter/Tab handled v KeyDown).
                        let printable: String = text.chars()
                            .filter(|c| !c.is_control()).collect();
                        if printable.is_empty() { return response; }
                        let nid = std::rc::Rc::as_ptr(&target) as usize;
                        let cur = target.attr("value").unwrap_or_default();
                        // Ensure EditorState exists + synced s attr value.
                        let entry = self.editors.entry(nid).or_insert_with(||
                            crate::browser::editor::EditorState::new(&cur));
                        if entry.text != cur { entry.set_text(&cur); }
                        entry.insert(&printable);
                        let new_value = entry.text.clone();
                        let new_caret_byte = entry.caret;
                        target.set_attr("value", &new_value);
                        // Sync legacy char-index input_caret (pouzity caret blink).
                        let char_idx = crate::browser::editor::byte_to_char_offset(
                            &new_value, new_caret_byte);
                        self.input_caret.insert(nid, char_idx);
                        if let Some(interp) = self.interpreter.as_mut() {
                            let mut event = crate::interpreter::JsObject::new();
                            event.set("type".into(), crate::interpreter::JsValue::Str("input".into()));
                            event.set("target".into(), crate::interpreter::JsValue::DomNode(
                                std::rc::Rc::clone(&target)));
                            let event_val = crate::interpreter::JsValue::Object(
                                std::rc::Rc::new(std::cell::RefCell::new(event)));
                            let _ = interp.dispatch_event(&target, "input", event_val);
                        }
                        response.dirty = true;
                        self.dirty = true;
                    }
                }
            }
            InputEvent::FocusChanged { .. } => {}
            InputEvent::Resize { width, height, scale_factor } => {
                self.resize(width, height, scale_factor);
                response.dirty = true;
            }
        }
        response
    }

    /// Renderuj page do offscreen texture. Pokud `dirty == false`, vrati
    /// posledni view bez prace. Pokud Engine je headless, vrati `None`.
    ///
    /// Phase 4b stav: alokuje + clear (transparent black). Real paint pipeline
    /// (cascade -> layout -> display list -> vertex buffer -> draw) prijde v
    /// Phase 5 - vyzaduje rozdeleni `browser::render::Renderer` na sdilene
    /// "page paint" + "compositor" vrstvy. Tj. soucasne WebView::render je
    /// API-functional ale jeste neproduce useful obraz.
    pub fn render(&mut self) -> Option<&wgpu::TextureView> {
        let device = self.engine.device.as_ref()?.clone();
        let queue = self.engine.queue.as_ref()?.clone();

        if self.target_texture.is_none() {
            self.ensure_target_texture();
        }
        if !self.dirty {
            return self.target_view.as_ref();
        }

        let view = self.target_view.as_ref()?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rwe-webview-render"),
        });
        {
            let _rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rwe-webview-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        queue.submit(std::iter::once(encoder.finish()));
        self.dirty = false;
        self.target_view.as_ref()
    }

    /// Renderuj page do offscreen texture pres dany Renderer. Real paint pass:
    /// cascade -> layout -> display list -> Renderer draw segments. Vrati view.
    ///
    /// Phase 4b step 2 dependence: Renderer drzi GPU pipelines + atlas, WebView
    /// si ho pujci na cas renderu. Phase 5 sloti tyto resources do Engine struct
    /// (sdilene Arc<>) a `WebView::render` bude self-contained.
    ///
    /// `renderer.config.width/height/scale_factor` MUSI odpovidat WebView viewport
    /// (jeden vp uniform). Hostujici aplikace `resize` WebView na renderer config
    /// pred `render_via` call.
    pub fn render_via(
        &mut self,
        renderer: &mut crate::browser::render::Renderer,
    ) -> Option<&wgpu::TextureView> {
        // Frame pacing: begin_frame na vstupu, mark_presented na vystupu.
        // Pres `browser::render::frame_pacing::FramePacer` foundation.
        let _frame_idx = self.frame_pacer.begin_frame();
        if self.target_texture.is_none() {
            self.ensure_target_texture();
        }
        // Drain periodicke setInterval + task_queue PRED dirty skip - jinak
        // pri idle frame interval bez efektu (cdp.js setInterval(pollEvents,250)
        // potrebuje run i kdyz DOM unchanged). Callback mozna modify DOM ->
        // dirty bump pres bump_dom_version -> dalsi check.
        //
        // BUG fix 2026-05-17: drain_timers byl AZ ZA dirty skip. Resolve native
        // fn push then cb do task_queue ale dirty skip vyskocil pred drain_timers
        // = 105+ frames cekani na mouse hover ktery dirty=true zpusobil. Mezi
        // SEND a then() = 27s.
        if let Some(interp) = self.interpreter.as_mut() {
            let _ = interp.drain_intervals();
            let _ = interp.drain_timers();
            // Plus drain fetches + raf + xhr po setup pred render skip.
            interp.drain_fetches();
            let _ = interp.drain_websockets();
            let ts_ms = self.animation_origin.elapsed().as_secs_f64() * 1000.0;
            let _ = interp.drain_raf_callbacks(ts_ms);
        }
        // JS DOM mutation pres interp.bump_dom_version() (setAttribute,
        // appendChild, innerHTML, ...) - diff vs sledovany snapshot -> dirty.
        if let Some(interp) = &self.interpreter {
            let cur = interp.dom_version();
            if cur != self.last_render_dom_version {
                self.dirty = true;
                self.last_render_dom_version = cur;
            }
            // Canvas kresleni (RAF) - generation diff -> dirty (canvas ops
            // nebumpaji dom_version). Robustni proti stabilnimu poctu ops
            // (clearRect reset -> stejny pocet ale jina kresba).
            let cgen = interp.canvas_generation();
            if cgen != self.last_render_canvas_gen {
                self.dirty = true;
                self.last_render_canvas_gen = cgen;
            }
        }
        // PERF: dirty skip - pokud nic se nezmenilo, vrat cached target_view
        // bez full cascade/layout/paint pipeline. Bez tohoto by render_via
        // bezel kazdy redraw frame i pro idle WebView (chrome bar, devtools
        // panel beztoho). Pri 3 WebView setup = 3x cascade/layout/paint per
        // frame = 1 FPS na velkych strankach.
        //
        // Co ovlivnuje dirty:
        // - Input events (scroll, click, key) set dirty=true v handle_input
        // - JS DOM mutation pres dom_version diff (vyse)
        // - load_html / set_zoom / resize
        //
        // Animations + smooth scroll + focused input -> NEN0 dirty, ale potreba
        // tick. Check zvlastne aktivni animace nez full skip.
        // Frame-skip gate: needs_animation_render (NE needs_continuous_render).
        // Interval/RAF uz drained vyse; pokud zmenily DOM/canvas -> dirty.
        // Pokud NE (idle setInterval/measureFps) -> skip layout+paint = levne.
        let needs_tick = self.needs_animation_render();
        if !self.dirty && !needs_tick {
            // Reset profilers - jinak title bar drzi historickou hodnotu z
            // prvni render (uvadi v omyl user diagnostiku).
            self.prof_cascade_ms = 0.0;
            self.prof_layout_ms = 0.0;
            self.prof_paint_ms = 0.0;
            self.prof_gpu_ms = 0.0;
            // Idle frame - texture reused. Marked presented (cached path).
            self.frame_pacer.mark_presented(_frame_idx);
            return self.target_view.as_ref();
        }
        // Renderer sdili pipeline + uniforms s WebView - sync browser zoom +
        // HiDPI scale_factor pred paint pass. NDC mapping pak shoduje s
        // RT physical px.
        renderer.zoom = self.zoom;
        renderer.scale_factor = self.scale_factor;
        // Override renderer target_size pres RT velikost (physical px).
        // Bez tohoto by NDC mapping pouzival full surface, vede k svisle
        // kompresi obsahu pri devtools split (RT je mensi nez surface).
        let rt_w = (self.viewport_w * self.scale_factor) as u32;
        let rt_h = (self.viewport_h * self.scale_factor) as u32;
        renderer.target_size = Some((rt_w, rt_h));
        // Sync scroll_pos od interpreteru. Bidirectional: JS scrollTo
        // zapise -> apply do scroll_x/y. Pri rozdilu kde scroll_x/y noveji
        // (host-side scrollbar drag/wheel) -> NE prepise z interp (stary).
        // Pravidlo: pokud interp.scroll_pos != scroll_x/y, posledni zmena
        // wins. Detekce: my drzime last_synced_scroll_pos, JS update detekt
        // pres (interp != last_synced) -> apply; nase update vyhrava jinak.
        if let Some(interp) = self.interpreter.as_ref() {
            let (jx, jy) = *interp.scroll_pos.borrow();
            let (lx, ly) = self.last_synced_scroll_pos;
            // Detekt: JS modifikoval interp.scroll_pos (diff vs last sync).
            let js_modified = (jx - lx).abs() > 0.5 || (jy - ly).abs() > 0.5;
            if js_modified {
                self.scroll_x = jx;
                self.scroll_y = jy;
                self.scroll_target_x = jx;
                self.scroll_target_y = jy;
                // JS scrollTo() je programatic instant - zrus active smooth anim
                // aby nepokracovala na stary target po JS overridu.
                self.scroll_anim_x = None;
                self.scroll_anim_y = None;
                self.dirty = true;
            }
        }
        // Smooth scroll tick: cubic-bezier ease-in-out, duration-based timing.
        // Frame-rate independent (drive lerp 25%/frame = 30 fps 2x pomalejsi).
        // Animation start v Scroll handler / kbd_scroll. Per-frame sample dle elapsed.
        let now = std::time::Instant::now();
        if let Some(anim) = self.scroll_anim_y {
            let (v, done) = anim.sample(now);
            self.scroll_y = v;
            if done { self.scroll_anim_y = None; self.scroll_y = anim.target_value; }
        } else {
            // Bez animace - direct snap (set_scroll, programatic).
            self.scroll_y = self.scroll_target_y;
        }
        if let Some(anim) = self.scroll_anim_x {
            let (v, done) = anim.sample(now);
            self.scroll_x = v;
            if done { self.scroll_anim_x = None; self.scroll_x = anim.target_value; }
        } else {
            self.scroll_x = self.scroll_target_x;
        }
        // Sync interp.scroll_pos do current scroll (pri wheel/scrollbar drag
        // animovany scroll, JS read pres pageXOffset/scrollX dostane realnou
        // hodnotu, ne jen JS-set hodnotu). Take updatuj last_synced_scroll_pos
        // - diff detection v dalsim frame ne triggerne false JS modified.
        if let Some(interp) = self.interpreter.as_ref() {
            *interp.scroll_pos.borrow_mut() = (self.scroll_x, self.scroll_y);
            self.last_synced_scroll_pos = (self.scroll_x, self.scroll_y);
        }
        // Sync element_scroll_overrides z interp -> self.element_scroll. JS
        // assign `el.scrollTop = N` populates overrides. Bez tohoto JS-driven
        // scroll per element nedosáhne layout/render.
        if let Some(interp) = self.interpreter.as_ref() {
            let mut overrides = interp.element_scroll_overrides.borrow_mut();
            if !overrides.is_empty() {
                for (ptr, (sx, sy)) in overrides.drain() {
                    self.element_scroll.insert(ptr, (sx, sy));
                }
                self.dirty = true;
            }
        }

        // Drain async jobs (image lazy loads, file IO callbacks). Volane PRED
        // cascade aby novy state byl dostupny v style_map (e.g. image natural
        // dims po load aktualizuji layout).
        self.async_jobs.drain();

        // Drain interpreter event queues (WebSocket frames, fetch responses,
        // requestAnimationFrame callbacks). Vola se kdyz interpreter existuje
        // (Po polarity invert WebView vlastni interpreter; drive App).
        // (Drains uz probehly nad dirty skip - vyhozeno duplicitni.)

        // target_view borrow odlozen do paint pass (pred L2 allocator
        // potrebujeme mut borrow self pres ensure_layer_texture).
        let doc = self.document.as_ref()?;

        // Layout viewport = logical CSS px / browser zoom. viewport_w/h jsou
        // uz LOGICAL (host predava surface_size / scale_factor).
        let viewport_w = self.viewport_w / self.zoom.max(0.01);
        let viewport_h = self.viewport_h / self.zoom.max(0.01);

        // 1. Cascade - resolve CSS styles per element. Cache pres hash klic
        // (dom_version, hovered_node, focused_node, viewport, stylesheets_len).
        // Pri pohybu mysi bez zmeny hovered_node = reuse cached map = O(1)
        // misto O(N*M) walk.
        // Pred cascade set thread_local na per-WV hover stav (selectory ctou
        // pres set_hovered_node API). Po render_via NEN0 reset - dalsi WV
        // cascade nastavi pred svym walk.
        // DEBUG: RWE_FORCE_HOVER=<class> vynuti hover na prvni node s tou tridou
        // (mereni hover perf bez realne mysi - winit nevidi SetCursorPos).
        if let Ok(sel) = std::env::var("RWE_FORCE_HOVER") {
            if let Some(root) = self.last_layout_root.as_ref() {
                // RWE_FORCE_HOVER_ALT=1 alternuje hover/None kazdy frame =
                // simuluje prejezd mysi (hovered_node se meni = full cascade miss
                // + DOM eventy kazdy frame). Bez ALT = steady hover.
                let alt = std::env::var("RWE_FORCE_HOVER_ALT").is_ok();
                // ALT: cykli pres comma-separated tridy (simuluje prejezd mysi pres
                // ruzne boxy = nove transition + texture kazdy prejezd). Toggle
                // hover/None mezi nimi. Bez ALT = steady hover na prvni tridu.
                thread_local!(static FRAMECNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
                let classes: Vec<&str> = sel.split(',').map(|s| s.trim()).collect();
                let target = if alt {
                    let c = FRAMECNT.with(|t| { let v = t.get() + 1; t.set(v); v });
                    let phase = (c / 12) as usize; // ~12 framu per faze
                    if phase % 2 == 0 {
                        let cls = classes[(phase / 2) % classes.len()];
                        find_first_node_by_class(root, cls)
                    } else { None }
                } else {
                    find_first_node_by_class(root, classes[0])
                };
                if self.hovered_node_local != target {
                    self.hovered_node_local = target;
                    self.dirty = true;
                }
            }
        }
        crate::browser::cascade::set_hovered_node(self.hovered_node_local);

        let prof_t0 = std::time::Instant::now();
        // Per-element matched_decls cache invalidate pres dom mutate.
        // Pri DOM mutaci (interp.bump_dom_version), node_ptr mohou byt invalid
        // (mrtve element + recyklacky addr) - cache musime drop.
        let cur_dom_ver = self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0);
        if cur_dom_ver != self.last_matched_cache_dom_ver {
            // DISABLED pres multi-WV thread_local konflikt - kazda WV by
            // zhozila sdilenou cache jine WV. TODO: per-WV cache fields.
            // crate::browser::cascade::clear_matched_decls_cache();
            self.last_matched_cache_dom_ver = cur_dom_ver;
        }
        // PERF: ovlivnuje stylesheet :hover/:focus? Pokud ne, hover/focus
        // zmena nemeni style_map -> cache klic NEzahrnuje. Drasticky redukuje
        // cascade walks pri hover na pages bez :hover effects.
        let uses_hover = self.stylesheets.iter()
            .any(|s| crate::browser::cascade::stylesheet_uses_pseudo(s, "hover"));
        let uses_focus = self.stylesheets.iter()
            .any(|s| crate::browser::cascade::stylesheet_uses_pseudo(s, "focus"));
        let uses_active = self.stylesheets.iter()
            .any(|s| crate::browser::cascade::stylesheet_uses_pseudo(s, "active"));
        let cache_key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            // dom_style_version (NE dom_version) - geometry-only attr mutace
            // (SVG points animace per-frame) nebumpaji style verzi -> cache
            // prezije = no full re-cascade kazdy frame.
            self.interpreter.as_ref().map(|i| i.dom_style_version()).unwrap_or(0).hash(&mut hasher);
            // Per-WV hovered/focused. Bez per-WV by jine WV mouse_move
            // invalidoval cache i kdyz tahla WV nezmenila hover.
            // Conditional: jen pokud stylesheet pouziva :hover/:focus.
            if uses_hover {
                self.hovered_node_local.unwrap_or(0).hash(&mut hasher);
            }
            if uses_focus {
                self.focused_node_local.unwrap_or(0).hash(&mut hasher);
            }
            // :active node (set na MouseDown). Bez tohoto by se :active styly
            // nepromitly (cache hit -> stara cascade).
            if uses_active {
                crate::browser::cascade::get_active_node().unwrap_or(0).hash(&mut hasher);
            }
            // viewport rounded
            (viewport_w as u32).hash(&mut hasher);
            (viewport_h as u32).hash(&mut hasher);
            // stylesheets - cheap identity (len + first rule count)
            self.stylesheets.len().hash(&mut hasher);
            for s in &self.stylesheets {
                s.rules.len().hash(&mut hasher);
            }
            hasher.finish()
        };
        let cascade_was_miss = Some(cache_key) != self.cascade_cache_key;
        let mut style_map = if !cascade_was_miss {
            // Cache hit - reuse Rc clone.
            self.cascade_cache_value.as_ref().unwrap().clone()
        } else {
            // Per-WV cache izolace: host_id = root ptr, dom_ver = JS counter.
            // Pres host_id se WV-A entries nesmichaji s WV-B; pres dom_ver
            // stale entries po DOM mutaci auto-invalidne (key miss).
            let host_id = std::rc::Rc::as_ptr(&doc.root) as usize as u64;
            let dom_ver = self.interpreter.as_ref().map(|i| i.dom_style_version()).unwrap_or(0);
            crate::browser::cascade::set_cascade_ctx(host_id, dom_ver);
            let m = std::rc::Rc::new(crate::browser::cascade::cascade_with_viewport(
                &doc.root, &self.stylesheets, viewport_w, viewport_h));
            self.cascade_cache_key = Some(cache_key);
            self.cascade_cache_value = Some(m.clone());
            // PROFILE: log breakdown pokud cascade > 10ms.
            let prof = crate::browser::cascade::cascade_prof_snapshot();
            let total = prof.viewport_prep_ms + prof.keys_prep_ms + prof.walk_ms
                      + prof.ua_defaults_ms + prof.propagate_ms;
            if total > 10.0 {
                eprintln!("[CASCADE PROF] total={:.1}ms vp={:.1} keys={:.1} walk={:.1} ua={:.1} prop={:.1} | nodes={} might_calls={} might_pass={} hits={} decls={}",
                    total, prof.viewport_prep_ms, prof.keys_prep_ms, prof.walk_ms,
                    prof.ua_defaults_ms, prof.propagate_ms,
                    prof.nodes, prof.might_match_calls, prof.might_match_pass,
                    prof.matches_selector_hits, prof.decls_applied);
            }
            m
        };

        let elapsed = self.animation_origin.elapsed().as_secs_f32();

        // 1b. CSS Transitions: detect zmeny vs prev_style_map -> aktivni
        // transitions. Apply tween na current style_map. PERF: skip kompletne
        // kdyz CSS neobsahuje "transition" property.
        let css_uses_transitions = self.stylesheets.iter()
            .any(|s| s.rules.iter().any(|r| r.declarations.iter()
                .any(|d| d.property.starts_with("transition"))));
        let mut ended_transitions: Vec<(usize, String)> = Vec::new();
        if css_uses_transitions {
            if let Some(prev) = &self.prev_style_map {
                let same_map = std::rc::Rc::ptr_eq(prev, &style_map);
                if !same_map {
                    let active_before = std::mem::take(&mut self.active_transitions);
                    let prev_keys: std::collections::HashSet<(usize, String)> = active_before.iter()
                        .map(|t| (t.node_id, t.property.clone())).collect();
                    self.active_transitions = crate::browser::cascade::detect_transitions(
                        &**prev, &*style_map, active_before, elapsed);
                    let now_keys: std::collections::HashSet<(usize, String)> = self.active_transitions.iter()
                        .map(|t| (t.node_id, t.property.clone())).collect();
                    for k in prev_keys.difference(&now_keys) {
                        ended_transitions.push(k.clone());
                    }
                } else {
                    // No cascade change -> drop expired, keep rest.
                    let active_before = std::mem::take(&mut self.active_transitions);
                    for at in active_before {
                        let total = at.spec.duration_secs + at.spec.delay_secs;
                        if elapsed - at.start_time < total {
                            self.active_transitions.push(at);
                        }
                    }
                }
            }
        }
        // Cascade BASE (PRED apply_transitions) pro prev_style_map pristi frame.
        // KLIC: detect_transitions musi porovnavat BASE vs BASE (computed cascade
        // hodnoty), NE post-transition interpolovane. Driv prev_style_map = mapa
        // PO apply_transitions = obsahovala "scale(1.300)"; detect ji porovnal s
        // cascade target "scale(1.3)" -> string mismatch -> spurious no-op
        // transition kazdy frame -> (a) jitter scale = layer texture realloc =
        // "1 FPS"; (b) ta spurious transition blokovala REVERSE (4088 check) pri
        // un-hover -> box se NEVRATIL. Drzenim base se make_mut nize donuti
        // klonovat (refcount 2) -> base zustane nezmenena.
        let cascade_base = style_map.clone();
        if !self.active_transitions.is_empty() {
            crate::browser::cascade::apply_transitions(
                std::rc::Rc::make_mut(&mut style_map), &self.active_transitions, elapsed);
        }
        if std::env::var("RWE_TRANS_DBG").is_ok() && !self.active_transitions.is_empty() {
            for at in &self.active_transitions {
                let t = (elapsed - at.start_time - at.spec.delay_secs).max(0.0);
                let prog = (t / at.spec.duration_secs).clamp(0.0, 1.0);
                eprintln!("[TRANS] node={} prop={} {}->{} prog={:.2} (start={:.2} elapsed={:.2} dur={:.2})",
                    at.node_id, at.property, at.from_value, at.to_value, prog,
                    at.start_time, elapsed, at.spec.duration_secs);
            }
        }

        // 1c. CSS @keyframes animation tick - aplikuj current keyframe values
        // dle elapsed time. Pri presence @keyframes v CSS, style_map dostane
        // overlay s animated property values (transform, opacity, left, ...).
        let has_keyframes = self.stylesheets.iter().any(|s| !s.keyframes.is_empty());
        if has_keyframes {
            let _animating = crate::browser::cascade::apply_animations(
                std::rc::Rc::make_mut(&mut style_map), &self.stylesheets, elapsed);
            let max_scroll = (style_map.len() as f32).max(1.0);
            let scroll_progress = if max_scroll > 1.0 { self.scroll_y / max_scroll.max(1.0) } else { 0.0 };
            let _ = crate::browser::cascade::apply_scroll_animations(
                std::rc::Rc::make_mut(&mut style_map), &self.stylesheets, scroll_progress);
        }

        // 1d. Animation event detection (start / end / iteration). Vyzaduje
        // walk vsech elementu se spec, porovna s active_animations + iter
        // counter.
        let mut current_anims: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
        let mut iter_events: Vec<(usize, String, i32)> = Vec::new();
        if has_keyframes {
            for (node_id, styles) in &*style_map {
                if let Some(spec) = crate::browser::cascade::AnimationSpec::from_styles(styles) {
                    let t = elapsed - spec.delay_secs;
                    if t >= 0.0 && (spec.iteration_count.is_infinite() || t / spec.duration_secs < spec.iteration_count) {
                        let key = (*node_id, spec.name.clone());
                        current_anims.insert(key.clone());
                        let cur_iter = (t / spec.duration_secs).floor() as i32;
                        let prev_iter = self.animation_iterations.get(&key).copied().unwrap_or(-1);
                        if cur_iter > prev_iter && cur_iter > 0 {
                            iter_events.push((*node_id, spec.name.clone(), cur_iter));
                        }
                        self.animation_iterations.insert(key, cur_iter);
                    }
                }
            }
        }
        let started: Vec<(usize, String)> = current_anims.difference(&self.active_animations).cloned().collect();
        let ended_anims: Vec<(usize, String)> = self.active_animations.difference(&current_anims).cloned().collect();
        self.active_animations = current_anims;

        // 1e. Dispatch transition / animation events do JS interpretu.
        if let Some(interp) = self.interpreter.as_mut() {
            use std::rc::Rc;
            let doc_root = Rc::clone(&interp.document.borrow().root);
            // transitionend
            for (node_id, prop) in &ended_transitions {
                if let Some(target) = crate::browser::render::find_node_by_ptr(&doc_root, *node_id) {
                    let mut event = crate::interpreter::JsObject::new();
                    event.set("type".into(), crate::interpreter::JsValue::Str("transitionend".into()));
                    event.set("propertyName".into(), crate::interpreter::JsValue::Str(prop.clone()));
                    event.set("target".into(), crate::interpreter::JsValue::DomNode(Rc::clone(&target)));
                    let event_val = crate::interpreter::JsValue::Object(
                        Rc::new(std::cell::RefCell::new(event)));
                    let _ = interp.dispatch_event(&target, "transitionend", event_val);
                }
            }
            // animationstart / animationend
            for (event_type, list) in [("animationstart", &started), ("animationend", &ended_anims)] {
                for (node_id, name) in list {
                    if let Some(target) = crate::browser::render::find_node_by_ptr(&doc_root, *node_id) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str(event_type.into()));
                        event.set("animationName".into(), crate::interpreter::JsValue::Str(name.clone()));
                        event.set("target".into(), crate::interpreter::JsValue::DomNode(Rc::clone(&target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(&target, event_type, event_val);
                    }
                }
            }
            // animationiteration
            for (node_id, name, _iter) in &iter_events {
                if let Some(target) = crate::browser::render::find_node_by_ptr(&doc_root, *node_id) {
                    let mut event = crate::interpreter::JsObject::new();
                    event.set("type".into(), crate::interpreter::JsValue::Str("animationiteration".into()));
                    event.set("animationName".into(), crate::interpreter::JsValue::Str(name.clone()));
                    event.set("target".into(), crate::interpreter::JsValue::DomNode(Rc::clone(&target)));
                    let event_val = crate::interpreter::JsValue::Object(
                        Rc::new(std::cell::RefCell::new(event)));
                    let _ = interp.dispatch_event(&target, "animationiteration", event_val);
                }
            }
        }

        // Sync prev_style_map pro pristi frame transitions detection.
        // Pouzij cascade BASE (ne post-transition style_map) - viz komentar vyse.
        self.prev_style_map = Some(cascade_base);
        let prof_t1 = std::time::Instant::now();
        self.prof_cascade_ms = prof_t1.duration_since(prof_t0).as_secs_f32() * 1000.0;

        // Paint cache: pokud kompletni style_map content match predchozi
        // frame (mouse hover bez visible CSS effect = novy Rc ale same content)
        // -> SKIP layout+paint+gpu, reuse target_view. Vykresleny obsah uz
        // existuje v target_texture, nic se nezmenilo.
        // Skip jen pokud scroll a active anim taky nezmenily.
        // paint_fp cache - Rc identity klic (same trick jako layout_fp).
        let style_map_ptr_for_pfp = std::rc::Rc::as_ptr(&style_map) as usize;
        let paint_fp = if let Some((cached_ptr, cached_fp)) = self.paint_fp_cache {
            if cached_ptr == style_map_ptr_for_pfp { cached_fp } else {
                let fp = crate::browser::cascade::paint_fingerprint(&style_map);
                self.paint_fp_cache = Some((style_map_ptr_for_pfp, fp));
                fp
            }
        } else {
            let fp = crate::browser::cascade::paint_fingerprint(&style_map);
            self.paint_fp_cache = Some((style_map_ptr_for_pfp, fp));
            fp
        };
        // Paint cache check - mimo dirty flag (state change beyond style_map jako
        // element_scroll vyzaduje re-render). Dirty=true vzdy bypasses cache.
        // Bez dirty checku: wheel scroll on element changed element_scroll mapu,
        // style_map identicky -> paint_fp match -> cache hit -> NO RE-RENDER ->
        // page jevi "spici" (zustane na predchozim framu, scroll jen visible
        // pri nasledne MouseMove ktery nastavi dirty znova).
        let paint_cache_hit = !self.dirty
            && Some(paint_fp) == self.last_paint_fingerprint
            && !needs_tick
            && self.target_view.is_some();
        if paint_cache_hit {
            // Cache hit - vse identicke, reuse predchozi frame.
            self.prof_layout_ms = 0.0;
            self.prof_paint_ms = 0.0;
            self.prof_gpu_ms = 0.0;
            renderer.target_size = None;
            self.dirty = false;
            // Cache hit = texture reused = presented (cached path).
            self.frame_pacer.mark_presented(_frame_idx);
            return self.target_view.as_ref();
        }
        self.last_paint_fingerprint = Some(paint_fp);

        // 2. Layout cache - content-based klic. Hash pres LAYOUT_RELEVANT_PROPS
        // ne cely style_map. Hover zmena typicky meni color/background - layout
        // hash zustava stable -> reuse cached layout_root. Skip layout_tree
        // call (363ms drop na <1ms v debug).
        // layout_fp cache - Rc identity klic.
        let style_map_ptr = std::rc::Rc::as_ptr(&style_map) as usize;
        let layout_fp = if let Some((cached_ptr, cached_fp)) = self.layout_fp_cache {
            if cached_ptr == style_map_ptr { cached_fp } else {
                let fp = crate::browser::cascade::layout_fingerprint(&style_map);
                self.layout_fp_cache = Some((style_map_ptr, fp));
                fp
            }
        } else {
            let fp = crate::browser::cascade::layout_fingerprint(&style_map);
            self.layout_fp_cache = Some((style_map_ptr, fp));
            fp
        };
        // PERF: scroll_y NEN0 v key - smooth scroll inertia by jinak invalidoval
        // cache kazdy frame (lerp 25% per step = scroll_y meni kazdy pixel).
        // Layout je viewport+style closure, scroll je paint-time offset.
        // Sticky positions zachycuje apply_sticky() pres mutaci cached root.
        // dom_style_version v klici: textContent / structural / class/id/style
        // mutace meni layout (text size, pridane elementy) ale NEMENI style_map
        // fingerprint -> bez nej layout cache HIT = stary text/struktura
        // (napr. onclick co meni textContent se nezobrazil). dom_style_version
        // bumpne pri techto mutacich ale NE pri SVG geometry (points) animaci.
        // Layout keyuje na dom_LAYOUT_version (style + textContent/value, NE SVG
        // geometry). textContent re-layoutuje text ale cascade (dom_style_ver)
        // PREZIJE = zadny 12ms re-cascade per frame. SVG points (content-only)
        // NEre-layoutuji - jen re-paint (re-raster).
        let dom_layout_ver = self.interpreter.as_ref()
            .map(|i| i.dom_layout_version()).unwrap_or(0);
        let dom_style_ver = self.interpreter.as_ref()
            .map(|i| i.dom_style_version()).unwrap_or(0);
        let layout_key = (
            layout_fp,
            dom_layout_ver,
            (viewport_w as u32),
            (viewport_h as u32),
        );
        let mut layout_root = if Some(layout_key) == self.layout_cache_key
            && self.last_layout_root.is_some()
        {
            // Cache hit - MOVE (take) tree z predchoziho framu misto clone.
            // PERF: last_layout_root se stejne za chvili prepise novym clonem
            // (`self.last_layout_root = Some(layout_root.clone())` po pass_a).
            // Drivejsi `.clone()` tady = DVA cele klony 667-node tree per frame
            // (tenhle + ten po pass_a). Take() = jen jeden clon. Mezi take a
            // restore se self.last_layout_root necte (HIT vetev nejde do MISS
            // vetve co ho borrowuje pro subtree reuse) a fn nema early-return.
            self.last_layout_root.take().unwrap()
        } else {
            self.layout_cache_key = Some(layout_key);
            // Layout subtree cache: pri MISS na top-level (fingerprint zmena nekde),
            // predame prev_root pres raw ptr index - subtree match HIT pres
            // fingerprint reuse prev subtree (clone jen pri HIT). Drasticky snizuje
            // rebuild kdyz hover zmeni jen 1 element a celej zbytek je stejny.
            crate::browser::layout::reset_build_box_stats();
            // Build PseudoStyleMap (::before / ::after / ::marker / atd) - bez
            // tohoto nikdy build_pseudo_box NE-emit pseudo content. Drive bylo
            // empty_pseudo = vsechny ::before/::after invisible.
            let t_ps = std::time::Instant::now();
            // Cache cascade_pseudo (~6ms) - reuse pokud se DOM strukturalne/tridami
            // nezmenil (dom_style_version + stylesheets sig stejne). Hover co meni
            // jen layout prop (padding/border) tim nemusi re-matchovat vsechny
            // pseudo selektory.
            let pseudo_key = (
                dom_style_ver,
                self.stylesheets.len()
                    + self.stylesheets.iter().map(|s| s.rules.len()).sum::<usize>(),
            );
            let pseudo_map = if self.pseudo_map_cache_key == Some(pseudo_key) {
                self.pseudo_map_cache.as_ref().unwrap().clone()
            } else {
                let m = crate::browser::cascade::cascade_pseudo(&doc.root, &self.stylesheets);
                self.pseudo_map_cache = Some(m.clone());
                self.pseudo_map_cache_key = Some(pseudo_key);
                m
            };
            let t = std::time::Instant::now();
            let r = crate::browser::layout::layout_tree_with_pseudo_cached(
                &doc.root, &style_map, &pseudo_map, viewport_w, viewport_h,
                self.last_layout_root.as_ref());
            let _ = t_ps;
            let elapsed = t.elapsed().as_secs_f32() * 1000.0;
            if elapsed > 100.0 {
                let node_count = count_nodes(&doc.root);
                let (bb_count, bb_total_us) = crate::browser::layout::take_build_box_stats();
                eprintln!("[LAYOUT SLOW] total:{:.0}ms ({} nodes = {:.1}ms/node) | build_box: {} calls, {:.1}ms cumulative",
                    elapsed, node_count, elapsed / (node_count.max(1) as f32),
                    bb_count, bb_total_us as f32 / 1000.0);
            }
            // DIAG: pokud root pretahuje viewport, layout vyleti -> hledame
            // flex bug nebo overflow nedodrzeni. Rate-limit (max 3 logy).
            if (r.rect.width > viewport_w * 1.05 || r.rect.height > viewport_h * 5.0)
                && self.layout_overflow_log_count < 3 {
                eprintln!("[LAYOUT OVERFLOW #{}] root w={:.0}/h={:.0} vs viewport w={:.0}/h={:.0} | body overflow_x={:?} overflow_y={:?}",
                    self.layout_overflow_log_count + 1,
                    r.rect.width, r.rect.height, viewport_w, viewport_h,
                    r.overflow_x, r.overflow_y);
                self.layout_overflow_log_count += 1;
            }
            r
        };

        // 2b. Sticky positioning - PRESUNUTO az tesne pred extract_layer_tree.
        // Drive bylo tady (pred apply_paint_animations + pass_b), ALE
        // apply_paint_animations (anim_baseline = rect - layout_offset) sticky
        // shift undoval -> layer tree mel puvodni rect.y -> sticky nedrzel.

        // 2c. Paint-side animations apply (transform overlay, opacity tween).
        crate::browser::render::apply_paint_animations(&mut layout_root, &style_map);

        // 2c2. Per-element scroll - 2-pass:
        // Pass A: nastavit bx.scroll_offset_y/x z mapy NA layout_root (bez
        //         shiftu children). Clamp do mapy.
        // Save: clean clone (children un-shifted) jako last_layout_root - hit-test
        //         pak cita bx.scroll_offset_y pro coord adjustment.
        // Pass B: shift children of scrollable boxes by -offset (paint kopie).
        fn pass_a_set_offsets(
            bx: &mut crate::browser::layout::LayoutBox,
            map: &mut std::collections::HashMap<usize, (f32, f32)>,
        ) {
            use crate::browser::scroll::Scrollable;
            if bx.needs_scrollbar_y() || bx.needs_scrollbar_x() {
                let mut sx = 0.0f32;
                let mut sy = 0.0f32;
                if let Some(node) = bx.node.as_ref() {
                    let id = std::rc::Rc::as_ptr(node) as usize;
                    let (max_x, max_y) = bx.max_scroll();
                    if let Some(&(mx, my)) = map.get(&id) {
                        sx = mx.clamp(0.0, max_x);
                        sy = my.clamp(0.0, max_y);
                        if (sx - mx).abs() > 0.01 || (sy - my).abs() > 0.01 {
                            map.insert(id, (sx, sy));
                        }
                    }
                }
                bx.scroll_offset_x = sx;
                bx.scroll_offset_y = sy;
            }
            for ch in bx.children.iter_mut() {
                pass_a_set_offsets(ch, map);
            }
        }
        fn pass_b_shift_children(bx: &mut crate::browser::layout::LayoutBox) {
            use crate::browser::scroll::Scrollable;
            if bx.needs_scrollbar_y() || bx.needs_scrollbar_x() {
                let sx = bx.scroll_offset_x;
                let sy = bx.scroll_offset_y;
                if sx.abs() > 0.01 || sy.abs() > 0.01 {
                    for ch in bx.children.iter_mut() {
                        crate::browser::layout::shift_subtree(ch, -sx, -sy);
                    }
                }
            }
            for ch in bx.children.iter_mut() {
                pass_b_shift_children(ch);
            }
        }
        pass_a_set_offsets(&mut layout_root, &mut self.element_scroll);
        // Save clean (offsets set, children un-shifted) - hit_test pres
        // pak respektuje scroll_offset pro coord adjustment.
        self.last_layout_root = Some(layout_root.clone());
        // R-tree spatial hit index: LAZY build. Drive se buildil tady KAZDY
        // frame (i bez pohybu mysi) = zbytecna O(N log N) prace na animovanych
        // strankach. Ted jen invalidace - build az pri prvnim mouse-move dotazu
        // (viz handle_input MouseMove). Hit-testing je event-driven, ne per-frame.
        self.hit_rtree = None;

        // Fire ResizeObserver / IntersectionObserver callbacks po layout.
        // PERF: cely blok (collect_rects walk + rect_map.clone alokace) bezi jen
        // kdyz jsou nejake observery registrovane. Vetsina stranek zadne nema ->
        // skip = usetreny O(N) walk + 2 HashMap alokace per frame.
        fn collect_rects(bx: &crate::browser::layout::LayoutBox,
                         out: &mut std::collections::HashMap<usize, (f32, f32, f32, f32)>) {
            if let Some(n) = &bx.node {
                let id = std::rc::Rc::as_ptr(n) as usize;
                out.insert(id, (bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height));
            }
            for ch in &bx.children { collect_rects(ch, out); }
        }
        let has_observers = self.interpreter.as_ref().map(|i|
            !i.resize_observers.borrow().is_empty()
            || !i.intersection_observers.borrow().is_empty()).unwrap_or(false);
        if has_observers {
            let mut rect_map: std::collections::HashMap<usize, (f32, f32, f32, f32)> =
                std::collections::HashMap::new();
            collect_rects(&layout_root, &mut rect_map);
            let viewport_rect = (self.scroll_x, self.scroll_y, viewport_w, viewport_h);
            let rect_map_ro = rect_map.clone();
            if let Some(interp) = self.interpreter.as_mut() {
                interp.fire_resize_observers(|id| rect_map_ro.get(&id).map(|r| (r.2, r.3)));
                interp.fire_intersection_observers(
                    |id| rect_map.get(&id).copied(),
                    viewport_rect,
                );
            }
        }

        pass_b_shift_children(&mut layout_root);
        // 2b (presunuto sem): sticky positioning JAKO POSLEDNI uprava rect.y -
        // po apply_paint_animations + pass_b, aby je nic neresetovalo. Layer tree
        // (nize) pak ma sticky-shiftnute rect.y -> sticky header/sidebar drzi.
        crate::browser::layout::apply_sticky(&mut layout_root, self.scroll_y);
        let save_layout_root_at_end = false;

        // 2d. L1+L2 compositor: extract LayerTree z layout + damage tracking.
        // L2: per-layer offscreen texture allocator + damage rect detection.
        // Damage: porovnani fingerprint vs prev frame. Same fingerprint = no
        // damage = mozne reuse cached texture (TODO D3+D4).
        // Set viewport cull PRED extract aby compute_fingerprints ignoroval
        // off-screen content (off-screen bg/color animace nedamaguje root =
        // zadny zbytecny 9ms full root re-paint). build_layered_display_list nize
        // si cull prenastavi sam.
        crate::browser::paint::set_viewport_cull(self.scroll_y, self.scroll_y + viewport_h);
        // Promote elementy s aktivni @keyframes animaci na vlastni layer. KLIC
        // pro perf: paint-only animace (colorCycle bg/border) co NEjsou
        // transform/opacity by jinak zustaly v ROOT layeru a kazdy frame
        // damagovaly cely root -> paint_layer_into prekresli VSECHEN root content
        // (i off-screen) = 17ms (35 FPS pri animacni sekci). Promote = damage
        // izolovany na maly layer = root cache reuse.
        let anim_layer_ids: std::collections::HashSet<usize> =
            self.active_animations.iter().map(|(id, _)| *id).collect();
        crate::browser::compositor::set_force_layer_nodes(anim_layer_ids);
        let mut layer_tree = crate::browser::compositor::extract_layer_tree(&layout_root);
        // Compositor-driven anim tick - posune progress + override layer
        // opacity/transform values BEZ re-cascade. Pri animaci jen tyhle props
        // = structural_fp identical = damage_rect = None = texture cache reuse.
        // (Priority 4 z RENDER_RETROSPECTIVE.)
        {
            let now = std::time::Instant::now();
            let _any_active = self.compositor_anims.tick(now);
            self.compositor_anims.apply_to_layer_tree(&mut layer_tree);
        }
        crate::browser::compositor::mark_damage(
            &mut layer_tree, &mut self.prev_layer_fingerprints);
        crate::browser::compositor::mark_tile_damage(
            &mut layer_tree, &mut self.prev_tile_fingerprints);
        // Vrstvy kterym se prave (re)vytvorila texture (size change ze scale
        // spring overshoot) - musi se re-rastrovat i kdyz nejsou damaged (jinak
        // nova prazdna texture = vanish). Deklarovano pred alloc blokem aby bylo
        // viditelne i v raster bloku (layer_gpu_mode nize).
        let mut recreated_layer_tex: std::collections::HashSet<usize> = std::collections::HashSet::new();
        if std::env::var("RWE_DAMAGE_DBG").is_ok() {
            let total = crate::browser::compositor::count_layers(&layer_tree);
            let damaged = crate::browser::compositor::count_damaged_layers(&layer_tree);
            let (dirty_tiles, total_tiles) = crate::browser::compositor::count_tile_damage(&layer_tree);
            eprintln!("[DAMAGE] {}/{} layers dirty, {}/{} tiles dirty",
                damaged, total, dirty_tiles, total_tiles);
        }
        {
            let mut alive = std::collections::HashSet::new();
            crate::browser::compositor::collect_layer_ids(&layer_tree, &mut alive);
            self.gc_layer_textures(&alive);
            let mut flat: Vec<&crate::browser::compositor::LayerNode> = Vec::new();
            crate::browser::compositor::flatten_layers(&layer_tree, &mut flat);
            // Collect tile dimensions before borrow ends.
            let tile_alloc: Vec<(usize, usize, f32, f32, bool)> = flat.iter()
                .flat_map(|layer| layer.tiles.iter().enumerate().map(|(idx, tile)| {
                    (layer.id, idx, tile.local_rect.width, tile.local_rect.height, tile.dirty)
                }).collect::<Vec<_>>())
                .collect();
            // Per-layer texture alloc - SKIP pri oversize (tile mode handle).
            // Pres oversize, single texture by byla clamped = compress artifact.
            const MAX_TEX_DIM_ALLOC: f32 = 8192.0;
            // Match ensure_layer_texture formula = zoom * sf. Drive sf only =
            // crash pri zoom (alloc internal calc 8192+).
            let sf_alloc = self.zoom * self.scale_factor;
            for layer in &flat {
                let lw = layer.root_rect.width.max(1.0);
                let lh = layer.root_rect.height.max(1.0);
                let rscale = layer_raster_scale(&layer.transforms);
                let phys_max = (lw * sf_alloc * rscale).max(lh * sf_alloc * rscale);
                if phys_max <= MAX_TEX_DIM_ALLOC {
                    if self.ensure_layer_texture(layer.id, lw, lh, rscale) {
                        recreated_layer_tex.insert(layer.id);
                    }
                }
            }
            // Per-tile alloc - tile texture < 8192 phys = no clamp ever.
            // Pres oversize layer = tile path active. Pres small layer = tiles
            // unused (single layer texture path). Alloc anyway pres consistency
            // (no perf cost - tile.dirty=false skipped pres render path).
            for (lid, idx, w, h, _dirty) in tile_alloc {
                let _ = self.ensure_tile_texture(lid, idx, w.max(1.0), h.max(1.0));
            }
        }
        // Layout dump diag - per-WV 1x kdyz dom_version >= 100 (DOM populated).
        if std::env::var("RWE_LAYOUT_DUMP").is_ok() && !self.layout_dumped
            && self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0) > 50
        {
            self.layout_dumped = true;
            fn dump_box(bx: &crate::browser::layout::LayoutBox, depth: usize, max_depth: usize) {
                if depth > max_depth { return; }
                let cls = bx.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
                let tag = bx.tag.as_deref().unwrap_or("?");
                eprintln!("{}{} class={:?} rect=({:.0},{:.0},{:.0}x{:.0}) disp={:?} dir={:?} ovf_y={:?} icw/h={:.0}/{:.0} exh={:?} grow={}",
                    " ".repeat(depth), tag, cls, bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height,
                    bx.display, bx.flex_direction, bx.overflow_y, bx.inner_content_w, bx.inner_content_h, bx.explicit_height, bx.flex_grow);
                for ch in &bx.children {
                    dump_box(ch, depth + 1, max_depth);
                }
            }
            eprintln!("=== LAYOUT DUMP (WV addr={:p}) ===", self);
            dump_box(&layout_root, 0, 8);
        }
        let prof_t2 = std::time::Instant::now();
        self.prof_layout_ms = prof_t2.duration_since(prof_t1).as_secs_f32() * 1000.0;

        // D4 GPU layer pipeline (WebRender-style compositing) = DEFAULT a spravna
        // architektura. Opt-out monolithic pres RWE_LAYER_GPU_OFF (CPU fallback).
        // Transform clip v layer compose byl opraven SYSTEMOVE: paint_box uz
        // neaplikuje bx.transform na layer boxy (else branch guard) - transform
        // dela vyhradne GPU compose na quad = zadny double-transform/clip.
        // MONOLITHIC_PAINT flag drzi i CPU fallback korektni (geometry pro transformy).
        let layer_gpu_mode = std::env::var("RWE_LAYER_GPU_OFF").is_err();
        crate::browser::paint::set_monolithic_paint(!layer_gpu_mode);

        // 3. Paint - per-layer pass dle damage_rect.
        // Per layer s damage_rect = Some: repaint commands + cache.
        // Per layer s damage_rect = None: reuse cached commands z prev framu.
        // Assembly: concat per-layer caches v layer tree order.
        // Inspired by WebRender Picture cache (gfx/wr/webrender/src/picture.rs).
        let mut display_list = build_layered_display_list(
            &layer_tree, &layout_root, self.scroll_y, viewport_h,
            &mut self.layer_paint_cache);

        // D4 GPU layer pipeline pres env var prepinac. Default ON (layer mode).
        // POZN: transform vytvori LAYER a layer transform aplikuje JEN compositor.
        // Monolithic (RWE_LAYER_GPU_OFF) ho ztrati -> transform animace ZAMRZNOU.
        // Proto NELZE defaultit monolithic. Layer compositor overhead pri hoveru
        // (re-render layeru, paint 9-280ms) je TODO: fix damage tracking aby
        // re-renderoval jen zmenene layery.
        // Auto-disable D4 pri layer texture exceed GPU max dim (= 8192 default
        // down-level baseline). Pri page_h * scale > max_tex, layer texture
        // clamped = vertical compression v texture = compose stretch back =
        // visible glyph artifact (outline/blurry). Fallback monolithic.
        // D4 layer mode active default. Pres oversized layer (> 8192 phys),
        // tile path activates per-layer = no single big texture alloc, no clamp.
        // Drive any_oversized DISABLOVAL D4 cele = monolithic fallback = user
        // nechce fallback. Tile path handluje oversized.
        // (layer_gpu_mode + monolithic flag uz nastaveny vyse pred build.)
        let mut d4_overlay_start: usize = 0;
        if layer_gpu_mode {
            // CRITICAL: warm atlas PRED layer raster. Pri D4 layer-mode build_vertices
            // potrebuje atlas glyfy hned pri prvni render_into_layer/tile call.
            // POZOR: musim warm pres local_cache cmds (= full layer content) NE
            // display_list ktery je viewport-CULLED (= visible-only chars). Layer
            // texture obsahuje cely layer content - pri scroll exposuje
            // initially-off-screen chars co MUSI byt v atlasu. Drive warm pres
            // display_list -> scroll exposed chars co nikdy nebyly warmnute = chars
            // missing pri scroll.
            let mut local_cache: std::collections::HashMap<usize, Vec<crate::browser::paint::DisplayCommand>>
                = std::collections::HashMap::new();
            build_layer_local_cache(&layer_tree, &layout_root, &mut local_cache);
            let mut flat: Vec<&crate::browser::compositor::LayerNode> = Vec::new();
            crate::browser::compositor::flatten_layers(&layer_tree, &mut flat);
            // Warm atlas pres VSECHNY layer-local cmds (= full content per layer).
            // PERF guard: skip warm loop pri scroll-only frames (DOM/layers stable).
            // Warm jen kdyz nove layery prisly nebo first frame. text_cmd_warmed
            // HashSet by stejne skipnul vsechny per-cmd hash lookups ale iterace
            // 5000+ cmds + hash compute = ~5-10 ms wasted.
            // damage_rect.is_some() na ANY layer = need warm new content.
            let any_damaged = flat.iter().any(|l| l.damage_rect.is_some());
            if any_damaged {
                // Warm JEN DAMAGED layery. Nedamaged layery maji glyfy uz warmnute
                // z framu kdy byly novy/damaged (atlas je perzistentni). Drive se
                // warmovaly VSECHNY layery pri ANY damage = iterace 5000+ cmds +
                // hash kazdy hover frame = ~9ms (hlavni hover lag, "velke zpozdeni").
                // Per-layer raster_scale - scale(N) layer warmuje glyfy v N x vetsim
                // physical_size = ostry text u scale(N).
                for layer in &flat {
                    if layer.damage_rect.is_none() { continue; }
                    if let Some(cmds) = local_cache.get(&layer.id) {
                        let rs = layer_raster_scale(&layer.transforms);
                        renderer.warm_atlas_for_scaled(cmds, self.base_url.as_deref(), rs);
                    }
                }
            }
            // Auto-detect tile mode per layer:
            // Layer dim * sf > GPU max_tex_dim (= 8192 baseline) -> tile path.
            // Tile cesta = grid 256-logical tiles, kazdy < 8192 phys = NO clamp,
            // NO compose stretch artifact (= "outline text" bug). Damage tracking
            // per-tile -> dirty tile = re-raster; cached tiles preserved.
            // Pres small layer = single texture path (faster, 1 compose).
            const MAX_TEX_DIM: f32 = 8192.0;
            let sf = self.zoom * self.scale_factor;
            let force_tiles = std::env::var("RWE_FORCE_TILES").is_ok();
            let needs_tiles = |layer: &crate::browser::compositor::LayerNode| -> bool {
                if layer.tiles.is_empty() { return false; }
                if force_tiles { return true; }
                // Tile mode pri layer phys overflow texture max (= avoid clamp +
                // double-interp glyph blur artifact). Pres small layers vyhodnejsi
                // single texture path (1 compose draw vs N tile draws).
                (layer.root_rect.width * sf).max(layer.root_rect.height * sf) > MAX_TEX_DIM
            };
            let mut d4_renders = 0u32;
            let mut d4_tile_renders = 0u32;
            for layer in &flat {
                // Re-raster kdyz damaged NEBO se prave (re)vytvorila texture
                // (size change ze scale spring overshoot) - jinak nova prazdna
                // texture = vanish (flip scaleX(-1) na konci animace mizel).
                if layer.damage_rect.is_none() && !recreated_layer_tex.contains(&layer.id) { continue; }
                let cmds = match local_cache.get(&layer.id) {
                    Some(c) if !c.is_empty() => c.clone(),
                    _ => continue,
                };
                if needs_tiles(layer) {
                    // Per-tile raster: kazdou dirty tile vykresli do tile texture.
                    for (idx, tile) in layer.tiles.iter().enumerate() {
                        if !tile.dirty { continue; }
                        let key = (layer.id, idx);
                        let view = self.tile_textures.get(&key).map(|s| s.view.clone());
                        if let Some(view) = view {
                            renderer.render_into_tile(
                                &view,
                                tile.local_rect.x, tile.local_rect.y,
                                tile.local_rect.width.max(1.0),
                                tile.local_rect.height.max(1.0),
                                &cmds,
                            );
                            d4_tile_renders += 1;
                        }
                    }
                } else {
                    // Whole-layer raster (small layers, fits texture).
                    let view = self.layer_textures.get(&layer.id).map(|s| s.view.clone());
                    if let Some(view) = view {
                        let lw = layer.root_rect.width.max(1.0);
                        let lh = layer.root_rect.height.max(1.0);
                        let rscale = layer_raster_scale(&layer.transforms);
                        renderer.render_into_layer_scaled(&view, lw, lh, rscale, &cmds);
                        d4_renders += 1;
                    }
                }
            }
            if std::env::var("RWE_DAMAGE_DBG").is_ok() {
                eprintln!("[D4 GPU] {} layers, {} tiles rendered ({}/{} total layers)",
                    d4_renders, d4_tile_renders, d4_renders + d4_tile_renders, flat.len());
            }
            // Track display_list len at point overlay items zacinaji byt appendovany.
            // Vse pred = layer content (replaced by composite). Vse po = overlay
            // (scrollbar, devtools, canvas overlay) -> painted to target_view PO composite.
            d4_overlay_start = display_list.len();
        }

        self.last_layer_tree = Some(layer_tree);

        // 3-canvas. Canvas2D ops -> DisplayCommands (po body paint).
        if let Some(interp) = self.interpreter.as_ref() {
            let canvas_ops = interp.canvas_ops.borrow();
            crate::browser::render::canvas_paint::paint_canvas_ops(
                &layout_root, &canvas_ops, &mut display_list);
        }

        // 3-caret. Blinking caret jen na focused TEXT-EDITABLE input/textarea.
        // NE checkbox/radio/button/range (tam se objevoval text kursor po kliku).
        if let Some(focused) = self.focused_dom_node() {
            let typ = focused.attr("type").unwrap_or_else(|| "text".into()).to_lowercase();
            let is_input = focused.tag_name().as_deref() == Some("textarea")
                || (focused.tag_name().as_deref() == Some("input")
                    && matches!(typ.as_str(), "text" | "email" | "password" | "url"
                        | "tel" | "search" | "number" | ""));
            if is_input {
                let nid = std::rc::Rc::as_ptr(&focused) as usize;
                let value = focused.attr("value").unwrap_or_default();
                let chars: Vec<char> = value.chars().collect();
                let caret = (*self.input_caret.get(&nid).unwrap_or(&chars.len()))
                    .min(chars.len());
                // Find LayoutBox pre this node (walk layout_root).
                fn find_box<'a>(b: &'a crate::browser::layout::LayoutBox, target_id: usize)
                    -> Option<&'a crate::browser::layout::LayoutBox> {
                    if let Some(n) = &b.node {
                        if std::rc::Rc::as_ptr(n) as usize == target_id {
                            return Some(b);
                        }
                    }
                    for ch in &b.children {
                        if let Some(f) = find_box(ch, target_id) { return Some(f); }
                    }
                    None
                }
                if let Some(input_box) = find_box(&layout_root, nid) {
                    let weight = input_box.effective_weight();
                    // Single source of truth: shape_text vraci stejne advance
                    // jako measure_text_width_full (= layout canonical).
                    // x_at_char pres ShapedText cumulative pole - bez separateho
                    // prefix sum (drive prefix_w mereny zvlast, mohl rounding-differ).
                    let (_runs, shaped) = crate::browser::editor::shape_text(
                        &value, input_box.font_size, weight, input_box.italic,
                        &input_box.font_family, input_box.letter_spacing);
                    let prefix_w = shaped.x_at_char(caret);
                    // Pad z node-specific - musi shodovat s paint.rs text_x.
                    // Pad_l asymmetric pripady (padding_left wins).
                    let pad_l = input_box.padding_left.unwrap_or(input_box.padding);
                    let pad_t = input_box.padding_top.unwrap_or(input_box.padding);
                    let pad_b = input_box.padding_bottom.unwrap_or(input_box.padding);
                    let border = input_box.border_width.max(0.0);
                    // Inner h pro vertical centering (CSS technique stejna jako
                    // paint.rs vertical center pres v_offset = (inner_h - 1.5*fs)/2).
                    let inner_h = input_box.rect.height - pad_t - pad_b - 2.0 * border;
                    let v_offset = ((inner_h - 1.5 * input_box.font_size) * 0.5).max(0.0);
                    let caret_x = input_box.rect.x + border + pad_l + prefix_w;
                    let caret_y = input_box.rect.y + border + pad_t + v_offset;
                    let caret_h = input_box.font_size * 1.2;
                    // Blink 1 Hz: even seconds visible, odd off.
                    let elapsed = self.animation_origin.elapsed().as_secs_f32();
                    let blink_on = (elapsed * 2.0) as i32 % 2 == 0;
                    if blink_on {
                        // Caret barva: kontrastni proti bg (default tmavy text
                        // na svetlem bg = cerny caret; light text na tmavem = bily).
                        // Heuristika dle text_color luma.
                        let text_color = input_box.text_color.unwrap_or([20, 20, 20, 255]);
                        let luma = 0.299 * text_color[0] as f32
                                 + 0.587 * text_color[1] as f32
                                 + 0.114 * text_color[2] as f32;
                        // Pokud text je svetly (luma > 128), caret bily; jinak cerny.
                        let caret_color = if luma > 128.0 {
                            [220, 220, 230, 255]
                        } else {
                            [20, 20, 30, 255]
                        };
                        display_list.push(crate::browser::paint::DisplayCommand::Rect {
                            x: caret_x, y: caret_y,
                            w: 1.5, h: caret_h,
                            color: caret_color, radius: 0.0,
                        });
                    }
                }
            }
        }

        // 3-sel. Text selection highlight - kdy page_selection Some, emit
        // modry Rect overlays nad selected text runs.
        if let Some(interp) = self.interpreter.as_ref() {
            let doc = interp.document.borrow();
            let reg = doc.selection.borrow();
            if let Some(ps) = reg.page_selection.as_ref() {
                let a = ps.anchor;
                let c = ps.current;
                let (start, end) = if a.1 < c.1 || (a.1 == c.1 && a.0 <= c.0) {
                    (a, c)
                } else { (c, a) };
                if (end.0 - start.0).abs() > 1.0 || (end.1 - start.1).abs() > 1.0 {
                    // PER-ELEMENT ::selection: kazdy hit nese selection_bg/color
                    // sveho boxu. Scoped `.foo::selection` se drzi jen ve .foo.
                    let mut hits: Vec<(f32, f32, f32, f32, Option<[u8; 4]>, Option<[u8; 4]>)> = Vec::new();
                    collect_text_lines(&layout_root, start.0, start.1, end.0, end.1, &mut hits);
                    // 1) bg rect pod text (alpha < 255 aby text prosvital = citelny
                    // bez prebarveni). S gamma-space blendem (rgba fix) blenduje
                    // ted spravne jako Chrome. Alpha 150/120 = subtle highlight,
                    // original text prosvita.
                    for (hx, hy, hw, hh, sel_bg, _) in &hits {
                        let color = sel_bg
                            .map(|c| if c[3] >= 255 { [c[0], c[1], c[2], 150] } else { c })
                            .unwrap_or([80, 150, 255, 120]);
                        display_list.push(crate::browser::paint::DisplayCommand::Rect {
                            x: *hx, y: *hy, w: *hw, h: *hh, color, radius: 0.0,
                        });
                    }
                    // 2) ::selection {color} prebarvi text JEN kdyz je explicitne
                    // nastaveny (sel_col Some). NEPREBARVUJEME automaticky - text je
                    // jeden DisplayCommand per uzel, takze "always recolor" prebarvil
                    // CELY uzel i kdyz byla vybrana jen cast (regrese). Subtle bg
                    // (krok 1) zajisti citelnost i bez recoloringu.
                    let recolor: Vec<crate::browser::paint::DisplayCommand> = display_list.iter()
                        .filter_map(|cmd| {
                            if let crate::browser::paint::DisplayCommand::Text { x, y, .. } = cmd {
                                for (hx, hy, hw, hh, _, sel_col) in &hits {
                                    if let Some(sc) = sel_col {
                                        if *x >= hx - 2.0 && *x <= hx + hw + 2.0
                                            && *y >= hy - 2.0 && *y <= hy + hh + 2.0 {
                                            let mut c = cmd.clone();
                                            if let crate::browser::paint::DisplayCommand::Text { color, .. } = &mut c {
                                                *color = *sc;
                                            }
                                            return Some(c);
                                        }
                                    }
                                }
                            }
                            None
                        }).collect();
                    display_list.extend(recolor);
                }
            }
        }

        // 3z. Overlay painter callback - hostujici aplikace emit DODATECNE
        // DisplayCommands (inspector highlight, devtools, ...). Volane PRED
        // scroll shift -> overlay coords v content-space.
        if let Some(painter) = self.overlay_painter.as_mut() {
            painter(&layout_root, self.scroll_y, &mut display_list);
        }

        // 3a. Apply scroll: posun page commands o -scroll_y/x. Respektuje
        // NoScrollShiftBegin/End markers (position:fixed subtree zustava
        // staticke vuci viewportu). Scrollbar overlay (pridany nize) je
        // viewport-relative -> add PO shift.
        crate::browser::render::segments::apply_scroll_shift(
            &mut display_list, -self.scroll_x, -self.scroll_y);

        // 3a2. <select> open dropdown overlay - viewport-relative emit.
        if let Some((select_id, anchor_x, anchor_y, anchor_w)) = self.open_select {
            if let Some(interp) = self.interpreter.as_ref() {
                let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                if let Some(select_node) = crate::browser::render::find_node_by_ptr(&doc_root, select_id) {
                    let opt_h = 24.0_f32;
                    let pad_l = 8.0_f32;
                    let popup_x = anchor_x;
                    let popup_y = anchor_y + 24.0 - self.scroll_y;
                    let options: Vec<std::rc::Rc<crate::browser::dom::Node>> = select_node.children.borrow()
                        .iter().filter(|c| c.tag_name().as_deref() == Some("option")).cloned().collect();
                    let popup_h = opt_h * options.len() as f32;
                    if popup_h > 0.0 {
                        display_list.push(crate::browser::paint::DisplayCommand::Shadow {
                            x: popup_x, y: popup_y, w: anchor_w, h: popup_h,
                            offset_x: 0.0, offset_y: 2.0, blur: 8.0, spread: 0.0,
                            color: [0, 0, 0, 80], radius: 4.0, inset: false,
                        });
                        display_list.push(crate::browser::paint::DisplayCommand::Rect {
                            x: popup_x, y: popup_y, w: anchor_w, h: popup_h,
                            color: [255, 255, 255, 255], radius: 4.0,
                        });
                        display_list.push(crate::browser::paint::DisplayCommand::Border {
                            x: popup_x, y: popup_y, w: anchor_w, h: popup_h,
                            width: 1.0, color: [200, 200, 210, 255],
                        });
                    }
                    for (idx, opt) in options.iter().enumerate() {
                        let opt_y = popup_y + (idx as f32) * opt_h;
                        let hovered = self.mouse_x >= popup_x && self.mouse_x < popup_x + anchor_w
                            && self.mouse_y >= opt_y && self.mouse_y < opt_y + opt_h;
                        if hovered {
                            display_list.push(crate::browser::paint::DisplayCommand::Rect {
                                x: popup_x, y: opt_y, w: anchor_w, h: opt_h,
                                color: [230, 240, 255, 255], radius: 0.0,
                            });
                        }
                        let txt = opt.text_content().trim().to_string();
                        display_list.push(crate::browser::paint::DisplayCommand::Text {
                            x: popup_x + pad_l, y: opt_y + 6.0,
                            content: txt,
                            color: [40, 40, 50, 255],
                            font_size: 14.0, bold: false, font_weight: 400,
                            italic: false,
                            font_family: String::new(),
                            strikethrough: false, underline: false,
                        });
                    }
                }
            }
        }

        // 3b. Scrollbar overlay - kdyz content > viewport.
        // PERF: emit AT END display_list aby byl nad page contents.
        crate::browser::paint::emit_main_scrollbar_overlay(
            &layout_root, &mut display_list,
            viewport_w, viewport_h,
            self.scroll_x, self.scroll_y,
        );

        // 4. Warm-up glyph atlas + image atlas pred draw.
        // Pri D4 layer_gpu_mode warm uz probehl vyse pres local_cache (= full
        // layer content). Tady jen monolithic path nebo D4 overlay items.
        if !layer_gpu_mode {
            renderer.warm_atlas_for(&display_list, self.base_url.as_deref());
        }

        // 4b. Extract text runs (per-glyph cumulative advances) - foundation
        // pro per-glyph hit-test selection. Walks display_list TEXT cmds +
        // measure pres atlas. Page cmds only (overlay text neselectable).
        self.painted_text_runs = crate::browser::render::extract_text_runs(
            &display_list, renderer.atlas(), renderer.zoom);

        let prof_t3 = std::time::Instant::now();
        self.prof_paint_ms = prof_t3.duration_since(prof_t2).as_secs_f32() * 1000.0;

        // 5. Renderer kresli display list do target_view.
        // D4 plne pipeline pri layer_gpu_mode:
        //   - Skip monolithic draw, misto toho composite layer textures
        //   - Walk layer_tree z-order, per layer compose_view_to_view do target_view
        //   - Apply viewport scroll (subtract scroll_x/y, except fixed layers)
        //   - Pak overlay-only cmds (po d4_overlay_start) draw nad composite
        let target_view = self.target_view.as_ref()?;
        // Pro mix-blend-mode layer compose potreba Texture handle (snapshot backdrop).
        let target_texture = self.target_texture.as_ref()?;
        let _tile_gpu_mode = std::env::var("RWE_TILE_GPU").is_ok();
        if layer_gpu_mode {
            // Composite all layers into target_view.
            let layer_tree_ref = self.last_layer_tree.as_ref()
                .expect("layer_tree saved pred draw");
            let mut flat: Vec<&crate::browser::compositor::LayerNode> = Vec::new();
            crate::browser::compositor::flatten_layers(layer_tree_ref, &mut flat);
            // Single encoder pres celou compose phase = 1 submit (drive N submits/frame
            // = ~25-80x mensi GPU sync overhead).
            let mut compose_encoder = renderer.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("d4_compose_batch") });
            let mut first = true;
            for layer in &flat {
                let is_fixed = matches!(layer.reason,
                    crate::browser::compositor::LayerReason::PositionFixed);
                let pos_x = layer.root_rect.x - if is_fixed { 0.0 } else { self.scroll_x };
                let pos_y = layer.root_rect.y - if is_fixed { 0.0 } else { self.scroll_y };
                // Tile compose path - walk per-tile textures + compose kazdou.
                let sf_compose = self.zoom * self.scale_factor;
                let force_tiles_c = std::env::var("RWE_FORCE_TILES").is_ok();
                let layer_needs_tiles = !layer.tiles.is_empty()
                    && (force_tiles_c
                        || (layer.root_rect.width * sf_compose).max(layer.root_rect.height * sf_compose) > 8192.0);
                if layer_needs_tiles && layer.transform.is_none() {
                    for (idx, tile) in layer.tiles.iter().enumerate() {
                        let key = (layer.id, idx);
                        let tile_view = match self.tile_textures.get(&key) {
                            Some(s) => s.view.clone(),
                            None => continue,
                        };
                        renderer.compose_view_to_view_into_encoder(
                            &mut compose_encoder,
                            target_view, &tile_view,
                            pos_x + tile.local_rect.x,
                            pos_y + tile.local_rect.y,
                            tile.local_rect.width,
                            tile.local_rect.height,
                            layer.opacity.clamp(0.0, 1.0),
                            first,
                        );
                        first = false;
                    }
                    continue;
                }
                let view = match self.layer_textures.get(&layer.id) {
                    Some(s) => s.view.clone(),
                    None => continue,
                };
                if std::env::var("RWE_COMPOSE_DBG").is_ok() {
                    eprintln!("[COMPOSE] layer={} root_rect=({},{},{},{}) pos=({},{}) scroll=({},{}) damage={:?} transform={:?}",
                        layer.id, layer.root_rect.x, layer.root_rect.y,
                        layer.root_rect.width, layer.root_rect.height,
                        pos_x, pos_y, self.scroll_x, self.scroll_y,
                        layer.damage_rect, layer.transform);
                }
                if layer.transform.is_some() {
                    // Layer texture obsahuje UNTRANSFORMED content (paint emit
                    // skip transform pri layer). Compose pres transform pipeline
                    // rotuje quad pres GPU (= rotates content + glyphs spolu).
                    // Center = pos + w/2 = element rect center (layer.root_rect
                    // = orig rect = element bbox).
                    // POUZIVAME PLNY CHAIN (layer.transforms), ne singular
                    // layer.transform - jinak multi-op 3D (rotateX+rotateY,
                    // perspective+rotateY) dostane jen rozbity prvni op.
                    let m = crate::browser::layout::compute_transform_matrix(
                        &layer.transforms, None);
                    renderer.compose_view_to_view_transform_into_encoder(
                        &mut compose_encoder,
                        target_view, &view,
                        pos_x, pos_y,
                        layer.root_rect.width, layer.root_rect.height,
                        &m, first,
                    );
                } else if !first && !matches!(layer.blend_mode,
                        crate::browser::computed_style::BlendMode::Normal) {
                    // mix-blend-mode: blend layer tex pres dosud vykompozitlovany
                    // backdrop. !first = existuje backdrop (jinak normal compose).
                    let mode_id = blend_mode_discriminant(layer.blend_mode);
                    renderer.compose_blend_layer_into_encoder(
                        &mut compose_encoder,
                        target_view, target_texture, &view,
                        pos_x, pos_y,
                        layer.root_rect.width, layer.root_rect.height,
                        mode_id,
                        layer.opacity.clamp(0.0, 1.0),
                    );
                } else {
                    renderer.compose_view_to_view_into_encoder(
                        &mut compose_encoder,
                        target_view, &view,
                        pos_x, pos_y,
                        layer.root_rect.width, layer.root_rect.height,
                        layer.opacity.clamp(0.0, 1.0),
                        first,
                    );
                }
                first = false;
            }
            // Submit batched compose encoder = single GPU sync.
            renderer.queue.submit(std::iter::once(compose_encoder.finish()));
            // Po composite: draw overlay-only cmds (vse appendoval po build_layered).
            if d4_overlay_start < display_list.len() {
                let overlay = &display_list[d4_overlay_start..];
                if !overlay.is_empty() {
                    renderer.draw_segments_into_view_clipped(
                        target_view, overlay, first, None);
                }
            } else if first {
                // No layers at all - clear target_view via draw_segments empty path
                // (alespoň clear color). Pri page bez layers (rare) by zustal stale.
            }
        } else {
            let _had = renderer.draw_segments_into_view_clipped(
                target_view, &display_list, true, None);
        }

        // 5b. WebGL canvas frame - per <canvas> s WebGL state encode wgpu
        // draw passes do per-canvas RT + compose do target_view. NO-OP pri
        // zadnem WebGL canvasu na strance.
        if let Some(interp) = self.interpreter.as_ref() {
            let webgl_states = interp.webgl_states.clone();
            let states = webgl_states.borrow();
            if std::env::var("RWE_WEBGL_DBG").is_ok() {
                eprintln!("[webgl] states.len={} (canvases with WebGLState)",
                    states.len());
                for (ptr, state) in states.iter() {
                    let q_len = state.borrow().draw_queue.len();
                    eprintln!("  canvas_ptr={} draw_queue.len={}", ptr, q_len);
                }
            }
            if !states.is_empty() {
                let did = renderer.run_webgl_frame(&layout_root, target_view, &*states, self.scroll_y);
                if std::env::var("RWE_WEBGL_DBG").is_ok() {
                    eprintln!("[webgl] run_webgl_frame returned: {}", did);
                }
            }
        }

        // 6. Stash layout_root pro hostujici aplikaci (overlay paint pass).
        // Populate layout_rects (node ptr -> rect) + cascade_props sdilene
        // s interpreter lookups (getBoundingClientRect / getComputedStyle).
        {
            let mut rects = self.layout_rects.borrow_mut();
            rects.clear();
            populate_layout_rects(&layout_root, self.scroll_x, self.scroll_y, &mut rects);
        }
        {
            // PERF: drive `props.insert(*ptr, style.clone())` per element =
            // O(N * keys) hashmap clone (~30ms pri 3434 elementu pres devtools).
            // Ted Rc::clone = 1us. Callback dela lookup pres borrow().as_ref().
            *self.cascade_props.borrow_mut() = Some(std::rc::Rc::clone(&style_map));
        }
        // last_layout_root uz set pred apply_element_scroll (CLEAN snapshot).
        // Zde NO-OP: kdyz nebyl element scroll active, save_layout_root_at_end
        // dovoluje skip - ale jednodussi je vzdy nastavit clean pre-apply.
        // (Drop layout_root - byla mutated kopie pro paint.)
        if save_layout_root_at_end {
            self.last_layout_root = Some(layout_root);
        } else {
            drop(layout_root);
        }

        // Reset renderer target_size override - shell present_split + jine
        // pas v swap chain pouziva config size.
        renderer.target_size = None;

        let prof_t4 = std::time::Instant::now();
        self.prof_gpu_ms = prof_t4.duration_since(prof_t3).as_secs_f32() * 1000.0;
        if std::env::var("RWE_PROF").is_ok() {
            let total = prof_t4.duration_since(prof_t0).as_secs_f32() * 1000.0;
            eprintln!("[PROF] total={:.1}ms cascade={:.1} layout={:.1} paint={:.1} gpu={:.1}",
                total, self.prof_cascade_ms, self.prof_layout_ms, self.prof_paint_ms, self.prof_gpu_ms);
        }

        // Frame done - mark v paceru pro telemetry.
        self.frame_pacer.mark_presented(_frame_idx);

        self.dirty = false;
        self.target_view.as_ref()
    }

    /// Aktivni offscreen render target view (vyrobeny v `render`).
    /// Pouziti: host kompozici - blit tuto texturu do swap chain.
    pub fn target_view(&self) -> Option<&wgpu::TextureView> {
        self.target_view.as_ref()
    }

    /// Aktivni offscreen texture (alternativa k `target_view` pro shell
    /// kompozici - texture handle umoznuje create_view s vlastnim format).
    pub fn target_texture(&self) -> Option<&wgpu::Texture> {
        self.target_texture.as_ref()
    }

    /// Velikost dokumentu (content w / h) pro scrollbar sizing v shellu.
    /// Spousti layout pres aktualni viewport + cascade. Pomerne drahe -
    /// hostujici aplikace by ho mela volat opportunisticky (po load_html
    /// / resize), ne kazdy frame.
    pub fn page_size(&self) -> (f32, f32) {
        let doc = match &self.document { Some(d) => d, None => return (0.0, 0.0) };
        let viewport_w = self.viewport_w / self.zoom;
        let viewport_h = self.viewport_h / self.zoom;
        let style_map = crate::browser::cascade::cascade_with_viewport(
            &doc.root, &self.stylesheets, viewport_w, viewport_h);
        let layout_root = crate::browser::layout::layout_tree(
            &doc.root, &style_map, viewport_w, viewport_h);
        let content_w = layout_root.rect.width.max(viewport_w);
        let content_h = layout_root.rect.height.max(viewport_h);
        (content_w, content_h)
    }

    /// Nastav scroll position (instant - smooth target taky aktualizovan
    /// aby nasledne wheel scroll nezacal z stale hodnoty).
    pub fn set_scroll(&mut self, x: f32, y: f32) {
        if (self.scroll_x - x).abs() > 0.5 || (self.scroll_y - y).abs() > 0.5 {
            self.scroll_x = x;
            self.scroll_y = y;
            self.scroll_target_x = x;
            self.scroll_target_y = y;
            // Programatic set: zrus aktivni smooth scroll anim aby nepokracovala
            // na stary target. Bez tohohle by anim po set_scroll pokracovala
            // na puvodni hodnotu = jump back na anim end.
            self.scroll_anim_x = None;
            self.scroll_anim_y = None;
            self.dirty = true;
            // Sync interp.scroll_pos pro JS window.pageXOffset/scrollX reads.
            if let Some(interp) = self.interpreter.as_ref() {
                *interp.scroll_pos.borrow_mut() = (x, y);
                self.last_synced_scroll_pos = (x, y);
            }
            // Dispatch window 'scroll' event do JS.
            if let Some(interp) = self.interpreter.as_mut() {
                interp.dispatch_window_event("scroll", crate::interpreter::JsValue::Undefined);
            }
        }
    }

    /// Aktualni scroll position.
    pub fn scroll(&self) -> (f32, f32) { (self.scroll_x, self.scroll_y) }

    /// Web Vitals snapshot - LCP/CLS/INP collector pres last paint.
    /// Pres `browser::web_vitals::WebVitalsCollector`. Read-only view.
    pub fn web_vitals(&self) -> &crate::browser::web_vitals::WebVitalsCollector {
        &self.web_vitals
    }

    /// Frame pacer - per-frame stage timings + drop counter.
    /// Pres `browser::render::frame_pacing::FramePacer`. Read-only.
    pub fn frame_pacer(&self) -> &crate::browser::render::frame_pacing::FramePacer {
        &self.frame_pacer
    }

    /// Aktualni zoom (1.0 = 100%).
    pub fn zoom(&self) -> f32 { self.zoom }

    /// Aktualni viewport (logical CSS px) sirka.
    pub fn viewport_size(&self) -> (f32, f32) { (self.viewport_w, self.viewport_h) }

    /// HiDPI scale_factor (1.0 / 1.5 / 2.0 ...).
    pub fn scale_factor(&self) -> f32 { self.scale_factor }

    /// `true` pokud stylesheets obsahuji @keyframes / aktivni CSS transitions
    /// / smooth scroll still tweening / focused input (caret blink).
    /// Hostujici aplikace pak request_redraw kazdy frame dokud nestihnem
    /// ustaleni.
    pub fn has_active_animations(&self) -> bool {
        // Alias - kanonicky check pres needs_continuous_render. Shell loop
        // pres tento kontroluje "render next frame?". One source of truth.
        self.needs_continuous_render()
    }

    /// D4.5 - Compositor-only frame detekce. Vraci true kdyz vsechny active
    /// animations / transitions affect POUZE transform/opacity (= GPU compositor
    /// can update composite uniforms bez paint). Foundation pro skip-layer-paint
    /// optimization v plne D4 GPU pipeline.
    /// Inspired by Chromium `cc/animation/animation_host.cc::AnimationsPreserveAxisAlignment`
    /// + WebRender PictureCompositeMode.
    pub fn is_compositor_only_frame(&self) -> bool {
        if self.active_animations.is_empty() && self.active_transitions.is_empty() {
            return false; // no anim = no skip relevant
        }
        // Iterate active animations - check keyframes obsahuje jen transform/opacity.
        // For each (node_id, anim_name) projdeme stylesheets keyframes a kontrolujeme.
        for (_node_id, anim_name) in &self.active_animations {
            let mut anim_compositor_ok = false;
            'outer: for sheet in &self.stylesheets {
                for kf in &sheet.keyframes {
                    if kf.name != *anim_name { continue; }
                    let all_props_compositor = kf.frames.iter().all(|(_, decls)| {
                        decls.iter().all(|d| {
                            matches!(d.property.as_str(), "transform" | "opacity"
                                | "filter" | "translate" | "rotate" | "scale")
                        })
                    });
                    if all_props_compositor {
                        anim_compositor_ok = true;
                    }
                    break 'outer;
                }
            }
            if !anim_compositor_ok { return false; }
        }
        // Active transitions - check property je transform/opacity.
        for at in &self.active_transitions {
            if !matches!(at.property.as_str(), "transform" | "opacity"
                | "filter" | "translate" | "rotate" | "scale") {
                return false;
            }
        }
        true
    }

    /// Single source of truth: vraci true kdyz frame N+1 ma byt render bez
    /// novyho input eventu. Inspired by Chromium BeginMainFrame trigger flags:
    /// 1. @keyframes/transitions active (active_animations / active_transitions)
    /// 2. Smooth scroll lerp v progressu (viewport target vs current)
    /// 3. Caret blink (focused input - blink loop)
    /// 4. setTimeout/setInterval pending (JS timer callback fires)
    ///
    /// Nepatri sem: element scroll, hover state, DOM mutace - ty set dirty=true
    /// pri vzniku a render single frame. Continuous loop neni potreba.
    pub fn needs_continuous_render(&self) -> bool {
        self.needs_animation_render()
            || self.has_pending_intervals()
            || self.has_pending_raf()
    }

    /// Potreba FULL render (layout+paint) tento frame i bez dirty: CSS anim/
    /// transition/smooth-scroll/caret = meni visual KAZDY frame. NEzahrnuje
    /// intervaly/RAF - ty se drainuji v render_via PRED frame-skip a pokud
    /// callback neco zmenil, dirty se nastavi (dom_version/canvas_gen diff).
    /// Bez tohoto rozdeleni full render kazdy frame jen kvuli setInterval/RAF
    /// co nic nemeni = devtools/stranka <30 FPS i kdyz se NIC nedeje.
    pub fn needs_animation_render(&self) -> bool {
        !self.active_animations.is_empty()
            || !self.active_transitions.is_empty()
            || self.scroll_anim_y.is_some()
            || self.scroll_anim_x.is_some()
            || (self.scroll_target_y - self.scroll_y).abs() > 0.5
            || (self.scroll_target_x - self.scroll_x).abs() > 0.5
            || self.focused_is_input()
    }

    /// Nastav zoom level. Stejne jako resize trigger relayout.
    pub fn set_zoom(&mut self, zoom: f32) {
        let z = zoom.clamp(0.25, 5.0);
        if (self.zoom - z).abs() > 0.001 {
            self.zoom = z;
            self.dirty = true;
            // Invalidate layer textures (size depends na zoom). Bez tohoto
            // realloc'd empty tex zustane prazdny (damage=None = no re-raster).
            // Clear vse + prev fingerprints -> next frame mark_damage detects
            // "new layer" (prev_fp None) -> damage=Some -> render_into_layer
            // raster fresh content.
            self.layer_textures.clear();
            self.tile_textures.clear();
            self.prev_layer_fingerprints.clear();
            self.prev_tile_fingerprints.clear();
            self.layer_paint_cache.clear();
            self.last_paint_fingerprint = None;
            self.layout_cache_key = None;
            self.cascade_cache_key = None;
            // Invalidate prev_root + element scroll cache. Prev layout boxes
            // drzí rect.width / explicit_width z prev zoom - pres cache hit
            // pres subtree by aplikoval STARY viewport-relative dims (= inner
            // elements ne-wrappuji pri novem zoom uziejsim viewportu).
            self.last_layout_root = None;
        }
    }

    /// Page title (z `<title>` ci `document.title = ...`).
    pub fn title(&self) -> &str { &self.title }

    /// Nastavi <input type=range> value dle x pozice (klik/drag) + fire input +
    /// change event. content_x = x ve content coords (scroll uz pricten).
    pub(crate) fn set_range_from_x(&mut self, target: &std::rc::Rc<crate::browser::dom::Node>, content_x: f32) {
        let rect = self.last_layout_root.as_ref()
            .and_then(|root| crate::browser::paint::find_box_by_node_id(root, std::rc::Rc::as_ptr(target) as usize))
            .map(|bx| (bx.rect.x, bx.rect.width));
        let (rx, rw) = match rect { Some(v) => v, None => return };
        let pf = |n: &str, d: f32| target.attr(n).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
        let (min, max) = (pf("min", 0.0), pf("max", 100.0));
        let step = pf("step", 1.0).max(1e-4);
        let frac = ((content_x - rx) / rw.max(1.0)).clamp(0.0, 1.0);
        let raw = min + frac * (max - min);
        let val = (((raw - min) / step).round() * step + min).clamp(min.min(max), min.max(max));
        let vs = if (val - val.round()).abs() < 1e-4 { format!("{}", val.round() as i64) }
                 else { format!("{}", (val * 1000.0).round() / 1000.0) };
        if target.attr("value").as_deref() == Some(vs.as_str()) { return; }
        target.set_attr("value", &vs);
        if let Some(interp) = self.interpreter.as_mut() {
            interp.bump_dom_version();
            for evt in ["input", "change"] {
                let mut event = crate::interpreter::JsObject::new();
                event.set("type".into(), crate::interpreter::JsValue::Str(evt.into()));
                event.set("target".into(), crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(target)));
                let event_val = crate::interpreter::JsValue::Object(std::rc::Rc::new(std::cell::RefCell::new(event)));
                let _ = interp.dispatch_event(target, evt, event_val);
            }
        }
        self.dirty = true;
    }

    /// Vybere option v <select> dle indexu - set `selected` attr na zvolenou,
    /// vymaz z ostatnich, nastav select `value` + fire input/change. Layout pak
    /// (collapsed text) reflektuje novou hodnotu.
    pub(crate) fn select_pick_option(&mut self, select_node: &std::rc::Rc<crate::browser::dom::Node>, idx: usize) {
        let options: Vec<std::rc::Rc<crate::browser::dom::Node>> = select_node.children.borrow()
            .iter().filter(|c| c.tag_name().as_deref() == Some("option")).cloned().collect();
        if let Some(opt) = options.get(idx) {
            for o in &options { o.remove_attr("selected"); }
            opt.set_attr("selected", "");
            let val = opt.attr("value")
                .unwrap_or_else(|| opt.text_content().trim().to_string());
            select_node.set_attr("value", &val);
            if let Some(interp) = self.interpreter.as_mut() {
                interp.bump_dom_version();
                for evt in ["input", "change"] {
                    let mut event = crate::interpreter::JsObject::new();
                    event.set("type".into(), crate::interpreter::JsValue::Str(evt.into()));
                    event.set("target".into(), crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(select_node)));
                    let event_val = crate::interpreter::JsValue::Object(std::rc::Rc::new(std::cell::RefCell::new(event)));
                    let _ = interp.dispatch_event(select_node, evt, event_val);
                }
            }
            self.dirty = true;
        }
    }

    /// offsetX/offsetY pro mouse event = pozice relativni k padding-boxu target
    /// elementu (content coords - box origin). Canvas kresleni + pozicni UI to
    /// ctou. Bez nich vraci JS undefined (engine-test canvas mousemove/down).
    fn event_offset(&self, node: &std::rc::Rc<crate::browser::dom::Node>, x: f32, y: f32) -> (f64, f64) {
        let content_x = x + self.scroll_x;
        let content_y = y + self.scroll_y;
        let nid = std::rc::Rc::as_ptr(node) as usize;
        if let Some(root) = self.last_layout_root.as_ref() {
            if let Some(bx) = crate::browser::paint::find_box_by_node_id(root, nid) {
                return ((content_x - bx.rect.x) as f64, (content_y - bx.rect.y) as f64);
            }
        }
        (x as f64, y as f64)
    }

    /// Base URL (file:// / http(s)://).
    pub fn base_url(&self) -> Option<&str> { self.base_url.as_deref() }

    /// Raw HTML source predany pri poslednim `load_html` (preserve).
    pub fn html(&self) -> &str { &self.raw_html }

    /// Raw CSS source (aggregat) predany pri poslednim `load_html` (preserve).
    pub fn css(&self) -> &str { &self.raw_css }

    /// Lokalni filesystem path pokud byla page nactena z file://.
    pub fn local_path(&self) -> Option<&PathBuf> { self.local_path.as_ref() }

    /// Setter pro local_path - shell / host vyplnuje kdyz vie ze file source
    /// (load_url s file:// to vyplni automaticky).
    pub fn set_local_path(&mut self, path: Option<PathBuf>) {
        self.local_path = path;
    }

    // -- low-level access (devtools, power users, shell crate) ----------

    /// Pristup k DOM - pro devtools Elements panel, observers.
    pub fn document(&self) -> Option<&Document> { self.document.as_ref() }

    /// Pristup k JS interpretu - pro devtools console eval, debug inspect.
    pub fn interpreter(&self) -> Option<&Interpreter> { self.interpreter.as_ref() }

    /// Mutable interpreter pro `interpreter.run(&program)` z hostujici aplikace
    /// (devtools console execute, JS injection).
    pub fn interpreter_mut(&mut self) -> Option<&mut Interpreter> {
        self.interpreter.as_mut()
    }

    /// Vezmi vlastnictvi interpretu z WebView. Po `take_interpreter` je WebView
    /// bez JS state - dalsi `load_html` ho znovu vytvori. Pouziti: App si bere
    /// interpreter pres `App::reload_from_html` move (transition phase, neez
    /// `App.interpreter` zustane primary).
    pub fn take_interpreter(&mut self) -> Option<Interpreter> {
        self.interpreter.take()
    }

    /// Vlozit existujici interpreter (po external mutation jako devtools
    /// debug step). WebView prevezme ownership.
    pub fn set_interpreter(&mut self, interp: Interpreter) {
        self.interpreter = Some(interp);
        self.dirty = true;
    }

    /// CSS stylesheets v poradi cascade priority.
    pub fn stylesheets(&self) -> &[Stylesheet] { &self.stylesheets }

    /// Engine reference (pro custom rendering hostujici aplikace).
    pub fn engine(&self) -> &Arc<Engine> { &self.engine }

    /// Aktualne focused DOM node (per-WebView focused_node_local).
    fn focused_dom_node(&self) -> Option<std::rc::Rc<crate::browser::dom::Node>> {
        let id = self.focused_node_local?;
        let interp = self.interpreter.as_ref()?;
        let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
        crate::browser::render::find_node_by_ptr(&doc_root, id)
    }

    /// True kdyz tento WebView ma focused input/textarea (per-WebView).
    pub fn has_focused_input(&self) -> bool {
        self.focused_dom_node().map(|n|
            matches!(n.tag_name().as_deref(), Some("input") | Some("textarea"))
        ).unwrap_or(false)
    }

    // -- Input editor state -----------------------------------------------

    /// Get-or-init `EditorState` pro <input>/<textarea> node. Synchronizuje
    /// text pres `value` attr (pri externi mutation z JS).
    pub(crate) fn editor_for_node(&mut self, node: &std::rc::Rc<crate::browser::dom::Node>)
        -> &mut crate::browser::editor::EditorState
    {
        let nid = std::rc::Rc::as_ptr(node) as usize;
        let cur_value = node.attr("value").unwrap_or_default();
        let entry = self.editors.entry(nid).or_insert_with(|| {
            crate::browser::editor::EditorState::new(&cur_value)
        });
        // Pri JS mutation `el.value = "x"` text divergne - resync.
        if entry.text != cur_value {
            entry.set_text(&cur_value);
        }
        entry
    }

    /// Hit-test x koord (clientX viewport-relative) na <input>/<textarea>
    /// glyph -> nastav caret. Sync `input_caret` (char-index legacy) z
    /// `editors[nid].caret` (byte). Volane z MouseDown po focusable detekci.
    pub(crate) fn editor_hit_test_input(
        &mut self,
        node: &std::rc::Rc<crate::browser::dom::Node>,
        client_x: f32,
        extend: bool,
    ) {
        let nid = std::rc::Rc::as_ptr(node) as usize;
        // Najdi layout box pre node (potrebujem font_size, family, padding).
        let Some(layout_root) = self.last_layout_root.as_ref() else { return };
        fn find_box<'a>(b: &'a crate::browser::layout::LayoutBox, target_id: usize)
            -> Option<&'a crate::browser::layout::LayoutBox> {
            if let Some(n) = &b.node {
                if std::rc::Rc::as_ptr(n) as usize == target_id { return Some(b); }
            }
            for ch in &b.children {
                if let Some(f) = find_box(ch, target_id) { return Some(f); }
            }
            None
        }
        let Some(input_box) = find_box(layout_root, nid) else { return };
        let weight = input_box.effective_weight();
        let italic = input_box.italic;
        let fam = input_box.font_family.clone();
        let fs = input_box.font_size;
        let ls = input_box.letter_spacing;
        let pad_l = input_box.padding_left.unwrap_or(input_box.padding);
        let border = input_box.border_width.max(0.0);
        // text_origin_x v content-space (= rect.x). client_x je viewport
        // koord -> prevod na content pres scroll_x.
        let content_x = client_x + self.scroll_x;
        let text_origin_x = input_box.rect.x + border + pad_l;
        // Shape pres value (= editor.text after sync).
        let ed = self.editor_for_node(node);
        let (_runs, shaped) = crate::browser::editor::shape_text(
            &ed.text, fs, weight, italic, &fam, ls);
        let local_x = content_x - text_origin_x;
        ed.hit_test(&shaped, local_x, extend);
        // Sync legacy input_caret (char index).
        let char_idx = ed.caret_char_index();
        self.input_caret.insert(nid, char_idx);
    }

    // -- Page selection (text drag) ---------------------------------------

    /// Zacni text selection drag pri MouseDown.
    fn sel_begin(&self, content_x: f32, content_y: f32) {
        let Some(interp) = &self.interpreter else { return };
        let doc = interp.document.borrow();
        doc.selection.borrow_mut().page_selection = Some(
            crate::browser::selection::PageSelection {
                anchor: (content_x, content_y),
                current: (content_x, content_y),
                dragging: true,
                cached_text: String::new(),
            });
    }

    fn sel_update(&self, content_x: f32, content_y: f32) {
        let Some(interp) = &self.interpreter else { return };
        let doc = interp.document.borrow();
        let mut reg = doc.selection.borrow_mut();
        if let Some(ps) = reg.page_selection.as_mut() {
            ps.current = (content_x, content_y);
        }
    }

    fn sel_end(&self) {
        let Some(interp) = &self.interpreter else { return };
        let doc = interp.document.borrow();
        let mut reg = doc.selection.borrow_mut();
        if let Some(ps) = reg.page_selection.as_mut() {
            ps.dragging = false;
            if (ps.anchor.0 - ps.current.0).abs() < 3.0
                && (ps.anchor.1 - ps.current.1).abs() < 3.0 {
                reg.page_selection = None;
            }
        }
    }

    /// Registruj overlay painter - closure ktera emituje DODATECNE
    /// DisplayCommands po build_display_list (PRED scroll shift). Pouziti:
    /// inspector overlay paint (devtools highlight), badge overlays,
    /// custom debugging visualizace.
    ///
    /// Closure signature: `FnMut(&LayoutBox, scroll_y, &mut Vec<cmds>)`.
    pub fn set_overlay_painter(
        &mut self,
        painter: Box<dyn FnMut(
            &crate::browser::layout::LayoutBox,
            f32,
            &mut Vec<crate::browser::paint::DisplayCommand>,
        )>,
    ) {
        self.overlay_painter = Some(painter);
    }

    /// `true` pokud focused element je input nebo textarea (host shell:
    /// Space scroll skip kdyz user pise do inputu).
    pub fn focused_is_input(&self) -> bool {
        self.focused_dom_node()
            .map(|n| matches!(n.tag_name().as_deref(),
                Some("input") | Some("textarea")))
            .unwrap_or(false)
    }

    /// Clear text selection (Esc).
    pub fn clear_selection(&mut self) {
        let Some(interp) = &self.interpreter else { return };
        let doc = interp.document.borrow();
        if doc.selection.borrow().page_selection.is_some() {
            doc.selection.borrow_mut().page_selection = None;
            self.dirty = true;
        }
    }

    /// Select all - anchor (0, 0), current (huge, huge) -> celá stránka.
    pub fn select_all(&mut self) {
        let Some(interp) = &self.interpreter else { return };
        let doc = interp.document.borrow();
        let max = 1_000_000.0_f32;
        doc.selection.borrow_mut().page_selection = Some(
            crate::browser::selection::PageSelection {
                anchor: (0.0, 0.0),
                current: (max, max),
                dragging: false,
                cached_text: String::new(),
            });
        self.dirty = true;
    }

    fn sel_dragging(&self) -> bool {
        self.interpreter.as_ref()
            .map(|i| i.document.borrow().selection.borrow().page_selection
                .as_ref().map(|p| p.dragging).unwrap_or(false))
            .unwrap_or(false)
    }

    /// Extract selected text (anchor->current rect range pres painted_text_runs).
    pub fn selection_text(&self) -> Option<String> {
        let interp = self.interpreter.as_ref()?;
        let doc = interp.document.borrow();
        let reg = doc.selection.borrow();
        let ps = reg.page_selection.as_ref()?;
        let anchor = self.hit_test_text(ps.anchor.0, ps.anchor.1)?;
        let focus = self.hit_test_text(ps.current.0, ps.current.1)?;
        let sel = crate::browser::textrun::TextSelection { anchor, focus };
        Some(sel.extract_text(&self.painted_text_runs))
    }
}

/// Walk layout tree + collect highlight rects pro selected text lines.
/// Flow-based: first/last line maji partial X range, middle full.
/// Walk LayoutBox tree + populate layout_rects mapu (node_ptr -> rect).
/// Pouziti: JS getBoundingClientRect / offsetWidth pres interp.layout_lookup.
/// Pri scroll_x/y odecte (rect je document-space, JS API ocekava viewport-space).
fn populate_layout_rects(
    b: &crate::browser::layout::LayoutBox,
    scroll_x: f32,
    scroll_y: f32,
    out: &mut std::collections::HashMap<usize, (f32, f32, f32, f32)>,
) {
    if let Some(node) = &b.node {
        let ptr = std::rc::Rc::as_ptr(node) as usize;
        // Viewport-space rect: subtract scroll offsets.
        let x = b.rect.x - scroll_x;
        let y = b.rect.y - scroll_y;
        out.insert(ptr, (x, y, b.rect.width, b.rect.height));
    }
    for child in &b.children {
        populate_layout_rects(child, scroll_x, scroll_y, out);
    }
}

fn collect_text_lines(
    b: &crate::browser::layout::LayoutBox,
    sx: f32, sy: f32, ex: f32, ey: f32,
    out: &mut Vec<(f32, f32, f32, f32, Option<[u8; 4]>, Option<[u8; 4]>)>,
) {
    if let Some(text) = &b.text {
        // line_start_x musi pouzit pad_l + border (stejny jako paint.rs
        // text_x = bx.rect.x + pad_l + align_offset). Bez tohoto byla
        // selection posunuta vlevo o pad_l - oznacovala mimo viditelny text.
        let pad_l = b.padding_left.unwrap_or(b.padding);
        let border = b.border_width.max(0.0);
        let bx0 = b.rect.x + border + pad_l;
        let by0 = b.rect.y;
        let by1 = by0 + b.rect.height;
        let lh = (b.line_height * b.font_size).max(b.font_size * 1.2);
        if !(by1 < sy || by0 > ey) {
            let weight = b.effective_weight();
            let lines: Vec<&str> = text.split('\n').collect();
            for (li, line) in lines.iter().enumerate() {
                let line_y = by0 + (li as f32) * lh;
                let line_y_end = line_y + lh;
                if line_y_end < sy || line_y > ey { continue; }
                let is_first_line = sy >= line_y && sy < line_y_end;
                let is_last_line = ey >= line_y && ey < line_y_end;
                let italic = b.italic;
                let fam = b.font_family.clone();
                let ls = b.letter_spacing;
                // Single source of truth: shape_text vraci per-char advance
                // shodne s caret + paint mereni. Drive 2 separate calls
                // (line_w bulk + ch-by-ch acc) co mohly differ pri letter
                // spacing 0 ale s fallback fonts.
                let (_runs, shaped) = crate::browser::editor::shape_text(
                    line, b.font_size, weight, italic, &fam, ls);
                let line_w = shaped.total_width;
                let line_start_x = bx0;
                let (x_lo, x_hi) = if is_first_line && is_last_line {
                    (sx.min(ex), sx.max(ex))
                } else if is_first_line {
                    (sx, line_start_x + line_w)
                } else if is_last_line {
                    (line_start_x, ex)
                } else {
                    (line_start_x, line_start_x + line_w)
                };
                let sel_left = (x_lo - line_start_x).max(0.0);
                let sel_right = (x_hi - line_start_x).min(line_w);
                if sel_right <= sel_left + 0.5 { continue; }
                // Hit-test pres ShapedText: najdi char index pro sel_left
                // (start) a sel_right (end). x_at_char vrati edge X (cumulative).
                let start_char = shaped.char_at_x(sel_left);
                let end_char = shaped.char_at_x(sel_right);
                let hs = shaped.x_at_char(start_char);
                let he = shaped.x_at_char(end_char);
                if he > hs + 0.5 {
                    out.push((line_start_x + hs, line_y, he - hs, lh,
                        b.selection_bg, b.selection_color));
                }
            }
        }
    }
    for ch in &b.children {
        collect_text_lines(ch, sx, sy, ex, ey, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::event::KeyModifiers;

    fn fresh() -> WebView {
        WebView::new(Arc::new(Engine::new_headless()), 1280, 720)
    }

    #[test]
    fn new_webview_is_empty() {
        let wv = fresh();
        assert!(wv.document().is_none());
        assert!(wv.interpreter().is_none());
        assert!(wv.stylesheets().is_empty());
        assert_eq!(wv.title(), "");
        assert_eq!(wv.scroll(), (0.0, 0.0));
        assert_eq!(wv.zoom(), 1.0);
    }

    #[test]
    fn load_html_populates_state() {
        let mut wv = fresh();
        let html = "<html><head><title>Test Page</title></head>\
                    <body><h1>Hello</h1></body></html>";
        let css = "h1 { color: red; }";
        let result = wv.load_html(html, css, Some("file:///tmp/test.html".to_string()));

        assert_eq!(result.status, 200);
        assert_eq!(result.stylesheet_count, 1);
        assert!(wv.document().is_some());
        assert!(wv.interpreter().is_some());
        assert_eq!(wv.title(), "Test Page");
        assert_eq!(wv.base_url(), Some("file:///tmp/test.html"));
        assert!(!wv.stylesheets().is_empty());
        assert!(wv.dirty);
    }

    #[test]
    fn load_html_runs_inline_script() {
        let mut wv = fresh();
        let html = "<html><body>\
                    <script>console.log('hello from script');</script>\
                    </body></html>";
        wv.load_html(html, "", None);
        let interp = wv.interpreter().expect("interpreter must exist");
        let logs = interp.console_log.borrow();
        let found = logs.iter().any(|(_, msg)| msg.contains("hello from script"));
        assert!(found, "script output missing in console_log: {:?}", *logs);
    }

    #[test]
    fn load_html_picks_up_js_title_assignment() {
        let mut wv = fresh();
        let html = "<html><head><title>Initial</title></head>\
                    <body><script>document.title = 'Updated';</script></body></html>";
        wv.load_html(html, "", None);
        assert_eq!(wv.title(), "Updated");
    }

    #[test]
    fn load_dom_skips_scripts() {
        let mut wv = fresh();
        // Stejny HTML jako load_html_runs_inline_script.
        let html = "<html><body>\
                    <script>console.log('side-effect MUSI NEbezet');</script>\
                    </body></html>";
        wv.load_dom(html, "", None);
        let interp = wv.interpreter().expect("interpreter present");
        let logs = interp.console_log.borrow();
        let found = logs.iter().any(|(_, msg)| msg.contains("side-effect"));
        assert!(!found, "load_dom musi NEspustit scripts; found in console: {:?}", *logs);
    }

    #[test]
    fn load_dom_preserves_raw_html_and_css() {
        let mut wv = fresh();
        let html = "<html><body>HI</body></html>";
        let css = "body { color: red; }";
        wv.load_dom(html, css, None);
        assert_eq!(wv.html(), html);
        assert_eq!(wv.css(), css);
    }

    #[test]
    fn set_zoom_clamps_range() {
        let mut wv = fresh();
        wv.set_zoom(10.0);
        assert_eq!(wv.zoom(), 5.0);
        wv.set_zoom(0.01);
        assert_eq!(wv.zoom(), 0.25);
        wv.set_zoom(1.5);
        assert_eq!(wv.zoom(), 1.5);
    }

    #[test]
    fn set_scroll_marks_dirty() {
        let mut wv = fresh();
        wv.dirty = false;
        wv.set_scroll(0.0, 0.0);
        assert!(!wv.dirty, "no-op scroll should not dirty");
        wv.set_scroll(0.0, 100.0);
        assert!(wv.dirty, "scroll change must dirty");
        assert_eq!(wv.scroll(), (0.0, 100.0));
    }

    #[test]
    fn resize_updates_viewport_and_dirty() {
        let mut wv = fresh();
        wv.dirty = false;
        wv.resize(800, 600, 1.5);
        assert_eq!(wv.viewport_w, 800.0);
        assert_eq!(wv.viewport_h, 600.0);
        assert_eq!(wv.scale_factor, 1.5);
        assert!(wv.dirty);
    }

    #[test]
    fn page_size_nonempty_after_load() {
        let mut wv = fresh();
        let html = "<html><body><div style=\"width:200px;height:300px\">x</div></body></html>";
        wv.load_html(html, "", None);
        let (w, h) = wv.page_size();
        assert!(w >= wv.viewport_w, "content w {w} < viewport_w {}", wv.viewport_w);
        assert!(h >= wv.viewport_h, "content h {h} < viewport_h {}", wv.viewport_h);
    }

    #[test]
    fn engine_headless_has_no_gpu() {
        let eng = Engine::new_headless();
        assert!(!eng.has_gpu());
        assert!(eng.device().is_none());
        assert!(eng.queue().is_none());
    }

    #[test]
    fn handle_input_scroll_updates_target() {
        let mut wv = fresh();
        wv.dirty = false;
        let resp = wv.handle_input(InputEvent::Scroll {
            dx: 0.0, dy: 50.0, x: 100.0, y: 100.0,
            modifiers: KeyModifiers::default(),
        });
        assert!(resp.dirty, "scroll musi dirty webview");
        // Smooth scroll: target je novy, actual scroll lerp pri render_via.
        assert_eq!(wv.scroll_target_y, 50.0);
        assert_eq!(wv.scroll(), (0.0, 0.0));
    }

    #[test]
    fn handle_input_scroll_clamps_negative() {
        let mut wv = fresh();
        wv.handle_input(InputEvent::Scroll {
            dx: -100.0, dy: -100.0, x: 0.0, y: 0.0,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(wv.scroll_target_x, 0.0);
        assert_eq!(wv.scroll_target_y, 0.0);
    }

    #[test]
    fn handle_input_resize_dirty() {
        let mut wv = fresh();
        wv.dirty = false;
        let resp = wv.handle_input(InputEvent::Resize {
            width: 800, height: 600, scale_factor: 1.0,
        });
        assert!(resp.dirty);
        assert_eq!(wv.viewport_w, 800.0);
        assert_eq!(wv.viewport_h, 600.0);
    }

    #[test]
    fn render_returns_none_on_headless_engine() {
        // Headless = no GPU - render musi gracefully vratit None misto panik.
        let mut wv = fresh();
        wv.load_html("<html><body>x</body></html>", "", None);
        assert!(wv.render().is_none(), "headless render musi vratit None");
        assert!(wv.target_view().is_none());
        assert!(wv.target_texture().is_none());
    }

    #[test]
    fn editor_for_node_inits_from_value_attr() {
        // editor_for_node musi vratit EditorState s text = value attr.
        // Bez render volani (headless) musim si rucne vytvorit node.
        let mut wv = fresh();
        wv.load_html("<html><body><input value=\"hello\" /></body></html>", "", None);
        let doc = wv.document().expect("doc");
        let root = std::rc::Rc::clone(&doc.root);
        // Najdi <input> node v DOM tree.
        fn find_input(n: &std::rc::Rc<crate::browser::dom::Node>)
            -> Option<std::rc::Rc<crate::browser::dom::Node>>
        {
            if n.tag_name().as_deref() == Some("input") {
                return Some(std::rc::Rc::clone(n));
            }
            for ch in n.children.borrow().iter() {
                if let Some(r) = find_input(ch) { return Some(r); }
            }
            None
        }
        let input = find_input(&root).expect("<input> v DOM");
        let ed = wv.editor_for_node(&input);
        assert_eq!(ed.text, "hello");
        assert_eq!(ed.caret, 5);
    }

    #[test]
    fn editor_for_node_resyncs_on_value_change() {
        let mut wv = fresh();
        wv.load_html("<html><body><input value=\"abc\" /></body></html>", "", None);
        let doc = wv.document().expect("doc");
        let root = std::rc::Rc::clone(&doc.root);
        fn find_input(n: &std::rc::Rc<crate::browser::dom::Node>)
            -> Option<std::rc::Rc<crate::browser::dom::Node>>
        {
            if n.tag_name().as_deref() == Some("input") {
                return Some(std::rc::Rc::clone(n));
            }
            for ch in n.children.borrow().iter() {
                if let Some(r) = find_input(ch) { return Some(r); }
            }
            None
        }
        let input = find_input(&root).expect("<input> v DOM");
        {
            let ed = wv.editor_for_node(&input);
            assert_eq!(ed.text, "abc");
        }
        // External JS-like mutation `el.value = "xyz"`.
        input.set_attr("value", "xyz");
        let ed = wv.editor_for_node(&input);
        assert_eq!(ed.text, "xyz", "editor musi resync s value attr");
    }
}
