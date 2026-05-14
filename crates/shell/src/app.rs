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

pub struct ShellApp {
    html: String,
    css: String,
    base_url: Option<String>,
    local_path: Option<PathBuf>,

    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    engine: Option<Arc<Engine>>,
    webview: Option<WebView>,

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
            mouse_x: 0.0,
            mouse_y: 0.0,
            modifiers: winit::keyboard::ModifiersState::empty(),
            history: Vec::new(),
            history_idx: 0,
        }
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
        let webview = match &mut self.webview { Some(w) => w, None => return };
        // WebView vsechno: cascade -> anim tick -> layout -> sticky ->
        // paint anim -> paint -> scroll shift -> scrollbar overlay ->
        // atlas warm -> draw_segments -> RT view.
        if webview.render_via(renderer).is_none() { return; }
        // Shell jen present: RT view -> swap chain.
        if let Some(view) = webview.target_view() {
            renderer.present_external_to_swap_chain(view);
        }
        // Sync window title z page title.
        if let Some(window) = &self.window {
            let t = webview.title();
            if !t.is_empty() {
                let win_title = format!("{} - RustWebEngine", t);
                if window.title() != win_title {
                    window.set_title(&win_title);
                }
            }
        }
        // Pokud stranka ma aktivni animace, request_redraw na pristi frame.
        // Bez tohoto by anim "zamrzla" po prvnim renderu (RedrawRequested je
        // event-driven, ne continual).
        if webview.has_active_animations() {
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
                if let (Some(r), Some(wv)) = (&self.renderer, &mut self.webview) {
                    let sf = r.scale_factor_value().max(0.01);
                    let (sw, sh) = r.surface_size();
                    let lw = ((sw as f32 / sf) as u32).max(1);
                    let lh = ((sh as f32 / sf) as u32).max(1);
                    wv.resize(lw, lh, sf);
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(r), Some(wv)) = (&self.renderer, &mut self.webview) {
                    let (sw, sh) = r.surface_size();
                    let sf = (scale_factor as f32).max(0.01);
                    let lw = ((sw as f32 / sf) as u32).max(1);
                    let lh = ((sh as f32 / sf) as u32).max(1);
                    wv.resize(lw, lh, sf);
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
                if let Some(wv) = &mut self.webview {
                    let resp = wv.handle_input(InputEvent::MouseMove {
                        x: self.mouse_x,
                        y: self.mouse_y,
                        modifiers: KeyModifiers::default(),
                    });
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
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * -60.0, y * -60.0),
                    MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
                };
                let webview = match &mut self.webview { Some(w) => w, None => return };
                // Ctrl+Wheel = zoom in/out (common browser pattern).
                if self.modifiers.control_key() {
                    let z = webview.zoom();
                    let new_zoom = if dy < 0.0 {
                        (z * 1.1).min(5.0)
                    } else {
                        (z / 1.1).max(0.25)
                    };
                    webview.set_zoom(new_zoom);
                    println!("[shell zoom] {:.0}%", new_zoom * 100.0);
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
                let response = webview.handle_input(InputEvent::Scroll {
                    dx, dy,
                    x: self.mouse_x,
                    y: self.mouse_y,
                    modifiers: KeyModifiers::default(),
                });
                if response.dirty {
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
                            if let Some(wv) = &self.webview {
                                if let Some(text) = wv.selection_text() {
                                    if !text.is_empty() {
                                        if let Ok(mut cb) = arboard::Clipboard::new() {
                                            let _ = cb.set_text(text);
                                            println!("[shell] copy: selection -> clipboard");
                                        }
                                    }
                                }
                            }
                            return;
                        }
                        if s.eq_ignore_ascii_case("a") {
                            // Ctrl+A: select all
                            if let Some(wv) = &mut self.webview {
                                wv.select_all();
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                            return;
                        }
                        // Ctrl+= / Ctrl++ / Ctrl+- / Ctrl+0 zoom controls.
                        if s.eq_ignore_ascii_case("r") {
                            // Ctrl+R: reload current page.
                            if let (Some(wv), Some(last)) = (&mut self.webview, self.history.get(self.history_idx).cloned()) {
                                wv.load_url(&last);
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                            return;
                        }
                        if matches!(s.as_str(), "+" | "=" | "-" | "_" | "0") {
                            if let Some(wv) = &mut self.webview {
                                let z = wv.zoom();
                                let new_zoom = match s.as_str() {
                                    "+" | "=" => (z * 1.1).min(5.0),
                                    "-" | "_" => (z / 1.1).max(0.25),
                                    "0" => 1.0,
                                    _ => z,
                                };
                                wv.set_zoom(new_zoom);
                                println!("[shell zoom] {:.0}%", new_zoom * 100.0);
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
                }
                // Scroll keys: PageDown/Up, ArrowUp/Down, Home, End, Space.
                if matches!(key_event.state, ElementState::Pressed) {
                    let webview = match &mut self.webview { Some(w) => w, None => return };
                    let (_vw, vh) = webview.viewport_size();
                    let (sx, sy) = webview.scroll();
                    let new_y = match &key_event.logical_key {
                        Key::Named(NamedKey::PageDown) => Some(sy + vh * 0.9),
                        Key::Named(NamedKey::PageUp) => Some(sy - vh * 0.9),
                        Key::Named(NamedKey::ArrowDown) if !self.modifiers.control_key() => Some(sy + 60.0),
                        Key::Named(NamedKey::ArrowUp) if !self.modifiers.control_key() => Some(sy - 60.0),
                        Key::Named(NamedKey::Home) => Some(0.0),
                        Key::Named(NamedKey::End) => Some(1_000_000.0),
                        Key::Named(NamedKey::Space) if !webview.focused_is_input() => {
                            let delta = if self.modifiers.shift_key() { -vh * 0.9 } else { vh * 0.9 };
                            Some(sy + delta)
                        }
                        _ => None,
                    };
                    if let Some(ny) = new_y {
                        webview.set_scroll(sx, ny.max(0.0));
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
                let webview = match &mut self.webview { Some(w) => w, None => return };
                let event = if matches!(key_event.state, ElementState::Pressed) {
                    let resp = webview.handle_input(InputEvent::KeyDown {
                        key: key_str.clone(),
                        modifiers: KeyModifiers::default(),
                    });
                    if resp.dirty {
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }
                    // Character keys taky emit TextInput.
                    if let Key::Character(s) = &key_event.logical_key {
                        let resp = webview.handle_input(InputEvent::TextInput {
                            text: s.to_string(),
                        });
                        if resp.dirty {
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                    }
                    InputEvent::KeyDown { key: key_str, modifiers: KeyModifiers::default() }
                } else {
                    InputEvent::KeyUp { key: key_str, modifiers: KeyModifiers::default() }
                };
                let _ = event; // dispatched outside if pressed
                if matches!(key_event.state, ElementState::Released) {
                    let key_str_release = match &key_event.logical_key {
                        Key::Character(s) => s.to_string(),
                        Key::Named(NamedKey::Enter) => "Enter".into(),
                        _ => return,
                    };
                    webview.handle_input(InputEvent::KeyUp {
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
                let webview = match &mut self.webview { Some(w) => w, None => return };
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
                let resp = webview.handle_input(event);
                if let Some(nav) = resp.navigation {
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
