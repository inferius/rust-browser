//! ShellApp - winit `ApplicationHandler` ktery vlastni Window + Surface +
//! Renderer + WebView. Renderuje stranku pres `WebView::render_via` do
//! offscreen RT a kompozituje do swap chain pres
//! `Renderer::present_external_to_swap_chain`.
//!
//! Phase 4c step 3 (minimal): bez chrome bar, bez tabs, bez addr/find.
//! Cilem ten cestu validovat - shell crate je nezavislym hostem enginu.
//! Phase 5+ pridava chrome paint a multi-tab.

use std::path::PathBuf;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use rwe_engine::browser::render::Renderer;
use rwe_engine::embed::{Engine, InputEvent, KeyModifiers, MouseButton, WebView};
use rwe_engine::interpreter::{helpers::native, JsValue};

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
    /// True kdyz devtools je viditelne. D4a (MVP) = full-screen toggle
    /// (renderuje se bud page, nebo devtools). D4b real split layout TBD.
    devtools_visible: bool,

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
            mouse_x: 0.0,
            mouse_y: 0.0,
            modifiers: winit::keyboard::ModifiersState::empty(),
            history: Vec::new(),
            history_idx: 0,
        }
    }

    /// D6a: Nainstaluje native CDP funkce do interpreter v devtools WebView.
    ///
    /// `__rwe_cdp_send_native(json_str)`: prijima JSON-stringified
    /// DevtoolsRequest, dispatch'ne pres DevtoolsTarget (D6b - aktualne stub).
    /// Vrati JSON-stringified DevtoolsResponse (nebo "" pokud async).
    ///
    /// `__rwe_cdp_poll_events()`: vrati JSON array bufferovanych events +
    /// pending responses. Volana periodicky z cdp.js setInterval.
    ///
    /// D6a stub: logy a vraci empty strings. D6b implementuje real wire-up
    /// pres Rc<RefCell<CdpChannel>> sdileny mezi shell main loop a natives.
    fn install_cdp_natives(devtools: &mut WebView) {
        let interp = match devtools.interpreter_mut() {
            Some(i) => i,
            None => {
                eprintln!("[cdp] devtools interpreter chybi, natives neinstaluju");
                return;
            }
        };
        // __rwe_cdp_send_native(json_str) -> response_json_str (or "")
        let send_fn = native("__rwe_cdp_send_native", |args| {
            let json = args.first()
                .map(|v| v.to_string())
                .unwrap_or_default();
            println!("[cdp send] {}", json);
            // D6b: parse to DevtoolsRequest, push to req_queue,
            // dispatch ve main loop, push response to resp_queue,
            // poll vrati pozdeji.
            // D6a: return empty string - cdp.js handleResponseJson handle.
            Ok(JsValue::Str(String::new()))
        });
        // __rwe_cdp_poll_events() -> JSON array of events + pending responses
        let poll_fn = native("__rwe_cdp_poll_events", |_args| {
            // D6b: drain resp_queue + event_queue.
            // D6a: empty array.
            Ok(JsValue::Str("[]".into()))
        });
        interp.global.borrow_mut().define("__rwe_cdp_send_native", send_fn);
        interp.global.borrow_mut().define("__rwe_cdp_poll_events", poll_fn);
        println!("[cdp] D6a stub natives installed (send/poll)");
    }

    /// Slozi devtools INDEX_HTML s injectnutymi panel HTMLs + theme.css
    /// + cdp.js. Vlozi do <head> jako `<script id="theme-css">` + cdp.js
    /// + `window.__rwe_panel_html__ = { elements: ..., console: ... }`.
    fn build_devtools_html() -> String {
        use rwe_devtools_frontend::*;
        // JSON-escape kazdy panel HTML pro bezpecne vlozeni do JS string.
        fn js_escape(s: &str) -> String {
            let mut out = String::with_capacity(s.len() + 16);
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '<' => out.push_str("\\u003c"), // </script breaker
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        // INDEX_HTML obsahuje:
        //   <style id="theme-css"></style>
        //   <script id="cdp-js"></script>
        // Nahradime tyto prazdne elementy obsahem.
        let with_theme = INDEX_HTML.replace(
            "<style id=\"theme-css\"></style>",
            &format!("<style id=\"theme-css\">{}</style>", THEME_CSS),
        );
        let panel_map = format!(
            "<script>window.__rwe_panel_html__ = {{\
                elements: {}, console: {}, sources: {}, network: {}, performance: {} }};\
            </script>",
            js_escape(ELEMENTS_HTML),
            js_escape(CONSOLE_HTML),
            js_escape(SOURCES_HTML),
            js_escape(NETWORK_HTML),
            js_escape(PERFORMANCE_HTML),
        );
        let with_cdp = with_theme.replace(
            "<script id=\"cdp-js\"></script>",
            &format!("{}<script id=\"cdp-js\">{}</script>", panel_map, CDP_JS),
        );
        with_cdp
    }

    /// Pristup k aktivnimu WebView (devtools pokud visible, jinak page).
    /// Input events route do nej pres handle_input. Closure forma kvuli
    /// split borrow checker (self.window access po dispatch_input).
    fn with_active_mut<R, F>(&mut self, f: F) -> Option<R>
    where F: FnOnce(&mut WebView) -> R {
        if self.devtools_visible {
            self.devtools.as_mut().map(f)
        } else {
            self.webview.as_mut().map(f)
        }
    }

    fn with_active<R, F>(&self, f: F) -> Option<R>
    where F: FnOnce(&WebView) -> R {
        if self.devtools_visible {
            self.devtools.as_ref().map(f)
        } else {
            self.webview.as_ref().map(f)
        }
    }

    /// Konvenience: dispatch InputEvent na aktivni WebView, vrati response.
    fn dispatch_input(&mut self, event: InputEvent) -> rwe_engine::embed::EventResponse {
        self.with_active_mut(|wv| wv.handle_input(event)).unwrap_or_default()
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
            let _ = dv.load_html(&dv_html, "", None);
            // D6a: install CDP native fns na devtools interpreter PO load_html
            // (po run_scripts). Pri behu cdp.js definuje window.cdp, ale
            // window.cdp.send/pollEvents jeste neexistuje protoze cdp.js
            // resolve nativy az pri callu - takze ted nainstalovane natives
            // budou viditelne kdyz event handler vola send/pollEvents pozdeji.
            Self::install_cdp_natives(&mut dv);
            self.devtools = Some(dv);
            println!("[shell] devtools WebView vytvoreno + INDEX_HTML loaded + CDP natives");
        }
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
        let renderer = match &mut self.renderer { Some(r) => r, None => return };
        // D4a (MVP): full-screen toggle - pri devtools_visible renderuje se
        // devtools WebView, jinak page WebView. Real split layout = D4b
        // (vyzaduje Renderer::present_two_external_split helper).
        let active: &mut WebView = if self.devtools_visible {
            match &mut self.devtools { Some(w) => w, None => return }
        } else {
            match &mut self.webview { Some(w) => w, None => return }
        };
        // WebView vsechno: cascade -> anim tick -> layout -> sticky ->
        // paint anim -> paint -> scroll shift -> scrollbar overlay ->
        // atlas warm -> draw_segments -> RT view.
        if active.render_via(renderer).is_none() { return; }
        // Shell jen present: RT view -> swap chain.
        if let Some(view) = active.target_view() {
            renderer.present_external_to_swap_chain(view);
        }
        let active_anim = active.has_active_animations();
        // Sync window title z page (ne devtools) title.
        if !self.devtools_visible {
            if let (Some(window), Some(wv)) = (&self.window, &self.webview) {
                let t = wv.title();
                if !t.is_empty() {
                    let win_title = format!("{} - RustWebEngine", t);
                    if window.title() != win_title {
                        window.set_title(&win_title);
                    }
                }
            }
        }
        // Continual redraw pri active animations.
        if active_anim {
            if let Some(w) = &self.window { w.request_redraw(); }
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
                if let Some(r) = &self.renderer {
                    let sf = r.scale_factor_value().max(0.01);
                    let (sw, sh) = r.surface_size();
                    let lw = ((sw as f32 / sf) as u32).max(1);
                    let lh = ((sh as f32 / sf) as u32).max(1);
                    if let Some(wv) = &mut self.webview { wv.resize(lw, lh, sf); }
                    if let Some(dv) = &mut self.devtools { dv.resize(lw, lh, sf); }
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(r) = &self.renderer {
                    let (sw, sh) = r.surface_size();
                    let sf = (scale_factor as f32).max(0.01);
                    let lw = ((sw as f32 / sf) as u32).max(1);
                    let lh = ((sh as f32 / sf) as u32).max(1);
                    if let Some(wv) = &mut self.webview { wv.resize(lw, lh, sf); }
                    if let Some(dv) = &mut self.devtools { dv.resize(lw, lh, sf); }
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.renderer.as_ref().map(|r| r.scale_factor_value()).unwrap_or(1.0);
                self.mouse_x = position.x as f32 / scale;
                self.mouse_y = position.y as f32 / scale;
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
                            webview.set_scroll(sx, y.max(0.0));
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
