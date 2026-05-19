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
    /// Scrollbar drag state - Some(grab_offset_y) pri V thumb drag.
    /// Pri main page scrollbar: None v node_ptr. Pri inner scrollable element:
    /// Some(node_ptr) - thumb drag updates element_scroll[ptr].
    pub(crate) v_scrollbar_drag: Option<f32>,
    pub(crate) v_scrollbar_drag_node: Option<usize>,
    pub(crate) h_scrollbar_drag: Option<f32>,
    pub(crate) h_scrollbar_drag_node: Option<usize>,
    /// Per-element scroll offset (x, y) pres `overflow: auto/scroll` boxes.
    /// Wheel + thumb drag updates. Paint translates content children.
    pub(crate) element_scroll: std::collections::HashMap<usize, (f32, f32)>,
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
    pub(crate) layout_cache_key: Option<(u64, u32, u32)>,
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
    /// Posledni LayerTree z extract_layer_tree. Hostujici code muze sample.
    /// Diagnostika + invalidation tracking.
    pub(crate) last_layer_tree: Option<crate::browser::compositor::LayerNode>,
    /// Paint cache: hash style_map full content. Pri shode (cascade vraci
    /// novy Rc ale identicky content - hover bez :hover effect) skip paint
    /// + gpu submit, reuse cached target_view. Klicova win pres hover bez
    /// vizualni odezvy = 0ms frame.
    pub(crate) last_paint_fingerprint: Option<u64>,
}

/// Pomocnik pro debug log node count v layout slow path.
fn count_nodes(node: &std::rc::Rc<crate::browser::dom::NodeData>) -> usize {
    let mut c = 1;
    for child in node.children.borrow().iter() {
        c += count_nodes(child);
    }
    c
}

/// Find LayoutBox pres node_ptr v subtree (drag mouseup target).
fn find_box_by_ptr(
    root: &crate::browser::layout::LayoutBox,
    target_ptr: usize,
) -> Option<&crate::browser::layout::LayoutBox> {
    if let Some(n) = &root.node {
        if std::rc::Rc::as_ptr(n) as usize == target_ptr {
            return Some(root);
        }
    }
    for ch in &root.children {
        if let Some(found) = find_box_by_ptr(ch, target_ptr) {
            return Some(found);
        }
    }
    None
}

/// Inner scrollbar thumb hit-test pres MouseDown. Walk layout pres scrollable
/// boxes, check zda (x, y) je na thumbu V/H scrollbaru. Vraci (node_ptr, axis,
/// grab_offset) pri zasahu, None jinak.
fn find_inner_scrollbar_at(
    root: &crate::browser::layout::LayoutBox,
    x: f32, y: f32,
    element_scroll: &std::collections::HashMap<usize, (f32, f32)>,
) -> Option<(usize, char, f32)> {
    use crate::browser::layout::Overflow;
    fn walk(
        bx: &crate::browser::layout::LayoutBox,
        x: f32, y: f32,
        element_scroll: &std::collections::HashMap<usize, (f32, f32)>,
        best: &mut Option<(usize, char, f32)>,
    ) {
        let needs_y = matches!(bx.overflow_y, Overflow::Auto | Overflow::Scroll)
            && bx.inner_content_h > bx.rect.height + 0.5;
        let needs_x = matches!(bx.overflow_x, Overflow::Auto | Overflow::Scroll)
            && bx.inner_content_w > bx.rect.width + 0.5;
        if (needs_y || needs_x) && bx.node.is_some() {
            let ptr = std::rc::Rc::as_ptr(bx.node.as_ref().unwrap()) as usize;
            let (cur_sx, cur_sy) = element_scroll.get(&ptr).copied().unwrap_or((0.0, 0.0));
            if needs_y {
                let bar_w = bx.scrollbar_size.max(8.0).min(14.0);
                let bar_x = bx.rect.x + bx.rect.width - bar_w;
                let bar_y = bx.rect.y;
                let bar_h = bx.rect.height;
                if x >= bar_x && x < bar_x + bar_w && y >= bar_y && y < bar_y + bar_h {
                    let thumb_h = (bar_h * bar_h / bx.inner_content_h).max(30.0);
                    let max_scroll = (bx.inner_content_h - bar_h).max(1.0);
                    let scroll_ratio = (cur_sy / max_scroll).clamp(0.0, 1.0);
                    let thumb_y = bar_y + (bar_h - thumb_h) * scroll_ratio;
                    if y >= thumb_y && y < thumb_y + thumb_h {
                        *best = Some((ptr, 'y', y - thumb_y));
                    }
                }
            }
            if needs_x {
                let bar_h = bx.scrollbar_size.max(8.0).min(14.0);
                let bar_x = bx.rect.x;
                let bar_y = bx.rect.y + bx.rect.height - bar_h;
                let bar_w = bx.rect.width - if needs_y { 12.0 } else { 0.0 };
                if x >= bar_x && x < bar_x + bar_w && y >= bar_y && y < bar_y + bar_h {
                    let thumb_w = (bar_w * bar_w / bx.inner_content_w).max(30.0);
                    let max_scroll_x = (bx.inner_content_w - bar_w).max(1.0);
                    let scroll_ratio = (cur_sx / max_scroll_x).clamp(0.0, 1.0);
                    let thumb_x = bar_x + (bar_w - thumb_w) * scroll_ratio;
                    if x >= thumb_x && x < thumb_x + thumb_w {
                        *best = Some((ptr, 'x', x - thumb_x));
                    }
                }
            }
        }
        for ch in &bx.children {
            walk(ch, x, y, element_scroll, best);
        }
    }
    let mut best = None;
    walk(root, x, y, element_scroll, &mut best);
    best
}

/// Apply per-element scroll - pres scrollable box mutate child rects o (-sx, -sy).
/// MVP bez clip: descendants render shifted, mohou prelevat pres parent rect.
fn apply_element_scroll(
    bx: &mut crate::browser::layout::LayoutBox,
    element_scroll: &std::collections::HashMap<usize, (f32, f32)>,
) {
    if let Some(node) = &bx.node {
        let ptr = std::rc::Rc::as_ptr(node) as usize;
        if let Some(&(sx, sy)) = element_scroll.get(&ptr) {
            if sx.abs() > 0.01 || sy.abs() > 0.01 {
                for ch in bx.children.iter_mut() {
                    shift_subtree_rect(ch, -sx, -sy);
                }
            }
        }
    }
    for ch in bx.children.iter_mut() {
        apply_element_scroll(ch, element_scroll);
    }
}

fn shift_subtree_rect(bx: &mut crate::browser::layout::LayoutBox, dx: f32, dy: f32) {
    bx.rect.x += dx;
    bx.rect.y += dy;
    for ch in bx.children.iter_mut() {
        shift_subtree_rect(ch, dx, dy);
    }
}

/// Find nearest scrollable ancestor pod kurzorem (x, y v content coords).
/// Vraci (node_ptr, max_scroll_y, max_scroll_x) pri nalezeni scrollable boxu
/// s content > rect. None pri zadnem (fallback na page-level scroll).
fn find_scrollable_ancestor(
    root: &crate::browser::layout::LayoutBox,
    x: f32, y: f32,
) -> Option<(usize, f32, f32)> {
    use crate::browser::layout::Overflow;
    // Walk top-down, deepest scrollable wins.
    fn walk(
        bx: &crate::browser::layout::LayoutBox,
        x: f32, y: f32,
        best: &mut Option<(usize, f32, f32)>,
    ) {
        if x < bx.rect.x || x >= bx.rect.x + bx.rect.width
            || y < bx.rect.y || y >= bx.rect.y + bx.rect.height
        {
            return;
        }
        let scroll_y = matches!(bx.overflow_y, Overflow::Auto | Overflow::Scroll)
            && bx.inner_content_h > bx.rect.height + 0.5;
        let scroll_x = matches!(bx.overflow_x, Overflow::Auto | Overflow::Scroll)
            && bx.inner_content_w > bx.rect.width + 0.5;
        if (scroll_y || scroll_x) && bx.node.is_some() {
            let ptr = std::rc::Rc::as_ptr(bx.node.as_ref().unwrap()) as usize;
            let max_y = (bx.inner_content_h - bx.rect.height).max(0.0);
            let max_x = (bx.inner_content_w - bx.rect.width).max(0.0);
            *best = Some((ptr, max_y, max_x));
        }
        for ch in &bx.children {
            walk(ch, x, y, best);
        }
    }
    let mut best = None;
    walk(root, x, y, &mut best);
    best
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
            v_scrollbar_drag: None,
            v_scrollbar_drag_node: None,
            h_scrollbar_drag: None,
            h_scrollbar_drag_node: None,
            element_scroll: std::collections::HashMap::new(),
            last_layout_root: None,
            layout_rects: std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
            cascade_props: std::rc::Rc::new(std::cell::RefCell::new(None)),
            stylesheets_data: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            async_jobs: crate::browser::async_jobs::AsyncJobsRegistry::new(),
            nav_id: 0,
            collected_sources: Vec::new(),
            last_render_dom_version: 0,
            cascade_cache_key: None,
            cascade_cache_value: None,
            layout_fp_cache: None,
            paint_fp_cache: None,
            hit_test_cache: None,
            hovered_node_local: None,
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
            last_paint_fingerprint: None,
        }
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

    /// Posledni render_via per-phase timing (ms): (cascade, layout, paint, gpu).
    /// Pro diagnostiku - shell title bar nebo overlay.
    pub fn render_phase_times(&self) -> (f32, f32, f32, f32) {
        (self.prof_cascade_ms, self.prof_layout_ms, self.prof_paint_ms, self.prof_gpu_ms)
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
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
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
    pub(crate) fn ensure_layer_texture(
        &mut self,
        layer_id: usize,
        logical_w: f32,
        logical_h: f32,
    ) -> Option<()> {
        let device = self.engine.device.as_ref()?.clone();
        let phys_w = ((logical_w * self.scale_factor) as u32).max(1);
        let phys_h = ((logical_h * self.scale_factor) as u32).max(1);

        // Reuse pri shode size.
        if let Some(slot) = self.layer_textures.get(&layer_id) {
            if slot.width == phys_w && slot.height == phys_h {
                return Some(());
            }
        }

        // Alloc nova (replace pripadnou starou).
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rwe-layer-offscreen"),
            size: wgpu::Extent3d { width: phys_w, height: phys_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
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
        Some(())
    }

    /// Garbage collect layer_textures - drop entries jejichz layer_id neni v
    /// current_layers set. Volat po extract_layer_tree v render_via.
    /// Bez GC: layer_textures roste pri DOM mutaci (smazane elementy ale
    /// jejich texture cache zustava).
    pub(crate) fn gc_layer_textures(&mut self, alive_layer_ids: &std::collections::HashSet<usize>) {
        self.layer_textures.retain(|k, _| alive_layer_ids.contains(k));
    }

    /// Zpracuj input event. Vrati `EventResponse` se zmenami pro hostujici
    /// aplikaci (dirty flag, cursor change, navigation request, ...).
    ///
    /// Phase 5 minimal implementacne: scroll + mouse move + resize. Click/key
    /// dispatch do JS event listeneru = Phase 99 (vyzaduje hit-test pres
    /// layout tree + DOM addEventListener registry).
    pub fn handle_input(&mut self, event: InputEvent) -> EventResponse {
        let mut response = EventResponse::default();
        match event {
            InputEvent::Scroll { dx, dy, .. } => {
                // Wheel hit-test: prefer nearest scrollable ancestor (inner
                // overflow:auto/scroll) - update jeho element_scroll. Pri zadnem
                // inner scrollable fallback na page-level scroll_target_y.
                let viewport_h = self.viewport_h / self.zoom.max(0.01);
                let viewport_w = self.viewport_w / self.zoom.max(0.01);
                let mx = self.mouse_x + self.scroll_x;
                let my = self.mouse_y + self.scroll_y;
                let inner_target = self.last_layout_root.as_ref().and_then(|root|
                    find_scrollable_ancestor(root, mx, my));
                if let Some((node_ptr, max_inner_y, max_inner_x)) = inner_target {
                    let entry = self.element_scroll.entry(node_ptr).or_insert((0.0, 0.0));
                    entry.0 = (entry.0 + dx).clamp(0.0, max_inner_x);
                    entry.1 = (entry.1 + dy).clamp(0.0, max_inner_y);
                    self.dirty = true;
                    response.dirty = true;
                } else {
                    let (max_y, max_x) = match &self.last_layout_root {
                        Some(l) => (
                            (l.rect.height - viewport_h).max(0.0),
                            (l.rect.width - viewport_w).max(0.0),
                        ),
                        None => (f32::INFINITY, f32::INFINITY),
                    };
                    self.scroll_target_x = (self.scroll_target_x + dx).clamp(0.0, max_x);
                    self.scroll_target_y = (self.scroll_target_y + dy).clamp(0.0, max_y);
                    self.dirty = true;
                    response.dirty = true;
                }
            }
            InputEvent::MouseMove { x, y, .. } => {
                if (self.mouse_x - x).abs() > 0.5 || (self.mouse_y - y).abs() > 0.5 {
                    self.mouse_x = x;
                    self.mouse_y = y;
                    // Scrollbar thumb drag - update scroll position pres
                    // mouse pos vs thumb grab offset.
                    let viewport_w = self.viewport_w / self.zoom.max(0.01);
                    let viewport_h = self.viewport_h / self.zoom.max(0.01);
                    // Inner scrollbar drag - update element_scroll[node].
                    if let (Some(grab_y), Some(node_ptr)) = (self.v_scrollbar_drag, self.v_scrollbar_drag_node) {
                        if let Some(layout) = &self.last_layout_root {
                            if let Some(bx) = find_box_by_ptr(layout, node_ptr) {
                                let bar_h = bx.rect.height;
                                let thumb_h = (bar_h * bar_h / bx.inner_content_h).max(30.0);
                                let track_h = (bar_h - thumb_h).max(1.0);
                                let new_thumb_y = (y - bx.rect.y - grab_y).max(0.0).min(track_h);
                                let max_scroll = (bx.inner_content_h - bar_h).max(1.0);
                                let new_scroll = (new_thumb_y / track_h) * max_scroll;
                                self.element_scroll.entry(node_ptr).or_insert((0.0, 0.0)).1 = new_scroll;
                                self.dirty = true;
                                response.dirty = true;
                                return response;
                            }
                        }
                    }
                    if let (Some(grab_x), Some(node_ptr)) = (self.h_scrollbar_drag, self.h_scrollbar_drag_node) {
                        if let Some(layout) = &self.last_layout_root {
                            if let Some(bx) = find_box_by_ptr(layout, node_ptr) {
                                let bar_w = bx.rect.width;
                                let thumb_w = (bar_w * bar_w / bx.inner_content_w).max(30.0);
                                let track_w = (bar_w - thumb_w).max(1.0);
                                let new_thumb_x = (x - bx.rect.x - grab_x).max(0.0).min(track_w);
                                let max_scroll_x = (bx.inner_content_w - bar_w).max(1.0);
                                let new_scroll = (new_thumb_x / track_w) * max_scroll_x;
                                self.element_scroll.entry(node_ptr).or_insert((0.0, 0.0)).0 = new_scroll;
                                self.dirty = true;
                                response.dirty = true;
                                return response;
                            }
                        }
                    }
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
                    // hovered_id (bez tree walk).
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    let dom_v = self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0);
                    let hit_key = ((content_x / 2.0) as i32, (content_y / 2.0) as i32, dom_v);
                    let hovered_id = match &self.hit_test_cache {
                        Some((k, v)) if *k == hit_key => *v,
                        _ => {
                            let h = self.last_layout_root.as_ref()
                                .and_then(|root| root.hit_test(content_x, content_y))
                                .and_then(|bx| bx.node.as_ref().map(|n|
                                    std::rc::Rc::as_ptr(n) as usize));
                            self.hit_test_cache = Some((hit_key, h));
                            h
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
                    // Cursor icon dle hovered tag.
                    let hovered_tag = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.as_ref().map(|n| n.tag_name()))
                        .flatten();
                    response.cursor = Some(match hovered_tag.as_deref() {
                        Some("a") | Some("button") => crate::embed::CursorIcon::Pointer,
                        Some("input") | Some("textarea") => crate::embed::CursorIcon::Text,
                        _ => {
                            // Pres text node -> taky text cursor.
                            let over_text = self.last_layout_root.as_ref()
                                .and_then(|root| root.hit_test(content_x, content_y))
                                .map(|bx| bx.text.is_some()).unwrap_or(false);
                            if over_text {
                                crate::embed::CursorIcon::Text
                            } else {
                                crate::embed::CursorIcon::Default
                            }
                        }
                    });
                }
            }
            InputEvent::MouseDown { x, y, button, .. } => {
                if matches!(button, crate::embed::MouseButton::Left) {
                    // Scrollbar thumb hit-test PRED page hit-test.
                    let viewport_w = self.viewport_w / self.zoom.max(0.01);
                    let viewport_h = self.viewport_h / self.zoom.max(0.01);
                    // Pred page-level scrollbar hit-test: check inner scrollbar thumbs.
                    // Walk layout, find scrollable box jehoz thumb pres (x, y).
                    if let Some(layout) = &self.last_layout_root {
                        let inner_hit = find_inner_scrollbar_at(layout, x, y, &self.element_scroll);
                        if let Some((node_ptr, axis, grab_offset)) = inner_hit {
                            if axis == 'y' {
                                self.v_scrollbar_drag = Some(grab_offset);
                                self.v_scrollbar_drag_node = Some(node_ptr);
                            } else {
                                self.h_scrollbar_drag = Some(grab_offset);
                                self.h_scrollbar_drag_node = Some(node_ptr);
                            }
                            response.dirty = true;
                            self.dirty = true;
                            return response;
                        }
                    }
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
                    // Hit-test layout_root pres content coords. Store target +
                    // pos pro MouseUp click-vs-drag distinguish.
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    let target_node = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.clone());
                    // Focus / blur - per-WebView focused state.
                    if let Some(target) = target_node.as_ref() {
                        let focusable = matches!(target.tag_name().as_deref(),
                            Some("input") | Some("textarea") | Some("button")
                            | Some("a") | Some("select"));
                        let new_id = if focusable {
                            Some(std::rc::Rc::as_ptr(target) as usize)
                        } else { None };
                        self.focused_node_local = new_id;
                        // Cascade global = mirror per-WebView pro :focus styling
                        // (cascade.rs PSEUDO :focus check). Single thread,
                        // posledni MouseDown wins. Multi-WebView problem: posledni
                        // klik prepise styling pro vsechny - akceptace pri F12.
                        crate::browser::cascade::set_focused_node(new_id);
                    } else {
                        self.focused_node_local = None;
                        crate::browser::cascade::set_focused_node(None);
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
                    if let (Some(target), Some(interp)) = (target_node.clone(), self.interpreter.as_mut()) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("mousedown".into()));
                        event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                        event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
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
                    let click_on_input = target_node.as_ref()
                        .map(|n| matches!(n.tag_name().as_deref(),
                            Some("input") | Some("textarea")))
                        .unwrap_or(false);
                    if let Some(target) = target_node {
                        self.mouse_down_at = Some((x, y, target));
                    }
                    if !click_on_input {
                        self.sel_begin(content_x, content_y);
                    }
                    response.dirty = true;
                    self.dirty = true;
                }
            }
            InputEvent::MouseUp { x, y, button, .. } => {
                if matches!(button, crate::embed::MouseButton::Left) {
                    // End scrollbar drag.
                    if self.v_scrollbar_drag.is_some() || self.h_scrollbar_drag.is_some() {
                        self.v_scrollbar_drag = None;
                        self.h_scrollbar_drag = None;
                        self.v_scrollbar_drag_node = None;
                        self.h_scrollbar_drag_node = None;
                        response.dirty = true;
                        return response;
                    }
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    // End selection drag (collapse pri <3px movement).
                    self.sel_end();
                    let up_target = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.clone());
                    // mouseup event dispatch.
                    if let (Some(target), Some(interp)) = (up_target.as_ref(), self.interpreter.as_mut()) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("mouseup".into()));
                        event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                        event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
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
                            let event_obj_rc = std::rc::Rc::new(std::cell::RefCell::new({
                                let mut event = crate::interpreter::JsObject::new();
                                event.set("type".into(), crate::interpreter::JsValue::Str("click".into()));
                                event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                                event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
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
        let needs_tick = !self.active_animations.is_empty()
            || !self.active_transitions.is_empty()
            || (self.scroll_target_y - self.scroll_y).abs() > 0.5
            || (self.scroll_target_x - self.scroll_x).abs() > 0.5
            || self.focused_is_input();
        if !self.dirty && !needs_tick {
            // Reset profilers - jinak title bar drzi historickou hodnotu z
            // prvni render (uvadi v omyl user diagnostiku).
            self.prof_cascade_ms = 0.0;
            self.prof_layout_ms = 0.0;
            self.prof_paint_ms = 0.0;
            self.prof_gpu_ms = 0.0;
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
                self.dirty = true;
            }
        }
        // Smooth scroll tick: lerp scroll_y -> scroll_target_y 25 %% per frame.
        // Snap pri delta < 0.5 px aby render_via prestane request_redraw pri
        // ustaleni.
        let lerp = 0.25_f32;
        let dy = self.scroll_target_y - self.scroll_y;
        if dy.abs() > 0.5 { self.scroll_y += dy * lerp; }
        else if dy.abs() > 0.0 { self.scroll_y = self.scroll_target_y; }
        let dx = self.scroll_target_x - self.scroll_x;
        if dx.abs() > 0.5 { self.scroll_x += dx * lerp; }
        else if dx.abs() > 0.0 { self.scroll_x = self.scroll_target_x; }
        // Sync interp.scroll_pos do current scroll (pri wheel/scrollbar drag
        // animovany scroll, JS read pres pageXOffset/scrollX dostane realnou
        // hodnotu, ne jen JS-set hodnotu). Take updatuj last_synced_scroll_pos
        // - diff detection v dalsim frame ne triggerne false JS modified.
        if let Some(interp) = self.interpreter.as_ref() {
            *interp.scroll_pos.borrow_mut() = (self.scroll_x, self.scroll_y);
            self.last_synced_scroll_pos = (self.scroll_x, self.scroll_y);
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
        let cache_key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            // dom_version
            self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0).hash(&mut hasher);
            // Per-WV hovered/focused. Bez per-WV by jine WV mouse_move
            // invalidoval cache i kdyz tahla WV nezmenila hover.
            // Conditional: jen pokud stylesheet pouziva :hover/:focus.
            if uses_hover {
                self.hovered_node_local.unwrap_or(0).hash(&mut hasher);
            }
            if uses_focus {
                self.focused_node_local.unwrap_or(0).hash(&mut hasher);
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
            let dom_ver = self.interpreter.as_ref().map(|i| i.dom_version()).unwrap_or(0);
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
        if !self.active_transitions.is_empty() {
            crate::browser::cascade::apply_transitions(
                std::rc::Rc::make_mut(&mut style_map), &self.active_transitions, elapsed);
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
        self.prev_style_map = Some(style_map.clone());
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
        if Some(paint_fp) == self.last_paint_fingerprint
            && !needs_tick
            && self.target_view.is_some()
        {
            // Cache hit - vse identicke, reuse predchozi frame.
            self.prof_layout_ms = 0.0;
            self.prof_paint_ms = 0.0;
            self.prof_gpu_ms = 0.0;
            renderer.target_size = None;
            self.dirty = false;
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
        let layout_key = (
            layout_fp,
            (viewport_w as u32),
            (viewport_h as u32),
        );
        let mut layout_root = if Some(layout_key) == self.layout_cache_key
            && self.last_layout_root.is_some()
        {
            // Cache hit - reuse clone z predchoziho framu.
            self.last_layout_root.as_ref().unwrap().clone()
        } else {
            self.layout_cache_key = Some(layout_key);
            // Layout subtree cache: pri MISS na top-level (fingerprint zmena nekde),
            // predame prev_root pres raw ptr index - subtree match HIT pres
            // fingerprint reuse prev subtree (clone jen pri HIT). Drasticky snizuje
            // rebuild kdyz hover zmeni jen 1 element a celej zbytek je stejny.
            crate::browser::layout::reset_build_box_stats();
            let empty_pseudo = crate::browser::cascade::PseudoStyleMap::new();
            let t = std::time::Instant::now();
            let r = crate::browser::layout::layout_tree_with_pseudo_cached(
                &doc.root, &style_map, &empty_pseudo, viewport_w, viewport_h,
                self.last_layout_root.as_ref());
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

        // 2b. Sticky positioning post-process - position:sticky elementy
        // posunuju dle scroll_y aby drzeli na top viewportu uvnitr containeru.
        crate::browser::layout::apply_sticky(&mut layout_root, self.scroll_y);

        // Per-element scroll offset apply - pres scrollable boxes shift descendants
        // o (-sx, -sy). Hit-test + paint pak vidi posunute coords. Bez clip = MVP,
        // content moze prelevat pres rect bounds; opraveno pres clip_path emit
        // pres scrollable box.
        if !self.element_scroll.is_empty() {
            apply_element_scroll(&mut layout_root, &self.element_scroll);
        }

        // 2c. Paint-side animations apply (transform overlay, opacity tween).
        crate::browser::render::apply_paint_animations(&mut layout_root, &style_map);

        // 2d. L1+L2 compositor: extract LayerTree z layout.
        // L2: per-layer offscreen texture allocator. Pro kazdou layer alokuj
        // wgpu::Texture velikosti layer.root_rect (logical). Reuse pri size
        // match mezi frames. GC unreferenced layers.
        let layer_tree = crate::browser::compositor::extract_layer_tree(&layout_root);
        {
            let mut alive = std::collections::HashSet::new();
            crate::browser::compositor::collect_layer_ids(&layer_tree, &mut alive);
            self.gc_layer_textures(&alive);
            let mut flat: Vec<&crate::browser::compositor::LayerNode> = Vec::new();
            crate::browser::compositor::flatten_layers(&layer_tree, &mut flat);
            for layer in flat {
                let lw = layer.root_rect.width.max(1.0);
                let lh = layer.root_rect.height.max(1.0);
                let _ = self.ensure_layer_texture(layer.id, lw, lh);
            }
        }
        self.last_layer_tree = Some(layer_tree);

        let prof_t2 = std::time::Instant::now();
        self.prof_layout_ms = prof_t2.duration_since(prof_t1).as_secs_f32() * 1000.0;

        // 3. Paint - generate display list (culled na viewport).
        let mut display_list = crate::browser::paint::build_display_list_culled(
            &layout_root, self.scroll_y, viewport_h);

        // 3-canvas. Canvas2D ops -> DisplayCommands (po body paint).
        if let Some(interp) = self.interpreter.as_ref() {
            let canvas_ops = interp.canvas_ops.borrow();
            crate::browser::render::canvas_paint::paint_canvas_ops(
                &layout_root, &canvas_ops, &mut display_list);
        }

        // 3-caret. Blinking caret na focused <input>/<textarea>.
        if let Some(focused) = self.focused_dom_node() {
            let is_input = matches!(focused.tag_name().as_deref(),
                Some("input") | Some("textarea"));
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
                    let mut hits: Vec<(f32, f32, f32, f32)> = Vec::new();
                    collect_text_lines(&layout_root, start.0, start.1, end.0, end.1, &mut hits);
                    for (hx, hy, hw, hh) in hits {
                        display_list.push(crate::browser::paint::DisplayCommand::Rect {
                            x: hx, y: hy, w: hw, h: hh,
                            color: [80, 150, 255, 120], radius: 0.0,
                        });
                    }
                }
            }
        }

        // 3z. Overlay painter callback - hostujici aplikace emit DODATECNE
        // DisplayCommands (inspector highlight, devtools, ...). Volane PRED
        // scroll shift -> overlay coords v content-space.
        if let Some(painter) = self.overlay_painter.as_mut() {
            painter(&layout_root, self.scroll_y, &mut display_list);
        }

        // 3a. Apply scroll: posun page commands o -scroll_y. Scrollbar
        //     overlay (pridany nize) je viewport-relative -> add PO shift.
        for cmd in display_list.iter_mut() {
            crate::browser::render::segments::shift_command_y(cmd, -self.scroll_y);
            crate::browser::render::segments::shift_command_x(cmd, -self.scroll_x);
        }

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
            &self.element_scroll,
        );

        // 4. Warm-up glyph atlas + image atlas pred draw.
        renderer.warm_atlas_for(&display_list, self.base_url.as_deref());

        // 4b. Extract text runs (per-glyph cumulative advances) - foundation
        // pro per-glyph hit-test selection. Walks display_list TEXT cmds +
        // measure pres atlas. Page cmds only (overlay text neselectable).
        self.painted_text_runs = crate::browser::render::extract_text_runs(
            &display_list, renderer.atlas(), renderer.zoom);

        let prof_t3 = std::time::Instant::now();
        self.prof_paint_ms = prof_t3.duration_since(prof_t2).as_secs_f32() * 1000.0;

        // 5. Renderer kresli display list do target_view.
        let target_view = self.target_view.as_ref()?;
        let _had = renderer.draw_segments_into_view_clipped(
            target_view, &display_list, true, None);

        // 5b. WebGL canvas frame - per <canvas> s WebGL state encode wgpu
        // draw passes do per-canvas RT + compose do target_view. NO-OP pri
        // zadnem WebGL canvasu na strance.
        if let Some(interp) = self.interpreter.as_ref() {
            let webgl_states = interp.webgl_states.clone();
            let states = webgl_states.borrow();
            if !states.is_empty() {
                let _ = renderer.run_webgl_frame(&layout_root, target_view, &*states, self.scroll_y);
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
        self.last_layout_root = Some(layout_root);

        // Reset renderer target_size override - shell present_split + jine
        // pas v swap chain pouziva config size.
        renderer.target_size = None;

        let prof_t4 = std::time::Instant::now();
        self.prof_gpu_ms = prof_t4.duration_since(prof_t3).as_secs_f32() * 1000.0;

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
        // PERF FIX: drive bylo `stylesheets.any(|s| !s.keyframes.is_empty())`
        // ktere bylo TRUE i pokud stranka jen DEFINUJE @keyframes (bez pouziti
        // pres `animation:` property). To zapinalo nekonecny request_redraw
        // smycku v shellu (3 WebView render kazdy frame) = 1 FPS pri 3WV setup.
        // Now: cti `active_animations` - skutecne hrajici (set v render_via po
        // detekci animation: prop na elementech).
        !self.active_animations.is_empty()
            || !self.active_transitions.is_empty()
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
        }
    }

    /// Page title (z `<title>` ci `document.title = ...`).
    pub fn title(&self) -> &str { &self.title }

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
    out: &mut Vec<(f32, f32, f32, f32)>,
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
                    out.push((line_start_x + hs, line_y, he - hs, lh));
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
