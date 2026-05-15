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
    pub(crate) input_caret: std::collections::HashMap<usize, usize>,
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
    /// Scrollbar drag state - Some(grab_offset_y) pri V thumb drag.
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
    /// Cascade props per node ptr. Vytvoreny po cascade pass. Sdileny do
    /// interpreter cascade_lookup callback - JS getComputedStyle read.
    pub(crate) cascade_props: std::rc::Rc<std::cell::RefCell<
        std::collections::HashMap<usize, std::collections::HashMap<String, String>>
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
            overlay_painter: None,
            v_scrollbar_drag: None,
            h_scrollbar_drag: None,
            last_layout_root: None,
            layout_rects: std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
            cascade_props: std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
            stylesheets_data: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            async_jobs: crate::browser::async_jobs::AsyncJobsRegistry::new(),
        }
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
        result
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
        let doc = crate::browser::html_parser::parse_html(html, &base);

        let stylesheet = crate::browser::css_parser::parse_stylesheet(css);
        let stylesheet_count = if stylesheet.rules.is_empty() { 0 } else { 1 };

        // Init interpreter + set document. Bez run_scripts (volaci kod o ne stoji).
        let mut interp = Interpreter::new();
        let interp_doc = crate::browser::html_parser::parse_html(html, &base);
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
            cascade_clone.borrow().get(&(ptr as usize)).cloned().unwrap_or_default()
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
        self.document = Some(doc);
        self.stylesheets = vec![stylesheet];
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
        let interp = match self.interpreter.as_mut() {
            Some(i) => i,
            None => return,
        };
        let doc_ref = interp.document.clone();
        let base = self.base_url.clone().unwrap_or_default();
        let fetch_external = std::env::var("RWE_NO_SCRIPTS")
            .map(|v| v != "1" && !v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);

        let script_nodes = doc_ref.borrow().root.get_elements_by_tag("script");
        let mut scripts: Vec<(String, String)> = Vec::with_capacity(script_nodes.len());
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

        for (_url, src) in scripts {
            if src.trim().is_empty() { continue; }
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
                // Wheel adjusts smooth scroll target. render_via lerp aktivni
                // scroll_y -> scroll_target_y 25 %% per frame. Clamp na
                // [0, max] kde max = layout_h - viewport_h (z last render).
                let viewport_h = self.viewport_h / self.zoom.max(0.01);
                let viewport_w = self.viewport_w / self.zoom.max(0.01);
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
            InputEvent::MouseMove { x, y, .. } => {
                if (self.mouse_x - x).abs() > 0.5 || (self.mouse_y - y).abs() > 0.5 {
                    self.mouse_x = x;
                    self.mouse_y = y;
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
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    let hovered_id = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.as_ref().map(|n|
                            std::rc::Rc::as_ptr(n) as usize));
                    let prev = crate::browser::cascade::get_hovered_node();
                    if prev != hovered_id {
                        crate::browser::cascade::set_hovered_node(hovered_id);
                        self.dirty = true;
                        response.dirty = true;
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
                    if let Some(layout) = &self.last_layout_root {
                        let total_h = layout.rect.height;
                        let total_w = layout.rect.width;
                        // Vertical thumb hit.
                        if total_h > viewport_h && x >= viewport_w - 12.0 && x < viewport_w {
                            let thumb_h = (viewport_h * viewport_h / total_h).max(40.0);
                            let max_scroll = (total_h - viewport_h).max(1.0);
                            let thumb_y = (self.scroll_y / max_scroll) * (viewport_h - thumb_h);
                            if y >= thumb_y && y < thumb_y + thumb_h {
                                self.v_scrollbar_drag = Some(y - thumb_y);
                                response.dirty = true;
                                self.dirty = true;
                                return response;
                            }
                        }
                        // Horizontal thumb hit.
                        if total_w > viewport_w && y >= viewport_h - 12.0 && y < viewport_h {
                            let thumb_w = (viewport_w * viewport_w / total_w).max(40.0);
                            let max_scroll_x = (total_w - viewport_w).max(1.0);
                            let thumb_x = (self.scroll_x / max_scroll_x) * (viewport_w - thumb_w);
                            if x >= thumb_x && x < thumb_x + thumb_w {
                                self.h_scrollbar_drag = Some(x - thumb_x);
                                response.dirty = true;
                                self.dirty = true;
                                return response;
                            }
                        }
                    }
                    // Hit-test layout_root pres content coords. Store target +
                    // pos pro MouseUp click-vs-drag distinguish.
                    let content_x = x + self.scroll_x;
                    let content_y = y + self.scroll_y;
                    let target_node = self.last_layout_root.as_ref()
                        .and_then(|root| root.hit_test(content_x, content_y))
                        .and_then(|bx| bx.node.clone());
                    // Focus / blur.
                    if let Some(target) = target_node.as_ref() {
                        let focusable = matches!(target.tag_name().as_deref(),
                            Some("input") | Some("textarea") | Some("button")
                            | Some("a") | Some("select"));
                        if focusable {
                            crate::browser::cascade::set_focused_node(Some(
                                std::rc::Rc::as_ptr(target) as usize));
                        } else {
                            crate::browser::cascade::set_focused_node(None);
                        }
                    } else {
                        crate::browser::cascade::set_focused_node(None);
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
                    if let Some(target) = target_node {
                        self.mouse_down_at = Some((x, y, target));
                    }
                    // Begin text selection drag (content coords).
                    self.sel_begin(content_x, content_y);
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
                if crate::browser::cascade::get_hovered_node().is_some() {
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
                        let chars: Vec<char> = cur.chars().collect();
                        let mut caret = *self.input_caret.get(&nid).unwrap_or(&chars.len());
                        caret = caret.min(chars.len());
                        let mut mutated = false;
                        let mut new_chars = chars.clone();
                        match key.as_str() {
                            "Backspace" if caret > 0 => {
                                new_chars.remove(caret - 1);
                                caret -= 1;
                                mutated = true;
                            }
                            "Delete" if caret < new_chars.len() => {
                                new_chars.remove(caret);
                                mutated = true;
                            }
                            "ArrowLeft" => {
                                caret = caret.saturating_sub(1);
                                self.input_caret.insert(nid, caret);
                                response.dirty = true;
                                self.dirty = true;
                            }
                            "ArrowRight" => {
                                caret = (caret + 1).min(new_chars.len());
                                self.input_caret.insert(nid, caret);
                                response.dirty = true;
                                self.dirty = true;
                            }
                            "Home" => {
                                self.input_caret.insert(nid, 0);
                                response.dirty = true;
                                self.dirty = true;
                            }
                            "End" => {
                                self.input_caret.insert(nid, new_chars.len());
                                response.dirty = true;
                                self.dirty = true;
                            }
                            _ => {}
                        }
                        if mutated {
                            let new_value: String = new_chars.into_iter().collect();
                            target.set_attr("value", &new_value);
                            self.input_caret.insert(nid, caret);
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
                // Pri focused <input>/<textarea> insert text na caret pos +
                // dispatch "input" event. Caret advance o N graphemes.
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
                        let mut chars: Vec<char> = cur.chars().collect();
                        let caret = (*self.input_caret.get(&nid).unwrap_or(&chars.len()))
                            .min(chars.len());
                        let ins_chars: Vec<char> = printable.chars().collect();
                        let ins_n = ins_chars.len();
                        for (i, ch) in ins_chars.into_iter().enumerate() {
                            chars.insert(caret + i, ch);
                        }
                        let new_value: String = chars.into_iter().collect();
                        target.set_attr("value", &new_value);
                        self.input_caret.insert(nid, caret + ins_n);
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
        // Sync scroll_pos od interpreteru (JS window.scrollTo zapsal). Pri
        // zmene apply do self.scroll_x/y + scroll_target. Po render zpetne
        // sync scroll_pos = current scroll.
        if let Some(interp) = self.interpreter.as_ref() {
            let (jx, jy) = *interp.scroll_pos.borrow();
            if (jx - self.scroll_x).abs() > 0.5 || (jy - self.scroll_y).abs() > 0.5 {
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
        // hodnotu, ne jen JS-set hodnotu).
        if let Some(interp) = self.interpreter.as_ref() {
            *interp.scroll_pos.borrow_mut() = (self.scroll_x, self.scroll_y);
        }

        // Drain async jobs (image lazy loads, file IO callbacks). Volane PRED
        // cascade aby novy state byl dostupny v style_map (e.g. image natural
        // dims po load aktualizuji layout).
        self.async_jobs.drain();

        // Drain interpreter event queues (WebSocket frames, fetch responses,
        // requestAnimationFrame callbacks). Vola se kdyz interpreter existuje
        // (Po polarity invert WebView vlastni interpreter; drive App).
        if let Some(interp) = self.interpreter.as_mut() {
            let _ = interp.drain_websockets();
            interp.drain_fetches();
            let ts_ms = self.animation_origin.elapsed().as_secs_f64() * 1000.0;
            let _ = interp.drain_raf_callbacks(ts_ms);
        }

        let target_view = self.target_view.as_ref()?;
        let doc = self.document.as_ref()?;

        // Layout viewport = logical CSS px / browser zoom. viewport_w/h jsou
        // uz LOGICAL (host predava surface_size / scale_factor).
        let viewport_w = self.viewport_w / self.zoom.max(0.01);
        let viewport_h = self.viewport_h / self.zoom.max(0.01);

        // 1. Cascade - resolve CSS styles per element. Wrap v Rc aby
        // apply_animations / apply_transitions mohly Rc::make_mut mutate
        // (pri animaci tick anim values overlay puvodne resolved styles).
        let mut style_map = std::rc::Rc::new(crate::browser::cascade::cascade_with_viewport(
            &doc.root, &self.stylesheets, viewport_w, viewport_h));

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

        // 2. Layout - compute boxes (po anim tick aby left/top/width keyframes
        // ovlivnili layout pozice).
        let mut layout_root = crate::browser::layout::layout_tree(
            &doc.root, &style_map, viewport_w, viewport_h);

        // 2b. Sticky positioning post-process - position:sticky elementy
        // posunuju dle scroll_y aby drzeli na top viewportu uvnitr containeru.
        crate::browser::layout::apply_sticky(&mut layout_root, self.scroll_y);

        // 2c. Paint-side animations apply (transform overlay, opacity tween).
        crate::browser::render::apply_paint_animations(&mut layout_root, &style_map);

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
                    let prefix: String = chars[..caret].iter().collect();
                    let prefix_w = crate::browser::layout::measure_text_width_full(
                        &prefix, input_box.font_size, weight, input_box.italic,
                        &input_box.font_family, input_box.letter_spacing);
                    // Pad left ~6px (CSS input default), caret y od inner top.
                    let pad_l = 6.0_f32;
                    let pad_t = 4.0_f32;
                    let caret_x = input_box.rect.x + pad_l + prefix_w;
                    let caret_y = input_box.rect.y + pad_t;
                    let caret_h = input_box.font_size * 1.2;
                    // Blink 1 Hz: even seconds visible, odd off.
                    let elapsed = self.animation_origin.elapsed().as_secs_f32();
                    let blink_on = (elapsed * 2.0) as i32 % 2 == 0;
                    if blink_on {
                        display_list.push(crate::browser::paint::DisplayCommand::Rect {
                            x: caret_x, y: caret_y,
                            w: 1.5, h: caret_h,
                            color: [40, 40, 50, 255], radius: 0.0,
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
        let total_h = layout_root.rect.height;
        if total_h > viewport_h {
            let bar_w = 12.0_f32;
            let bar_x = viewport_w - bar_w;
            display_list.push(crate::browser::paint::DisplayCommand::Rect {
                x: bar_x, y: 0.0, w: bar_w, h: viewport_h,
                color: [240, 240, 245, 255], radius: 0.0,
            });
            let thumb_h = (viewport_h * viewport_h / total_h).max(40.0);
            let max_scroll = (total_h - viewport_h).max(1.0);
            let thumb_y = (self.scroll_y / max_scroll) * (viewport_h - thumb_h);
            display_list.push(crate::browser::paint::DisplayCommand::Rect {
                x: bar_x + 2.0, y: thumb_y + 2.0,
                w: bar_w - 4.0, h: thumb_h - 4.0,
                color: [160, 160, 170, 255], radius: (bar_w - 4.0) * 0.5,
            });
        }
        let total_w = layout_root.rect.width;
        if total_w > viewport_w {
            let bar_h = 12.0_f32;
            let bar_y = viewport_h - bar_h;
            display_list.push(crate::browser::paint::DisplayCommand::Rect {
                x: 0.0, y: bar_y, w: viewport_w, h: bar_h,
                color: [240, 240, 245, 255], radius: 0.0,
            });
            let thumb_w = (viewport_w * viewport_w / total_w).max(40.0);
            let max_scroll_x = (total_w - viewport_w).max(1.0);
            let thumb_x = (self.scroll_x / max_scroll_x) * (viewport_w - thumb_w);
            display_list.push(crate::browser::paint::DisplayCommand::Rect {
                x: thumb_x + 2.0, y: bar_y + 2.0,
                w: thumb_w - 4.0, h: bar_h - 4.0,
                color: [160, 160, 170, 255], radius: (bar_h - 4.0) * 0.5,
            });
        }

        // 4. Warm-up glyph atlas + image atlas pred draw.
        renderer.warm_atlas_for(&display_list, self.base_url.as_deref());

        // 4b. Extract text runs (per-glyph cumulative advances) - foundation
        // pro per-glyph hit-test selection. Walks display_list TEXT cmds +
        // measure pres atlas. Page cmds only (overlay text neselectable).
        self.painted_text_runs = crate::browser::render::extract_text_runs(
            &display_list, renderer.atlas(), renderer.zoom);

        // 5. Renderer kresli display list do target_view.
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
            let mut props = self.cascade_props.borrow_mut();
            props.clear();
            for (ptr, style) in style_map.iter() {
                props.insert(*ptr, style.clone());
            }
        }
        self.last_layout_root = Some(layout_root);

        // Reset renderer target_size override - shell present_split + jine
        // pas v swap chain pouziva config size.
        renderer.target_size = None;

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
        self.stylesheets.iter().any(|s| !s.keyframes.is_empty())
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

    /// Aktualne focused DOM node (z cascade thread_local) - pro keyboard
    /// event dispatch. Some pri focused <input>/<textarea>/<a>/<button>.
    fn focused_dom_node(&self) -> Option<std::rc::Rc<crate::browser::dom::Node>> {
        let id = crate::browser::cascade::get_focused_node()?;
        let interp = self.interpreter.as_ref()?;
        let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
        crate::browser::render::find_node_by_ptr(&doc_root, id)
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
        let bx0 = b.rect.x;
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
                let line_w = line.chars().map(|ch|
                    crate::browser::layout::measure_text_width_full(
                        &ch.to_string(), b.font_size, weight, italic, &fam, ls)).sum::<f32>();
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
                let mut acc = 0.0_f32;
                let mut hl_start: Option<f32> = None;
                let mut hl_end: f32 = line_w;
                for ch in line.chars() {
                    let adv = crate::browser::layout::measure_text_width_full(
                        &ch.to_string(), b.font_size, weight, italic, &fam, ls);
                    let mid = acc + adv * 0.5;
                    if hl_start.is_none() && mid >= sel_left {
                        hl_start = Some(acc);
                    }
                    if mid > sel_right {
                        hl_end = acc;
                        break;
                    }
                    acc += adv;
                }
                let hs = hl_start.unwrap_or(0.0);
                if hl_end > hs + 0.5 {
                    out.push((line_start_x + hs, line_y, hl_end - hs, lh));
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
}
