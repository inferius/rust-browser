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
    /// Last layout_root vyrobeny v render_via - getter pro hostujici aplikaci
    /// (App emits inspector overlay nad webview RT pres dalsi draw_segments
    /// pass; shell nepouziva).
    pub(crate) last_layout_root: Option<crate::browser::layout::LayoutBox>,
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
            target_texture: None,
            target_view: None,
            dirty: true,
            animation_origin: std::time::Instant::now(),
            prev_style_map: None,
            active_transitions: Vec::new(),
            last_layout_root: None,
            async_jobs: crate::browser::async_jobs::AsyncJobsRegistry::new(),
        }
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
        let interp = Interpreter::new();
        let interp_doc = crate::browser::html_parser::parse_html(html, &base);
        interp.set_document(interp_doc);

        self.title = doc.title.clone();
        self.document = Some(doc);
        self.stylesheets = vec![stylesheet];
        self.base_url = base_url.clone();
        self.interpreter = Some(interp);
        self.dirty = true;
        // Animation origin reset - fresh stranka start = anim elapsed 0.
        self.animation_origin = std::time::Instant::now();
        // Transitions detect drive z minulych frames -> nova stranka clean state.
        self.prev_style_map = None;

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
                let (x, y) = self.scroll();
                let new_x = (x + dx).max(0.0);
                let new_y = (y + dy).max(0.0);
                self.set_scroll(new_x, new_y);
                response.dirty = self.dirty;
            }
            InputEvent::MouseMove { x: _, y: _, .. } => {
                // Phase 99: hit-test layout tree + :hover state machine.
                // Pro ted no-op (dirty zustane).
            }
            InputEvent::MouseDown { .. } | InputEvent::MouseUp { .. } => {
                // Phase 99: hit-test + dispatch click events do JS listeneru.
            }
            InputEvent::MouseLeave => {}
            InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. } | InputEvent::TextInput { .. } => {
                // Phase 99: keyboard event dispatch + focused element input.
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
        if css_uses_transitions {
            if let Some(prev) = &self.prev_style_map {
                let same_map = std::rc::Rc::ptr_eq(prev, &style_map);
                if !same_map {
                    let active_before = std::mem::take(&mut self.active_transitions);
                    self.active_transitions = crate::browser::cascade::detect_transitions(
                        &**prev, &*style_map, active_before, elapsed);
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

        // 3a. Apply scroll: posun page commands o -scroll_y. Scrollbar
        //     overlay (pridany nize) je viewport-relative -> add PO shift.
        for cmd in display_list.iter_mut() {
            crate::browser::render::segments::shift_command_y(cmd, -self.scroll_y);
            crate::browser::render::segments::shift_command_x(cmd, -self.scroll_x);
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

        // 5. Renderer kresli display list do target_view.
        let _had = renderer.draw_segments_into_view_clipped(
            target_view, &display_list, true, None);

        // 6. Stash layout_root pro hostujici aplikaci (overlay paint pass).
        self.last_layout_root = Some(layout_root);

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

    /// Aktualni viewport (logical CSS px) sirka.
    pub fn viewport_size(&self) -> (f32, f32) { (self.viewport_w, self.viewport_h) }

    /// HiDPI scale_factor (1.0 / 1.5 / 2.0 ...).
    pub fn scale_factor(&self) -> f32 { self.scale_factor }

    /// `true` pokud stylesheets obsahuji @keyframes (= moznost aktivni
    /// animace) NEBO aktivni CSS transitions. Hostujici aplikace pak
    /// request_redraw kazdy frame.
    pub fn has_active_animations(&self) -> bool {
        self.stylesheets.iter().any(|s| !s.keyframes.is_empty())
            || !self.active_transitions.is_empty()
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
    fn handle_input_scroll_updates_position() {
        let mut wv = fresh();
        wv.dirty = false;
        let resp = wv.handle_input(InputEvent::Scroll {
            dx: 0.0, dy: 50.0, x: 100.0, y: 100.0,
            modifiers: KeyModifiers::default(),
        });
        assert!(resp.dirty, "scroll musi dirty webview");
        assert_eq!(wv.scroll(), (0.0, 50.0));
    }

    #[test]
    fn handle_input_scroll_clamps_negative() {
        let mut wv = fresh();
        // Pri scroll na negativni hodnotu clampujem na 0.
        wv.handle_input(InputEvent::Scroll {
            dx: -100.0, dy: -100.0, x: 0.0, y: 0.0,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(wv.scroll(), (0.0, 0.0));
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
