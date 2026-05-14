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
                    let _ = wv.handle_input(InputEvent::MouseMove {
                        x: self.mouse_x,
                        y: self.mouse_y,
                        modifiers: KeyModifiers::default(),
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * -60.0, y * -60.0),
                    MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
                };
                let webview = match &mut self.webview { Some(w) => w, None => return };
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
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                use winit::keyboard::{Key, NamedKey};
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
                    println!("[shell nav] {} ({:?})", nav.url, nav.target);
                    webview.load_url(&nav.url);
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
