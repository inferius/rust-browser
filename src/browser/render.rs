/// wgpu renderer + winit window + frame loop.
///
/// Real implementace - vertex buffer s rectangly + glyph atlas pro text.
/// Display list (paint::DisplayCommand) -> vertex data -> GPU.

use super::paint::DisplayCommand;
use bytemuck::{Pod, Zeroable};
use std::rc::Rc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],     // pixel coords
    color: [f32; 4],   // RGBA 0..1
    uv: [f32; 2],      // texture coords (pro text)
    is_text: f32,      // 0.0 = solid color, 1.0 = sample texture
    /// Local coords v ramci rectanglu (-1..1) - pro SDF rounded
    local: [f32; 2],
    /// Half size + radius (pro SDF rounded box)
    half_size: [f32; 2],
    radius: f32,
}

const RECT_SHADER: &str = r#"
struct Uniforms {
    viewport: vec2<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_smp: sampler;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) is_text: f32,
    @location(4) local: vec2<f32>,
    @location(5) half_size: vec2<f32>,
    @location(6) radius: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) is_text: f32,
    @location(3) local: vec2<f32>,
    @location(4) half_size: vec2<f32>,
    @location(5) radius: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let x = (in.pos.x / u.viewport.x) * 2.0 - 1.0;
    let y = 1.0 - (in.pos.y / u.viewport.y) * 2.0;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    out.is_text = in.is_text;
    out.local = in.local;
    out.half_size = in.half_size;
    out.radius = in.radius;
    return out;
}

/// Signed distance to rounded rectangle.
fn sdf_rounded_box(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if (in.is_text > 0.5) {
        let alpha = textureSample(atlas_tex, atlas_smp, in.uv).r;
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Solid rect with optional rounded corners (SDF anti-aliasing)
    if (in.radius > 0.5) {
        let d = sdf_rounded_box(in.local, in.half_size, in.radius);
        let aa = 1.0 - smoothstep(-1.0, 1.0, d);
        return vec4<f32>(in.color.rgb, in.color.a * aa);
    }
    return in.color;
}
"#;

/// Vrati bytemuck-friendly vertices pro display list.
/// Pro Rect: 6 vertexu (2 trojuhelniky). Pro text: 6 vertexu per glyph.
fn build_vertices(commands: &[DisplayCommand], atlas: &GlyphAtlas) -> Vec<Vertex> {
    let mut verts = Vec::new();
    for cmd in commands {
        match cmd {
            DisplayCommand::Rect { x, y, w, h, color, radius } => {
                push_rect_rounded(&mut verts, *x, *y, *w, *h, normalize_color(color), *radius);
            }
            DisplayCommand::Border { x, y, w, h, width, color } => {
                let c = normalize_color(color);
                let bw = *width;
                push_rect(&mut verts, *x, *y, *w, bw, c, [0.0, 0.0], 0.0);
                push_rect(&mut verts, *x, *y + *h - bw, *w, bw, c, [0.0, 0.0], 0.0);
                push_rect(&mut verts, *x, *y, bw, *h, c, [0.0, 0.0], 0.0);
                push_rect(&mut verts, *x + *w - bw, *y, bw, *h, c, [0.0, 0.0], 0.0);
            }
            DisplayCommand::Text { x, y, content, color, font_size, bold: _ } => {
                let c = normalize_color(color);
                let mut pen_x = *x;
                let pen_y = *y + *font_size;
                for ch in content.chars() {
                    if let Some(g) = atlas.get(ch, *font_size as u32) {
                        let gx = pen_x + g.bearing_x;
                        let gy = pen_y - g.bearing_y;
                        push_rect_uv(&mut verts, gx, gy, g.width, g.height, c, g.uv0, g.uv1, 1.0);
                        pen_x += g.advance;
                    } else {
                        pen_x += font_size * 0.5;
                    }
                }
            }
        }
    }
    verts
}

/// Posune Y souradnice display command (pro scroll).
fn shift_command_y(cmd: &mut DisplayCommand, dy: f32) {
    match cmd {
        DisplayCommand::Rect { y, .. }
        | DisplayCommand::Border { y, .. }
        | DisplayCommand::Text { y, .. } => *y += dy,
    }
}

/// Push rect with rounded corners (SDF rendering).
fn push_rect_rounded(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                     color: [f32; 4], radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    // 4 vertices kazdy ma local coords centered (-hw..hw, -hh..hh)
    let make = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [0.0, 0.0],
            is_text: 0.0,
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
        }
    };
    let tl = make(x,     y);
    let tr = make(x + w, y);
    let bl = make(x,     y + h);
    let br = make(x + w, y + h);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

fn push_rect(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
             color: [f32; 4], uv: [f32; 2], is_text: f32) {
    push_rect_uv(verts, x, y, w, h, color, uv, [uv[0], uv[1]], is_text);
}

fn push_rect_uv(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                color: [f32; 4], uv0: [f32; 2], uv1: [f32; 2], is_text: f32) {
    let mk = |px: f32, py: f32, u: f32, v: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [u, v],
            is_text,
            local: [0.0, 0.0],
            half_size: [0.0, 0.0],
            radius: 0.0,
        }
    };
    let tl = mk(x,     y,     uv0[0], uv0[1]);
    let tr = mk(x + w, y,     uv1[0], uv0[1]);
    let bl = mk(x,     y + h, uv0[0], uv1[1]);
    let br = mk(x + w, y + h, uv1[0], uv1[1]);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

fn normalize_color(c: &[u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// Pokusi se najit a nacist systemovy font (None pri selhani - pro layout fallback).
pub fn try_load_default_font() -> Option<Vec<u8>> {
    if let Ok(path) = std::env::var("RUST_WEB_ENGINE_FONT_PATH") {
        if let Ok(data) = std::fs::read(&path) { return Some(data); }
    }
    let candidates: &[&str] = &[
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\verdana.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

fn load_default_font() -> Vec<u8> {
    try_load_default_font()
        .expect("Nelze najit system font. Set RUST_WEB_ENGINE_FONT_PATH na cestu k TTF souboru.")
}

// ─── Glyph atlas ────────────────────────────────────────────────────────

const ATLAS_SIZE: u32 = 1024;

struct GlyphInfo {
    /// UV coords v atlasu (0..1)
    uv0: [f32; 2],
    uv1: [f32; 2],
    width: f32,
    height: f32,
    bearing_x: f32,
    bearing_y: f32,
    advance: f32,
}

struct GlyphAtlas {
    font: fontdue::Font,
    /// Atlas pixely (shedy: 0=transparent, 255=opaque)
    pixels: Vec<u8>,
    /// (char, font_size) -> glyph info
    cache: std::collections::HashMap<(char, u32), GlyphInfo>,
    /// Volna pozice pro dalsi glyph
    cursor_x: u32,
    cursor_y: u32,
    /// Vyska aktualniho radku
    row_height: u32,
}

impl GlyphAtlas {
    fn new() -> Self {
        // Loadnuti systemoveho fontu - zkusi standardni umisteni.
        // Override: env var RUST_WEB_ENGINE_FONT_PATH
        let font_data = load_default_font();
        let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
            .expect("font parse failed");
        GlyphAtlas {
            font,
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            cache: std::collections::HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    fn get(&self, ch: char, size: u32) -> Option<&GlyphInfo> {
        self.cache.get(&(ch, size))
    }

    /// Rasterize glyph and add to atlas. Returns GlyphInfo.
    fn add(&mut self, ch: char, size: u32) {
        if self.cache.contains_key(&(ch, size)) { return; }
        let (metrics, bitmap) = self.font.rasterize(ch, size as f32);
        let w = metrics.width as u32;
        let h = metrics.height as u32;

        // Najdi misto v atlasu
        if self.cursor_x + w > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h > ATLAS_SIZE {
            return; // atlas full
        }
        // Copy bitmap do atlasu
        for row in 0..h {
            for col in 0..w {
                let src = (row * w + col) as usize;
                let dst = ((self.cursor_y + row) * ATLAS_SIZE + (self.cursor_x + col)) as usize;
                if let Some(p) = bitmap.get(src) {
                    self.pixels[dst] = *p;
                }
            }
        }
        let info = GlyphInfo {
            uv0: [self.cursor_x as f32 / ATLAS_SIZE as f32,
                  self.cursor_y as f32 / ATLAS_SIZE as f32],
            uv1: [(self.cursor_x + w) as f32 / ATLAS_SIZE as f32,
                  (self.cursor_y + h) as f32 / ATLAS_SIZE as f32],
            width: w as f32,
            height: h as f32,
            bearing_x: metrics.xmin as f32,
            bearing_y: metrics.ymin as f32 + h as f32,
            advance: metrics.advance_width,
        };
        self.cache.insert((ch, size), info);
        self.cursor_x += w + 1;
        self.row_height = self.row_height.max(h);
    }
}

// ─── Public API ─────────────────────────────────────────────────────────

/// Text-mode dump display listu (bez okna).
pub fn run_browser(html: &str, css: &str) {
    use super::{html_parser, css_parser, cascade, layout, paint};

    let document = html_parser::parse_html(html, "about:blank");
    let stylesheets = vec![css_parser::parse_stylesheet(css)];
    let style_map = cascade::cascade(&document.root, &stylesheets);

    let viewport_w = 1024.0;
    let viewport_h = 768.0;
    let layout_root = layout::layout_tree(&document.root, &style_map, viewport_w, viewport_h);
    let display_list = paint::build_display_list(&layout_root);

    println!("Document title: {}", document.title);
    println!("Display list: {} commands", display_list.len());
    for (i, cmd) in display_list.iter().enumerate().take(20) {
        println!("  [{i}] {cmd:?}");
    }
    if display_list.len() > 20 {
        println!("  ... +{} more", display_list.len() - 20);
    }
}

/// Real GUI okno s wgpu rendering + JS event integrace.
pub fn run_window_with_html(html: String, css: String) -> Result<(), String> {
    use winit::application::ApplicationHandler;
    use winit::event::{WindowEvent, MouseButton, ElementState};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::window::{Window, WindowId};

    struct App {
        html: String,
        css: String,
        window: Option<std::sync::Arc<Window>>,
        renderer: Option<Renderer>,
        layout_root: Option<super::layout::LayoutBox>,
        interpreter: Option<crate::interpreter::Interpreter>,
        mouse_x: f32,
        mouse_y: f32,
        scroll_y: f32,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let attrs = Window::default_attributes()
                .with_title("Rust Web Engine")
                .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 768.0));
            let window = std::sync::Arc::new(event_loop.create_window(attrs).unwrap());
            self.window = Some(window.clone());
            self.renderer = Some(Renderer::new(window.clone()));

            // Vytvor interpreter + nacti HTML do jeho document
            let mut interp = crate::interpreter::Interpreter::new();
            let doc = super::html_parser::parse_html(&self.html, "about:blank");
            interp.set_document(doc);

            // Spust JS uvnitr <script> tagu
            self.run_inline_scripts(&mut interp);

            self.interpreter = Some(interp);
            self.render();
            window.request_redraw();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        r.resize(size.width.max(1), size.height.max(1));
                    }
                    self.render();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_x = position.x as f32;
                    self.mouse_y = position.y as f32 + self.scroll_y;
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                    self.handle_click(self.mouse_x, self.mouse_y);
                    self.render();
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    self.scroll_y -= scroll_amount;
                    if self.scroll_y < 0.0 { self.scroll_y = 0.0; }
                    // Clamp na max scroll
                    if let Some(layout) = &self.layout_root {
                        let viewport_h = self.renderer.as_ref().map(|r| r.config.height as f32).unwrap_or(768.0);
                        let max_scroll = (layout.rect.height - viewport_h).max(0.0);
                        if self.scroll_y > max_scroll { self.scroll_y = max_scroll; }
                    }
                    self.render();
                }
                WindowEvent::RedrawRequested => {
                    self.render();
                }
                _ => {}
            }
        }
    }

    impl App {
        fn run_inline_scripts(&self, interp: &mut crate::interpreter::Interpreter) {
            use crate::lexer::base::Lexer;
            use crate::parser::Parser;
            use crate::tokens::TokenKind;

            let doc_ref = interp.document.clone();
            let scripts: Vec<String> = doc_ref.borrow().root
                .get_elements_by_tag("script")
                .iter()
                .map(|s| s.text_content())
                .collect();

            for src in scripts {
                if src.trim().is_empty() { continue; }
                if let Ok(lex) = Lexer::parse_str(&src, "<inline>") {
                    let tokens: Vec<_> = lex.tokens.into_iter()
                        .filter(|t| !matches!(t.kind,
                            TokenKind::Whitespace | TokenKind::Newline
                            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
                        .collect();
                    let mut parser = Parser::new(tokens);
                    if let Ok(prog) = parser.parse() {
                        if let Err(e) = interp.run(&prog) {
                            eprintln!("[script error] {e}");
                        }
                    }
                }
            }
        }

        fn handle_click(&mut self, x: f32, y: f32) {
            let layout_root = match &self.layout_root { Some(l) => l, None => return };
            let interp = match &mut self.interpreter { Some(i) => i, None => return };

            // Hit test - najdi cilovy LayoutBox
            let target = layout_root.hit_test(x, y);
            if let Some(target) = target {
                if let Some(node) = &target.node {
                    // Vyvolej click listeners na node
                    let ids: Vec<usize> = node.listeners.borrow().get("click")
                        .cloned().unwrap_or_default();
                    if ids.is_empty() { return; }

                    let mut event = crate::interpreter::JsObject::new();
                    event.set("type".into(), crate::interpreter::JsValue::Str("click".into()));
                    event.set("clientX".into(), crate::interpreter::JsValue::Number(x as f64));
                    event.set("clientY".into(), crate::interpreter::JsValue::Number(y as f64));
                    event.set("target".into(), crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(node)));
                    let event_val = crate::interpreter::JsValue::Object(
                        std::rc::Rc::new(std::cell::RefCell::new(event))
                    );
                    for id in ids {
                        let cb = interp.event_callbacks.borrow().get(&id).cloned();
                        if let Some(cb) = cb {
                            let _ = interp.call_function(cb, vec![event_val.clone()], None);
                        }
                    }
                }
            }
        }

        fn render(&mut self) {
            use super::{css_parser, cascade, layout, paint};
            let r = match &mut self.renderer { Some(r) => r, None => return };

            // Pouzij document z interpreteru (po JS modifikacich)
            let document_root = match &self.interpreter {
                Some(i) => Rc::clone(&i.document.borrow().root),
                None => return,
            };

            let stylesheets = vec![css_parser::parse_stylesheet(&self.css)];
            let style_map = cascade::cascade(&document_root, &stylesheets);
            let viewport_w = r.config.width as f32;
            let viewport_h = r.config.height as f32;
            let layout_root = layout::layout_tree(&document_root, &style_map, viewport_w, viewport_h);
            let mut display_list = paint::build_display_list(&layout_root);

            // Apply scroll: posun vsechny y o -scroll_y
            for cmd in display_list.iter_mut() {
                shift_command_y(cmd, -self.scroll_y);
            }

            // Pre-rasterize vsechny glyfy do atlasu
            for cmd in &display_list {
                if let DisplayCommand::Text { content, font_size, .. } = cmd {
                    for ch in content.chars() {
                        r.atlas.add(ch, *font_size as u32);
                    }
                }
            }
            r.upload_atlas();

            let verts = build_vertices(&display_list, &r.atlas);
            r.draw(&verts);

            // Ulozim layout pro hit test
            self.layout_root = Some(layout_root);
        }
    }

    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App {
        html, css,
        window: None,
        renderer: None,
        layout_root: None,
        interpreter: None,
        mouse_x: 0.0,
        mouse_y: 0.0,
        scroll_y: 0.0,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Renderer ───────────────────────────────────────────────────────────

struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    atlas_tex: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    atlas_smp: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    atlas: GlyphAtlas,
}

impl Renderer {
    fn new(window: std::sync::Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            }
        )).expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
            None,
        )).expect("device");
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_caps.formats[0],
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Atlas texture
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_tex.create_view(&Default::default());
        let atlas_smp = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform"),
            size: 16, // viewport (vec2) + padding
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&atlas_smp) },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // pos
                        1 => Float32x4, // color
                        2 => Float32x2, // uv
                        3 => Float32,   // is_text
                        4 => Float32x2, // local
                        5 => Float32x2, // half_size
                        6 => Float32,   // radius
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let atlas = GlyphAtlas::new();

        Renderer {
            surface, device, queue, config, pipeline, uniform_buf,
            atlas_tex, atlas_view, atlas_smp, bind_group_layout, bind_group, atlas,
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    fn upload_atlas(&self) {
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.atlas.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_SIZE),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
        );
    }

    fn draw(&self, vertices: &[Vertex]) {
        // Update uniform: viewport
        let vp = [self.config.width as f32, self.config.height as f32, 0.0, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vb"),
            size: (vertices.len() * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            self.queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(vertices));
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
