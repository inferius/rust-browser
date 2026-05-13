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
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use rwe_engine::browser::render::Renderer;
use rwe_engine::embed::{Engine, WebView};

pub struct ShellApp {
    html: String,
    css: String,
    base_url: Option<String>,
    local_path: Option<PathBuf>,

    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    engine: Option<Arc<Engine>>,
    webview: Option<WebView>,
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
        }
    }

    fn redraw(&mut self) {
        let renderer = match &mut self.renderer { Some(r) => r, None => return };
        let webview = match &mut self.webview { Some(w) => w, None => return };
        // Renderuj page do offscreen texture.
        if webview.render_via(renderer).is_none() {
            return;
        }
        // Vezmi target view a kompozituj do swap chain.
        if let Some(view) = webview.target_view() {
            renderer.present_external_to_swap_chain(view);
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
        let mut webview = WebView::new(engine.clone(), sw, sh);
        webview.resize(sw, sh, renderer.scale_factor_value());
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
                    let (sw, sh) = r.surface_size();
                    wv.resize(sw, sh, r.scale_factor_value());
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(r), Some(wv)) = (&self.renderer, &mut self.webview) {
                    let (sw, sh) = r.surface_size();
                    wv.resize(sw, sh, scale_factor as f32);
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {
                // Input/scroll/devtools = Phase 5. Zatim no-op.
            }
        }
    }
}
