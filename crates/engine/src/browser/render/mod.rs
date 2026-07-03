/// wgpu renderer + winit window + frame loop.
///
/// Real implementace - vertex buffer s rectangly + glyph atlas pro text.
/// Display list (paint::DisplayCommand) -> vertex data -> GPU.

use super::paint::DisplayCommand;
use super::devtools_panel::{paint_devtools_panel, devtools_hit_test};
use super::webgl_helpers::{webgl_compute_stride, webgl_attrib_to_vertex_format, webgl_serialize_uniforms};
use bytemuck::{Pod, Zeroable};
use std::rc::Rc;

/// Format window title - pri >=2 tabech prefix s "(N)" tab counter.
pub fn format_window_title(page_title: &str, tab_count: usize) -> String {
    if tab_count >= 2 {
        format!("({}) {} - Rust Web Engine", tab_count, page_title)
    } else {
        format!("{} - Rust Web Engine", page_title)
    }
}

/// Resolvuj address bar input do navigovatelne URL.
/// - "https://x" / "http://x" / "file:///x" / "about:x" - passthrough
/// - "www.x" - prepend "https://"
/// - "domain.tld[/path]" - prepend "https://"
/// - "/abs/path" nebo "C:\..." - file path passthrough
/// - "search query" (s mezerou nebo bez tld) - DuckDuckGo Lite search URL
pub fn resolve_addr_input(input: &str) -> String {
    let s = input.trim();
    if s.starts_with("http://") || s.starts_with("https://")
       || s.starts_with("file:///") || s.starts_with("about:") {
        return s.to_string();
    }
    if s.starts_with("www.") {
        return format!("https://{}", s);
    }
    // Look like domain (contains dot, no spaces, ASCII-ish).
    let looks_like_domain = !s.contains(' ')
        && s.contains('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || ".-_/?:&=%#".contains(c));
    if looks_like_domain {
        return format!("https://{}", s);
    }
    // Local path heuristics.
    if s.starts_with('/') || (s.len() >= 3 && &s[1..3] == ":\\") {
        return s.to_string();
    }
    // Default: search query -> DuckDuckGo Lite.
    let q: String = s.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
    format!("https://duckduckgo.com/?q={}", q)
}

// BookmarkPickerState / BookmarkPickerFocus smazany (Session N+22) - shell concern.

// READING_MODE_CSS smazany (Session N+22) - reading mode je shell concern.

// Async worker pro JS exec vyzaduje Interpreter: Send. Aktualne Interpreter ma
// Rc<RefCell> interne, takze !Send. Wrappers `unsafe impl Send for SendInterp`
// nestaci protoze closure auto-trait check projde dovnitr Rc pres autoderef.
// Reseni: Arc<Mutex> rework napric ~30 souboru (Interpreter struct, JsValue,
// JsObject, NodeData, Document, atd.) - viz HANDOFF Arc rework TODO.
// Aktualne: shared_debugger + Continue Condvar foundation pripravena, ale
// scripts beti single-thread (UI). Pause = early-abort + rerun kompromis.

mod url;
pub use url::{fetch_text_url, fetch_image_bytes, resolve_url, cached_fetch_bytes};

// shell_chrome.rs smazany (Session N+22) - chrome paint je shell crate concern.

pub mod forms;
// forms helpers (find_ancestor_form/build_form_request/post_form) momentalne
// unused na App vrstve - form submit hit-dispatch presunut do WebView pipeline.


pub mod segments;
pub use segments::{Seg, partition_filter_segments};

mod polygon;
#[allow(unused_imports)] // pub use - test exposure (cargo build je nevidi)
pub use polygon::{polygon_signed_area, triangulate_polygon};

mod atlas;
pub use atlas::{try_load_default_font, ImageAtlas};
use atlas::{GlyphAtlas, ATLAS_SIZE, IMAGE_ATLAS_SIZE};

pub mod font_face;
pub use font_face::SwashFontFace;

mod shaders;
use shaders::{BLUR_SHADER, TRANSFORM_SHADER, COMPOSE_SHADER, RECT_SHADER, LCD_SHADER, ADVANCED_BLEND_SHADER, BLEND_COMPOSE_SHADER};

mod primitives;
use primitives::{push_rect, push_rect_rounded, push_rect_corners, push_rect_uv, push_skewed_quad,
    push_triangle, push_polygon_edge_aa, push_blurred_rect, push_image, push_gradient,
    push_clipped_linear_gradient,
    push_radial_gradient, push_conic_gradient, push_multi_stop_linear_gradient,
    push_multi_stop_radial_gradient, push_multi_stop_conic_gradient,
    push_shadow, push_inset_shadow, normalize_color};

pub mod canvas_paint;

mod webgl_paint;
mod text_input;
pub mod blend;
pub mod subpixel_aa;
pub mod compositor;
pub mod tiles;
pub mod frame_pacing;
pub mod hit_test_tree;
// tabs.rs smazany N+22 (TabManager + Tab + about: pages = shell concerns).
// extract_title presunut do `embed::loader::extract_title`.
#[allow(unused_imports)] // pub use - test exposure
pub use webgl_paint::paint_webgl_canvases;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Vertex {
    pub(super) pos: [f32; 2],
    pub(super) color: [f32; 4],
    pub(super) uv: [f32; 2],
    /// Mode: 0=solid, 1=text, 2=gradient, 3=shadow blur
    pub(super) mode: f32,
    /// Local coords v ramci rectanglu (centered)
    pub(super) local: [f32; 2],
    /// Half size pro SDF
    pub(super) half_size: [f32; 2],
    pub(super) radius: f32,
    /// Druha barva pro gradient (interpolovana z color->color2 podle uv.x)
    pub(super) color2: [f32; 4],
    /// Blur radius pro shadow
    pub(super) blur: f32,
}


/// Vrati bytemuck-friendly vertices pro display list.
/// Pro Rect: 6 vertexu (2 trojuhelniky). Pro text: 6 vertexu per glyph.
/// Build LCD subpixel pipeline (dual-source blend). Catch-unwind + error scope
/// guard - pri shader compile / pipeline validation fail vraci None (fallback
/// grayscale v hlavnim pipeline mode 9).
fn build_lcd_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
) -> Option<wgpu::RenderPipeline> {
    let shader_or_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lcd-shader"),
            source: wgpu::ShaderSource::Wgsl(LCD_SHADER.into()),
        })
    }));
    let lcd_shader = match shader_or_panic {
        Ok(s) => s,
        Err(_) => {
            eprintln!("[render] LCD shader compile panic - fallback grayscale");
            return None;
        }
    };
    let pipe_or_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            multiview_mask: None,
            label: Some("lcd-pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &lcd_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, 1 => Float32x4, 2 => Float32x2,
                        3 => Float32, 4 => Float32x2, 5 => Float32x2,
                        6 => Float32, 7 => Float32x4, 8 => Float32,
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &lcd_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Src1,
                            dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrc1Alpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        })
    }));
    match pipe_or_panic {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("[render] LCD pipeline create panic - fallback grayscale");
            None
        }
    }
}

/// Extract TextRun info z DisplayCommand::Text pro per-glyph selection.
/// Spocita cumulative_advances pres atlas glyph widths.
pub fn extract_text_runs(
    commands: &[DisplayCommand],
    atlas: &GlyphAtlas,
    zoom: f32,
) -> Vec<crate::browser::textrun::TextRun> {
    let mut runs = Vec::new();
    for cmd in commands {
        if let DisplayCommand::Text { x, y, content, font_size, font_weight, italic, font_family, .. } = cmd {
            let z = zoom.max(0.0001);
            let physical_size = (*font_size * z).round().max(1.0) as u32;
            let inv_z = 1.0 / z;
            // Styled lookup - atlas encoduje (family, weight, italic) interne.
            let mut cumulative: Vec<f32> = Vec::with_capacity(content.chars().count() + 1);
            cumulative.push(0.0);
            let mut acc = 0.0;
            for ch in content.chars() {
                let advance = atlas.get_styled(font_family, *font_weight, *italic, ch, physical_size)
                    .map(|g| g.advance * inv_z)
                    .unwrap_or(*font_size * 0.5);
                acc += advance;
                cumulative.push(acc);
            }
            runs.push(crate::browser::textrun::TextRun {
                node_id: 0, // populated later kdyz mam DOM ref
                text: content.clone(),
                origin_x: *x,
                origin_y: *y,
                cumulative_advances: cumulative,
                line_height: *font_size * 1.2,
            });
        }
    }
    runs
}

/// Stable hash pro vertex cache klic. DisplayCommand obsahuje f32 (no Hash trait
/// derive moznost). Hash pres Debug format string - cheap (1-2ms per 1000 cmds)
/// vs build_vertices 35ms - net pozitivni i pri vsem rebuildu.
fn hash_display_command<H: std::hash::Hasher>(c: &DisplayCommand, h: &mut H) {
    use std::hash::Hash;
    let s = format!("{:?}", c);
    s.hash(h);
}

thread_local! {
    /// Pixel-snap scale (= zoom * scale_factor). Renderer set pred build_vertices.
    /// 1.0 = no snap. Default 1.0 protoze unit tests + headless builds.
    static PIXEL_SNAP_SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
    /// CSS mix-blend-mode pipeline override pres compose_view_to_view_blend.
    /// 0 = Normal/default (alpha blend pipeline). 1+ = blend mode discriminant.
    static BLEND_MODE_OVERRIDE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// GPU scissor pro compose pass vrstvy (PHYSICAL px, x/y/w/h, uz clampnuty
    /// callerem na target dims). Nastavuje compose_layer_tree_into pro vrstvy
    /// s ancestor overflow clipem (LayerNode.clip_rect) - napr. marquee
    /// translateX anim uvnitr overflow:hidden. Compose fns (plain/transform/
    /// blend) ho aplikuji na render pass; w/h == 0 => draw SKIP (vse cliple).
    /// None = bez scissoru (default; nutno resetovat po compose vrstvy).
    static COMPOSE_SCISSOR: std::cell::Cell<Option<(u32, u32, u32, u32)>> =
        const { std::cell::Cell::new(None) };
    /// Viewport override pres render_into_tile - draw_segments_into_view_clipped
    /// drive prepisoval uniform na vp_dims (= full WebView viewport) ALE tile
    /// raster potrebuje vp = tile dims. Pri (w, h) > 0 = override aktivni.
    static VIEWPORT_OVERRIDE: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
    /// Glyph-level selection clip (Chrome selection painting): pri ne-prazdnem
    /// seznamu rectu build_vertices kresli Text glyfy JEN s centrem uvnitr
    /// nektereho rectu (prebarvene znaky pres puvodni vrstvu; nevybrane
    /// zustavaji z layer textury pod tim).
    static SELECTION_GLYPH_CLIP: std::cell::RefCell<Vec<(f32, f32, f32, f32)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Raster boost pres render_into_layer pro layery se scale transformem.
    /// Layer texture se alokuje scale_hint x vetsi a glyf/image raster se musi
    /// boostnout o stejny faktor (jinak atlas glyf zustane base size = upscale
    /// pri compose = rozmazany text u scale(N)). 1.0 = bez boostu (default).
    static RASTER_BOOST: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Build compose pipeline uniform [28 floats = 7 vec4]:
/// - 5 vec4 color matrix (row0..row3 + offset)
/// - vec4 dst_box (x_min_ndc, y_min_ndc, x_max_ndc, y_max_ndc)
/// - vec4 src_uv  (u_min, v_min_top, u_max, v_max_bottom)
///
/// Caller passes (dst_x, dst_y, dst_w, dst_h) v LOGICAL viewport coords +
/// (src_u0..src_v1) v normalized texture coords + viewport logical dims.
/// Computes visible intersection (dst clipped to [0, vp_w/h]) -> dst_box NDC
/// safely v [-1, 1] + src_uv slice corresponding visible portion.
///
/// Vraci `(uniform_data, visible)` - visible=false znamena dst je completely
/// off-screen (caller muze skip draw).
///
/// Y mapping konvencie:
///   - dst input v logical px, y=0 = top of viewport, y=vp_h = bottom
///   - dst_box NDC: x_min/max v [-1,1] (left to right), y_min = NDC bottom
///     (= logical y_max), y_max = NDC top (= logical y_min)
///   - src_uv input: v_min_top (texture top, v=0), v_max_bottom (v=1)
///

/// Vraci current zoom*dpr scale (pres PIXEL_SNAP_SCALE). Pouzivany pres
/// paint.rs pri inline SVG rasterize - target physical px = logical * scale.
pub fn current_zoom() -> Option<f32> {
    Some(PIXEL_SNAP_SCALE.with(|c| c.get()))
}

/// Flush INLINE_SVG_CACHE -> image_atlas. Volat pred build_vertices aby
/// DisplayCommand::Image s `__inline_svg_*` key nasel uploadovane bitmaps.
/// Inline SVG rasterized v paint pres resvg + tiny-skia (Chrome quality).
pub(crate) fn flush_inline_svg_cache(image_atlas: &mut ImageAtlas) {
    // Upload JEN dirty (nove/zmenene) klice - ne cely cache O(N) kazdy frame.
    // replace() prepise existujici atlas slot in-place (animovany SVG = stejny
    // slot, bez rustu) nebo prida novy.
    crate::browser::paint::INLINE_SVG_DIRTY.with(|d| {
        let mut dirty = d.borrow_mut();
        if dirty.is_empty() { return; }
        crate::browser::paint::INLINE_SVG_CACHE.with(|c| {
            let cache = c.borrow();
            for key in dirty.iter() {
                if let Some((rgba, w, h, _hash)) = cache.get(key) {
                    image_atlas.replace(key, *w, *h, rgba);
                }
            }
        });
        dirty.clear();
    });
}

/// Inspired by WebRender `composite.rs::CompositeTile` per-quad pattern.
pub(crate) fn build_compose_uniform_box(
    color_matrix: &[f32; 20],
    dst_x: f32, dst_y: f32, dst_w: f32, dst_h: f32,
    src_u0: f32, src_v0: f32, src_u1: f32, src_v1: f32,
    vp_w: f32, vp_h: f32,
) -> ([f32; 28], bool) {
    let vp_w = vp_w.max(0.001);
    let vp_h = vp_h.max(0.001);
    // Visible intersection of dst rect with viewport.
    let vis_x0 = dst_x.max(0.0).min(vp_w);
    let vis_y0 = dst_y.max(0.0).min(vp_h);
    let vis_x1 = (dst_x + dst_w).max(0.0).min(vp_w);
    let vis_y1 = (dst_y + dst_h).max(0.0).min(vp_h);
    let vis_w = vis_x1 - vis_x0;
    let vis_h = vis_y1 - vis_y0;
    let visible = vis_w > 0.0 && vis_h > 0.0;
    // UV slice corresponding to visible portion of source texture region.
    let inv_w = if dst_w.abs() > 1e-4 { 1.0 / dst_w } else { 0.0 };
    let inv_h = if dst_h.abs() > 1e-4 { 1.0 / dst_h } else { 0.0 };
    let frac_u0 = (vis_x0 - dst_x) * inv_w;
    let frac_u1 = (vis_x1 - dst_x) * inv_w;
    let frac_v0 = (vis_y0 - dst_y) * inv_h;
    let frac_v1 = (vis_y1 - dst_y) * inv_h;
    let u0 = src_u0 + (src_u1 - src_u0) * frac_u0;
    let u1 = src_u0 + (src_u1 - src_u0) * frac_u1;
    let v0 = src_v0 + (src_v1 - src_v0) * frac_v0;
    let v1 = src_v0 + (src_v1 - src_v0) * frac_v1;
    // NDC bounds. y_min/max NDC vychazi z logical: y=0 -> NDC y=+1 (top),
    // y=vp_h -> NDC y=-1 (bottom).
    let x_min_ndc = (vis_x0 / vp_w) * 2.0 - 1.0;
    let x_max_ndc = (vis_x1 / vp_w) * 2.0 - 1.0;
    let y_min_ndc = 1.0 - (vis_y1 / vp_h) * 2.0;
    let y_max_ndc = 1.0 - (vis_y0 / vp_h) * 2.0;
    let m = color_matrix;
    let data = [
        // row0..row3 (color matrix rows, alpha column = m[3], m[8], m[13], m[18])
        m[0], m[1], m[2], m[3],
        m[5], m[6], m[7], m[8],
        m[10], m[11], m[12], m[13],
        m[15], m[16], m[17], m[18],
        // offset (per-channel additive)
        m[4], m[9], m[14], m[19],
        // dst_box NDC corners
        x_min_ndc, y_min_ndc, x_max_ndc, y_max_ndc,
        // src_uv (u_min, v_min_top, u_max, v_max_bottom)
        u0, v0, u1, v1,
    ];
    (data, visible)
}

#[cfg(test)]
mod compose_uniform_tests {
    use super::*;

    const IDENTITY: [f32; 20] = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];

    /// Full-frame compose: dst = viewport, src = full UV.
    #[test]
    fn full_frame_compose_pres_full_ndc_and_uv() {
        let (data, vis) = build_compose_uniform_box(
            &IDENTITY, 0.0, 0.0, 1280.0, 800.0, 0.0, 0.0, 1.0, 1.0, 1280.0, 800.0);
        assert!(vis);
        // dst_box NDC corners.
        assert!((data[20] - (-1.0)).abs() < 1e-4, "x_min={}", data[20]);
        assert!((data[21] - (-1.0)).abs() < 1e-4, "y_min={}", data[21]);
        assert!((data[22] - 1.0).abs() < 1e-4, "x_max={}", data[22]);
        assert!((data[23] - 1.0).abs() < 1e-4, "y_max={}", data[23]);
        // src_uv = full texture.
        assert!((data[24] - 0.0).abs() < 1e-4);
        assert!((data[25] - 0.0).abs() < 1e-4);
        assert!((data[26] - 1.0).abs() < 1e-4);
        assert!((data[27] - 1.0).abs() < 1e-4);
    }

    /// Layer taller nez viewport (root layer = full page, page_h > vp_h).
    /// Scroll=0 -> visible = top portion of layer texture.
    #[test]
    fn tall_layer_no_scroll_pres_top_uv_slice() {
        // page_h=2000, vp_h=800. Compose celou layer rozsahu -> visible jen
        // top 800 logical px.
        let (data, vis) = build_compose_uniform_box(
            &IDENTITY, 0.0, 0.0, 1280.0, 2000.0, 0.0, 0.0, 1.0, 1.0, 1280.0, 800.0);
        assert!(vis);
        // Visible region cely viewport.
        assert!((data[20] - (-1.0)).abs() < 1e-4);
        assert!((data[21] - (-1.0)).abs() < 1e-4);
        assert!((data[22] - 1.0).abs() < 1e-4);
        assert!((data[23] - 1.0).abs() < 1e-4);
        // src_uv v range: top 800/2000 = 0..0.4 portion of texture.
        assert!((data[25] - 0.0).abs() < 1e-4, "v_min={}", data[25]);
        assert!((data[27] - 0.4).abs() < 1e-4, "v_max={}", data[27]);
    }

    /// Scroll posune zobrazenou portion texture.
    #[test]
    fn tall_layer_scrolled_pres_shifted_uv_slice() {
        // page_h=2000, vp_h=800, scroll_y=400. dst_y = -400 (layer.root_rect.y - scroll_y).
        let (data, vis) = build_compose_uniform_box(
            &IDENTITY, 0.0, -400.0, 1280.0, 2000.0, 0.0, 0.0, 1.0, 1.0, 1280.0, 800.0);
        assert!(vis);
        // Visible = viewport full.
        assert!((data[21] - (-1.0)).abs() < 1e-4, "y_min={}", data[21]);
        assert!((data[23] - 1.0).abs() < 1e-4, "y_max={}", data[23]);
        // src_uv shift: page y 400..1200 visible. v = 400/2000=0.2 .. 1200/2000=0.6.
        assert!((data[25] - 0.2).abs() < 1e-4, "v_min={}", data[25]);
        assert!((data[27] - 0.6).abs() < 1e-4, "v_max={}", data[27]);
    }

    /// Scroll near end - bottom of layer visible, top portion off-screen.
    #[test]
    fn tall_layer_scrolled_near_bottom() {
        // page_h=2000, scroll=1500. dst_y = -1500, dst_h = 2000.
        // Visible logical y = max(0, -1500) .. min(800, -1500+2000=500) = 0..500.
        // src_uv: frac_v0 = (0-(-1500))/2000 = 0.75, frac_v1 = (500-(-1500))/2000 = 1.0.
        let (data, vis) = build_compose_uniform_box(
            &IDENTITY, 0.0, -1500.0, 1280.0, 2000.0, 0.0, 0.0, 1.0, 1.0, 1280.0, 800.0);
        assert!(vis);
        // Visible y range = top 500 px of viewport (bottom 300 = nothing).
        // y_max_ndc = 1 - 0/800*2 = 1, y_min_ndc = 1 - 500/800*2 = -0.25.
        assert!((data[23] - 1.0).abs() < 1e-4, "y_max={}", data[23]);
        assert!((data[21] - (-0.25)).abs() < 1e-4, "y_min={}", data[21]);
        // UV: 0.75..1.0.
        assert!((data[25] - 0.75).abs() < 1e-4, "v_min={}", data[25]);
        assert!((data[27] - 1.0).abs() < 1e-4, "v_max={}", data[27]);
    }

    /// Layer mimo viewport (scroll past page end) -> not visible.
    #[test]
    fn layer_off_screen_pres_not_visible() {
        // dst_y = -2500, dst_h = 2000 -> layer od y=-2500 do y=-500 = vsechno mimo top.
        let (_, vis) = build_compose_uniform_box(
            &IDENTITY, 0.0, -2500.0, 1280.0, 2000.0, 0.0, 0.0, 1.0, 1.0, 1280.0, 800.0);
        assert!(!vis);
    }

    /// Layer wider nez viewport (rare - layer overflow horizontally).
    /// Visible jen left portion of layer.
    #[test]
    fn wide_layer_pres_left_uv_slice() {
        let (data, vis) = build_compose_uniform_box(
            &IDENTITY, 0.0, 0.0, 2560.0, 800.0, 0.0, 0.0, 1.0, 1.0, 1280.0, 800.0);
        assert!(vis);
        // src_uv u range = left half 0..0.5.
        assert!((data[24] - 0.0).abs() < 1e-4);
        assert!((data[26] - 0.5).abs() < 1e-4, "u_max={}", data[26]);
    }

    /// Identity matrix passes thru pres uniform offset.
    /// uniform layout: data[0..4]=row0, [4..8]=row1, [8..12]=row2, [12..16]=row3,
    /// [16..20]=offset, [20..24]=dst_box, [24..28]=src_uv.
    /// Identity: row3 = (0, 0, 0, 1) (alpha coefficient at .w = 1.0).
    #[test]
    fn identity_matrix_unchanged() {
        let (data, _) = build_compose_uniform_box(
            &IDENTITY, 0.0, 0.0, 100.0, 100.0, 0.0, 0.0, 1.0, 1.0, 100.0, 100.0);
        // row0 = (1, 0, 0, 0)
        assert!((data[0] - 1.0).abs() < 1e-6);
        assert!((data[1] - 0.0).abs() < 1e-6);
        // row3 = (0, 0, 0, 1) - alpha coef je posledni column (data[15]).
        assert!((data[12] - 0.0).abs() < 1e-6);
        assert!((data[13] - 0.0).abs() < 1e-6);
        assert!((data[14] - 0.0).abs() < 1e-6);
        assert!((data[15] - 1.0).abs() < 1e-6, "row3.w={}", data[15]);
        // offset = (0, 0, 0, 0).
        assert!((data[19] - 0.0).abs() < 1e-6);
    }

    /// Alpha multiply (opacity 0.5) reflektoval v row3 column 4 (alpha).
    #[test]
    fn opacity_half_pres_row3_w() {
        let mut m = IDENTITY;
        m[18] = 0.5; // row3 col 3 (alpha coefficient).
        let (data, _) = build_compose_uniform_box(
            &m, 0.0, 0.0, 100.0, 100.0, 0.0, 0.0, 1.0, 1.0, 100.0, 100.0);
        // row3 = (0, 0, 0, 0.5).
        assert!((data[12] - 0.0).abs() < 1e-6);
        assert!((data[13] - 0.0).abs() < 1e-6);
        assert!((data[14] - 0.0).abs() < 1e-6);
        assert!((data[15] - 0.5).abs() < 1e-6, "row3.w={}", data[15]);
    }
}

/// Pixel-snap axis-aligned coord do physical px grid.
/// `scale` = zoom * device_pixel_ratio. Snap eliminuje sub-pixel blur na
/// 1px borderech a glyph edge, hlavni vizualni vada pri sub-pixel rect.
/// Inspired by Chromium core/paint/PixelSnappedLayoutPoint + WebRender
/// gfx/wr/webrender/src/util.rs::SnapToDevicePixel.
#[inline]
fn snap_to_phys(v: f32, scale: f32) -> f32 {
    if scale <= 0.0 { return v; }
    (v * scale).round() / scale
}

/// Snap rect (x, y, w, h) so right+bottom edges tez snap (= w/h derived).
/// Bez tohohle by snap(x) + snap(w) divergoval s snap(x+w).
#[inline]
fn snap_rect(x: f32, y: f32, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
    let x0 = snap_to_phys(x, scale);
    let y0 = snap_to_phys(y, scale);
    let x1 = snap_to_phys(x + w, scale);
    let y1 = snap_to_phys(y + h, scale);
    (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Apply 4x5 CSS color matrix na [u8; 4] sRGB color. Layout match spec /
/// shader (rows R/G/B/A x cols R/G/B/A/offset). Pres CSS filter (sepia/
/// hue-rotate/saturate/brightness/contrast/grayscale) layer-mode bypass cesta.
/// In/out v sRGB byte space - matrix definovana pres sRGB pres CSS spec.
fn apply_color_matrix_rgba(c: &mut [u8; 4], m: &[f32; 20]) {
    let r = c[0] as f32 / 255.0;
    let g = c[1] as f32 / 255.0;
    let b = c[2] as f32 / 255.0;
    let a = c[3] as f32 / 255.0;
    let nr = m[0]*r + m[1]*g + m[2]*b + m[3]*a + m[4];
    let ng = m[5]*r + m[6]*g + m[7]*b + m[8]*a + m[9];
    let nb = m[10]*r + m[11]*g + m[12]*b + m[13]*a + m[14];
    let na = m[15]*r + m[16]*g + m[17]*b + m[18]*a + m[19];
    c[0] = (nr.clamp(0.0, 1.0) * 255.0).round() as u8;
    c[1] = (ng.clamp(0.0, 1.0) * 255.0).round() as u8;
    c[2] = (nb.clamp(0.0, 1.0) * 255.0).round() as u8;
    c[3] = (na.clamp(0.0, 1.0) * 255.0).round() as u8;
}

/// Walk display commands + apply color matrix na vsechny barevne fieldy.
/// Pres CSS filter (non-blur) layer-mode CPU bypass = matrix aplikovany na
/// vertex color misto offscreen RT roundtrip.
fn apply_color_matrix_to_cmds(cmds: &mut [DisplayCommand], matrix: &[f32; 20]) {
    for cmd in cmds.iter_mut() {
        match cmd {
            DisplayCommand::Rect { color, .. } |
            DisplayCommand::RectRounded { color, .. } |
            DisplayCommand::Border { color, .. } |
            DisplayCommand::Text { color, .. } |
            DisplayCommand::Shadow { color, .. } |
            DisplayCommand::BlurredRect { color, .. } |
            DisplayCommand::ClippedRect { color, .. } => {
                apply_color_matrix_rgba(color, matrix);
            }
            DisplayCommand::Gradient { stops, .. } => {
                for (_o, c) in stops.iter_mut() {
                    apply_color_matrix_rgba(c, matrix);
                }
            }
            _ => {}
        }
    }
}

fn build_vertices(commands: &[DisplayCommand], atlas: &GlyphAtlas, image_atlas: &ImageAtlas, zoom: f32) -> Vec<Vertex> {
    // PIXEL_SNAP_SCALE thread_local nastavi Renderer pred call (= zoom * dpr).
    // Default 1.0 = bez snapu.
    let snap = PIXEL_SNAP_SCALE.with(|c| c.get());
    // Selection glyph clip - lokalni kopie (prazdna = zadne filtrovani).
    let sel_clip: Vec<(f32, f32, f32, f32)> = SELECTION_GLYPH_CLIP.with(|c| c.borrow().clone());
    // PERF: pre-alloc capacity estimate (6 verts per cmd avg).
    let mut verts = Vec::with_capacity(commands.len() * 6);
    for cmd in commands {
        match cmd {
            DisplayCommand::Rect { x, y, w, h, color, radius } => {
                let (sx, sy, sw, sh) = snap_rect(*x, *y, *w, *h, snap);
                push_rect_rounded(&mut verts, sx, sy, sw, sh, normalize_color(color), *radius);
            }
            DisplayCommand::RectRounded { x, y, w, h, color, radii } => {
                let (sx, sy, sw, sh) = snap_rect(*x, *y, *w, *h, snap);
                push_rect_corners(&mut verts, sx, sy, sw, sh, normalize_color(color), *radii);
            }
            DisplayCommand::Border { x, y, w, h, width, color } => {
                let c = normalize_color(color);
                let bw = *width;
                // Snap outer rect, then derive 4 edge sub-rects. Bez snapu by
                // 1px borders byly 0.4-0.6 alpha blurry edges.
                let (sx, sy, sw, sh) = snap_rect(*x, *y, *w, *h, snap);
                let sbw = snap_to_phys(bw, snap).max(if bw > 0.0 { 1.0 / snap.max(1.0) } else { 0.0 });
                push_rect(&mut verts, sx, sy, sw, sbw, c, [0.0, 0.0], 0.0);
                push_rect(&mut verts, sx, sy + sh - sbw, sw, sbw, c, [0.0, 0.0], 0.0);
                push_rect(&mut verts, sx, sy, sbw, sh, c, [0.0, 0.0], 0.0);
                push_rect(&mut verts, sx + sw - sbw, sy, sbw, sh, c, [0.0, 0.0], 0.0);
            }
            DisplayCommand::Text { x, y, content, color, font_size, bold, font_weight, italic, font_family, strikethrough, underline } => {
                let c = normalize_color(color);
                let mut pen_x = *x;
                let mut pen_y = *y + *font_size;
                // Line advance match layout flush_inline (1.2 default) -
                // jinak \n wrap render advancuje vic nez layout-allocated
                // height -> text leze pres rect.height + nadbytecny visual
                // gap mezi radky.
                let line_advance = *font_size * 1.2;
                // Italic: pri dostupne italic font variante real italic raster
                // (skew = 0). Pri fallback fake skew transform.
                let has_real_italic = *italic && (
                    (*bold && atlas.font_bold_italic.is_some())
                    || (!*bold && atlas.font_italic.is_some())
                );
                let italic_skew: f32 = if *italic && !has_real_italic { 0.2 } else { 0.0 };
                let bold_offset: f32 = if *bold && atlas.font_bold.is_none() && atlas.font_bold_italic.is_none() { 1.0 } else { 0.0 };
                let start_x = pen_x;
                // Glyf rasterization na PHYSICAL px (font_size * zoom) -> sharp text
                // pri zoomu. Atlas lookup klicem physical_size, GlyphInfo metrics
                // jsou v physical px. Vertex emit deli velikost zoomem -> logical
                // glyf quad. NDC mapping (vp = window/zoom) pak skaluje zpet na
                // physical = 1:1 mapping atlas pixelu na obrazovku.
                // RASTER_BOOST (>1 pri render_into_layer_scaled = scale(N) layer)
                // zvysi glyf physical_size o scale faktor -> atlas glyf je N x
                // vetsi a vyplni N x vetsi layer texturu = ostry text u scale(N).
                // inv_z deli zpet na logical -> glyf quad logical zustava stejny.
                let z = (zoom * RASTER_BOOST.with(|c| c.get())).max(0.0001);
                let physical_size = (*font_size * z).round().max(1.0) as u32;
                let inv_z = 1.0 / z;
                // Styled cache key - compose pres atlas helper (encoding interne).
                let lookup_family: String = super::render::atlas::GlyphAtlas::compose_styled(font_family, *font_weight, *italic);
                let family_hash = super::render::atlas::GlyphAtlas::hash_family(&lookup_family);
                // COLR emoji key prefix - drive format per char, ted pre-build
                // base + concat char u32 + size jen pri lookup. Skip kdyz no
                // COLR fonts (image_atlas typicky empty pro normal pages).
                let has_color_fonts = !image_atlas.cache.is_empty();
                let colr_prefix: String = if has_color_fonts {
                    format!("__colr:{}:", font_family)
                } else { String::new() };
                // GLOBAL FIX: render glyph X positions z shape_text (single
                // source of truth s caret + selection). Bez tohoto atlas
                // advance + measure mohli rounding-differ -> caret mismatch.
                // Per-line shape - split obsahu na lines, kazda jako vlastni
                // shape (newline reset pen_x = start_x).
                let lines: Vec<&str> = content.split('\n').collect();
                for (line_idx, line) in lines.iter().enumerate() {
                    if line_idx > 0 {
                        pen_y += line_advance;
                    }
                    // pen_x reset implicit pri g_x_logical = start_x + run.x.
                    // Shape current line - x positions a advances per char.
                    let line_runs = super::editor::shape_text(
                        line, *font_size, *font_weight, *italic, font_family, 0.0
                    ).0;
                    for (ch_idx, ch) in line.chars().enumerate() {
                        let g_x_logical = start_x + line_runs.get(ch_idx).map(|r| r.x).unwrap_or(0.0);
                        if has_color_fonts {
                            let mut colr_key = colr_prefix.clone();
                            colr_key.push_str(&(ch as u32).to_string());
                            colr_key.push(':');
                            colr_key.push_str(&(*font_size as u32).to_string());
                            if let Some(info) = image_atlas.get(&colr_key) {
                                let gx = g_x_logical.round();
                                let gy = (pen_y - info.height).round();
                                push_image(&mut verts, gx, gy, info.width, info.height, info.uv0, info.uv1, 0.0);
                                continue;
                            }
                        }
                        if let Some(g) = atlas.get_hashed(family_hash, ch, physical_size) {
                            // Glyf bbox v physical -> dele inv_z na logical.
                            let g_w = g.width * inv_z;
                            let g_h = g.height * inv_z;
                            let g_bx = g.bearing_x * inv_z;
                            let g_by = g.bearing_y * inv_z;
                            let gx_raw = g_x_logical + g_bx;
                            let gy_raw = pen_y - g_by;
                            let gx = (gx_raw * z).round() * inv_z;
                            let gy = (gy_raw * z).round() * inv_z;
                            // Selection pass: glyf mimo highlight recty SKIP
                            // (zustane puvodni z vrstvy pod) - prebarvi se jen
                            // vybrane znaky, presne jako Chrome.
                            if !sel_clip.is_empty() {
                                let ccx = gx + g_w * 0.5;
                                let ccy = gy + g_h * 0.5;
                                let inside = sel_clip.iter().any(|(rx, ry, rw, rh)|
                                    ccx >= *rx && ccx <= rx + rw && ccy >= *ry && ccy <= ry + rh);
                                if !inside { continue; }
                            }
                            let text_mode = if g.lcd { 9.0 } else { 1.0 };
                            if italic_skew != 0.0 {
                                let skew = g_h * italic_skew;
                                push_skewed_quad(&mut verts, gx, gy, g_w, g_h, skew, c, g.uv0, g.uv1);
                            } else {
                                push_rect_uv(&mut verts, gx, gy, g_w, g_h, c, g.uv0, g.uv1, text_mode);
                            }
                            if bold_offset > 0.0 {
                                let bo = bold_offset * inv_z;
                                if italic_skew != 0.0 {
                                    let skew = g_h * italic_skew;
                                    push_skewed_quad(&mut verts, gx + bo, gy, g_w, g_h, skew, c, g.uv0, g.uv1);
                                } else {
                                    push_rect_uv(&mut verts, gx + bo, gy, g_w, g_h, c, g.uv0, g.uv1, text_mode);
                                }
                            }
                        } else if std::env::var("RWE_TEXT_MISS").is_ok() {
                            eprintln!("[TEXT MISS] char={:?} (u+{:04X}) family={:?} weight={} italic={} physical_size={} ctx={:?}",
                                ch, ch as u32, font_family, *font_weight, *italic, physical_size,
                                content.chars().take(30).collect::<String>());
                        }
                    }
                    // Po line update pen_x na konec line pres shape total.
                    pen_x = start_x + line_runs.iter().map(|r| r.advance).sum::<f32>();
                }
                // Strikethrough line cca v 50% font size od top.
                let text_w = pen_x - start_x;
                if *strikethrough && text_w > 0.0 {
                    let line_y = *y + *font_size * 0.5;
                    let thickness = (font_size * 0.06).max(1.0);
                    push_rect(&mut verts, start_x, line_y, text_w, thickness, c, [0.0, 0.0], 0.0);
                }
                if *underline && text_w > 0.0 {
                    let line_y = *y + *font_size * 0.95;
                    let thickness = (font_size * 0.06).max(1.0);
                    push_rect(&mut verts, start_x, line_y, text_w, thickness, c, [0.0, 0.0], 0.0);
                }
            }
            DisplayCommand::Gradient { x, y, w, h, kind, stops, radius } => {
                if stops.len() >= 2 {
                    let stops_f: Vec<(f32, [f32; 4])> = stops.iter()
                        .map(|(o, c)| (*o, normalize_color(c))).collect();
                    let c0 = stops_f[0].1;
                    let c1 = stops_f.last().unwrap().1;
                    use crate::browser::paint::GradientKind;
                    match kind {
                        GradientKind::Linear { angle_deg } => {
                            if std::env::var("RWE_GRAD_DBG").is_ok() {
                                let f: Vec<f32> = stops_f.iter().take(4).map(|s| s.0).collect();
                                eprintln!("[GRAD-EMIT] angle={} n={} box=({},{},{}x{}) first4={:?}",
                                    angle_deg, stops_f.len(), x, y, w, h, f);
                            }
                            if stops_f.len() > 2 {
                                push_multi_stop_linear_gradient(&mut verts, *x, *y, *w, *h, *angle_deg, &stops_f, *radius);
                            } else {
                                push_gradient(&mut verts, *x, *y, *w, *h, *angle_deg, c0, c1, *radius);
                            }
                        }
                        GradientKind::Radial { cx, cy, radius: grad_r, circle } => {
                            // circle = px KRUH -> CPU annuli tesselace (stejny
                            // radius obe osy) i pro 2 stopy. Mode 6 shader
                            // skaluje na box (ellipse) - pro circle spatne.
                            if stops_f.len() > 2 || *circle {
                                push_multi_stop_radial_gradient(&mut verts, *x, *y, *w, *h, *cx, *cy, *grad_r, &stops_f, *radius);
                            } else {
                                push_radial_gradient(&mut verts, *x, *y, *w, *h, *cx, *cy, *grad_r, c0, c1, *radius);
                            }
                        }
                        GradientKind::Conic { cx, cy, start_angle_deg } => {
                            if stops_f.len() > 2 {
                                push_multi_stop_conic_gradient(&mut verts, *x, *y, *w, *h, *cx, *cy, *start_angle_deg, &stops_f, *radius);
                            } else {
                                push_conic_gradient(&mut verts, *x, *y, *w, *h, *cx, *cy, *start_angle_deg, c0, c1, *radius);
                            }
                        }
                    }
                }
            }
            DisplayCommand::Shadow { x, y, w, h, color, blur, radius, inset, offset_x, offset_y, .. } => {
                if *inset {
                    push_inset_shadow(&mut verts, *x, *y, *w, *h, normalize_color(color), *blur, *radius, *offset_x, *offset_y);
                } else {
                    push_shadow(&mut verts, *x, *y, *w, *h, normalize_color(color), *blur, *radius);
                }
            }
            DisplayCommand::Image { x, y, w, h, src, radius } => {
                if let Some(info) = image_atlas.get(src) {
                    push_image(&mut verts, *x, *y, *w, *h, info.uv0, info.uv1, *radius);
                } else {
                    let placeholder = [0.7, 0.7, 0.75, 1.0];
                    push_rect_rounded(&mut verts, *x, *y, *w, *h, placeholder, *radius);
                }
            }
            DisplayCommand::ImageFit { x, y, w, h, src, radius, object_fit, object_position } => {
                if let Some(info) = image_atlas.get(src) {
                    let img_w = (info.uv1[0] - info.uv0[0]).abs();
                    let img_h = (info.uv1[1] - info.uv0[1]).abs();
                    // Original image aspect ratio (z UV ratios). Pri 0 fallback fill.
                    let src_aspect = if img_h > 0.0 { img_w / img_h } else { 1.0 };
                    let dst_aspect = if *h > 0.0 { *w / *h } else { 1.0 };
                    let (dw, dh) = match object_fit.as_str() {
                        "contain" => {
                            if src_aspect > dst_aspect { (*w, *w / src_aspect) }
                            else { (*h * src_aspect, *h) }
                        }
                        "cover" => {
                            if src_aspect > dst_aspect { (*h * src_aspect, *h) }
                            else { (*w, *w / src_aspect) }
                        }
                        "none" => (*w, *h), // bez znalosti orig px - keep dst
                        "scale-down" => {
                            // min(none, contain). Bez orig px = contain default.
                            if src_aspect > dst_aspect { (*w, *w / src_aspect) }
                            else { (*h * src_aspect, *h) }
                        }
                        _ => (*w, *h), // "fill" default
                    };
                    // Object-position: center default. Parse "left/right/top/bottom/center" + "%".
                    let (px_frac, py_frac) = parse_object_position(object_position);
                    let dx = *x + (*w - dw) * px_frac;
                    let dy = *y + (*h - dh) * py_frac;
                    push_image(&mut verts, dx, dy, dw, dh, info.uv0, info.uv1, *radius);
                } else {
                    let placeholder = [0.7, 0.7, 0.75, 1.0];
                    push_rect_rounded(&mut verts, *x, *y, *w, *h, placeholder, *radius);
                }
            }
            DisplayCommand::BlurredRect { x, y, w, h, color, radius, blur } => {
                push_blurred_rect(&mut verts, *x, *y, *w, *h, normalize_color(color), *radius, *blur);
            }
            DisplayCommand::FilterBegin { .. } | DisplayCommand::FilterEnd
            | DisplayCommand::BackdropFilterBegin { .. } | DisplayCommand::BackdropFilterEnd
            | DisplayCommand::TransformBegin { .. } | DisplayCommand::TransformEnd
            | DisplayCommand::MaskBegin { .. } | DisplayCommand::MaskEnd
            | DisplayCommand::NoScrollShiftBegin | DisplayCommand::NoScrollShiftEnd
            | DisplayCommand::BlendBegin { .. } | DisplayCommand::BlendEnd => {
                // Markers - zpracovava se v render flow / scroll shift, ne ve vertex builderu.
            }
            DisplayCommand::ClippedRect { color, points } => {
                // Ear-clipping triangulace - funguje pro convex i concave.
                let c = normalize_color(color);
                for (a, b, d) in triangulate_polygon(points) {
                    push_triangle(&mut verts, a, b, d, c);
                }
                // Edge AA: 1px feathered fringe smerem ven pro vyhlazeni hran.
                push_polygon_edge_aa(&mut verts, points, c, zoom);
            }
            DisplayCommand::ClippedGradient { points, x, y, w, h, angle_deg, c0, c1 } => {
                push_clipped_linear_gradient(
                    &mut verts, points, *x, *y, *w, *h, *angle_deg,
                    normalize_color(c0), normalize_color(c1));
            }
        }
    }
    verts
}


/// Najdi DOM node v stromu podle Rc::as_ptr hodnoty (use ve cascade).
/// Aplikuje animation/transition hodnoty z style_map na cached layout boxes.
/// Pouziva se kdyz cache je valid pro layout struktury, ale paint props
/// (transform/opacity/color/filter) se menily kazdy frame pres animations.
/// SVG -> RGBA raster pres resvg. Vraci true pri uspechu a uloz do atlasu.
fn try_decode_svg_into_atlas(bytes: &[u8], cache_key: &str,
                              atlas: &mut crate::browser::render::atlas::ImageAtlas) -> bool {
    // Rychly check: SVG bytes obsahuji "<svg" near start. Bez tohoto by
    // resvg parsoval nahodne bytes a vracel chybu pomalu.
    let head: &[u8] = if bytes.len() > 256 { &bytes[..256] } else { bytes };
    let head_str = std::str::from_utf8(head).unwrap_or("");
    if !head_str.contains("<svg") && !head_str.contains("<?xml") {
        return false;
    }
    let opt = usvg::Options::default();
    let tree = match usvg::Tree::from_data(bytes, &opt) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let size = tree.size();
    let w = size.width().ceil().max(1.0) as u32;
    let h = size.height().ceil().max(1.0) as u32;
    // Cap velikost na atlas pulku.
    let max = (IMAGE_ATLAS_SIZE / 2) as u32;
    let (target_w, target_h) = if w > max || h > max {
        let scale = (max as f32) / (w.max(h) as f32);
        (((w as f32) * scale) as u32, ((h as f32) * scale) as u32)
    } else { (w, h) };
    let mut pixmap = match tiny_skia::Pixmap::new(target_w, target_h) {
        Some(p) => p,
        None => return false,
    };
    let scale_x = (target_w as f32) / (w as f32);
    let scale_y = (target_h as f32) / (h as f32);
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // Natural dims (z SVG view box, ne rasterized target). Layout pak
    // pri max-width/height clamp aplikuje proper aspect ratio.
    crate::browser::layout::set_image_natural_dims(cache_key, w as f32, h as f32);
    atlas.add(cache_key, target_w, target_h, pixmap.data());
    true
}

/// Prefix platneho cisla pro input[type=number]: [+-]? cislice* (.cislice*)?
/// ([eE][+-]?cislice*)?. "Prefix" = povoluje rozepsane stavy ("1e", "-",
/// "3.") ale ne "e", "+-", "1ee".
pub(crate) fn is_valid_number_prefix(s: &str) -> bool {
    let mut it = s.chars().peekable();
    if matches!(it.peek(), Some('+') | Some('-')) { it.next(); }
    let mut seen_digit = false;
    while matches!(it.peek(), Some(c) if c.is_ascii_digit()) { it.next(); seen_digit = true; }
    if it.peek() == Some(&'.') {
        it.next();
        while matches!(it.peek(), Some(c) if c.is_ascii_digit()) { it.next(); seen_digit = true; }
    }
    if matches!(it.peek(), Some('e') | Some('E')) {
        if !seen_digit { return false; }
        it.next();
        if matches!(it.peek(), Some('+') | Some('-')) { it.next(); }
        while matches!(it.peek(), Some(c) if c.is_ascii_digit()) { it.next(); }
    }
    it.next().is_none()
}

pub fn apply_paint_animations(box_: &mut crate::browser::layout::LayoutBox,
                           style_map: &crate::browser::cascade::StyleMap) {
    apply_paint_animations_inner(box_, style_map, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
}

/// Rebake paint props na CACHED layout stromu pro nody jejichz computed styly
/// se zmenily mezi framy (hover/focus/active/:valid prepnuti). Od zruseni
/// force re-layoutu pri cascade miss je tohle jedina cesta jak se hover-OFF
/// dostane do stromu: apply_paint_animations prepisuje jen props PRITOMNE
/// ve stylech, takze zmizela hover bg by zustala baknuta navzdy.
///
/// Na rozdil od apply_paint_animations dela RESET na default pri chybejici
/// prop (bg None, transform clear, opacity 1...). Bezi jen na diff nodech
/// (HashMap eq porovnani per node = levne).
pub fn rebake_changed_paint_props(
    box_: &mut crate::browser::layout::LayoutBox,
    new_map: &crate::browser::cascade::StyleMap,
    prev_map: &crate::browser::cascade::StyleMap,
) {
    let node_id = box_.node.as_ref().map(|n| Rc::as_ptr(n) as usize).unwrap_or(0);
    if node_id != 0 {
        let new_s = new_map.get(&node_id);
        let prev_s = prev_map.get(&node_id);
        let differs = match (new_s, prev_s) {
            (Some(a), Some(b)) => !Rc::ptr_eq(a, b) && a.as_ref() != b.as_ref(),
            (None, None) => false,
            _ => true,
        };
        if differs {
            if let Some(styles) = new_s {
                use crate::browser::layout as lay;
                // color (inherited - po propagate vzdy pritomna; pri chybejici nech).
                if let Some(c) = styles.get("color") {
                    if let Some(rgb) = lay::parse_color(c) { box_.text_color = Some(rgb); }
                }
                // background: color varianta. Gradienty nech (vzacny hover case).
                let bgval = styles.get("background-color").or_else(|| styles.get("background"));
                match bgval {
                    Some(v) => {
                        if let Some(rgb) = lay::parse_color(v.trim()) {
                            box_.bg_color = Some(rgb);
                            for layer in box_.backgrounds.iter_mut() {
                                if layer.color.is_some() { layer.color = Some(rgb); }
                            }
                        }
                    }
                    None => {
                        // Hover-off: zadny bg v computed stylech -> reset.
                        box_.bg_color = None;
                        box_.backgrounds.retain(|l| l.color.is_none());
                    }
                }
                match styles.get("border-color") {
                    Some(c) => { if let Some(rgb) = lay::parse_color(c.trim()) { box_.border_color = Some(rgb); } }
                    None => {
                        // border shorthand muze nest barvu - zkus z "border".
                        if let Some(b) = styles.get("border") {
                            for tok in b.split_whitespace() {
                                if let Some(c) = lay::parse_color(tok) { box_.border_color = Some(c); }
                            }
                        }
                    }
                }
                match styles.get("opacity").and_then(|o| o.parse::<f32>().ok()) {
                    Some(v) => box_.opacity = v,
                    None => box_.opacity = 1.0,
                }
                match styles.get("transform") {
                    Some(t) => {
                        box_.transforms = lay::parse_transform_chain(t);
                        box_.transform = box_.transforms.first().cloned();
                    }
                    None => {
                        box_.transforms.clear();
                        box_.transform = None;
                    }
                }
                match styles.get("filter") {
                    Some(f) => box_.filter = lay::parse_filter_chain(f),
                    None => box_.filter.clear(),
                }
                match styles.get("box-shadow") {
                    Some(bs) => box_.box_shadow = lay::parse_box_shadow(bs),
                    None => box_.box_shadow = None,
                }
                match styles.get("text-shadow") {
                    Some(ts) => box_.text_shadow = lay::parse_text_shadow(ts),
                    None => box_.text_shadow.clear(),
                }
                match styles.get("outline-color").and_then(|c| lay::parse_color(c.trim())) {
                    Some(rgb) => box_.outline_color = Some(rgb),
                    None => {}
                }
                if let Some(ow) = styles.get("outline-width") {
                    box_.outline_width = lay::parse_length(ow.trim());
                }
                if let Some(os) = styles.get("outline-style") {
                    box_.outline_style = os.trim().to_string();
                }
                if let Some(ac) = styles.get("accent-color") {
                    box_.accent_color = lay::parse_color(ac.trim());
                }
                // Inherited text barva do primych TEXT-node deti (nemaji vlastni
                // cascade entry; pri buildu ji dedi - cached strom musi taky).
                if let Some(tc) = box_.text_color {
                    for ch in box_.children.iter_mut() {
                        if ch.tag.is_none() && ch.node.is_some() {
                            ch.text_color = Some(tc);
                        }
                    }
                }
            }
        }
    }
    for ch in box_.children.iter_mut() {
        rebake_changed_paint_props(ch, new_map, prev_map);
    }
}

fn apply_paint_animations_inner(box_: &mut crate::browser::layout::LayoutBox,
                                 style_map: &crate::browser::cascade::StyleMap,
                                 parent_width: f32,
                                 parent_height: f32,
                                 parent_delta_x: f32,
                                 parent_delta_y: f32,
                                 // Akumulator layout-applied shiftu od ancestoru.
                                 // shift_subtree(Relative ancestor) posunul nase rect.x
                                 // o jeho offset_left BEZ rebuildu kdyz layout cached.
                                 // Baseline init odecte tento amount aby dostal "kde by
                                 // box byl bez ancestor animace".
                                 parent_layout_dx: f32,
                                 parent_layout_dy: f32) {
    let node_id = box_.node.as_ref().map(|n| Rc::as_ptr(n) as usize).unwrap_or(0);
    // Resolve % border-radius proti finalnim rozmerum boxu (zname az po layoutu).
    // border-radius:50% na 90px boxu = 45px = kruh. Driv % bylo jen u pseudo-elem.
    if box_.border_radius_pct > 0.0 {
        box_.border_radius = box_.border_radius_pct
            * box_.rect.width.min(box_.rect.height);
    }
    let original_width = box_.rect.width;
    // Baseline rect: pri prvni apply zachyti pozici PRED jakoukoli animaci.
    // Dalsi frames cti baseline misto current rect aby se animace neakumulovala.
    //
    // CRITICAL: layout_block aplikuje shift_subtree(child, offset_left, offset_top)
    // pro Position::Relative + offset_left/top. Pri animation tick, cascade map
    // ma left/top zapsane Z aktualniho keyframe progress -> layout pass uz
    // posunul rect. Pak apply_paint_animations rect.x = baseline + parsed_left
    // by aplikoval offset DRUHE = double-shift.
    //
    // Fix: baseline.x = rect.x - layout_applied_offset_left. Tj. baseline =
    // "kde by box byl bez animace". Pak rect.x = baseline + current_left =
    // single shift.
    if box_.anim_baseline.is_none() {
        // 1) Vlastni position:relative offset (z cascade.left/.top - shift_subtree
        //    aplikovan PRI layout buildu) odecist od rect, abychom dostali "kde by
        //    box byl bez animace".
        // 2) Parent_layout_dx: layout shift_subtree pri Relative ancestor
        //    posunul nase rect o ancestor.offset_left BEZ rebuildu (kdyz cache hit).
        //    Tato hodnota neni soucasti rect.x "v plnu" + animation, je to layout-
        //    applied shift z minulosti. Odecist od rect aby baseline = "kde by box
        //    byl bez animace ANI parent animace".
        //    Pozn: parent_delta_x (= parent's our_delta po apply_paint) je BEZ uziti
        //    pro nase baseline init, protoze nase rect.x neni jeste shifted o
        //    parent's current animated delta - jen o parent's layout_dx.
        // offset_left/top odecitat JEN kdyz je layout skutecne aplikoval do
        // rect (Relative: shift_subtree; Absolute/Fixed: pozice z left/top).
        // STICKY je vyjimka: layout ho umistil v normal flow BEZ top offsetu
        // (top aplikuje az apply_sticky pri scrollu). Odecteni by dalo
        // baseline = rect - 48 -> our_delta_y = +48 -> VSECHNY deti sidebaru
        // se kreslily +48px pod hit-test stromem (klik v menu netrefoval).
        let sticky_self = matches!(box_.position, super::layout::Position::Sticky);
        let layout_dx = if sticky_self { 0.0 } else { box_.offset_left.unwrap_or(0.0) };
        let layout_dy = if sticky_self { 0.0 } else { box_.offset_top.unwrap_or(0.0) };
        let mut base = box_.rect;
        base.x -= layout_dx + parent_layout_dx;
        base.y -= layout_dy + parent_layout_dy;
        box_.anim_baseline = Some(base);
    }
    let baseline = box_.anim_baseline.unwrap_or(box_.rect);
    // Inherit parent shift do static children (text node uvnitr position:relative
    // boxu ktery se anim-posunul). Pri animace box.left 0->400 musi text uvnitr
    // shifted tez. Out-of-flow children maji vlastni baseline + abs pozici,
    // parent shift se neaplikuje (handled v left/right/top/bottom branchi nize).
    let is_static_self = matches!(box_.position, super::layout::Position::Static);
    if is_static_self && (parent_delta_x != 0.0 || parent_delta_y != 0.0) {
        box_.rect.x = baseline.x + parent_delta_x;
        box_.rect.y = baseline.y + parent_delta_y;
    }
    if let Some(styles) = style_map.get(&node_id) {
        if let Some(o) = styles.get("opacity") {
            if let Ok(v) = o.parse::<f32>() {
                box_.opacity = v;
            }
        }
        if let Some(c) = styles.get("color") {
            if let Some(rgb) = crate::browser::layout::parse_color(c) {
                box_.text_color = Some(rgb);
            }
        }
        // Animovana bg barva. KLIC: base "background:#335" vytvori backgrounds
        // layer se solid barvou a paint_box pak SKIPNE bx.bg_color
        // (bg_color_handled_by_layers). Takze nestaci nastavit bg_color - musime
        // updatnout i barvu backgrounds layeru, jinak se animace (colorCycle apod.)
        // nezobrazi. Zkousime "background-color" i "background" shorthand (keyframes
        // neexpanduji shorthand); "background" prepise (= ten kam animace zapsala).
        for prop in ["background-color", "background"] {
            if let Some(c) = styles.get(prop) {
                if let Some(rgb) = crate::browser::layout::parse_color(c.trim()) {
                    box_.bg_color = Some(rgb);
                    for layer in box_.backgrounds.iter_mut() {
                        if layer.color.is_some() { layer.color = Some(rgb); }
                    }
                }
            }
        }
        // Animovana border-color (transition/keyframe) - aplikuj na box aby se
        // border animoval. Bez tohoto pri cached layoutu interpolovana hodnota
        // nedosahne box.border_color.
        if let Some(c) = styles.get("border-color") {
            if let Some(rgb) = crate::browser::layout::parse_color(c.trim()) {
                box_.border_color = Some(rgb);
            }
        }
        if let Some(t) = styles.get("transform") {
            box_.transforms = crate::browser::layout::parse_transform_chain(t);
            // Sync i singular transform - jinak pri CACHED layoutu zustane stara
            // hodnota (build_box nebezi, transform je paint prop). Inconsistence
            // singular(stale) vs transforms(current) = layer compose pouzival
            // jiny scale nez raster/texture = scale-hover box MIZEL.
            box_.transform = box_.transforms.first().cloned();
        }
        if let Some(f) = styles.get("filter") {
            box_.filter = crate::browser::layout::parse_filter_chain(f);
        }
        // Dalsi paint props bakovane pri cached layoutu. Od zruseni force
        // re-layoutu pri cascade miss (hover) je tahle fn JEDINA cesta jak se
        // hover/focus/active paint zmeny dostanou do cached stromu.
        if let Some(bs) = styles.get("box-shadow") {
            box_.box_shadow = crate::browser::layout::parse_box_shadow(bs);
        }
        if let Some(ts) = styles.get("text-shadow") {
            box_.text_shadow = crate::browser::layout::parse_text_shadow(ts);
        }
        if let Some(oc) = styles.get("outline-color") {
            if let Some(rgb) = crate::browser::layout::parse_color(oc.trim()) {
                box_.outline_color = Some(rgb);
            }
        }
        if let Some(ow) = styles.get("outline-width") {
            box_.outline_width = crate::browser::layout::parse_length(ow.trim());
        }
        if let Some(os) = styles.get("outline-style") {
            box_.outline_style = os.trim().to_string();
        }
        if let Some(ac) = styles.get("accent-color") {
            box_.accent_color = crate::browser::layout::parse_color(ac.trim());
        }
        if let Some(br) = styles.get("border-radius") {
            let v = br.trim();
            // % resi border_radius_pct blok vyse; tady jen px/em hodnoty.
            if !v.contains('%') {
                box_.border_radius = crate::browser::layout::parse_length(v);
            }
        }
        // background-position animace (animated gradient: keyframes posouvaji
        // pozici pres background-size > 100%). Bake do bg layeru - paint pak
        // remapuje gradient stopy na okno boxu.
        if let Some(bp) = styles.get("background-position") {
            let pos = crate::browser::layout::parse_bg_position(bp);
            if std::env::var("RWE_GRAD_DBG").is_ok() {
                eprintln!("[GRAD] bake bg-position '{}' -> {:?} (layers={})",
                    bp, pos, box_.backgrounds.len());
            }
            for layer in box_.backgrounds.iter_mut() {
                layer.position = pos.clone();
            }
        }
        // INCREMENTAL LAYOUT: aplikuj animovanou width/height na rect kdyz
        // element ma overflow:hidden NEBO position != static (self-contained,
        // ne reflow). Drive typewriter potreboval full layout rebuild kazdy
        // frame, ted jen pricte rect.width upravu.
        let oh_x = box_.overflow_x.hides();
        let oh_y = box_.overflow_y.hides();
        // Sticky NENI oof pro left/top apply: layout ho umistil v normal flow
        // a top: offset aplikuje apply_sticky AZ pri scrollu. Drive Sticky tady
        // dostal top JESTE JEDNOU (rect = baseline + top) -> sidebar menu se
        // KRESLILO +48px pod hit-test stromem = "klikani v menu netrefuje".
        let is_oof = !matches!(box_.position,
            super::layout::Position::Static | super::layout::Position::Sticky);
        // Position-only animace (left/top/right/bottom): aplikuj jako offset
        // od baseline. Bez tohoto by slide-anim (left 0->400) trigeroval
        // hard layout kazdy frame (~12 ms na test.html).
        if is_oof {
            // % offset (left:50% u centered abs) resolve proti parent dimenzi -
            // parse_length(%)=0 jinak resetoval rect.x na baseline (cb origin)
            // -> centered abs element (top:50% left:50% + translate) skocil do
            // leveho horniho rohu (box bez offsetu, text s offsetem).
            let resolve_off = |v: &str, parent: f32| -> f32 {
                let t = v.trim();
                if let Some(pct) = t.strip_suffix('%') {
                    if let Ok(p) = pct.parse::<f32>() { return p / 100.0 * parent; }
                }
                crate::browser::layout::parse_length(t)
            };
            if let Some(l) = styles.get("left") {
                box_.rect.x = baseline.x + resolve_off(l, parent_width);
            } else if let Some(r) = styles.get("right") {
                box_.rect.x = baseline.x - resolve_off(r, parent_width);
            } else {
                box_.rect.x = baseline.x;
            }
            if let Some(t) = styles.get("top") {
                box_.rect.y = baseline.y + resolve_off(t, parent_height);
            } else if let Some(b) = styles.get("bottom") {
                box_.rect.y = baseline.y - resolve_off(b, parent_height);
            } else {
                box_.rect.y = baseline.y;
            }
        }
        if oh_x || oh_y || is_oof {
            // Pouzijeme cached parent_width pro % rozeznavani.
            // Width animace: aktualizuj rect.width pri overflow-x:hidden NEBO
            // pri position != static (out-of-flow nebo relative).
            if oh_x || is_oof {
                if let Some(w) = styles.get("width") {
                    let trimmed = w.trim();
                    if let Some(pct_str) = trimmed.strip_suffix('%') {
                        if let Ok(pct) = pct_str.parse::<f32>() {
                            if parent_width > 0.0 {
                                box_.rect.width = parent_width * (pct / 100.0);
                            }
                        }
                    } else {
                        let px = crate::browser::layout::parse_length(trimmed);
                        if px > 0.0 || trimmed.starts_with('0') {
                            box_.rect.width = px;
                        }
                    }
                }
            }
            if oh_y || is_oof {
                if let Some(h) = styles.get("height") {
                    let trimmed = h.trim();
                    if let Some(pct_str) = trimmed.strip_suffix('%') {
                        if let Ok(pct) = pct_str.parse::<f32>() {
                            // Pct height proti PARENT HEIGHT (ne parent_width - bug
                            // pred fixem). Pri parent_height=0 (indefinite) skip
                            // override - layout uz spravne resolved.
                            if parent_height > 0.0 {
                                box_.rect.height = parent_height * (pct / 100.0);
                            }
                        }
                    } else {
                        let px = crate::browser::layout::parse_length(trimmed);
                        if px > 0.0 || trimmed.starts_with('0') {
                            box_.rect.height = px;
                        }
                    }
                }
            }
        }
    }
    let _ = original_width;
    let our_width = box_.rect.width;
    let our_height = box_.rect.height;
    // Delta = current rect - baseline rect. Predat do recursive apply
    // aby static children shifted spolu se self.
    let our_delta_x = box_.rect.x - baseline.x;
    let our_delta_y = box_.rect.y - baseline.y;
    // Akumulator layout_dx pro descendants. Layout umistil deti na pozici
    // PARENTA vc. jeho left/top offsetu (Relative shift_subtree; Absolute/Fixed
    // rect = cb + offset primo). Dite tedy ma v layoutu uz offset zapocteny ->
    // jeho baseline ho musi odecist, jinak apply_paint left/top re-aplikuje
    // parent offset DRUHE (centered abs: text vyletel o +offset mimo box).
    let our_layout_dx = parent_layout_dx + match box_.position {
        super::layout::Position::Relative
        | super::layout::Position::Absolute
        | super::layout::Position::Fixed => box_.offset_left.unwrap_or(0.0),
        _ => 0.0,
    };
    let our_layout_dy = parent_layout_dy + match box_.position {
        super::layout::Position::Relative
        | super::layout::Position::Absolute
        | super::layout::Position::Fixed => box_.offset_top.unwrap_or(0.0),
        _ => 0.0,
    };
    for ch in &mut box_.children {
        apply_paint_animations_inner(ch, style_map, our_width, our_height,
            our_delta_x, our_delta_y, our_layout_dx, our_layout_dy);
    }
}

/// Eval JS via bytecode VM s globals z Interpreter env. Pri compile failure
/// nebo runtime error vrati Err s message. Pred eval definuje `$0` (selected
/// DevTools element) jako DomNode proxy v globalu.
fn console_eval_via_vm(src: &str, interp: &crate::interpreter::Interpreter, selected_node_id: Option<usize>) -> Result<crate::interpreter::JsValue, String> {
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::interpreter::bytecode::{compile_program, VM};
    use crate::interpreter::JsValue;

    // Definuj $0 = selected DOM node (or undefined).
    let dollar0 = match selected_node_id {
        Some(id) => {
            let root = std::rc::Rc::clone(&interp.document.borrow().root);
            match crate::devtools::model::elements::find_node_by_id(&root, id) {
                Some(n) => JsValue::DomNode(n),
                None => JsValue::Undefined,
            }
        }
        None => JsValue::Undefined,
    };
    interp.global.borrow_mut().define("$0", dollar0);

    let lex = Lexer::parse_str(src, "<console>").map_err(|e| format!("Lexer: {:?}", e))?;
    let mut parser = Parser::new(lex.tokens.clone());
    let program = parser.parse().map_err(|e| format!("Parser: {:?}", e))?;
    let code = compile_program(&program.body).map_err(|e| format!("Compile: {}", e))?;
    let mut vm = VM::with_env(interp.global.clone());
    vm.run(&code).map_err(|e| format!("Runtime: {}", e))
}

/// Parse CSS object-position do (x_frac, y_frac) v range [0,1].
/// "center" / "left" / "right" / "top" / "bottom" / "50%" / "0% 0%" / "left top".
fn parse_object_position(s: &str) -> (f32, f32) {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "center" { return (0.5, 0.5); }
    let toks: Vec<&str> = trimmed.split_whitespace().collect();
    let parse_axis = |t: &str, axis: char| -> f32 {
        match t {
            "left" => 0.0, "right" => 1.0, "top" => 0.0, "bottom" => 1.0,
            "center" => 0.5,
            _ => {
                if let Some(p) = t.strip_suffix('%') {
                    p.parse::<f32>().ok().map(|v| v / 100.0).unwrap_or(0.5)
                } else {
                    let _ = axis;
                    0.5
                }
            }
        }
    };
    match toks.len() {
        1 => {
            let v = parse_axis(toks[0], 'x');
            // Single token: "top"/"bottom" -> y, jine -> x.
            match toks[0] {
                "top" | "bottom" => (0.5, v),
                _ => (v, 0.5),
            }
        }
        2 => (parse_axis(toks[0], 'x'), parse_axis(toks[1], 'y')),
        _ => (0.5, 0.5),
    }
}

/// Walk DOM, najdi elementy matchujici selector + vykresli orange outline pres
/// jejich layout box. Pouzite pro match-preview toggle ctverec v styles panelu.
fn paint_match_preview_recursive(
    list: &mut Vec<DisplayCommand>,
    node: &Rc<crate::browser::dom::NodeData>,
    sel: &super::css_parser::Selector,
    layout_root: &super::layout::LayoutBox,
    scroll_y: f32,
) {
    if super::cascade::matches_selector(node, sel) {
        let node_ptr = Rc::as_ptr(node) as usize;
        if let Some((rx, ry, rw, rh)) = super::devtools_panel::find_box_rect_by_id(layout_root, node_ptr, scroll_y) {
            let y = ry;
            list.push(DisplayCommand::Rect {
                x: rx, y, w: rw, h: rh,
                color: [255, 165, 0, 60], radius: 0.0,
            });
            list.push(DisplayCommand::Rect {
                x: rx, y, w: rw, h: 2.0,
                color: [255, 165, 0, 220], radius: 0.0,
            });
            list.push(DisplayCommand::Rect {
                x: rx, y: y + rh - 2.0, w: rw, h: 2.0,
                color: [255, 165, 0, 220], radius: 0.0,
            });
            list.push(DisplayCommand::Rect {
                x: rx, y, w: 2.0, h: rh,
                color: [255, 165, 0, 220], radius: 0.0,
            });
            list.push(DisplayCommand::Rect {
                x: rx + rw - 2.0, y, w: 2.0, h: rh,
                color: [255, 165, 0, 220], radius: 0.0,
            });
        }
    }
    for child in node.children.borrow().iter() {
        paint_match_preview_recursive(list, child, sel, layout_root, scroll_y);
    }
}

pub fn find_node_by_ptr(root: &Rc<crate::browser::dom::NodeData>, ptr: usize) -> Option<Rc<crate::browser::dom::NodeData>> {
    if Rc::as_ptr(root) as usize == ptr {
        return Some(Rc::clone(root));
    }
    for child in root.children.borrow().iter() {
        if let Some(found) = find_node_by_ptr(child, ptr) {
            return Some(found);
        }
    }
    None
}

/// Vyber vsechny `node_id` v subtree pod node `target_id`. Pouziva ElementRow
/// flat list - depth indikuje strukturu.
fn collect_subtree_ids(
    rows: &[crate::devtools::model::elements::ElementRow],
    target_id: usize,
    out: &mut Vec<usize>,
) {
    let Some(start) = rows.iter().position(|r| r.node_id == target_id) else { return };
    let target_depth = rows[start].depth;
    out.push(target_id);
    for r in rows.iter().skip(start + 1) {
        if r.depth <= target_depth { break; }
        out.push(r.node_id);
    }
}



/// Push rect with rounded corners (SDF rendering).


/// Spusti okno s dodatecnymi options.
/// - `current_html_path`: pri Some umozni reload pres drag-drop (relativni paths v HTML)
/// - `auto_devtools`: pri true vygeneruje devtools.html a otevre v OS default browser
/// - `base_url`: page URL pro relative resolution (http(s)://... nebo file:///...).
///   Pri None se odvodi z `current_html_path`.
pub fn run_window_with_options(html: String, css: String, current_html_path: Option<std::path::PathBuf>, auto_devtools: bool, base_url: Option<String>) -> Result<(), String> {
    run_window_inner(html, css, current_html_path, auto_devtools, base_url)
}

fn run_window_inner(html: String, css: String, current_html_path: Option<std::path::PathBuf>, auto_devtools: bool, base_url: Option<String>) -> Result<(), String> {
    use winit::application::ApplicationHandler;
    use winit::event::{WindowEvent, MouseButton, ElementState};
    use winit::event_loop::ActiveEventLoop;
    use winit::window::{Window, WindowId};
    use winit::keyboard::{Key, NamedKey};

    // Mapuje winit logical_key na JS KeyboardEvent.key string (pro page keydown/
    // keyup dispatch do webview). Shoduje se s shell/app.rs konverzi.
    fn logical_key_to_js(key: &Key) -> Option<String> {
        Some(match key {
            Key::Named(NamedKey::Enter) => "Enter".into(),
            Key::Named(NamedKey::Escape) => "Escape".into(),
            Key::Named(NamedKey::Backspace) => "Backspace".into(),
            Key::Named(NamedKey::Tab) => "Tab".into(),
            Key::Named(NamedKey::Delete) => "Delete".into(),
            Key::Named(NamedKey::ArrowLeft) => "ArrowLeft".into(),
            Key::Named(NamedKey::ArrowRight) => "ArrowRight".into(),
            Key::Named(NamedKey::ArrowUp) => "ArrowUp".into(),
            Key::Named(NamedKey::ArrowDown) => "ArrowDown".into(),
            Key::Named(NamedKey::Home) => "Home".into(),
            Key::Named(NamedKey::End) => "End".into(),
            Key::Named(NamedKey::PageUp) => "PageUp".into(),
            Key::Named(NamedKey::PageDown) => "PageDown".into(),
            Key::Named(NamedKey::Space) => " ".into(),
            Key::Character(s) => s.to_string(),
            _ => return None,
        })
    }

    // Mapuje CSS `cursor` property hodnotu na winit CursorIcon. None pro
    // `auto`/neznamé (= fallback na InteractiveKind/text/default).
    fn css_cursor_to_icon(v: &str) -> Option<winit::window::CursorIcon> {
        use winit::window::CursorIcon as C;
        // url(...) fallback hodnoty: vezmi posledni keyword za carkou.
        let kw = v.split(',').next_back().unwrap_or(v).trim().to_ascii_lowercase();
        Some(match kw.as_str() {
            "auto" => return None,
            "default" => C::Default,
            "pointer" => C::Pointer,
            "text" | "vertical-text" => C::Text,
            "move" => C::Move,
            "grab" => C::Grab,
            "grabbing" => C::Grabbing,
            "not-allowed" | "no-drop" => C::NotAllowed,
            "crosshair" | "cell" => C::Crosshair,
            "help" => C::Help,
            "wait" => C::Wait,
            "progress" => C::Progress,
            "none" => return None, // skryti kurzoru neresime - nech default
            "ew-resize" => C::EwResize,
            "ns-resize" => C::NsResize,
            "nesw-resize" => C::NeswResize,
            "nwse-resize" => C::NwseResize,
            "col-resize" => C::ColResize,
            "row-resize" => C::RowResize,
            "n-resize" | "s-resize" => C::NsResize,
            "e-resize" | "w-resize" => C::EwResize,
            "ne-resize" | "sw-resize" => C::NeswResize,
            "nw-resize" | "se-resize" => C::NwseResize,
            "zoom-in" => C::ZoomIn,
            "zoom-out" => C::ZoomOut,
            "copy" => C::Copy,
            "alias" => C::Alias,
            "context-menu" => C::ContextMenu,
            "all-scroll" => C::AllScroll,
            "wait-progress" => C::Progress,
            _ => return None,
        })
    }

    struct App {
        // html/css/base_url/current_path fields smazany (polarity invert
        // kompletni). Initial data drzeny v `initial: Option<InitialData>`
        // do prvniho sync_webview, pak primary v webview.
        initial: Option<(String, String, Option<String>, Option<std::path::PathBuf>)>,
        /// Ring buffer poslednich N frame timing pro FPS counter overlay.
        /// Default 60 frame window. Render hori v ms, FPS = 1000 / avg.
        frame_times_ms: std::collections::VecDeque<f32>,
        /// Show FPS counter overlay (Ctrl+Shift+F nebo always-on dev mode).
        show_fps: bool,
        /// Wall-clock cas predchoziho render() volani - pro frame cadence FPS.
        last_render_instant: Option<std::time::Instant>,
        /// Throttle update window title FPS (set_title je syscall, ne kazdy frame).
        frames_since_title: u32,
        /// COALESCING hoveru: posledni mouse pozice (viewport coords) cekajici na
        /// zpracovani. CursorMoved jen ulozi pozici; webview hover pipeline
        /// (hit-test + JS dispatch + :hover cascade) se spusti JEDNOU za frame v
        /// renderu z teto pozice. Bez toho se kazdy z desitek CursorMoved/frame
        /// zpracoval plne -> queue buildup = hover lag + zbytecna prace.
        pending_hover: Option<(f32, f32)>,
        // animation_origin / animation_pause_start / paused_animation_nodes /
        // animations_scrubber_drag / paused_node_styles fields vsechny smazany
        // Phase 99: effective animace cas drzi webview.animation_origin. App-side
        // copies byly izolovane (menia jen sami sebe, ne render). Devtools
        // animations panel scrubber/pause/speed/restart bude per-WebView v dalsi fazi.
        // painted_text_runs field smazany Phase 99: nikdy zapisovan na App vrstve
        // (webview.painted_text_runs je primary). Delegate getter pres webview.
        /// Async jobs registry - background work (file IO, image lazy load).
        /// Drain per frame; on_complete callbacks beha v main thread (mohou
        /// modifikovat Interpreter pres Rc<RefCell>).
        async_jobs: crate::browser::async_jobs::AsyncJobsRegistry,
        // bookmark_picker field smazany (Session N+22) - shell concern.
        // current_path + base_url smazany (polarity invert) - drzeny v webview.
        // history / history_idx fields smazany N+22 - back/forward shell concern.
        // (profile history persist v ~/.rwe stale aktualizuje pres navigate_url).
        /// Otevreny <select> dropdown - hodnota = (node ptr, anchor x/y/w).
        open_select: Option<(usize, f32, f32, f32)>,
        /// Po startu otevri devtools.html v default browseru.
        auto_devtools: bool,
        window: Option<std::sync::Arc<Window>>,
        renderer: Option<Renderer>,
        // layout_root field smazany Phase 99: nikdy zapisovan (vzdy None od initu);
        // 11 read sites byly dead branches. Delegate na webview.last_layout_root().
        // interpreter field smazany Phase 99: nikdy zapisovan po polarity invert
        // (webview.interpreter je primary). 38 read sites migrace na self.interp().
        mouse_x: f32,
        mouse_y: f32,
        /// Koalescovany resize - Resized event jen uklada, realny realloc
        /// (surface + RT + webview target) probehne 1x v RedrawRequested.
        pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
        // scroll_x/y fields smazany (polarity invert) - read pres
        // self.scroll_y() method delegate webview.
        // start_time + prev_style_map fields smazany Phase 99: nikdy ctene
        // (jen self-write). start_time slouzil jako source pro animation_origin.
        // active_animations/animation_iterations/active_transitions fields smazany
        // Phase 99: vsechny jen clear()/is_empty() (vzdy prazdne). Animace
        // tracking je per-WebView (webview.cascade).
        /// DevTools state (theme, tab, panel_h, panel_open, elements, console, network,
        /// sources, performance, focus, context_menu, inspect_mode, frame_counter).
        devtools: crate::devtools::DevToolsState,
        /// True kdyz user drze LMB na resize grip a tahne.
        devtools_resizing: bool,
        /// Double-click detect: cas posledniho LMB pressed + pozice.
        last_click_time: Option<std::time::Instant>,
        last_click_pos: (f32, f32),
        /// Sdileny debugger state pres mezi UI a JS worker thread (foundation
        /// pro budouci Arc rework - aktualne primary path je single-thread).
        shared_debugger: crate::interpreter::SharedDebugger,
        /// Continue signal pri pause v worker thread.
        continue_signal: crate::interpreter::ContinueSignal,
        /// Hybrid debug-mode runner. Some pri devtools open + breakpoints set +
        /// page reload. Worker thread holds vlastni Interpreter, eval JS s
        /// blocking pause podpora. UI thread polluje events per frame.
        debug_runner: Option<crate::devtools::debug_runner::DebugRunner>,
        // zoom field smazany (polarity invert) - read pres self.zoom() method
        // delegate webview. Set pres self.set_zoom(z).
        /// Trackovany state Ctrl/Shift/Alt pro zoom shortcut detection.
        modifiers: winit::keyboard::ModifiersState,
        // find_open/find_query/find_match_idx + addr_open/addr_input smazany
        // (Session N+22) - shell concerns. Find bar + addr bar v Phase 99
        // patri do shell crate.
        // find_query / find_match_idx / addr_input fields smazany N+22 - shell concerns.
        /// Smooth scroll target. Render tick interpoluje scroll_y -> target.
        // scroll_target_x/y fields smazany (polarity invert) - read pres
        // self.scroll_target_y() method delegate webview.
        /// Text selection: anchor (mouse down pos), current (mouse drag pos).
        /// Pri obou Some + dragging = aktivni rect highlight. Ctrl+C extrahuje
        /// text uvnitr.
        /// Main page scrollbar drag - true pri LMB hold na vertical/horizontal thumb.
        page_scrollbar_v_drag: bool,
        page_scrollbar_h_drag: bool,
        // tab_drag_idx, tab_drag_x_start, status_hover_url, shell_tab_tooltip,
        // shell_tab_hover_pending smazany N+22 - shell concerns.
        // shortcuts_overlay_open / reading_mode_on / bookmarks_bar_visible
        // fields smazany (Session N+22) - shell concerns mimo engine.
        // shell_mode field smazan (Session N+22). Engine vzdy renderuje naked
        // viewport; chrome bar zustal v shell crate (Phase 99 - presun chrome
        // paint do shell::ShellApp).
        // shell_chrome_h field smazany (Session N+22) - vzdy 0.0 v engine.
        // Use sites volaji shell_chrome_h_active() ktery vrati 0.0.
        // title field smazany (Phase 99 polarity invert step) - read pres
        // self.webview.as_ref().map(|w| w.title()).
        /// Embeddable WebView mirror - sdileny page state s shell crate +
        /// power users. Sync'nuty z App pri reload + scroll/zoom changes.
        /// Phase 4a = sync (App primary, WebView side-effect populated).
        /// Phase 5 = WebView authoritative, App reads delegated.
        pub(super) webview: Option<crate::embed::WebView>,
    }

    impl App {
        /// Pristup k embedded WebView (read-only). Vrati None pred prvnim
        /// loadem (init v `resumed`). Pouziti: shell crate + power users
        /// chteji DOM/JS state bez sahnuti do interniho App stavu.
        pub fn webview(&self) -> Option<&crate::embed::WebView> {
            self.webview.as_ref()
        }

        // -- Polarity invert helpers (App reads webview state) ---------------

        /// Zoom factor pres webview (1.0 = 100%).
        fn zoom(&self) -> f32 {
            self.webview.as_ref().map(|w| w.zoom()).unwrap_or(1.0)
        }

        /// Set zoom pres webview + invalidate App layout cache.
        fn set_zoom(&mut self, z: f32) {
            if let Some(w) = self.webview.as_mut() {
                w.set_zoom(z);
            }
        }

        /// Smooth scroll target Y (webview-vlastnen po polarity invert).
        fn scroll_target_y(&self) -> f32 {
            self.webview.as_ref().map(|w| w.scroll_target_y).unwrap_or(0.0)
        }
        fn set_scroll_target_y(&mut self, y: f32) {
            if let Some(w) = self.webview.as_mut() { w.scroll_target_y = y; }
        }
        fn scroll_target_x(&self) -> f32 {
            self.webview.as_ref().map(|w| w.scroll_target_x).unwrap_or(0.0)
        }
        fn set_scroll_target_x(&mut self, x: f32) {
            if let Some(w) = self.webview.as_mut() { w.scroll_target_x = x; }
        }

        fn scroll_y(&self) -> f32 {
            self.webview.as_ref().map(|w| w.scroll_y).unwrap_or(0.0)
        }
        fn set_scroll_y(&mut self, y: f32) {
            if let Some(w) = self.webview.as_mut() { w.scroll_y = y; }
        }
        fn scroll_x(&self) -> f32 {
            self.webview.as_ref().map(|w| w.scroll_x).unwrap_or(0.0)
        }
        fn set_scroll_x(&mut self, x: f32) {
            if let Some(w) = self.webview.as_mut() { w.scroll_x = x; }
        }

        /// Raw HTML source delegate webview (polarity invert).
        fn html(&self) -> &str {
            self.webview.as_ref().map(|w| w.html()).unwrap_or("")
        }
        fn css(&self) -> &str {
            self.webview.as_ref().map(|w| w.css()).unwrap_or("")
        }
        fn base_url(&self) -> Option<String> {
            self.webview.as_ref().and_then(|w| w.base_url().map(|s| s.to_string()))
        }
        fn current_path(&self) -> Option<std::path::PathBuf> {
            self.webview.as_ref().and_then(|w| w.local_path().cloned())
        }


        /// Synchronizuje mirror WebView z App primary state. Vola se po kazdem
        /// reload / navigace. Phase 4a sync (App primary, WebView side-effect);
        /// Phase 5 obrati polarity (WebView primary, App reads).
        ///
        /// WebView interpreter je vlastni instance - JS spousteni je idempotentni
        /// (vola WebView::run_scripts pres load_html), state App.interpreter NE
        /// sdileny (Interpreter neni Clone bezpecne). Pro Phase 4a je OK 2x
        /// inicializovat scripts - dual run navic je u inline pages levne.
        /// Build novy WebView s realnymi GPU resources (kdyz Renderer ready)
        /// + load HTML/CSS s zachovanim zoom/scroll z pripadne predchazejci
        /// instance. Volane pri kazdem reload (initial load, drag-drop file,
        /// form POST response, navigate_url).
        fn sync_webview(
            &mut self,
            html: &str,
            css: &str,
            base_url: Option<String>,
            path: Option<std::path::PathBuf>,
        ) {
            let engine = if let Some(r) = &self.renderer {
                std::sync::Arc::new(crate::embed::Engine::new(
                    std::sync::Arc::new(r.device.clone()),
                    std::sync::Arc::new(r.queue.clone()),
                ))
            } else {
                std::sync::Arc::new(crate::embed::Engine::new_headless())
            };
            let (vw, vh, sf) = if let Some(r) = &self.renderer {
                let sf = r.scale_factor.max(0.01);
                let lw = ((r.config.width as f32 / sf) as u32).max(1);
                let lh = ((r.config.height as f32 / sf) as u32).max(1);
                (lw, lh, sf)
            } else {
                (1280u32, 900u32, 1.0f32)
            };
            let prev_zoom = self.zoom();
            let prev_scroll = (self.scroll_x(), self.scroll_y());
            let mut wv = crate::embed::WebView::new(engine, vw, vh);
            wv.resize(vw, vh, sf);
            wv.set_zoom(prev_zoom);
            wv.set_scroll(prev_scroll.0, prev_scroll.1);
            wv.set_local_path(path);
            let _ = wv.load_html(html, css, base_url);
            self.webview = Some(wv);
        }
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let (init_html, init_css, init_base, init_path) = self.initial.take()
                .unwrap_or_default();
            let title = match &init_path {
                Some(p) => format!("Rust Web Engine - {}", p.display()),
                None => "Rust Web Engine".to_string(),
            };
            let attrs = Window::default_attributes()
                .with_title(title)
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 900.0))
                .with_min_inner_size(winit::dpi::LogicalSize::new(400.0, 300.0));
            let window = std::sync::Arc::new(event_loop.create_window(attrs).unwrap());
            self.window = Some(window.clone());
            self.renderer = Some(Renderer::new(window.clone()));

            // Authoritative WebView: vytvori se v sync_webview + spousti scripts.
            self.sync_webview(&init_html, &init_css, init_base, init_path);
            // Webview drzi interpreter primarne (polarity invert).

            self.render();

            // Auto-open devtools.html po startu
            if self.auto_devtools {
                self.regenerate_and_open_devtools();
            }

            println!("[okno] F12 = otevri/regen DevTools | drag-drop HTML soubor pro reload");
            window.request_redraw();
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            // Kontinualni redraw pri aktivnich CSS @keyframes/transitions /
            // smooth scrollu / pending setInterval. Bez about_to_wait spolehal
            // engine jen na request_redraw() uvnitr RedrawRequested, coz winit
            // pri ControlFlow::Wait nekdy NEzopakuje (self-request behem zpracovani
            // RedrawRequested se zkoalescuje) -> @keyframes animace "zamrznou"
            // dokud neprijde dalsi event (scroll/klik). about_to_wait se vola
            // spolehlive po kazdem batchi events = pumpuje dalsi frame. Stejny
            // pattern jako shell crate. has_active_animations je naplnene v
            // render_via (= predchozi frame), takze loop se sam udrzuje.
            let scroll_anim = (self.scroll_y() - self.scroll_target_y()).abs() > 0.5
                || (self.scroll_x() - self.scroll_target_x()).abs() > 0.5;
            let page_anim = self.webview.as_ref()
                .map(|w| w.has_active_animations() || w.has_pending_intervals()
                    || w.has_pending_raf() || w.has_pending_timeouts())
                .unwrap_or(false);
            if scroll_anim || page_anim {
                // Kontinualni redraw pri aktivni animaci. Pacing resi Fifo present
                // mode (vsync): present() blokuje thread do dalsiho refreshe =
                // prirozeny frame cap na refresh rate + NIZKE CPU (thread spi v
                // GPU driveru, ne busy-spin). Driv preferovany Mailbox/Immediate
                // present je non-blocking -> request_redraw v kazde iteraci =
                // 100% CPU spin ("vykon nestal za moc"). about_to_wait je jediny
                // animacni pump (RedrawRequested se uz sam nerequestuje).
                if let Some(w) = &self.window { w.request_redraw(); }
            } else {
                // Zadna animace -> klasicky Wait (spi do dalsiho inputu, 0% CPU).
                _event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    // Session save smazany N+22 (multi-tab shell concern).
                    event_loop.exit();
                }
                WindowEvent::Resized(size) => {
                    // COALESCING (mirror shell N+32): jen uloz pending size +
                    // request_redraw. Realny resize (surface configure + 4x RT
                    // realloc + webview target realloc) probehne 1x v
                    // RedrawRequested. Drive SYNC v eventu -> drag-resize =
                    // ~230ms/step (SetWindowPos blokoval na surface rebuildu),
                    // winit dorucuje Resized per pixel kroku.
                    if std::env::var("RWE_RESIZE_DBG").is_ok() {
                        eprintln!("[RSZ] Resized event {}x{} t={:?}", size.width, size.height, std::time::Instant::now());
                    }
                    self.pending_resize = Some(size);
                    // PERF: layout cache invalidate jen pri viewport-dependent CSS.
                    // Bez @media/vh layout je viewport-independent (content size
                    // urcen z elements, ne window) -> kesovany layout zustava
                    // valid pri resize, render jen prepocita scrollbars + shifts.
                    // PERF: nevolame self.render() inline - winit posila pri
                    // startu vicero Resized eventu (initial + DPI + final).
                    // request_redraw() je coalescovany -> jeden RedrawRequested
                    // = jeden layout pass misto N pass.
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    if let Some(r) = &mut self.renderer {
                        r.scale_factor = scale_factor as f32;
                    }
                    // Scale factor zmena dela DPI shift glyph atlas - layout
                    // (logical px) zustava stejny.
                    let _ = scale_factor;
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    // Mouse position je physical px. Logical = physical / (zoom * scale_factor).
                    let scale = self.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
                    let new_x = (position.x as f32) / (self.zoom() * scale) + self.scroll_x();
                    let new_y = (position.y as f32) / (self.zoom() * scale) + self.scroll_y();
                    // Skip update kdyz se pozice nezmenila (deduplicate winit spam).
                    if (new_x - self.mouse_x).abs() < 0.5 && (new_y - self.mouse_y).abs() < 0.5 {
                        return;
                    }
                    self.mouse_x = new_x;
                    self.mouse_y = new_y;
                    // Scrollbar thumb drag routing: deleguj move do webview
                    // (prepocita scroll dle thumb pozice) + skip selection/hover.
                    if self.page_scrollbar_v_drag {
                        let vp_x = new_x - self.scroll_x();
                        let vp_y = new_y - self.scroll_y();
                        if let Some(wv) = self.webview.as_mut() {
                            let _ = wv.handle_input(crate::embed::InputEvent::MouseMove {
                                x: vp_x, y: vp_y,
                                modifiers: Default::default(),
                                coalesced: Vec::new(),
                            });
                        }
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                    // Tab drag reorder smazany N+22.
                    // Resize drag: aktualizuj panel size dle dock position.
                    if self.devtools_resizing {
                        use crate::devtools::profile::DockPosition;
                        let viewport_w = self.viewport_w_logical();
                        let viewport_h = self.viewport_h_logical();
                        let raw_x = new_x - self.scroll_x();
                        let raw_y = new_y - self.scroll_y();
                        let new_size = match self.devtools.dock_position {
                            DockPosition::Bottom | DockPosition::PopupWindow =>
                                (viewport_h - raw_y).max(60.0).min(viewport_h * 0.9),
                            DockPosition::Top =>
                                raw_y.max(60.0).min(viewport_h * 0.9),
                            DockPosition::Left =>
                                raw_x.max(180.0).min(viewport_w * 0.9),
                            DockPosition::Right =>
                                (viewport_w - raw_x).max(180.0).min(viewport_w * 0.9),
                        };
                        self.devtools.panel_h = new_size;
                        self.render();
                        return;
                    }
                    // Animations scrubber drag handler smazany Phase 99: efektivni
                    // animation_origin je na webview.animation_origin (App-side dead).
                    // Scrubber rework potrebuje webview.set_animation_origin API.
                    // Main page scrollbar drag - layout_root dead na App vrstve,
                    // dragging bude per-WebView v dalsi fazi. page_scrollbar_v/h_drag
                    // flag se uz neda nastavit (hit-test smazany), tak no-op.
                    // Splitter drag: aktualizuj split_x v logical px.
                    if self.devtools.elements.dragging_split {
                        let viewport_w = self.viewport_w_logical();
                        let max_x = viewport_w - self.devtools.side_panel_w - 200.0;
                        self.devtools.elements.split_x = (self.mouse_x - self.scroll_x()).clamp(200.0, max_x);
                        self.render();
                        return;
                    }
                    // Side panel splitter drag (per dock: convert mouse pos
                    // do panel-local coords).
                    if self.devtools.elements.dragging_side_split {
                        use crate::devtools::profile::DockPosition;
                        let viewport_w = self.viewport_w_logical();
                        let mx_screen = self.mouse_x - self.scroll_x();
                        let (px, _py, pw, _ph) = self.panel_rect_logical();
                        // Pri Bottom/Top: panel_w = viewport_w. Mouse_x v panel_x..panel_x+panel_w.
                        // styles_end = mouse_x_local; side_panel_w = panel_w - mouse_x_local.
                        let local_mx = mx_screen - px;
                        let max_w = (pw - 400.0).max(181.0);
                        let new_w = match self.devtools.dock_position {
                            DockPosition::Bottom | DockPosition::Top | DockPosition::PopupWindow =>
                                (pw - local_mx).clamp(180.0, max_w),
                            DockPosition::Left | DockPosition::Right =>
                                (pw - local_mx).clamp(180.0, max_w),
                        };
                        if new_w > 0.0 {
                            self.devtools.side_panel_w = new_w;
                        }
                        let _ = viewport_w;
                        self.render();
                        return;
                    }
                    // Scrollbar drag: prevod mouse_y na scroll_y.
                    if let Some(target) = self.devtools.elements.dragging_scrollbar {
                        use crate::devtools::ScrollTarget;
                        let viewport_h = self.viewport_h_logical();
                        let panel_h = self.panel_h_logical();
                        let panel_y = viewport_h - panel_h;
                        let body_y = panel_y + 4.0 + 30.0
                            + if self.devtools.elements.search.open { 28.0 } else { 0.0 };
                        let body_h = panel_h - 4.0 - 30.0
                            - if self.devtools.elements.search.open { 28.0 } else { 0.0 };
                        let raw_y = self.mouse_y - self.scroll_y();
                        let frac = ((raw_y - body_y) / body_h).clamp(0.0, 1.0);
                        match target {
                            ScrollTarget::ElementsTree => {
                                let total_h = self.devtools.elements.rows.len() as f32 * 18.0;
                                let max_scroll = (total_h - body_h).max(0.0);
                                self.devtools.elements.scroll_y = frac * max_scroll;
                            }
                            ScrollTarget::StylesPane => {
                                let total_h = self.devtools.styles.estimate_total_h();
                                let max_scroll = (total_h - body_h).max(0.0);
                                self.devtools.styles.scroll_y = frac * max_scroll;
                            }
                            _ => {}
                        }
                        self.render();
                        return;
                    }
                    // Page hover -> deleguj MouseMove do webview.handle_input
                    // (dispatch JS mouseover/mouseenter/mousemove/mouseout/mouseleave
                    // + inline on* handlery + update hovered_node_local pro CSS
                    // :hover cascade). Drive se webview pipeline pro NORMALNI pohyb
                    // NEvolala (jen pri scrollbar dragu) -> page hover (CSS i JS)
                    // byl uplne mrtvy. Routujeme I behem selection dragu (browsery
                    // dispatchuji mousemove pri vyberu; navic canvas drag-kresleni
                    // potrebuje mousemove i kdyz App soubezne drzi page-selection).
                    let hov_panel_h = self.panel_h_logical();
                    let hov_viewport_h = self.viewport_h_logical();
                    let hov_raw_y = self.mouse_y - self.scroll_y();
                    let hov_in_devtools = self.devtools.panel_open
                        && hov_raw_y >= hov_viewport_h - hov_panel_h;
                    if !hov_in_devtools {
                        // COALESCING: jen ulozim posledni pozici; webview hover
                        // pipeline (hit-test + JS mouseover/move dispatch + :hover
                        // cascade) se spusti 1x/frame v render_via_webview. Desitky
                        // CursorMoved/frame se tim slouci -> bez queue lagu.
                        let vp_x = self.mouse_x - self.scroll_x();
                        self.pending_hover = Some((vp_x, hov_raw_y));
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }
                    self.update_hover();
                    if self.page_sel_dragging() {
                        self.page_sel_update_current((self.mouse_x, self.mouse_y));
                        self.render();
                    } else if self.open_select.is_some() {
                        self.render();
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                    let was_scrollbar_drag = self.page_scrollbar_v_drag || self.page_scrollbar_h_drag;
                    if self.devtools_resizing {
                        self.devtools_resizing = false;
                        self.render();
                    }
                    if self.devtools.elements.dragging_split {
                        self.devtools.elements.dragging_split = false;
                        self.render();
                    }
                    if self.devtools.elements.dragging_side_split {
                        self.devtools.elements.dragging_side_split = false;
                        self.render();
                    }
                    if self.devtools.elements.dragging_scrollbar.is_some() {
                        self.devtools.elements.dragging_scrollbar = None;
                        self.render();
                    }
                    if self.page_scrollbar_v_drag || self.page_scrollbar_h_drag {
                        // Ukonci scrollbar thumb drag ve webview.
                        let vp_x = self.mouse_x - self.scroll_x();
                        let vp_y = self.mouse_y - self.scroll_y();
                        if let Some(wv) = self.webview.as_mut() {
                            let _ = wv.handle_input(crate::embed::InputEvent::MouseUp {
                                x: vp_x, y: vp_y,
                                button: crate::embed::MouseButton::Left,
                                modifiers: Default::default(),
                            });
                        }
                        self.page_scrollbar_v_drag = false;
                        self.page_scrollbar_h_drag = false;
                        self.render();
                    }
                    // animations_scrubber_drag dead-flag smazany Phase 99.
                    if self.page_sel_dragging() {
                        self.page_sel_end_drag();
                        self.render();
                    }
                    // Page click -> deleguj MouseUp do webview = dispatch JS
                    // mouseup + CLICK (pokud down+up na stejnem elementu < 5px).
                    // Bez tohoto onclick / addEventListener('click') nefirovaly.
                    if !was_scrollbar_drag {
                        let raw_y = self.mouse_y - self.scroll_y();
                        let in_devtools_panel = self.devtools.panel_open
                            && raw_y >= self.viewport_h_logical() - self.panel_h_logical();
                        if !in_devtools_panel {
                            let vp_x = self.mouse_x - self.scroll_x();
                            if let Some(wv) = self.webview.as_mut() {
                                let _ = wv.handle_input(crate::embed::InputEvent::MouseUp {
                                    x: vp_x, y: raw_y,
                                    button: crate::embed::MouseButton::Left,
                                    modifiers: Default::default(),
                                });
                            }
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                    // Scrollbar thumb drag: pri kliku do scrollbar zony deleguj
                    // cely drag do webview.handle_input (ma plnou thumb logiku +
                    // content height) + skip page handling (selection). Webview
                    // ocekava viewport-relativni CSS px (= mouse - scroll).
                    {
                        let vw = self.viewport_w_logical();
                        let vh = self.viewport_h_logical();
                        let vp_x = self.mouse_x - self.scroll_x();
                        let vp_y = self.mouse_y - self.scroll_y();
                        let (cw, ch) = self.webview.as_ref()
                            .and_then(|w| w.last_layout_root())
                            .map(|l| (l.rect.width, l.rect.height))
                            .unwrap_or((0.0, 0.0));
                        let in_v = ch > vh && vp_x >= vw - 12.0 && vp_x < vw;
                        let in_h = cw > vw && vp_y >= vh - 12.0 && vp_y < vh;
                        if in_v || in_h {
                            if let Some(wv) = self.webview.as_mut() {
                                let _ = wv.handle_input(crate::embed::InputEvent::MouseDown {
                                    x: vp_x, y: vp_y,
                                    button: crate::embed::MouseButton::Left,
                                    modifiers: Default::default(),
                                });
                            }
                            // page_scrollbar_v_drag = routing flag (CursorMoved +
                            // MouseUp pak deleguji do webview).
                            self.page_scrollbar_v_drag = true;
                            if let Some(w) = &self.window { w.request_redraw(); }
                            return;
                        }
                    }
                    // Addr bar click handler smazany N+22.
                    // Double-click detection: 400ms okno + < 5px vzdalenost.
                    let now = std::time::Instant::now();
                    let is_double_click = self.last_click_time
                        .map(|t| {
                            let dt = now.duration_since(t).as_millis() < 400;
                            let dx = (self.mouse_x - self.last_click_pos.0).abs();
                            let dy = (self.mouse_y - self.last_click_pos.1).abs();
                            dt && dx < 5.0 && dy < 5.0
                        })
                        .unwrap_or(false);
                    self.last_click_time = Some(now);
                    self.last_click_pos = (self.mouse_x, self.mouse_y);

                    // Selection start: kazdy MouseDown nastavi anchor. Drag move
                    // updatuje current. Release < 3px diff = clear (simple click).
                    // ALE: klik na CSS resize grip -> NEspoustet selekci (jinak
                    // tah za grip selektuje text misto/vedle resize = "nejde
                    // resizovat" pocit + ruseni textu pod kurzorem).
                    let on_resize_grip = self.webview.as_ref()
                        .and_then(|w| w.last_layout_root())
                        .and_then(|root| crate::embed::webview::find_resize_grip(
                            root, self.mouse_x, self.mouse_y))
                        .is_some();
                    // Klik na draggable=true element -> NEselektovat (tah = drag&drop,
                    // ne text selection).
                    let (sc_x, sc_y) = (self.scroll_x(), self.scroll_y());
                    let on_draggable = self.webview.as_ref()
                        .and_then(|w| w.last_layout_root())
                        .and_then(|root| root.hit_test_scrolled(self.mouse_x, self.mouse_y, sc_x, sc_y))
                        .and_then(|bx| bx.node.clone())
                        .and_then(|n| crate::embed::webview::find_draggable_ancestor(&n))
                        .is_some();
                    // user-select:none (button/UI/draggable) -> NEselektovat. Walk
                    // root->point: hit vraci nejhlubsi box (text node bez vlastniho
                    // user-select), ale ancestor div ho ma -> kontrola cele cesty.
                    let on_unselectable = self.webview.as_ref()
                        .and_then(|w| w.last_layout_root())
                        .map(|root| {
                            fn path_unsel(b: &super::layout::LayoutBox, mx: f32, my: f32, sx: f32, sy: f32) -> bool {
                                // Fixed/sticky subtree sedi na viewport pozici -> test
                                // s viewport coords (konzistentni s hit_test_scrolled).
                                if (sx != 0.0 || sy != 0.0)
                                    && matches!(b.position, super::layout::Position::Fixed | super::layout::Position::Sticky) {
                                    return path_unsel(b, mx - sx, my - sy, 0.0, 0.0);
                                }
                                if mx < b.rect.x || mx >= b.rect.x + b.rect.width
                                    || my < b.rect.y || my >= b.rect.y + b.rect.height { return false; }
                                if b.user_select_none { return true; }
                                let cx = mx + b.scroll_offset_x;
                                let cy = my + b.scroll_offset_y;
                                b.children.iter().any(|c| path_unsel(c, cx, cy, sx, sy))
                            }
                            path_unsel(root, self.mouse_x, self.mouse_y, sc_x, sc_y)
                        })
                        .unwrap_or(false);
                    if std::env::var("RWE_INPUT_DBG").is_ok() {
                        let tag = self.webview.as_ref().and_then(|w| w.last_layout_root())
                            .and_then(|r| r.hit_test_scrolled(self.mouse_x, self.mouse_y, sc_x, sc_y))
                            .and_then(|b| b.node.clone())
                            .and_then(|n| n.tag_name()).unwrap_or_else(|| "?".into());
                        eprintln!("[SEL] MouseDown mouse=({:.0},{:.0}) hit={} on_grip={} on_drag={} unsel={}",
                            self.mouse_x, self.mouse_y, tag, on_resize_grip, on_draggable, on_unselectable);
                        // Hit-chain dump: cesta root -> hit (rect + tag + class) pro
                        // diagnozu layout-vs-render mismatchu (RWE_INPUT_DBG=chain).
                        if std::env::var("RWE_INPUT_DBG").map(|v| v == "chain").unwrap_or(false) {
                            fn dump_chain(b: &crate::browser::layout::LayoutBox, x: f32, y: f32, depth: usize) {
                                let cx = x + b.scroll_offset_x;
                                let cy = y + b.scroll_offset_y;
                                let inside = x >= b.rect.x && x <= b.rect.x + b.rect.width
                                    && y >= b.rect.y && y <= b.rect.y + b.rect.height;
                                let tag = b.node.as_ref().and_then(|n| n.tag_name()).unwrap_or_else(|| "anon".into());
                                let cls = b.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
                                if inside || depth == 0 || tag == "a" || tag == "nav" {
                                    eprintln!("[CHAIN] {}<{} class='{}'> rect=({:.0},{:.0},{:.0}x{:.0}) pos={:?} scrolloff=({:.0},{:.0}) inside={}",
                                        "  ".repeat(depth), tag, cls, b.rect.x, b.rect.y, b.rect.width, b.rect.height, b.position,
                                        b.scroll_offset_x, b.scroll_offset_y, inside);
                                }
                                for ch in &b.children { dump_chain(ch, cx, cy, depth + 1); }
                            }
                            if let Some(root) = self.webview.as_ref().and_then(|w| w.last_layout_root()) {
                                dump_chain(root, self.mouse_x, self.mouse_y, 0);
                            }
                        }
                    }
                    if !on_resize_grip && !on_draggable && !on_unselectable {
                        self.page_sel_begin((self.mouse_x, self.mouse_y));
                    }
                    // Devtools panel hit-test ma prioritu nad page hit-testem.
                    // mouse_x/y v doc-logical, raw_y v screen-logical. viewport_w/h v logical.
                    let raw_y = self.mouse_y - self.scroll_y();
                    let viewport_w = self.viewport_w_logical();
                    let viewport_h = self.viewport_h_logical();
                    let panel_h = self.panel_h_logical();

                    // Page click -> deleguj do webview.handle_input (dispatch JS
                    // mousedown/click event + focus + <a href> nav). Drive App
                    // click NEdispatchoval do JS vubec -> inline onclick i
                    // addEventListener('click') nefirovaly = "mrtve" klikani.
                    // Skip kdyz klik do otevreneho devtools panelu (jeho zona).
                    let in_devtools_panel = self.devtools.panel_open
                        && raw_y >= viewport_h - panel_h;
                    if !in_devtools_panel {
                        let vp_x = self.mouse_x - self.scroll_x();
                        if let Some(wv) = self.webview.as_mut() {
                            let _ = wv.handle_input(crate::embed::InputEvent::MouseDown {
                                x: vp_x, y: raw_y,
                                button: crate::embed::MouseButton::Left,
                                modifiers: Default::default(),
                            });
                        }
                        if let Some(w) = &self.window { w.request_redraw(); }
                    }

                    // Address bar autocomplete suggestion klik.
                    // Scroll-to-top button hit (pravy dolni roh, jen pri scroll_y > 200).
                    // Shell chrome hit-test (priorita nad page).

                    // Main page scrollbar hit-test - layout_root dead na App
                    // vrstve, scrollbar drag init bude per-WebView v dalsi fazi.

                    // Double-click v Elements tab -> zacni editaci attr value / text node.
                    if is_double_click && self.devtools.panel_open && raw_y >= viewport_h - panel_h
                        && self.devtools.tab == crate::devtools::Tab::Elements {
                        use crate::browser::devtools_panel::{double_click_hit_elements, RESIZE_GRIP_H, TAB_H, DevtoolsHit};
                        let content_y = viewport_h - panel_h + RESIZE_GRIP_H + TAB_H;
                        let dchit = double_click_hit_elements(&self.devtools, viewport_w, content_y, self.mouse_x, raw_y);
                        match dchit {
                            DevtoolsHit::EditAttributeValue { node_id, attr } => {
                                self.start_edit_attribute_value(node_id, attr);
                                self.render();
                                return;
                            }
                            DevtoolsHit::EditTextNode { node_id } => {
                                self.start_edit_text_node(node_id);
                                self.render();
                                return;
                            }
                            DevtoolsHit::EditStyleValue { node_id, property } => {
                                self.start_edit_style_property(node_id, property);
                                self.render();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // Modal popups (settings/color picker) zachycuji klik kdekoli
                    // - ne jen v panel boundsi. Bez teto vyjimky popup centered
                    // mimo panel rect by neslo zavrit ani klikat.
                    let modal_active = self.devtools.settings_popup_open
                        || self.devtools.color_picker.is_some()
                        || self.devtools.class_manager_open;
                    // Pouzij point_in_devtools (respektuje dock position) misto
                    // bottom-only check raw_y >= viewport_h - panel_h. Bez teto
                    // opravy pri Top/Left/Right dock klik v panelu propaguje na page.
                    let in_panel = self.point_in_devtools(self.mouse_x - self.scroll_x(), raw_y);
                    if self.devtools.panel_open && (in_panel || modal_active) {
                        if let Some(layout) = self.webview.as_ref().and_then(|w| w.last_layout_root()) {
                            let hit = devtools_hit_test(&self.devtools, layout, viewport_w, viewport_h, self.mouse_x, raw_y);
                            use crate::browser::devtools_panel::DevtoolsHit;
                            match hit {
                                DevtoolsHit::TabClick(t) => {
                                    self.devtools.tab = t;
                                    // Auto-select first source pri prepnuti na Sources tab.
                                    if t == crate::devtools::Tab::Sources
                                        && self.devtools.sources.selected_id.is_none()
                                        && !self.devtools.sources.files.is_empty() {
                                        self.devtools.sources.selected_id = Some(self.devtools.sources.files[0].id);
                                    }
                                }
                                DevtoolsHit::TreeRow(node_id) => {
                                    // Pri aktivni inline edit + click na editovany row:
                                    // prevod x na byte idx + set cursor v edit.buffer.
                                    if let Some(edit) = self.devtools.elements.edit.as_mut() {
                                        use crate::devtools::EditTarget;
                                        let edit_node = match &edit.target {
                                            EditTarget::AttributeValue { node_id, .. } => *node_id,
                                            EditTarget::AttributeName { node_id, .. } => *node_id,
                                            EditTarget::TextNode { node_id } => *node_id,
                                            EditTarget::InlineStyleProperty { node_id, .. } => *node_id,
                                        };
                                        if edit_node == node_id {
                                            use crate::browser::devtools_panel::{dt_text_width, dt_byte_idx_at_x, INDENT_PX};
                                            let depth = self.devtools.elements.rows.iter()
                                                .find(|r| r.node_id == node_id)
                                                .map(|r| r.depth).unwrap_or(0);
                                            let prefix = match &edit.target {
                                                EditTarget::AttributeValue { attr, .. } => format!("{}=", attr),
                                                EditTarget::AttributeName { value, .. } => format!("[new]={}=", value),
                                                EditTarget::TextNode { .. } => "text:".to_string(),
                                                EditTarget::InlineStyleProperty { property, .. } => format!("{}: ", property),
                                            };
                                            let text_x = 8.0 + depth as f32 * INDENT_PX + dt_text_width(&prefix);
                                            let rel_x = self.mouse_x - text_x;
                                            let idx = dt_byte_idx_at_x(&edit.buffer.text, rel_x);
                                            edit.buffer.set_cursor_byte(idx);
                                            self.render();
                                            return;
                                        }
                                    }
                                    self.devtools.elements.selected = Some(node_id);
                                }
                                DevtoolsHit::TreeCaret(node_id) => {
                                    if self.devtools.elements.collapsed.contains(&node_id) {
                                        self.devtools.elements.collapsed.remove(&node_id);
                                    } else {
                                        self.devtools.elements.collapsed.insert(node_id);
                                    }
                                    if let Some(interp) = self.interp() {
                                        let root = std::rc::Rc::clone(&interp.document.borrow().root);
                                        crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                                    }
                                }
                                DevtoolsHit::InspectToggle => {
                                    self.devtools.inspect_mode = !self.devtools.inspect_mode;
                                }
                                DevtoolsHit::ResizeGrip => {
                                    self.devtools_resizing = true;
                                }
                                DevtoolsHit::Close => { self.devtools.panel_open = false; }
                                DevtoolsHit::ThemeToggle => {
                                    use crate::devtools::theme::ThemeMode;
                                    self.devtools.theme.mode = match self.devtools.theme.mode {
                                        ThemeMode::Auto => ThemeMode::Light,
                                        ThemeMode::Light => ThemeMode::Dark,
                                        ThemeMode::Dark => ThemeMode::Auto,
                                    };
                                    crate::devtools::theme::save_persisted(self.devtools.theme);
                                }
                                DevtoolsHit::ThemeChoice(m) => {
                                    self.devtools.theme.mode = m;
                                    crate::devtools::theme::save_persisted(self.devtools.theme);
                                }
                                DevtoolsHit::FlavorChoice(f) => {
                                    self.devtools.theme.flavor = f;
                                    crate::devtools::theme::save_persisted(self.devtools.theme);
                                }
                                DevtoolsHit::ConsoleClear => {
                                    self.devtools.console.log.clear();
                                }
                                DevtoolsHit::NetworkClear => {
                                    self.devtools.network.entries.clear();
                                }
                                DevtoolsHit::ConsoleInput => {
                                    self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsConsole;
                                    // Click-to-position cursor: prevod mouse_x na byte idx.
                                    use crate::browser::devtools_panel::{dt_text_width, dt_byte_idx_at_x};
                                    let prompt_x = 10.0 + dt_text_width("> ");
                                    let rel_x = self.mouse_x - prompt_x;
                                    let text = self.devtools.console.input.text.clone();
                                    let idx = dt_byte_idx_at_x(&text, rel_x);
                                    self.devtools.console.input.set_cursor_byte(idx);
                                }
                                DevtoolsHit::ElementsSearchBar => {
                                    self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsElementsSearch;
                                }
                                DevtoolsHit::SourcesFileRow(id) => {
                                    self.devtools.sources.selected_id = Some(id);
                                }
                                DevtoolsHit::SourcesGutter { file_id, line } => {
                                    self.devtools.sources.toggle_breakpoint(file_id, line);
                                    // Auto-aktivace debug mode pri prvnim BP.
                                    if self.devtools.panel_open && !self.devtools.sources.breakpoints.is_empty()
                                        && self.debug_runner.is_none() {
                                        self.activate_debug_mode();
                                    }
                                }
                                DevtoolsHit::NetworkRow(idx) => {
                                    self.devtools.network.selected = Some(idx);
                                    self.devtools.network.detail_open = true;
                                }
                                DevtoolsHit::NetworkFilterClick(f) => {
                                    self.devtools.network.filter = f;
                                }
                                DevtoolsHit::SourcesToggleOriginal => {
                                    self.devtools.sources.show_original = !self.devtools.sources.show_original;
                                }
                                DevtoolsHit::PanelArea => {
                                    self.devtools.focus = crate::devtools::focus::FocusTarget::Page;
                                }
                                DevtoolsHit::DismissContextMenu => {
                                    self.devtools.context_menu = None;
                                }
                                DevtoolsHit::ContextMenuItem(idx) => {
                                    let action = self.devtools.context_menu.as_ref()
                                        .and_then(|m| m.action_at(idx)).cloned();
                                    self.devtools.context_menu = None;
                                    if let Some(a) = action {
                                        self.dispatch_menu_action(a);
                                    }
                                }
                                DevtoolsHit::DebuggerContinue => {
                                    if let Some(interp) = self.interp() {
                                        interp.debugger.borrow_mut().resume();
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                    self.rerun_paused_scripts();
                                }
                                DevtoolsHit::DebuggerStepOver => {
                                    if let Some(interp) = self.interp() {
                                        interp.debugger.borrow_mut().start_step(crate::interpreter::StepKind::Over);
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                }
                                DevtoolsHit::DebuggerStepInto => {
                                    if let Some(interp) = self.interp() {
                                        interp.debugger.borrow_mut().start_step(crate::interpreter::StepKind::Into);
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                }
                                DevtoolsHit::DebuggerStepOut => {
                                    if let Some(interp) = self.interp() {
                                        interp.debugger.borrow_mut().start_step(crate::interpreter::StepKind::Out);
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                }
                                DevtoolsHit::SplitterDrag => {
                                    self.devtools.elements.dragging_split = true;
                                }
                                DevtoolsHit::ScrollbarThumb(target) => {
                                    self.devtools.elements.dragging_scrollbar = Some(target);
                                }
                                DevtoolsHit::TabOverflowToggle => {
                                    self.devtools.tab_overflow_open = !self.devtools.tab_overflow_open;
                                }
                                DevtoolsHit::TabOverflowPick(t) => {
                                    self.devtools.tab = t;
                                    self.devtools.tab_overflow_open = false;
                                }
                                _ if self.devtools.tab_overflow_open => {
                                    // Klik mimo overflow popup -> dismiss.
                                    self.devtools.tab_overflow_open = false;
                                }
                                DevtoolsHit::SidePanelTabClick(t) => {
                                    self.devtools.side_panel_tab = t;
                                    // Po vyberu z dropdown menu zavri overflow.
                                    self.devtools.side_panel_overflow_open = false;
                                }
                                DevtoolsHit::SidePanelSplitterDrag => {
                                    self.devtools.elements.dragging_side_split = true;
                                }
                                DevtoolsHit::SectionToggle(id) => {
                                    if self.devtools.collapsed_sections.contains(&id) {
                                        self.devtools.collapsed_sections.remove(&id);
                                    } else {
                                        self.devtools.collapsed_sections.insert(id);
                                    }
                                }
                                DevtoolsHit::ComputedShorthandToggle(name) => {
                                    let mut s = self.devtools.styles.computed_expanded.borrow_mut();
                                    if s.contains(&name) {
                                        s.remove(&name);
                                    } else {
                                        s.insert(name);
                                    }
                                }
                                DevtoolsHit::SidePanelOverflowToggle => {
                                    self.devtools.side_panel_overflow_open = !self.devtools.side_panel_overflow_open;
                                }
                                DevtoolsHit::AnimationsScrub(_progress) => {
                                    // animation_origin/animation_pause_start dead na App vrstve.
                                    // Effective animace cas zije ve webview.animation_origin.
                                    // Scrubber rework vyzaduje webview.set_animation_origin API.
                                }
                                DevtoolsHit::AnimationsAction(action) => {
                                    // pause/speed/restart handler smazany Phase 99 - vsechny
                                    // operace patri webview.animation_origin (App-side dead).
                                    // devtools.animations_paused flag zustal toggle-only:
                                    if action == "pause" {
                                        self.devtools.animations_paused = !self.devtools.animations_paused;
                                    }
                                }
                                DevtoolsHit::ColorPickerHexFocus => {
                                    if let Some(cp) = &mut self.devtools.color_picker {
                                        cp.hex_focused = true;
                                        cp.rgb_focused = None;
                                    }
                                }
                                DevtoolsHit::ColorPickerRgbFocus(i) => {
                                    if let Some(cp) = &mut self.devtools.color_picker {
                                        cp.hex_focused = false;
                                        cp.rgb_focused = Some(i);
                                    }
                                }
                                DevtoolsHit::EditDeclValue(rule_idx, prop) => {
                                    // Initial buffer = current value z konkretni rule (ne computed).
                                    let cur = self.devtools.styles.matched_rules.get(rule_idx)
                                        .and_then(|r| r.declarations.iter()
                                            .find(|d| d.property == prop)
                                            .map(|d| d.value.clone()))
                                        .unwrap_or_default();
                                    self.devtools.styles.editing_value = Some((rule_idx, prop, cur));
                                }
                                DevtoolsHit::EditInlineValue(prop) => {
                                    // Read current inline value z node attr "style".
                                    let val = self.devtools.elements.selected
                                        .and_then(|sel| {
                                            self.interp().and_then(|i| {
                                                let root = std::rc::Rc::clone(&i.document.borrow().root);
                                                find_node_by_ptr(&root, sel)
                                            })
                                        })
                                        .and_then(|n| n.attributes.borrow().get("style").cloned())
                                        .and_then(|s| {
                                            for d in s.split(';') {
                                                let mut parts = d.splitn(2, ':');
                                                if let (Some(p), Some(v)) = (parts.next(), parts.next()) {
                                                    if p.trim() == prop { return Some(v.trim().to_string()); }
                                                }
                                            }
                                            None
                                        }).unwrap_or_default();
                                    self.devtools.styles.editing_inline = Some((prop, val));
                                }
                                DevtoolsHit::AddInlineDecl => {
                                    use crate::devtools::model::styles::{AddingInlineDecl, AddPhase};
                                    self.devtools.styles.adding_inline_decl = Some(AddingInlineDecl {
                                        phase: AddPhase::Property,
                                        prop_buffer: String::new(),
                                        value_buffer: String::new(),
                                    });
                                }
                                DevtoolsHit::ToggleMatchPreview(sel) => {
                                    if self.devtools.match_preview_selector.as_deref() == Some(&sel) {
                                        self.devtools.match_preview_selector = None;
                                    } else {
                                        self.devtools.match_preview_selector = Some(sel);
                                    }
                                }
                                DevtoolsHit::OpenSourceLink(label) => {
                                    // Prepnout na Sources tab. Najdi file dle label
                                    // (filename z URL nebo "<style> #idx").
                                    self.devtools.tab = crate::devtools::Tab::Sources;
                                    // Best-effort: kdyz label obsahuje filename, najdi
                                    // odpovidajici source file v sources panel.
                                    if !label.starts_with("<style>") && !label.starts_with("user agent") && !label.starts_with("inline") {
                                        let files = self.devtools.sources.files.clone();
                                        for (idx, f) in files.iter().enumerate() {
                                            if f.url.ends_with(&label) || f.url.contains(&label) {
                                                self.devtools.sources.selected_id = Some(idx as u32);
                                                break;
                                            }
                                        }
                                    }
                                }
                                DevtoolsHit::SettingsToggle => {
                                    self.devtools.settings_popup_open = !self.devtools.settings_popup_open;
                                }
                                DevtoolsHit::SettingsDock(pos) => {
                                    self.devtools.dock_position = pos;
                                    crate::devtools::profile::save_dock_position(pos);
                                }
                                DevtoolsHit::SettingsTheme(t) => {
                                    self.devtools.theme.mode = t;
                                    crate::devtools::theme::save_persisted(self.devtools.theme);
                                }
                                DevtoolsHit::SettingsFlavor(f) => {
                                    self.devtools.theme.flavor = f;
                                    crate::devtools::theme::save_persisted(self.devtools.theme);
                                }
                                DevtoolsHit::SettingsClose => {
                                    self.devtools.settings_popup_open = false;
                                }
                                DevtoolsHit::ColorPickerHue(h) => {
                                    if let Some(cp) = self.devtools.color_picker.as_mut() {
                                        cp.hue = h;
                                        cp.rgba = crate::devtools::hsv_to_rgb(cp.hue, cp.sat, cp.val);
                                        cp.sync_inputs_from_rgba();
                                    }
                                    self.write_back_color_picker();
                                }
                                DevtoolsHit::ColorPickerSV(s, v) => {
                                    if let Some(cp) = self.devtools.color_picker.as_mut() {
                                        cp.sat = s; cp.val = v;
                                        cp.rgba = crate::devtools::hsv_to_rgb(cp.hue, cp.sat, cp.val);
                                        cp.sync_inputs_from_rgba();
                                    }
                                    self.write_back_color_picker();
                                }
                                DevtoolsHit::ColorPickerClose => {
                                    self.devtools.color_picker = None;
                                }
                                DevtoolsHit::OpenColorPicker { anchor_x, anchor_y, color, property } => {
                                    let target = self.devtools.elements.selected.map(|id| (id, property.clone()));
                                    let (h, s, v) = crate::devtools::rgb_to_hsv(color[0], color[1], color[2]);
                                    let mut cp = crate::devtools::ColorPickerState {
                                        anchor_x, anchor_y,
                                        rgba: color,
                                        hue: h, sat: s, val: v,
                                        target,
                                        hex_input: format!("{:02x}{:02x}{:02x}", color[0], color[1], color[2]),
                                        hex_focused: false,
                                        rgb_inputs: [color[0].to_string(), color[1].to_string(), color[2].to_string()],
                                        rgb_focused: None,
                                    };
                                    cp.sync_inputs_from_rgba();
                                    self.devtools.color_picker = Some(cp);
                                }
                                DevtoolsHit::ForcePseudoToggle => {
                                    // Cycle: none -> hover -> focus -> active -> none.
                                    let h = self.devtools.force_hover;
                                    let f = self.devtools.force_focus;
                                    let a = self.devtools.force_active;
                                    if !h && !f && !a {
                                        self.devtools.force_hover = true;
                                    } else if h {
                                        self.devtools.force_hover = false;
                                        self.devtools.force_focus = true;
                                    } else if f {
                                        self.devtools.force_focus = false;
                                        self.devtools.force_active = true;
                                    } else {
                                        self.devtools.force_active = false;
                                    }
                                }
                                DevtoolsHit::ClassManagerToggle => {
                                    self.devtools.class_manager_open = !self.devtools.class_manager_open;
                                }
                                DevtoolsHit::AddNewRule => {
                                    // Pridej prazdny inline style attribut na selected node + open editor.
                                    if let Some(sel_id) = self.devtools.elements.selected {
                                        if let Some(interp) = self.interp() {
                                            let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                                            if let Some(node) = find_node_by_ptr(&doc_root, sel_id) {
                                                let mut attrs = node.attributes.borrow_mut();
                                                let cur_style = attrs.get("style").cloned().unwrap_or_default();
                                                if !cur_style.contains("/* nova vlastnost */") {
                                                    let appended = if cur_style.is_empty() {
                                                        "/* nova vlastnost */: ;".to_string()
                                                    } else {
                                                        format!("{}; /* nova vlastnost */: ;", cur_style.trim_end_matches(';'))
                                                    };
                                                    attrs.insert("style".to_string(), appended);
                                                }
                                                drop(attrs);
                                                println!("[devtools] add rule - inline style updated");
                                            }
                                        }
                                    }
                                }
                                DevtoolsHit::ClassManagerToggleClass(cls) => {
                                    if let Some(sel_id) = self.devtools.elements.selected {
                                        if let Some(interp) = self.interp() {
                                            let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                                            if let Some(node) = find_node_by_ptr(&doc_root, sel_id) {
                                                let cur = node.attributes.borrow().get("class").cloned().unwrap_or_default();
                                                let mut classes: Vec<&str> = cur.split_whitespace().collect();
                                                if let Some(pos) = classes.iter().position(|c| **c == cls) {
                                                    classes.remove(pos);
                                                } else {
                                                    classes.push(&cls);
                                                }
                                                let new_val = classes.join(" ");
                                                node.attributes.borrow_mut().insert("class".to_string(), new_val.clone());
                                                // Changes log entry.
                                                self.devtools.changes.push(crate::devtools::ChangeEntry {
                                                    timestamp_ts: crate::devtools::history::now_ts(),
                                                    kind: crate::devtools::ChangeKind::ClassToggle,
                                                    target_node_id: sel_id,
                                                    property: "class".to_string(),
                                                    old_value: cur,
                                                    new_value: new_val,
                                                });
                                            }
                                        }
                                    }
                                }
                                DevtoolsHit::JumpToVar(name) => {
                                    if let Some(idx) = self.devtools.styles.matched_rules.iter().position(|r|
                                        r.declarations.iter().any(|d| d.property == name)) {
                                        let target_y = (idx as f32) * 18.0 * 5.0;
                                        self.devtools.styles.scroll_y = target_y.max(0.0);
                                    }
                                    // Highlight cilove var rule po N frames.
                                    self.devtools.var_highlight = Some((name, 90)); // ~1.5s @ 60fps
                                }
                                DevtoolsHit::OverlayToggle(kind, node_id) => {
                                    let pos = self.devtools.overlays.iter().position(|o|
                                        o.node_id == node_id && o.kind == kind);
                                    match pos {
                                        Some(i) => { self.devtools.overlays.remove(i); }
                                        None => self.devtools.overlays.push(crate::devtools::OverlayDescriptor {
                                            node_id, kind,
                                        }),
                                    }
                                }
                                DevtoolsHit::None | _ => {}
                            }
                        }
                        self.render();
                        return;
                    } else {
                        // Klik mimo panel - reset focus na Page.
                        self.devtools.focus = crate::devtools::focus::FocusTarget::Page;
                    }
                    // Inspect mode: kliknuti na main viewport vybira node v tree.
                    // Pick pres webview.last_layout_root() - prevedeni screen px na
                    // page-local px (minus scroll_y).
                    if self.devtools.inspect_mode {
                        let pick = self.webview.as_ref()
                            .and_then(|w| w.last_layout_root())
                            .and_then(|lay| {
                                let scroll_y = self.scroll_y();
                                crate::browser::devtools_panel::pick_node_at_screen_pos(
                                    lay, self.mouse_x, self.mouse_y, scroll_y)
                            });
                        self.devtools.inspect_mode = false;
                        if let Some(node_id) = pick {
                            self.select_and_reveal_node(node_id);
                        }
                        self.render();
                        return;
                    }
                    self.handle_click(self.mouse_x, self.mouse_y);
                    self.render();
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Middle, .. } => {
                    // Middle-click na tab chip = zavrit ten tab.
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                    let raw_y = self.mouse_y - self.scroll_y();
                    let viewport_h = self.viewport_h_logical();
                    let panel_h = self.panel_h_logical();
                    // Shell chrome RMB: tab/bookmark context menu.
                    if self.devtools.panel_open && raw_y >= viewport_h - panel_h {
                        use crate::browser::devtools_panel::{RESIZE_GRIP_H, SEARCH_H};
                        use crate::devtools::context_menu::{ContextMenuState,
                            elements_row_menu, console_text_menu, console_log_menu,
                            network_row_menu, sources_line_menu};
                        let _ = console_log_menu;
                        let items = match self.devtools.tab {
                            crate::devtools::Tab::Elements => {
                                let split_x = self.devtools.elements.split_x.max(200.0);
                                if self.mouse_x < split_x {
                                    let body_y = viewport_h - panel_h + RESIZE_GRIP_H + crate::browser::devtools_panel::TAB_H
                                        + if self.devtools.elements.search.open { SEARCH_H } else { 0.0 };
                                    let row_idx = ((raw_y - body_y + self.devtools.elements.scroll_y) / 18.0) as usize;
                                    if row_idx < self.devtools.elements.rows.len() {
                                        let nid = self.devtools.elements.rows[row_idx].node_id;
                                        self.devtools.elements.selected = Some(nid);
                                        Some(elements_row_menu(nid))
                                    } else {
                                        None
                                    }
                                } else { None }
                            }
                            crate::devtools::Tab::Console => Some(console_text_menu()),
                            crate::devtools::Tab::Network => {
                                let header_h = 18.0 + 4.0;
                                let toolbar_top = viewport_h - panel_h + RESIZE_GRIP_H + crate::browser::devtools_panel::TAB_H;
                                let row_y = toolbar_top + header_h + 2.0;
                                let idx = ((raw_y - row_y) / 18.0) as usize;
                                if idx < self.devtools.network.entries.len() {
                                    Some(network_row_menu(idx))
                                } else { None }
                            }
                            crate::devtools::Tab::Sources => {
                                if let Some(file_id) = self.devtools.sources.selected_id {
                                    let toolbar_top = viewport_h - panel_h + RESIZE_GRIP_H + crate::browser::devtools_panel::TAB_H;
                                    let line_idx = ((raw_y - toolbar_top + self.devtools.sources.scroll_y) / 18.0) as usize;
                                    Some(sources_line_menu(file_id, line_idx as u32 + 1))
                                } else { None }
                            }
                            _ => None,
                        };
                        if let Some(items) = items {
                            self.devtools.context_menu = Some(ContextMenuState::new(self.mouse_x, raw_y, items));
                        }
                        self.render();
                    } else {
                        // Page area RMB: deleguj do webview -> dispatch mousedown
                        // (button 2) + contextmenu na element pod kurzorem. Drive
                        // pravy klik na stranku nedelal NIC (jen devtools menu v
                        // panelu) -> oncontextmenu handlery byly mrtve.
                        let vp_x = self.mouse_x - self.scroll_x();
                        let vp_y = self.mouse_y - self.scroll_y();
                        if let Some(wv) = self.webview.as_mut() {
                            let r = wv.handle_input(crate::embed::InputEvent::MouseDown {
                                x: vp_x, y: vp_y,
                                button: crate::embed::MouseButton::Right,
                                modifiers: Default::default(),
                            });
                            if r.dirty {
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                        }
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    // Pri kurzoru nad devtools panelem - scrolluj tree, ne stranku.
                    // mouse_x/y jsou logical (po /(zoom*scale)), srovnani s logical viewport.
                    let raw_y_logical = self.mouse_y - self.scroll_y();
                    let viewport_w = self.viewport_w_logical();
                    if self.point_in_devtools(self.mouse_x - self.scroll_x(), raw_y_logical) {
                        let scroll_amount_logical = scroll_amount / (self.zoom() * self.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0));
                        // Globalni scroll routing: vzdy ta cast pod kurzorem.
                        // Pri Elements: 3 zony - tree | styles | side_panel.
                        match self.devtools.tab {
                            crate::devtools::Tab::Elements => {
                                let mx_local = self.mouse_x - self.scroll_x();
                                let side_panel_w = self.devtools.side_panel_w.clamp(180.0, (viewport_w - 400.0).max(181.0));
                                let styles_end = viewport_w - side_panel_w;
                                let default_tree_split = (viewport_w - side_panel_w) * 0.45;
                                let tree_split = if self.devtools.elements.split_x < 1.0 { default_tree_split }
                                                  else { self.devtools.elements.split_x.max(200.0).min(viewport_w - side_panel_w - 200.0) };
                                let body_h = self.panel_h_logical() - 4.0 - 30.0
                                    - if self.devtools.elements.search.open { 28.0 } else { 0.0 };
                                if mx_local >= styles_end {
                                    // Side panel - obsah obvykle maly, scroll jen pri overflow.
                                    // Pro ted: no-op (side panel nescrolluje).
                                } else if mx_local >= tree_split {
                                    // Styles pane.
                                    let total_h = self.estimate_styles_total_h();
                                    let max_scroll = (total_h - body_h).max(0.0);
                                    if max_scroll > 0.0 {
                                        self.devtools.styles.scroll_y = (self.devtools.styles.scroll_y - scroll_amount_logical).clamp(0.0, max_scroll);
                                    }
                                } else {
                                    // Tree pane.
                                    let total_h = self.devtools.elements.rows.len() as f32 * 18.0;
                                    let max_scroll = (total_h - body_h).max(0.0);
                                    if max_scroll > 0.0 {
                                        self.devtools.elements.scroll_y = (self.devtools.elements.scroll_y - scroll_amount_logical).clamp(0.0, max_scroll);
                                    }
                                }
                            }
                            crate::devtools::Tab::Sources => {
                                self.devtools.sources.scroll_y = (self.devtools.sources.scroll_y - scroll_amount_logical).max(0.0);
                            }
                            crate::devtools::Tab::Console => {
                                // Wheel up = scroll dovrchu (off bottom). Pri
                                // user-initiated scroll vypni stick_to_bottom
                                // (jinak by se okamzite vratil zpet na konec).
                                self.devtools.console.stick_to_bottom = false;
                                self.devtools.console.scroll_y = (self.devtools.console.scroll_y - scroll_amount_logical).max(0.0);
                            }
                            _ => {}
                        }
                        self.render();
                        return;
                    }
                    // Scroll amount je v physical px ze winit; layout je
                    // v logical -> dele zoom. Smooth scroll: meni TARGET, render
                    // tick interpoluje scroll_y -> target.
                    let scale = self.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
                    let logical_scroll = scroll_amount / (self.zoom() * scale).max(0.0001);
                    if self.modifiers.shift_key() {
                        self.set_scroll_target_x((self.scroll_target_x() - logical_scroll).max(0.0));
                    } else {
                        // Nejdriv zkus INNER overflow kontejner pod kurzorem
                        // (overflow:auto/scroll sekce). Pokud scrollnut inner,
                        // NEscrolluj stranku - jinak se inner thumb nehybal +
                        // "scrolluju strankou misto sekce".
                        let dy_inner = -logical_scroll;
                        // App.mouse_x/y jsou CONTENT coords (CursorMoved pricita
                        // scroll); try_inner_wheel_scroll ocekava VIEWPORT coords
                        // (pricita scroll sam). Dvojite pricteni = hit-test mimo
                        // -> target None -> wheel scrolloval stranku misto
                        // vnitrniho overflow containeru (sekce 11).
                        let vx = self.mouse_x - self.scroll_x();
                        let vy = self.mouse_y - self.scroll_y();
                        let inner = self.webview.as_mut()
                            .map(|w| w.try_inner_wheel_scroll(vx, vy, dy_inner))
                            .unwrap_or(false);
                        if std::env::var("RWE_SCROLL_DBG").is_ok() {
                            eprintln!("[WHEEL] mouse=({:.0},{:.0}) dy_inner={:.1} inner_scrolled={}",
                                self.mouse_x, self.mouse_y, dy_inner, inner);
                        }
                        if !inner {
                            self.set_scroll_target_y((self.scroll_target_y() - logical_scroll).max(0.0));
                            self.clamp_scroll_to_layout();
                        }
                    }
                    self.render();
                }
                WindowEvent::RedrawRequested => {
                    let _rr_t0 = std::time::Instant::now();
                    let _had_resize = self.pending_resize.is_some();
                    if let Some(size) = self.pending_resize.take() {
                        let mut sf = 1.0_f32;
                        if let Some(r) = &mut self.renderer {
                            if let Some(w) = &self.window {
                                r.scale_factor = w.scale_factor() as f32;
                            }
                            sf = r.scale_factor.max(0.01);
                            r.resize(size.width.max(1), size.height.max(1));
                        }
                        let lw = ((size.width as f32 / sf) as u32).max(1);
                        let lh = ((size.height as f32 / sf) as u32).max(1);
                        if let Some(wv) = self.webview.as_mut() {
                            wv.resize(lw, lh, sf);
                        }
                    }
                    let _rr_t1 = std::time::Instant::now();
                    self.render();
                    if std::env::var("RWE_RESIZE_DBG").is_ok() && _had_resize {
                        eprintln!("[RSZ] redraw: realloc={:.1}ms render={:.1}ms",
                            _rr_t1.duration_since(_rr_t0).as_secs_f32()*1000.0,
                            _rr_t1.elapsed().as_secs_f32()*1000.0);
                    }
                    // POZN: continual redraw pump pri aktivnich animacich resi
                    // VYHRADNE about_to_wait (s frame-rate capem ~60 FPS pres
                    // WaitUntil). Driv se request_redraw volal i tady na konci
                    // RedrawRequested -> queued event prebil WaitUntil deadline =
                    // loop nikdy nespal = 100% CPU spin. Self-request odstranen,
                    // about_to_wait je jediny zdroj animacniho pumpu.
                }
                // Drag-drop HTML soubor: reload okna s novym souborem.
                WindowEvent::DroppedFile(path) => {
                    println!("[drop] {}", path.display());
                    self.load_path(&path);
                    self.render();
                }
                WindowEvent::HoveredFile(_) => {
                    // Nic - winit jen oznamuje hover.
                }
                WindowEvent::ModifiersChanged(m) => {
                    self.modifiers = m.state();
                }
                // F12 = regenerace devtools.html + open v default browseru.
                // F5 / Ctrl+R = reload current file.
                // Alt+Left = back, Alt+Right = forward (browser history).
                // Ctrl++ / Ctrl+- / Ctrl+0 = zoom in/out/reset (page reflow).
                WindowEvent::KeyboardInput { event: key_event, .. } => {
                    if key_event.state != ElementState::Pressed {
                        // KeyUp: forward page keyup JS do webview (focused element).
                        if let Some(js_key) = logical_key_to_js(&key_event.logical_key) {
                            if let Some(wv) = self.webview.as_mut() {
                                let resp = wv.handle_input(crate::embed::InputEvent::KeyUp {
                                    key: js_key, modifiers: Default::default(),
                                });
                                if resp.dirty { if let Some(w) = &self.window { w.request_redraw(); } }
                            }
                        }
                        return;
                    }
                    // Esc zavre vsechny popupy (color picker / settings / class
                    // manager / tab overflow) pred ostatnim handlingom.
                    if matches!(&key_event.logical_key, Key::Named(NamedKey::Escape)) {
                        if self.handle_escape_close_popups() {
                            self.render();
                            return;
                        }
                    }
                    // Editing inline (klik value v prvek{}) - typovani.
                    if self.devtools.styles.editing_inline.is_some() {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                if let Some((_, b)) = self.devtools.styles.editing_inline.as_mut() {
                                    b.pop();
                                }
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some((prop, val)) = self.devtools.styles.editing_inline.take() {
                                    self.write_back_style_edit(&prop, &val);
                                }
                            }
                            Key::Named(NamedKey::Escape) => {
                                self.devtools.styles.editing_inline = None;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some((_, b)) = self.devtools.styles.editing_inline.as_mut() {
                                    b.push(' ');
                                }
                            }
                            Key::Character(s) => {
                                if let Some((_, b)) = self.devtools.styles.editing_inline.as_mut() {
                                    b.push_str(s);
                                }
                            }
                            _ => {}
                        }
                        self.render();
                        return;
                    }
                    // Adding new inline decl.
                    if self.devtools.styles.adding_inline_decl.is_some() {
                        use crate::devtools::model::styles::AddPhase;
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(a) = self.devtools.styles.adding_inline_decl.as_mut() {
                                    match a.phase {
                                        AddPhase::Property => { a.prop_buffer.pop(); }
                                        AddPhase::Value => { a.value_buffer.pop(); }
                                    }
                                }
                            }
                            Key::Named(NamedKey::Tab) => {
                                if let Some(a) = self.devtools.styles.adding_inline_decl.as_mut() {
                                    a.phase = match a.phase {
                                        AddPhase::Property => AddPhase::Value,
                                        AddPhase::Value => AddPhase::Property,
                                    };
                                }
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some(a) = self.devtools.styles.adding_inline_decl.as_ref() {
                                    if a.phase == AddPhase::Property && !a.prop_buffer.is_empty() {
                                        // Move na value phase.
                                        if let Some(am) = self.devtools.styles.adding_inline_decl.as_mut() {
                                            am.phase = AddPhase::Value;
                                        }
                                    } else if !a.prop_buffer.is_empty() && !a.value_buffer.is_empty() {
                                        let prop = a.prop_buffer.clone();
                                        let val = a.value_buffer.clone();
                                        self.devtools.styles.adding_inline_decl = None;
                                        self.write_back_style_edit(&prop, &val);
                                    }
                                }
                            }
                            Key::Named(NamedKey::Escape) => {
                                self.devtools.styles.adding_inline_decl = None;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some(a) = self.devtools.styles.adding_inline_decl.as_mut() {
                                    match a.phase {
                                        AddPhase::Property => a.prop_buffer.push(' '),
                                        AddPhase::Value => a.value_buffer.push(' '),
                                    }
                                }
                            }
                            Key::Character(s) => {
                                if let Some(a) = self.devtools.styles.adding_inline_decl.as_mut() {
                                    match a.phase {
                                        AddPhase::Property => a.prop_buffer.push_str(s),
                                        AddPhase::Value => a.value_buffer.push_str(s),
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.render();
                        return;
                    }
                    // Editing value v styles pane - typovani do bufferu.
                    if self.devtools.styles.editing_value.is_some() {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                if let Some((_, _, b)) = self.devtools.styles.editing_value.as_mut() {
                                    b.pop();
                                }
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some((_ri, prop, val)) = self.devtools.styles.editing_value.take() {
                                    self.write_back_style_edit(&prop, &val);
                                }
                            }
                            Key::Named(NamedKey::Escape) => {
                                self.devtools.styles.editing_value = None;
                            }
                            Key::Character(s) => {
                                if let Some((_, _, b)) = self.devtools.styles.editing_value.as_mut() {
                                    b.push_str(s);
                                }
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some((_, _, b)) = self.devtools.styles.editing_value.as_mut() {
                                    b.push(' ');
                                }
                            }
                            _ => {}
                        }
                        self.render();
                        return;
                    }
                    // Color picker hex/rgb input keyboard - typovani.
                    if let Some(cp) = self.devtools.color_picker.as_mut() {
                        if cp.hex_focused || cp.rgb_focused.is_some() {
                            match &key_event.logical_key {
                                Key::Named(NamedKey::Backspace) => {
                                    if cp.hex_focused { cp.hex_input.pop(); }
                                    else if let Some(i) = cp.rgb_focused {
                                        cp.rgb_inputs[i].pop();
                                    }
                                }
                                Key::Named(NamedKey::Enter) => {
                                    let ok = if cp.hex_focused { cp.apply_hex() }
                                             else if let Some(i) = cp.rgb_focused { cp.apply_rgb(i) }
                                             else { false };
                                    if ok { self.write_back_color_picker(); }
                                }
                                Key::Character(s) => {
                                    let ch = s.chars().next();
                                    if let Some(c) = ch {
                                        if cp.hex_focused {
                                            // HEX: jen [0-9a-fA-F], max 6.
                                            if c.is_ascii_hexdigit() && cp.hex_input.len() < 6 {
                                                cp.hex_input.push(c.to_ascii_lowercase());
                                                cp.apply_hex();
                                                self.write_back_color_picker();
                                            }
                                        } else if let Some(i) = cp.rgb_focused {
                                            // RGB: jen [0-9], max 3.
                                            if c.is_ascii_digit() && cp.rgb_inputs[i].len() < 3 {
                                                cp.rgb_inputs[i].push(c);
                                                cp.apply_rgb(i);
                                                self.write_back_color_picker();
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            self.render();
                            return;
                        }
                    }
                    // Edit mode (DOM/CSS edit) - presmeruj key events do edit.buffer.
                    use crate::devtools::focus::FocusTarget;
                    if self.devtools.panel_open && self.devtools.elements.edit.is_some() {
                        let ctrl = self.modifiers.control_key();
                        let shift = self.modifiers.shift_key();
                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                        let outcome = {
                            let edit = self.devtools.elements.edit.as_mut().unwrap();
                            dispatch_text_key(&mut edit.buffer, &key_event.logical_key, ctrl, shift)
                        };
                        match outcome {
                            TextKeyOutcome::Cancel => { self.cancel_edit(); }
                            TextKeyOutcome::Submit | TextKeyOutcome::Tab => { self.commit_edit(); }
                            TextKeyOutcome::Newline => {
                                if let Some(edit) = self.devtools.elements.edit.as_mut() {
                                    edit.buffer.insert("\n");
                                }
                            }
                            TextKeyOutcome::Handled | TextKeyOutcome::Unhandled => {}
                        }
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }

                    // Console input - proper text field s cursor / selection / history / clipboard.
                    if self.devtools.panel_open && self.devtools.focus == FocusTarget::DevToolsConsole {
                        let ctrl = self.modifiers.control_key();
                        let shift = self.modifiers.shift_key();
                        let input = &mut self.devtools.console.input;
                        // Pri otevrenem autocomplete: Up/Down/Enter/Tab navigate.
                        let ac_open = self.devtools.console.autocomplete.is_some();
                        if ac_open {
                            match &key_event.logical_key {
                                Key::Named(NamedKey::ArrowUp) => {
                                    if let Some(ac) = &mut self.devtools.console.autocomplete { ac.move_up(); }
                                    if let Some(w) = &self.window { w.request_redraw(); }
                                    return;
                                }
                                Key::Named(NamedKey::ArrowDown) => {
                                    if let Some(ac) = &mut self.devtools.console.autocomplete { ac.move_down(); }
                                    if let Some(w) = &self.window { w.request_redraw(); }
                                    return;
                                }
                                Key::Named(NamedKey::Tab) | Key::Named(NamedKey::Enter) => {
                                    self.accept_autocomplete();
                                    if let Some(w) = &self.window { w.request_redraw(); }
                                    return;
                                }
                                Key::Named(NamedKey::Escape) => {
                                    self.devtools.console.autocomplete = None;
                                    if let Some(w) = &self.window { w.request_redraw(); }
                                    return;
                                }
                                _ => {}
                            }
                        }
                        // Console-specific specialty: Up/Down = history navigation.
                        match &key_event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => {
                                input.history_prev();
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                input.history_next();
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                            _ => {}
                        }
                        // Centralni dispatch (Backspace/Space/Arrow/Home/End/Ctrl shortcuts).
                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                        let outcome = dispatch_text_key(input, &key_event.logical_key, ctrl, shift);
                        match outcome {
                            TextKeyOutcome::Handled => {
                                // Live autocomplete refresh po kazdem typed znaku.
                                self.trigger_autocomplete();
                                if let Some(w) = &self.window { w.request_redraw(); }
                            }
                            TextKeyOutcome::Tab => {
                                self.trigger_autocomplete();
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                            TextKeyOutcome::Cancel => {
                                self.devtools.focus = FocusTarget::Page;
                            }
                            TextKeyOutcome::Newline => {
                                input.insert("\n");
                            }
                            TextKeyOutcome::Submit => {
                                let cmd = self.devtools.console.input.submit();
                                if !cmd.trim().is_empty() {
                                    use crate::devtools::model::console::{LogEntry, LogLevel};
                                    self.devtools.console.push_log(LogEntry {
                                        level: LogLevel::InputEcho,
                                        text: cmd.clone(),
                                        args: Vec::new(),
                                    });
                                    let sel_id = self.devtools.elements.selected;
                                    if let Some(interp) = self.interp_mut() {
                                        let result = console_eval_via_vm(&cmd, interp, sel_id);
                                        match result {
                                            Ok(v) => {
                                                let arg = crate::interpreter::console_args::ConsoleArg::from_jsvalue(&v);
                                                self.devtools.console.push_log(LogEntry {
                                                    level: LogLevel::Result,
                                                    text: v.pretty_print(),
                                                    args: vec![arg],
                                                });
                                            }
                                            Err(e) => self.devtools.console.push_log(LogEntry {
                                                level: LogLevel::Error,
                                                text: e,
                                                args: Vec::new(),
                                            }),
                                        }
                                    }
                                }
                            }
                            TextKeyOutcome::Unhandled => {}
                        }
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                    // Elements search bar input.
                    if self.devtools.panel_open && self.devtools.focus == FocusTarget::DevToolsElementsSearch {
                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                        let ctrl = self.modifiers.control_key();
                        let shift = self.modifiers.shift_key();
                        let outcome = dispatch_text_key(&mut self.devtools.elements.search.query,
                            &key_event.logical_key, ctrl, shift);
                        match outcome {
                            TextKeyOutcome::Cancel => {
                                self.devtools.focus = FocusTarget::Page;
                                self.devtools.elements.search.open = false;
                            }
                            TextKeyOutcome::Submit => {
                                if shift {
                                    if self.devtools.elements.search.current == 0 {
                                        let n = self.devtools.elements.search.matches.len();
                                        if n > 0 { self.devtools.elements.search.current = n - 1; }
                                    } else {
                                        self.devtools.elements.search.current -= 1;
                                    }
                                } else {
                                    let n = self.devtools.elements.search.matches.len();
                                    if n > 0 {
                                        self.devtools.elements.search.current = (self.devtools.elements.search.current + 1) % n;
                                    }
                                }
                                self.jump_to_search_match();
                            }
                            TextKeyOutcome::Handled => {
                                self.run_elements_search();
                            }
                            _ => {}
                        }
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                    // Form input typing: pri focused input/textarea routovat pres
                    // DomInputBuffer + centralni dispatch_text_key. Cursor + selection
                    // zije v NodeData.input_cursor / input_anchor.
                    {
                        let focused_id = super::cascade::get_focused_node();
                        if std::env::var("RWE_INPUT_DBG").is_ok() {
                            eprintln!("[KEYIN] focused_id={:?} dt_focus_text={} ctrl={}",
                                focused_id, self.devtools.focus.is_text_input(), self.modifiers.control_key());
                        }
                        // Ctrl eventy do input handleru pousti jen: AltGr (Ctrl+Alt =
                        // CZ/DE znaky @#{}...) a input-local shortcuts (Ctrl+A/C/V/X
                        // resi dispatch_text_key). Ostatni Ctrl (L/F/P...) propadnou
                        // na browser-level handlery nize. Drive blokovalo VSECHNY
                        // ctrl eventy = @ na CZ layoutu nesel napsat + paste mrtvy.
                        let ctrl_raw = self.modifiers.control_key();
                        let altgr = ctrl_raw && self.modifiers.alt_key();
                        let input_local_ctrl = ctrl_raw && !altgr
                            && matches!(&key_event.logical_key,
                                Key::Character(s) if matches!(s.as_str(),
                                    "a" | "A" | "c" | "C" | "v" | "V" | "x" | "X"));
                        if let Some(fid) = focused_id {
                            if !false && !false
                                && !self.devtools.focus.is_text_input()
                                && (!ctrl_raw || altgr || input_local_ctrl)
                            {
                                let (node_opt, doc_rc) = self.interp().map(|interp| {
                                    let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                                    let n = find_node_by_ptr(&doc_root, fid);
                                    (n, std::rc::Rc::clone(&interp.document))
                                }).unwrap_or((None, std::rc::Rc::new(std::cell::RefCell::new(crate::browser::dom::Document::empty(String::new())))));
                                if let Some(node) = node_opt {
                                    let tag = node.tag_name().unwrap_or_default();
                                    if std::env::var("RWE_INPUT_DBG").is_ok() {
                                        eprintln!("[KEYIN] node tag={}", tag);
                                    }
                                    if matches!(tag.as_str(), "input" | "textarea") {
                                        use crate::browser::dom_input_buffer::DomInputBuffer;
                                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                                        // AltGr (Ctrl+Alt) = znak, ne shortcut (viz vyse).
                                        let ctrl = input_local_ctrl;
                                        let shift = self.modifiers.shift_key();
                                        // type=number: filtruj ne-numericke znaky uz na vstupu
                                        // (Chrome chovani - pismena se do number inputu nedostanou).
                                        // Whitelist znaku je 1. brana; po editu jeste prefix-check
                                        // cele hodnoty (samotne "e"/"+"/"eee" by jinak proslo).
                                        let input_type = node.attr("type")
                                            .map(|t| t.to_lowercase()).unwrap_or_default();
                                        if input_type == "number" && !ctrl {
                                            if let winit::keyboard::Key::Character(s) = &key_event.logical_key {
                                                let ok = s.chars().all(|c|
                                                    c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'));
                                                if !ok {
                                                    // Konzumovat bez efektu (zadny beep, zadny insert).
                                                    if let Some(w) = &self.window { w.request_redraw(); }
                                                    return;
                                                }
                                            }
                                        }
                                        // Hodnota PRED editaci - detekce realne mutace (vs arrow/home).
                                        let value_before = node.attr("value").unwrap_or_default();
                                        let mut buf = DomInputBuffer::new(std::rc::Rc::clone(&node), doc_rc);
                                        let outcome = dispatch_text_key(&mut buf, &key_event.logical_key, ctrl, shift);
                                        let consumed = !matches!(outcome, TextKeyOutcome::Unhandled);
                                        if std::env::var("RWE_INPUT_DBG").is_ok() {
                                            eprintln!("[KEYIN] dispatch outcome consumed={} key={:?}", consumed, key_event.logical_key);
                                        }
                                        if matches!(outcome, TextKeyOutcome::Submit | TextKeyOutcome::Newline) {
                                            if tag == "textarea" {
                                                // Enter v textarea = novy radek (Chrome).
                                                use crate::devtools::model::text_buffer::TextBuffer;
                                                buf.insert("\n");
                                            }
                                            // TODO form submit pro single-line input (ancestor form).
                                        }
                                        drop(buf); // Drop -> commit_back value attr.
                                        // Pri realne zmene value: bump dom_version (jinak
                                        // display stale = "psani se zpozdenim", placeholder
                                        // nezmizi) + fire 'input' event (jinak oninput /
                                        // JS-driven UI se nikdy neaktualizuje). Driv
                                        // commit_back jen tise zapsal attr.
                                        let mut value_after = node.attr("value").unwrap_or_default();
                                        // type=number 2. brana: vysledek musi byt prefix
                                        // platneho cisla ([+-]?dig*(.dig*)?(e[+-]?dig*)?).
                                        // Samotne "e"/"+-"/"1ee" whitelist pustil - revert.
                                        if input_type == "number"
                                            && value_after != value_before
                                            && !is_valid_number_prefix(&value_after)
                                        {
                                            node.attributes.borrow_mut()
                                                .insert("value".to_string(), value_before.clone());
                                            value_after = value_before.clone();
                                        }
                                        if std::env::var("RWE_INPUT_DBG").is_ok() {
                                            eprintln!("[KEYIN] value '{}' -> '{}'", value_before, value_after);
                                        }
                                        if value_after != value_before {
                                            // Typing NEMENI layout (form control ma synteticke
                                            // stabilni dims) -> CONTENT-only bump + primy update
                                            // control_text v cached layout stromu. Plny re-layout
                                            // + re-cascade per pismeno byl zdroj "psani se
                                            // zpozdenim". Validity stav (:valid/:invalid) chyta
                                            // cascade cache klic (hash stavu inputu) - re-cascade
                                            // probehne JEN pri prepnuti stavu.
                                            let node_ptr = std::rc::Rc::as_ptr(&node) as usize;
                                            if let Some(wv) = self.webview.as_mut() {
                                                wv.update_control_text(node_ptr, &value_after);
                                            }
                                            if let Some(interp) = self.interp_mut() {
                                                interp.bump_dom_version_content_only();
                                                let mut ev = crate::interpreter::JsObject::new();
                                                ev.set("type".into(), crate::interpreter::JsValue::Str("input".into()));
                                                ev.set("target".into(), crate::interpreter::JsValue::DomNode(
                                                    std::rc::Rc::clone(&node)));
                                                let ev_val = crate::interpreter::JsValue::Object(
                                                    std::rc::Rc::new(std::cell::RefCell::new(ev)));
                                                let _ = interp.dispatch_event(&node, "input", ev_val);
                                            }
                                        }
                                        if consumed {
                                            self.render();
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Address bar typing.
                    // Find-on-page typing: pri otevrenem overlay capture chars.
                    // Ctrl+Shift+F = toggle FPS counter overlay.
                    if self.modifiers.control_key() && self.modifiers.shift_key() {
                        if let Key::Character(s) = &key_event.logical_key {
                            if s.as_str() == "f" || s.as_str() == "F" {
                                self.show_fps = !self.show_fps;
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                        }
                    }
                    // Ctrl+Shift+D = dump layout tree do souboru pro debug.
                    // Vypise rect/tag/class/text per box do layout-dump.txt.
                    if self.modifiers.control_key() && self.modifiers.shift_key() {
                        if let Key::Character(s) = &key_event.logical_key {
                            if s.as_str() == "d" || s.as_str() == "D" {
                                // Ctrl+Shift+D layout dump - layout_root dead na App
                                // vrstve, dump bude per-WebView v dalsi fazi.
                                return;
                            }
                        }
                    }
                    // Ctrl+Shift+T = cycle theme (Auto/Light/Dark).
                    if self.modifiers.control_key() && self.modifiers.shift_key() {
                        if let Key::Character(s) = &key_event.logical_key {
                            if s.as_str() == "t" || s.as_str() == "T" {
                                use crate::devtools::theme::ThemeMode;
                                self.devtools.theme.mode = match self.devtools.theme.mode {
                                    ThemeMode::Auto => ThemeMode::Light,
                                    ThemeMode::Light => ThemeMode::Dark,
                                    ThemeMode::Dark => ThemeMode::Auto,
                                };
                                println!("[theme] {:?}", self.devtools.theme.mode);
                                self.render();
                                return;
                            }
                        }
                    }
                    // Ctrl+F = toggle find overlay (page) NEBO devtools elements search
                    // (kdyz devtools.panel_open + Tab Elements).
                    if self.modifiers.control_key() {
                        if let Key::Character(s) = &key_event.logical_key {
                            if s.as_str() == "f" || s.as_str() == "F" {
                                if self.devtools.panel_open && self.devtools.tab == crate::devtools::Tab::Elements {
                                    self.devtools.elements.search.open = true;
                                    self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsElementsSearch;
                                } else {
            // (invalid bool assignment removed Session N+22)
                                }
                                self.render();
                                return;
                            }
                            if s.as_str() == "c" || s.as_str() == "C" {
                                // Ctrl+C: copy text v selection rectu do clipboardu.
                                self.copy_selection_to_clipboard();
                                return;
                            }
                            if s.as_str() == "a" || s.as_str() == "A" {
                                // Ctrl+A: select cely document - layout_root dead
                                // na App vrstve, full-doc select bude per-WebView.
                                return;
                            }
                            if s.as_str() == "p" || s.as_str() == "P" {
                                // Ctrl+P: export current page do PDF.
                                self.export_pdf();
                                return;
                            }
                            // Ctrl+L (addr bar), Ctrl+D (bookmarks), Ctrl+H (history)
                            // smazany N+22 - shell concerns.
                            if s.as_str() == "j" || s.as_str() == "J" {
                                // Ctrl+J: open downloads page.
                                self.navigate_url("about:downloads");
                                return;
                            }
                            if s.as_str() == "s" || s.as_str() == "S" {
                                // Ctrl+S: save current page HTML do Downloads/.
                                self.save_page_to_downloads();
                                return;
                            }
                            if s.as_str() == "B" && self.modifiers.shift_key() {
                                // Ctrl+Shift+B: toggle bookmarks bar visibility.
            // (invalid assignment removed Session N+22)
                                self.render();
                                return;
                            }
                            if s.as_str() == "b" || s.as_str() == "B" {
                                // Ctrl+B: open bookmarks page.
                                self.navigate_url("about:bookmarks");
                                return;
                            }
                            // Ctrl+Alt+R = toggle reading mode (zen view).
                            if (s.as_str() == "r" || s.as_str() == "R") && self.modifiers.alt_key() {
            // (invalid assignment removed Session N+22)
                                self.render();
                                return;
                            }
                            // Shell tab shortcuts.
                        }
                        // Ctrl+Tab = next tab smazany N+22 (shell concern).
                    }
                    // Ctrl+= / Ctrl++ / Ctrl+- / Ctrl+0 = zoom controls.
                    if self.modifiers.control_key() {
                        if let Key::Character(s) = &key_event.logical_key {
                            match s.as_str() {
                                "+" | "=" => {
                                    self.set_zoom((self.zoom() * 1.1).min(5.0));
                                    self.clamp_scroll_to_layout();
                                    println!("[zoom] {:.0}%", self.zoom() * 100.0);
                                    self.render();
                                    return;
                                }
                                "-" | "_" => {
                                    self.set_zoom((self.zoom() / 1.1).max(0.25));
                                    self.clamp_scroll_to_layout();
                                    println!("[zoom] {:.0}%", self.zoom() * 100.0);
                                    self.render();
                                    return;
                                }
                                "0" => {
                                    self.set_zoom(1.0);
                                    self.clamp_scroll_to_layout();
                                    println!("[zoom] 100%");
                                    self.render();
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                    // Page keydown JS dispatch: zadna chrome/devtools UI klavesu
                    // nezkonzumovala -> forward do webview (dispatch na focused page
                    // element). Diky tomu funguji onkeydown handlery (napr. JS Events
                    // keyboard zone, tabindex divy). Scroll/F-key default akce nize
                    // bezi dal (browser-like: keydown fire pak default action).
                    if let Some(js_key) = logical_key_to_js(&key_event.logical_key) {
                        if let Some(wv) = self.webview.as_mut() {
                            let resp = wv.handle_input(crate::embed::InputEvent::KeyDown {
                                key: js_key, modifiers: Default::default(),
                            });
                            if resp.dirty { if let Some(w) = &self.window { w.request_redraw(); } }
                        }
                    }
                    match key_event.logical_key {
                        Key::Named(NamedKey::F1) => {
            // (invalid assignment removed Session N+22)
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                        Key::Named(NamedKey::F12) => {
                            self.devtools.panel_open = !self.devtools.panel_open;
                            if self.devtools.panel_open {
                                if let Some(interp) = self.interp() {
                                    let root = std::rc::Rc::clone(&interp.document.borrow().root);
                                    crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                                }
                                // Hybrid: pri otevreni s breakpointy nastav debug mode.
                                if !self.devtools.sources.breakpoints.is_empty() {
                                    self.activate_debug_mode();
                                }
                            } else {
                                // Pri zavreni F12: deaktivuj debug worker.
                                self.deactivate_debug_mode();
                            }
                            println!("[F12] devtools panel = {}", if self.devtools.panel_open { "ON" } else { "OFF" });
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                        Key::Named(NamedKey::F11) => {
                            // F11 = DEPRECATED static devtools.html snapshot.
                            // Pouzivej F12 inline panel pro aktivni vyvoj.
                            println!("[F11 DEPRECATED] static devtools.html snapshot - prefer F12 inline panel");
                            self.regenerate_and_open_devtools();
                        }
                        Key::Named(NamedKey::F5) => {
                            if let Some(p) = self.current_path() {
                                println!("[F5 reload] {}", p.display());
                                self.load_path(&p);
                                self.render();
                            } else if let Some(url) = self.base_url() {
                                println!("[F5 reload] {url}");
                                self.navigate_url_no_history(&url);
                            }
                        }
                        // Alt+Left/Right history back/forward smazany N+22 - shell concern.
                        // Vertikalni scroll keys: PageDown/Up = +/- viewport_h,
                        // ArrowDown/Up = +/- 60 px (line height steps), Space =
                        // PageDown, Shift+Space = PageUp, Home = top, End = bottom.
                        Key::Named(NamedKey::PageDown) => {
                            self.scroll_by_y(self.viewport_h_logical() * 0.9);
                        }
                        Key::Named(NamedKey::PageUp) => {
                            self.scroll_by_y(-self.viewport_h_logical() * 0.9);
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.scroll_by_y(60.0);
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.scroll_by_y(-60.0);
                        }
                        Key::Named(NamedKey::Home) => {
                            // Home: smooth scroll to top.
                            self.set_scroll_target_y(0.0);
                            self.set_scroll_target_x(0.0);
                            self.render();
                        }
                        Key::Named(NamedKey::End) => {
                            // End: scroll to bottom - layout_root dead na App vrstve,
                            // bude per-WebView pres webview.last_layout_root().
                        }
                        Key::Named(NamedKey::Space) => {
                            let dir = if self.modifiers.shift_key() { -1.0 } else { 1.0 };
                            self.scroll_by_y(dir * self.viewport_h_logical() * 0.9);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn find_matches_in(layout: &super::layout::LayoutBox, query: &str) -> Vec<(f32, f32, f32)> {
        let mut out = Vec::new();
        let q = query.to_lowercase();
        if q.is_empty() { return out; }
        fn walk(b: &super::layout::LayoutBox, q: &str, out: &mut Vec<(f32, f32, f32)>) {
            if let Some(text) = &b.text {
                if text.to_lowercase().contains(q) {
                    out.push((b.rect.y, b.rect.x, b.rect.width.max(50.0)));
                }
            }
            for c in &b.children { walk(c, q, out); }
        }
        walk(layout, &q, &mut out);
        out
    }

    impl App {
        fn handle_escape_close_popups(&mut self) -> bool {
            // Close prioritou: bookmark picker > shortcuts overlay > color picker > settings >
            // class manager > tab overflow > addr bar > find > selection.
            // bookmark_picker close smazany (Session N+22).
            if self.devtools.color_picker.is_some() {
                self.devtools.color_picker = None;
                return true;
            }
            if self.devtools.settings_popup_open {
                self.devtools.settings_popup_open = false;
                return true;
            }
            if self.devtools.class_manager_open {
                self.devtools.class_manager_open = false;
                return true;
            }
            if self.devtools.tab_overflow_open {
                self.devtools.tab_overflow_open = false;
                return true;
            }
            // Last resort: clear page selection.
            if self.page_sel_anchor().is_some() {
                self.page_sel_clear();
                return true;
            }
            false
        }
        fn viewport_h_logical(&self) -> f32 {
            self.renderer.as_ref().map(|r| (r.config.height as f32) / (self.zoom() * r.scale_factor)).unwrap_or(768.0)
        }
        fn viewport_w_logical(&self) -> f32 {
            self.renderer.as_ref().map(|r| (r.config.width as f32) / (self.zoom() * r.scale_factor)).unwrap_or(1024.0)
        }
        /// Velikost devtools panelu (na perpendicular axis k dock side).
        /// Bottom/Top: vyska. Left/Right: sirka.
        fn panel_size_logical(&self) -> f32 {
            if !self.devtools.panel_open { return 0.0; }
            use crate::devtools::profile::DockPosition;
            let max_dim = match self.devtools.dock_position {
                DockPosition::Left | DockPosition::Right => self.viewport_w_logical(),
                _ => self.viewport_h_logical(),
            };
            self.devtools.panel_h.min(max_dim * 0.7)
        }
        /// Legacy alias.
        fn panel_h_logical(&self) -> f32 { self.panel_size_logical() }
        /// Vraci rect devtools panelu v logical px: (x, y, w, h).
        fn panel_rect_logical(&self) -> (f32, f32, f32, f32) {
            use crate::devtools::profile::DockPosition;
            let vw = self.viewport_w_logical();
            let vh = self.viewport_h_logical();
            if !self.devtools.panel_open { return (0.0, vh, vw, 0.0); }
            let s = self.panel_size_logical();
            match self.devtools.dock_position {
                DockPosition::Bottom | DockPosition::PopupWindow =>
                    (0.0, vh - s, vw, s),
                DockPosition::Top => (0.0, 0.0, vw, s),
                DockPosition::Left => (0.0, 0.0, s, vh),
                DockPosition::Right => (vw - s, 0.0, s, vh),
            }
        }
        /// Page area rect (viewport bez devtools panelu).
        fn page_rect_logical(&self) -> (f32, f32, f32, f32) {
            use crate::devtools::profile::DockPosition;
            let vw = self.viewport_w_logical();
            let vh = self.viewport_h_logical();
            if !self.devtools.panel_open { return (0.0, 0.0, vw, vh); }
            let s = self.panel_size_logical();
            match self.devtools.dock_position {
                DockPosition::Bottom | DockPosition::PopupWindow =>
                    (0.0, 0.0, vw, vh - s),
                DockPosition::Top => (0.0, s, vw, vh - s),
                DockPosition::Left => (s, 0.0, vw - s, vh),
                DockPosition::Right => (0.0, 0.0, vw - s, vh),
            }
        }
        /// Top edge devtools panelu v logical px - LEGACY (predpokala bottom).
        /// Pouziva existujici hit-test code, dokud nezmigruji na panel_rect.
        fn panel_top_logical(&self) -> f32 {
            self.panel_rect_logical().1
        }
        /// Test: je dany logical-y bod v devtools panelu?
        fn point_in_devtools(&self, logical_x: f32, logical_y: f32) -> bool {
            if !self.devtools.panel_open { return false; }
            let (px, py, pw, ph) = self.panel_rect_logical();
            logical_x >= px && logical_x < px + pw
                && logical_y >= py && logical_y < py + ph
        }
        fn estimate_styles_total_h(&self) -> f32 {
            self.devtools.styles.estimate_total_h()
        }
        fn shell_chrome_h_active(&self) -> f32 {
            // Engine renderuje naked viewport - bez chrome bar (Session N+22).
            0.0
        }
        /// Page commands shift dolu o chrome height (pri shell_mode).
        /// MUSI handlovat VSECHNY varianty DisplayCommand s y - jinak compose
        /// pipelines (FilterBegin/TransformBegin/ClippedRect) zustanou v
        /// puvodni y a renderuji se NAD chrome bar misto pod nim. Pouzivame
        /// segments::shift_command_y ktery zna vsechny varianty.
        fn shift_page_for_chrome(&self, list: &mut [DisplayCommand]) {
            let dy = self.shell_chrome_h_active();
            if dy < 0.5 { return; }
            for cmd in list.iter_mut() {
                segments::shift_command_y(cmd, dy);
            }
        }
    }

    /// Paint chrome bar (tabs + nav) - free fn aby slo volat behem renderer borrow.
    /// Inline style update - replace or append "{prop}: {value};".
    pub fn update_inline_style(cur: &str, prop: &str, value: &str) -> String {
        // Parse cur do prop:value pairs.
        let mut out: Vec<String> = Vec::new();
        let mut found = false;
        for decl in cur.split(';') {
            let decl = decl.trim();
            if decl.is_empty() { continue; }
            if let Some(colon) = decl.find(':') {
                let k = decl[..colon].trim();
                if k == prop {
                    out.push(format!("{}: {}", prop, value));
                    found = true;
                } else {
                    out.push(decl.to_string());
                }
            } else {
                out.push(decl.to_string());
            }
        }
        if !found {
            out.push(format!("{}: {}", prop, value));
        }
        out.join("; ")
    }

    impl App {
        /// Webview interpreter delegate - po polarity invert je interpreter
        /// primary v WebView, ne App. App.interpreter field smazany Phase 99.
        fn interp(&self) -> Option<&crate::interpreter::Interpreter> {
            self.webview.as_ref().and_then(|w| w.interpreter())
        }
        fn interp_mut(&mut self) -> Option<&mut crate::interpreter::Interpreter> {
            self.webview.as_mut().and_then(|w| w.interpreter_mut())
        }

        // Page selection accessors (Document.selection.page_selection)
        // App.selection_* fields zruseny - registry je primary state.

        fn page_sel_anchor(&self) -> Option<(f32, f32)> {
            self.interp()
                .and_then(|i| i.document.borrow().selection.borrow().page_selection.as_ref().map(|p| p.anchor))
        }
        fn page_sel_current(&self) -> Option<(f32, f32)> {
            self.interp()
                .and_then(|i| i.document.borrow().selection.borrow().page_selection.as_ref().map(|p| p.current))
        }
        fn page_sel_dragging(&self) -> bool {
            self.interp()
                .map(|i| i.document.borrow().selection.borrow().page_selection.as_ref().map(|p| p.dragging).unwrap_or(false))
                .unwrap_or(false)
        }
        fn page_sel_begin(&self, anchor: (f32, f32)) {
            let Some(interp) = self.interp() else { return };
            let doc = interp.document.borrow();
            doc.selection.borrow_mut().page_selection = Some(crate::browser::selection::PageSelection {
                anchor,
                current: anchor,
                dragging: true,
                cached_text: String::new(),
            });
        }
        fn page_sel_update_current(&self, current: (f32, f32)) {
            let Some(anchor) = self.page_sel_anchor() else { return };
            let cached = self.compute_selection_text(anchor, current);
            let Some(interp) = self.interp() else { return };
            let doc = interp.document.borrow();
            let mut reg = doc.selection.borrow_mut();
            if let Some(ps) = reg.page_selection.as_mut() {
                ps.current = current;
                ps.cached_text = cached;
            }
        }
        fn page_sel_end_drag(&self) {
            let Some(interp) = self.interp() else { return };
            let doc = interp.document.borrow();
            let mut reg = doc.selection.borrow_mut();
            if let Some(ps) = reg.page_selection.as_mut() {
                ps.dragging = false;
                if (ps.anchor.0 - ps.current.0).abs() < 3.0 && (ps.anchor.1 - ps.current.1).abs() < 3.0 {
                    reg.page_selection = None;
                }
            }
        }
        /// Hit-test (x, y) na painted_text_runs pro per-glyph selection.
        /// Vraci SelectionPos nebo None (mimo vsech runs). Delegate na webview.
        pub fn hit_test_text_run(&self, x: f32, y: f32) -> Option<crate::browser::textrun::SelectionPos> {
            self.webview.as_ref().and_then(|w| w.hit_test_text(x, y))
        }

        /// Extract text z anchor->focus SelectionPos pro Ctrl+C copy.
        /// Per-glyph precision (vs flow-based bbox extract). Delegate na webview.
        pub fn extract_text_run_selection(&self,
            anchor: crate::browser::textrun::SelectionPos,
            focus: crate::browser::textrun::SelectionPos,
        ) -> String {
            let sel = crate::browser::textrun::TextSelection { anchor, focus };
            self.webview.as_ref()
                .map(|w| sel.extract_text(w.text_runs()))
                .unwrap_or_default()
        }

        fn page_sel_clear(&self) {
            let Some(interp) = self.interp() else { return };
            let doc = interp.document.borrow();
            doc.selection.borrow_mut().page_selection = None;
        }
        fn page_sel_set_full(&self, anchor: (f32, f32), current: (f32, f32)) {
            let cached = self.compute_selection_text(anchor, current);
            let Some(interp) = self.interp() else { return };
            let doc = interp.document.borrow();
            doc.selection.borrow_mut().page_selection = Some(crate::browser::selection::PageSelection {
                anchor, current, dragging: false, cached_text: cached,
            });
        }

        /// Flow-based text extract pres LayoutBox tree. layout_root dead na App
        /// vrstve - selection text extract presunut do WebView pipeline.
        fn compute_selection_text(&self, _a: (f32, f32), _c: (f32, f32)) -> String {
            String::new()
        }
        /// Centralni cursor icon dispatch - dle pozice + DOM/devtools state.
        fn compute_cursor_icon(&self, target: Option<&super::layout::LayoutBox>) -> winit::window::CursorIcon {
            use winit::window::CursorIcon;
            use crate::devtools::profile::DockPosition;
            // 1. Devtools panel hit?
            let mx_screen = self.mouse_x - self.scroll_x();
            let my_screen = self.mouse_y - self.scroll_y();
            // Resize grip cursor (per dock).
            if self.devtools.panel_open {
                let (px, py, pw, ph) = self.panel_rect_logical();
                if mx_screen >= px && mx_screen < px + pw
                   && my_screen >= py && my_screen < py + ph {
                    let grip_hit = match self.devtools.dock_position {
                        DockPosition::Bottom | DockPosition::PopupWindow =>
                            my_screen < py + 4.0,
                        DockPosition::Top =>
                            my_screen >= py + ph - 4.0,
                        DockPosition::Left =>
                            mx_screen >= px + pw - 4.0,
                        DockPosition::Right =>
                            mx_screen < px + 4.0,
                    };
                    if grip_hit {
                        return match self.devtools.dock_position {
                            DockPosition::Bottom | DockPosition::Top | DockPosition::PopupWindow =>
                                CursorIcon::RowResize,
                            DockPosition::Left | DockPosition::Right =>
                                CursorIcon::ColResize,
                        };
                    }
                }
            }
            if self.point_in_devtools(mx_screen, my_screen) {
                // Console input + elements search input + inline edit -> Text.
                use crate::devtools::focus::FocusTarget;
                if matches!(self.devtools.focus,
                    FocusTarget::DevToolsConsole | FocusTarget::DevToolsElementsSearch)
                    || self.devtools.elements.edit.is_some() {
                    // Hit-test by mohl byt presnejsi (jen nad input rect), ale Text
                    // je zelec pri text edit panelu uzitecny default.
                    return CursorIcon::Text;
                }
                // Splitter drag zone -> ColResize.
                if self.devtools.tab == crate::devtools::Tab::Elements {
                    let viewport_w = self.viewport_w_logical();
                    let default_split = viewport_w * 0.7;
                    let split_x = if self.devtools.elements.split_x < 1.0 { default_split }
                                  else { self.devtools.elements.split_x.max(200.0).min(viewport_w - 220.0) };
                    if (mx_screen - split_x).abs() < 6.0 {
                        return CursorIcon::ColResize;
                    }
                }
                // Resize grip top -> RowResize.
                if (my_screen - self.panel_top_logical()).abs() < 4.0 {
                    return CursorIcon::RowResize;
                }
                return CursorIcon::Default;
            }
            // 2a. Resize grip (CSS resize: both/horizontal/vertical) -> resize
            // cursor. Bez tohoto user nevidi kde je 16px grip = "nejde resizovat".
            if let Some(root) = self.webview.as_ref().and_then(|w| w.last_layout_root()) {
                if let Some((_, _, _, axis)) =
                    crate::embed::webview::find_resize_grip(root, self.mouse_x, self.mouse_y)
                {
                    return match axis.as_str() {
                        "horizontal" => CursorIcon::EwResize,
                        "vertical" => CursorIcon::NsResize,
                        _ => CursorIcon::NwseResize,
                    };
                }
            }
            // 2. Page main scrollbar -> Default. layout_root dead na App vrstve;
            // scrollbar cursor hit-test patri do WebView pipeline.
            // 3. Page element classify - CSS `cursor` property MA PRIORITU pred
            // InteractiveKind defaultem. Drive se CSS cursor uplne ignoroval ->
            // `cursor:grab/move/not-allowed/...` nefungovaly, draggable divy nemely
            // grab kurzor. (cursor je inherited -> hit box ma efektivni hodnotu.)
            // CSS cursor: walk root->mouse point, deepest set cursor wins (cursor
            // je inherited -> propaguj do anonymnich text boxu co nemaji vlastni).
            // Bez walku by hit na text uzlu (bez cursor) vratil Text misto grab.
            if let Some(root) = self.webview.as_ref().and_then(|w| w.last_layout_root()) {
                fn css_cursor_at(b: &super::layout::LayoutBox, mx: f32, my: f32,
                                 inherited: Option<String>) -> Option<String> {
                    if mx < b.rect.x || mx >= b.rect.x + b.rect.width
                        || my < b.rect.y || my >= b.rect.y + b.rect.height {
                        return None;
                    }
                    let cur = b.cursor.clone().or(inherited);
                    let cx = mx + b.scroll_offset_x;
                    let cy = my + b.scroll_offset_y;
                    for ch in &b.children {
                        if let Some(c) = css_cursor_at(ch, cx, cy, cur.clone()) {
                            return Some(c);
                        }
                    }
                    cur
                }
                if let Some(css) = css_cursor_at(root, self.mouse_x, self.mouse_y, None) {
                    if let Some(icon) = css_cursor_to_icon(&css) {
                        return icon;
                    }
                }
            }
            if let Some(t) = target {
                if let Some(node) = &t.node {
                    let kind = crate::browser::interactive::classify(node);
                    if kind != crate::browser::interactive::InteractiveKind::None {
                        return kind.cursor_icon();
                    }
                }
                if t.text.is_some() {
                    return CursorIcon::Text;
                }
                // Descendant text recurz.
                fn has_text(b: &super::layout::LayoutBox, mx: f32, my: f32) -> bool {
                    if b.text.is_some()
                        && mx >= b.rect.x && mx < b.rect.x + b.rect.width
                        && my >= b.rect.y && my < b.rect.y + b.rect.height { return true; }
                    for c in &b.children { if has_text(c, mx, my) { return true; } }
                    false
                }
                if has_text(t, self.mouse_x, self.mouse_y) {
                    return CursorIcon::Text;
                }
            }
            CursorIcon::Default
        }
        /// Body content y range Elements/Sources/Console - od top toolbaru.
        fn devtools_body_h(&self) -> f32 {
            (self.panel_h_logical() - 4.0 - 30.0).max(0.0)
        }
        /// Export aktualni stranky do PDF souboru. Walk LayoutBox tree, emituje
        /// text uzly + bg rects do printpdf documentu. layout_root dead na App
        /// vrstve - PDF export bude per-WebView pres webview.last_layout_root()
        /// v dalsi fazi.
        fn export_pdf(&mut self) {}
        /// Extrahuje text z LayoutBoxu prekryvajicich selection rect, posle do
        /// system clipboard pres arboard. Selection coords v logical px (uz s
        /// scroll_y aplikovany na mouse).
        fn copy_selection_to_clipboard(&mut self) {
            let (a, c) = match (self.page_sel_anchor(), self.page_sel_current()) {
                (Some(a), Some(c)) => (a, c),
                _ => return,
            };
            // Reuse char-level extractor (compute_selection_text). Pred phase 6
            // copy bral cely text intersect boxes; ted jen chars v selection
            // range pres fontdue advance.
            let trimmed = self.compute_selection_text(a, c);
            if trimmed.is_empty() { return; }
            match arboard::Clipboard::new() {
                Ok(mut cb) => {
                    if let Err(e) = cb.set_text(&trimmed) {
                        eprintln!("[clipboard] set_text fail: {e}");
                    } else {
                        println!("[clipboard] copied {} chars", trimmed.len());
                    }
                }
                Err(e) => eprintln!("[clipboard] open fail: {e}"),
            }
        }
        fn scroll_by_y(&mut self, dy: f32) {
            // Smooth scroll: posun target. Render tick interpoluje scroll_y -> target.
            self.set_scroll_target_y((self.scroll_target_y() + dy).max(0.0));
            self.clamp_scroll_to_layout();
            self.render();
        }
        /// Po zoom change: clamp scroll_y/scroll_x do max scrollu pro nove
        /// layout dimensions. Pri zoomu out se layout zmensi -> overflow muze
        /// zmizet -> max_scroll = 0. layout_root dead na App vrstve - clamp
        /// logic se presune do WebView pipeline pres webview.last_layout_root().
        fn clamp_scroll_to_layout(&mut self) {
            // Clamp scroll na [0, content - viewport] - bez tohoto byl scroll
            // shora omezen (.max(0.0)) ale zdola NE -> nekonecny scroll za konec
            // stranky. content z webview layout rootu (rect = cely dokument).
            let (content_w, content_h) = self.webview.as_ref()
                .and_then(|w| w.last_layout_root())
                .map(|l| (l.rect.width, l.rect.height))
                .unwrap_or((0.0, 0.0));
            if content_h <= 0.0 { return; }
            let vw = self.viewport_w_logical();
            let vh = self.viewport_h_logical();
            let max_x = (content_w - vw).max(0.0);
            let max_y = (content_h - vh).max(0.0);
            if let Some(w) = self.webview.as_mut() {
                w.scroll_target_y = w.scroll_target_y.clamp(0.0, max_y);
                w.scroll_target_x = w.scroll_target_x.clamp(0.0, max_x);
                w.scroll_y = w.scroll_y.clamp(0.0, max_y);
                w.scroll_x = w.scroll_x.clamp(0.0, max_x);
            }
        }
        /// Smooth scroll tick. Lerp scroll_y -> scroll_target_y. Vrati true pokud
        /// stale animuje (volajici by mel request_redraw).
        fn smooth_scroll_tick(&mut self) -> bool {
            let dy = self.scroll_target_y() - self.scroll_y();
            let dx = self.scroll_target_x() - self.scroll_x();
            let mut animating = false;
            if dy.abs() > 0.5 {
                self.set_scroll_y(self.scroll_y() + dy * 0.25);
                animating = true;
            } else {
                self.set_scroll_y(self.scroll_target_y());
            }
            if dx.abs() > 0.5 {
                self.set_scroll_x(self.scroll_x() + dx * 0.25);
                animating = true;
            } else {
                self.set_scroll_x(self.scroll_target_x());
            }
            animating
        }
    }

    impl App {
        /// Nacti novy HTML soubor (drop / F5 reload). Resetuje interpreter, scroll, animace.
        fn load_path(&mut self, path: &std::path::Path) {
            // Akceptuj jen HTML soubory (nebo neznamou priponu).
            let ext_ok = match path.extension().and_then(|e| e.to_str()) {
                Some(e) => matches!(e.to_lowercase().as_str(), "html" | "htm" | "xhtml"),
                None => true,
            };
            if !ext_ok {
                eprintln!("[drop] ignoruji - ne HTML soubor: {}", path.display());
                return;
            }
            let html = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => { eprintln!("[load] nelze nacist {}: {e}", path.display()); return; }
            };
            // Extract CSS: co-located .css + inline <style> + <link rel=stylesheet>.
            let css_path = path.with_extension("css");
            let mut css = std::fs::read_to_string(&css_path).unwrap_or_default();
            // Pre-parse HTML pro extract <style> + <link>. Plny parse az pozdeji.
            let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
            let preview_doc = super::html_parser::parse_html(&html, &url);
            for style in preview_doc.root.get_elements_by_tag("style") {
                css.push('\n');
                css.push_str(&style.text_content());
            }
            for link in preview_doc.root.get_elements_by_tag("link") {
                let rel = link.attr("rel").unwrap_or_default().to_lowercase();
                if rel.contains("stylesheet") {
                    if let Some(href) = link.attr("href") {
                        let resolved = resolve_url(&url, &href);
                        if let Some(p) = resolved.strip_prefix("file:///") {
                            let p = p.replace('/', std::path::MAIN_SEPARATOR_STR);
                            if let Ok(c) = std::fs::read_to_string(&p) {
                                css.push('\n');
                                css.push_str(&c);
                            }
                        } else if resolved.starts_with("http") {
                            if let Some(c) = fetch_text_url(&resolved) {
                                css.push('\n');
                                css.push_str(&c);
                            }
                        }
                    }
                }
            }
            self.set_scroll_y(0.0);
            self.set_scroll_target_y(0.0);
            self.set_scroll_x(0.0);
            self.set_scroll_target_x(0.0);
            // Authoritative WebView restart pres sync_webview - novy webview s
            // real load_html (spousti scripts). Po loadu take_interpreter ->
            // App.interpreter.
            let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
            self.sync_webview(&html, &css, Some(url), Some(path.to_path_buf()));
            // Webview drzi interpreter primarne (polarity invert).
            let page_title = crate::embed::loader::extract_title(self.html())
                .unwrap_or_else(|| path.file_name()
                    .and_then(|n| n.to_str()).unwrap_or("page").to_string());
            // title je primary v webview - sync_webview_from_app + load_html
            // ho nastavi. App.title field smazany (Phase 99 polarity invert step).
            if let Some(w) = &self.window {
                w.set_title(&format_window_title(&page_title, 1));
            }
            // Pokud je auto_devtools zaplo, take regen + open po reload.
            if self.auto_devtools {
                self.regenerate_and_open_devtools();
            }
        }

        /// Regen devtools.html + otevri ho v default OS browseru.
        fn regenerate_and_open_devtools(&self) {
            let Some(interp) = self.interp() else { return };
            let stylesheets = vec![super::css_parser::parse_stylesheet(self.css())];
            let console_log = interp.console_log.borrow().clone();
            let network_log = interp.network_log.borrow().clone();
            // Borrow document, vygeneruj HTML, drop borrow.
            let html_out = {
                let doc = interp.document.borrow();
                let scripts: Vec<String> = doc.root.get_elements_by_tag("script")
                    .iter().map(|s| s.text_content()).collect();
                let script_src = scripts.iter().find(|s| !s.trim().is_empty()).cloned();
                crate::debug_view::devtools::generate_devtools_html(
                    &doc, &stylesheets, script_src.as_deref(), &console_log, &network_log,
                )
            };
            let out_path = std::env::current_dir().map(|d| d.join("devtools.html"))
                .unwrap_or_else(|_| std::path::PathBuf::from("devtools.html"));
            if let Err(e) = std::fs::write(&out_path, &html_out) {
                eprintln!("[devtools] zapis selhal: {e}");
                return;
            }
            println!("[devtools] {} (console: {}, network: {})", out_path.display(), console_log.len(), network_log.len());
            open_in_default_browser(&out_path);
        }

        /// Rerun vsech scriptu po Continue. resume() uz nastavila skip_once_line
        /// na predchozi pause line, takze script se pri stejnem BP nezacykli a
        /// pokracuje az do dalsiho hitu nebo konce.
        fn rerun_paused_scripts(&mut self) {
            if self.interp().is_none() { return }
            // Vytvor novy Interpreter (cisty state) ALE zkopiruj breakpoints +
            // skip_once_line z aktualniho debuggeru aby pause logic fungovala
            // dale. DOM je v interpreter.document - zachova v novem.
            let saved_bp;
            let saved_skip;
            let saved_console;
            let saved_doc;
            {
                let interp = self.interp().unwrap();
                let dbg = interp.debugger.borrow();
                saved_bp = dbg.breakpoints.clone();
                saved_skip = dbg.skip_once_line;
                saved_console = interp.console_log.borrow().clone();
                saved_doc = std::rc::Rc::clone(&interp.document.borrow().root);
            }
            let new_interp = crate::interpreter::Interpreter::new();
            new_interp.set_document(crate::browser::dom::Document {
                selection: std::cell::RefCell::new(crate::browser::selection::SelectionRegistry::new()),
                title: String::new(),
                url: String::new(),
                root: saved_doc,
            });
            *new_interp.console_log.borrow_mut() = saved_console;
            {
                let mut dbg = new_interp.debugger.borrow_mut();
                dbg.breakpoints = saved_bp;
                dbg.skip_once_line = saved_skip;
            }
            // Pres webview run_scripts: presunout interp do webview, vola
            // Webview drzi interpreter primarne (polarity invert).
            if let Some(wv) = self.webview.as_mut() {
                wv.set_interpreter(new_interp);
                wv.run_scripts();
            }
            // (App.interpreter fallback smazany Phase 99 - webview vzdy Some po resumed.)
        }

        /// Notify worker thread pres Condvar - po klik Continue/Step.
        fn notify_continue(&self) {
            // Hybrid debug runner pres vlastni Condvar.
            if let Some(runner) = &self.debug_runner {
                runner.notify_continue();
                return;
            }
            // Fallback: own continue_signal (legacy, pro single-thread early-abort path).
            let (lock, cvar) = &*self.continue_signal;
            let mut continued = lock.lock().unwrap();
            *continued = true;
            cvar.notify_all();
        }

        /// Aktivace hybrid debug mode - spawn worker thread s vlastnim Interpreter.
        /// Vraci true pokud spawn uspesny (nebo uz aktivni). Page UI dale render
        /// cached layout dokud worker neposla Done event.
        fn activate_debug_mode(&mut self) {
            if self.debug_runner.is_some() { return; }
            let html = self.html().to_string();
            let base_url = self.base_url().unwrap_or_default();
            let bp_lines: Vec<u32> = self.devtools.sources.breakpoints.iter()
                .map(|b| b.line).collect();
            let runner = crate::devtools::debug_runner::DebugRunner::spawn(
                html, base_url, bp_lines);
            self.devtools.console.push_log(crate::devtools::model::console::LogEntry {
                level: crate::devtools::model::console::LogLevel::Info,
                text: "[debug-mode] Worker thread spustil eval JS - real freeze pause aktivni".into(),
                args: Vec::new(),
            });
            self.debug_runner = Some(runner);
        }

        /// Deaktivuj debug mode - graceful join worker.
        fn deactivate_debug_mode(&mut self) {
            if let Some(runner) = self.debug_runner.take() {
                runner.notify_continue(); // wake jakkoliv stale paused worker
                runner.join();
                self.devtools.console.push_log(crate::devtools::model::console::LogEntry {
                    level: crate::devtools::model::console::LogLevel::Info,
                    text: "[debug-mode] Worker thread skoncil".into(),
                    args: Vec::new(),
                });
            }
        }

        /// Poll events z debug worker - vola se per render frame.
        fn poll_debug_runner(&mut self) {
            let Some(runner) = &mut self.debug_runner else { return };
            let events = runner.drain_events();
            use crate::devtools::debug_runner::WorkerEvent;
            use crate::devtools::model::console::{LogEntry, LogLevel};
            for ev in events {
                match ev {
                    WorkerEvent::Started => {}
                    WorkerEvent::Log { level, msg } => {
                        let lvl = match level.as_str() {
                            "error" => LogLevel::Error,
                            "warn" => LogLevel::Warn,
                            _ => LogLevel::Info,
                        };
                        self.devtools.console.push_log(LogEntry { level: lvl, text: msg, args: Vec::new() });
                    }
                    WorkerEvent::Network { url, status } => {
                        use crate::devtools::model::network::{NetworkEntry, NetworkResourceType};
                        self.devtools.network.entries.push(NetworkEntry {
                            url: url.clone(),
                            method: "GET".into(),
                            status,
                            resource_type: NetworkResourceType::from_url(&url),
                            size_bytes: 0,
                            duration_ms: 0,
                            started_at_ms: 0,
                        });
                    }
                    WorkerEvent::Pause { line } => {
                        self.devtools.sources.debugger_paused = true;
                        if let Some(file_id) = self.devtools.sources.selected_id
                            .or_else(|| self.devtools.sources.files.first().map(|f| f.id))
                        {
                            self.devtools.sources.current_pause_location = Some((file_id, line));
                        }
                        // Mirror locals z shared dbg.
                        let dbg = runner.debugger.lock().unwrap();
                        self.devtools.sources.locals = dbg.locals.clone();
                    }
                    WorkerEvent::Done => {
                        self.devtools.console.push_log(LogEntry {
                            level: LogLevel::Info,
                            text: "[debug-mode] Script done".into(),
                            args: Vec::new(),
                        });
                        self.devtools.sources.debugger_paused = false;
                        self.devtools.sources.current_pause_location = None;
                    }
                    WorkerEvent::Error(e) => {
                        self.devtools.console.push_log(LogEntry {
                            level: LogLevel::Error,
                            text: format!("[debug-mode] Error: {}", e),
                            args: Vec::new(),
                        });
                    }
                }
            }
            // Po Done event, join worker (uvolni handle).
            if runner.is_finished() {
                self.deactivate_debug_mode();
            }
        }


        fn trigger_autocomplete(&mut self) {
            use crate::devtools::model::console::{suggest, AutocompleteState};
            let text = self.devtools.console.input.text.clone();
            let cursor = self.devtools.console.input.cursor;
            let globals: Vec<String> = if let Some(interp) = self.interp() {
                interp.global.borrow().names()
            } else { Vec::new() };
            // Reflective property resolver: base ident -> own props z JsObject.
            // Hleda v env: kdyz base je top-level var, a hodnota je Object,
            // vrat keys. Jinak prazdny - fallback na hardcoded baseline.
            let interp_ref = self.interp();
            let resolve = |base: &str| -> Vec<String> {
                let Some(interp) = interp_ref else { return Vec::new() };
                let val = interp.global.borrow().get(base);
                match val {
                    Some(crate::interpreter::JsValue::Object(obj)) => {
                        let b = obj.borrow();
                        b.props.keys()
                            .filter(|k| !k.starts_with("__"))
                            .cloned()
                            .collect()
                    }
                    _ => Vec::new(),
                }
            };
            if let Some((start, hits)) = suggest(&text, cursor, &globals, &resolve) {
                self.devtools.console.autocomplete = AutocompleteState::open(hits, start);
            } else {
                self.devtools.console.autocomplete = None;
            }
        }

        fn accept_autocomplete(&mut self) {
            let Some(ac) = self.devtools.console.autocomplete.take() else { return };
            let Some(hit) = ac.hits.get(ac.selected).cloned() else { return };
            let input = &mut self.devtools.console.input;
            let prefix_start = ac.prefix_start;
            let cursor = input.cursor;
            input.text.replace_range(prefix_start..cursor, &hit.text);
            input.cursor = prefix_start + hit.text.len();
        }

        fn start_edit_attribute_value(&mut self, node_id: usize, attr: String) {
            let Some(interp) = self.interp() else { return };
            let root = std::rc::Rc::clone(&interp.document.borrow().root);
            let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) else { return };
            let original = node.attributes.borrow().iter()
                .find(|(k, _)| k.as_str() == attr.as_str())
                .map(|(_, v)| v.clone()).unwrap_or_default();
            use crate::devtools::{EditState, EditTarget};
            use crate::devtools::model::console::ConsoleInput;
            let mut buf = ConsoleInput::new();
            buf.text = original.clone();
            buf.cursor = original.len();
            self.devtools.elements.edit = Some(EditState {
                target: EditTarget::AttributeValue { node_id, attr },
                buffer: buf,
            });
            self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsConsole;
        }

        fn start_edit_text_node(&mut self, node_id: usize) {
            let Some(interp) = self.interp() else { return };
            let root = std::rc::Rc::clone(&interp.document.borrow().root);
            let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) else { return };
            let original = if let crate::browser::dom::NodeKind::Text(t) = &node.kind {
                t.clone()
            } else { return };
            use crate::devtools::{EditState, EditTarget};
            use crate::devtools::model::console::ConsoleInput;
            let mut buf = ConsoleInput::new();
            buf.text = original.clone();
            buf.cursor = original.len();
            self.devtools.elements.edit = Some(EditState {
                target: EditTarget::TextNode { node_id },
                buffer: buf,
            });
            self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsConsole;
        }

        fn start_edit_style_property(&mut self, node_id: usize, property: String) {
            use crate::devtools::{EditState, EditTarget};
            use crate::devtools::model::console::ConsoleInput;
            let original = self.devtools.styles.computed.iter()
                .find(|(k, _)| k == &property).map(|(_, v)| v.clone()).unwrap_or_default();
            let mut buf = ConsoleInput::new();
            buf.text = original.clone();
            buf.cursor = original.len();
            self.devtools.elements.edit = Some(EditState {
                target: EditTarget::InlineStyleProperty { node_id, property },
                buffer: buf,
            });
            self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsConsole;
        }

        fn commit_edit(&mut self) {
            use crate::devtools::EditTarget;
            let Some(edit) = self.devtools.elements.edit.take() else { return };
            let new_value = edit.buffer.text;
            let Some(interp) = self.interp_mut() else { return };
            let root = std::rc::Rc::clone(&interp.document.borrow().root);
            match edit.target {
                EditTarget::AttributeValue { node_id, attr } => {
                    if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                        node.attributes.borrow_mut().insert(attr, new_value);
                    }
                }
                EditTarget::AttributeName { node_id, value } => {
                    let new_name = new_value.trim().to_string();
                    if !new_name.is_empty() {
                        if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                            node.attributes.borrow_mut().insert(new_name, value);
                        }
                    }
                }
                EditTarget::TextNode { node_id } => {
                    // NodeKind nelze in-place mutovat (neni RefCell). Workaround: najit
                    // parent + index v children, vytvorit novy Rc<NodeData> s novym
                    // textem, swap. Stary node se garbage-colectuje (Rc count -> 0).
                    if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                        if let Some(parent) = node.parent.borrow().upgrade() {
                            let mut kids = parent.children.borrow_mut();
                            if let Some(idx) = kids.iter().position(|c| std::rc::Rc::as_ptr(c) as usize == node_id) {
                                let new_node = std::rc::Rc::new(crate::browser::dom::NodeData {
                                    kind: crate::browser::dom::NodeKind::Text(new_value),
                                    attributes: std::cell::RefCell::new(std::collections::HashMap::new()),
                                    parent: std::cell::RefCell::new(std::rc::Rc::downgrade(&parent)),
                                    children: std::cell::RefCell::new(Vec::new()),
                                    listeners: std::cell::RefCell::new(std::collections::HashMap::new()),
                                });
                                kids[idx] = new_node;
                            }
                        }
                    }
                }
                EditTarget::InlineStyleProperty { node_id, property } => {
                    if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                        let mut attrs = node.attributes.borrow_mut();
                        let existing = attrs.iter().find(|(k, _)| k.as_str() == "style")
                            .map(|(_, v)| v.clone()).unwrap_or_default();
                        // Replace nebo append do inline style.
                        let mut decls: Vec<(String, String)> = existing.split(';')
                            .filter_map(|d| {
                                let d = d.trim();
                                if d.is_empty() { return None; }
                                let (k, v) = d.split_once(':')?;
                                Some((k.trim().to_string(), v.trim().to_string()))
                            })
                            .collect();
                        if let Some(idx) = decls.iter().position(|(k, _)| k == &property) {
                            decls[idx].1 = new_value;
                        } else {
                            decls.push((property, new_value));
                        }
                        let new_style = decls.iter().map(|(k, v)| format!("{}: {}", k, v))
                            .collect::<Vec<_>>().join("; ");
                        attrs.insert("style".into(), new_style);
                    }
                }
            }
            // Invalidate caches - cascade + layout musi rebuilt.
            crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
            self.devtools.focus = crate::devtools::focus::FocusTarget::Page;
        }

        fn cancel_edit(&mut self) {
            self.devtools.elements.edit = None;
            self.devtools.focus = crate::devtools::focus::FocusTarget::Page;
        }

        fn dispatch_menu_action(&mut self, action: crate::devtools::context_menu::MenuAction) {
            use crate::devtools::context_menu::MenuAction::*;
            match action {
                // TabClose / TabCloseOthers / TabDuplicate / TabSetGroup /
                // TabPinToggle / TabReload smazany N+22 - multi-tab je shell concern.
                TabClose(_) | TabCloseOthers(_) | TabDuplicate(_)
                | TabSetGroup(..) | TabPinToggle(_) | TabReload(_) => {}
                BookmarkOpen(url) => {
                    self.navigate_url(&url);
                }
                BookmarkDelete(url) => {
                    crate::devtools::bookmarks::remove_bookmark(&url);
                }
                AddAttribute { node_id } => {
                    use crate::devtools::{EditState, EditTarget};
                    use crate::devtools::model::console::ConsoleInput;
                    self.devtools.elements.edit = Some(EditState {
                        target: EditTarget::AttributeName { node_id, value: "".to_string() },
                        buffer: ConsoleInput::new(),
                    });
                    self.devtools.focus = crate::devtools::focus::FocusTarget::DevToolsConsole;
                }
                CopySelector { node_id } | CopyXPath { node_id } | CopyOuterHtml { node_id } | CopyInnerHtml { node_id } => {
                    if let Some(interp) = self.interp() {
                        let root = std::rc::Rc::clone(&interp.document.borrow().root);
                        if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                            let txt = node.tag_name().unwrap_or_default();
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(txt);
                            }
                        }
                    }
                }
                ScrollIntoView { node_id: _ } => {
                    // layout_root dead na App vrstve - scroll-to-node patri
                    // do WebView pipeline (find_layout_box pres webview).
                }
                ExpandAll { node_id } => {
                    let mut to_expand = Vec::new();
                    collect_subtree_ids(&self.devtools.elements.rows, node_id, &mut to_expand);
                    for id in to_expand {
                        self.devtools.elements.collapsed.remove(&id);
                    }
                    if let Some(interp) = self.interp() {
                        let root = std::rc::Rc::clone(&interp.document.borrow().root);
                        crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                    }
                }
                CollapseAll { node_id } => {
                    let mut to_collapse = Vec::new();
                    collect_subtree_ids(&self.devtools.elements.rows, node_id, &mut to_collapse);
                    for id in to_collapse {
                        self.devtools.elements.collapsed.insert(id);
                    }
                    if let Some(interp) = self.interp() {
                        let root = std::rc::Rc::clone(&interp.document.borrow().root);
                        crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                    }
                }
                ClearConsole => {
                    self.devtools.console.log.clear();
                    if let Some(interp) = self.interp() {
                        interp.console_log.borrow_mut().clear();
                    }
                }
                Copy => {
                    if let Some(t) = self.devtools.console.input.selected_text() {
                        if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(t); }
                    }
                }
                Cut => {
                    if let Some(t) = self.devtools.console.input.cut() {
                        if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(t); }
                    }
                }
                Paste => {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        if let Ok(t) = cb.get_text() {
                            self.devtools.console.input.insert(&t);
                        }
                    }
                }
                SelectAll => { self.devtools.console.input.select_all(); }
                CopyUrl { idx } => {
                    if let Some(e) = self.devtools.network.entries.get(idx) {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(e.url.clone());
                        }
                    } else if let Some(interp) = self.interp() {
                        let logs = interp.network_log.borrow();
                        if let Some((url, _)) = logs.get(idx) {
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(url.clone());
                            }
                        }
                    }
                }
                CopyAsCurl { idx } => {
                    let url = self.devtools.network.entries.get(idx).map(|e| e.url.clone())
                        .or_else(|| self.interp()
                            .and_then(|i| i.network_log.borrow().get(idx).map(|(u, _)| u.clone())));
                    if let Some(u) = url {
                        let curl = format!("curl '{}' -A 'RustWebEngine/0.1'", u);
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(curl);
                        }
                    }
                }
                AddBreakpoint { file_id, line } => {
                    self.devtools.sources.toggle_breakpoint(file_id, line);
                }
                RemoveAllBreakpoints => {
                    self.devtools.sources.breakpoints.clear();
                }
                _ => {}
            }
            self.render();
        }

        fn run_elements_search(&mut self) {
            let q = self.devtools.elements.search.query.text.clone();
            let mode = self.devtools.elements.search.mode;
            self.devtools.elements.search.matches.clear();
            self.devtools.elements.search.current = 0;
            if q.trim().is_empty() { return; }
            if let Some(interp) = self.interp() {
                let root = std::rc::Rc::clone(&interp.document.borrow().root);
                let hits = crate::devtools::search::search(&root, &q, mode);
                self.devtools.elements.search.matches = hits;
            }
        }

        fn jump_to_search_match(&mut self) {
            let s = &self.devtools.elements.search;
            if let Some(node_id) = s.matches.get(s.current).copied() {
                self.select_and_reveal_node(node_id);
            }
        }

        /// Select node v Elements tree + uncollapse cely parent chain aby radek
        /// byl viditelny + rebuild rows. Pouziva search jump, page-side inspect
        /// click, externi inspect API (CDP $0).
        fn select_and_reveal_node(&mut self, node_id: usize) {
            self.devtools.elements.selected = Some(node_id);
            if let Some(interp) = self.interp() {
                let root = std::rc::Rc::clone(&interp.document.borrow().root);
                if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                    let mut p = node.parent.borrow().upgrade();
                    while let Some(par) = p {
                        let pid = std::rc::Rc::as_ptr(&par) as usize;
                        self.devtools.elements.collapsed.remove(&pid);
                        p = par.parent.borrow().upgrade();
                    }
                }
                crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
            }
            // Auto-otevri devtools panel pokud zavren.
            if !self.devtools.panel_open {
                self.devtools.panel_open = true;
            }
            // Switch na Elements tab.
            self.devtools.tab = crate::devtools::Tab::Elements;
        }

        fn handle_click(&mut self, x: f32, y: f32) {
            // Hit-test pres open dropdown popup PRED layout hit_test.
            if let Some((select_id, anchor_x, anchor_y, anchor_w)) = self.open_select {
                let popup_x = anchor_x;
                let popup_y = anchor_y + 24.0; // y v page-space (bez -scroll); klik je page-space.
                let opt_h = 24.0_f32;
                if let Some(interp) = self.interp() {
                    let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                    if let Some(select_node) = find_node_by_ptr(&doc_root, select_id) {
                        let options: Vec<std::rc::Rc<crate::browser::dom::Node>> = select_node.children.borrow()
                            .iter().filter(|c| c.tag_name().as_deref() == Some("option")).cloned().collect();
                        let popup_h = opt_h * options.len() as f32;
                        // Klik mimo popup -> close.
                        let in_popup = x >= popup_x && x < popup_x + anchor_w
                            && y >= popup_y && y < popup_y + popup_h;
                        if in_popup {
                            let idx = ((y - popup_y) / opt_h) as usize;
                            if let Some(opt) = options.get(idx) {
                                for ch in select_node.children.borrow().iter() {
                                    if ch.tag_name().as_deref() == Some("option") {
                                        ch.attributes.borrow_mut().remove("selected");
                                    }
                                }
                                opt.attributes.borrow_mut().insert("selected".to_string(), "selected".to_string());
                            }
                            self.open_select = None;
                            self.render();
                            return;
                        } else {
                            self.open_select = None;
                            self.render();
                            // Pokracuj normalne s hit-test.
                        }
                    }
                }
            }
            // layout_root + interpreter dead na App vrstve - hit-test/dispatch
            // (form submit, link nav, checkbox toggle, click listener fire)
            // bude per-WebView v dalsi fazi. Zatim handle_click resi jen
            // open_select popup nahore.
        }

        /// Write arbitrary style value (z editing buffer) na selected node inline style.
        /// Swap interpreter mezi App.interpreter a target_tab.stored_interpreter.
        /// Stash aktualni interp do soucasneho aktivniho tabu pred prepnutim.
        /// switch_tab_with_swap smazany N+22 - single-tab, no-op.
        fn switch_tab_with_swap(&mut self, _target_idx: usize) {}

        /// Ctrl+S = save current page HTML do $HOME/Downloads + zaznam do downloads tracker.
        fn save_page_to_downloads(&mut self) {
            let dl_dir = std::env::var("USERPROFILE").ok()
                .or_else(|| std::env::var("HOME").ok())
                .map(|h| std::path::PathBuf::from(h).join("Downloads"));
            let Some(dir) = dl_dir else { return };
            let _ = std::fs::create_dir_all(&dir);
            let url = self.base_url().unwrap_or_else(|| "page".to_string());
            // Filename z URL last segment + timestamp pro unique.
            let base = url.split('/').last().unwrap_or("page");
            let safe: String = base.chars()
                .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
                .collect();
            let ts = crate::devtools::downloads::now_ts();
            let fname = if safe.is_empty() || safe == "_" {
                format!("page_{}.html", ts)
            } else if safe.ends_with(".html") || safe.ends_with(".htm") {
                format!("{}_{}.html", &safe[..safe.len()-5.min(safe.len())], ts)
            } else {
                format!("{}_{}.html", safe, ts)
            };
            let path = dir.join(&fname);
            if let Err(e) = std::fs::write(&path, self.html()) {
                eprintln!("[save] {}: {}", path.display(), e);
                return;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let rec = crate::devtools::downloads::DownloadRecord {
                url, path: path.display().to_string(),
                size_bytes: size, timestamp_ts: ts,
                mime: "text/html".to_string(),
            };
            crate::devtools::downloads::append_record(&rec);
            println!("[save] {} ({} B)", path.display(), size);
        }

        fn write_back_style_edit(&mut self, prop: &str, value: &str) {
            let Some(node_id) = self.devtools.elements.selected else { return };
            if let Some(interp) = self.interp() {
                let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                if let Some(node) = find_node_by_ptr(&doc_root, node_id) {
                    let mut attrs = node.attributes.borrow_mut();
                    let cur_style = attrs.get("style").cloned().unwrap_or_default();
                    let new_style = update_inline_style(&cur_style, prop, value);
                    let old_value = cur_style.clone();
                    attrs.insert("style".to_string(), new_style.clone());
                    drop(attrs);
                    self.devtools.changes.push(crate::devtools::ChangeEntry {
                        timestamp_ts: crate::devtools::history::now_ts(),
                        kind: crate::devtools::ChangeKind::StyleEdit,
                        target_node_id: node_id,
                        property: prop.to_string(),
                        old_value,
                        new_value: new_style,
                    });
                }
            }
        }

        /// Write picker color do source CSS (inline style attr na target node).
        fn write_back_color_picker(&mut self) {
            let Some(cp) = self.devtools.color_picker.clone() else { return };
            let Some((node_id, prop)) = cp.target else { return };
            let hex = format!("#{:02x}{:02x}{:02x}", cp.rgba[0], cp.rgba[1], cp.rgba[2]);
            if let Some(interp) = self.interp() {
                let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                if let Some(node) = find_node_by_ptr(&doc_root, node_id) {
                    let mut attrs = node.attributes.borrow_mut();
                    let cur_style = attrs.get("style").cloned().unwrap_or_default();
                    // Replace prop value v inline style nebo append.
                    let new_style = update_inline_style(&cur_style, &prop, &hex);
                    let old_value = cur_style.clone();
                    attrs.insert("style".to_string(), new_style.clone());
                    drop(attrs);
                    // Changes log.
                    self.devtools.changes.push(crate::devtools::ChangeEntry {
                        timestamp_ts: crate::devtools::history::now_ts(),
                        kind: crate::devtools::ChangeKind::StyleEdit,
                        target_node_id: node_id,
                        property: prop,
                        old_value,
                        new_value: new_style,
                    });
                }
            }
        }

        /// navigate_about smazany N+22 - about: pages (newtab, config, history,
        /// bookmarks, about, downloads) jsou shell concern.
        fn navigate_about(&mut self, _url: &str) -> bool { false }

        /// Navigate (history persist v ~/.rwe profile, in-memory back/forward
        /// stack smazany N+22 - shell concern).
        fn navigate_url(&mut self, url: &str) {
            self.navigate_url_no_history(url);
            // Persist v profile history (~/.rwe/profiles/<active>/history.json).
            // Pouzij realny <title> tagu (z aktivniho tab po nav), fallback URL last segment.
            let title = self.webview.as_ref().map(|w| w.title().to_string()).unwrap_or_default();
            crate::devtools::history::append_entry(&crate::devtools::history::HistoryEntry {
                url: url.to_string(),
                title,
                visited_at: crate::devtools::history::now_ts(),
            });
        }

        /// Navigate bez modifikace history (back/forward use this).
        fn navigate_url_no_history(&mut self, url: &str) {
            // about: URL handled internally.
            if url.starts_with("about:") {
                if self.navigate_about(url) {
                    self.render();
                    return;
                }
            }
            let is_url = url.starts_with("http://") || url.starts_with("https://");
            if is_url {
                let html = match fetch_text_url(url) { Some(s) => s, None => return };
                // Extract <link> CSS + inline <style>.
                let document = super::html_parser::parse_html(&html, url);
                let mut css = String::new();
                for link in document.root.get_elements_by_tag("link") {
                    let rel = link.attr("rel").unwrap_or_default().to_lowercase();
                    if rel.contains("stylesheet") {
                        if let Some(href) = link.attr("href") {
                            let resolved = resolve_url(url, &href);
                            if let Some(c) = fetch_text_url(&resolved) { css.push('\n'); css.push_str(&c); }
                        }
                    }
                }
                for style in document.root.get_elements_by_tag("style") {
                    css.push('\n'); css.push_str(&style.text_content());
                }
                self.set_scroll_y(0.0);
                self.set_scroll_target_y(0.0);
                self.set_scroll_x(0.0);
                self.set_scroll_target_x(0.0);
                self.sync_webview(&html, &css, Some(url.to_string()), None);
                // Webview drzi interpreter primarne (polarity invert).
                let page_title = crate::embed::loader::extract_title(self.html())
                    .unwrap_or_else(|| url.to_string());
                // title je primary v webview - sync_webview_from_app + load_html
            // ho nastavi. App.title field smazany (Phase 99 polarity invert step).
                if let Some(w) = &self.window {
                    w.set_title(&format_window_title(&page_title, 1usize));
                }
                self.render();
            } else if let Some(rest) = url.strip_prefix("file:///") {
                let path = std::path::PathBuf::from(rest.replace('/', std::path::MAIN_SEPARATOR_STR));
                self.load_path(&path);
                self.render();
            } else {
                let path = std::path::PathBuf::from(url);
                self.load_path(&path);
                self.render();
            }
        }

        /// Update :hover na zaklade aktualni mouse position. Vola se z CursorMoved.
        fn update_hover(&mut self) {
            // Page hover (set_hovered_node) presunut do webview pipeline; App layer
            // resi jen devtools tooltip swatch/var chip hover.
            // Tooltip update: hover nad swatch -> hex string, var chip -> name.
            self.devtools.tooltip = None;
            if self.devtools.panel_open {
                let mx_screen = self.mouse_x - self.scroll_x();
                let my_screen = self.mouse_y - self.scroll_y();
                let zones = self.devtools.styles.swatch_zones.borrow();
                for (zx, zy, zw, zh, col, _prop) in zones.iter() {
                    if mx_screen >= *zx && mx_screen < zx + zw
                       && my_screen >= *zy && my_screen < zy + zh {
                        self.devtools.tooltip = Some(crate::devtools::TooltipState {
                            x: zx + zw + 4.0,
                            y: zy - 6.0,
                            text: format!("#{:02x}{:02x}{:02x} alpha={}", col[0], col[1], col[2], col[3]),
                        });
                        break;
                    }
                }
                drop(zones);
                if self.devtools.tooltip.is_none() {
                    let vzones = self.devtools.styles.var_zones.borrow();
                    for (zx, zy, zw, zh, name) in vzones.iter() {
                        if mx_screen >= *zx && mx_screen < zx + zw
                           && my_screen >= *zy && my_screen < zy + zh {
                            self.devtools.tooltip = Some(crate::devtools::TooltipState {
                                x: zx + zw + 4.0,
                                y: zy - 6.0,
                                text: format!("Klikni pro skok na {}", name),
                            });
                            break;
                        }
                    }
                }
            }
            // Force-hover/focus/active z styles toolbar - prepise reality.
            if self.devtools.force_hover {
                if let Some(sel) = self.devtools.elements.selected {
                    super::cascade::set_hovered_node(Some(sel));
                }
            }
            if self.devtools.force_focus {
                if let Some(sel) = self.devtools.elements.selected {
                    super::cascade::set_focused_node(Some(sel));
                }
            }
            // Devtools tree hover: mouse v devtools panelu nad Elements tree
            // -> set hovered (Firefox-style page overlay). Mimo tree -> clear.
            // Inspect mode prepise hover na page-side hit-test.
            let mx_screen = self.mouse_x - self.scroll_x();
            let my_screen = self.mouse_y - self.scroll_y();
            let in_devtools = self.point_in_devtools(mx_screen, my_screen);
            let mut tree_hover_id: Option<usize> = None;
            if in_devtools && self.devtools.tab == crate::devtools::Tab::Elements {
                let viewport_w = self.viewport_w_logical();
                let panel_top = self.panel_top_logical();
                let panel_h = self.panel_h_logical();
                let default_tree_split = (viewport_w - self.devtools.side_panel_w) * 0.45;
                let tree_split = if self.devtools.elements.split_x < 1.0 { default_tree_split }
                                 else { self.devtools.elements.split_x.max(200.0).min(viewport_w - self.devtools.side_panel_w - 200.0) };
                let body_y = panel_top + 4.0 + 30.0
                    + if self.devtools.elements.search.open { 28.0 } else { 0.0 };
                let _body_h = panel_h - 4.0 - 30.0
                    - if self.devtools.elements.search.open { 28.0 } else { 0.0 };
                if mx_screen < tree_split && my_screen >= body_y {
                    let row_idx = ((my_screen - body_y + self.devtools.elements.scroll_y) / 18.0) as usize;
                    if row_idx < self.devtools.elements.rows.len() {
                        tree_hover_id = Some(self.devtools.elements.rows[row_idx].node_id);
                    }
                }
            }
            if self.devtools.inspect_mode {
                // Inspect mode hover ID by se ziskal pres webview hit-test; zatim None.
                self.devtools.elements.hovered = None;
            } else if tree_hover_id.is_some() {
                self.devtools.elements.hovered = tree_hover_id;
            } else if self.devtools.elements.hovered.is_some() {
                self.devtools.elements.hovered = None;
            }
            // Cursor icon stack - jeden compute_cursor_icon() s prioritou:
            // 1. Devtools panel? -> dle hit_test (search/console = Text, scrollbar/btn = Default/Pointer)
            // 2-5: Page hit - hit-test webview layout root na mouse content pos.
            //   Drive target=None (layout_root dead na App) -> page cursor (pointer
            //   nad button/a, I-beam nad textem) se NIKDY neaplikoval. Ted hit-test
            //   pres webview.last_layout_root().
            if self.window.is_some() {
                let (sc_x, sc_y) = (self.scroll_x(), self.scroll_y());
                let target_box = self.webview.as_ref()
                    .and_then(|wv| wv.last_layout_root())
                    .and_then(|root| root.hit_test_scrolled(self.mouse_x, self.mouse_y, sc_x, sc_y));
                if std::env::var("RWE_CURSOR_DBG").is_ok() {
                    let tag = target_box.and_then(|b| b.node.as_ref())
                        .and_then(|n| n.tag_name()).unwrap_or_else(|| "?".into());
                    eprintln!("[CURSOR] mouse=({:.0},{:.0}) hit_tag={} icon={:?}",
                        self.mouse_x, self.mouse_y, tag, self.compute_cursor_icon(target_box));
                }
                let icon = self.compute_cursor_icon(target_box);
                if let Some(window) = &self.window { window.set_cursor(icon); }
            }
        }

        /// Novy thin render path - webview je primary, App emit overlays + present.
        /// Default cesta od polarity invert kompletni. Legacy 1266-LOC App.render
        /// fallback pres `RWE_RENDER_LEGACY=1` env var.
        fn render_via_webview(&mut self) {
            self.poll_debug_runner();
            // COALESCED hover: zpracuj posledni mouse pozici 1x pred renderem
            // (hit-test + JS mouseover/mousemove dispatch + :hover cascade update).
            // Slouci desitky CursorMoved/frame do jednoho = bez hover lagu.
            if let Some((hx, hy)) = self.pending_hover.take() {
                if let Some(wv) = self.webview.as_mut() {
                    let _t0 = std::time::Instant::now();
                    let _ = wv.handle_input(crate::embed::InputEvent::MouseMove {
                        x: hx, y: hy,
                        modifiers: Default::default(),
                        coalesced: Vec::new(),
                    });
                    if std::env::var("RWE_PROF").is_ok() {
                        let ms = _t0.elapsed().as_secs_f32() * 1000.0;
                        if ms > 1.0 {
                            eprintln!("[INPUT] handle_input(hover MouseMove) = {:.1}ms (hit-test + DOM eventy + JS)", ms);
                        }
                    }
                }
            }
            self.sync_devtools_from_interp();
            let _ = self.smooth_scroll_tick();
            // Sync zoom from App-side state (Ctrl+= adjusts) do webview.
            let cur_zoom_val = self.zoom();
            if let Some(wv) = self.webview.as_mut() {
                wv.set_zoom(cur_zoom_val);
            }
            // Render page do offscreen RT.
            let renderer = match &mut self.renderer { Some(r) => r, None => return };
            let webview = match &mut self.webview { Some(w) => w, None => return };
            let _rv0 = std::time::Instant::now();
            if webview.render_via(renderer).is_none() { return; }
            let _rv1 = std::time::Instant::now();
            // Overlay pass - paint devtools panel + inspector overlay + FPS nad
            // webview RT (start_clear=false ABS extra draw NEsmaz page).
            let layout_clone = webview.last_layout_root().cloned();
            let _rv2 = std::time::Instant::now();
            let target_view_present = webview.target_view().is_some();
            if let (Some(layout), true) = (layout_clone, target_view_present) {
                let mut overlay_cmds = Vec::new();
                let (vw, vh) = webview.viewport_size();
                let mouse_pos = (self.mouse_x, self.mouse_y);
                let cur_scroll_y = self.scroll_y();
                let cur_scroll_x = self.scroll_x();
                // Inspector overlays + element highlight.
                let chrome_dx = -cur_scroll_x;
                crate::browser::devtools_panel::paint_element_highlight_offset(
                    &mut overlay_cmds, &layout, &self.devtools,
                    cur_scroll_y, chrome_dx, 0.0);
                crate::browser::devtools_panel::paint_inspector_overlays(
                    &mut overlay_cmds, &layout, &self.devtools, cur_scroll_y);
                // Devtools panel UI paint.
                self.devtools.tick_frame();
                // Navigation detection - pri zmene nav_id drain sources + reset stav.
                if let Some(wv) = self.webview.as_mut() {
                    let cur_nav = wv.nav_id();
                    if cur_nav != self.devtools.last_nav_id {
                        let sources = wv.take_collected_sources();
                        // Reset Sources files - fresh sada per navigaci.
                        self.devtools.sources.files.clear();
                        self.devtools.sources.selected_id = None;
                        for (url, body, lang_marker) in sources {
                            use crate::devtools::model::sources::SourceLang;
                            let lang = match lang_marker {
                                "js"   => SourceLang::JavaScript,
                                "css"  => SourceLang::Css,
                                "html" => SourceLang::Html,
                                _      => SourceLang::Other,
                            };
                            self.devtools.sources.add_file(url, body, lang);
                        }
                        // Auto-select prvni JS file pokud nejaky existuje.
                        if let Some(first_js) = self.devtools.sources.files.iter()
                            .find(|f| f.language == crate::devtools::model::sources::SourceLang::JavaScript)
                        {
                            self.devtools.sources.selected_id = Some(first_js.id);
                        }
                        self.devtools.last_nav_id = cur_nav;
                    }
                }
                // Live DOM mutation detection - pokud webview DOM se zmenil
                // (appendChild/setAttribute/innerHTML z JS), rebuild Elements tree.
                if let Some(wv) = self.webview.as_ref() {
                    let cur_version = wv.dom_version();
                    if cur_version != self.devtools.last_dom_version {
                        if let Some(doc) = wv.document() {
                            let root = std::rc::Rc::clone(&doc.root);
                            crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                        }
                        self.devtools.last_dom_version = cur_version;
                    }
                }
                let interp_ref_opt = self.webview.as_ref().and_then(|w| w.interpreter());
                paint_devtools_panel(
                    &mut overlay_cmds, &layout, &self.devtools, interp_ref_opt,
                    vw, vh, mouse_pos.0 - cur_scroll_x, mouse_pos.1 - cur_scroll_y);
                // FPS counter.
                if self.show_fps && !self.frame_times_ms.is_empty() {
                    let avg_ms = self.frame_times_ms.iter().sum::<f32>() / self.frame_times_ms.len() as f32;
                    let fps = if avg_ms > 0.01 { 1000.0 / avg_ms } else { 999.0 };
                    let max_ms = self.frame_times_ms.iter().cloned().fold(0.0_f32, f32::max);
                    let (rect_w, rect_h) = (130.0_f32, 36.0_f32);
                    let fps_x = vw - rect_w - 8.0;
                    let fps_y = 8.0;
                    let color = if fps >= 50.0 { [80, 220, 120, 255] }
                        else if fps >= 30.0 { [240, 200, 80, 255] }
                        else { [240, 80, 80, 255] };
                    overlay_cmds.push(DisplayCommand::Rect {
                        x: fps_x, y: fps_y, w: rect_w, h: rect_h,
                        color: [20, 20, 26, 220], radius: 4.0,
                    });
                    overlay_cmds.push(DisplayCommand::Text {
                        x: fps_x + 8.0, y: fps_y + 4.0,
                        content: format!("{:.0} FPS  {:.1}ms", fps, avg_ms),
                        color, font_size: 12.0, bold: true, font_weight: 700,
                        italic: false, font_family: "CamingoMono".into(),
                        strikethrough: false, underline: false,
                    });
                    overlay_cmds.push(DisplayCommand::Text {
                        x: fps_x + 8.0, y: fps_y + 20.0,
                        content: format!("max {:.1}ms", max_ms),
                        color: [180, 180, 190, 200],
                        font_size: 10.0, bold: false, font_weight: 400, italic: false,
                        font_family: "CamingoMono".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // Warm atlas pro overlay cmds + draw.
                let webview = self.webview.as_mut().unwrap();
                let renderer = self.renderer.as_mut().unwrap();
                renderer.warm_atlas_for(&overlay_cmds, webview.base_url());
                if let Some(view) = webview.target_view() {
                    let _ = renderer.draw_segments_into_view_clipped(
                        view, &overlay_cmds, false, None);
                }
            }
            // Present do swap chain.
            let _rv3 = std::time::Instant::now();
            let webview = self.webview.as_ref().unwrap();
            let renderer = self.renderer.as_ref().unwrap();
            if let Some(view) = webview.target_view() {
                renderer.present_external_to_swap_chain(view);
            }
            if std::env::var("RWE_RESIZE_DBG").is_ok() {
                let total = _rv0.elapsed().as_secs_f32()*1000.0;
                if total > 30.0 {
                    eprintln!("[RVW] total={:.1} render_via={:.1} layout_clone={:.1} overlay={:.1} present={:.1}",
                        total,
                        _rv1.duration_since(_rv0).as_secs_f32()*1000.0,
                        _rv2.duration_since(_rv1).as_secs_f32()*1000.0,
                        _rv3.duration_since(_rv2).as_secs_f32()*1000.0,
                        _rv3.elapsed().as_secs_f32()*1000.0);
                }
            }
        }

        /// Sync devtools state z interpretu (console/network log mirroring).
        fn sync_devtools_from_interp(&mut self) {
            // Sync interpreter breakpoints from devtools state.
            let bp_lines: std::collections::HashSet<u32> = self.devtools.sources.breakpoints.iter()
                .map(|b| b.line).collect();
            let paused_info: Option<(u32, Vec<(String, String)>)>;
            if let Some(interp) = self.webview.as_ref().and_then(|w| w.interpreter()) {
                let mut dbg = interp.debugger.borrow_mut();
                if dbg.breakpoints != bp_lines {
                    dbg.breakpoints = bp_lines;
                }
                paused_info = dbg.paused_at.map(|l| (l, dbg.locals.clone()));
            } else {
                paused_info = None;
            }
            if let Some((line, locals)) = paused_info {
                if let Some(file_id) = self.devtools.sources.selected_id {
                    self.devtools.sources.current_pause_location = Some((file_id, line));
                    self.devtools.sources.debugger_paused = true;
                    self.devtools.sources.locals = locals;
                }
            } else {
                self.devtools.sources.debugger_paused = false;
                self.devtools.sources.locals.clear();
            }
            // Mirror console_log + console_log_args do DevToolsState.
            // Paruje i-ty entry s i-tymi args (parallel arrays z interpreter).
            let new_logs: Vec<(String, String)>;
            let new_args: Vec<Vec<crate::interpreter::console_args::ConsoleArg>>;
            if let Some(interp) = self.webview.as_ref().and_then(|w| w.interpreter()) {
                let logs = interp.console_log.borrow();
                let args = interp.console_log_args.borrow();
                let already = self.devtools.console.log.len();
                new_logs = if logs.len() > already {
                    logs.iter().skip(already).cloned().collect()
                } else { Vec::new() };
                new_args = if args.len() > already {
                    args.iter().skip(already).cloned().collect()
                } else { Vec::new() };
            } else {
                new_logs = Vec::new();
                new_args = Vec::new();
            }
            if !new_logs.is_empty() {
                use crate::devtools::model::console::{LogEntry, LogLevel};
                for (i, (level, msg)) in new_logs.into_iter().enumerate() {
                    let lvl = match level.as_str() {
                        "error" => LogLevel::Error,
                        "warn" => LogLevel::Warn,
                        _ => LogLevel::Info,
                    };
                    let entry_args = new_args.get(i).cloned().unwrap_or_default();
                    self.devtools.console.log.push(LogEntry { level: lvl, text: msg, args: entry_args });
                }
                self.devtools.console.stick_to_bottom = true;
            }
        }

        fn render(&mut self) {
            // Wall-clock frame cadence: delta mezi po sobe jdoucimi render()
            // volani. Pri kontinualnim animacnim redrawu = realne FPS co user vidi
            // (vc. vsync wait + cele pipeline). Idle gapy (>500ms) ignorujeme.
            let now = std::time::Instant::now();
            if let Some(last) = self.last_render_instant {
                let dt = now.duration_since(last).as_secs_f32() * 1000.0;
                if dt > 0.0 && dt < 500.0 {
                    if self.frame_times_ms.len() >= 60 { self.frame_times_ms.pop_front(); }
                    self.frame_times_ms.push_back(dt);
                }
            }
            self.last_render_instant = Some(now);

            self.render_via_webview();

            // FPS do window title (throttle ~kazdych 12 framu - set_title syscall).
            self.frames_since_title += 1;
            if self.frames_since_title >= 12 && self.frame_times_ms.len() >= 5 {
                self.frames_since_title = 0;
                let avg = self.frame_times_ms.iter().sum::<f32>()
                    / self.frame_times_ms.len() as f32;
                let fps = if avg > 0.01 { 1000.0 / avg } else { 999.0 };
                let page_title = self.webview.as_ref()
                    .map(|w| w.title().to_string()).unwrap_or_default();
                let base = if page_title.is_empty() { "RustWebEngine" } else { &page_title };
                if let Some(w) = &self.window {
                    w.set_title(&format!("{} - {:.0} FPS ({:.1} ms)", base, fps, avg));
                }
            }
        }
    }

    // EventLoop muze byt na non-main thread pres any_thread (Windows specifika).
    // main.rs spawnne worker thread s 256 MB stack pro robustnost.
    #[cfg(target_os = "windows")]
    let event_loop = {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        winit::event_loop::EventLoop::builder().with_any_thread(true).build().map_err(|e| e.to_string())?
    };
    #[cfg(not(target_os = "windows"))]
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    // Title z <title> tagu nebo URL fallback.
    // Title nyni drzí webview (App polarity invert step).
    let mut app = App {
        initial: Some((html, css, base_url, current_html_path)),
        async_jobs: crate::browser::async_jobs::AsyncJobsRegistry::new(),
        frame_times_ms: std::collections::VecDeque::with_capacity(60),
        // FPS overlay default off - Ctrl+Shift+F toggle. Drive default zapnuty
        // pri PERF_DEBUG=1, ale env var ma byt jen pro logging - overlay je
        // separate UI feature.
        show_fps: false,
        last_render_instant: None,
        frames_since_title: 0,
        pending_hover: None,
        open_select: None,
        auto_devtools,
        window: None,
        renderer: None,
        mouse_x: 0.0,
        mouse_y: 0.0,
        pending_resize: None,
        devtools: crate::devtools::DevToolsState::default(),
        devtools_resizing: false,
        last_click_time: None,
        last_click_pos: (0.0, 0.0),
        shared_debugger: std::sync::Arc::new(std::sync::Mutex::new(
            crate::interpreter::DebuggerState::default())),
        continue_signal: std::sync::Arc::new((
            std::sync::Mutex::new(false), std::sync::Condvar::new())),
        debug_runner: None,
        modifiers: winit::keyboard::ModifiersState::empty(),
        page_scrollbar_v_drag: false,
        page_scrollbar_h_drag: false,
        // WebView mirror inicializovan v `resumed` (znama viewport size z winit
        // + chrome offset zname). Pred resumed App nema window -> None.
        webview: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    Ok(())
}


/// Otevri soubor (HTML/PDF/...) v default OS browseru/aplikaci.
fn open_in_default_browser(path: &std::path::Path) {
    let p = path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", &p]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&p).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
    }
}


// ─── Renderer ───────────────────────────────────────────────────────────

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Browser zoom factor. Vertex px coordinates jsou v logickem px (viewport
    /// width / zoom). Uniform vp je nastaven na (config.w / zoom, config.h /
    /// zoom) tak aby NDC mapping render-koval zoom*logical px na physical px.
    pub zoom: f32,
    /// HiDPI scale_factor z winit. config.width je v physical px = logical *
    /// scale_factor. CSS coords jsou logical -> NDC mapping musi pouzit logical
    /// vp = config.width / scale_factor.
    pub scale_factor: f32,
    /// Override target size (physical px) pro `vp` uniform pri render_via.
    /// None = pouzit config.width/height (default - present do swap chain).
    /// Some((w, h)) = render do RT s touto velikosti (WebView offscreen RT).
    /// Bez override by NDC pocital pres full surface, ale RT je smaller,
    /// vede ke kompresi obsahu.
    pub target_size: Option<(u32, u32)>,
    /// Page canvas background (body/html bg) - clear barva pro first compose
    /// pass v D4 layer mode. None = default svetle seda 0.95 (UA default).
    /// Bez tohoto prosvita bily clear za layout rootem (scrollbar gutter,
    /// plocha pod kratkym contentem) na strankach s tmavym bg.
    pub page_clear_color: Option<[f32; 4]>,
    pipeline: wgpu::RenderPipeline,
    /// Optional LCD pipeline pro real subpixel text - vyzaduje DUAL_SOURCE_BLENDING.
    /// None pri unsupported HW (fallback grayscale v hlavnim shaderu mode 9).
    lcd_pipeline: Option<wgpu::RenderPipeline>,
    uniform_buf: wgpu::Buffer,
    atlas_tex: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    atlas_smp: wgpu::Sampler,
    /// Nearest sampler pro layer/tile compose paths. Bilinear (atlas_smp) na 1:1
    /// physical pixel mapping mezi layer tex a target view ZPUSOBUJE blur na
    /// sub-pixel float NDC boundaries. Nearest sampling = exact texel = sharp.
    /// Atlas glyph stale samplovan pres atlas_smp (Linear pres edge AA).
    compose_smp: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    atlas: GlyphAtlas,
    /// Image RGBA atlas + GPU texture
    image_atlas: ImageAtlas,
    /// Cached source bytes per URL pro re-resample pri zoomu (load_image_as
    /// stores, resample_image_for_size cte).
    image_source_bytes: std::collections::HashMap<String, Vec<u8>>,
    /// Tombstone set: src URL klice ktere selhaly pri fetchu nebo decode.
    /// load_image_as skip dalsi pokusy aby kazdy frame sync ureq + decode
    /// nezdrzoval. Resi 1 s lag per frame na strankach s SVG/unknown img.
    image_load_failed: std::collections::HashSet<String>,
    /// Cache hashes Text commands (content + font_size + font_family + bold/italic)
    /// uz prosly atlas warm-up. Per-Text fast-path skip cele char iterace pri
    /// znovu-pouziti stejneho textu. Resi 20 ms warm-up loop na strankach
    /// kde display list je stable mezi framy.
    text_cmd_warmed: std::collections::HashSet<u64>,
    /// Vertex buffer cache pres DisplayCommand slice hash + atlas/image gen
    /// counter + zoom. Pri stable display_list (typicky hover bez visual change)
    /// reuse drive vyrobeny `Vec<Vertex>` misto rebuild (35ms saved per frame).
    /// LRU 8 entries (multiple segments per frame: page WV, devtools, chrome).
    vert_cache: std::collections::VecDeque<(u64, Vec<Vertex>)>,
    /// Atlas generation counter - bump pri glyph add/upload. Vertex cache klic
    /// zahrnuje aktualni gen, takze stary cache entry expiruje po novem glyphu.
    pub atlas_gen: u64,
    /// Image atlas generation counter - rovnez.
    pub image_atlas_gen: u64,
    image_tex: wgpu::Texture,
    image_view: wgpu::TextureView,
    /// @font-face loaded fonts: family -> Font.
    font_registry: std::collections::HashMap<String, SwashFontFace>,
    /// Loaded font URLs (skip re-load).
    loaded_font_urls: std::collections::HashSet<String>,
    /// Color fonts: family -> ColrData (layers + palette pro emoji rasterization).
    color_fonts: std::collections::HashMap<String, super::emoji_fonts::ColrData>,
    /// Offscreen RT pro filter blur / view-transitions (RGBA8 viewport size).
    offscreen_tex: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    /// Druhy RT pro blur 2-pass (ping-pong)
    offscreen_tex_b: wgpu::Texture,
    offscreen_view_b: wgpu::TextureView,
    /// Hlavni RT - vse kreslime sem misto prima na swap chain. Backdrop-filter
    /// snapshotuje obsah tohoto RT. Na konci framu composit main_rt -> swap chain.
    /// Usage: TEXTURE_BINDING | RENDER_ATTACHMENT | COPY_SRC
    main_rt: wgpu::Texture,
    /// Shell RT - browser chrome (tabs / address bar / scrollbars / find overlay).
    /// Dvouvrstvy render: page content -> main_rt, shell chrome -> shell_rt,
    /// pak compose obojiho do swap chainu (shell pres page s alpha blend).
    /// Tohle umozni page cache/incremental + shell vzdy responsive (60fps),
    /// nezavisle na page paint cost.
    shell_rt: wgpu::Texture,
    /// Blur pipeline + bind group layout (separate od main)
    blur_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform pro blur direction (0=horizontal, 1=vertical) + radius
    blur_uniform_buf: wgpu::Buffer,
    /// Compose pipeline - samples offscreen_tex a kresli do swap chain.
    /// Pouziva fullscreen triangle + scissor pro region + color matrix uniform.
    compose_pipeline: wgpu::RenderPipeline,
    /// CSS mix-blend-mode pipelines (wgpu BlendState varianty).
    /// Pres compose_view_to_view_blend(mode_id). Index mapuje na
    /// computed_style::BlendMode discriminant.
    compose_pipeline_multiply: wgpu::RenderPipeline,
    compose_pipeline_screen: wgpu::RenderPipeline,
    compose_pipeline_darken: wgpu::RenderPipeline,
    compose_pipeline_lighten: wgpu::RenderPipeline,
    compose_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform pro compose color matrix (5x vec4 = 80 bytes)
    compose_uniform_buf: wgpu::Buffer,
    /// CSS mix-blend-mode advanced shader: per-pixel dst sample (src+dst snapshot).
    /// Resi Overlay/ColorDodge/SoftLight/Difference/Hue/Sat/Color/Lum - vse co
    /// fixed-function BlendState neumi. Implementuje vsech 16 modes per spec.
    advanced_blend_pipeline: wgpu::RenderPipeline,
    advanced_blend_bgl: wgpu::BindGroupLayout,
    /// Uniform: blend_mode u32 + opacity f32 + 2x pad (16 bytes).
    advanced_blend_uniform: wgpu::Buffer,
    /// CSS mix-blend-mode LAYER compose: blend layer tex pres backdrop snapshot
    /// pri kompozici (D4 compositor). src = layer tex (dst_box positioned),
    /// dst = target snapshot. Resi vsechny modes vc. non-separable HSL.
    blend_compose_pipeline: wgpu::RenderPipeline,
    blend_compose_bgl: wgpu::BindGroupLayout,
    /// WebGL phase 3c1: cache zkompilovanych shader modules per program ID.
    /// Klic = WebGLProgram id (z linkProgram).
    webgl_shader_modules: std::collections::HashMap<u32, (wgpu::ShaderModule, wgpu::ShaderModule)>,
    /// WebGL pipeline cache per program ID. Build pri prvnim Draw* commandu.
    webgl_pipelines: std::collections::HashMap<u32, wgpu::RenderPipeline>,
    /// Uploadovane vertex/index buffers per WebGLBuffer ID.
    webgl_buffers: std::collections::HashMap<u32, wgpu::Buffer>,
    /// Per-canvas offscreen RT (canvas_ptr -> Texture + View).
    webgl_canvas_rts: std::collections::HashMap<usize, (wgpu::Texture, wgpu::TextureView, u32, u32)>,
    /// Per-program uniform buffer cache (program_id -> Buffer).
    webgl_uniform_buffers: std::collections::HashMap<u32, wgpu::Buffer>,
    /// Per-program uniform bind group layout cache.
    webgl_uniform_bgls: std::collections::HashMap<u32, wgpu::BindGroupLayout>,
    /// Per-WebGLTexture id -> wgpu::Texture + View.
    webgl_textures: std::collections::HashMap<u32, (wgpu::Texture, wgpu::TextureView)>,
    /// Default sampler pro WebGL texture binding (linear filter, repeat wrap).
    webgl_default_sampler: Option<wgpu::Sampler>,
    /// 3D transform compose pipeline (samples offscreen RT, kresli quad transformovany matici)
    transform_pipeline: wgpu::RenderPipeline,
    transform_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform pro transform matrix + center + viewport + uv_box (8x vec4 = 128 bytes)
    transform_uniform_buf: wgpu::Buffer,
}

impl Renderer {
    pub fn new(window: std::sync::Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            }
        )).expect("adapter");
        // Pokus se requestnout DUAL_SOURCE_BLENDING pro real LCD subpixel text.
        // Default ON - shader ma `enable dual_source_blending` directive, vetsina
        // HW + driver podporuji. Opt-out pres RUST_WEB_ENGINE_LCD=0 (grayscale
        // fallback) pro debug nebo problematicky driver. build_lcd_pipeline ma
        // catch_unwind = pri compile fail tichy fallback bez crash.
        let lcd_opt_out = std::env::var("RUST_WEB_ENGINE_LCD")
            .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
            .unwrap_or(false);
        let supports_dual_source = !lcd_opt_out
            && adapter.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING);
        let (device, queue) = if supports_dual_source {
            pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("rwe-device"),
                    required_features: wgpu::Features::DUAL_SOURCE_BLENDING,
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                },
            )).unwrap_or_else(|_| {
                eprintln!("[render] DUAL_SOURCE_BLENDING request selhal, fallback");
                pollster::block_on(adapter.request_device(
                    &wgpu::DeviceDescriptor::default(),
                )).expect("device")
            })
        } else {
            eprintln!("[render] adapter nema DUAL_SOURCE_BLENDING - LCD subpixel grayscale fallback");
            pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor::default(),
            )).expect("device")
        };
        let dual_source_blend = device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING);
        crate::vlog!("[render] dual_source_blending: {}", dual_source_blend);
        let size = window.inner_size();
        let scale_factor = window.scale_factor();
        let surface_caps = surface.get_capabilities(&adapter);
        crate::vlog!("[render] window inner_size = {}x{} physical, scale_factor = {} (logical = {}x{})",
            size.width, size.height, scale_factor,
            (size.width as f64 / scale_factor) as u32,
            (size.height as f64 / scale_factor) as u32);
        // Pokud winit jeste nedostal WM_SIZE na Windows, inner_size = 0. Fallback na rozumny default.
        let init_w = if size.width > 0 { size.width } else { 1280 };
        let init_h = if size.height > 0 { size.height } else { 900 };
        // Present mode: Fifo (klasicky vsync). present()/get_current_texture()
        // BLOKUJE thread do dalsiho monitor refreshe = prirozeny frame cap na
        // refresh rate (60/144Hz) + NIZKE CPU (thread spi v GPU driveru misto
        // busy-spin). Driv preferovany Mailbox/Immediate je non-blocking -> s
        // kontinualnim animacnim redraw se rendrovalo na max rychlost = 100% CPU
        // ("vykon nestal za moc" i v release buildu). Fifo je vzdy podporovany
        // (wgpu spec guarantee) takze fallback netreba.
        let present_mode = wgpu::PresentMode::Fifo;
        crate::vlog!("[render] present_mode = {:?} (available: {:?})", present_mode, surface_caps.present_modes);
        // GAMMA-SPACE compositing: vyber NON-sRGB (Unorm) surface format aby HW
        // alpha blending probihal v gamma/sRGB prostoru = STEJNE jako Chrome
        // (CSS compositing neni linear). Driv formats[0] = typicky sRGB -> blend
        // v linear -> semi-transparent rgba nesedely (rgba 0.5 pres tmavou
        // davalo R=190 misto Chrome 148). Fallback na [0] kdyz non-sRGB neni.
        let surface_format = surface_caps.formats.iter().copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        crate::vlog!("[render] surface_format = {:?} (sRGB={})", surface_format, surface_format.is_srgb());
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: init_w,
            height: init_h,
            present_mode,
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
        // Nearest sampler: glyph atlas + image atlas pres tento sampler.
        // Sampler pouzity pro glyph atlas + image atlas + offscreen RT.
        // Linear filter pro smooth upscale (images, RT compose). Pro text
        // glyfy rasterujeme na physical_size = font_size * zoom takze atlas
        // px = screen px (1:1 mapping) a Linear vs Nearest neda blur.
        // Atlas sampler - Linear by glyph stored at zoom-aware physical size
        // sample at exact texel center = sharp pres zoom. Pri non-aligned UV
        // (sub-pixel offset) bilinear interpolates = soft. Atlas rasterizes
        // glyph pres font_size * zoom = matching display size = 1:1 = sharp.
        let atlas_smp = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let compose_smp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("compose_nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Image RGBA atlas texture
        let image_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image_atlas"),
            size: wgpu::Extent3d {
                width: IMAGE_ATLAS_SIZE, height: IMAGE_ATLAS_SIZE, depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Unorm (ne Srgb): GAMMA-space rendering - sample vraci sRGB-encoded
            // hodnoty konzistentni s normalize_color (zadna linear konverze).
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let image_view = image_tex.create_view(&Default::default());

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
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&image_view) },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some("pl"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // pos
                        1 => Float32x4, // color
                        2 => Float32x2, // uv
                        3 => Float32,   // mode
                        4 => Float32x2, // local
                        5 => Float32x2, // half_size
                        6 => Float32,   // radius
                        7 => Float32x4, // color2
                        8 => Float32,   // blur
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
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
            cache: None,
        });

        // Real LCD subpixel pipeline - dual-source blend pri HW support.
        // Separate shader module (LCD_SHADER) s `enable dual_source_blending`
        // directive - kompiluje se jen kdyz device feature aktivni. Pri non-
        // support zustava None (mode 9 vertices se vykresli s main pipeline pres
        // gamma-correct grayscale fallback v main fs_main).
        // LCD pipeline build via helper s error scope guard.
        let lcd_pipeline = if dual_source_blend {
            build_lcd_pipeline(&device, &pipeline_layout, config.format)
        } else { None };

        let atlas = GlyphAtlas::new();

        let image_atlas = ImageAtlas::new();

        // Offscreen RT_a + RT_b - viewport size, format = swap chain format
        // (aby main pipeline mohl renderovat do RT a compose pipeline samplovat).
        let offscreen_format = config.format;
        let make_rt = |dev: &wgpu::Device, label: &str, w: u32, h: u32| {
            let tex = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: offscreen_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            (tex, view)
        };
        let (offscreen_tex,   offscreen_view)   = make_rt(&device, "offscreen_rt_a", config.width, config.height);
        let (offscreen_tex_b, offscreen_view_b) = make_rt(&device, "offscreen_rt_b", config.width, config.height);
        // Main RT - COPY_SRC misto COPY_DST (backdrop-filter snapshots z tohoto RT)
        let main_rt = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("main_rt"),
            size: wgpu::Extent3d { width: config.width.max(1), height: config.height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: offscreen_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // Shell RT - browser chrome only.
        let shell_rt = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shell_rt"),
            size: wgpu::Extent3d { width: config.width.max(1), height: config.height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: offscreen_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        // Blur shader + pipeline
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_shader"),
            source: wgpu::ShaderSource::Wgsl(BLUR_SHADER.into()),
        });
        let blur_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur_uniform"),
            size: 16, // direction.xy + radius + texel
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some("blur_pl"),
            bind_group_layouts: &[Some(&blur_bind_group_layout)],
            
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some("blur_pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader, entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[], // fullscreen triangle, no vertex buffer
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader, entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: offscreen_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        // Compose shader + pipeline - samples offscreen RT, kresli do swap chain
        let compose_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compose_shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSE_SHADER.into()),
        });
        let compose_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compose_uniform"),
            size: 112, // 7x vec4: 5 matrix + dst_box + src_uv
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let compose_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compose_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let compose_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some("compose_pl"),
            bind_group_layouts: &[Some(&compose_bind_group_layout)],
            
        });
        // Helper - build compose pipeline s daným BlendState.
        let make_compose_pipeline = |label: &'static str, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
                label: Some(label),
                layout: Some(&compose_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &compose_shader, entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &compose_shader, entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                cache: None,
            })
        };
        // Compose pipeline blend = PREMUL OVER (src_factor=One, ne SrcAlpha).
        // Layer/tile texture pri raster pres ALPHA_BLENDING ulozi PREMUL data
        // (src.rgb * src.a stored po blend). Compose pak NESMI ALPHA_BLENDING
        // ktery by zase nasobil src.rgb * src.a = double-multiply = desaturace
        // a tmavnuti barev (uzivatel videl "stupne sedi").
        // Premul OVER: out = src.rgb + dst.rgb * (1 - src.a). Spravne pro src
        // stored as premultiplied.
        let compose_pipeline = make_compose_pipeline("compose_pipeline_normal",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            });
        // CSS mix-blend-mode pipelines pres wgpu BlendState factors:
        // - Multiply: src*dst.  src_factor=Dst, dst_factor=Zero, op=Add.
        // - Screen: 1-(1-s)(1-d) = s + d - s*d.  src_factor=One, dst_factor=OneMinusSrc, op=Add.
        // - Darken: min(s, d).  BlendOperation::Min.
        // - Lighten: max(s, d).  BlendOperation::Max.
        // Vsechny ostatni modes (Overlay/ColorDodge/SoftLight/Difference/Hue/Sat/Color/Lum)
        // vyzaduji shader-side dst sample = TODO (potreba copy fb -> snapshot tex).
        let compose_pipeline_multiply = make_compose_pipeline("compose_pipeline_multiply",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            });
        let compose_pipeline_screen = make_compose_pipeline("compose_pipeline_screen",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrc,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            });
        let compose_pipeline_darken = make_compose_pipeline("compose_pipeline_darken",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Min,
                },
                alpha: wgpu::BlendComponent::OVER,
            });
        let compose_pipeline_lighten = make_compose_pipeline("compose_pipeline_lighten",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Max,
                },
                alpha: wgpu::BlendComponent::OVER,
            });

        // Advanced blend pipeline - per-pixel dst sample (src_tex + dst_tex snapshot).
        // Resi vsechny separable + non-separable modes ktere fixed-function neumi.
        let advanced_blend_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("advanced_blend_shader"),
            source: wgpu::ShaderSource::Wgsl(ADVANCED_BLEND_SHADER.into()),
        });
        let advanced_blend_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("advanced_blend_uniform"),
            size: 16, // blend_mode u32 + opacity f32 + 2x pad
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let advanced_blend_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("advanced_blend_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                    }, count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                    }, count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None,
                    }, count: None,
                },
            ],
        });
        let advanced_blend_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some("advanced_blend_pl"),
            bind_group_layouts: &[Some(&advanced_blend_bgl)],
        });
        let advanced_blend_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some("advanced_blend_pipeline"),
            layout: Some(&advanced_blend_pl),
            vertex: wgpu::VertexState {
                module: &advanced_blend_shader, entry_point: Some("vs_main"),
                compilation_options: Default::default(), buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &advanced_blend_shader, entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // Shader sam dela Porter-Duff over (out = sa*blended + (1-sa)*dst)
                    // -> REPLACE, zadny dalsi HW blend.
                    format: offscreen_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        // Blend-compose pipeline (mix-blend-mode na urovni layer compose).
        let blend_compose_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blend_compose_shader"),
            source: wgpu::ShaderSource::Wgsl(BLEND_COMPOSE_SHADER.into()),
        });
        let blend_compose_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blend_compose_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let blend_compose_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some("blend_compose_pl"),
            bind_group_layouts: &[Some(&blend_compose_bgl)],
        });
        let blend_compose_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some("blend_compose_pipeline"),
            layout: Some(&blend_compose_pl),
            vertex: wgpu::VertexState { module: &blend_compose_shader, entry_point: Some("vs_main"),
                compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &blend_compose_shader, entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // Shader rekonstruuje premul vystup (blend + porter-duff over) -> REPLACE.
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        // Transform pipeline - samples offscreen, drawat 3D transformed quad
        let transform_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("transform_shader"),
            source: wgpu::ShaderSource::Wgsl(TRANSFORM_SHADER.into()),
        });
        let transform_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transform_uniform"),
            size: 128, // 8x vec4 (mat 4x4 + center + viewport + uv_box)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let transform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("transform_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // VERTEX i FRAGMENT: fs_main cte tp.uv_box pro edge AA
                    // feather (zubate hrany rotovanych layeru; docx r.23).
                    binding: 2, visibility: wgpu::ShaderStages::VERTEX
                        .union(wgpu::ShaderStages::FRAGMENT),
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let transform_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some("transform_pl"),
            bind_group_layouts: &[Some(&transform_bind_group_layout)],
            
        });
        let transform_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some("transform_pipeline"),
            layout: Some(&transform_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &transform_shader, entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &transform_shader, entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    // Premul OVER (src=One, ne SrcAlpha) - layer/offscreen RT
                    // stored premul, compose nesmi znovu multiplikovat src.a =
                    // double-alpha = transform content darkens to invisible.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        Renderer {
            surface, device, queue, config, zoom: 1.0,
            scale_factor: scale_factor as f32,
            target_size: None,
            page_clear_color: None,
            pipeline, lcd_pipeline, uniform_buf,
            atlas_tex, atlas_view, atlas_smp, compose_smp, bind_group_layout, bind_group, atlas,
            image_atlas, image_tex, image_view,
            image_source_bytes: std::collections::HashMap::new(),
            image_load_failed: std::collections::HashSet::new(),
            text_cmd_warmed: std::collections::HashSet::new(),
            vert_cache: std::collections::VecDeque::with_capacity(8),
            atlas_gen: 0,
            image_atlas_gen: 0,
            font_registry: std::collections::HashMap::new(),
            loaded_font_urls: std::collections::HashSet::new(),
            color_fonts: std::collections::HashMap::new(),
            offscreen_tex, offscreen_view,
            offscreen_tex_b, offscreen_view_b,
            main_rt,
            shell_rt,
            blur_pipeline, blur_bind_group_layout, blur_uniform_buf,
            compose_pipeline,
            compose_pipeline_multiply,
            compose_pipeline_screen,
            compose_pipeline_darken,
            compose_pipeline_lighten,
            compose_bind_group_layout, compose_uniform_buf,
            advanced_blend_pipeline, advanced_blend_bgl, advanced_blend_uniform,
            blend_compose_pipeline, blend_compose_bgl,
            transform_pipeline, transform_bind_group_layout, transform_uniform_buf,
            webgl_shader_modules: std::collections::HashMap::new(),
            webgl_pipelines: std::collections::HashMap::new(),
            webgl_buffers: std::collections::HashMap::new(),
            webgl_canvas_rts: std::collections::HashMap::new(),
            webgl_uniform_buffers: std::collections::HashMap::new(),
            webgl_uniform_bgls: std::collections::HashMap::new(),
            webgl_textures: std::collections::HashMap::new(),
            webgl_default_sampler: None,
        }
    }

    /// Upload WebGL texture data do GPU. RGBA bytes (rozmer = w*h*4).
    /// Format: GL_RGBA (0x1908) -> Rgba8Unorm (gamma-space, konzistentni s RT).
    /// Idempotent reupload.
    pub fn upload_webgl_texture(&mut self, texture_id: u32, w: u32, h: u32, format: u32, data: &[u8]) -> bool {
        if w == 0 || h == 0 || data.is_empty() { return false; }
        // Format mapping. GL_RGBA = 0x1908, GL_RGB = 0x1907.
        // Pro Rgb -> dopadovat na Rgba (GPU nepodporuje 24-bit usually).
        // Unorm (ne Srgb): gamma-space rendering - sample vraci sRGB-encoded.
        let wgpu_format = wgpu::TextureFormat::Rgba8Unorm;
        let bytes_per_pixel = 4u32;
        // Pri RGB (3 bytes/pixel) konvertujem na RGBA s alpha=255.
        let rgba_data: Vec<u8> = match format {
            0x1907 => {
                // RGB -> RGBA
                let mut out = Vec::with_capacity((w * h * 4) as usize);
                for chunk in data.chunks_exact(3) {
                    out.extend_from_slice(chunk);
                    out.push(255);
                }
                out
            }
            _ => data.to_vec(),
        };
        let expected_size = (w * h * bytes_per_pixel) as usize;
        if rgba_data.len() < expected_size { return false; }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("webgl_tex_{texture_id}")),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex, mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba_data[..expected_size],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * bytes_per_pixel),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&Default::default());
        self.webgl_textures.insert(texture_id, (tex, view));
        if self.webgl_default_sampler.is_none() {
            self.webgl_default_sampler = Some(self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("webgl_default_sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            }));
        }
        true
    }

    pub fn webgl_texture_count(&self) -> usize {
        self.webgl_textures.len()
    }
    pub fn webgl_has_texture(&self, texture_id: u32) -> bool {
        self.webgl_textures.contains_key(&texture_id)
    }

    /// Ensure full bind group layout - uniform buffer + texture entries + sampler entries.
    /// Vse na groupe 0 dle naga binding indexu. Idempotent (cache).
    pub fn ensure_webgl_full_resources(
        &mut self,
        program_id: u32,
        uniform_buffer_size: u64,
        uniform_binding: Option<u32>,
        texture_bindings: &[(String, u32)],
        sampler_bindings: &[(String, u32)],
    ) {
        // Nejdriv uniform buffer
        if uniform_buffer_size > 0 && !self.webgl_uniform_buffers.contains_key(&program_id) {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("webgl_uniform_buf_{program_id}")),
                size: uniform_buffer_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.webgl_uniform_buffers.insert(program_id, buf);
        }
        // Pak BGL - jen pokud aspon 1 binding existuje
        let has_uniform = uniform_buffer_size > 0;
        let has_resources = has_uniform || !texture_bindings.is_empty() || !sampler_bindings.is_empty();
        if !has_resources { return; }
        if self.webgl_uniform_bgls.contains_key(&program_id) { return; }
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
        if has_uniform {
            let binding = uniform_binding.unwrap_or(0);
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false, min_binding_size: None,
                },
                count: None,
            });
        }
        for (_, b) in texture_bindings {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: *b,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        for (_, b) in sampler_bindings {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: *b,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("webgl_full_bgl_{program_id}")),
            entries: &entries,
        });
        self.webgl_uniform_bgls.insert(program_id, bgl);
        // Lazy default sampler
        if !sampler_bindings.is_empty() && self.webgl_default_sampler.is_none() {
            self.webgl_default_sampler = Some(self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("webgl_default_sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            }));
        }
    }

    pub fn webgl_has_uniform_buffer(&self, program_id: u32) -> bool {
        self.webgl_uniform_buffers.contains_key(&program_id)
    }
    pub fn webgl_uniform_buffer_count(&self) -> usize {
        self.webgl_uniform_buffers.len()
    }

    /// Ensure per-canvas offscreen RT vznikne (alloc pri prvni navsteve nebo
    /// resize). Vraci view.
    pub fn ensure_webgl_canvas_rt(&mut self, canvas_ptr: usize, w: u32, h: u32) {
        let need_create = match self.webgl_canvas_rts.get(&canvas_ptr) {
            Some((_, _, cw, ch)) => *cw != w || *ch != h,
            None => true,
        };
        if need_create {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("webgl_canvas_rt_{canvas_ptr}")),
                size: wgpu::Extent3d {
                    width: w.max(1), height: h.max(1), depth_or_array_layers: 1,
                },
                mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            self.webgl_canvas_rts.insert(canvas_ptr, (tex, view, w, h));
        }
    }

    pub fn webgl_canvas_rt_count(&self) -> usize {
        self.webgl_canvas_rts.len()
    }
    pub fn webgl_has_canvas_rt(&self, canvas_ptr: usize) -> bool {
        self.webgl_canvas_rts.contains_key(&canvas_ptr)
    }

    /// Build wgpu pipeline z cached shader modules + vertex layout dle attribs.
    /// Cached per program_id. Vraci true pokud build success (nebo cache hit).
    /// Pokud uniform_bgl exists pro program (po ensure_webgl_full_resources),
    /// pridava se k pipeline layout.
    pub fn ensure_webgl_pipeline(&mut self, program_id: u32, attribs: &[(u32, crate::interpreter::WebGLAttribSlot)]) -> bool {
        if self.webgl_pipelines.contains_key(&program_id) {
            return true;
        }
        let modules = match self.webgl_shader_modules.get(&program_id) {
            Some(m) => m,
            None => return false,  // shader modules nutno predtim
        };
        // Vertex layout: jeden vertex buffer s vsemi attribs.
        let stride = webgl_compute_stride(attribs);
        let attrs: Vec<wgpu::VertexAttribute> = attribs.iter().filter_map(|(loc, slot)| {
            webgl_attrib_to_vertex_format(slot.size, slot.component_type).map(|fmt| {
                wgpu::VertexAttribute {
                    format: fmt,
                    offset: slot.offset as u64,
                    shader_location: *loc,
                }
            })
        }).collect();
        let buffers: Vec<wgpu::VertexBufferLayout> = if attrs.is_empty() {
            Vec::new()
        } else {
            vec![wgpu::VertexBufferLayout {
                array_stride: stride.max(4),
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attrs,
            }]
        };
        // Pripoj uniform bind group layout pokud existuje pro program.
        let bgl_refs: Vec<Option<&wgpu::BindGroupLayout>> = if let Some(bgl) = self.webgl_uniform_bgls.get(&program_id) {
            vec![Some(bgl)]
        } else {
            Vec::new()
        };
        let pl_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { immediate_size: 0,
            label: Some(&format!("webgl_pl_{program_id}")),
            bind_group_layouts: &bgl_refs,
            
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some(&format!("webgl_pipeline_{program_id}")),
            layout: Some(&pl_layout),
            vertex: wgpu::VertexState {
                module: &modules.0, entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &modules.1, entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });
        self.webgl_pipelines.insert(program_id, pipeline);
        true
    }

    /// Build bind group entries dle program bindings + texture units.
    /// Vraci None pokud zadne resources nejsou potreba.
    fn build_webgl_bind_group(
        &self,
        program_id: u32,
        uniform_bytes: &[u8],
        uniform_binding: Option<u32>,
        texture_bindings: &[(String, u32)],
        sampler_bindings: &[(String, u32)],
        texture_units: &std::collections::HashMap<u32, u32>,
    ) -> Option<wgpu::BindGroup> {
        let bgl = self.webgl_uniform_bgls.get(&program_id)?;
        let has_uniform = !uniform_bytes.is_empty() && self.webgl_uniform_buffers.contains_key(&program_id);
        let has_resources = has_uniform || !texture_bindings.is_empty() || !sampler_bindings.is_empty();
        if !has_resources { return None; }
        if has_uniform {
            if let Some(buf) = self.webgl_uniform_buffers.get(&program_id) {
                self.queue.write_buffer(buf, 0, uniform_bytes);
            }
        }
        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
        if has_uniform {
            let binding = uniform_binding.unwrap_or(0);
            if let Some(buf) = self.webgl_uniform_buffers.get(&program_id) {
                entries.push(wgpu::BindGroupEntry {
                    binding,
                    resource: buf.as_entire_binding(),
                });
            }
        }
        // Texture entries: pri texture_bindings[i], pouzij texture_units[i] -> texture
        // (default = unit 0 pokud unit chybi)
        for (i, (_, b)) in texture_bindings.iter().enumerate() {
            let unit = i as u32;
            let tex_id = texture_units.get(&unit).copied()
                .or_else(|| texture_units.values().next().copied());
            if let Some(tid) = tex_id {
                if let Some((_, view)) = self.webgl_textures.get(&tid) {
                    entries.push(wgpu::BindGroupEntry {
                        binding: *b,
                        resource: wgpu::BindingResource::TextureView(view),
                    });
                }
            }
        }
        // Sampler entries: vsechny default sampler
        if !sampler_bindings.is_empty() {
            if let Some(samp) = &self.webgl_default_sampler {
                for (_, b) in sampler_bindings {
                    entries.push(wgpu::BindGroupEntry {
                        binding: *b,
                        resource: wgpu::BindingResource::Sampler(samp),
                    });
                }
            }
        }
        if entries.is_empty() { return None; }
        Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("webgl_bg_{program_id}")),
            layout: bgl,
            entries: &entries,
        }))
    }

    /// Encode wgpu draw call do canvas RT.
    /// Pipeline + buffer musi byt cached. Vraci true pokud emit success.
    /// Pokud bindings neprazdne, build full bind group s uniform+textures+samplers.
    #[allow(clippy::too_many_arguments)]
    pub fn webgl_encode_draw_arrays(
        &self,
        canvas_ptr: usize,
        program_id: u32,
        first: i32,
        count: i32,
        attribs: &[(u32, crate::interpreter::WebGLAttribSlot)],
        clear_color: Option<[f32; 4]>,
        uniform_bytes: &[u8],
        uniform_binding: Option<u32>,
        texture_bindings: &[(String, u32)],
        sampler_bindings: &[(String, u32)],
        texture_units: &std::collections::HashMap<u32, u32>,
    ) -> bool {
        let view = match self.webgl_canvas_rts.get(&canvas_ptr) {
            Some((_, v, _, _)) => v,
            None => return false,
        };
        let pipeline = match self.webgl_pipelines.get(&program_id) {
            Some(p) => p,
            None => return false,
        };
        let bind_group = self.build_webgl_bind_group(
            program_id, uniform_bytes, uniform_binding,
            texture_bindings, sampler_bindings, texture_units,
        );
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let load = match clear_color {
            Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64,
            }),
            None => wgpu::LoadOp::Load,
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("webgl_draw_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            if let Some(bg) = &bind_group {
                pass.set_bind_group(0, bg, &[]);
            }
            if let Some((_, slot)) = attribs.first() {
                if let Some(buf) = self.webgl_buffers.get(&slot.buffer_id) {
                    pass.set_vertex_buffer(0, buf.slice(..));
                }
            }
            if count > 0 {
                pass.draw(first as u32..(first + count) as u32, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    /// Walk layout tree + pro kazdy canvas s WebGL state, drain queue
    /// a encode real wgpu draw passes do per-canvas RT, composit RT do
    /// swap chain. Vraci true pokud aspon 1 canvas drawnut.
    pub fn run_webgl_frame(
        &mut self,
        root: &super::layout::LayoutBox,
        swap_view: &wgpu::TextureView,
        webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
        scroll_y: f32,
    ) -> bool {
        let mut any = false;
        self.walk_webgl(root, swap_view, webgl_states, &mut any, scroll_y);
        any
    }

    fn walk_webgl(
        &mut self,
        bx: &super::layout::LayoutBox,
        swap_view: &wgpu::TextureView,
        webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
        any: &mut bool,
        scroll_y: f32,
    ) {
        if bx.tag.as_deref() == Some("canvas") {
            if let Some(node) = &bx.node {
                let ptr = std::rc::Rc::as_ptr(node) as usize;
                if let Some(state_rc) = webgl_states.get(&ptr) {
                    if self.execute_webgl_canvas(ptr, state_rc, bx, swap_view, scroll_y) {
                        *any = true;
                    }
                }
            }
        }
        for ch in &bx.children {
            self.walk_webgl(ch, swap_view, webgl_states, any, scroll_y);
        }
    }

    fn execute_webgl_canvas(
        &mut self,
        canvas_ptr: usize,
        state_rc: &std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>,
        bx: &super::layout::LayoutBox,
        swap_view: &wgpu::TextureView,
        scroll_y: f32,
    ) -> bool {
        use crate::interpreter::WebGLDrawCmd;
        // Canvas RT at PHYSICAL scale (logical * zoom * sf) - matches compose
        // target_view scale = 1:1 pixel mapping = sharp. Drive logical only =
        // upsample pres compose = blur.
        let scale = (self.zoom * self.scale_factor).max(0.01);
        let w = ((bx.rect.width * scale) as u32).max(1);
        let h = ((bx.rect.height * scale) as u32).max(1);
        if std::env::var("RWE_WEBGL_DBG").is_ok() {
            eprintln!("[webgl] canvas bx.rect=({:.0},{:.0},{:.0}x{:.0}) scale={:.2} rt={}x{}",
                bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height, scale, w, h);
        }
        self.ensure_webgl_canvas_rt(canvas_ptr, w, h);

        // Extract data z state, pak release borrow
        type ProgInfo = (
            Option<String>, Option<String>,
            Vec<crate::interpreter::UniformSlot>, u64,
            Option<u32>, Vec<(String, u32)>, Vec<(String, u32)>,
        );
        let (cmds, buffers_data, programs_data, textures_data, texture_units_map, default_clear): (
            Vec<WebGLDrawCmd>,
            std::collections::HashMap<u32, Vec<u8>>,
            std::collections::HashMap<u32, ProgInfo>,
            std::collections::HashMap<u32, (u32, u32, u32, Vec<u8>)>,
            std::collections::HashMap<u32, u32>,
            [f32; 4],
        ) = {
            let mut state = state_rc.borrow_mut();
            let cmds: Vec<_> = state.draw_queue.drain(..).collect();
            let buffers: std::collections::HashMap<u32, Vec<u8>> = state.buffers.clone();
            let programs = state.programs.iter()
                .map(|(k, p)| (*k, (
                    p.vertex_wgsl.clone(),
                    p.fragment_wgsl.clone(),
                    p.uniform_layout.clone(),
                    p.uniform_buffer_size,
                    p.uniform_binding,
                    p.texture_bindings.clone(),
                    p.sampler_bindings.clone(),
                )))
                .collect();
            let textures = state.textures.iter()
                .map(|(k, t)| (*k, (t.width, t.height, t.format, t.data.clone())))
                .collect();
            let units = state.texture_units.clone();
            let cc = state.clear_color;
            (cmds, buffers, programs, textures, units, cc)
        };

        // Upload buffers
        for (id, data) in &buffers_data {
            if !self.webgl_buffers.contains_key(id) && !data.is_empty() {
                self.upload_webgl_buffer(*id, data);
            }
        }
        // Upload textures
        for (id, (w, h, format, data)) in &textures_data {
            if !self.webgl_textures.contains_key(id) && !data.is_empty() && *w > 0 && *h > 0 {
                self.upload_webgl_texture(*id, *w, *h, *format, data);
            }
        }
        // Build shader modules + full resources (uniform + textures + samplers)
        for (pid, (vs, fs, _layout, buffer_size, ub, tb, sb)) in &programs_data {
            if let (Some(v), Some(f)) = (vs, fs) {
                self.build_webgl_shader_modules(*pid, v, f);
            }
            self.ensure_webgl_full_resources(*pid, *buffer_size, *ub, tb, sb);
        }

        // Process commands
        let mut pending_clear: Option<[f32; 4]> = None;
        let mut had_render = false;
        for cmd in cmds {
            match cmd {
                WebGLDrawCmd::ClearColor(c) => pending_clear = Some(c),
                WebGLDrawCmd::Clear(mask) => {
                    if mask & 0x4000 != 0 {
                        let c = pending_clear.unwrap_or(default_clear);
                        self.webgl_encode_clear(canvas_ptr, c);
                        pending_clear = None;
                        had_render = true;
                    }
                }
                WebGLDrawCmd::DrawArrays { program_id, first, count, attribs, uniforms, .. } => {
                    if let Some(pid) = program_id {
                        let prog_info = programs_data.get(&pid);
                        let (bytes, ub, tb, sb): (Vec<u8>, Option<u32>, Vec<(String, u32)>, Vec<(String, u32)>) = if let Some((_, _, layout, size, ub, tb, sb)) = prog_info {
                            let bytes = if *size > 0 {
                                webgl_serialize_uniforms(layout, &uniforms, *size)
                            } else { Vec::new() };
                            (bytes, *ub, tb.clone(), sb.clone())
                        } else { (Vec::new(), None, Vec::new(), Vec::new()) };
                        if self.ensure_webgl_pipeline(pid, &attribs) {
                            let cc = pending_clear.take();
                            self.webgl_encode_draw_arrays(canvas_ptr, pid, first, count, &attribs, cc, &bytes, ub, &tb, &sb, &texture_units_map);
                            had_render = true;
                        }
                    }
                }
                WebGLDrawCmd::DrawElements { program_id, mode, count, index_type, offset, index_buffer_id, attribs, uniforms, viewport: _ } => {
                    let _ = mode;
                    if let (Some(pid), Some(ibo)) = (program_id, index_buffer_id) {
                        let prog_info = programs_data.get(&pid);
                        let (bytes, ub, tb, sb): (Vec<u8>, Option<u32>, Vec<(String, u32)>, Vec<(String, u32)>) = if let Some((_, _, layout, size, ub, tb, sb)) = prog_info {
                            let bytes = if *size > 0 {
                                webgl_serialize_uniforms(layout, &uniforms, *size)
                            } else { Vec::new() };
                            (bytes, *ub, tb.clone(), sb.clone())
                        } else { (Vec::new(), None, Vec::new(), Vec::new()) };
                        if self.ensure_webgl_pipeline(pid, &attribs) {
                            let cc = pending_clear.take();
                            self.webgl_encode_draw_elements(canvas_ptr, pid, count, index_type, offset, ibo, &attribs, cc, &bytes, ub, &tb, &sb, &texture_units_map);
                            had_render = true;
                        }
                    }
                }
            }
        }

        // Composit canvas RT region do swap chain. Pri prvnim frame had_render
        // nastavi true (process clear/draw), na dalsi frame queue prazdny ale
        // RT obsahuje predchozi clear color - musime composit kazdy frame jinak
        // canvas vypada jako prazdny po prvnim render.
        let rt_exists = self.webgl_canvas_rts.contains_key(&canvas_ptr);
        if rt_exists {
            let new_view = self.webgl_canvas_rts.get(&canvas_ptr).map(|(tex, _, _, _)| {
                tex.create_view(&Default::default())
            });
            if let Some(view) = new_view {
                // Canvas screen-space pozice = page rect.y - scroll_y. Predtim
                // composit pres bx.rect.y (page-space) coz canvas drzelo na
                // top-levem rohu okna i pri scrollu.
                let screen_y = bx.rect.y - scroll_y;
                self.compose_view_to_swap(swap_view, &view, bx.rect.x, screen_y, bx.rect.width, bx.rect.height);
            }
        }
        // Vrat true pokud aspon RT exists (animation loop tick continues).
        had_render || rt_exists
    }

    /// Encode drawElements (indexed draw) do canvas RT.
    /// Pipeline + vertex buffer + index buffer musi byt cached.
    #[allow(clippy::too_many_arguments)]
    pub fn webgl_encode_draw_elements(
        &self,
        canvas_ptr: usize,
        program_id: u32,
        count: i32,
        index_type: u32,
        offset: i32,
        index_buffer_id: u32,
        attribs: &[(u32, crate::interpreter::WebGLAttribSlot)],
        clear_color: Option<[f32; 4]>,
        uniform_bytes: &[u8],
        uniform_binding: Option<u32>,
        texture_bindings: &[(String, u32)],
        sampler_bindings: &[(String, u32)],
        texture_units: &std::collections::HashMap<u32, u32>,
    ) -> bool {
        let view = match self.webgl_canvas_rts.get(&canvas_ptr) {
            Some((_, v, _, _)) => v,
            None => return false,
        };
        let pipeline = match self.webgl_pipelines.get(&program_id) {
            Some(p) => p,
            None => return false,
        };
        let index_buf = match self.webgl_buffers.get(&index_buffer_id) {
            Some(b) => b,
            None => return false,
        };
        // Index format: GL_UNSIGNED_SHORT (0x1403) -> Uint16, GL_UNSIGNED_INT (0x1405) -> Uint32
        let idx_format = match index_type {
            0x1403 => wgpu::IndexFormat::Uint16,
            0x1405 => wgpu::IndexFormat::Uint32,
            _ => wgpu::IndexFormat::Uint16,
        };
        let idx_size_bytes: u64 = if matches!(idx_format, wgpu::IndexFormat::Uint16) { 2 } else { 4 };
        let bind_group = self.build_webgl_bind_group(
            program_id, uniform_bytes, uniform_binding,
            texture_bindings, sampler_bindings, texture_units,
        );
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let load = match clear_color {
            Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64,
            }),
            None => wgpu::LoadOp::Load,
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("webgl_draw_elements"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            if let Some(bg) = &bind_group {
                pass.set_bind_group(0, bg, &[]);
            }
            if let Some((_, slot)) = attribs.first() {
                if let Some(buf) = self.webgl_buffers.get(&slot.buffer_id) {
                    pass.set_vertex_buffer(0, buf.slice(..));
                }
            }
            let offset_bytes = offset as u64 * idx_size_bytes;
            pass.set_index_buffer(index_buf.slice(offset_bytes..), idx_format);
            if count > 0 {
                pass.draw_indexed(0..count as u32, 0, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    /// Encode jen clear color do canvas RT (bez draw call).
    pub fn webgl_encode_clear(&self, canvas_ptr: usize, color: [f32; 4]) -> bool {
        let view = match self.webgl_canvas_rts.get(&canvas_ptr) {
            Some((_, v, _, _)) => v,
            None => return false,
        };
        // RT je nyni Unorm (config.format = non-sRGB pro gamma-space compositing)
        // -> wgpu Color se uklada raw, takze WebGL sRGB clearColor predavame
        // primo bez linear konverze. Driv sRGB RT ocekaval linear (s2l).
        fn s2l(s: f32) -> f64 { s as f64 }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("webgl_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: s2l(color[0]), g: s2l(color[1]),
                            b: s2l(color[2]), a: color[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    /// WebGL phase 3c1: zkompiluje WGSL strings na ShaderModules a ulozi
    /// do cache. Vraci true pokud cache miss + uspesny build, false pokud
    /// uz cached (idempotent) nebo build failed.
    pub fn build_webgl_shader_modules(&mut self, program_id: u32, vertex_wgsl: &str, fragment_wgsl: &str) -> bool {
        if self.webgl_shader_modules.contains_key(&program_id) {
            return false;  // already cached
        }
        let v_module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("webgl_v_{}", program_id)),
            source: wgpu::ShaderSource::Wgsl(vertex_wgsl.into()),
        });
        let f_module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("webgl_f_{}", program_id)),
            source: wgpu::ShaderSource::Wgsl(fragment_wgsl.into()),
        });
        self.webgl_shader_modules.insert(program_id, (v_module, f_module));
        true
    }

    /// Diagnostic - kolik shader modules v cache.
    pub fn webgl_shader_modules_count(&self) -> usize {
        self.webgl_shader_modules.len()
    }
    /// Diagnostic - true pokud program ID je v cache.
    pub fn webgl_has_shader_modules(&self, program_id: u32) -> bool {
        self.webgl_shader_modules.contains_key(&program_id)
    }
    /// Diagnostic - kolik pipelines v cache.
    pub fn webgl_pipelines_count(&self) -> usize {
        self.webgl_pipelines.len()
    }

    /// WebGL phase 3c2: upload buffer dat do wgpu::Buffer + cache.
    /// Idempotent - update existujiciho bufferu pri opetovnem volani.
    pub fn upload_webgl_buffer(&mut self, buffer_id: u32, data: &[u8]) {
        if data.is_empty() { return; }
        // Round size na 4-byte align (WGSL min)
        let size = ((data.len() + 3) & !3) as u64;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("webgl_buf_{buffer_id}")),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, data);
        self.webgl_buffers.insert(buffer_id, buf);
    }

    pub fn webgl_buffers_count(&self) -> usize {
        self.webgl_buffers.len()
    }
    pub fn webgl_has_buffer(&self, buffer_id: u32) -> bool {
        self.webgl_buffers.contains_key(&buffer_id)
    }

    /// Provede 2-pass gauss blur na offscreen_tex_a -> tex_b -> tex_a.
    /// Volat po vykresleni do offscreen_tex_a.
    fn run_blur_passes(&mut self, radius: f32) {
        let mut encoder = self.device.create_command_encoder(&Default::default());

        // radius je v LOGICAL CSS px, ale texel = 1/config.width je PHYSICAL.
        // Offscreen RT je physical-sized + obsah mapovan pres vp (logical->NDC),
        // takze 1 logical px = zoom*scale_factor physical px. Bez prepoctu byl
        // blur zoom*scale_factor-krat uzsi (mensi rozmazani pri HiDPI/zoomu).
        let phys_scale = (self.zoom * self.scale_factor).max(0.01);
        let r = radius * phys_scale;
        // Pass 1: horizontal RT_a -> RT_b
        let texel_x = 1.0 / self.config.width as f32;
        let params_h = [1.0_f32, 0.0, r, texel_x];
        self.queue.write_buffer(&self.blur_uniform_buf, 0, bytemuck::cast_slice(&params_h));
        let bg_h = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_bg_h"), layout: &self.blur_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.blur_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.offscreen_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("blur_h"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view: &self.offscreen_view_b,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &bg_h, &[]);
            pass.draw(0..3, 0..1);
        }

        // Pass 2: vertical RT_b -> RT_a
        let texel_y = 1.0 / self.config.height as f32;
        let params_v = [0.0_f32, 1.0, r, texel_y];
        // Pouzijeme stejny buffer (write_buffer)
        let mut encoder2 = self.device.create_command_encoder(&Default::default());
        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.write_buffer(&self.blur_uniform_buf, 0, bytemuck::cast_slice(&params_v));
        let bg_v = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_bg_v"), layout: &self.blur_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.blur_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.offscreen_view_b) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
            ],
        });
        {
            let mut pass = encoder2.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("blur_v"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view: &self.offscreen_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &bg_v, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(encoder2.finish()));
    }

    /// Nacte fonty z @font-face declarations do Font registry.
    /// Podporuje HTTP/HTTPS (ureq sync), file:///, FS path. Resolve relativni
    /// URL proti page base_url. Skip uz nahrane URL.
    fn load_font_faces(&mut self, font_faces: &[crate::browser::css_parser::FontFace], base_url: Option<&str>) {
        use crate::browser::css_parser::extract_font_url;
        for ff in font_faces {
            let url = match extract_font_url(&ff.src) {
                Some(u) => u,
                None => {
                    let src_short: String = if ff.src.len() > 100 {
                        format!("{}... ({} chars)", &ff.src[..100], ff.src.len())
                    } else { ff.src.clone() };
                    eprintln!("[font-face] SKIP family={} - extract_font_url(src=...) selhal. src={}",
                        ff.family, src_short);
                    continue;
                }
            };
            if self.loaded_font_urls.contains(&url) { continue; }
            // Resolve relativni URL proti page base_url. Pokud HTTPS/HTTP -
            // ureq fetch. Jinak FS read (s static/ fallback pro relativni).
            let final_url = if url.starts_with("http://") || url.starts_with("https://") {
                url.clone()
            } else if let Some(base) = base_url {
                resolve_url(base, &url)
            } else {
                url.clone()
            };
            let bytes_opt: Option<Vec<u8>> = if final_url.starts_with("http://") || final_url.starts_with("https://") {
                // Use cached fetch - fonts cachuji se disk + RAM, reload nemusi re-fetch.
                super::render::cached_fetch_bytes(&final_url)
            } else {
                let path = if let Some(rest) = final_url.strip_prefix("file:///") {
                    rest.replace('/', std::path::MAIN_SEPARATOR_STR)
                } else if final_url.starts_with('/') {
                    final_url.clone()
                } else {
                    format!("static/{final_url}")
                };
                std::fs::read(&path).ok()
            };
            if let Some(bytes) = bytes_opt {
                crate::vlog!("[font-face] fetched family={} url={} bytes={}",
                    ff.family, final_url, bytes.len());
                // WOFF/WOFF2 dekomprese (no-op pri TTF/OTF bytes).
                let decoded = super::woff::maybe_decode_woff(&bytes);
                // Variable font detection: log axes pri prvnim nahrani.
                let axes = super::variable_fonts::parse_axes(&decoded);
                if !axes.is_empty() {
                    println!("[font] {} je variable font ({} axes):", ff.family, axes.len());
                    for ax in &axes {
                        println!("  {} {:.0}..{:.0} (default {:.0})",
                            ax.tag, ax.min_value, ax.max_value, ax.default_value);
                    }
                }
                // Color font detection: COLR/CBDT/sbix/SVG.
                let color_info = super::emoji_fonts::detect_color_format(&decoded);
                use super::emoji_fonts::ColorFormat;
                if color_info.format != ColorFormat::None {
                    println!("[font] {} je color font: {:?} (base={}, layers={}, palettes={})",
                        ff.family, color_info.format,
                        color_info.colr_base_count, color_info.colr_layer_count,
                        color_info.cpal_palette_count);
                    // Pri COLR v0: full parse pro rasterization.
                    if matches!(color_info.format, ColorFormat::ColrV0) {
                        if let Some(colr) = super::emoji_fonts::parse_colr_full(&decoded) {
                            self.color_fonts.insert(ff.family.clone(), colr);
                            println!("[font] {} COLR data ulozeny do color_fonts.", ff.family);
                        }
                    }
                }
                match SwashFontFace::from_bytes(decoded) {
                    Some(font) => {
                        // Parse font-weight (default 400 = regular). Bold = >= 600.
                        // Style: italic / oblique.
                        let weight: u32 = ff.weight.split(|c: char| !c.is_ascii_digit())
                            .filter(|s| !s.is_empty())
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(if ff.weight.contains("bold") { 700 } else { 400 });
                        let italic = ff.style.contains("italic") || ff.style.contains("oblique");
                        crate::vlog!("[font-face] OK family={} weight={} italic={} (extra_fonts subset push)",
                            ff.family, weight, italic);
                        // font_registry stale drzi "primary" font per family (prvni).
                        self.font_registry.entry(ff.family.clone()).or_insert_with(|| font.clone());
                        // Atlas extra_fonts ulozi pod 2 keys:
                        // 1) base family - fallback pri lookup bez weight info
                        // 2) "<family>__w<weight>__[i__]" - per CSS Fonts L4
                        //    nearest-match weight lookup (font_for_weight).
                        self.atlas.extra_fonts.entry(ff.family.clone())
                            .or_insert_with(Vec::new)
                            .push(font.clone());
                        crate::browser::layout::register_measure_font(&ff.family, font.clone());
                        let weight_key = if italic {
                            format!("{}__w{}__i__", ff.family, weight)
                        } else {
                            format!("{}__w{}__", ff.family, weight)
                        };
                        self.atlas.extra_fonts.entry(weight_key.clone())
                            .or_insert_with(Vec::new)
                            .push(font.clone());
                        crate::browser::layout::register_measure_font(&weight_key, font);
                        self.loaded_font_urls.insert(url);
                    }
                    None => {
                        eprintln!("[font-face] FAIL SwashFontFace::from_bytes family={}", ff.family);
                    }
                }
            } else {
                eprintln!("[font-face] FAIL fetch family={} url={}", ff.family, final_url);
            }
        }
    }

    /// Nacte image ze souboru / HTTP / data URI a vlozi do RGBA atlasu.
    /// HTTP fetch pres ureq (sync). Data URI dekodovani base64.
    fn load_image(&mut self, src: &str) {
        self.load_image_as(src, src);
    }
    /// Stejne ale fetch_url se lisi od cache key (pro relative URL resolution).
    fn load_image_as(&mut self, cache_key: &str, fetch_url: &str) {
        if self.image_atlas.cache.contains_key(cache_key) { return; }
        // Tombstone check: predchozi pokus selhal (fetch failed nebo decode
        // failed) - dalsi frame uz znova nezkousime. Bez tohoto kazdy frame
        // sync ureq fetch + image::load_from_memory na nesupportovanem
        // formatu = 1 s lag per frame na realnych strankach.
        if self.image_load_failed.contains(cache_key) { return; }
        let bytes_opt = fetch_image_bytes(fetch_url);
        let bytes = match bytes_opt {
            Some(b) => b,
            None => {
                self.image_load_failed.insert(cache_key.to_string());
                return;
            }
        };
        // Cache source bytes pro budouci re-resample pri zoomu.
        self.image_source_bytes.insert(cache_key.to_string(), bytes.clone());
        // Magic-byte sniff (foundation modul `browser::image_decoder`) pre-empti
        // image::load_from_memory pri formatech ktere `image` crate nema.
        let format = crate::browser::image_decoder::detect_format(&bytes);
        // AVIF: pure-Rust dekoder pres `zenavif` (rav1d). Bez system deps -
        // browser sam dekoduje, user nemusi nic instalovat.
        if matches!(format, crate::browser::image_decoder::ImageFormat::Avif) {
            match crate::browser::avif_decode::decode(&bytes) {
                Ok((w, h, rgba)) => {
                    super::layout::set_image_natural_dims(cache_key, w as f32, h as f32);
                    let needs_resize = w > IMAGE_ATLAS_SIZE / 2 || h > IMAGE_ATLAS_SIZE / 2;
                    if needs_resize {
                        let max = IMAGE_ATLAS_SIZE / 2;
                        let scale = (max as f32 / w.max(h) as f32).min(1.0);
                        let new_w = (w as f32 * scale) as u32;
                        let new_h = (h as f32 * scale) as u32;
                        if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
                            let dyn_img = image::DynamicImage::ImageRgba8(img);
                            let small = dyn_img.resize_exact(new_w, new_h,
                                image::imageops::FilterType::Triangle);
                            self.image_atlas.add(cache_key, new_w, new_h, &small.to_rgba8().into_raw());
                            return;
                        }
                        // Fallback: nepodarilo se RgbaImage::from_raw, akceptujem
                        // tombstone (rare - dimensions mismatch).
                        self.image_load_failed.insert(cache_key.to_string());
                        return;
                    }
                    self.image_atlas.add(cache_key, w, h, &rgba);
                    return;
                }
                Err(e) => {
                    eprintln!("[image] AVIF decode failed at {}: {}", cache_key, e.0);
                    self.image_load_failed.insert(cache_key.to_string());
                    return;
                }
            }
        }
        // HEIF/HEIC: pure-Rust pres `heic` crate (H.265 SIMD). Bez system deps.
        if matches!(format, crate::browser::image_decoder::ImageFormat::Heif) {
            match crate::browser::heif_decode::decode(&bytes) {
                Ok((w, h, rgba)) => {
                    super::layout::set_image_natural_dims(cache_key, w as f32, h as f32);
                    let needs_resize = w > IMAGE_ATLAS_SIZE / 2 || h > IMAGE_ATLAS_SIZE / 2;
                    if needs_resize {
                        let max = IMAGE_ATLAS_SIZE / 2;
                        let scale = (max as f32 / w.max(h) as f32).min(1.0);
                        let new_w = (w as f32 * scale) as u32;
                        let new_h = (h as f32 * scale) as u32;
                        if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
                            let dyn_img = image::DynamicImage::ImageRgba8(img);
                            let small = dyn_img.resize_exact(new_w, new_h,
                                image::imageops::FilterType::Triangle);
                            self.image_atlas.add(cache_key, new_w, new_h, &small.to_rgba8().into_raw());
                            return;
                        }
                        self.image_load_failed.insert(cache_key.to_string());
                        return;
                    }
                    self.image_atlas.add(cache_key, w, h, &rgba);
                    return;
                }
                Err(e) => {
                    eprintln!("[image] HEIF decode failed at {}: {}", cache_key, e.0);
                    self.image_load_failed.insert(cache_key.to_string());
                    return;
                }
            }
        }
        // JPEG XL: pure-Rust dekoder pres `jxl-oxide`. Bez system deps.
        if matches!(format, crate::browser::image_decoder::ImageFormat::Jxl) {
            match crate::browser::jxl_decode::decode(&bytes) {
                Ok((w, h, rgba)) => {
                    super::layout::set_image_natural_dims(cache_key, w as f32, h as f32);
                    let needs_resize = w > IMAGE_ATLAS_SIZE / 2 || h > IMAGE_ATLAS_SIZE / 2;
                    if needs_resize {
                        let max = IMAGE_ATLAS_SIZE / 2;
                        let scale = (max as f32 / w.max(h) as f32).min(1.0);
                        let new_w = (w as f32 * scale) as u32;
                        let new_h = (h as f32 * scale) as u32;
                        if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
                            let dyn_img = image::DynamicImage::ImageRgba8(img);
                            let small = dyn_img.resize_exact(new_w, new_h,
                                image::imageops::FilterType::Triangle);
                            self.image_atlas.add(cache_key, new_w, new_h, &small.to_rgba8().into_raw());
                            return;
                        }
                        self.image_load_failed.insert(cache_key.to_string());
                        return;
                    }
                    self.image_atlas.add(cache_key, w, h, &rgba);
                    return;
                }
                Err(e) => {
                    eprintln!("[image] JXL decode failed at {}: {}", cache_key, e.0);
                    self.image_load_failed.insert(cache_key.to_string());
                    return;
                }
            }
        }
        let _ = format;
        if let Ok(img) = image::load_from_memory(&bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
            // Natural dims do layout thread_local cache. flush_inline pri img
            // s max-width/height uses pro proper aspect-preserving resize.
            super::layout::set_image_natural_dims(cache_key, w as f32, h as f32);
            // Velke obrazky downscalujem aby se vesly do atlasu
            if w > IMAGE_ATLAS_SIZE / 2 || h > IMAGE_ATLAS_SIZE / 2 {
                let max = IMAGE_ATLAS_SIZE / 2;
                let scale = (max as f32 / w.max(h) as f32).min(1.0);
                let new_w = (w as f32 * scale) as u32;
                let new_h = (h as f32 * scale) as u32;
                if let Ok(decoded) = image::load_from_memory(&bytes) {
                    let small = decoded.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);
                    let small_rgba = small.to_rgba8();
                    self.image_atlas.add(cache_key, new_w, new_h, &small_rgba.into_raw());
                    return;
                }
            }
            self.image_atlas.add(cache_key, w, h, &raw);
            return;
        }
        // image crate failed - zkusime SVG (resvg). Realne stranky (google
        // logo, github icons) maji SVG ikony.
        if try_decode_svg_into_atlas(&bytes, cache_key, &mut self.image_atlas) {
            return;
        }
        // Vsechno selhalo - tombstone.
        self.image_load_failed.insert(cache_key.to_string());
    }
    /// Re-resample image atlas entry na target physical size. Pouzity pri zoomu
    /// aby image byl ostry na fyzickem rozliseni screen px.
    fn resample_image_for_size(&mut self, cache_key: &str, target_w: u32, target_h: u32) {
        // Skip pokud cached size uz blizko target.
        if let Some(info) = self.image_atlas.cache.get(cache_key) {
            let dw = (info.width as i32 - target_w as i32).abs();
            let dh = (info.height as i32 - target_h as i32).abs();
            if dw < 4 && dh < 4 { return; }
        }
        let bytes = match self.image_source_bytes.get(cache_key).cloned() {
            Some(b) => b,
            None => return,
        };
        let max_atlas = IMAGE_ATLAS_SIZE / 2;
        let cw = target_w.min(max_atlas);
        let ch = target_h.min(max_atlas);
        // SVG: re-rasterize at target_w/h via resvg. Pri ne-SVG raster image:
        // image::load_from_memory + bilinear resize. Bez SVG re-raster by SVG
        // bitmap byl bilinear-upscaled pri zoomu = blur. Chrome re-rasters SVG
        // per visible size.
        let head_len = bytes.len().min(512);
        let is_svg = bytes[..head_len].windows(4)
            .any(|w| w == b"<svg" || w == b"<?xm");
        if is_svg {
            if let Ok(svg_text) = std::str::from_utf8(&bytes) {
                let opt = usvg::Options::default();
                if let Ok(tree) = usvg::Tree::from_str(svg_text, &opt) {
                    let mut pixmap = tiny_skia::Pixmap::new(cw, ch).unwrap();
                    let tree_size = tree.size();
                    let sx = cw as f32 / tree_size.width();
                    let sy = ch as f32 / tree_size.height();
                    let transform = tiny_skia::Transform::from_scale(sx, sy);
                    resvg::render(&tree, transform, &mut pixmap.as_mut());
                    self.image_atlas.add(cache_key, cw, ch, pixmap.data());
                    return;
                }
            }
        }
        if let Ok(img) = image::load_from_memory(&bytes) {
            let resized = img.resize_exact(cw, ch, image::imageops::FilterType::Triangle);
            let rgba = resized.to_rgba8();
            // image_atlas.add nahradi existing entry stejnym key.
            self.image_atlas.add(cache_key, cw, ch, &rgba.into_raw());
        }
    }

    /// Upload image atlas do GPU, jen pokud byly pridany nove obrazky.
    fn upload_image_atlas(&mut self) {
        if !self.image_atlas.dirty { return; }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.image_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.image_atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(IMAGE_ATLAS_SIZE * 4),
                rows_per_image: Some(IMAGE_ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: IMAGE_ATLAS_SIZE, height: IMAGE_ATLAS_SIZE, depth_or_array_layers: 1,
            },
        );
        self.image_atlas.dirty = false;
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        crate::vlog!("[render] resize physical = {}x{} (scale_factor={}, logical = {}x{})",
            w, h, self.scale_factor,
            (w as f32 / self.scale_factor) as u32,
            (h as f32 / self.scale_factor) as u32);
        // Recreate offscreen RTs (format = swap chain pro main pipeline kompat)
        let fmt = self.config.format;
        let make = |dev: &wgpu::Device, label: &str| {
            let tex = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            (tex, view)
        };
        let (a_t, a_v) = make(&self.device, "offscreen_rt_a");
        let (b_t, b_v) = make(&self.device, "offscreen_rt_b");
        self.offscreen_tex = a_t; self.offscreen_view = a_v;
        self.offscreen_tex_b = b_t; self.offscreen_view_b = b_v;
        self.main_rt = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("main_rt"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.shell_rt = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shell_rt"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
    }

    fn upload_atlas(&mut self) {
        if !self.atlas.dirty { return; }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_SIZE),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
        );
        self.atlas.dirty = false;
    }

    /// Renderuje display list s podporou filter subtree + backdrop-filter
    /// + WebGL canvas pass v ramci JEDNOHO swap chain frame.
    /// Vse kreslime do main_rt (intermediate RT), na konci compose -> swap chain.
    /// Backdrop-filter muze cist obsah main_rt (scena za elementem).
    /// Dual render varianta s cache hints: page_skip/shell_skip = true -> skip
    /// render do toho RT, reuse texture obsah z minuleho framu. Compose
    /// posklada vzdy z obou RT (cached + fresh).
    pub fn draw_full_frame_cached(
        &mut self,
        cmds: &[DisplayCommand],
        overlay_cmds: &[DisplayCommand],
        shell_cmds: &[DisplayCommand],
        layout_root: &super::layout::LayoutBox,
        webgl_states: Option<&std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>>,
        scroll_y: f32,
        chrome_top_logical: f32,
        page_skip: bool,
        shell_skip: bool,
    ) {
        // Update viewport uniform pro main pipeline
        // Browser zoom: vp uniform = logical dims (window/zoom). Vertex px coords
        // jsou v logical px (layout running at logical viewport). NDC mapping
        // px/vp pak skaluje obsah o zoom faktor pri compose do framebufferu.
        let (vp_w, vp_h) = self.vp_dims();
        let vp = [vp_w, vp_h, self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));

        // Acquire frame
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return,
        };
        let swap_view = frame.texture.create_view(&Default::default());
        // Main RT view - sem kreslime (ne primo na swap chain)
        let main_rt_view = self.main_rt.create_view(&Default::default());

        // 1. CSS display list (page content) -> main_rt s scissor pod chrome bar.
        // Logical chrome_top_logical -> physical px = * zoom * scale_factor.
        // (Dual-RT shell_rt zatim disabled - shell zustava v overlay_cmds do
        // main_rt. Scissor proto pretrvava.)
        let chrome_top_phys = (chrome_top_logical * self.zoom * self.scale_factor).round() as u32;
        let page_scissor = if chrome_top_phys > 0 {
            Some((0u32, chrome_top_phys, self.config.width,
                  self.config.height.saturating_sub(chrome_top_phys)))
        } else { None };
        // page_skip -> skip render do main_rt, reuse z minulosti.
        let had_segments = if page_skip {
            !cmds.is_empty()
        } else {
            self.draw_segments_into_view_clipped(&main_rt_view, cmds, true, page_scissor)
        };

        // 2. WebGL pass -> main_rt (po page contentu, pred overlay)
        let mut webgl_did_render = false;
        if let Some(states) = webgl_states {
            if !states.is_empty() && !page_skip {
                webgl_did_render = self.run_webgl_frame(layout_root, &main_rt_view, states, scroll_y);
            } else if !states.is_empty() && page_skip {
                webgl_did_render = true;  // reuse signal
            }
        }

        // 3. Overlay (devtools, scrollbars, addr/find bar) -> main_rt PO WebGL,
        // aby UI prvky neprekryl WebGL clear color. start_clear=false zachova
        // existujici page + WebGL obsah.
        let had_overlay = if !overlay_cmds.is_empty() && !page_skip {
            self.draw_segments_into_view_clipped(&main_rt_view, overlay_cmds, false, None)
        } else if !overlay_cmds.is_empty() {
            true  // reuse signal
        } else { false };

        // 4. Shell pass (browser chrome: tabs, addr bar, scrollbars) -> shell_rt.
        // Separate target umozni page main_rt cache + shell-only redraw nezavisle.
        // shell_skip -> reuse shell_rt content z minulosti.
        let shell_rt_view = self.shell_rt.create_view(&Default::default());
        let had_shell = if shell_cmds.is_empty() {
            false
        } else if shell_skip {
            true  // reuse signal
        } else {
            // Explicit clear shell_rt to transparent (0,0,0,0) PRED render, aby
            // alpha-blend compose neperokryl page mimo shell area.
            {
                let mut encoder = self.device.create_command_encoder(&Default::default());
                let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                    label: Some("shell_rt_clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                        view: &shell_rt_view, resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
                });
                self.queue.submit(std::iter::once(encoder.finish()));
            }
            // Render shell_cmds s start_clear=false (uz mame transparent clear).
            self.draw_segments_into_view_clipped(&shell_rt_view, shell_cmds, false, None)
        };

        // 5. Composit main_rt -> swap chain, pak shell_rt overlay nad page.
        // compose_view_to_swap pouziva transform_pipeline (BlendState::ALPHA_BLENDING)
        // + LoadOp::Load. Druhy compose alpha-blend shell pres page.
        if had_segments || webgl_did_render || had_overlay || had_shell {
            let vw = self.config.width as f32;
            let vh = self.config.height as f32;
            self.compose_view_to_swap(&swap_view, &main_rt_view, 0.0, 0.0, vw, vh);
            if had_shell {
                self.compose_view_to_swap(&swap_view, &shell_rt_view, 0.0, 0.0, vw, vh);
            }
        } else {
            // Nic nekresleno - clear swap chain primo
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                    label: Some("frame_clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                        view: &swap_view, resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.page_clear()),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
                });
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // 4. Single present
        frame.present();
    }

    /// Renderuje display list segmenty do existujici TextureView (bez acquire/present).
    /// Vraci true pokud aspon jedna pass byla provedena (pro frame fallback clear).
    /// Pro BackdropFilter: view musi byt main_rt (COPY_SRC) - snapshotuje obsah
    /// pres copy_texture_to_texture pred aplikaci filtru.
    /// Pri start_clear=false neclearuje texturu pri prvni passi (Load namisto
    /// Clear), pouzite pro overlay pass po WebGL.
    /// Pri Some scissor (logical px) clipuje render pass na rect.
    /// Warm-up glyph atlas + image atlas pre kazdy display command vyzaduje
    /// glyf/image. Vola se PRED `draw_segments_into_view_clipped`. Atlas
    /// upload pak ucinkuje vsechny vertices ktere sample texturu.
    ///
    /// `base_url` pouzity pro relative image URL resolve.
    /// warm_atlas_for s raster boostem pro scale(N) layery - glyfy se warmuji
    /// ve vetsim physical_size aby je build_vertices (taky pres RASTER_BOOST)
    /// nasel hi-res = ostry text po scale compose. raster_scale=1.0 = no-op.
    pub fn warm_atlas_for_scaled(&mut self, cmds: &[DisplayCommand], base_url: Option<&str>, raster_scale: f32) {
        RASTER_BOOST.with(|c| c.set(raster_scale.max(1.0)));
        self.warm_atlas_for(cmds, base_url);
        RASTER_BOOST.with(|c| c.set(1.0));
    }

    pub fn warm_atlas_for(&mut self, cmds: &[DisplayCommand], base_url: Option<&str>) {
        for cmd in cmds {
            match cmd {
                DisplayCommand::Text { content, font_size, font_family, color, bold, font_weight, italic, .. } => {
                    let cmd_hash = {
                        use std::hash::{Hash, Hasher};
                        let mut h = ahash::AHasher::default();
                        content.hash(&mut h);
                        (*font_size as u32).hash(&mut h);
                        font_family.hash(&mut h);
                        font_weight.hash(&mut h);
                        italic.hash(&mut h);
                        color.hash(&mut h);
                        // ZOOM v hash - physical_size = font_size * zoom. Bez zoom
                        // v hash by warm same content + style hashed stejne pres
                        // ruzny zoom = atlas physical_size never updated = chars
                        // missing po zoom change.
                        self.zoom.to_bits().hash(&mut h);
                        self.scale_factor.to_bits().hash(&mut h);
                        // RASTER_BOOST v hashi - scale(N) layer warmuje glyfy ve
                        // vetsim physical_size; bez boostu v hashi by dedup
                        // preskocil boosted warm pokud uz existoval unboosted glyf.
                        RASTER_BOOST.with(|c| c.get()).to_bits().hash(&mut h);
                        h.finish()
                    };
                    if self.text_cmd_warmed.contains(&cmd_hash) { continue; }
                    self.text_cmd_warmed.insert(cmd_hash);
                    let _ = bold;
                    let warm_boost = RASTER_BOOST.with(|c| c.get());
                    let phys = (*font_size * self.zoom * warm_boost).round().max(1.0) as u32;
                    let color_font: Option<_> = self.color_fonts.get(font_family).cloned();
                    let color_font_obj: Option<_> = if color_font.is_some() {
                        self.font_registry.get(font_family).cloned()
                    } else { None };
                    for ch in content.chars() {
                        let mut color_added = false;
                        if let (Some(colr), Some(font)) = (color_font.as_ref(), color_font_obj.as_ref()) {
                            let glyph_id = font.glyph_id(ch);
                            if glyph_id != 0 && colr.base_to_layers.contains_key(&glyph_id) {
                                let key = format!("__colr:{}:{}:{}", font_family, ch as u32, *font_size as u32);
                                if !self.image_atlas.contains(&key) {
                                    if let Some((w, h, _, _, rgba)) = super::emoji_fonts::rasterize_color_glyph(
                                        font, glyph_id, *font_size, colr, *color,
                                    ) {
                                        self.image_atlas.add(&key, w as u32, h as u32, &rgba);
                                    }
                                }
                                color_added = true;
                            }
                        }
                        if !color_added {
                            self.atlas.add_styled(font_family, *font_weight, *italic, ch, phys);
                        }
                    }
                }
                DisplayCommand::Image { src, w, h, .. } => {
                    let resolved = match base_url {
                        Some(base) => resolve_url(base, src),
                        None => src.clone(),
                    };
                    self.load_image_as(src, &resolved);
                    let target_w = (*w * self.zoom).round().max(1.0) as u32;
                    let target_h = (*h * self.zoom).round().max(1.0) as u32;
                    self.resample_image_for_size(src, target_w, target_h);
                }
                _ => {}
            }
        }
        // Flush inline-SVG raster cache do image_atlas PRED GPU upload. Bez
        // toho se SVG (+ <text>) prida do CPU atlasu az pozdeji ve
        // flush_inline_svg_cache uvnitr draw_segments - tj. PO tomto uploadu ->
        // GPU image_tex je na miste SVG prazdna -> Image quad sampluje nic ->
        // SVG (shapes i text) nevidet. Na staticke strance se layer texture
        // vyrenderuje 1x (prazdny SVG) a cachuje (no damage) -> uz nikdy.
        // (Na animovane strane to "doženou" continuous re-rendery, proto to
        // driv vypadalo ze SVG funguje jen nekde.)
        flush_inline_svg_cache(&mut self.image_atlas);
        self.upload_atlas();
        self.upload_image_atlas();
    }

    pub fn invalidate_vert_cache(&mut self) {
        self.vert_cache.clear();
        self.atlas_gen = self.atlas_gen.wrapping_add(1);
        self.image_atlas_gen = self.image_atlas_gen.wrapping_add(1);
    }

    /// Nastav / zrus GPU scissor pro nasledujici compose_*_into_encoder cally
    /// (PHYSICAL px, caller clampuje na target dims). Viz COMPOSE_SCISSOR doc.
    pub fn set_compose_scissor(&self, scissor: Option<(u32, u32, u32, u32)>) {
        COMPOSE_SCISSOR.with(|c| c.set(scissor));
    }

    /// Aplikuj COMPOSE_SCISSOR na render pass. Vraci false kdyz je scissor
    /// zero-area (= obsah kompletne cliply) -> caller preskoci draw.
    fn apply_compose_scissor(pass: &mut wgpu::RenderPass) -> bool {
        match COMPOSE_SCISSOR.with(|c| c.get()) {
            Some((x, y, w, h)) => {
                if w == 0 || h == 0 { return false; }
                pass.set_scissor_rect(x, y, w, h);
                true
            }
            None => true,
        }
    }

    /// Render display list do per-layer offscreen texture. Vola se per layer pri
    /// D4 per-layer GPU caching. `layer_w/h` = logical layer dims; `view` =
    /// layer texture view; cmds = layer-local coords (origin (0,0) at layer top-left).
    /// Inspired by WebRender Picture target render (`gfx/wr/webrender/src/picture.rs`).
    pub fn render_into_layer(
        &mut self,
        view: &wgpu::TextureView,
        layer_w: f32,
        layer_h: f32,
        cmds: &[DisplayCommand],
    ) -> bool {
        self.render_into_layer_scaled(view, layer_w, layer_h, 1.0, cmds)
    }

    /// Jako render_into_layer, ale s raster_scale boostem pro layery se scale
    /// transformem. Texture je alokovana raster_scale x vetsi (viz
    /// ensure_layer_texture) a glyf/image raster se boostne o stejny faktor ->
    /// compose pak samluje hi-res texturu = ostry text u scale(N). VIEWPORT
    /// override zustava LOGICAL (content vyplni celou - vetsi - texturu pres NDC).
    pub fn render_into_layer_scaled(
        &mut self,
        view: &wgpu::TextureView,
        layer_w: f32,
        layer_h: f32,
        raster_scale: f32,
        cmds: &[DisplayCommand],
    ) -> bool {
        if cmds.is_empty() { return false; }
        // Set vp override pres layer dims (draw_segments_into_view_clipped uses).
        VIEWPORT_OVERRIDE.with(|c| c.set((layer_w, layer_h)));
        RASTER_BOOST.with(|c| c.set(raster_scale.max(1.0)));
        let res = self.draw_segments_into_view_clipped(view, cmds, true, None);
        RASTER_BOOST.with(|c| c.set(1.0));
        VIEWPORT_OVERRIDE.with(|c| c.set((0.0, 0.0)));
        res
    }

    /// Render display list do per-tile offscreen texture (priority 5 - tile
    /// rasterization phase 2).
    ///
    /// `layer_cmds` = cmds v layer-local coords (origin (0,0) = layer top-left).
    /// `tile_local_x/y` = tile origin offset uvnitr layer.
    /// `tile_w/h` = tile dimensions.
    ///
    /// Vnitrne: clone cmds, shift o (-tile_x, -tile_y) -> tile-local coords,
    /// pak draw_segments do tile view s vp = (tile_w, tile_h).
    ///
    /// Win vs render_into_layer: pri damaged single tile out of N, repaint
    /// jen tile_w*tile_h pixels misto layer_w*layer_h.
    /// Inspired by WebRender `picture::Picture::raster_to_target` per-tile path.
    pub fn render_into_tile(
        &mut self,
        view: &wgpu::TextureView,
        tile_local_x: f32,
        tile_local_y: f32,
        tile_w: f32,
        tile_h: f32,
        layer_cmds: &[DisplayCommand],
    ) -> bool {
        if layer_cmds.is_empty() { return false; }
        // Filtruj cmds na tile rect (layer-local coords) - jen commands co
        // protinaji tile + scoped segmenty atomicky. Tim render_into_tile
        // NEbuildi vertices z CELE vrstvy per tile (drive N dirty tiles = N x
        // full-layer vertex build = hover spiky 18-24ms na tiled root layeru).
        let mut tile_cmds = crate::browser::render::segments::filter_cmds_to_tile(
            layer_cmds, (tile_local_x, tile_local_y, tile_w, tile_h));
        // Prazdny vysledek (nic neprotina tile) = fallback na cely layer, aby se
        // tile texture aspon vyclearovala (draw_segments early-returnuje bez clearu
        // pri prazdnem vstupu = stale/garbage v nove tile texture). Vzacne - body
        // bg obvykle pokryva vsechny tiles.
        if tile_cmds.is_empty() {
            tile_cmds = layer_cmds.to_vec();
        }
        // Shift do tile-local origin (tile top-left = (0,0)).
        for cmd in tile_cmds.iter_mut() {
            crate::browser::render::segments::shift_command_x(cmd, -tile_local_x);
            crate::browser::render::segments::shift_command_y(cmd, -tile_local_y);
        }
        if std::env::var("RWE_TILE_DBG").is_ok() {
            eprintln!("[TILE] origin=({:.0},{:.0}) size=({:.0},{:.0}) cmds={}/{}",
                tile_local_x, tile_local_y, tile_w, tile_h, tile_cmds.len(), layer_cmds.len());
        }
        // Set vp override pres tile dims (draw_segments_into_view_clipped uses).
        VIEWPORT_OVERRIDE.with(|c| c.set((tile_w, tile_h)));
        let res = self.draw_segments_into_view_clipped(view, &tile_cmds, true, None);
        VIEWPORT_OVERRIDE.with(|c| c.set((0.0, 0.0)));
        res
    }

    pub fn draw_segments_into_view_clipped(&mut self, view: &wgpu::TextureView,
                                        cmds: &[DisplayCommand], start_clear: bool,
                                        scissor: Option<(u32, u32, u32, u32)>) -> bool {
        // Set pixel-snap scale pro build_vertices (= zoom * dpr).
        // RASTER_BOOST (= scale-transform hint pri render_into_layer) zvysi
        // glyph/image raster rozliseni o scale faktor -> ostry text u scale(N).
        let raster_boost = RASTER_BOOST.with(|c| c.get());
        PIXEL_SNAP_SCALE.with(|c| c.set(self.zoom * self.scale_factor * raster_boost));
        // vp uniform: (logical_w, logical_h, zoom, _pad). NDC mapping v
        // RECT_SHADER pouziva uniform.viewport. Pres VIEWPORT_OVERRIDE
        // (= render_into_tile sets) pouzij override - jinak self.vp_dims.
        let (ovr_w, ovr_h) = VIEWPORT_OVERRIDE.with(|c| c.get());
        let (vp_w, vp_h) = if ovr_w > 0.0 && ovr_h > 0.0 {
            (ovr_w, ovr_h)
        } else {
            self.vp_dims()
        };
        let vp = [vp_w, vp_h, self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));
        if cmds.is_empty() { return false; }
        let segments: Vec<Seg> = partition_filter_segments(cmds);
        if segments.is_empty() { return false; }
        let mut first_pass = start_clear;
        for seg in segments {
            match seg {
                Seg::Main(slice) => {
                    // Vertex buffer cache - hash cmds slice + atlas/image gen + zoom.
                    // Cache hit reuse Vec<Vertex> = skip build_vertices (20ms saved).
                    // Pri stable display_list (mass hover frames bez visual change)
                    // gpu phase 40-65ms -> 7-10ms.
                    let key = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        slice.len().hash(&mut h);
                        for c in slice {
                            hash_display_command(c, &mut h);
                        }
                        self.atlas_gen.hash(&mut h);
                        self.image_atlas_gen.hash(&mut h);
                        self.zoom.to_bits().hash(&mut h);
                        h.finish()
                    };
                    // Pred build_vertices flush inline-SVG cache (paint zachytil
                    // raster bitmapy v paint pres resvg). Bez tohoto Image cmd s
                    // `__inline_svg_*` key nenasel info v atlas = placeholder.
                    flush_inline_svg_cache(&mut self.image_atlas);
                    let verts_cached = self.vert_cache.iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| v.clone());
                    let verts = if let Some(v) = verts_cached {
                        v
                    } else {
                        let v = build_vertices(slice, &self.atlas, &self.image_atlas, self.zoom);
                        if self.vert_cache.len() >= 8 {
                            self.vert_cache.pop_front();
                        }
                        self.vert_cache.push_back((key, v.clone()));
                        v
                    };
                    self.draw_main_pass_clipped(view, &verts, first_pass, scissor);
                    first_pass = false;
                }
                Seg::Filter { inner, x, y, w, h, radius, color_matrix } => {
                    // Pri VIEWPORT_OVERRIDE active (= inside render_into_layer):
                    // offscreen RT je config-sized, layer vp = mensi. NDC mismatch
                    // mezi draw_to_offscreen + compose_offscreen = double-resample
                    // = blur. Bypass offscreen path - CPU apply color matrix na
                    // inner cmds + draw inline. Pres NON-BLUR filters (sepia,
                    // hue-rotate, saturate, brightness, contrast, grayscale).
                    // Pres BLUR filtry fallback na offscreen path (vyzaduje blur).
                    let override_active = VIEWPORT_OVERRIDE.with(|c| c.get()) != (0.0, 0.0);
                    if override_active && radius < 0.5 {
                        // CPU-side color matrix application na cmds clone.
                        // Inner je &[DisplayCommand] - musime clone do mut Vec.
                        let mut inner_mod: Vec<DisplayCommand> = inner.to_vec();
                        apply_color_matrix_to_cmds(&mut inner_mod, &color_matrix);
                        let inner_verts = build_vertices(&inner_mod, &self.atlas, &self.image_atlas, self.zoom);
                        self.draw_main_pass_clipped(view, &inner_verts, first_pass, scissor);
                        let _ = (x, y, w, h);
                    } else {
                        let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas, self.zoom);
                        self.draw_to_offscreen(&inner_verts);
                        if radius >= 0.5 {
                            self.run_blur_passes(radius);
                        }
                        self.compose_offscreen(view, x, y, w, h, &color_matrix, first_pass);
                    }
                    first_pass = false;
                }
                Seg::Transform3D { inner, x, y, w, h, matrix } => {
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas, self.zoom);
                    self.draw_to_offscreen(&inner_verts);
                    self.compose_transform(view, x, y, w, h, &matrix, first_pass);
                    first_pass = false;
                }
                Seg::Mask { inner, x, y, w, h, mask_src } => {
                    // 1. Render obsah do offscreen RT
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas, self.zoom);
                    self.draw_to_offscreen(&inner_verts);
                    // 2. Compose offscreen -> view s identity color matrix
                    // Pro gradient masku by bylo treba druhy RT s maskovanim;
                    // zatim composit bez modifikace (mask parsing TODO).
                    let identity = [
                        1.0, 0.0, 0.0, 0.0, 0.0,
                        0.0, 1.0, 0.0, 0.0, 0.0,
                        0.0, 0.0, 1.0, 0.0, 0.0,
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ];
                    self.compose_offscreen(view, x, y, w, h, &identity, first_pass);
                    let _ = mask_src;
                    first_pass = false;
                }
                Seg::BackdropFilter { inner, x, y, w, h, radius, color_matrix } => {
                    // V layer-GPU rasteru (VIEWPORT_OVERRIDE aktivni) main_rt
                    // NEOBSAHUJE page za hlavickou (sklada se po vrstvach do
                    // target_view) -> snapshot by byl cerny -> rgba(10,10,12,0.7)
                    // pres cernou = plna tmave seda (= "top bar je sedy"). V tom
                    // pripade preskoc backdrop snapshot/blur/compose a vykresli jen
                    // inner obsah; layer compose ho alpha-blendne pres realny page
                    // content = "lehka rgba" (bez blur efektu, ale spravna barva).
                    let in_layer_raster = VIEWPORT_OVERRIDE.with(|c| c.get()) != (0.0, 0.0);
                    if !in_layer_raster {
                        // 1. Snapshot main_rt -> offscreen_tex (scena za elementem)
                        let mut enc = self.device.create_command_encoder(&Default::default());
                        enc.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.main_rt,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.offscreen_tex,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::Extent3d {
                                width: self.config.width.max(1),
                                height: self.config.height.max(1),
                                depth_or_array_layers: 1,
                            },
                        );
                        self.queue.submit(std::iter::once(enc.finish()));

                        // 2. Blur snapshot
                        if radius >= 0.5 {
                            self.run_blur_passes(radius);
                        }

                        // 3. Composit filtrovany snapshot jako podklad do view
                        self.compose_offscreen(view, x, y, w, h, &color_matrix, first_pass);
                        first_pass = false;
                    } else {
                        let _ = (x, y, w, h, radius, &color_matrix);
                    }

                    // 4. Render inner obsah elementu nahoru (primo do view)
                    let inner_segs = partition_filter_segments(inner);
                    for iseg in inner_segs {
                        match iseg {
                            Seg::Main(s) => {
                                let v = build_vertices(s, &self.atlas, &self.image_atlas, self.zoom);
                                self.draw_main_pass(view, &v, false);
                            }
                            Seg::Filter { inner: fi, x: fx, y: fy, w: fw, h: fh, radius: fr, color_matrix: fm } => {
                                let iv = build_vertices(fi, &self.atlas, &self.image_atlas, self.zoom);
                                self.draw_to_offscreen(&iv);
                                if fr >= 0.5 { self.run_blur_passes(fr); }
                                self.compose_offscreen(view, fx, fy, fw, fh, &fm, false);
                            }
                            Seg::Transform3D { inner: ti, x: tx, y: ty, w: tw, h: th, matrix: tm } => {
                                let iv = build_vertices(ti, &self.atlas, &self.image_atlas, self.zoom);
                                self.draw_to_offscreen(&iv);
                                self.compose_transform(view, tx, ty, tw, th, &tm, false);
                            }
                            Seg::BackdropFilter { .. } | Seg::Mask { .. } | Seg::Blend { .. } => {
                                // Nested backdrop/mask/blend uvnitr backdrop-filter: skip (nepodporovano)
                            }
                        }
                    }
                }
                Seg::Blend { inner, x, y, w, h, mode } => {
                    // CSS mix-blend-mode pres shader-side dst sample (vsech 16 modes).
                    let in_layer_raster = VIEWPORT_OVERRIDE.with(|c| c.get()) != (0.0, 0.0);
                    // 1. Render inner subtree do offscreen_tex (src).
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas, self.zoom);
                    self.draw_to_offscreen(&inner_verts);
                    if in_layer_raster {
                        // V layer-raster main_rt neobsahuje backdrop (layer se sklada
                        // separatne) -> dst snapshot by byl spatny. Fallback na
                        // alpha-over compose (vizualne aspon element nakreslen).
                        let identity = [
                            1.0, 0.0, 0.0, 0.0, 0.0,
                            0.0, 1.0, 0.0, 0.0, 0.0,
                            0.0, 0.0, 1.0, 0.0, 0.0,
                            0.0, 0.0, 0.0, 1.0, 0.0,
                        ];
                        self.compose_offscreen(view, x, y, w, h, &identity, first_pass);
                        first_pass = false;
                    } else {
                        // 2. Snapshot main_rt (dst = scena za blended elementem) ->
                        //    offscreen_tex_b.
                        let mut enc = self.device.create_command_encoder(&Default::default());
                        enc.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.main_rt, mip_level: 0,
                                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.offscreen_tex_b, mip_level: 0,
                                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::Extent3d {
                                width: self.config.width.max(1),
                                height: self.config.height.max(1),
                                depth_or_array_layers: 1,
                            },
                        );
                        self.queue.submit(std::iter::once(enc.finish()));
                        // 3. Blend(src, dst) per-pixel -> write do view (= main_rt).
                        let _ = (x, y, w, h);
                        self.compose_blend_advanced(view, mode, 1.0);
                        first_pass = false;
                    }
                }
            }
        }
        // Pri start_clear=false vraci true vzdy (nelze rozliset z first_pass);
        // pri start_clear=true vraci !first_pass = at least one Seg processed.
        if start_clear { !first_pass } else { true }
    }

    /// Legacy wrapper - draw_segments bez WebGL pass.
    /// Pro App::render se preferuje draw_full_frame ktera handluje WebGL.
    fn draw_segments(&mut self, cmds: &[DisplayCommand]) {
        // Update viewport uniform
        // Browser zoom: vp uniform = logical dims (window/zoom). Vertex px coords
        // jsou v logical px (layout running at logical viewport). NDC mapping
        // px/vp pak skaluje obsah o zoom faktor pri compose do framebufferu.
        let (vp_w, vp_h) = self.vp_dims();
        let vp = [vp_w, vp_h, self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let had_segments = self.draw_segments_into_view_clipped(&view, cmds, true, None);
        if !had_segments {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                    label: Some("clear_only"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                        view: &view, resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.page_clear()),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
                });
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        frame.present();
    }

    /// Vykresli main vertex strip do swap chain (pripadne s Clear, pripadne Load).
    fn draw_main_pass(&self, view: &wgpu::TextureView, vertices: &[Vertex], first: bool) {
        self.draw_main_pass_clipped(view, vertices, first, None);
    }

    /// Verze s scissor rect (physical px x, y, w, h). Mimo rect cliped.
    fn draw_main_pass_clipped(&self, view: &wgpu::TextureView, vertices: &[Vertex], first: bool,
                               scissor: Option<(u32, u32, u32, u32)>) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vb_main"),
            size: ((vertices.len().max(1)) * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            self.queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(vertices));
        }
        // Pri layer/tile render (VIEWPORT_OVERRIDE active) clear na TRANSPARENT
        // = layer ne-paintnute pixely blendnou pres alpha=0 (dst zachova).
        // Pri primary path (override == 0,0) zachovej grey clear (CSS-spec ne-set bg).
        let is_layer_or_tile = VIEWPORT_OVERRIDE.with(|c| c.get()) != (0.0, 0.0);
        let load = if first {
            if is_layer_or_tile {
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
            } else {
                wgpu::LoadOp::Clear(self.page_clear())
            }
        } else {
            wgpu::LoadOp::Load
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("main_seg"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            if let Some((sx, sy, sw, sh)) = scissor {
                // wgpu vyzaduje scissor uvnitr framebuffer dimenze.
                let (fb_w, fb_h) = self.fb_dims();
                let cx = sx.min(fb_w);
                let cy = sy.min(fb_h);
                let cw = sw.min(fb_w.saturating_sub(cx));
                let ch = sh.min(fb_h.saturating_sub(cy));
                if cw > 0 && ch > 0 {
                    pass.set_scissor_rect(cx, cy, cw, ch);
                }
            }
            if !vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
                // LCD subpixel text - second pass s dual-source pipeline.
                // Filter vertices na mode == 9. Hardware si kresli per-channel
                // mask blend (real ClearType-style color fringes).
                // SKIP v layer/tile rasteru (VIEWPORT_OVERRIDE aktivni): dual-
                // source per-channel blend do TRANSPARENT textury produkuje
                // barevna per-channel data ktera nasledny premul-OVER compose
                // neslozi = "rozsypany barevny text" v layerech (mix-blend
                // .blend-text, z-index text layery). Chrome stejne: subpixel AA
                // jen pri opaque backdropu, composited layery = grayscale AA
                // (mode 9 avg z main passu je uz nakresleny).
                let in_layer_raster = VIEWPORT_OVERRIDE.with(|c| c.get()) != (0.0, 0.0);
                if let Some(lcd_pipe) = (&self.lcd_pipeline).as_ref().filter(|_| !in_layer_raster) {
                    let lcd_verts: Vec<Vertex> = vertices.iter()
                        .filter(|v| (v.mode - 9.0).abs() < 0.5)
                        .copied().collect();
                    if !lcd_verts.is_empty() {
                        let lcd_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("lcd_vb"),
                            size: (lcd_verts.len() * std::mem::size_of::<Vertex>()) as u64,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        self.queue.write_buffer(&lcd_vbuf, 0, bytemuck::cast_slice(&lcd_verts));
                        pass.set_pipeline(lcd_pipe);
                        pass.set_bind_group(0, &self.bind_group, &[]);
                        pass.set_vertex_buffer(0, lcd_vbuf.slice(..));
                        pass.draw(0..lcd_verts.len() as u32, 0..1);
                    }
                }
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Renderuje filter inner cmds do offscreen_view (clear transparent).
    fn draw_to_offscreen(&self, vertices: &[Vertex]) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vb_offscr"),
            size: ((vertices.len().max(1)) * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            self.queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(vertices));
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("offscreen_subtree"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view: &self.offscreen_view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            if !vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Composit libovolny TextureView do swap chain v ramci canvas rect.
    /// Pouziva transform_pipeline (samples z source view, mapuje uv 0..1
    /// na canvas rect quad). Source view musi byt config.format.
    pub fn compose_view_to_swap(&self, swap_view: &wgpu::TextureView, source_view: &wgpu::TextureView, x: f32, y: f32, w: f32, h: f32) {
        // x/y/w/h v logical px - vp uniform v logical (window/zoom) aby NDC
        // mapping odpovidal hlavnimu pipeline (zoom skalovani v render).
        // Pres target_size override (webview RT mensi nez surface) pouzij
        // target dims jako effective vp - jinak compose maps na wrong pixel
        // grid = WebGL canvas neviditelny (drawn outside webview RT bounds).
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let hw = w * 0.5;
        let hh = h * 0.5;
        let (vp_w, vp_h) = self.vp_dims();
        let vw = vp_w;
        let vh = vp_h;
        // Identity matrix v transform shader format
        let uniform_data: [f32; 32] = [
            1.0, 0.0, 0.0, 0.0,  // row0
            0.0, 1.0, 0.0, 0.0,  // row1
            0.0, 0.0, 1.0, 0.0,  // row2
            0.0, 0.0, 0.0, 1.0,  // row3
            cx, cy, hw, hh,       // center
            vw, vh, 0.0, 0.0,     // viewport
            0.0, 0.0, 1.0, 1.0,   // uv_box (u0, v0, u1, v1)
            0.0, 0.0, 0.0, 0.0,   // padding
        ];
        self.queue.write_buffer(&self.transform_uniform_buf, 0, bytemuck::cast_slice(&uniform_data));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compose_view_bg"),
            layout: &self.transform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.transform_uniform_buf.as_entire_binding() },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("compose_view_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view: swap_view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.transform_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Composit offscreen_tex_a do swap chain pres scissor (x, y, w, h).
    /// Aplikuje 4x5 color matrix (identity = passthrough).
    /// Pouziva fullscreen triangle + alpha blend; scissor omezi vystup na bbox.
    /// Acquire swap chain texture + composite extern target view (typicky
    /// WebView offscreen RT) fullscreen do swap chain + present. Pouziti:
    /// shell crate / hostujici aplikace renderuje WebView do offscreen,
    /// pak vola tuto fn aby se zobrazil v okne.
    ///
    /// Vrati `false` pokud surface get_current_texture selhal nebo
    /// compose pipeline neni dostupna.
    pub fn present_external_to_swap_chain(&self, src_view: &wgpu::TextureView) -> bool {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return false,
        };
        let swap_view = frame.texture.create_view(&Default::default());

        // Identity color matrix - 4x5 layout (4 rows x 4 channels + offset col).
        // build_compose_uniform_box reads m[0..4] row0, m[5..9] row1, m[10..14] row2,
        // m[15..19] row3, offsets m[4],m[9],m[14],m[19].
        // POZOR: drive 4x4 + offset_row layout = row1 cetla z m[5..8]=(1,0,0,0)
        // misto (0,1,0,0) = vystup (R,R,R,R) = GRAYSCALE bug (= "stupne sedi").
        let identity: [f32; 20] = [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ];
        let (vp_w, vp_h) = self.vp_dims();
        let (uniform_data, _vis) = build_compose_uniform_box(
            &identity, 0.0, 0.0, vp_w, vp_h, 0.0, 0.0, 1.0, 1.0, vp_w, vp_h);
        self.queue.write_buffer(&self.compose_uniform_buf, 0, bytemuck::cast_slice(&uniform_data));

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present_external_bg"),
            layout: &self.compose_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.compose_uniform_buf.as_entire_binding() },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: Some("present_external_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &swap_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.page_clear()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.compose_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        true
    }

    /// Logical viewport dimensions pro vp uniform. Pri WebView render_via
    /// override pres target_size (RT velikost). Default = full surface.
    fn vp_dims(&self) -> (f32, f32) {
        let (w, h) = self.target_size.unwrap_or((self.config.width, self.config.height));
        let scale = (self.zoom * self.scale_factor).max(0.01);
        (w as f32 / scale, h as f32 / scale)
    }

    /// Framebuffer dimensions pro scissor/viewport clamping (physical px).
    /// target_size.unwrap_or(config) - pri WebView RT pouzij RT velikost.
    fn fb_dims(&self) -> (u32, u32) {
        self.target_size.unwrap_or((self.config.width, self.config.height))
    }

    /// Compositni N offscreen textures vertical-stacked do swap chain.
    /// Kazdy layer = (view, height_physical_px). Posledni layer dostane
    /// zbytek vysky (clip pri overflow).
    ///
    /// Pouziti: shell chrome top + page middle + devtools bottom.
    pub fn present_layered_external_to_swap_chain(
        &self,
        layers: &[(&wgpu::TextureView, u32)],
    ) -> bool {
        if layers.is_empty() { return false; }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return false,
        };
        let swap_view = frame.texture.create_view(&Default::default());

        // Identity 4x5 layout (see present_external_to_swap_chain comment).
        let identity: [f32; 20] = [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ];
        // present_layered: per set_viewport quad fullscreen sample full UV.
        let (vp_w_id, vp_h_id) = self.vp_dims();
        let (uniform_data_layered, _vis) = build_compose_uniform_box(
            &identity, 0.0, 0.0, vp_w_id, vp_h_id, 0.0, 0.0, 1.0, 1.0, vp_w_id, vp_h_id);
        self.queue.write_buffer(&self.compose_uniform_buf, 0, bytemuck::cast_slice(&uniform_data_layered));

        let w_total = self.config.width as f32;
        let h_total = self.config.height as f32;

        // Build bind groups + viewport positions.
        let mut bind_groups: Vec<wgpu::BindGroup> = Vec::with_capacity(layers.len());
        let mut viewports: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(layers.len());
        let mut y_cursor = 0.0_f32;
        for (i, (view, h_px)) in layers.iter().enumerate() {
            let h = if i == layers.len() - 1 {
                (h_total - y_cursor).max(1.0)
            } else {
                (*h_px as f32).min(h_total - y_cursor).max(1.0)
            };
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("present_layered_bg"),
                layout: &self.compose_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.compose_uniform_buf.as_entire_binding() },
                ],
            });
            bind_groups.push(bg);
            viewports.push((0.0, y_cursor, w_total, h));
            y_cursor += h;
        }

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: Some("present_layered_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &swap_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.compose_pipeline);
            for (i, (vx, vy, vw, vh)) in viewports.iter().enumerate() {
                pass.set_viewport(*vx, *vy, *vw, *vh, 0.0, 1.0);
                pass.set_bind_group(0, &bind_groups[i], &[]);
                pass.draw(0..6, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        true
    }

    /// Compositni 2 offscreen textures vertical split do swap chain.
    /// `top_view` se zobrazi v top `split_ratio` cast (0.0..1.0), `bottom_view`
    /// dole. Bez separatoru / borderu - shell je muze nakreslit pres.
    /// Pres set_viewport scaling - kazdy fullscreen triangle samples 0..1 ze
    /// sve src_view, ale fyzicky zabira jen sub-rect swap chain.
    ///
    /// Pouziti: D4b devtools split layout (page top 70%, devtools bottom 30%).
    /// Pokud `split_ratio` <= 0.0 nebo >= 1.0, redukuje na single-view present.
    pub fn present_split_external_to_swap_chain(
        &self,
        top_view: &wgpu::TextureView,
        bottom_view: &wgpu::TextureView,
        split_ratio: f32,
    ) -> bool {
        if split_ratio <= 0.0 { return self.present_external_to_swap_chain(bottom_view); }
        if split_ratio >= 1.0 { return self.present_external_to_swap_chain(top_view); }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return false,
        };
        let swap_view = frame.texture.create_view(&Default::default());

        // Identity 4x5 layout (see present_external_to_swap_chain comment).
        let identity: [f32; 20] = [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ];
        let (vp_w_split, vp_h_split) = self.vp_dims();
        let (uniform_data_split, _vis) = build_compose_uniform_box(
            &identity, 0.0, 0.0, vp_w_split, vp_h_split, 0.0, 0.0, 1.0, 1.0, vp_w_split, vp_h_split);
        self.queue.write_buffer(&self.compose_uniform_buf, 0, bytemuck::cast_slice(&uniform_data_split));

        let bg_top = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present_split_top_bg"),
            layout: &self.compose_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(top_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.compose_uniform_buf.as_entire_binding() },
            ],
        });
        let bg_bottom = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present_split_bottom_bg"),
            layout: &self.compose_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(bottom_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.compose_uniform_buf.as_entire_binding() },
            ],
        });

        let w = self.config.width as f32;
        let h = self.config.height as f32;
        let top_h = (h * split_ratio).max(1.0).floor();
        let bottom_y = top_h;
        let bottom_h = (h - top_h).max(1.0);

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: Some("present_split_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &swap_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.compose_pipeline);
            pass.set_viewport(0.0, 0.0, w, top_h, 0.0, 1.0);
            pass.set_bind_group(0, &bg_top, &[]);
            pass.draw(0..6, 0..1);
            pass.set_viewport(0.0, bottom_y, w, bottom_h, 0.0, 1.0);
            pass.set_bind_group(0, &bg_bottom, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        true
    }

    /// Pristup k device Renderer pro hostujici aplikaci (custom rendering,
    /// shader compile, buffer create). Sdileny zaroven s WebView pres Engine.
    pub fn device(&self) -> &wgpu::Device { &self.device }

    /// Pristup ke queue Renderer pro hostujici aplikaci (submit, ...).
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }

    /// Aktualni surface config (width, height, scale_factor).
    pub fn surface_size(&self) -> (u32, u32) { (self.config.width, self.config.height) }

    /// HiDPI scale_factor (CSS px -> physical px).
    pub fn scale_factor_value(&self) -> f32 { self.scale_factor }

    /// Resize surface + interni RTs - hostujici aplikace vola na winit Resized.
    pub fn resize_surface(&mut self, w: u32, h: u32) {
        self.resize(w.max(1), h.max(1));
    }

    /// Pristup ke glyph atlas (cumulative advances pro per-glyph selection).
    pub fn atlas(&self) -> &GlyphAtlas { &self.atlas }

    /// Composite arbitrary src view to dst view (D4 per-layer compositing).
    /// Sample z `src` (layer texture), paint quad at (x, y, w, h) v dst v
    /// LOGICAL souradnicich (scaled internally pres zoom * scale_factor).
    /// `opacity` skaluje alpha (1.0 = no-op). `first` = clear vs load.
    /// Inspired by WebRender composite.rs::ComposeOp.
    /// CSS mix-blend-mode aware composite. mode_id mapuje na
    /// computed_style::BlendMode discriminant (0=Normal, 1=Multiply, 2=Screen,
    /// 3=Overlay, 4=Darken, 5=Lighten, ...).
    ///
    /// Supportovane via wgpu BlendState pipeline: Normal/Multiply/Screen/
    /// Darken/Lighten. Ostatni modes fall through na Normal (= alpha blend).
    /// Pro real Overlay/ColorDodge/Diff/Hue/Sat/Color/Lum potreba shader-side
    /// dst sample (copy fb -> snapshot tex). TODO.
    pub fn compose_view_to_view_blend(
        &self,
        dst: &wgpu::TextureView,
        src: &wgpu::TextureView,
        x: f32, y: f32, w: f32, h: f32,
        opacity: f32,
        blend_mode_id: u32,
        first: bool,
    ) {
        // Set thread_local blend pipeline override -> compose_view_to_view uses it.
        BLEND_MODE_OVERRIDE.with(|c| c.set(blend_mode_id));
        self.compose_view_to_view(dst, src, x, y, w, h, opacity, first);
        BLEND_MODE_OVERRIDE.with(|c| c.set(0));
    }

    pub fn compose_view_to_view(
        &self,
        dst: &wgpu::TextureView,
        src: &wgpu::TextureView,
        x: f32, y: f32, w: f32, h: f32,
        opacity: f32,
        first: bool,
    ) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.compose_view_to_view_into_encoder(&mut encoder, dst, src, x, y, w, h, opacity, first);
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Compose pres existujici encoder - pres batch usage (= jeden submit
    /// pres N compose draws). Caller manage encoder + submit.
    /// Allokuje SVUJ uniform buffer per call (NESDILI self.compose_uniform_buf
    /// jako single compose path - write_buffer by mezi N calls prepsal vsechny
    /// na poslední uniform).
    /// Chrome-style selection text pass: kresli recolor Text klony s
    /// glyph-level clipem na highlight recty (jen vybrane znaky). Vola
    /// webview po overlay draw; bez clear (pres existujici obsah).
    pub fn draw_selection_text_pass(
        &mut self,
        view: &wgpu::TextureView,
        cmds: &[DisplayCommand],
        rects: &[(f32, f32, f32, f32)],
    ) {
        if cmds.is_empty() || rects.is_empty() { return; }
        SELECTION_GLYPH_CLIP.with(|c| *c.borrow_mut() = rects.to_vec());
        // Vert cache bypass neni nutny - hash cmds (jine barvy nez original)
        // + rects hash pridan do klice v draw_segments neni; misto toho
        // build primo bez cache:
        PIXEL_SNAP_SCALE.with(|c| c.set(self.zoom * self.scale_factor));
        let (vp_w, vp_h) = self.vp_dims();
        let vp = [vp_w, vp_h, self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));
        let verts = build_vertices(cmds, &self.atlas, &self.image_atlas, self.zoom);
        SELECTION_GLYPH_CLIP.with(|c| c.borrow_mut().clear());
        if verts.is_empty() { return; }
        self.draw_main_pass_clipped(view, &verts, false, None);
    }

    /// Clear barva prvniho render passu - page canvas bg (body/html z
    /// paint::canvas_background) nebo UA default svetla 0.95. Na tmavych
    /// strankach bez tohoto prosvitala bila za layout rootem (scrollbar
    /// gutter + plocha pod kratkym contentem).
    pub(crate) fn page_clear(&self) -> wgpu::Color {
        match self.page_clear_color {
            Some(c) => wgpu::Color {
                r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64,
            },
            None => wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 },
        }
    }

    pub fn compose_view_to_view_into_encoder(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        dst: &wgpu::TextureView,
        src: &wgpu::TextureView,
        x: f32, y: f32, w: f32, h: f32,
        opacity: f32,
        first: bool,
    ) {
        let a = opacity.clamp(0.0, 1.0);
        // 4x5 color matrix layout (channels + offset). Build_compose_uniform_box
        // reads m[0..4], m[5..9], m[10..14], m[15..19] jako rows + m[4],m[9],m[14],m[19]
        // jako offsets. Identity = diagonal 1s + alpha=a v row3 col3.
        let m: [f32; 20] = [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, a,   0.0,
        ];
        let (vp_w, vp_h) = self.vp_dims();
        let (uniform_data, vis) = build_compose_uniform_box(
            &m, x, y, w, h, 0.0, 0.0, 1.0, 1.0, vp_w, vp_h);
        // Per-call uniform buffer - batch safe.
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compose_layer_uniform_batch"),
            size: 112,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, bytemuck::cast_slice(&uniform_data));
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compose_layer_bg"),
            layout: &self.compose_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                // Nearest sampler - layer tex 1:1 phys px -> target = sharp.
                // Bilinear by zpusobil text blur na sub-pixel float NDC boundaries.
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: buf.as_entire_binding() },
            ],
        });
        let load = if first {
            wgpu::LoadOp::Clear(self.page_clear())
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("compose_layer_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None, view: dst, resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
        });
        if vis && Self::apply_compose_scissor(&mut pass) {
            let mode = BLEND_MODE_OVERRIDE.with(|c| c.get());
            let pipeline = match mode {
                1 => &self.compose_pipeline_multiply,
                2 => &self.compose_pipeline_screen,
                4 => &self.compose_pipeline_darken,
                5 => &self.compose_pipeline_lighten,
                _ => &self.compose_pipeline,
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
    }

    /// CSS mix-blend-mode LAYER compose. Blend `src` (layer texture) pres
    /// backdrop = aktualni obsah `dst_texture` (= co se uz vykompozitlo). Snapshot
    /// dst_texture -> offscreen_tex_b, pak blend shader sampluje oboje a zapise
    /// vysledek do `dst_view` v regionu (x,y,w,h). Vse do sdileneho encoderu
    /// (snapshot vidi predchozi compose passy - GPU serializuje commandy).
    /// `mode` = computed_style::BlendMode discriminant (1=Multiply..15=Luminosity).
    #[allow(clippy::too_many_arguments)]
    /// DEBUG: dump GPU textury do PNG souboru (RWE_DUMP_LAYERS). Blocking
    /// readback - jen pro diagnostiku.
    pub fn debug_dump_texture(&self, tex: &wgpu::Texture, w: u32, h: u32, path: &str) {
        let bpr = ((w * 4 + 255) / 256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dbg_dump"),
            size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0,
                    bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(std::iter::once(enc.finish()));
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        if rx.recv().map(|r| r.is_ok()).unwrap_or(false) {
            let data = slice.get_mapped_range();
            let mut img: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
            for row in 0..h {
                let start = (row * bpr) as usize;
                for px in 0..w {
                    let i = start + (px * 4) as usize;
                    // BGRA -> RGBA
                    img.push(data[i + 2]);
                    img.push(data[i + 1]);
                    img.push(data[i]);
                    img.push(data[i + 3]);
                }
            }
            drop(data);
            let _ = image::save_buffer(path, &img, w, h, image::ColorType::Rgba8);
            eprintln!("[DUMP] {} ({}x{})", path, w, h);
        }
        buf.unmap();
    }

    pub fn compose_blend_layer_into_encoder(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        dst_view: &wgpu::TextureView,
        dst_texture: &wgpu::Texture,
        src: &wgpu::TextureView,
        x: f32, y: f32, w: f32, h: f32,
        mode: u8,
        opacity: f32,
    ) {
        // 1. Snapshot backdrop (dst_texture) -> offscreen_tex_b. Copy extent =
        //    min rozmeru (target vs offscreen) - robustni pri ruznych velikostech.
        let cw = self.config.width.max(1).min(dst_texture.width());
        let ch = self.config.height.max(1).min(dst_texture.height());
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { texture: dst_texture, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyTextureInfo { texture: &self.offscreen_tex_b, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
        );
        // 2. dst_box NDC + src_uv pres build_compose_uniform_box (identity matrix).
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ];
        let (vp_w, vp_h) = self.vp_dims();
        let (cdata, vis) = build_compose_uniform_box(&identity, x, y, w, h, 0.0, 0.0, 1.0, 1.0, vp_w, vp_h);
        if !vis { return; }
        // BCParams: dst_box[4] + src_uv[4] + mode u32 + opacity f32 + duv_scale vec2 = 48 B.
        let mut buf = [0u8; 48];
        // dst_box = cdata[20..24], src_uv = cdata[24..28]
        for i in 0..4 { buf[i*4..i*4+4].copy_from_slice(&cdata[20+i].to_le_bytes()); }
        for i in 0..4 { buf[16+i*4..16+i*4+4].copy_from_slice(&cdata[24+i].to_le_bytes()); }
        buf[32..36].copy_from_slice(&(mode as u32).to_le_bytes());
        buf[36..40].copy_from_slice(&opacity.clamp(0.0, 1.0).to_le_bytes());
        // duv_scale = target/offscreen_b: backdrop kopie je 1:1 texel, ale
        // offscreen_b muze byt vetsi (config/window) nez dst (page textura bez
        // chrome baru) -> bez scale by duv cetlo posunuty backdrop (offset roste
        // s y = raw pruh na spodku blend boxu, blendoval s obsahem POD sebou).
        let dsx = dst_texture.width() as f32 / self.offscreen_tex_b.width().max(1) as f32;
        let dsy = dst_texture.height() as f32 / self.offscreen_tex_b.height().max(1) as f32;
        buf[40..44].copy_from_slice(&dsx.to_le_bytes());
        buf[44..48].copy_from_slice(&dsy.to_le_bytes());
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blend_compose_uniform_batch"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&ubuf, 0, &buf);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blend_compose_bg"),
            layout: &self.blend_compose_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.offscreen_view_b) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.compose_smp) },
                wgpu::BindGroupEntry { binding: 4, resource: ubuf.as_entire_binding() },
            ],
        });
        // 3. Blend pass (REPLACE) do dst_view - jen dst_box quad, zbytek Load.
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
            label: Some("blend_compose_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                view: dst_view, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
        });
        if Self::apply_compose_scissor(&mut pass) {
            pass.set_pipeline(&self.blend_compose_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
    }

    /// CSS mix-blend-mode advanced compose: per-pixel dst sample.
    /// src = offscreen_tex (uz vyrenderovany blended element subtree),
    /// dst = offscreen_tex_b (snapshot main_rt = scena za elementem).
    /// Shader aplikuje blend formuli + Porter-Duff over -> primy write do `view`.
    /// `view` musi byt main_rt (= ten co se snapshotoval do offscreen_tex_b).
    fn compose_blend_advanced(&self, view: &wgpu::TextureView, mode: u8, opacity: f32) {
        // Uniform: blend_mode u32 + opacity f32 + 2x pad.
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&(mode as u32).to_le_bytes());
        buf[4..8].copy_from_slice(&opacity.to_le_bytes());
        self.queue.write_buffer(&self.advanced_blend_uniform, 0, &buf);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("advanced_blend_bg"),
            layout: &self.advanced_blend_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.offscreen_view_b) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
                wgpu::BindGroupEntry { binding: 4, resource: self.advanced_blend_uniform.as_entire_binding() },
            ],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("advanced_blend_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    // Shader rekonstruuje cely vystup (vc. dst kde src.a=0) -> Load
                    // staci, full-screen triangle prepise vsechny pixely (REPLACE).
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.advanced_blend_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1); // fullscreen triangle
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn compose_offscreen(&self, view: &wgpu::TextureView, x: f32, y: f32, w: f32, h: f32, color_matrix: &[f32; 20], first: bool) {
        // offscreen_tex je fb-sized snapshot. UV sample region = element bbox/viewport.
        // POZOR: vp musi byt STEJNE jako vp pri draw_to_offscreen call (= aktualni
        // vp uniform). Pri render_into_layer = VIEWPORT_OVERRIDE set na layer dims.
        // Drive self.vp_dims() vracelo webview viewport - UV nesedeli s NDC mapping
        // -> filter content composnuty mimo vidne UV = neviditelne.
        let (ovr_w, ovr_h) = VIEWPORT_OVERRIDE.with(|c| c.get());
        let (vp_w, vp_h) = if ovr_w > 0.0 && ovr_h > 0.0 {
            (ovr_w, ovr_h)
        } else {
            self.vp_dims()
        };
        let u0 = (x / vp_w.max(0.001)).clamp(0.0, 1.0);
        let v0 = (y / vp_h.max(0.001)).clamp(0.0, 1.0);
        let u1 = ((x + w) / vp_w.max(0.001)).clamp(0.0, 1.0);
        let v1 = ((y + h) / vp_h.max(0.001)).clamp(0.0, 1.0);
        let (uniform_data, vis) = build_compose_uniform_box(
            color_matrix, x, y, w, h, u0, v0, u1, v1, vp_w, vp_h);
        self.queue.write_buffer(&self.compose_uniform_buf, 0, bytemuck::cast_slice(&uniform_data));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compose_bg"),
            layout: &self.compose_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.compose_uniform_buf.as_entire_binding() },
            ],
        });
        let load = if first {
            wgpu::LoadOp::Clear(self.page_clear())
        } else {
            wgpu::LoadOp::Load
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("compose_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            if vis {
                pass.set_pipeline(&self.compose_pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Compose offscreen RT do swap chain pres 3D transform pipeline.
    /// Vykresli quad s 4 rohy transformovanymi 4x4 matici (vc perspective).
    /// Composite arbitrary src view do dst pres 4x4 transform matrix. Pro
    /// layer compose - layer.texture je v local coords (origin 0,0, full UV).
    /// Inspired by WebRender composite.rs transform handling.
    pub fn compose_view_to_view_transform(
        &self,
        dst: &wgpu::TextureView,
        src: &wgpu::TextureView,
        x: f32, y: f32, w: f32, h: f32,
        matrix: &[f32; 16],
        first: bool,
    ) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.compose_view_to_view_transform_into_encoder(
            &mut encoder, dst, src, x, y, w, h, matrix, first);
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Transform compose pres sdileny encoder - batch usage. Caller submit.
    /// Per-call uniform buffer (sdileny self.transform_uniform_buf NESLOUZI
    /// batch use - write_buffer prepise predchozi compose data).
    pub fn compose_view_to_view_transform_into_encoder(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        dst: &wgpu::TextureView,
        src: &wgpu::TextureView,
        x: f32, y: f32, w: f32, h: f32,
        matrix: &[f32; 16],
        first: bool,
    ) {
        // Layer tex je sized presne na layer dims, full UV (0,0)-(1,1).
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let hw = w * 0.5;
        let hh = h * 0.5;
        let z = (self.zoom * self.scale_factor).max(0.0001);
        let (fb_w, fb_h) = self.fb_dims();
        let vw = fb_w as f32;
        let vh = fb_h as f32;
        let m = matrix;
        let uniform_data: [f32; 32] = [
            m[0], m[1], m[2], m[3],
            m[4], m[5], m[6], m[7],
            m[8], m[9], m[10], m[11],
            m[12], m[13], m[14], m[15],
            cx, cy, hw, hh,
            vw / z, vh / z, 0.0, 0.0,
            // UV box = full layer tex (no sub-region).
            0.0, 0.0, 1.0, 1.0,
            0.0, 0.0, 0.0, 0.0,
        ];
        // Per-call uniform buffer - batch safe.
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compose_layer_transform_uniform_batch"),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, bytemuck::cast_slice(&uniform_data));
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compose_layer_transform_bg"),
            layout: &self.transform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: buf.as_entire_binding() },
            ],
        });
        let load = if first {
            wgpu::LoadOp::Clear(self.page_clear())
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("compose_layer_transform_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None, view: dst, resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
        });
        if Self::apply_compose_scissor(&mut pass) {
            pass.set_pipeline(&self.transform_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
    }

    fn compose_transform(&self, view: &wgpu::TextureView, x: f32, y: f32, w: f32, h: f32, matrix: &[f32; 16], first: bool) {
        // UV box: jaka cast offscreen RT obsahuje element. Offscreen RT je
        // viewport size, element je v logical px (x..x+w, y..y+h). UV mapping
        // musi pouzit STEJNY vp jako draw_to_offscreen (= VIEWPORT_OVERRIDE pri
        // render_into_layer, jinak vp_dims). UV = x_logical / vp_logical.
        // Drive vw = config.width_phys + z multiplier - fungovalo pres
        // monolithic vp (= surface logical), ale pri layer override UV nesedi =
        // transform content composnuty mimo vidne UV = neviditelne (= "transform
        // zmizel pri D4").
        let z = (self.zoom * self.scale_factor).max(0.0001);
        let (ovr_w, ovr_h) = VIEWPORT_OVERRIDE.with(|c| c.get());
        let (vp_w, vp_h) = if ovr_w > 0.0 && ovr_h > 0.0 {
            (ovr_w, ovr_h)
        } else {
            self.vp_dims()
        };
        let u0 = (x / vp_w.max(0.001)).clamp(0.0, 1.0);
        let v0 = (y / vp_h.max(0.001)).clamp(0.0, 1.0);
        let u1 = ((x + w) / vp_w.max(0.001)).clamp(0.0, 1.0);
        let v1 = ((y + h) / vp_h.max(0.001)).clamp(0.0, 1.0);
        // Compose-pres-vp uniform tady taky pres override-aware vp.
        let vw = vp_w * z;
        let vh = vp_h * z;
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let hw = w * 0.5;
        let hh = h * 0.5;

        // Layout uniformu: 8x vec4 = 128 bytes
        // matrix v WGSL row-major: row0 = [m[0], m[1], m[2], m[3]], etc.
        let m = matrix;
        let uniform_data: [f32; 32] = [
            // row0
            m[0], m[1], m[2], m[3],
            // row1
            m[4], m[5], m[6], m[7],
            // row2
            m[8], m[9], m[10], m[11],
            // row3
            m[12], m[13], m[14], m[15],
            // center (cx, cy, hw, hh) - vsechno v logical px (vp uniform tez logical)
            cx, cy, hw, hh,
            // viewport - logical (window/zoom). NDC = px / logical_vp -> px*zoom/window physical.
            vw / z, vh / z, 0.0, 0.0,
            // uv_box
            u0, v0, u1, v1,
            // padding
            0.0, 0.0, 0.0, 0.0,
        ];
        self.queue.write_buffer(&self.transform_uniform_buf, 0, bytemuck::cast_slice(&uniform_data));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("transform_bg"),
            layout: &self.transform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.transform_uniform_buf.as_entire_binding() },
            ],
        });
        let load = if first {
            wgpu::LoadOp::Clear(self.page_clear())
        } else {
            wgpu::LoadOp::Load
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("transform_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.transform_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn draw(&self, vertices: &[Vertex]) {
        // Update uniform: viewport
        // Browser zoom: vp uniform = logical dims (window/zoom). Vertex px coords
        // jsou v logical px (layout running at logical viewport). NDC mapping
        // px/vp pak skaluje obsah o zoom faktor pri compose do framebufferu.
        let (vp_w, vp_h) = self.vp_dims();
        let vp = [vp_w, vp_h, self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return,
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.page_clear()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn red_rgba(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&[255, 0, 0, 255]);
        }
        v
    }

    #[test]
    fn image_atlas_add_returns_uv() {
        let mut atlas = ImageAtlas::new();
        let ok = atlas.add("a.png", 100, 50, &red_rgba(100, 50));
        assert!(ok);
        let info = atlas.get("a.png").unwrap();
        assert_eq!(info.width, 100.0);
        assert_eq!(info.height, 50.0);
        assert_eq!(info.uv0, [0.0, 0.0]);
        assert!(info.uv1[0] > 0.0 && info.uv1[0] <= 1.0);
        assert!(atlas.dirty);
    }

    #[test]
    fn image_atlas_packs_two_images_side_by_side() {
        let mut atlas = ImageAtlas::new();
        atlas.add("a.png", 100, 50, &red_rgba(100, 50));
        atlas.add("b.png", 200, 80, &red_rgba(200, 80));
        let a = atlas.get("a.png").unwrap();
        let b = atlas.get("b.png").unwrap();
        // b ma pozici za a (cursor_x posunut)
        assert!(b.uv0[0] > a.uv1[0] - 1e-6, "b ma byt vpravo od a");
        assert_eq!(a.uv0[1], b.uv0[1]); // stejny radek
    }

    #[test]
    fn image_atlas_overflow_returns_false() {
        let mut atlas = ImageAtlas::new();
        // Vetsi nez atlas
        let ok = atlas.add("huge.png", IMAGE_ATLAS_SIZE + 100, 100, &red_rgba(IMAGE_ATLAS_SIZE + 100, 100));
        assert!(!ok);
        assert!(atlas.get("huge.png").is_none());
    }

    #[test]
    fn image_atlas_dedup_same_src() {
        let mut atlas = ImageAtlas::new();
        atlas.add("a.png", 100, 50, &red_rgba(100, 50));
        let cursor_after_first = atlas.cursor_x;
        atlas.add("a.png", 100, 50, &red_rgba(100, 50));
        // Druhe pridani neposunuje kurzor (cached)
        assert_eq!(atlas.cursor_x, cursor_after_first);
    }
}
