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

use super::engine::Engine;
use super::event::{EventResponse, InputEvent, NavigationResult};

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

    /// Offscreen render target texture - vytvori se v `new` (Phase 5).
    /// Phase 2 placeholder `None`.
    pub(crate) target_texture: Option<wgpu::Texture>,
    /// View handle vraceny z `render()`.
    pub(crate) target_view: Option<wgpu::TextureView>,

    /// Dirty flag - `render()` skipne pokud false. Set true pri handle_input
    /// kdyz neco zmenilo viditelne (hover, scroll, JS DOM mutation).
    pub(crate) dirty: bool,
}

impl WebView {
    /// Vytvori prazdny WebView s viewportem dane velikosti. Offscreen RT
    /// alokovan az v Phase 5 - Phase 2 nech `target_texture = None`.
    pub fn new(engine: Arc<Engine>, viewport_w: u32, viewport_h: u32) -> Self {
        Self {
            engine,
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
            target_texture: None,
            target_view: None,
            dirty: true,
        }
    }

    /// Nahraj HTML + CSS string. `base_url` se pouzije pro relative
    /// `<link rel=stylesheet>` a `<img src=...>` resolve.
    ///
    /// Phase 2 stub - Phase 3 sem presune logiku z `App::reload_from_html`.
    pub fn load_html(&mut self, _html: &str, _css: &str, _base_url: Option<String>) -> NavigationResult {
        todo!("Phase 3: presunout App::reload_from_html state setup")
    }

    /// Naviguj na URL. `http(s)://` jde pres ureq, `file://` cte z disku,
    /// `about:blank` -> empty document.
    ///
    /// Phase 2 stub.
    pub fn load_url(&mut self, _url: &str) -> NavigationResult {
        todo!("Phase 3: presunout fetch + load_html pipeline z lib::run_cli")
    }

    /// Zmena velikosti viewportu. Trigger relayout pri pristim `render()`.
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        self.viewport_w = width as f32;
        self.viewport_h = height as f32;
        self.scale_factor = scale_factor;
        self.dirty = true;
        // Phase 5: realokovat target_texture.
    }

    /// Zpracuj input event. Vrati `EventResponse` se zmenami pro hostujici
    /// aplikaci (dirty flag, cursor change, navigation request, ...).
    pub fn handle_input(&mut self, _event: InputEvent) -> EventResponse {
        todo!("Phase 4: presunout input handling z App")
    }

    /// Renderuj page do offscreen texture. Pokud `dirty == false`, vrati
    /// posledni view bez prace.
    pub fn render(&mut self) -> Option<&wgpu::TextureView> {
        todo!("Phase 5: paint -> render passes -> target_view")
    }

    /// Velikost dokumentu (content w / h) pro scrollbar sizing v shellu.
    pub fn page_size(&self) -> (f32, f32) {
        // Phase 5: vrati posledni layout root content extent.
        (self.viewport_w, self.viewport_h)
    }

    /// Nastav scroll position.
    pub fn set_scroll(&mut self, x: f32, y: f32) {
        if (self.scroll_x - x).abs() > 0.5 || (self.scroll_y - y).abs() > 0.5 {
            self.scroll_x = x;
            self.scroll_y = y;
            self.dirty = true;
        }
    }

    /// Aktualni scroll position.
    pub fn scroll(&self) -> (f32, f32) { (self.scroll_x, self.scroll_y) }

    /// Aktualni zoom (1.0 = 100%).
    pub fn zoom(&self) -> f32 { self.zoom }

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

    /// CSS stylesheets v poradi cascade priority.
    pub fn stylesheets(&self) -> &[Stylesheet] { &self.stylesheets }

    /// Engine reference (pro custom rendering hostujici aplikace).
    pub fn engine(&self) -> &Arc<Engine> { &self.engine }
}
