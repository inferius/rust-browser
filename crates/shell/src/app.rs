//! ShellApp - winit `ApplicationHandler` ktery vlastni Window + Surface +
//! Renderer + WebView. Renderuje stranku pres `WebView::render_via` do
//! offscreen RT a kompozituje do swap chain pres
//! `Renderer::present_external_to_swap_chain`.
//!
//! Phase 4c step 3 (minimal): bez chrome bar, bez tabs, bez addr/find.
//! Cilem ten cestu validovat - shell crate je nezavislym hostem enginu.
//! Phase 5+ pridava chrome paint a multi-tab.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use rwe_engine::browser::render::Renderer;
use rwe_engine::embed::{DevtoolsTarget, Engine, InputEvent, KeyModifiers, MouseButton, WebView};
use rwe_engine::interpreter::{helpers::native, JsValue};
use rwe_devtools_proto::DevtoolsRequest;

/// Guess resource_type pro Network.requestWillBeSent event z URL extension.
/// Real impl by mela header Content-Type, ale aktualne network_log nema.
/// ConsoleArg (Phase A3 strukturovany format) -> CDP RemoteObject JSON.
/// Format CDP Runtime.RemoteObject:
///   { type, subtype?, value?, description?, preview? }
/// Mapping per ConsoleArgKind. Pro Object/Array/Map/Set vlozi `preview`
/// s prvni-uroven children jako properties array.
fn console_arg_to_cdp_remote_object(arg: &rwe_engine::interpreter::console_args::ConsoleArg) -> serde_json::Value {
    use rwe_engine::interpreter::console_args::ConsoleArgKind as K;
    let (cdp_type, cdp_subtype): (&str, Option<&str>) = match arg.kind {
        K::String                 => ("string", None),
        K::Number                 => ("number", None),
        K::Bool                   => ("boolean", None),
        K::Undefined              => ("undefined", None),
        K::Null                   => ("object", Some("null")),
        K::BigInt                 => ("bigint", None),
        K::Function               => ("function", None),
        K::Object                 => ("object", None),
        K::Array                  => ("object", Some("array")),
        K::Error                  => ("object", Some("error")),
        K::Date                   => ("object", Some("date")),
        K::RegExp                 => ("object", Some("regexp")),
        K::Dom                    => ("object", Some("node")),
        K::Map                    => ("object", Some("map")),
        K::Set                    => ("object", Some("set")),
        K::Promise                => ("object", Some("promise")),
    };
    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::Value::String(cdp_type.into()));
    if let Some(sub) = cdp_subtype {
        obj.insert("subtype".into(), serde_json::Value::String(sub.into()));
    }
    obj.insert("description".into(), serde_json::Value::String(arg.repr.clone()));
    // Pro String/Number/Bool inline value (frontend rendering bez expand).
    match arg.kind {
        K::String => { obj.insert("value".into(), serde_json::Value::String(arg.repr.clone())); }
        K::Number => {
            if let Ok(n) = arg.repr.parse::<f64>() {
                if let Some(v) = serde_json::Number::from_f64(n) {
                    obj.insert("value".into(), serde_json::Value::Number(v));
                }
            }
        }
        K::Bool => {
            obj.insert("value".into(), serde_json::Value::Bool(arg.repr == "true"));
        }
        _ => {}
    }
    // Preview s children pro Object/Array/Map/Set.
    if !arg.children.is_empty() {
        let props: Vec<serde_json::Value> = arg.children.iter().map(|(k, v)| {
            serde_json::json!({
                "name": k,
                "type": "string",
                "value": v,
            })
        }).collect();
        obj.insert("preview".into(), serde_json::json!({
            "type": cdp_type,
            "description": arg.repr,
            "overflow": arg.children.len() >= 16,
            "properties": props,
        }));
    }
    serde_json::Value::Object(obj)
}

fn guess_resource_type(url: &str) -> &'static str {
    let lc = url.to_ascii_lowercase();
    if lc.ends_with(".js") || lc.ends_with(".mjs") { "Script" }
    else if lc.ends_with(".css") { "Stylesheet" }
    else if lc.ends_with(".png") || lc.ends_with(".jpg") || lc.ends_with(".jpeg")
         || lc.ends_with(".gif") || lc.ends_with(".webp") || lc.ends_with(".svg") { "Image" }
    else if lc.ends_with(".woff") || lc.ends_with(".woff2") || lc.ends_with(".ttf")
         || lc.ends_with(".otf") { "Font" }
    else if lc.ends_with(".html") || lc.ends_with(".htm") { "Document" }
    else if lc.ends_with(".json") { "XHR" }
    else { "Other" }
}

/// Find smallest ELEMENT LayoutBox containing (x, y). DFS prefer descendant.
/// Used pres inspect_mode hover hit-test + CDP picker.
///
/// Chrome behavior: kliknuti na text uvnitr `<h1>Title</h1>` selectne `<h1>`,
/// ne text node. Frontend tree renderuje text inline pres parent element row,
/// takze text node nema vlastni selectable row. Skip text/comment nodes -
/// return prvni ELEMENT ancestor (= bx.tag.is_some()).
fn pick_node_at(
    root: &rwe_engine::browser::layout::LayoutBox,
    x: f32, y: f32,
) -> Option<usize> {
    let r = &root.rect;
    let in_self = x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
    if !in_self { return None; }
    // Try children first - prefer deepest ELEMENT.
    for child in &root.children {
        if let Some(p) = pick_node_at(child, x, y) {
            return Some(p);
        }
    }
    // Return self JEN pokud element (= tag Some). Text/comment nodes
    // (tag None) jsou inline-renderny pres parent v devtools tree.
    if root.tag.is_some() {
        root.node.as_ref().map(|n| std::rc::Rc::as_ptr(n) as usize)
    } else {
        None
    }
}

/// Find LayoutBox rect dle node ptr. Walk DFS, return rect (x,y,w,h)
/// nebo None pokud node neexistuje v layout tree.
fn find_layout_rect(
    root: &rwe_engine::browser::layout::LayoutBox,
    target_ptr: usize,
) -> Option<(f32, f32, f32, f32)> {
    if let Some(n) = &root.node {
        if std::rc::Rc::as_ptr(n) as usize == target_ptr {
            return Some((root.rect.x, root.rect.y, root.rect.width, root.rect.height));
        }
    }
    for child in &root.children {
        if let Some(r) = find_layout_rect(child, target_ptr) {
            return Some(r);
        }
    }
    None
}

/// Selected node outline - tenky purple ramecek okolo content rectu (Chrome
/// convention pro persistent selected node post-picker-click). Bez 4-layer
/// fill (= conflicting overlay s hover paint).
pub(crate) fn emit_selected_outline(
    layout_root: &rwe_engine::browser::layout::LayoutBox,
    target_ptr: usize,
    _scroll_y: f32,  // POZOR: overlay_painter emit v CONTENT-SPACE,
                     // engine sam apply scroll shift na vsechny commands.
                     // Manualni odecet by zpusobil 2x scroll = outline jezdi rychleji.
    cmds: &mut Vec<rwe_engine::browser::paint::DisplayCommand>,
) {
    fn find<'a>(
        root: &'a rwe_engine::browser::layout::LayoutBox,
        target: usize,
    ) -> Option<&'a rwe_engine::browser::layout::LayoutBox> {
        if let Some(n) = &root.node {
            if std::rc::Rc::as_ptr(n) as usize == target { return Some(root); }
        }
        for ch in &root.children {
            if let Some(f) = find(ch, target) { return Some(f); }
        }
        None
    }
    let Some(bx) = find(layout_root, target_ptr) else { return };
    use rwe_engine::browser::paint::DisplayCommand;
    let r = &bx.rect;
    let color = [180, 113, 255, 255]; // purple Chrome-style
    // Pri transform != none: emit transformed polygon outline (4 rotated corners
    // + edges as ClippedRect strips). Bez transform - axis-aligned rect strips.
    if !bx.transforms.is_empty() {
        let m = rwe_engine::browser::layout::compute_transform_matrix(
            &bx.transforms, None);
        let (cx, cy) = (r.x + r.width * 0.5, r.y + r.height * 0.5);
        let corners_local = [
            (r.x - cx, r.y - cy),
            (r.x + r.width - cx, r.y - cy),
            (r.x + r.width - cx, r.y + r.height - cy),
            (r.x - cx, r.y + r.height - cy),
        ];
        // Matrix ROW-MAJOR per transform_op_matrix. m[r*4+c] = row r, col c.
        let transform_point = |lx: f32, ly: f32| -> (f32, f32) {
            let lz = 0.0; let lw = 1.0;
            let tx = m[0]*lx + m[1]*ly + m[2]*lz + m[3]*lw;
            let ty = m[4]*lx + m[5]*ly + m[6]*lz + m[7]*lw;
            let tw = m[12]*lx + m[13]*ly + m[14]*lz + m[15]*lw;
            let inv_w = if tw.abs() > 1e-6 { 1.0 / tw } else { 1.0 };
            (tx * inv_w + cx, ty * inv_w + cy)
        };
        let corners: Vec<(f32, f32)> = corners_local.iter()
            .map(|(lx, ly)| transform_point(*lx, *ly))
            .collect();
        let bw = 2.0_f32;
        // 4 edges - kazdou jako tenký polygon (= ClippedRect 4 corners).
        for i in 0..4 {
            let p0 = corners[i];
            let p1 = corners[(i + 1) % 4];
            let dx = p1.0 - p0.0;
            let dy = p1.1 - p0.1;
            let len = (dx*dx + dy*dy).sqrt();
            if len < 0.001 { continue; }
            let nx = -dy / len * bw * 0.5;
            let ny = dx / len * bw * 0.5;
            let pts = vec![
                (p0.0 - nx, p0.1 - ny),
                (p1.0 - nx, p1.1 - ny),
                (p1.0 + nx, p1.1 + ny),
                (p0.0 + nx, p0.1 + ny),
            ];
            cmds.push(DisplayCommand::ClippedRect { color, points: pts });
        }
    } else {
        let (x, y, w, h, bw) = (r.x, r.y, r.width, r.height, 2.0);
        cmds.push(DisplayCommand::Rect { x, y, w, h: bw, color, radius: 0.0 });
        cmds.push(DisplayCommand::Rect { x, y: y + h - bw, w, h: bw, color, radius: 0.0 });
        cmds.push(DisplayCommand::Rect { x, y, w: bw, h, color, radius: 0.0 });
        cmds.push(DisplayCommand::Rect { x: x + w - bw, y, w: bw, h, color, radius: 0.0 });
    }
}

/// Box-model highlight emit - 4-layer overlay rects (margin/border/padding/
/// content) jako Chrome inspector. Volaane pres page WV overlay_painter
/// pri inspect_state.hovered_node Some.
pub(crate) fn emit_box_model_highlight(
    layout_root: &rwe_engine::browser::layout::LayoutBox,
    target_ptr: usize,
    _scroll_y: f32,  // POZOR: overlay_painter emit v CONTENT-SPACE, engine sam
                     // apply scroll shift na vsechny commands. Manualni odecet
                     // = 2x scroll = overlay jezdi rychleji nez page.
    opts: &rwe_engine::embed::inspect_state::HighlightOptions,
    cmds: &mut Vec<rwe_engine::browser::paint::DisplayCommand>,
) {
    fn find<'a>(
        root: &'a rwe_engine::browser::layout::LayoutBox,
        target: usize,
    ) -> Option<&'a rwe_engine::browser::layout::LayoutBox> {
        if let Some(n) = &root.node {
            if std::rc::Rc::as_ptr(n) as usize == target { return Some(root); }
        }
        for ch in &root.children {
            if let Some(f) = find(ch, target) { return Some(f); }
        }
        None
    }
    let Some(bx) = find(layout_root, target_ptr) else { return };
    use rwe_engine::browser::paint::DisplayCommand;
    let r = &bx.rect;
    let p_t = bx.padding_top.unwrap_or(bx.padding);
    let p_r = bx.padding_right.unwrap_or(bx.padding);
    let p_b = bx.padding_bottom.unwrap_or(bx.padding);
    let p_l = bx.padding_left.unwrap_or(bx.padding);
    let m_t = bx.margin_top.unwrap_or(bx.margin);
    let m_r = bx.margin_right.unwrap_or(bx.margin);
    let m_b = bx.margin_bottom.unwrap_or(bx.margin);
    let m_l = bx.margin_left.unwrap_or(bx.margin);
    let bw = bx.border_width.max(0.0);
    // Box model layers - margin/border/padding/content rects.
    let layers: [([f32; 4], [u8; 4]); 4] = [
        // margin (outer)
        ([r.x - p_l - bw - m_l, r.y - p_t - bw - m_t,
          r.width + p_l + p_r + 2.0*bw + m_l + m_r,
          r.height + p_t + p_b + 2.0*bw + m_t + m_b], opts.margin_color),
        // border
        ([r.x - p_l - bw, r.y - p_t - bw,
          r.width + p_l + p_r + 2.0*bw, r.height + p_t + p_b + 2.0*bw], opts.border_color),
        // padding
        ([r.x - p_l, r.y - p_t,
          r.width + p_l + p_r, r.height + p_t + p_b], opts.padding_color),
        // content
        ([r.x, r.y, r.width, r.height], opts.content_color),
    ];
    if !bx.transforms.is_empty() {
        // Transform applied - emit per-layer rotated polygon. Centroid = content
        // rect center (CSS transform-origin: 50% 50% default).
        let m = rwe_engine::browser::layout::compute_transform_matrix(
            &bx.transforms, None);
        let cx = r.x + r.width * 0.5;
        let cy = r.y + r.height * 0.5;
        // Matrix ROW-MAJOR per transform_op_matrix.
        let transform_point = |px: f32, py: f32| -> (f32, f32) {
            let lx = px - cx; let ly = py - cy;
            let lz = 0.0; let lw = 1.0;
            let tx = m[0]*lx + m[1]*ly + m[2]*lz + m[3]*lw;
            let ty = m[4]*lx + m[5]*ly + m[6]*lz + m[7]*lw;
            let tw = m[12]*lx + m[13]*ly + m[14]*lz + m[15]*lw;
            let inv_w = if tw.abs() > 1e-6 { 1.0 / tw } else { 1.0 };
            (tx * inv_w + cx, ty * inv_w + cy)
        };
        for ([lx, ly, lw, lh], color) in layers.iter() {
            let corners = [
                (*lx,        *ly       ),
                (*lx + *lw,  *ly       ),
                (*lx + *lw,  *ly + *lh ),
                (*lx,        *ly + *lh ),
            ];
            let pts: Vec<(f32, f32)> = corners.iter()
                .map(|(px, py)| transform_point(*px, *py)).collect();
            cmds.push(DisplayCommand::ClippedRect { color: *color, points: pts });
        }
    } else {
        for ([lx, ly, lw, lh], color) in layers.iter() {
            cmds.push(DisplayCommand::Rect { x: *lx, y: *ly, w: *lw, h: *lh,
                color: *color, radius: 0.0 });
        }
    }
}

/// Shell commands z chrome bar JS bridge. Chrome WebView volat
/// `__shell_navigate__(url)` etc. -> push do command queue. Shell main
/// loop drain + execute (load_url, nav_back, ...).
#[derive(Debug, Clone)]
pub enum ShellCommand {
    Back,
    Forward,
    Reload,
    ToggleDevtools,
    Navigate(String),
}

/// Sdilena command queue mezi chrome native fns a shell main loop.
pub type ShellCmdQueue = Rc<RefCell<VecDeque<ShellCommand>>>;

/// CDP channel - queue messages mezi devtools WebView JS bridge a shell
/// main loop dispatch. Native `__rwe_cdp_send_native` (v devtools interp)
/// pushne request do req_queue. Main loop kazdy frame drain req_queue,
/// dispatch via DevtoolsTarget pres page WebView, push response do
/// resp_queue (JSON-serialized). Native `__rwe_cdp_poll_events` drains
/// resp_queue + vraci jako JSON array stringy.
#[derive(Default, Clone)]
pub struct CdpChannel {
    /// Pending requests od devtools UI - drain ve main loop.
    pub req_queue: Rc<RefCell<VecDeque<DevtoolsRequest>>>,
    /// Pending responses + events pro devtools UI - drain pres pollEvents.
    /// Format: kazdy item = JSON string (DevtoolsResponse nebo DevtoolsEvent).
    pub resp_queue: Rc<RefCell<VecDeque<String>>>,
}

impl CdpChannel {
    fn new() -> Self {
        Self {
            req_queue: Rc::new(RefCell::new(VecDeque::new())),
            resp_queue: Rc::new(RefCell::new(VecDeque::new())),
        }
    }
}

pub struct ShellApp {
    html: String,
    css: String,
    base_url: Option<String>,
    local_path: Option<PathBuf>,

    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    engine: Option<Arc<Engine>>,
    webview: Option<WebView>,
    /// Chrome bar WebView (back/fwd/reload + URL input). Visible vzdy nad
    /// page. Fixed height = chrome_h logical px.
    chrome: Option<WebView>,
    /// Chrome bar vyska (logical px).
    chrome_h: f32,
    /// Command queue z chrome bar (back/fwd/reload/navigate). Native fns
    /// instalovane na chrome interpreter push pri user click. Shell main
    /// loop drain pred kazdym render.
    chrome_cmds: ShellCmdQueue,
    /// DevTools WebView (D4). Some pri F12 toggle on - load INDEX_HTML s
    /// injectnutymi panel HTMLs + theme.css + cdp.js. Komunikace s page
    /// webview pres `window.cdp.send(...)` JS API (D6 nativní binding).
    devtools: Option<WebView>,
    /// True kdyz devtools je viditelne. D4b = horizontal split (page top,
    /// devtools bottom). Page WebView dostane top `1 - devtools_split_ratio`
    /// vysky, devtools dostane bottom `devtools_split_ratio`.
    devtools_visible: bool,
    /// Pomer devtools cast vyrazneho viewport (0.0..1.0). Default 0.4 =
    /// devtools dostane spodnich 40%, page top 60%. Splitter drag (D4d)
    /// upravi pres mouse drag na hranici.
    devtools_split_ratio: f32,
    /// True kdyz user drze LMB na splitter line a tahne. Pri MouseMove se
    /// split_ratio updatuje + oba webview resizuji.
    splitter_drag: bool,
    /// D5: Inspector mode toggle pres Ctrl+Shift+C. Pri active hover na
    /// page emit blue overlay nad hovered element, click vyzve devtools
    /// elements panel select (DOM.inspectNodeRequested CDP event).
    inspect_mode: bool,
    /// D5: Sdilene state s page WebView overlay_painter closure. Pri inspect
    /// hover update node_id, closure cte + emit highlight rect.
    /// Legacy: shell-local Ctrl+Shift+C picker target. Pres devtools propojeni
    /// pouzij `inspect_state.hovered_node` (= shared cross-WV).
    inspect_target: std::rc::Rc<std::cell::RefCell<Option<usize>>>,
    /// Shared inspector state mezi shell + page WV + devtools WV target.
    /// Pres CDP Overlay.highlightNode / setInspectMode update -> page WV
    /// overlay_painter cte hovered_node + emit box-model rect.
    inspect_state: std::rc::Rc<std::cell::RefCell<rwe_engine::embed::inspect_state::InspectState>>,
    /// Address bar otevreny (Ctrl+L). Pri zapnuti capture klavesnice
    /// (znaky -> addr_input), Enter -> load_url. Esc -> close bez navigace.
    addr_open: bool,
    /// Aktualni text v address bar (pred Enter submit).
    addr_input: String,
    /// Find on page otevreny (Ctrl+F). Capture klavesnice -> find_query.
    /// Enter najde next match, Esc close.
    find_open: bool,
    /// Find query string.
    find_query: String,
    /// DevTools target adapter (D2). Lazy init pri F12 toggle. Drzi events
    /// buffer + breakpoint counter. Dispatch volame `target.handle_request(
    /// &mut self.webview, req)` ve main loop.
    devtools_target: Option<DevtoolsTarget>,
    /// CDP channel (D6b). Sdileny mezi devtools native fns (send/poll) a
    /// shell main loop (drain + dispatch). Rc<RefCell<>> queues.
    cdp_channel: Option<CdpChannel>,
    /// Posledni viditelny idx v page.interpreter().network_log. Pump_cdp
    /// detekuje nove entries a emit Network.* events.
    cdp_network_log_idx: usize,
    /// Posledni viditelny idx v page.interpreter().console_log. Pump_cdp
    /// detekuje nove entries a emit Runtime.consoleAPICalled events.
    cdp_console_log_idx: usize,
    /// Last seen page.nav_id - pri zmene drain collected_sources do
    /// cdp_sources_cache + emit Debugger.scriptParsed events.
    cdp_last_nav_id: u64,
    /// Cache sebranych source files - klic = scriptId (sekvencni string).
    /// Format: (scriptId, url, body, lang_marker). Pouziva
    /// Debugger.getScriptSource handler pro retrieve body.
    cdp_sources_cache: Vec<(String, String, String, String)>,
    /// Sekvencni script ID generator.
    cdp_next_script_id: u64,
    /// Last seen page.dom_version() - pri zmene emit DOM.documentUpdated
    /// (frontend pak znovu vyzve DOM.getDocument).
    cdp_last_dom_version: u64,
    /// Posledni emit DOM.documentUpdated cas (Instant.elapsed seconds). Pouziva
    /// se pro throttle - pri velkem DOMu s castymi mutacemi (animations,
    /// timers) by sync rerender frontend zabilo FPS. Min 0.5s mezi emity.
    cdp_last_dom_emit: std::time::Instant,
    /// Posledni mouse_move dispatch cas - throttle na 60fps cap. Bez tohoto
    /// pri pohybu mysi nad devtools (28x :hover v CSS) by mass cascade walks
    /// vytizely CPU 100%.
    last_mouse_move: std::time::Instant,
    /// Coalescing: kazdy raw CursorMoved push do `pending_coalesced` (predchozi
    /// position). Posledni position v `pending_mouse_pos`. Pri about_to_wait
    /// dispatch jeden InputEvent::MouseMove s coalesced history. JS pak cte
    /// pres PointerEvent.getCoalescedEvents(). Inspired by Chromium RenderWidget
    /// `CoalesceMouseMovesIfPossible`.
    pending_mouse_pos: Option<(f32, f32)>,
    pending_coalesced: Vec<(f32, f32)>,
    /// Coalesced window resize - winit posila Resized 1/pixel pri tazeni okna.
    /// Drz posledni velikost, aplikuj realny resize (texture realloc + reflow)
    /// jen 1x v RedrawRequested misto per-event = plynule tazeni.
    pending_resize: Option<(u32, u32)>,
    /// Posledni redraw cas - FPS counter (EMA 30 frames).
    frame_times_ms: std::collections::VecDeque<f32>,
    last_frame_time: std::time::Instant,
    /// Per-WebView posledni render_via doba (ms) - diagnostika v title bar.
    last_chrome_ms: f32,
    last_page_ms: f32,
    last_dev_ms: f32,
    /// Frame counter pro debug log per-frame.
    frame_counter: u64,

    mouse_x: f32,
    mouse_y: f32,
    modifiers: winit::keyboard::ModifiersState,
    history: Vec<String>,
    history_idx: usize,
    /// AUTOTEST exit deadline (RWE_AUTOTEST env). None = manual run.
    autotest_deadline: Option<std::time::Instant>,
    /// AUTOTEST start time pro relativni stagery.
    autotest_start: std::time::Instant,
    autotest_f12: bool,
    autotest_hover: bool,
    /// Pres flag - autotest F12 uz proveden.
    autotest_f12_done: bool,
    /// Hover ticker (per-frame increment, modulo na unique X positions).
    autotest_hover_tick: u32,
    #[allow(dead_code)]
    autotest_click_done: bool,
}

impl ShellApp {
    pub fn new(
        html: String,
        css: String,
        base_url: Option<String>,
        local_path: Option<PathBuf>,
    ) -> Self {
        Self {
            html, css, base_url, local_path,
            window: None,
            renderer: None,
            engine: None,
            webview: None,
            chrome: None,
            chrome_h: 36.0,
            chrome_cmds: Rc::new(RefCell::new(VecDeque::new())),
            devtools: None,
            devtools_visible: false,
            devtools_split_ratio: 0.4,
            splitter_drag: false,
            inspect_mode: false,
            inspect_target: Rc::new(RefCell::new(None)),
            inspect_state: rwe_engine::embed::inspect_state::InspectState::shared(),
            addr_open: false,
            addr_input: String::new(),
            find_open: false,
            find_query: String::new(),
            devtools_target: None,
            cdp_channel: None,
            cdp_network_log_idx: 0,
            cdp_console_log_idx: 0,
            cdp_last_nav_id: 0,
            cdp_sources_cache: Vec::new(),
            cdp_next_script_id: 1,
            cdp_last_dom_version: 0,
            cdp_last_dom_emit: std::time::Instant::now(),
            last_mouse_move: std::time::Instant::now(),
            pending_mouse_pos: None,
            pending_resize: None,
            pending_coalesced: Vec::new(),
            frame_times_ms: std::collections::VecDeque::with_capacity(30),
            last_frame_time: std::time::Instant::now(),
            last_chrome_ms: 0.0,
            last_page_ms: 0.0,
            last_dev_ms: 0.0,
            frame_counter: 0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            modifiers: winit::keyboard::ModifiersState::empty(),
            history: Vec::new(),
            history_idx: 0,
            autotest_deadline: None,
            autotest_start: std::time::Instant::now(),
            autotest_f12: false,
            autotest_hover: false,
            autotest_f12_done: false,
            autotest_hover_tick: 0,
            autotest_click_done: false,
        }
    }

    /// Extrahuje vsechny inline `<style>...</style>` bloky z HTML stringu
    /// + spoji do jednoho CSS textu. Naive regex-free parser: hleda
    /// <style otevreny + </style> closing, vse mezi tim concatenate.
    fn extract_inline_styles(html: &str) -> String {
        let lc = html.to_ascii_lowercase();
        let mut out = String::new();
        let mut cursor = 0;
        while cursor < lc.len() {
            let open = match lc[cursor..].find("<style") {
                Some(p) => cursor + p,
                None => break,
            };
            let after_tag = match lc[open..].find('>') {
                Some(p) => open + p + 1,
                None => break,
            };
            let close = match lc[after_tag..].find("</style>") {
                Some(p) => after_tag + p,
                None => break,
            };
            out.push_str(&html[after_tag..close]);
            out.push('\n');
            cursor = close + "</style>".len();
        }
        out
    }

    /// D6b: Nainstaluje native CDP funkce na devtools interpreter, capturuje
    /// channel Rc clones do closures.
    ///
    /// `__rwe_cdp_send_native(json_str)`: parse to DevtoolsRequest, push do
    /// channel.req_queue, vrati "". Response delivered async pres pollEvents.
    ///
    /// `__rwe_cdp_poll_events()`: drain channel.resp_queue, vrati JSON array.
    /// Format: pole stringu (kazdy DevtoolsResponse nebo DevtoolsEvent jako
    /// samostatny JSON obj). cdp.js handleResponseJson(s) parse + dispatch.
    fn install_cdp_natives(devtools: &mut WebView, channel: &CdpChannel) {
        let interp = match devtools.interpreter_mut() {
            Some(i) => i,
            None => {
                eprintln!("[cdp] devtools interpreter chybi, natives neinstaluju");
                return;
            }
        };
        // __rwe_cdp_send_native(json_str) -> "" (async dispatch).
        let req_q = Rc::clone(&channel.req_queue);
        let send_fn = native("__rwe_cdp_send_native", move |args| {
            let json = args.first().map(|v| v.to_string()).unwrap_or_default();
            match serde_json::from_str::<DevtoolsRequest>(&json) {
                Ok(req) => {
                    req_q.borrow_mut().push_back(req);
                }
                Err(e) => eprintln!("[CDP SEND] parse err: {} (json: {})", e, json),
            }
            Ok(JsValue::Str(String::new()))
        });
        // __rwe_cdp_poll_events() -> JSON array of pending response/event strings.
        let resp_q = Rc::clone(&channel.resp_queue);
        let poll_fn = native("__rwe_cdp_poll_events", move |_args| {
            let mut q = resp_q.borrow_mut();
            if q.is_empty() {
                return Ok(JsValue::Str("[]".into()));
            }
            // Items v queue jsou uz JSON-serialized objekty. Slozit array:
            // "[<obj>,<obj>,...]"
            let mut out = String::from("[");
            let mut first = true;
            while let Some(item) = q.pop_front() {
                if !first { out.push(','); }
                out.push_str(&item);
                first = false;
            }
            out.push(']');
            Ok(JsValue::Str(out))
        });
        interp.global.borrow_mut().define("__rwe_cdp_send_native", send_fn);
        interp.global.borrow_mut().define("__rwe_cdp_poll_events", poll_fn);
        println!("[cdp] D6b natives installed (send/poll wired to channel)");
    }

    /// Drain CDP requests z channelu, dispatch pres devtools_target + page,
    /// push responses + events do resp_queue jako JSON strings. Volana
    /// per-frame z redraw.
    fn pump_cdp(&mut self) {
        let (target, channel, page) = match (
            self.devtools_target.as_ref(),
            self.cdp_channel.as_ref(),
            self.webview.as_mut(),
        ) {
            (Some(t), Some(c), Some(p)) => (t, c, p),
            _ => return,
        };
        // Take pending requests (drain).
        let pending: Vec<DevtoolsRequest> = {
            let mut q = channel.req_queue.borrow_mut();
            q.drain(..).collect()
        };
        if pending.is_empty() && target.take_events().is_empty() {
            // Nothing to do. take_events musi probehnout pres ref - drain z
            // ABOVE smaze. Redundance: re-check po dispatch nize.
        }
        // Dispatch requests sekvencne. Kazda response -> resp_queue JSON.
        for req in pending {
            let req_id = req.id;
            let req_method = req.method.clone();
            let t_dispatch_start = std::time::Instant::now();
            // Shell-side intercept: Debugger.getScriptSource - sources nejsou
            // v WebView, drzi je shell.cdp_sources_cache.
            let resp = if req.method == "Debugger.getScriptSource" {
                let script_id = req.params.get("script_id")
                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                let body = self.cdp_sources_cache.iter()
                    .find(|(id, _, _, _)| *id == script_id)
                    .map(|(_, _, body, _)| body.clone());
                match body {
                    Some(b) => rwe_devtools_proto::DevtoolsResponse {
                        id: req.id,
                        result: Some(serde_json::json!({ "script_source": b })),
                        error: None,
                    },
                    None => rwe_devtools_proto::DevtoolsResponse {
                        id: req.id,
                        result: None,
                        error: Some(rwe_devtools_proto::DevtoolsError {
                            code: rwe_devtools_proto::error_codes::NODE_NOT_FOUND,
                            message: format!("Script {script_id} not found"),
                        }),
                    },
                }
            } else {
                target.handle_request(page, req)
            };
            let t_handle_done = std::time::Instant::now();
            let json = serde_json::to_string(&resp)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialize: {e}\"}}"));
            let t_serialize_done = std::time::Instant::now();
            let handle_ms = t_handle_done.duration_since(t_dispatch_start).as_secs_f32() * 1000.0;
            let serialize_ms = t_serialize_done.duration_since(t_handle_done).as_secs_f32() * 1000.0;
            // Log jen pri pomale dispatch (> 10ms).
            if handle_ms + serialize_ms > 10.0 {
                eprintln!("[CDP DISPATCH SLOW] id={} method={} handle:{:.1}ms serialize:{:.1}ms resp_len={}",
                    req_id, req_method, handle_ms, serialize_ms, json.len());
            }
            let _ = (req_id, req_method);
            channel.resp_queue.borrow_mut().push_back(json);
        }
        // Drain pending events (z target.events) - push do resp_queue.
        let events = target.take_events();
        for evt in events {
            let json = serde_json::to_string(&evt)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialize: {e}\"}}"));
            channel.resp_queue.borrow_mut().push_back(json);
        }
        // Nav detekce - pri zmene page.nav_id() drain collected_sources +
        // emit Debugger.scriptParsed + DOM.documentUpdated.
        let cur_nav = page.nav_id();
        if cur_nav != self.cdp_last_nav_id {
            let sources = page.take_collected_sources();
            // Clear sources cache pri nove stranky.
            self.cdp_sources_cache.clear();
            for (url, body, lang) in sources {
                if lang != "js" { continue; } // CDP scriptParsed je jen pro JS
                let script_id = self.cdp_next_script_id.to_string();
                self.cdp_next_script_id += 1;
                let line_count = body.lines().count() as u32;
                let end_col = body.lines().last().map(|l| l.len() as u32).unwrap_or(0);
                let evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "Debugger.scriptParsed".to_string(),
                    params: serde_json::json!({
                        "scriptId": script_id,
                        "url": url,
                        "startLine": 0,
                        "startColumn": 0,
                        "endLine": line_count.saturating_sub(1),
                        "endColumn": end_col,
                        "executionContextId": 1,
                        "hash": "",
                        "isLiveEdit": false,
                        "sourceMapURL": "",
                        "hasSourceURL": false,
                        "isModule": false,
                        "length": body.len(),
                    }),
                };
                let json = serde_json::to_string(&evt).unwrap_or_default();
                channel.resp_queue.borrow_mut().push_back(json);
                self.cdp_sources_cache.push((script_id, url.clone(), body, lang.to_string()));
            }
            // DOM.documentUpdated - frontend reload tree. Pri DOM rebuild musime
            // clear NodeIdTable v target - puvodni ptr->id mapping je invalid
            // (Weak<Node> upgrade by selhal pres frame reuse heap).
            target.clear_node_ids();
            let dom_evt = rwe_devtools_proto::DevtoolsEvent {
                method: "DOM.documentUpdated".to_string(),
                params: serde_json::json!({}),
            };
            channel.resp_queue.borrow_mut().push_back(
                serde_json::to_string(&dom_evt).unwrap_or_default());
            self.cdp_last_nav_id = cur_nav;
            // Sync dom_version - po nav je dom_version > 0, frontend uz
            // dostal documentUpdated event vyse, neopakovat.
            self.cdp_last_dom_version = page.dom_version();
        }
        // DOM mutation detekce - pri rozdilu emit DOM.documentUpdated.
        // Throttle 500ms - pri velkem DOMu s castymi mutacemi (animations,
        // setInterval timers) by sync rerender zaplavil frontend a srazil FPS.
        // dom_STYLE_version (strukturalni), NE dom_version - jinak SVG geometry
        // animace (points kazdy frame) trigger DOM.documentUpdated kazdych 500ms
        // -> devtools tree re-fetch + 1s render = <1 FPS. Tree se meni jen pri
        // strukturalni/class/style zmene.
        let cur_dom = page.dom_style_version();
        if cur_dom != self.cdp_last_dom_version {
            if self.cdp_last_dom_emit.elapsed().as_millis() >= 500 {
                let evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "DOM.documentUpdated".to_string(),
                    params: serde_json::json!({}),
                };
                channel.resp_queue.borrow_mut().push_back(
                    serde_json::to_string(&evt).unwrap_or_default());
                self.cdp_last_dom_version = cur_dom;
                self.cdp_last_dom_emit = std::time::Instant::now();
            }
            // Pokud throttle aktivni, last_dom_version se neaktualizuje -
            // zajistime ze priste emit projde (zachova dirty flag).
        }
        // Diff page network_log od last index -> emit Network events.
        // Format network_log entry: (url, status). Status 0 = pending.
        let interp = page.interpreter();
        if let Some(interp) = interp {
            let net_log = interp.network_log.borrow();
            let console_log = interp.console_log.borrow();
            // Detekce page reload - len(network/console) zmensila se vs
            // last idx => page byla rebuild. Emit DOM.documentUpdated +
            // reset indexy.
            if self.cdp_network_log_idx > net_log.len()
                || self.cdp_console_log_idx > console_log.len() {
                self.cdp_network_log_idx = 0;
                self.cdp_console_log_idx = 0;
                let evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "DOM.documentUpdated".to_string(),
                    params: serde_json::json!({}),
                };
                let json = serde_json::to_string(&evt).unwrap_or_default();
                channel.resp_queue.borrow_mut().push_back(json);
            }
            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64()).unwrap_or(0.0);
            for entry in net_log.iter().skip(self.cdp_network_log_idx) {
                let (url, status) = entry;
                let req_id = url.clone();
                let resource_type = guess_resource_type(url);
                let req_evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "Network.requestWillBeSent".to_string(),
                    params: serde_json::json!({
                        "request_id": req_id,
                        "url": url,
                        "method": "GET",
                        "timestamp": now_ts,
                        "resource_type": resource_type,
                    }),
                };
                let resp_evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "Network.responseReceived".to_string(),
                    params: serde_json::json!({
                        "request_id": req_id,
                        "status": *status as u32,
                        "status_text": if *status >= 200 && *status < 300 { "OK" } else { "" },
                        "mime_type": "text/plain",
                        "timestamp": now_ts,
                    }),
                };
                let fin_evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "Network.loadingFinished".to_string(),
                    params: serde_json::json!({
                        "request_id": req_id,
                        "encoded_data_length": 0u64,
                        "timestamp": now_ts,
                    }),
                };
                for e in [req_evt, resp_evt, fin_evt] {
                    let json = serde_json::to_string(&e).unwrap_or_default();
                    channel.resp_queue.borrow_mut().push_back(json);
                }
            }
            self.cdp_network_log_idx = net_log.len();
            // Diff console_log + console_log_args -> emit Runtime.consoleAPICalled events.
            // Paralelne s console_log_args (i-ty entry = i-ty zaznam) pro typove
            // preview. Bez args fallback na plain string (legacy / worker entries).
            let log_args = interp.console_log_args.borrow();
            for (i, (level, msg)) in console_log.iter().enumerate().skip(self.cdp_console_log_idx) {
                let args_json: serde_json::Value = if let Some(structured) = log_args.get(i) {
                    if structured.is_empty() {
                        serde_json::json!([{ "type": "string", "value": msg, "description": msg }])
                    } else {
                        let arr: Vec<serde_json::Value> = structured.iter()
                            .map(console_arg_to_cdp_remote_object).collect();
                        serde_json::Value::Array(arr)
                    }
                } else {
                    serde_json::json!([{ "type": "string", "value": msg, "description": msg }])
                };
                let evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "Runtime.consoleAPICalled".to_string(),
                    params: serde_json::json!({
                        "type": level,
                        "args": args_json,
                        "timestamp": now_ts,
                    }),
                };
                let json = serde_json::to_string(&evt).unwrap_or_default();
                channel.resp_queue.borrow_mut().push_back(json);
            }
            self.cdp_console_log_idx = console_log.len();
        }
    }

    /// Sync chrome address input dle history[idx] (po nav back/fwd/reload/load).
    fn sync_chrome_url(&mut self) {
        let cur = self.history.get(self.history_idx).cloned();
        if let Some(url) = cur {
            self.update_chrome_url(&url);
        }
    }

    /// Updatuje address input v chrome bar na novou URL. Volane po nav
    /// (back/forward/reload/navigate). Najde #addr element v chrome DOM
    /// + nastavi value attr + force redraw.
    fn update_chrome_url(&mut self, url: &str) {
        let chrome = match self.chrome.as_mut() { Some(c) => c, None => return };
        let interp = match chrome.interpreter() { Some(i) => i, None => return };
        let doc = interp.document.borrow();
        let root = std::rc::Rc::clone(&doc.root);
        drop(doc);
        // Najdi #addr element pres DFS.
        fn find_by_id(
            node: &std::rc::Rc<rwe_engine::browser::dom::Node>,
            id: &str,
        ) -> Option<std::rc::Rc<rwe_engine::browser::dom::Node>> {
            if let Some(attr_val) = node.attributes.borrow().get("id") {
                if attr_val == id { return Some(std::rc::Rc::clone(node)); }
            }
            for c in node.children.borrow().iter() {
                if let Some(f) = find_by_id(c, id) { return Some(f); }
            }
            None
        }
        if let Some(addr) = find_by_id(&root, "addr") {
            addr.attributes.borrow_mut().insert("value".to_string(), url.to_string());
        }
        // Force dirty pro pristi render.
        chrome.resize(chrome.viewport_size().0 as u32, chrome.viewport_size().1 as u32,
            chrome.scale_factor());
    }

    /// Install __shell_*__ native fns na chrome interpreter. Kazda push
    /// ShellCommand do chrome_cmds queue. Shell main loop drain + execute.
    fn install_chrome_natives(chrome: &mut WebView, cmds: &ShellCmdQueue) {
        let interp = match chrome.interpreter_mut() {
            Some(i) => i,
            None => return,
        };
        // __shell_back__()
        let q = Rc::clone(cmds);
        interp.global.borrow_mut().define("__shell_back__",
            native("__shell_back__", move |_| {
                q.borrow_mut().push_back(ShellCommand::Back);
                Ok(JsValue::Undefined)
            }));
        // __shell_fwd__()
        let q = Rc::clone(cmds);
        interp.global.borrow_mut().define("__shell_fwd__",
            native("__shell_fwd__", move |_| {
                q.borrow_mut().push_back(ShellCommand::Forward);
                Ok(JsValue::Undefined)
            }));
        // __shell_reload__()
        let q = Rc::clone(cmds);
        interp.global.borrow_mut().define("__shell_reload__",
            native("__shell_reload__", move |_| {
                q.borrow_mut().push_back(ShellCommand::Reload);
                Ok(JsValue::Undefined)
            }));
        // __shell_toggle_devtools__()
        let q = Rc::clone(cmds);
        interp.global.borrow_mut().define("__shell_toggle_devtools__",
            native("__shell_toggle_devtools__", move |_| {
                q.borrow_mut().push_back(ShellCommand::ToggleDevtools);
                Ok(JsValue::Undefined)
            }));
        // __shell_navigate__(url)
        let q = Rc::clone(cmds);
        interp.global.borrow_mut().define("__shell_navigate__",
            native("__shell_navigate__", move |args| {
                let url = args.first().map(|v| v.to_string()).unwrap_or_default();
                if !url.is_empty() {
                    q.borrow_mut().push_back(ShellCommand::Navigate(url));
                }
                Ok(JsValue::Undefined)
            }));
        println!("[shell chrome] natives installed (back/fwd/reload/devtools/navigate)");
    }

    /// Drain command queue + execute. Po kazde akci request_redraw.
    fn drain_chrome_cmds(&mut self) {
        let cmds: Vec<ShellCommand> = self.chrome_cmds.borrow_mut().drain(..).collect();
        if cmds.is_empty() { return; }
        for cmd in cmds {
            match cmd {
                ShellCommand::Back => self.nav_back(),
                ShellCommand::Forward => self.nav_forward(),
                ShellCommand::Reload => {
                    if let (Some(wv), Some(last)) = (
                        &mut self.webview,
                        self.history.get(self.history_idx).cloned()
                    ) {
                        wv.load_url(&last);
                    }
                }
                ShellCommand::ToggleDevtools => self.toggle_devtools(),
                ShellCommand::Navigate(url) => {
                    let loaded = self.webview.as_mut()
                        .map(|wv| wv.load_url(&url).is_some()).unwrap_or(false);
                    if loaded {
                        self.history.truncate(self.history_idx + 1);
                        self.history.push(url);
                        self.history_idx = self.history.len() - 1;
                    }
                }
            }
        }
        self.sync_chrome_url();
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    /// Chrome bar HTML - back/fwd/reload buttons + URL input. Bez native
    /// bindings (MVP) - vizualni only. Click handlery emit pres
    /// __shell_*__ globalni fns (instaluji se po load_html).
    fn build_chrome_html(initial_url: &str) -> String {
        format!(r#"<!DOCTYPE html>
<html><head><style>
* {{ box-sizing: border-box; }}
html, body {{ margin: 0; padding: 0; height: 100%; background: #202124; color: #e8eaed;
  font-family: 'Segoe UI', sans-serif; font-size: 12px; overflow: hidden; }}
.bar {{ display: flex; align-items: center; height: 100%; padding: 4px 8px; gap: 6px; }}
.btn {{ background: #3c4043; color: #fff; border: 1px solid #5f6368; padding: 2px 8px;
  border-radius: 4px; cursor: pointer; min-width: 24px; height: 22px; line-height: 18px;
  text-align: center; font-size: 12px; }}
.btn:hover {{ background: #5f6368; }}
.addr {{ flex: 1; background: #292a2d; color: #e8eaed; border: 1px solid #3c4043;
  border-radius: 11px; padding: 2px 12px; outline: none; height: 22px; line-height: 18px;
  font-size: 12px; }}
.addr:focus {{ border-color: #8ab4f8; }}
</style></head><body>
<div class="bar">
  <button class="btn" id="back">&lt;</button>
  <button class="btn" id="fwd">&gt;</button>
  <button class="btn" id="reload">R</button>
  <input class="addr" id="addr" value="{}" type="text" />
  <button class="btn" id="devtools">F12</button>
</div>
<script>
  function on(id, ev, fn) {{ var e = document.getElementById(id); if (e) e.addEventListener(ev, fn); }}
  on('back', 'click', function() {{ if (window.__shell_back__) __shell_back__(); }});
  on('fwd', 'click', function() {{ if (window.__shell_fwd__) __shell_fwd__(); }});
  on('reload', 'click', function() {{ if (window.__shell_reload__) __shell_reload__(); }});
  on('devtools', 'click', function() {{ if (window.__shell_toggle_devtools__) __shell_toggle_devtools__(); }});
  on('addr', 'keydown', function(e) {{
    if (e.key === 'Enter' && window.__shell_navigate__) {{
      __shell_navigate__(document.getElementById('addr').value);
    }}
  }});
</script></body></html>
"#, initial_url)
    }

    /// Slozi devtools HTML: INDEX_HTML (single-file Firefox-like Theme + i18n +
    /// Lucide ikony + Firefox 3-col Inspector) s injectnutym CDP JS clientem.
    /// Per-panel HTML injection (predchozi pattern) drop - novy index.html
    /// drzi vsechny panely v jedne strance.
    fn build_devtools_html() -> String {
        use rwe_devtools_frontend::*;
        let mut out = INDEX_HTML.to_string();
        out = out.replace(
            "<script id=\"cdp-js\"></script>",
            &format!("<script id=\"cdp-js\">{}</script>", CDP_JS),
        );
        out = out.replace(
            "<script id=\"lucide-js\"></script>",
            &format!("<script id=\"lucide-js\">{}</script>", LUCIDE_JS),
        );
        out
    }

    /// True kdyz mouse_y je v zone +- 3px okolo split line A x neni v page
    /// scrollbar zone (pravy edge 12px). Bez x check by splitter chytil
    /// klik na bottom scrollbar tracku.
    fn point_on_splitter(&self, x: f32, y: f32) -> bool {
        if !self.devtools_visible { return false; }
        let split_y = self.devtools_y_offset();
        if (y - split_y).abs() >= 3.0 { return false; }
        // Page scrollbar zone (x > page.viewport_w - 12) priority.
        if let Some(wv) = &self.webview {
            let (vw, _) = wv.viewport_size();
            if x >= vw - 12.0 { return false; }
        }
        true
    }

    /// True kdyz mouse_y je v chrome bar area (top, fixed h).
    fn point_in_chrome(&self, y: f32) -> bool {
        y < self.chrome_h
    }

    /// True kdyz devtools je viditelne A mouse_y je v devtools area (bottom).
    fn point_in_devtools(&self, y: f32) -> bool {
        if !self.devtools_visible { return false; }
        let r = match &self.renderer { Some(r) => r, None => return false };
        let sf = r.scale_factor_value().max(0.01);
        let (_sw, sh) = r.surface_size();
        let lh_full = (sh as f32 / sf).max(1.0);
        let content_h = (lh_full - self.chrome_h).max(1.0);
        let split = self.devtools_split_ratio.clamp(0.05, 0.95);
        let dev_start = self.chrome_h + content_h * (1.0 - split);
        y >= dev_start
    }

    /// Y offset pro mouse_y do devtools WebView local coords.
    fn devtools_y_offset(&self) -> f32 {
        let r = match &self.renderer { Some(r) => r, None => return 0.0 };
        let sf = r.scale_factor_value().max(0.01);
        let (_sw, sh) = r.surface_size();
        let lh_full = (sh as f32 / sf).max(1.0);
        let content_h = (lh_full - self.chrome_h).max(1.0);
        let split = self.devtools_split_ratio.clamp(0.05, 0.95);
        self.chrome_h + content_h * (1.0 - split)
    }

    /// Pristup k aktivnimu WebView (D4c: dle mouse_y position - chrome top,
    /// devtools bottom, jinak page).
    fn with_active_mut<R, F>(&mut self, f: F) -> Option<R>
    where F: FnOnce(&mut WebView) -> R {
        if self.point_in_chrome(self.mouse_y) {
            self.chrome.as_mut().map(f)
        } else if self.point_in_devtools(self.mouse_y) {
            self.devtools.as_mut().map(f)
        } else {
            self.webview.as_mut().map(f)
        }
    }

    fn with_active<R, F>(&self, f: F) -> Option<R>
    where F: FnOnce(&WebView) -> R {
        if self.point_in_chrome(self.mouse_y) {
            self.chrome.as_ref().map(f)
        } else if self.point_in_devtools(self.mouse_y) {
            self.devtools.as_ref().map(f)
        } else {
            self.webview.as_ref().map(f)
        }
    }

    /// Flush coalesced MouseMove buffer (volaane pred redraw). Dispatchne
    /// JEDEN MouseMove s posledni position + history pres PointerEvent
    /// .getCoalescedEvents() JS API. Vrati Option<EventResponse> = None pri
    /// prazdne buffer (no flush), Some pri dispatched event.
    fn flush_pending_mouse_move(&mut self) -> Option<rwe_engine::embed::EventResponse> {
        let (mx, my) = self.pending_mouse_pos.take()?;
        let coalesced = std::mem::take(&mut self.pending_coalesced);
        self.last_mouse_move = std::time::Instant::now();
        let event = InputEvent::MouseMove {
            x: mx, y: my,
            modifiers: KeyModifiers::default(),
            coalesced,
        };
        let resp = self.dispatch_input(event);
        if let (Some(cursor), Some(window)) = (resp.cursor.clone(), &self.window) {
            use rwe_engine::embed::CursorIcon as IC;
            let winit_cursor = match cursor {
                IC::Pointer => winit::window::CursorIcon::Pointer,
                IC::Text => winit::window::CursorIcon::Text,
                IC::Wait => winit::window::CursorIcon::Wait,
                IC::Help => winit::window::CursorIcon::Help,
                IC::Crosshair => winit::window::CursorIcon::Crosshair,
                IC::Move => winit::window::CursorIcon::Move,
                IC::NotAllowed => winit::window::CursorIcon::NotAllowed,
                IC::Grab => winit::window::CursorIcon::Grab,
                IC::Grabbing => winit::window::CursorIcon::Grabbing,
                IC::ResizeEw => winit::window::CursorIcon::EwResize,
                IC::ResizeNs => winit::window::CursorIcon::NsResize,
                IC::ResizeNesw => winit::window::CursorIcon::NeswResize,
                IC::ResizeNwse => winit::window::CursorIcon::NwseResize,
                IC::Default => winit::window::CursorIcon::Default,
            };
            window.set_cursor(winit_cursor);
        }
        Some(resp)
    }

    /// Konvenience: dispatch InputEvent na spravny WebView.
    /// - Mouse events (Move/Down/Up/Scroll): dle y position (chrome/page/dev).
    /// - Keyboard events (KeyDown/KeyUp/TextInput): dle WebView s focused
    ///   input/textarea. Bez tohoto by mouse drift posunul keyboard dispatch
    ///   na nesedici pane.
    fn dispatch_input(&mut self, event: InputEvent) -> rwe_engine::embed::EventResponse {
        // Keyboard events: route do focused pane.
        let is_keyboard = matches!(event,
            InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. }
            | InputEvent::TextInput { .. });
        if is_keyboard {
            let chrome_focused = self.chrome.as_ref()
                .map(|wv| wv.has_focused_input()).unwrap_or(false);
            let page_focused = self.webview.as_ref()
                .map(|wv| wv.has_focused_input()).unwrap_or(false);
            let dev_focused = self.devtools.as_ref()
                .map(|wv| wv.has_focused_input()).unwrap_or(false);
            if chrome_focused {
                return self.chrome.as_mut()
                    .map(|wv| wv.handle_input(event)).unwrap_or_default();
            }
            if dev_focused {
                return self.devtools.as_mut()
                    .map(|wv| wv.handle_input(event)).unwrap_or_default();
            }
            if page_focused {
                return self.webview.as_mut()
                    .map(|wv| wv.handle_input(event)).unwrap_or_default();
            }
            // Fallback: klavesnice bez fokusu jde do PAGE webview - drive
            // spadla do position-based routingu (mouse na chrome baru =>
            // Tab traversal fokusoval chrome tlacitka misto stranky).
            return self.webview.as_mut()
                .map(|wv| wv.handle_input(event)).unwrap_or_default();
        }
        // Route dle Y z EVENTU (fallback last mouse_y pro non-pozicni eventy).
        // Drive vzdy self.mouse_y (posledni CursorMoved) - down/up/scroll bez
        // predchoziho move (napr. testovaci PostMessage) sel do spatneho pane.
        let event_y = match &event {
            InputEvent::MouseMove { y, .. } | InputEvent::MouseDown { y, .. }
            | InputEvent::MouseUp { y, .. } | InputEvent::Scroll { y, .. } => Some(*y),
            _ => None,
        };
        let ref_y = event_y.unwrap_or(self.mouse_y);
        let in_chrome = self.point_in_chrome(ref_y);
        let in_dev = !in_chrome && self.point_in_devtools(ref_y);
        let y_off = if in_chrome { 0.0 }
                    else if in_dev { self.devtools_y_offset() }
                    else { self.chrome_h };  // page area starts at chrome_h
        let adjusted = match event {
            InputEvent::MouseMove { x, y, modifiers, coalesced } =>
                InputEvent::MouseMove {
                    x, y: y - y_off, modifiers,
                    // Adjust coalesced positions by same y_off.
                    coalesced: coalesced.into_iter()
                        .map(|(cx, cy)| (cx, cy - y_off)).collect(),
                },
            InputEvent::MouseDown { x, y, button, modifiers } =>
                InputEvent::MouseDown { x, y: y - y_off, button, modifiers },
            InputEvent::MouseUp { x, y, button, modifiers } =>
                InputEvent::MouseUp { x, y: y - y_off, button, modifiers },
            InputEvent::Scroll { dx, dy, x, y, modifiers } =>
                InputEvent::Scroll { dx, dy, x, y: y - y_off, modifiers },
            other => other,
        };
        if in_chrome {
            self.chrome.as_mut().map(|wv| wv.handle_input(adjusted)).unwrap_or_default()
        } else if in_dev {
            self.devtools.as_mut().map(|wv| wv.handle_input(adjusted)).unwrap_or_default()
        } else {
            self.webview.as_mut().map(|wv| wv.handle_input(adjusted)).unwrap_or_default()
        }
    }

    /// D5: Toggle inspector mode (Ctrl+Shift+C). Pri zapnuti se pri kazdem
    /// CursorMoved nad page hit-testuje DOM, set inspect_target. Overlay
    /// painter na page WebView (registered pri prvnim toggle) paint modry
    /// rect okolo hovered element. Klik v inspect mode -> emit CDP event.
    fn toggle_inspect_mode(&mut self) {
        let was = self.inspect_mode;
        self.inspect_mode = !was;
        if self.inspect_mode {
            // Install overlay_painter na page WebView. Closure cte sdilene
            // inspect_target Rc + emit modry rect okolo node z layout_root.
            if let Some(wv) = &mut self.webview {
                let target = Rc::clone(&self.inspect_target);
                let painter: Box<dyn FnMut(
                    &rwe_engine::browser::layout::LayoutBox,
                    f32,
                    &mut Vec<rwe_engine::browser::paint::DisplayCommand>,
                )> = Box::new(move |layout_root, _scroll_y, cmds| {
                    let target_id = *target.borrow();
                    let Some(target_ptr) = target_id else { return };
                    if let Some(rect) = find_layout_rect(layout_root, target_ptr) {
                        // Outline: 4 tenke rects okolo bounds.
                        let (x, y, w, h) = rect;
                        let border = 2.0;
                        let color = [80, 180, 240, 255]; // Modra
                        use rwe_engine::browser::paint::DisplayCommand;
                        // Top
                        cmds.push(DisplayCommand::Rect { x, y, w, h: border, color, radius: 0.0 });
                        // Bottom
                        cmds.push(DisplayCommand::Rect { x, y: y + h - border, w, h: border, color, radius: 0.0 });
                        // Left
                        cmds.push(DisplayCommand::Rect { x, y, w: border, h, color, radius: 0.0 });
                        // Right
                        cmds.push(DisplayCommand::Rect { x: x + w - border, y, w: border, h, color, radius: 0.0 });
                        // Polo-pruhledne pozadi.
                        cmds.push(DisplayCommand::Rect {
                            x: x + border, y: y + border,
                            w: (w - 2.0 * border).max(0.0),
                            h: (h - 2.0 * border).max(0.0),
                            color: [80, 180, 240, 50],
                            radius: 0.0,
                        });
                    }
                });
                wv.set_overlay_painter(painter);
            }
            println!("[shell] inspect mode ON (Ctrl+Shift+C toggle)");
        } else {
            // Clear target + remove overlay painter (set None pres dummy).
            *self.inspect_target.borrow_mut() = None;
            if let Some(wv) = &mut self.webview {
                wv.set_overlay_painter(Box::new(|_, _, _| {}));
            }
            println!("[shell] inspect mode OFF");
        }
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    /// F12 toggle: pri prvnim volani vytvori devtools WebView + load
    /// build_devtools_html(). Pri kazdem dalsim flippe visibility flag.
    fn toggle_devtools(&mut self) {
        let was_visible = self.devtools_visible;
        self.devtools_visible = !was_visible;
        if !self.devtools_visible {
            // Hide -> DROP devtools WebView. JS interp, DOM, caches, timers,
            // animations - vse odpoji. Pri dalsim toggle se rebuilduje cold.
            // Bez tohoto stale tickly intervals + thread_local caches accumulated
            // entries -> page perf degradace po close.
            self.devtools = None;
            self.devtools_target = None;
            self.cdp_channel = None;
            // Cleanup inspect state - bez tohoto picker_active=true persistne
            // i po devtools close (page hit-test + highlight nedeaktivuje).
            // Take page WV overlay_painter clean - dummy painter (no-op).
            {
                let mut s = self.inspect_state.borrow_mut();
                s.picker_active = false;
                s.hovered_node = None;
                s.selected_node = None;
            }
            if let Some(wv) = &mut self.webview {
                wv.set_overlay_painter(Box::new(|_, _, _| {}));
            }
        }
        if self.devtools_visible && self.devtools.is_none() {
            let t_start = std::time::Instant::now();
            let engine = match &self.engine { Some(e) => e.clone(), None => return };
            let renderer = match &self.renderer { Some(r) => r, None => return };
            let (sw, sh) = renderer.surface_size();
            let sf = renderer.scale_factor_value().max(0.01);
            let lw = ((sw as f32 / sf) as u32).max(1);
            let lh = ((sh as f32 / sf) as u32).max(1);
            let mut dv = WebView::new(engine, lw, lh);
            dv.resize(lw, lh, sf);
            let t_alloc = std::time::Instant::now();
            let dv_html = Self::build_devtools_html();
            let inline_css = Self::extract_inline_styles(&dv_html);
            let t_html = std::time::Instant::now();
            let _ = dv.load_dom(&dv_html, &inline_css, None);
            let t_load_dom = std::time::Instant::now();
            let channel = CdpChannel::new();
            Self::install_cdp_natives(&mut dv, &channel);
            let t_natives = std::time::Instant::now();
            dv.run_scripts();
            let t_scripts = std::time::Instant::now();
            let html_len = dv_html.len();
            let css_len = inline_css.len();
            eprintln!("[PROF F12] alloc:{:.0}ms html_build:{:.0}ms load_dom:{:.0}ms natives:{:.0}ms scripts:{:.0}ms (html={} css={})",
                t_alloc.duration_since(t_start).as_secs_f32() * 1000.0,
                t_html.duration_since(t_alloc).as_secs_f32() * 1000.0,
                t_load_dom.duration_since(t_html).as_secs_f32() * 1000.0,
                t_natives.duration_since(t_load_dom).as_secs_f32() * 1000.0,
                t_scripts.duration_since(t_natives).as_secs_f32() * 1000.0,
                html_len, css_len);
            self.devtools = Some(dv);
            self.devtools_target = Some(
                DevtoolsTarget::new().with_inspect_state(Rc::clone(&self.inspect_state))
            );
            // Install overlay_painter na PAGE WebView - cte shared InspectState
            // a emit box-model highlight pres hovered_node (= devtools tree
            // hover OR picker hit-test).
            if let Some(wv) = &mut self.webview {
                let st = Rc::clone(&self.inspect_state);
                wv.set_overlay_painter(Box::new(move |layout_root, scroll_y, cmds| {
                    let inspect = st.borrow();
                    // Hovered (transient = picker mode hover OR tree row hover):
                    // standard Chrome 4-layer box-model overlay.
                    if let Some(nid) = inspect.hovered_node {
                        emit_box_model_highlight(layout_root, nid, scroll_y,
                            &inspect.highlight_options, cmds);
                    }
                    // Selected (persistent = po picker click NEBO tree click):
                    // jen content rect outline (purple = Chrome convention),
                    // bez full margin/border/padding fill (= no overlap s hover).
                    if let Some(nid) = inspect.selected_node {
                        if Some(nid) != inspect.hovered_node {
                            emit_selected_outline(layout_root, nid, scroll_y, cmds);
                        }
                    }
                }));
            }
            self.cdp_channel = Some(channel);
            eprintln!("[PROF F12] TOTAL toggle_devtools:{:.0}ms",
                t_scripts.duration_since(t_start).as_secs_f32() * 1000.0);
        }
        // Resize obou webview podle aktualniho split state (toggle on/off).
        self.resize_views();
        println!("[shell] devtools visible: {}", self.devtools_visible);
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    fn nav_back(&mut self) {
        if self.history_idx == 0 { return; }
        self.history_idx -= 1;
        let url = self.history[self.history_idx].clone();
        if let Some(wv) = &mut self.webview {
            wv.load_url(&url);
        }
        self.sync_chrome_url();
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    fn nav_forward(&mut self) {
        if self.history_idx + 1 >= self.history.len() { return; }
        self.history_idx += 1;
        let url = self.history[self.history_idx].clone();
        if let Some(wv) = &mut self.webview {
            wv.load_url(&url);
        }
        self.sync_chrome_url();
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    /// Self-driven test scenario. Pri RWE_AUTOTEST_F12=1 po 500ms toggle devtools.
    /// Pri RWE_AUTOTEST_HOVER=1 emit mouse_move pres devtools area per frame.
    /// Bez user GUI interakce, captured stderr/stdout pres `cargo run 2>&1 | tee`.
    fn autotest_tick(&mut self) {
        if self.autotest_deadline.is_none() { return; }
        let elapsed = self.autotest_start.elapsed();
        // F12 toggle po 500ms.
        if self.autotest_f12 && !self.autotest_f12_done && elapsed.as_millis() >= 500 {
            eprintln!("[AUTOTEST] toggle devtools (t={}ms)", elapsed.as_millis());
            self.toggle_devtools();
            self.autotest_f12_done = true;
            if let Some(w) = &self.window { w.request_redraw(); }
            return;
        }
        // Hover ticks po 1500ms - emit mouse_move pres devtools area.
        if self.autotest_hover && self.autotest_f12_done && elapsed.as_millis() >= 1500 {
            self.autotest_hover_tick = self.autotest_hover_tick.wrapping_add(1);
            // Pohyb cyklicky pres x=100..600 y=500..700 (devtools area).
            let tx = 100.0 + (self.autotest_hover_tick % 50) as f32 * 10.0;
            let ty = 500.0 + ((self.autotest_hover_tick / 50) % 20) as f32 * 10.0;
            self.mouse_x = tx;
            self.mouse_y = ty;
            let evt = rwe_engine::embed::InputEvent::MouseMove {
                x: tx, y: ty,
                modifiers: rwe_engine::embed::KeyModifiers::default(),
                coalesced: Vec::new(),
            };
            let _ = self.dispatch_input(evt);
            if let Some(w) = &self.window { w.request_redraw(); }
        }
    }

    fn redraw(&mut self) {
        // FPS counter - measure frame time + EMA (30 frame ring).
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32() * 1000.0;
        self.last_frame_time = now;
        if self.frame_times_ms.len() >= 30 { self.frame_times_ms.pop_front(); }
        self.frame_times_ms.push_back(dt);

        // STRUCT LOG: per-frame breakdown kdyz frame trva > 50ms (slow frame).
        let t_frame_start = now;
        let frame_idx = self.frame_counter;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Drain chrome bar command queue (back/fwd/navigate/reload).
        self.drain_chrome_cmds();
        let t_chrome_drain = std::time::Instant::now();
        // CDP pump pred render - drain pending requests + push responses.
        // Po pump musime force redraw - inspect_state.hovered_node mohl byt
        // setnut, ale page WV by jinak nemel duvod redraw (mouse over devtools
        // pane != mouse over page).
        if self.devtools_visible {
            let prev_hovered = self.inspect_state.borrow().hovered_node;
            self.pump_cdp();
            let cur_hovered = self.inspect_state.borrow().hovered_node;
            if prev_hovered != cur_hovered {
                if let Some(w) = &self.window { w.request_redraw(); }
            }
        }
        let t_pump_cdp = std::time::Instant::now();
        let renderer = match &mut self.renderer { Some(r) => r, None => return };

        // Render vsech 3 WebViews do jejich offscreen RT (field-disjoint mut).
        // Per-WV timing pro diagnostiku - ulozime do struct fields.
        let t0 = std::time::Instant::now();
        let chrome_anim = self.chrome.as_mut()
            .map(|wv| { let _ = wv.render_via(renderer); wv.has_active_animations() })
            .unwrap_or(false);
        let t1 = std::time::Instant::now();
        let page_anim = self.webview.as_mut()
            .map(|wv| { let _ = wv.render_via(renderer); wv.has_active_animations() })
            .unwrap_or(false);
        let t2 = std::time::Instant::now();
        let dev_anim = if self.devtools_visible {
            self.devtools.as_mut()
                .map(|wv| { let _ = wv.render_via(renderer); wv.has_active_animations() })
                .unwrap_or(false)
        } else { false };
        let t3 = std::time::Instant::now();
        self.last_chrome_ms = t1.duration_since(t0).as_secs_f32() * 1000.0;
        self.last_page_ms = t2.duration_since(t1).as_secs_f32() * 1000.0;
        self.last_dev_ms = t3.duration_since(t2).as_secs_f32() * 1000.0;
        let _ = renderer;

        // Present layered: chrome top (fixed h), page middle, devtools bottom.
        let sf = self.renderer.as_ref().map(|r| r.scale_factor_value()).unwrap_or(1.0);
        let chrome_h_px = (self.chrome_h * sf) as u32;
        let dev_visible = self.devtools_visible;
        let r = match self.renderer.as_ref() { Some(r) => r, None => return };
        let chrome_v = self.chrome.as_ref().and_then(|w| w.target_view());
        let page_v = self.webview.as_ref().and_then(|w| w.target_view());
        let dev_v = self.devtools.as_ref().and_then(|w| w.target_view());

        // Build layers vec - chrome + page + maybe devtools.
        // Last layer dostane zbytek vysky (presents helper).
        match (chrome_v, page_v, dev_v) {
            (Some(c), Some(p), Some(d)) if dev_visible => {
                let surface_h = r.surface_size().1;
                let split = self.devtools_split_ratio.clamp(0.05, 0.95);
                let content_h = surface_h.saturating_sub(chrome_h_px);
                let dev_h_px = ((content_h as f32) * split) as u32;
                let page_h_px = content_h.saturating_sub(dev_h_px);
                r.present_layered_external_to_swap_chain(&[
                    (c, chrome_h_px), (p, page_h_px), (d, dev_h_px),
                ]);
            }
            (Some(c), Some(p), _) => {
                r.present_layered_external_to_swap_chain(&[
                    (c, chrome_h_px), (p, 0),  // page bere zbytek
                ]);
            }
            (None, Some(p), _) => {
                r.present_external_to_swap_chain(p);
            }
            _ => return,
        }

        // Window title sync + FPS counter.
        if let (Some(window), Some(wv)) = (&self.window, &self.webview) {
            let t = wv.title();
            let avg_ms = if self.frame_times_ms.is_empty() { 0.0 }
                else { self.frame_times_ms.iter().sum::<f32>() / self.frame_times_ms.len() as f32 };
            let fps = if avg_ms > 0.01 { 1000.0 / avg_ms } else { 999.0 };
            let title_base = if t.is_empty() { "RustWebEngine".to_string() }
                else { format!("{} - RustWebEngine", t) };
            // Per-phase timing PAGE WV - kde je drahy: cascade/layout/paint/gpu?
            // (Driv devtools WV = vzdy 0 kdyz zavrene; page je co optimalizujem.)
            let (dc, dl, dp, dg) = self.webview.as_ref()
                .map(|w| w.render_phase_times())
                .unwrap_or((0.0, 0.0, 0.0, 0.0));
            // L1 compositor diagnostika: pocet layer + cache hit rate v posledni WV render.
            let layer_p = self.webview.as_ref().map(|w| w.layer_count()).unwrap_or(0);
            let layer_d = self.devtools.as_ref().map(|w| w.layer_count()).unwrap_or(0);
            let (page_cached, _, page_total) = self.webview.as_ref()
                .map(|w| w.layer_cache_stats()).unwrap_or((0, 0, 0));
            let win_title = format!(
                "[{:.0} FPS {:.1}ms | C:{:.1} P:{:.1}/L{}({}c/{}) D:{:.1}/L{} (cas:{:.1} lay:{:.1} pnt:{:.1} gpu:{:.1})] {}",
                fps, avg_ms, self.last_chrome_ms, self.last_page_ms, layer_p,
                page_cached, page_total,
                self.last_dev_ms, layer_d,
                dc, dl, dp, dg, title_base);
            if window.title() != win_title {
                window.set_title(&win_title);
            }
        }
        // Trigger redraw pri active anim NEBO pending setInterval (CDP poll).
        let any_intervals = self.chrome.as_ref().map(|w| w.has_pending_intervals()).unwrap_or(false)
            || self.webview.as_ref().map(|w| w.has_pending_intervals()).unwrap_or(false)
            || self.devtools.as_ref().map(|w| w.has_pending_intervals()).unwrap_or(false);
        if chrome_anim || page_anim || dev_anim || any_intervals {
            if let Some(w) = &self.window { w.request_redraw(); }
        }

        // STRUCT LOG: slow frame trace (>50ms). Vsechny stage cas.
        let t_end = std::time::Instant::now();
        let total_ms = t_end.duration_since(t_frame_start).as_secs_f32() * 1000.0;
        if total_ms > 50.0 {
            let chrome_drain_ms = t_chrome_drain.duration_since(t_frame_start).as_secs_f32() * 1000.0;
            let pump_ms = t_pump_cdp.duration_since(t_chrome_drain).as_secs_f32() * 1000.0;
            eprintln!("[FRAME #{} SLOW {:.0}ms] chrome_drain:{:.1} pump_cdp:{:.1} chrome_render:{:.1} page_render:{:.1} dev_render:{:.1} dev_phases(cas/lay/pnt/gpu):{:.1}/{:.1}/{:.1}/{:.1}",
                frame_idx, total_ms,
                chrome_drain_ms, pump_ms,
                self.last_chrome_ms, self.last_page_ms, self.last_dev_ms,
                self.devtools.as_ref().map(|w| w.render_phase_times().0).unwrap_or(0.0),
                self.devtools.as_ref().map(|w| w.render_phase_times().1).unwrap_or(0.0),
                self.devtools.as_ref().map(|w| w.render_phase_times().2).unwrap_or(0.0),
                self.devtools.as_ref().map(|w| w.render_phase_times().3).unwrap_or(0.0),
            );
        }
    }

    /// Resize page + devtools webview podle aktualniho devtools_visible
    /// + devtools_split_ratio. Volat pri F12 toggle + WindowResize.
    fn resize_views(&mut self) {
        let r = match &self.renderer { Some(r) => r, None => return };
        let sf = r.scale_factor_value().max(0.01);
        let (sw, sh) = r.surface_size();
        let lw = ((sw as f32 / sf) as u32).max(1);
        let lh_full = ((sh as f32 / sf) as u32).max(1);
        // Chrome bar fixni vyska nahore. Page + devtools sdili zbytek.
        let chrome_h = (self.chrome_h as u32).min(lh_full.saturating_sub(1));
        if let Some(c) = &mut self.chrome { c.resize(lw, chrome_h.max(1), sf); }
        let content_h = lh_full.saturating_sub(chrome_h).max(1);
        if self.devtools_visible {
            let split = self.devtools_split_ratio.clamp(0.05, 0.95);
            let dev_h = ((content_h as f32) * split).round().max(1.0) as u32;
            let page_h = content_h.saturating_sub(dev_h).max(1);
            if let Some(wv) = &mut self.webview { wv.resize(lw, page_h, sf); }
            if let Some(dv) = &mut self.devtools { dv.resize(lw, dev_h, sf); }
        } else {
            if let Some(wv) = &mut self.webview { wv.resize(lw, content_h, sf); }
            if let Some(dv) = &mut self.devtools { dv.resize(lw, content_h, sf); }
        }
    }
}

impl ApplicationHandler for ShellApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let title = match &self.local_path {
            Some(p) => format!("RustWebEngine Shell - {}", p.display()),
            None => "RustWebEngine Shell".to_string(),
        };
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 900.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(400.0, 300.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create_window"));
        let renderer = Renderer::new(window.clone());

        let device = Arc::new(renderer.device().clone());
        let queue = Arc::new(renderer.queue().clone());
        let engine = Arc::new(Engine::new(device, queue));

        let (sw, sh) = renderer.surface_size();
        let sf = renderer.scale_factor_value().max(0.01);
        let lw = ((sw as f32 / sf) as u32).max(1);
        let lh = ((sh as f32 / sf) as u32).max(1);
        let mut webview = WebView::new(engine.clone(), lw, lh);
        webview.resize(lw, lh, sf);
        webview.set_local_path(self.local_path.clone());
        let _ = webview.load_html(&self.html, &self.css, self.base_url.clone());

        // Chrome bar WebView - back/fwd/reload + URL input. Fixed top.
        let chrome_h_px = (self.chrome_h * sf) as u32;
        let mut chrome = WebView::new(engine.clone(), lw, chrome_h_px.max(1));
        chrome.resize(lw, chrome_h_px.max(1), sf);
        let initial_url = self.base_url.clone().unwrap_or_else(|| "about:blank".to_string());
        let chrome_html = Self::build_chrome_html(&initial_url);
        let chrome_css = Self::extract_inline_styles(&chrome_html);
        let _ = chrome.load_html(&chrome_html, &chrome_css, None);
        // Install native shell command bindings (back/fwd/reload/navigate).
        Self::install_chrome_natives(&mut chrome, &self.chrome_cmds);

        // History init s initial URL - pri startu pres CLI arg.
        if self.history.is_empty() {
            let init_url = self.base_url.clone()
                .or_else(|| self.local_path.as_ref().map(|p| p.to_string_lossy().to_string()))
                .unwrap_or_else(|| "about:blank".to_string());
            self.history.push(init_url);
            self.history_idx = 0;
        }

        self.window = Some(window.clone());
        self.renderer = Some(renderer);
        self.engine = Some(engine);
        self.webview = Some(webview);
        self.chrome = Some(chrome);
        // Resize_views aplikuje chrome_h slot - page dostane lh - chrome_h.
        self.resize_views();
        // Initial chrome URL sync z history.
        self.sync_chrome_url();

        println!("[shell] vlastni okno + WebView + chrome bar");
        window.request_redraw();

        // AUTOTEST hook - po N sekundach proc exit + log ticker pro:
        // RWE_AUTOTEST=N        - exit po N s
        // RWE_AUTOTEST_F12=1    - po 500ms toggle devtools (pres script tick)
        // RWE_AUTOTEST_HOVER=1  - po 1500ms emit mouse hovers nad devtools
        // Hover akce pres script_tick - bypass ApplicationHandler thread safety
        // (single-thread event loop polluje misto cross-thread synthesis).
        if let Ok(v) = std::env::var("RWE_AUTOTEST") {
            let secs: u64 = v.parse().unwrap_or(8);
            self.autotest_deadline = Some(std::time::Instant::now()
                + std::time::Duration::from_secs(secs));
            self.autotest_f12 = std::env::var("RWE_AUTOTEST_F12").is_ok();
            self.autotest_hover = std::env::var("RWE_AUTOTEST_HOVER").is_ok();
            self.autotest_start = std::time::Instant::now();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(secs));
                std::process::exit(0);
            });
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Event loop drained vsechny pending events. Flush coalesced MouseMove
        // buffer + request redraw JEN kdyz dispatch zmenil visual state (dirty).
        // Bez tohoto by 1000Hz mouse mensim eventem requestoval 1000 redraws/s
        // = 100% CPU i bez visual change.
        if let Some(resp) = self.flush_pending_mouse_move() {
            if resp.dirty {
                if let Some(w) = &self.window { w.request_redraw(); }
            }
        }
        // Has pending JS intervals/timers / has_pending_intervals across views?
        // Then schedule redraw to keep them ticking.
        let needs_tick = self.webview.as_ref().map(|w| w.has_pending_intervals() || w.has_pending_raf()).unwrap_or(false)
            || self.chrome.as_ref().map(|w| w.has_pending_intervals()).unwrap_or(false)
            || self.devtools.as_ref().map(|w| w.has_pending_intervals()).unwrap_or(false);
        if needs_tick {
            if let Some(w) = &self.window { w.request_redraw(); }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                // Coalesce - jen uloz posledni velikost, realny resize (texture
                // realloc + reflow) az v RedrawRequested. Bez toho winit per-pixel
                // Resized = N realloc+reflow per frame = trhane tazeni okna.
                self.pending_resize = Some((size.width, size.height));
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.resize_views();
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                // Aplikuj coalesced resize jednou (posledni velikost) pred redraw.
                if let Some((rw, rh)) = self.pending_resize.take() {
                    if let Some(r) = &mut self.renderer {
                        r.resize_surface(rw, rh);
                    }
                    self.resize_views();
                }
                // Flush coalesced MouseMove buffer pred redraw - single dispatch
                // s history pres PointerEvent.getCoalescedEvents() API.
                self.flush_pending_mouse_move();
                self.autotest_tick();
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.renderer.as_ref().map(|r| r.scale_factor_value()).unwrap_or(1.0);
                self.mouse_x = position.x as f32 / scale;
                self.mouse_y = position.y as f32 / scale;
                // CDP Overlay picker mode: pres mouse over page area + picker_active
                // hit-test layout + set hovered_node v shared InspectState.
                // Devtools-frontend emits Overlay.setInspectMode -> picker_active true.
                {
                    let picker = self.inspect_state.borrow().picker_active;
                    if picker && !self.point_in_chrome(self.mouse_y) && !self.point_in_devtools(self.mouse_y) {
                        let chrome_h = self.chrome_h;
                        // Mouse coords pres LOGICAL pixels (z winit / scale_factor).
                        // Layout coords v VIEWPORT_LOGICAL/zoom (= logical/zoom).
                        // Per zoom 2x mouse_x=100 -> layout_x = 100/2 = 50.
                        let zoom = self.webview.as_ref().map(|w| w.zoom()).unwrap_or(1.0).max(0.01);
                        let page_x = self.mouse_x / zoom;
                        let page_y = (self.mouse_y - chrome_h) / zoom;
                        let (scroll_x, scroll_y) = self.webview.as_ref()
                            .map(|w| w.scroll()).unwrap_or((0.0, 0.0));
                        let target = self.webview.as_ref()
                            .and_then(|w| w.last_layout_root())
                            .and_then(|root| pick_node_at(root, page_x + scroll_x, page_y + scroll_y));
                        let prev = self.inspect_state.borrow().hovered_node;
                        if target != prev {
                            self.inspect_state.borrow_mut().hovered_node = target;
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                    }
                }
                if self.inspect_mode && !self.point_in_devtools(self.mouse_y) {
                    let target = self.webview.as_ref()
                        .and_then(|w| w.last_layout_root())
                        .and_then(|root| pick_node_at(root, self.mouse_x, self.mouse_y));
                    let prev = *self.inspect_target.borrow();
                    if target != prev {
                        *self.inspect_target.borrow_mut() = target;
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }
                    return;
                }
                // D4d: pri active splitter drag updatuj split_ratio.
                if self.splitter_drag {
                    let r = match &self.renderer { Some(r) => r, None => return };
                    let sf = r.scale_factor_value().max(0.01);
                    let (_sw, sh) = r.surface_size();
                    let lh_full = (sh as f32 / sf).max(1.0);
                    // mouse_y = split_y line, devtools start at this y, devtools_h = lh_full - mouse_y
                    let new_ratio = ((lh_full - self.mouse_y) / lh_full).clamp(0.05, 0.95);
                    self.devtools_split_ratio = new_ratio;
                    self.resize_views();
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
                // D4d: hover splitter -> NS resize cursor.
                if self.point_on_splitter(self.mouse_x, self.mouse_y) {
                    if let Some(window) = &self.window {
                        window.set_cursor(winit::window::CursorIcon::NsResize);
                    }
                    return;
                }
                // Coalesce MouseMove eventy do buffer. OS dodava CursorMoved
                // ~1000Hz, my dispatch jen 1 per frame slot v about_to_wait.
                // Prev pending position push do coalesced history pro JS
                // PointerEvent.getCoalescedEvents() API.
                // POZOR: NE request_redraw - to by trigerlo full WebView render
                // (5ms+) per mouse event burst = 200 redraws/s = 100% CPU.
                // about_to_wait flush + dirty check rozhodne zda redraw nutny.
                if let Some(prev) = self.pending_mouse_pos.replace((self.mouse_x, self.mouse_y)) {
                    if self.pending_coalesced.len() < 64 {
                        self.pending_coalesced.push(prev);
                    }
                }
                return;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * -60.0, y * -60.0),
                    MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
                };
                if std::env::var("RWE_SCROLL_DBG").is_ok() {
                    eprintln!("[shell wheel] dx={} dy={} mouse_y={} in_chrome={} in_dev={}",
                        dx, dy, self.mouse_y,
                        self.point_in_chrome(self.mouse_y),
                        self.point_in_devtools(self.mouse_y));
                }
                // Ctrl+Wheel = zoom in/out (common browser pattern).
                if self.modifiers.control_key() {
                    let new_zoom = self.with_active_mut(|wv| {
                        let z = wv.zoom();
                        let nz = if dy < 0.0 { (z * 1.1).min(5.0) } else { (z / 1.1).max(0.25) };
                        wv.set_zoom(nz);
                        nz
                    });
                    if let Some(nz) = new_zoom {
                        println!("[shell zoom] {:.0}%", nz * 100.0);
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }
                    return;
                }
                // Wheel routing: chrome bar nema scrollable content (jen
                // address bar + buttons), takze wheel events vzdy route do PAGE
                // WebView (Chrome/FF pattern). Devtools panel = scrollable, takze
                // pri mouse_y in devtools -> route to devtools.
                let in_dev = self.point_in_devtools(self.mouse_y);
                let y_off = if in_dev { self.devtools_y_offset() } else { self.chrome_h };
                let event = InputEvent::Scroll {
                    dx, dy,
                    x: self.mouse_x,
                    y: (self.mouse_y - y_off).max(0.0),
                    modifiers: KeyModifiers::default(),
                };
                let target_wv = if in_dev { self.devtools.as_mut() } else { self.webview.as_mut() };
                let resp = target_wv.map(|wv| wv.handle_input(event)).unwrap_or_default();
                if std::env::var("RWE_SCROLL_DBG").is_ok() {
                    eprintln!("[shell wheel route] in_dev={} dirty={}", in_dev, resp.dirty);
                }
                if resp.dirty {
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::DroppedFile(path) => {
                let path_str = path.to_string_lossy().to_string();
                println!("[shell drop] {path_str}");
                if let Some(wv) = &mut self.webview {
                    wv.load_url(&path_str);
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                use winit::keyboard::{Key, NamedKey};
                // Find on page capture.
                if self.find_open && matches!(key_event.state, ElementState::Pressed) {
                    match &key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.find_open = false;
                            self.find_query.clear();
                            println!("[shell find] cancel");
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            println!("[shell find] search '{}' (TBD highlight matches)", self.find_query);
                            // Real impl: find text v webview.text_runs + highlight + scroll to first.
                            return;
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.find_query.pop();
                            println!("[shell find] {}", self.find_query);
                            return;
                        }
                        Key::Character(s) => {
                            self.find_query.push_str(s);
                            println!("[shell find] {}", self.find_query);
                            return;
                        }
                        _ => return,
                    }
                }
                // Address bar capture - pri addr_open intercepta vsechny keys.
                if self.addr_open && matches!(key_event.state, ElementState::Pressed) {
                    match &key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.addr_open = false;
                            self.addr_input.clear();
                            println!("[shell addr] cancel");
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            let url = self.addr_input.trim().to_string();
                            self.addr_open = false;
                            self.addr_input.clear();
                            if !url.is_empty() {
                                println!("[shell addr] navigate: {}", url);
                                if let Some(wv) = &mut self.webview {
                                    if wv.load_url(&url).is_some() {
                                        self.history.truncate(self.history_idx + 1);
                                        self.history.push(url);
                                        self.history_idx = self.history.len() - 1;
                                        if let Some(w) = &self.window { w.request_redraw(); }
                                    } else {
                                        eprintln!("[shell addr] load failed");
                                    }
                                }
                            }
                            return;
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.addr_input.pop();
                            println!("[shell addr] {}", self.addr_input);
                            return;
                        }
                        Key::Character(s) => {
                            self.addr_input.push_str(s);
                            println!("[shell addr] {}", self.addr_input);
                            return;
                        }
                        _ => return,
                    }
                }
                // D5: Ctrl+Shift+C toggle inspect mode.
                if matches!(key_event.state, ElementState::Pressed)
                    && self.modifiers.control_key() && self.modifiers.shift_key() {
                    if let Key::Character(s) = &key_event.logical_key {
                        if s.eq_ignore_ascii_case("c") {
                            self.toggle_inspect_mode();
                            return;
                        }
                    }
                }
                // Ctrl+C: copy text selection do system clipboardu.
                if matches!(key_event.state, ElementState::Pressed) && self.modifiers.control_key() {
                    if let Key::Character(s) = &key_event.logical_key {
                        if s.eq_ignore_ascii_case("c") {
                            if let Some(text) = self.with_active(|wv| wv.selection_text()).flatten() {
                                if !text.is_empty() {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        let _ = cb.set_text(text);
                                        println!("[shell] copy: selection -> clipboard");
                                    }
                                }
                            }
                            return;
                        }
                        if s.eq_ignore_ascii_case("a") {
                            // Ctrl+A: ve focused inputu vybere text inputu, jinak
                            // celou stranku (docx2 r.44 "Ctrl+a oznaci celou stranku").
                            let in_input = self.with_active(|wv| wv.focused_is_input())
                                .unwrap_or(false);
                            if in_input {
                                self.with_active_mut(|wv| { wv.select_all_focused_input(); });
                            } else {
                                self.with_active_mut(|wv| wv.select_all());
                            }
                            if let Some(w) = &self.window { w.request_redraw(); }
                            return;
                        }
                        if s.eq_ignore_ascii_case("r") {
                            // Ctrl+R: reload PAGE (devtools ignore).
                            if let (Some(wv), Some(last)) = (&mut self.webview, self.history.get(self.history_idx).cloned()) {
                                wv.load_url(&last);
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                            return;
                        }
                        if s.eq_ignore_ascii_case("l") {
                            // Ctrl+L: toggle address bar. Vstup pres stdout
                            // (visual overlay = Phase 99). User tipuje, Enter
                            // load_url, Esc close.
                            self.addr_open = !self.addr_open;
                            if self.addr_open {
                                self.addr_input = self.history.get(self.history_idx)
                                    .cloned().unwrap_or_default();
                                println!("[shell addr] open. Tipuj URL + Enter / Esc.");
                                println!("[shell addr] current: {}", self.addr_input);
                            } else {
                                self.addr_input.clear();
                                println!("[shell addr] closed");
                            }
                            return;
                        }
                        if s.eq_ignore_ascii_case("f") {
                            // Ctrl+F: toggle find on page. Vstup pres stdout
                            // (visual overlay TBD). User tipuje query.
                            self.find_open = !self.find_open;
                            if self.find_open {
                                self.find_query.clear();
                                println!("[shell find] open. Tipuj query + Enter / Esc.");
                            } else {
                                self.find_query.clear();
                                println!("[shell find] closed");
                            }
                            return;
                        }
                        if matches!(s.as_str(), "+" | "=" | "-" | "_" | "0") {
                            let new_zoom = self.with_active_mut(|wv| {
                                let z = wv.zoom();
                                let nz = match s.as_str() {
                                    "+" | "=" => (z * 1.1).min(5.0),
                                    "-" | "_" => (z / 1.1).max(0.25),
                                    "0" => 1.0,
                                    _ => z,
                                };
                                wv.set_zoom(nz);
                                nz
                            });
                            if let Some(nz) = new_zoom {
                                println!("[shell zoom] {:.0}%", nz * 100.0);
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                            return;
                        }
                    }
                }
                // Alt+Left/Right -> history back/forward. F5 -> reload.
                if matches!(key_event.state, ElementState::Pressed) {
                    if self.modifiers.alt_key() {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => { self.nav_back(); return; }
                            Key::Named(NamedKey::ArrowRight) => { self.nav_forward(); return; }
                            _ => {}
                        }
                    }
                    if matches!(&key_event.logical_key, Key::Named(NamedKey::F5)) {
                        if let (Some(wv), Some(last)) = (&mut self.webview, self.history.get(self.history_idx).cloned()) {
                            wv.load_url(&last);
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                        return;
                    }
                    // F12: toggle DevTools (D4a full-screen swap; D4b split TBD).
                    if matches!(&key_event.logical_key, Key::Named(NamedKey::F12)) {
                        self.toggle_devtools();
                        return;
                    }
                    // Esc: clear selection v aktivnim WebView.
                    if matches!(&key_event.logical_key, Key::Named(NamedKey::Escape)) {
                        self.with_active_mut(|wv| wv.clear_selection());
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                }
                // Scroll keys: PageDown/Up, ArrowUp/Down, Home, End, Space.
                // Pri focused input -> skip scroll handler, klave projdou
                // do dispatch_input -> webview text insert/move handlery.
                if matches!(key_event.state, ElementState::Pressed) {
                    let shift = self.modifiers.shift_key();
                    let ctrl = self.modifiers.control_key();
                    let key_logical = key_event.logical_key.clone();
                    let focused_input = self.with_active(|wv| wv.focused_is_input()).unwrap_or(false);
                    let new_y = if focused_input { false } else {
                        self.with_active_mut(|webview| {
                            let (_vw, vh) = webview.viewport_size();
                            // Pri scroll keys dispatch pres webview.kbd_scroll_y -
                            // rozhodne mezi inner element scroll (pod kurzorem) vs
                            // viewport scroll.
                            let dy = match &key_logical {
                                Key::Named(NamedKey::PageDown) => Some(vh * 0.9),
                                Key::Named(NamedKey::PageUp) => Some(-vh * 0.9),
                                Key::Named(NamedKey::ArrowDown) if !ctrl => Some(60.0),
                                Key::Named(NamedKey::ArrowUp) if !ctrl => Some(-60.0),
                                Key::Named(NamedKey::Home) => Some(-1_000_000.0),
                                Key::Named(NamedKey::End) => Some(1_000_000.0),
                                Key::Named(NamedKey::Space) => {
                                    Some(if shift { -vh * 0.9 } else { vh * 0.9 })
                                }
                                _ => None,
                            };
                            if let Some(d) = dy { webview.kbd_scroll_y(d) }
                            else { false }
                        }).unwrap_or(false)
                    };
                    if new_y {
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                }
                let key_str: String = match &key_event.logical_key {
                    Key::Named(NamedKey::Enter) => "Enter".into(),
                    Key::Named(NamedKey::Escape) => "Escape".into(),
                    Key::Named(NamedKey::Backspace) => "Backspace".into(),
                    Key::Named(NamedKey::Tab) => "Tab".into(),
                    Key::Named(NamedKey::ArrowLeft) => "ArrowLeft".into(),
                    Key::Named(NamedKey::ArrowRight) => "ArrowRight".into(),
                    Key::Named(NamedKey::ArrowUp) => "ArrowUp".into(),
                    Key::Named(NamedKey::ArrowDown) => "ArrowDown".into(),
                    Key::Named(NamedKey::Home) => "Home".into(),
                    Key::Named(NamedKey::End) => "End".into(),
                    Key::Named(NamedKey::Delete) => "Delete".into(),
                    Key::Named(NamedKey::Space) => " ".into(),
                    Key::Character(s) => s.to_string(),
                    _ => return,
                };
                // Propaguj modifikatory (shift=extend selection, ctrl=po slovech)
                // do input editoru. Driv KeyModifiers::default() -> shift+home /
                // ctrl+sipka / Home / End nefungovaly (docx2 "vsechny zakladni
                // zkratky na ktere jsou lide zvykli").
                let mods = KeyModifiers {
                    shift: self.modifiers.shift_key(),
                    ctrl: self.modifiers.control_key(),
                    alt: self.modifiers.alt_key(),
                    meta: self.modifiers.super_key(),
                };
                if matches!(key_event.state, ElementState::Pressed) {
                    let resp = self.dispatch_input(InputEvent::KeyDown {
                        key: key_str.clone(),
                        modifiers: mods,
                    });
                    if resp.dirty {
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }
                    // Char keys + Space taky emit TextInput. Bez Space NamedKey
                    // by mezernik se nikdy nevlozil do input pole (Space je
                    // Key::Named, ne Character).
                    let text_to_insert: Option<String> = match &key_event.logical_key {
                        Key::Character(s) => Some(s.to_string()),
                        Key::Named(NamedKey::Space) => Some(" ".into()),
                        _ => None,
                    };
                    if let Some(t) = text_to_insert {
                        let resp = self.dispatch_input(InputEvent::TextInput { text: t });
                        if resp.dirty {
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                    }
                } else {
                    let key_str_release = match &key_event.logical_key {
                        Key::Character(s) => s.to_string(),
                        Key::Named(NamedKey::Enter) => "Enter".into(),
                        _ => return,
                    };
                    self.dispatch_input(InputEvent::KeyUp {
                        key: key_str_release,
                        modifiers: KeyModifiers::default(),
                    });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    WinitMouseButton::Left => MouseButton::Left,
                    WinitMouseButton::Right => MouseButton::Right,
                    WinitMouseButton::Middle => MouseButton::Middle,
                    WinitMouseButton::Back => MouseButton::Other(3),
                    WinitMouseButton::Forward => MouseButton::Other(4),
                    WinitMouseButton::Other(b) => MouseButton::Other(b),
                };
                // D4d: LMB Down/Up na splitter zacne / ukonci drag.
                if matches!(btn, MouseButton::Left) {
                    match state {
                        ElementState::Pressed if self.point_on_splitter(self.mouse_x, self.mouse_y) => {
                            self.splitter_drag = true;
                            return;
                        }
                        ElementState::Released if self.splitter_drag => {
                            self.splitter_drag = false;
                            return;
                        }
                        _ => {}
                    }
                }
                // CDP Overlay picker mode (Phase 1b): LMB Press v page area pri
                // picker_active=true -> emit Overlay.inspectNodeRequested CDP event
                // (frontend listener select node v tree + scroll to it).
                // Toggle off picker after click (Chrome behavior).
                if matches!(btn, MouseButton::Left)
                    && matches!(state, ElementState::Pressed)
                    && self.inspect_state.borrow().picker_active
                    && !self.point_in_devtools(self.mouse_y)
                    && !self.point_in_chrome(self.mouse_y)
                {
                    let node_ptr = self.inspect_state.borrow().hovered_node;
                    if let (Some(ptr), Some(channel), Some(target))
                        = (node_ptr, self.cdp_channel.as_ref(), self.devtools_target.as_ref())
                    {
                        // Resolve ptr -> CDP NodeId (sequential int = Chrome standard).
                        // None = ptr neni v table (= node nebyl jeste serializovany pres
                        // DOM.getDocument). Frontend tree muze nemit row pres tento node.
                        let id = target.id_for_ptr(ptr).unwrap_or(0);
                        let evt = rwe_devtools_proto::DevtoolsEvent {
                            method: "Overlay.inspectNodeRequested".to_string(),
                            params: serde_json::json!({
                                "backendNodeId": id,
                                "nodeId": id,
                            }),
                        };
                        let json = serde_json::to_string(&evt).unwrap_or_default();
                        channel.resp_queue.borrow_mut().push_back(json);
                        // Take set selected_node v shared state - page overlay
                        // bude render persistent highlight pres selected (vs transient hovered).
                        let mut s = self.inspect_state.borrow_mut();
                        s.selected_node = Some(ptr);
                        s.picker_active = false;
                        s.hovered_node = None;
                        let _ = ptr; let _ = id;
                    } else {
                        // No hovered node (click outside any element) - just exit picker.
                        let mut s = self.inspect_state.borrow_mut();
                        s.picker_active = false;
                        s.hovered_node = None;
                    }
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
                // D5: LMB Press v inspect_mode + page area -> emit CDP event
                // DOM.inspectNodeRequested (frontend elements panel selectne).
                if matches!(btn, MouseButton::Left)
                    && matches!(state, ElementState::Pressed)
                    && self.inspect_mode
                    && !self.point_in_devtools(self.mouse_y) {
                    let node_ptr = *self.inspect_target.borrow();
                    if let (Some(ptr), Some(channel)) = (node_ptr, self.cdp_channel.as_ref()) {
                        let evt = rwe_devtools_proto::DevtoolsEvent {
                            method: "DOM.inspectNodeRequested".to_string(),
                            params: serde_json::json!({ "nodeId": ptr as u64 }),
                        };
                        let json = serde_json::to_string(&evt).unwrap_or_default();
                        channel.resp_queue.borrow_mut().push_back(json);
                        println!("[shell inspect] click -> emit DOM.inspectNodeRequested nodeId={}", ptr);
                    }
                    // Toggle off + auto-open devtools pokud zavren.
                    self.inspect_mode = false;
                    *self.inspect_target.borrow_mut() = None;
                    if !self.devtools_visible {
                        self.toggle_devtools();
                    }
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
                let event = match state {
                    ElementState::Pressed => InputEvent::MouseDown {
                        x: self.mouse_x, y: self.mouse_y, button: btn,
                        modifiers: KeyModifiers::default(),
                    },
                    ElementState::Released => InputEvent::MouseUp {
                        x: self.mouse_x, y: self.mouse_y, button: btn,
                        modifiers: KeyModifiers::default(),
                    },
                };
                let resp = self.dispatch_input(event);
                // Navigation requests jen z page (devtools click NEnavigates main page).
                if !self.devtools_visible && let Some(nav) = resp.navigation {
                    println!("[shell nav] {:?} {} ({:?})", nav.method, nav.url, nav.target);
                    match nav.method {
                        rwe_engine::embed::NavigationMethod::Get => {
                            // History push pro back/forward.
                            self.history.truncate(self.history_idx + 1);
                            self.history.push(nav.url.clone());
                            self.history_idx = self.history.len() - 1;
                            if let Some(wv) = &mut self.webview { wv.load_url(&nav.url); }
                        }
                        rwe_engine::embed::NavigationMethod::Post => {
                            let body = nav.body.as_ref()
                                .and_then(|b| std::str::from_utf8(b).ok())
                                .unwrap_or_default();
                            if let Some(wv) = &mut self.webview {
                                wv.load_url_post(&nav.url, body);
                            }
                        }
                    }
                }
                if resp.dirty {
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            _ => {
                // Key dispatch do JS = Phase 99 (focused element + keydown event).
            }
        }
    }
}
