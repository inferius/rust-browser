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

/// Find smallest LayoutBox containing (x, y). DFS prefer descendant
/// (deepest node ktery obsahuje point). Used pres inspect_mode hover
/// hit-test.
fn pick_node_at(
    root: &rwe_engine::browser::layout::LayoutBox,
    x: f32, y: f32,
) -> Option<usize> {
    // Hit-test self?
    let r = &root.rect;
    let in_self = x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
    if !in_self { return None; }
    // Try children first - prefer deepest.
    for child in &root.children {
        if let Some(p) = pick_node_at(child, x, y) {
            return Some(p);
        }
    }
    // Otherwise return self (if node exists).
    root.node.as_ref().map(|n| std::rc::Rc::as_ptr(n) as usize)
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
    inspect_target: std::rc::Rc<std::cell::RefCell<Option<usize>>>,
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

    mouse_x: f32,
    mouse_y: f32,
    modifiers: winit::keyboard::ModifiersState,
    history: Vec<String>,
    history_idx: usize,
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
            devtools: None,
            devtools_visible: false,
            devtools_split_ratio: 0.4,
            splitter_drag: false,
            inspect_mode: false,
            inspect_target: Rc::new(RefCell::new(None)),
            addr_open: false,
            addr_input: String::new(),
            find_open: false,
            find_query: String::new(),
            devtools_target: None,
            cdp_channel: None,
            cdp_network_log_idx: 0,
            cdp_console_log_idx: 0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            modifiers: winit::keyboard::ModifiersState::empty(),
            history: Vec::new(),
            history_idx: 0,
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
                Err(e) => eprintln!("[cdp send] parse err: {} (json: {})", e, json),
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
            let resp = target.handle_request(page, req);
            let json = serde_json::to_string(&resp)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialize: {e}\"}}"));
            println!("[cdp dispatch] id={} method={} resp_len={}",
                req_id, req_method, json.len());
            channel.resp_queue.borrow_mut().push_back(json);
        }
        // Drain pending events (z target.events) - push do resp_queue.
        let events = target.take_events();
        for evt in events {
            let json = serde_json::to_string(&evt)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialize: {e}\"}}"));
            channel.resp_queue.borrow_mut().push_back(json);
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
            // Diff console_log -> emit Runtime.consoleAPICalled events.
            for (level, msg) in console_log.iter().skip(self.cdp_console_log_idx) {
                let evt = rwe_devtools_proto::DevtoolsEvent {
                    method: "Runtime.consoleAPICalled".to_string(),
                    params: serde_json::json!({
                        "type": level,
                        "args": [{ "type": "string", "value": msg, "description": msg }],
                        "timestamp": now_ts,
                    }),
                };
                let json = serde_json::to_string(&evt).unwrap_or_default();
                channel.resp_queue.borrow_mut().push_back(json);
            }
            self.cdp_console_log_idx = console_log.len();
        }
    }

    /// Slozi devtools HTML: INDEX_HTML s nahrazenymi placeholdery na
    /// theme.css + cdp.js + per-panel HTML injectnuty primo do <div id="panel-*">.
    /// Tab swap pres display style (ne innerHTML swap) - vsechny panely
    /// v DOM od zacatku, event listeners aktivni.
    fn build_devtools_html() -> String {
        use rwe_devtools_frontend::*;
        let mut out = INDEX_HTML.to_string();
        out = out.replace(
            "<style id=\"theme-css\"></style>",
            &format!("<style id=\"theme-css\">{}</style>", THEME_CSS),
        );
        out = out.replace(
            "<script id=\"cdp-js\"></script>",
            &format!("<script id=\"cdp-js\">{}</script>", CDP_JS),
        );
        // Inject kazdy panel HTML do prislusneho <div id="panel-X"></div>.
        // Naivni String::replace - matchne prvni vyskyt (jednou per panel).
        for (id, body) in [
            ("panel-elements", ELEMENTS_HTML),
            ("panel-console", CONSOLE_HTML),
            ("panel-sources", SOURCES_HTML),
            ("panel-network", NETWORK_HTML),
            ("panel-performance", PERFORMANCE_HTML),
        ] {
            let open = format!("<div id=\"{}\" class=\"panel\"></div>", id);
            let open_hidden = format!("<div id=\"{}\" class=\"panel\" style=\"display:none\"></div>", id);
            let filled = format!("<div id=\"{}\" class=\"panel\">{}</div>", id, body);
            let filled_hidden = format!("<div id=\"{}\" class=\"panel\" style=\"display:none\">{}</div>", id, body);
            out = out.replace(&open, &filled);
            out = out.replace(&open_hidden, &filled_hidden);
        }
        out
    }

    /// True kdyz mouse_y je v zone +- 4px okolo split line. Aktivni hover
    /// zone pro splitter drag.
    fn point_on_splitter(&self, y: f32) -> bool {
        if !self.devtools_visible { return false; }
        let split_y = self.devtools_y_offset();
        (y - split_y).abs() < 4.0
    }

    /// True kdyz devtools je viditelne A mouse_y je v devtools area (bottom).
    /// Pri devtools_visible=false vraci false vzdy (page-only).
    fn point_in_devtools(&self, y: f32) -> bool {
        if !self.devtools_visible { return false; }
        let r = match &self.renderer { Some(r) => r, None => return false };
        let sf = r.scale_factor_value().max(0.01);
        let (_sw, sh) = r.surface_size();
        let lh_full = (sh as f32 / sf).max(1.0);
        let split = self.devtools_split_ratio.clamp(0.05, 0.95);
        let page_h = lh_full * (1.0 - split);
        y >= page_h
    }

    /// Y offset pro mouse_y do devtools WebView local coords (subtract page_h).
    fn devtools_y_offset(&self) -> f32 {
        let r = match &self.renderer { Some(r) => r, None => return 0.0 };
        let sf = r.scale_factor_value().max(0.01);
        let (_sw, sh) = r.surface_size();
        let lh_full = (sh as f32 / sf).max(1.0);
        let split = self.devtools_split_ratio.clamp(0.05, 0.95);
        lh_full * (1.0 - split)
    }

    /// Pristup k aktivnimu WebView (D4c: dle mouse_y position pokud
    /// devtools_visible, jinak page). Pro keyboard fallback je devtools
    /// kdyz visible (mouse-area-independent decision).
    fn with_active_mut<R, F>(&mut self, f: F) -> Option<R>
    where F: FnOnce(&mut WebView) -> R {
        if self.point_in_devtools(self.mouse_y) {
            self.devtools.as_mut().map(f)
        } else {
            self.webview.as_mut().map(f)
        }
    }

    fn with_active<R, F>(&self, f: F) -> Option<R>
    where F: FnOnce(&WebView) -> R {
        if self.point_in_devtools(self.mouse_y) {
            self.devtools.as_ref().map(f)
        } else {
            self.webview.as_ref().map(f)
        }
    }

    /// Konvenience: dispatch InputEvent na aktivni WebView dle y position.
    /// Mouse events maji x/y - dle y rozhoduje. Pred dispatch event upravi
    /// y na local-to-pane (subtract page_h pokud devtools).
    fn dispatch_input(&mut self, event: InputEvent) -> rwe_engine::embed::EventResponse {
        let in_dev = self.point_in_devtools(self.mouse_y);
        let y_off = if in_dev { self.devtools_y_offset() } else { 0.0 };
        // Adjust y koord v event aby webview videl coordy ve sve local space.
        let adjusted = match event {
            InputEvent::MouseMove { x, y, modifiers } =>
                InputEvent::MouseMove { x, y: y - y_off, modifiers },
            InputEvent::MouseDown { x, y, button, modifiers } =>
                InputEvent::MouseDown { x, y: y - y_off, button, modifiers },
            InputEvent::MouseUp { x, y, button, modifiers } =>
                InputEvent::MouseUp { x, y: y - y_off, button, modifiers },
            InputEvent::Scroll { dx, dy, x, y, modifiers } =>
                InputEvent::Scroll { dx, dy, x, y: y - y_off, modifiers },
            other => other,
        };
        if in_dev {
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
        if self.devtools_visible && self.devtools.is_none() {
            let engine = match &self.engine { Some(e) => e.clone(), None => return };
            let renderer = match &self.renderer { Some(r) => r, None => return };
            let (sw, sh) = renderer.surface_size();
            let sf = renderer.scale_factor_value().max(0.01);
            let lw = ((sw as f32 / sf) as u32).max(1);
            let lh = ((sh as f32 / sf) as u32).max(1);
            let mut dv = WebView::new(engine, lw, lh);
            dv.resize(lw, lh, sf);
            let dv_html = Self::build_devtools_html();
            // load_html bere html + css separate. Inline <style> tagy v <head>
            // by se ignorovaly (loader::extract_inline_styles bezi jen pres
            // load_url cestu). Extract rucne a predat jako css parametr.
            let inline_css = Self::extract_inline_styles(&dv_html);
            let _ = dv.load_html(&dv_html, &inline_css, None);
            // D6b: setup channel + target. Native fns install po load_html
            // (cdp.js definovany pred volanim native). Pumpa probiha v
            // redraw - drain queue + dispatch pres target + page WebView.
            let channel = CdpChannel::new();
            Self::install_cdp_natives(&mut dv, &channel);
            let html_len = Self::build_devtools_html().len();
            let css_len = Self::extract_inline_styles(&Self::build_devtools_html()).len();
            println!("[shell] devtools WebView armed: html={} bytes, inline css={} bytes",
                html_len, css_len);
            self.devtools = Some(dv);
            self.devtools_target = Some(DevtoolsTarget::new());
            self.cdp_channel = Some(channel);
            println!("[shell] devtools CDP channel ready");
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
            if let Some(w) = &self.window { w.request_redraw(); }
        }
    }

    fn nav_forward(&mut self) {
        if self.history_idx + 1 >= self.history.len() { return; }
        self.history_idx += 1;
        let url = self.history[self.history_idx].clone();
        if let Some(wv) = &mut self.webview {
            wv.load_url(&url);
            if let Some(w) = &self.window { w.request_redraw(); }
        }
    }

    fn redraw(&mut self) {
        // CDP pump pred render - drain pending requests + push responses.
        if self.devtools_visible {
            self.pump_cdp();
        }
        let renderer = match &mut self.renderer { Some(r) => r, None => return };

        if !self.devtools_visible {
            // Page-only fullscreen.
            let webview = match &mut self.webview { Some(w) => w, None => return };
            if webview.render_via(renderer).is_none() { return; }
            if let Some(view) = webview.target_view() {
                renderer.present_external_to_swap_chain(view);
            }
            let active_anim = webview.has_active_animations();
            if let (Some(window), Some(wv)) = (&self.window, &self.webview) {
                let t = wv.title();
                if !t.is_empty() {
                    let win_title = format!("{} - RustWebEngine", t);
                    if window.title() != win_title {
                        window.set_title(&win_title);
                    }
                }
            }
            if active_anim {
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            return;
        }

        // D4b split layout: page top, devtools bottom. Oba viewporty dostanou
        // sve vlasne velikosti pres resize call (toggle/Resized handler).
        // Render kazdy do offscreen RT, pak present_split_external compose.
        let split = self.devtools_split_ratio.clamp(0.05, 0.95);
        let page_ratio = 1.0 - split;
        // `renderer` lokal je &mut z line above. Render kazde do offscreen
        // RT s field-disjoint borrows do self.webview / self.devtools.
        let page_anim = self.webview.as_mut()
            .map(|wv| { let _ = wv.render_via(renderer); wv.has_active_animations() })
            .unwrap_or(false);
        let dev_anim = self.devtools.as_mut()
            .map(|wv| { let _ = wv.render_via(renderer); wv.has_active_animations() })
            .unwrap_or(false);
        // Drop &mut renderer, znova borrowuj jako &renderer pro present_split.
        // Field-disjoint immut refs do webview/devtools/renderer = OK.
        let _ = renderer;
        if let (Some(r), Some(page_v), Some(dev_v)) = (
            self.renderer.as_ref(),
            self.webview.as_ref().and_then(|w| w.target_view()),
            self.devtools.as_ref().and_then(|w| w.target_view()),
        ) {
            r.present_split_external_to_swap_chain(page_v, dev_v, page_ratio);
        }
        if page_anim || dev_anim {
            if let Some(w) = &self.window { w.request_redraw(); }
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
        if self.devtools_visible {
            let split = self.devtools_split_ratio.clamp(0.05, 0.95);
            let dev_h = ((lh_full as f32) * split).round().max(1.0) as u32;
            let page_h = (lh_full - dev_h).max(1);
            if let Some(wv) = &mut self.webview { wv.resize(lw, page_h, sf); }
            if let Some(dv) = &mut self.devtools { dv.resize(lw, dev_h, sf); }
        } else {
            if let Some(wv) = &mut self.webview { wv.resize(lw, lh_full, sf); }
            if let Some(dv) = &mut self.devtools { dv.resize(lw, lh_full, sf); }
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
        // History init s initial URL (pro Alt+Left/Right back/forward).
        if let Some(url) = &self.base_url {
            self.history.push(url.clone());
            self.history_idx = 0;
        }

        self.window = Some(window.clone());
        self.renderer = Some(renderer);
        self.engine = Some(engine);
        self.webview = Some(webview);

        println!("[shell] vlastni okno + WebView render path (no chrome v Phase 4c)");
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize_surface(size.width, size.height);
                }
                self.resize_views();
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.resize_views();
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.renderer.as_ref().map(|r| r.scale_factor_value()).unwrap_or(1.0);
                self.mouse_x = position.x as f32 / scale;
                self.mouse_y = position.y as f32 / scale;
                // D5: pri inspect_mode + mouse v page area, hit-test layout
                // a updatuj inspect_target. Overlay painter prekresli.
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
                if self.point_on_splitter(self.mouse_y) {
                    if let Some(window) = &self.window {
                        window.set_cursor(winit::window::CursorIcon::NsResize);
                    }
                    return;
                }
                let event = InputEvent::MouseMove {
                    x: self.mouse_x,
                    y: self.mouse_y,
                    modifiers: KeyModifiers::default(),
                };
                let resp = self.dispatch_input(event);
                if let (Some(cursor), Some(window)) = (resp.cursor, &self.window) {
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
                if resp.dirty {
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * -60.0, y * -60.0),
                    MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
                };
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
                let event = InputEvent::Scroll {
                    dx, dy,
                    x: self.mouse_x,
                    y: self.mouse_y,
                    modifiers: KeyModifiers::default(),
                };
                let resp = self.dispatch_input(event);
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
                            // Ctrl+A: select all v aktivnim WebView.
                            self.with_active_mut(|wv| wv.select_all());
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
                if matches!(key_event.state, ElementState::Pressed) {
                    let shift = self.modifiers.shift_key();
                    let ctrl = self.modifiers.control_key();
                    let key_logical = key_event.logical_key.clone();
                    let new_y = self.with_active_mut(|webview| {
                        let (_vw, vh) = webview.viewport_size();
                        let (sx, sy) = webview.scroll();
                        let ny = match &key_logical {
                            Key::Named(NamedKey::PageDown) => Some(sy + vh * 0.9),
                            Key::Named(NamedKey::PageUp) => Some(sy - vh * 0.9),
                            Key::Named(NamedKey::ArrowDown) if !ctrl => Some(sy + 60.0),
                            Key::Named(NamedKey::ArrowUp) if !ctrl => Some(sy - 60.0),
                            Key::Named(NamedKey::Home) => Some(0.0),
                            Key::Named(NamedKey::End) => Some(1_000_000.0),
                            Key::Named(NamedKey::Space) if !webview.focused_is_input() => {
                                let delta = if shift { -vh * 0.9 } else { vh * 0.9 };
                                Some(sy + delta)
                            }
                            _ => None,
                        };
                        if let Some(y) = ny {
                            // Clamp na [0, max] kde max = layout_h - viewport_h.
                            let max_y = webview.last_layout_root()
                                .map(|l| (l.rect.height - vh).max(0.0))
                                .unwrap_or(f32::INFINITY);
                            webview.set_scroll(sx, y.clamp(0.0, max_y));
                            true
                        } else { false }
                    }).unwrap_or(false);
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
                    Key::Named(NamedKey::Space) => " ".into(),
                    Key::Character(s) => s.to_string(),
                    _ => return,
                };
                if matches!(key_event.state, ElementState::Pressed) {
                    let resp = self.dispatch_input(InputEvent::KeyDown {
                        key: key_str.clone(),
                        modifiers: KeyModifiers::default(),
                    });
                    if resp.dirty {
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }
                    // Character keys taky emit TextInput.
                    if let Key::Character(s) = &key_event.logical_key {
                        let resp = self.dispatch_input(InputEvent::TextInput {
                            text: s.to_string(),
                        });
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
                        ElementState::Pressed if self.point_on_splitter(self.mouse_y) => {
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
