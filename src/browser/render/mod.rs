/// wgpu renderer + winit window + frame loop.
///
/// Real implementace - vertex buffer s rectangly + glyph atlas pro text.
/// Display list (paint::DisplayCommand) -> vertex data -> GPU.

use super::paint::DisplayCommand;
use super::devtools_panel::{paint_devtools_panel, devtools_hit_test, pick_node_at_screen_pos};
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

#[derive(Debug, Clone)]
pub struct BookmarkPickerState {
    pub url: String,
    pub title_buffer: String,
    pub folder_buffer: String,
    pub focus: BookmarkPickerFocus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BookmarkPickerFocus { Title, Folder }

/// Reading mode CSS - injected pri Ctrl+Alt+R toggle. Schova
/// nav/aside/footer, centrovani main do max 720px, beige bg, vetsi serif.
const READING_MODE_CSS: &str = r#"
body { background: #f4ecd8 !important; color: #2a2520 !important; }
nav, aside, footer, header.global, .sidebar, .ad, .advertisement, .banner, .cookie { display: none !important; }
main, article, [role=main], .content, .post, .entry { max-width: 720px !important; margin: 0 auto !important; padding: 32px 24px !important; background: #fbf6e9 !important; }
body * { font-family: Georgia, serif !important; }
p, li, blockquote { line-height: 1.7 !important; font-size: 18px !important; }
h1 { font-size: 32px !important; }
h2 { font-size: 24px !important; }
h3 { font-size: 20px !important; }
img, video { max-width: 100% !important; height: auto !important; }
a { color: #2a4d8f !important; }
"#;

// Async worker pro JS exec vyzaduje Interpreter: Send. Aktualne Interpreter ma
// Rc<RefCell> interne, takze !Send. Wrappers `unsafe impl Send for SendInterp`
// nestaci protoze closure auto-trait check projde dovnitr Rc pres autoderef.
// Reseni: Arc<Mutex> rework napric ~30 souboru (Interpreter struct, JsValue,
// JsObject, NodeData, Document, atd.) - viz HANDOFF Arc rework TODO.
// Aktualne: shared_debugger + Continue Condvar foundation pripravena, ale
// scripts beti single-thread (UI). Pause = early-abort + rerun kompromis.

mod url;
pub use url::{fetch_text_url, fetch_image_bytes, resolve_url};

mod forms;
use forms::{find_ancestor_form, build_form_request, post_form};

mod dirty;
pub use dirty::DirtyRegion;

mod segments;
pub use segments::{Seg, partition_filter_segments};
use segments::{shift_command_y, shift_command_x};

mod polygon;
#[allow(unused_imports)] // pub use - test exposure (cargo build je nevidi)
pub use polygon::{polygon_signed_area, triangulate_polygon};

mod atlas;
pub use atlas::{try_load_default_font, ImageAtlas};
use atlas::{GlyphAtlas, ATLAS_SIZE, IMAGE_ATLAS_SIZE};

mod shaders;
use shaders::{BLUR_SHADER, TRANSFORM_SHADER, COMPOSE_SHADER, RECT_SHADER, LCD_SHADER};

mod primitives;
use primitives::{push_rect, push_rect_rounded, push_rect_uv, push_skewed_quad,
    push_triangle, push_polygon_edge_aa, push_blurred_rect, push_image, push_gradient,
    push_radial_gradient, push_conic_gradient, push_multi_stop_linear_gradient,
    push_multi_stop_radial_gradient, push_multi_stop_conic_gradient,
    push_shadow, push_inset_shadow, normalize_color};

mod canvas_paint;
use canvas_paint::paint_canvas_ops;

mod webgl_paint;
mod text_input;
pub mod tabs;
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
fn extract_text_runs(
    commands: &[DisplayCommand],
    atlas: &GlyphAtlas,
    zoom: f32,
) -> Vec<crate::browser::textrun::TextRun> {
    let mut runs = Vec::new();
    for cmd in commands {
        if let DisplayCommand::Text { x, y, content, font_size, bold, italic, font_family, .. } = cmd {
            let z = zoom.max(0.0001);
            let physical_size = (*font_size * z).round().max(1.0) as u32;
            let inv_z = 1.0 / z;
            let lookup_family = match (*bold, *italic) {
                (true, true) if atlas.font_bold_italic.is_some() =>
                    format!("__bi__:{}", font_family),
                (false, true) if atlas.font_italic.is_some() =>
                    format!("__italic__:{}", font_family),
                (true, _) if atlas.font_bold.is_some() =>
                    format!("__bold__:{}", font_family),
                _ => font_family.clone(),
            };
            let mut cumulative: Vec<f32> = Vec::with_capacity(content.chars().count() + 1);
            cumulative.push(0.0);
            let mut acc = 0.0;
            for ch in content.chars() {
                let advance = atlas.get(&lookup_family, ch, physical_size)
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

fn build_vertices(commands: &[DisplayCommand], atlas: &GlyphAtlas, image_atlas: &ImageAtlas, zoom: f32) -> Vec<Vertex> {
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
            DisplayCommand::Text { x, y, content, color, font_size, bold, italic, font_family, strikethrough, underline } => {
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
                let z = zoom.max(0.0001);
                let physical_size = (*font_size * z).round().max(1.0) as u32;
                let inv_z = 1.0 / z;
                for ch in content.chars() {
                    if ch == '\n' {
                        pen_y += line_advance;
                        pen_x = start_x;
                        continue;
                    }
                    let colr_key = format!("__colr:{}:{}:{}", font_family, ch as u32, *font_size as u32);
                    if let Some(info) = image_atlas.get(&colr_key) {
                        let gx = pen_x.round();
                        let gy = (pen_y - info.height).round();
                        push_image(&mut verts, gx, gy, info.width, info.height, info.uv0, info.uv1, 0.0);
                        pen_x += info.width;
                        continue;
                    }
                    let lookup_family = match (*bold, *italic) {
                        (true, true) if atlas.font_bold_italic.is_some() =>
                            format!("__bi__:{}", font_family),
                        (false, true) if atlas.font_italic.is_some() =>
                            format!("__italic__:{}", font_family),
                        (true, _) if atlas.font_bold.is_some() =>
                            format!("__bold__:{}", font_family),
                        _ => font_family.clone(),
                    };
                    if let Some(g) = atlas.get(&lookup_family, ch, physical_size) {
                        // Glyf metrics v physical -> dele inv_z na logical.
                        let g_w = g.width * inv_z;
                        let g_h = g.height * inv_z;
                        let g_bx = g.bearing_x * inv_z;
                        let g_by = g.bearing_y * inv_z;
                        let g_adv = g.advance * inv_z;
                        let gx_raw = pen_x + g_bx;
                        let gy_raw = pen_y - g_by;
                        // Round na logical-px hranici (pri zoom=1 = integer phys);
                        // pri zoomu > 1 je krok jemnejsi (1/zoom logical px = 1 phys).
                        let gx = (gx_raw * z).round() * inv_z;
                        let gy = (gy_raw * z).round() * inv_z;
                        // Mode 9 = LCD subpixel text (3-tap shader sample), 1 = grayscale.
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
                        pen_x += g_adv + bold_offset * inv_z;
                    } else {
                        pen_x += font_size * 0.5;
                    }
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
                            if stops_f.len() > 2 {
                                push_multi_stop_linear_gradient(&mut verts, *x, *y, *w, *h, *angle_deg, &stops_f, *radius);
                            } else {
                                push_gradient(&mut verts, *x, *y, *w, *h, *angle_deg, c0, c1, *radius);
                            }
                        }
                        GradientKind::Radial { cx, cy, radius: grad_r } => {
                            if stops_f.len() > 2 {
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
            | DisplayCommand::MaskBegin { .. } | DisplayCommand::MaskEnd => {
                // Markers - zpracovava se v render flow, ne ve vertex builderu.
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
        }
    }
    verts
}


/// Najdi DOM node v stromu podle Rc::as_ptr hodnoty (use ve cascade).
/// Aplikuje animation/transition hodnoty z style_map na cached layout boxes.
/// Pouziva se kdyz cache je valid pro layout struktury, ale paint props
/// (transform/opacity/color/filter) se menily kazdy frame pres animations.
fn apply_paint_animations(box_: &mut crate::browser::layout::LayoutBox,
                           style_map: &crate::browser::cascade::StyleMap) {
    apply_paint_animations_inner(box_, style_map, 0.0);
}

fn apply_paint_animations_inner(box_: &mut crate::browser::layout::LayoutBox,
                                 style_map: &crate::browser::cascade::StyleMap,
                                 parent_width: f32) {
    let node_id = box_.node.as_ref().map(|n| Rc::as_ptr(n) as usize).unwrap_or(0);
    let original_width = box_.rect.width;
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
        if let Some(c) = styles.get("background-color") {
            if let Some(rgb) = crate::browser::layout::parse_color(c) {
                box_.bg_color = Some(rgb);
            }
        }
        if let Some(t) = styles.get("transform") {
            box_.transforms = crate::browser::layout::parse_transform_chain(t);
        }
        if let Some(f) = styles.get("filter") {
            box_.filter = crate::browser::layout::parse_filter_chain(f);
        }
        // INCREMENTAL LAYOUT: aplikuj animovanou width/height na rect kdyz
        // element ma overflow:hidden (self-contained, ne reflow). Drive
        // typewriter potreboval full layout rebuild kazdy frame, ted jen
        // pricte rect.width upravu.
        let oh_x = box_.overflow_x.as_str() == "hidden" || box_.overflow_x.as_str() == "clip";
        let oh_y = box_.overflow_y.as_str() == "hidden" || box_.overflow_y.as_str() == "clip";
        if oh_x || oh_y {
            // Pouzijeme cached parent_width pro % rozeznavani.
            // Width animace: aktualizuj rect.width pri overflow-x:hidden.
            if oh_x {
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
            if oh_y {
                if let Some(h) = styles.get("height") {
                    let trimmed = h.trim();
                    if let Some(pct_str) = trimmed.strip_suffix('%') {
                        if let Ok(pct) = pct_str.parse::<f32>() {
                            // Parent width fallback - real chrome by pouzilo parent height
                            // ale pro typewriter case staci. Pro vetsi pres potreba
                            // dalsi argument.
                            if parent_width > 0.0 {
                                box_.rect.height = parent_width * (pct / 100.0);
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
    for ch in &mut box_.children {
        apply_paint_animations_inner(ch, style_map, our_width);
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

fn find_node_by_ptr(root: &Rc<crate::browser::dom::NodeData>, ptr: usize) -> Option<Rc<crate::browser::dom::NodeData>> {
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
pub fn run_window_with_shell(html: String, css: String, current_html_path: Option<std::path::PathBuf>, auto_devtools: bool, base_url: Option<String>) -> Result<(), String> {
    run_window_inner(html, css, current_html_path, auto_devtools, base_url, true)
}

pub fn run_window_with_options(html: String, css: String, current_html_path: Option<std::path::PathBuf>, auto_devtools: bool, base_url: Option<String>) -> Result<(), String> {
    run_window_inner(html, css, current_html_path, auto_devtools, base_url, false)
}

fn run_window_inner(html: String, css: String, current_html_path: Option<std::path::PathBuf>, auto_devtools: bool, base_url: Option<String>, shell_mode: bool) -> Result<(), String> {
    use winit::application::ApplicationHandler;
    use winit::event::{WindowEvent, MouseButton, ElementState};
    use winit::event_loop::ActiveEventLoop;
    use winit::window::{Window, WindowId};
    use winit::keyboard::{Key, NamedKey};

    struct App {
        html: String,
        css: String,
        /// Cache parsed stylesheets (css string hash -> Vec<Stylesheet>).
        cached_stylesheets_hash: u64,
        cached_stylesheets: Option<Vec<super::css_parser::Stylesheet>>,
        /// Reuse display list buffer napric frames (alloc-free).
        display_list_buffer: Vec<super::paint::DisplayCommand>,
        /// Cached layout_root - reuse pri ne-layout-affecting animations.
        cached_layout_root: Option<super::layout::LayoutBox>,
        /// True kdyz animations modify layout-affecting props.
        animations_affect_layout: bool,
        /// Zda current CSS pouziva :hover / :focus selektory. Pokud ne, hover
        /// change neinvaliduje cascade cache.
        css_uses_hover: bool,
        css_uses_focus: bool,
        /// CSS pouziva @media/@container queries, takze viewport zmena ovlivni
        /// cascade. Bez teto detekce by se cascade rebuilovala pri kazdem
        /// pixelu resize -> resize lag.
        css_uses_viewport: bool,
        /// CSS obsahuje "transition" property - pak je potreba prev_style_map
        /// pro detekci transitions vs current. Bez "transition" v CSS preskakujeme
        /// drahy clone style_map kazdy frame.
        css_uses_transitions: bool,
        /// CSS obsahuje "@keyframes" - pak runtime animations rotation. Bez nich
        /// preskakujeme apply_animations + clone.
        css_uses_keyframes: bool,
        /// Layout obsahuje position: sticky elementy - pak apply_sticky musi mit
        /// mutable layout_root. Bez sticky preskakujeme clone cached_layout_root.
        layout_has_sticky: bool,
        /// Set jmen keyframes ktere ovlivnuji layout-affecting props (width/height/
        /// margin/padding/...). Cache invalidate jen kdyz aktivni animace ma name
        /// v tomto set. Drive: jakekoliv layout-prop @keyframes v CSS = perma
        /// invalidace, i kdyz se nikdy nespusti.
        layout_affecting_animations: std::collections::HashSet<String>,
        /// Subset layout_affecting_animations: jmena ktera animuji POUZE
        /// width/height (zadne jine layout-props). Tyto muzu aplikovat primo
        /// na rect.width/height (Chrome-style "self-contained layout") kdyz
        /// element ma overflow:hidden - width change nezpusobi reflow.
        /// Klic optimalizace pro typewriter (overflow:hidden + anim width).
        width_height_only_animations: std::collections::HashSet<String>,
        /// Ring buffer poslednich N frame timing pro FPS counter overlay.
        /// Default 60 frame window. Render hori v ms, FPS = 1000 / avg.
        frame_times_ms: std::collections::VecDeque<f32>,
        /// Show FPS counter overlay (Ctrl+Shift+F nebo always-on dev mode).
        show_fps: bool,
        /// Cache cascade output (DOM root ptr hash -> StyleMap).
        /// Rc<StyleMap> aby per-frame Rc::clone byl cheap (jen pointer + atomic
        /// counter). Drive HashMap clone = ~1 ms na 5000 nodu kazdy frame.
        /// Mutace via Rc::make_mut (clones if shared) - jen kdyz animations
        /// realne meni styly.
        cached_cascade_hash: u64,
        cached_style_map: Option<Rc<super::cascade::StyleMap>>,
        /// Cache pro matched_rules build - (selected_node, cascade_hash).
        /// Bez zmeny ani jednoho neni potreba pretizet rules walk.
        cached_matched_key: Option<(Option<usize>, u64)>,
        /// Animations time origin - drahy speed/pause/restart pres devtools panel.
        /// Effective_anim_time = (now - origin) * speed; pri pause snapshot do paused_at.
        animation_origin: std::time::Instant,
        animation_pause_start: Option<std::time::Instant>,
        /// Per-element pause - selected nodes maji animace zamrzly v presne
        /// fazi pri toggle. Pause v Animations panelu (s vybranym elementem)
        /// snapshot animated style v ten moment + restore kazdy frame.
        paused_animation_nodes: std::collections::HashSet<usize>,
        /// Per-node frozen style snapshot - pri pause toggle uchova soucasny
        /// (animated) styl, kazdy frame se aplikuje misto fresh anim tick.
        paused_node_styles: std::collections::HashMap<usize, std::collections::HashMap<String, String>>,
        /// Drag timeline scrubber state - pri MouseDown na track v Animations
        /// panelu zacne drag. Pri pohybu mysi se animation_origin shifte tak
        /// aby progress odpovidal pozici kursoru na track.
        animations_scrubber_drag: bool,
        /// Per-frame painted TextRun pole - foundation pro per-glyph selection.
        /// Naplnuje se po build_vertices, pouzivame pro hit-test (mouse -> SelectionPos).
        /// Aktualne flow-extract zustava authoritative pro copy; postupny prechod
        /// na per-glyph anchor/focus.
        painted_text_runs: Vec<crate::browser::textrun::TextRun>,
        /// Async jobs registry - background work (file IO, image lazy load).
        /// Drain per frame; on_complete callbacks beha v main thread (mohou
        /// modifikovat Interpreter pres Rc<RefCell>).
        async_jobs: crate::browser::async_jobs::AsyncJobsRegistry,
        /// Bookmark picker (Ctrl+D popup): edit title + folder.
        bookmark_picker: Option<BookmarkPickerState>,
        cached_pseudo_map: Option<super::cascade::PseudoStyleMap>,
        /// Cesta k aktualne nactenemu HTML souboru (pro reload + relativni paths).
        current_path: Option<std::path::PathBuf>,
        /// Page URL (http(s)://... nebo file:///...) pro relative URL resolution.
        base_url: Option<String>,
        /// Browser history: stack URLs + aktualni index.
        /// Push pri navigate. Alt+Left = back (idx-=1), Alt+Right = forward (idx+=1).
        history: Vec<String>,
        history_idx: usize,
        /// Otevreny <select> dropdown - hodnota = (node ptr, anchor x/y/w).
        open_select: Option<(usize, f32, f32, f32)>,
        /// Po startu otevri devtools.html v default browseru.
        auto_devtools: bool,
        window: Option<std::sync::Arc<Window>>,
        renderer: Option<Renderer>,
        layout_root: Option<super::layout::LayoutBox>,
        interpreter: Option<crate::interpreter::Interpreter>,
        mouse_x: f32,
        mouse_y: f32,
        scroll_y: f32,
        scroll_x: f32,
        start_time: std::time::Instant,
        /// Predchozi cascaded styles - pro detekci transitions.
        prev_style_map: Option<Rc<super::cascade::StyleMap>>,
        /// Track running animations per (node_id, anim_name) - pro dispatch animationstart/end
        active_animations: std::collections::HashSet<(usize, String)>,
        /// Iteration counter per animation pro animationiteration event.
        animation_iterations: std::collections::HashMap<(usize, String), i32>,
        /// Aktivni CSS transitions.
        active_transitions: Vec<super::cascade::ActiveTransition>,
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
        /// Browser zoom factor (1.0 = 100%). Ctrl++/Ctrl+- meni v krocich,
        /// Ctrl+0 reset. Layout viewport pri zoomu = window/zoom (tj. logical
        /// dimensions mensi -> reflow). Render uniform vp = window/zoom -> px
        /// padaji do scaled NDC. Glyf rasterization ale na zoom = blur, ale
        /// browsersko-funkcni.
        zoom: f32,
        /// Trackovany state Ctrl/Shift/Alt pro zoom shortcut detection.
        modifiers: winit::keyboard::ModifiersState,
        /// Find-on-page (Ctrl+F): otevreny overlay + query + matches.
        find_open: bool,
        find_query: crate::devtools::model::text_buffer::SimpleStringBuffer,
        find_match_idx: usize,
        /// Address bar (Ctrl+L): toggleable input overlay. Enter navigate.
        addr_open: bool,
        addr_input: crate::devtools::model::text_buffer::SimpleStringBuffer,
        /// Smooth scroll target. Render tick interpoluje scroll_y -> target.
        scroll_target_y: f32,
        scroll_target_x: f32,
        /// Text selection: anchor (mouse down pos), current (mouse drag pos).
        /// Pri obou Some + dragging = aktivni rect highlight. Ctrl+C extrahuje
        /// text uvnitr.
        /// Main page scrollbar drag - true pri LMB hold na vertical/horizontal thumb.
        page_scrollbar_v_drag: bool,
        page_scrollbar_h_drag: bool,
        /// Tab drag reorder state - Some(idx) pri probihajici drag.
        tab_drag_idx: Option<usize>,
        tab_drag_x_start: f32,
        /// Status bar hover URL preview (Some = link hover, None = hidden).
        status_hover_url: Option<String>,
        /// Tooltip nad shell tab chip (full title) - (text, x, y screen logical).
        shell_tab_tooltip: Option<(String, f32, f32)>,
        /// Hover state pro tab tooltip - (tab_idx, x, y, hover_start).
        /// Tooltip se zobrazi az kdyz hover trva > 500ms na stejnem tabu.
        shell_tab_hover_pending: Option<(usize, f32, f32, std::time::Instant)>,
        /// F1 toggle: keyboard shortcuts cheat sheet overlay.
        shortcuts_overlay_open: bool,
        /// Reading mode (Ctrl+Alt+R): inject zen-style CSS - hide nav/sidebar/footer,
        /// center main, beige bg, vetsi serif text.
        reading_mode_on: bool,
        /// Bookmarks bar visibility (Ctrl+Shift+B toggle). Default true.
        bookmarks_bar_visible: bool,
        /// Browser shell mode - kdyz true, vykresli se chrome bar (tabs +
        /// address bar + back/forward) + page area zacne pod chromem.
        /// Toggle pres CLI flag --shell nebo Ctrl+Shift+B.
        shell_mode: bool,
        /// Chrome bar vyska (tab bar + nav bar).
        shell_chrome_h: f32,
        /// Multi-tab manager (shell mode). Active tab drives App.html/css/...
        /// Switch tab = swap active state via tabs.switch_to + reload.
        tabs: tabs::TabManager,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let title = match &self.current_path {
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

            // Vytvor interpreter + nacti HTML do jeho document
            let mut interp = crate::interpreter::Interpreter::new();
            let url = match &self.current_path {
                Some(p) => format!("file:///{}", p.display().to_string().replace('\\', "/")),
                None => "about:blank".to_string(),
            };
            let doc = super::html_parser::parse_html(&self.html, &url);
            interp.set_document(doc);

            // Spust JS uvnitr <script> tagu
            self.run_inline_scripts(&mut interp);

            self.interpreter = Some(interp);
            self.render();

            // Auto-open devtools.html po startu
            if self.auto_devtools {
                self.regenerate_and_open_devtools();
            }

            println!("[okno] F12 = otevri/regen DevTools | drag-drop HTML soubor pro reload");
            window.request_redraw();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    // Save session pri shell mode quit.
                    if self.shell_mode {
                        // Save current tab state pred snapshot.
                        {
                            let cur = self.tabs.active_tab_mut();
                            cur.scroll_y = self.scroll_y;
                            cur.scroll_x = self.scroll_x;
                            cur.url = self.base_url.clone();
                        }
                        let session = crate::devtools::session::Session {
                            tabs: self.tabs.tabs.iter().map(|t|
                                crate::devtools::session::SessionTab {
                                    url: t.url.clone(),
                                    title: t.title.clone(),
                                }).collect(),
                            active: self.tabs.active,
                        };
                        crate::devtools::session::save_session(&session);
                    }
                    event_loop.exit();
                }
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        if let Some(w) = &self.window {
                            r.scale_factor = w.scale_factor() as f32;
                        }
                        r.resize(size.width.max(1), size.height.max(1));
                    }
                    // PERF: layout cache invalidate jen pri viewport-dependent CSS.
                    // Bez @media/vh layout je viewport-independent (content size
                    // urcen z elements, ne window) -> kesovany layout zustava
                    // valid pri resize, render jen prepocita scrollbars + shifts.
                    if self.css_uses_viewport {
                        self.cached_layout_root = None;
                    }
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
                    let new_x = (position.x as f32) / (self.zoom * scale) + self.scroll_x;
                    let new_y = (position.y as f32) / (self.zoom * scale) + self.scroll_y;
                    // Skip update kdyz se pozice nezmenila (deduplicate winit spam).
                    if (new_x - self.mouse_x).abs() < 0.5 && (new_y - self.mouse_y).abs() < 0.5 {
                        return;
                    }
                    self.mouse_x = new_x;
                    self.mouse_y = new_y;
                    // Tab drag reorder.
                    if let Some(drag_idx) = self.tab_drag_idx {
                        if self.shell_mode {
                            let viewport_w = self.viewport_w_logical();
                            let n = self.tabs.tabs.len();
                            let tab_w = 200.0_f32.min((viewport_w - 60.0) / (n as f32).max(1.0));
                            let target = ((self.mouse_x - 4.0) / (tab_w + 2.0)).max(0.0) as usize;
                            let target = target.min(n.saturating_sub(1));
                            if target != drag_idx {
                                let tab = self.tabs.tabs.remove(drag_idx);
                                self.tabs.tabs.insert(target, tab);
                                if self.tabs.active == drag_idx {
                                    self.tabs.active = target;
                                } else if drag_idx < self.tabs.active && target >= self.tabs.active {
                                    self.tabs.active -= 1;
                                } else if drag_idx > self.tabs.active && target <= self.tabs.active {
                                    self.tabs.active += 1;
                                }
                                self.tab_drag_idx = Some(target);
                            }
                            self.render();
                            return;
                        }
                    }
                    // Resize drag: aktualizuj panel size dle dock position.
                    if self.devtools_resizing {
                        use crate::devtools::profile::DockPosition;
                        let viewport_w = self.viewport_w_logical();
                        let viewport_h = self.viewport_h_logical();
                        let raw_x = new_x - self.scroll_x;
                        let raw_y = new_y - self.scroll_y;
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
                    // Animations scrubber drag - update progress podle mouse_x.
                    if self.animations_scrubber_drag {
                        let bz = self.devtools.styles.animations_btn_zones.borrow();
                        for (zx, zy, zw, zh, action) in bz.iter() {
                            if action == "scrub" {
                                let local_mx = self.mouse_x;
                                let local_my = self.mouse_y - self.scroll_y;
                                if local_my >= *zy - 10.0 && local_my < zy + zh + 10.0 {
                                    let progress = ((local_mx - zx) / zw).clamp(0.0, 1.0);
                                    drop(bz);
                                    use std::time::{Instant, Duration};
                                    let dur = self.devtools.styles.computed.iter()
                                        .find(|(k, _)| k == "animation-duration")
                                        .and_then(|(_, v)| {
                                            let s = v.trim();
                                            if let Some(n) = s.strip_suffix("ms") {
                                                n.parse::<f32>().ok().map(|x| x / 1000.0)
                                            } else if let Some(n) = s.strip_suffix('s') {
                                                n.parse::<f32>().ok()
                                            } else { s.parse::<f32>().ok() }
                                        }).unwrap_or(1.0).max(0.1);
                                    let target_t = progress * dur;
                                    let speed = self.devtools.animations_speed.max(0.01);
                                    let now = Instant::now();
                                    self.animation_origin = now - Duration::from_secs_f32((target_t / speed).max(0.0));
                                    if self.devtools.animations_paused {
                                        self.animation_pause_start = Some(now);
                                    }
                                    self.paused_node_styles.clear();
                                    self.render();
                                    return;
                                }
                                break;
                            }
                        }
                    }
                    // Main page scrollbar drag.
                    if self.page_scrollbar_v_drag || self.page_scrollbar_h_drag {
                        if let Some(layout) = &self.layout_root {
                            let viewport_w = self.viewport_w_logical();
                            let viewport_h = self.viewport_h_logical() - self.panel_h_logical();
                            if self.page_scrollbar_v_drag && layout.rect.height > viewport_h {
                                let max_scroll = (layout.rect.height - viewport_h).max(1.0);
                                let thumb_h = (viewport_h * viewport_h / layout.rect.height).max(40.0);
                                let my_screen = self.mouse_y - self.scroll_y;
                                let frac = ((my_screen - thumb_h * 0.5) / (viewport_h - thumb_h)).clamp(0.0, 1.0);
                                // Drag scrollbar = direct (bez smooth interp). User chce
                                // immediate response; smooth lerp by zpomalil drag.
                                let target = frac * max_scroll;
                                self.scroll_target_y = target;
                                self.scroll_y = target;
                            }
                            if self.page_scrollbar_h_drag && layout.rect.width > viewport_w {
                                let max_scroll = (layout.rect.width - viewport_w).max(1.0);
                                let thumb_w = (viewport_w * viewport_w / layout.rect.width).max(40.0);
                                let frac = ((self.mouse_x - thumb_w * 0.5) / (viewport_w - thumb_w)).clamp(0.0, 1.0);
                                let target = frac * max_scroll;
                                self.scroll_target_x = target;
                                self.scroll_x = target;
                            }
                        }
                        self.render();
                        return;
                    }
                    // Splitter drag: aktualizuj split_x v logical px.
                    if self.devtools.elements.dragging_split {
                        let viewport_w = self.viewport_w_logical();
                        let max_x = viewport_w - self.devtools.side_panel_w - 200.0;
                        self.devtools.elements.split_x = (self.mouse_x - self.scroll_x).clamp(200.0, max_x);
                        self.render();
                        return;
                    }
                    // Side panel splitter drag (per dock: convert mouse pos
                    // do panel-local coords).
                    if self.devtools.elements.dragging_side_split {
                        use crate::devtools::profile::DockPosition;
                        let viewport_w = self.viewport_w_logical();
                        let mx_screen = self.mouse_x - self.scroll_x;
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
                        let raw_y = self.mouse_y - self.scroll_y;
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
                    self.update_hover();
                    // Shell tab tooltip - 500ms delay pred zobrazenim. Hover na
                    // jiny tab nebo mimo chrome resetuje pending. Pri hover_start
                    // staci > 500ms -> tooltip aktivni.
                    self.shell_tab_tooltip = None;
                    if self.shell_mode {
                        let scale_local = self.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
                        let mx = (position.x as f32) / (self.zoom * scale_local);
                        let my = (position.y as f32) / (self.zoom * scale_local);
                        let mut active_idx: Option<usize> = None;
                        if my < self.shell_chrome_h {
                            let viewport_w = self.viewport_w_logical();
                            let hit = hit_chrome(viewport_w, self.shell_chrome_h, &self.tabs, mx, my);
                            if let ChromeHit::TabClick(idx) = hit {
                                active_idx = Some(idx);
                            }
                        }
                        match (active_idx, self.shell_tab_hover_pending.as_ref()) {
                            (Some(idx), Some((prev_idx, _, _, _))) if *prev_idx == idx => {
                                // Stale stejny tab - check elapsed.
                                let elapsed = self.shell_tab_hover_pending.as_ref()
                                    .map(|(_, _, _, t)| t.elapsed()).unwrap();
                                if elapsed >= std::time::Duration::from_millis(500) {
                                    if let Some(t) = self.tabs.tabs.get(idx) {
                                        if t.title.chars().count() > 20 {
                                            self.shell_tab_tooltip = Some((t.title.clone(), mx, my + 16.0));
                                        }
                                    }
                                }
                            }
                            (Some(idx), _) => {
                                // Novy tab - reset hover_start.
                                self.shell_tab_hover_pending = Some((idx, mx, my, std::time::Instant::now()));
                            }
                            (None, _) => {
                                self.shell_tab_hover_pending = None;
                            }
                        }
                    }
                    if self.page_sel_dragging() {
                        self.page_sel_update_current((self.mouse_x, self.mouse_y));
                        self.render();
                    } else if self.open_select.is_some() {
                        self.render();
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
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
                        self.page_scrollbar_v_drag = false;
                        self.page_scrollbar_h_drag = false;
                        self.render();
                    }
                    if self.animations_scrubber_drag {
                        self.animations_scrubber_drag = false;
                    }
                    if self.tab_drag_idx.is_some() {
                        self.tab_drag_idx = None;
                        self.render();
                    }
                    if self.page_sel_dragging() {
                        self.page_sel_end_drag();
                        self.render();
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
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
                    self.page_sel_begin((self.mouse_x, self.mouse_y));
                    // Devtools panel hit-test ma prioritu nad page hit-testem.
                    // mouse_x/y v doc-logical, raw_y v screen-logical. viewport_w/h v logical.
                    let raw_y = self.mouse_y - self.scroll_y;
                    let viewport_w = self.viewport_w_logical();
                    let viewport_h = self.viewport_h_logical();
                    let panel_h = self.panel_h_logical();

                    // Address bar autocomplete suggestion klik.
                    if self.addr_open && !self.addr_input.text.is_empty() {
                        let viewport_w = self.viewport_w_logical();
                        let bar_w = (viewport_w - 80.0).min(800.0);
                        let bar_x = (viewport_w - bar_w) * 0.5;
                        let bar_y = 8.0_f32;
                        let bar_h = 40.0_f32;
                        let popup_y = bar_y + bar_h + 4.0;
                        let item_h = 28.0_f32;
                        let mx = self.mouse_x;
                        let my_screen = self.mouse_y - self.scroll_y;
                        if mx >= bar_x && mx < bar_x + bar_w && my_screen >= popup_y {
                            let q = self.addr_input.text.to_lowercase();
                            let history = crate::devtools::history::load_history();
                            let bookmarks = crate::devtools::bookmarks::load_bookmarks();
                            let mut suggest: Vec<String> = Vec::new();
                            for b in &bookmarks {
                                if b.url.to_lowercase().contains(&q) || b.title.to_lowercase().contains(&q) {
                                    suggest.push(b.url.clone());
                                    if suggest.len() >= 8 { break; }
                                }
                            }
                            for h in history.iter().rev() {
                                if suggest.len() >= 8 { break; }
                                if suggest.contains(&h.url) { continue; }
                                if h.url.to_lowercase().contains(&q) || h.title.to_lowercase().contains(&q) {
                                    suggest.push(h.url.clone());
                                }
                            }
                            let idx = ((my_screen - popup_y) / item_h) as usize;
                            if let Some(url) = suggest.get(idx).cloned() {
                                self.addr_open = false;
                                self.addr_input.clear();
                                self.navigate_url(&url);
                                return;
                            }
                        }
                    }
                    // Scroll-to-top button hit (pravy dolni roh, jen pri scroll_y > 200).
                    if self.shell_mode && self.scroll_y > 200.0 {
                        let viewport_w = self.viewport_w_logical();
                        let viewport_h = self.viewport_h_logical();
                        let panel_h = self.panel_h_logical();
                        let btn_x = viewport_w - 40.0;
                        let btn_y = viewport_h - panel_h - 50.0;
                        let mx = self.mouse_x;
                        let my_screen = self.mouse_y - self.scroll_y;
                        if mx >= btn_x && mx < btn_x + 32.0
                           && my_screen >= btn_y && my_screen < btn_y + 32.0 {
                            self.scroll_target_y = 0.0;
                            self.render();
                            return;
                        }
                    }
                    // Shell chrome hit-test (priorita nad page).
                    if self.shell_mode {
                        let viewport_w = self.viewport_w_logical();
                        let mx = self.mouse_x;
                        let my_screen = self.mouse_y - self.scroll_y;
                        // Reading mode badge click - toggle.
                        if self.reading_mode_on && my_screen < 28.0 {
                            let bx = viewport_w - 200.0;
                            if mx >= bx && mx <= bx + 100.0 && my_screen >= 4.0 && my_screen <= 24.0 {
                                self.reading_mode_on = false;
                                self.cached_stylesheets = None;
                                self.cached_style_map = None;
                                self.cached_pseudo_map = None;
                                self.cached_layout_root = None;
                                self.render();
                                return;
                            }
                        }
                        if my_screen < self.shell_chrome_h {
                            let hit = hit_chrome(viewport_w, self.shell_chrome_h, &self.tabs, mx, my_screen);
                            match hit {
                                ChromeHit::TabClick(idx) => {
                                    // Initiate drag tracking + switch.
                                    self.tab_drag_idx = Some(idx);
                                    self.tab_drag_x_start = self.mouse_x;
                                    // Save current tab state pred switch.
                                    {
                                        let cur = self.tabs.active_tab_mut();
                                        cur.scroll_y = self.scroll_y;
                                        cur.scroll_x = self.scroll_x;
                                        cur.html = self.html.clone();
                                        cur.css = self.css.clone();
                                        cur.url = self.base_url.clone();
                                    }
                                    self.switch_tab_with_swap(idx);
                                    let t = self.tabs.active_tab().clone();
                                    self.html = t.html;
                                    self.css = t.css;
                                    self.base_url = t.url;
                                    self.cached_layout_root = None;
                                    self.cached_stylesheets = None;
                                    self.scroll_y = t.scroll_y;
                                    self.scroll_x = t.scroll_x;
                                    self.render();
                                    return;
                                }
                                ChromeHit::TabClose(idx) => {
                                    self.tabs.close(idx);
                                    let t = self.tabs.active_tab().clone();
                                    self.html = t.html;
                                    self.css = t.css;
                                    self.base_url = t.url;
                                    self.cached_layout_root = None;
                                    self.cached_stylesheets = None;
                                    self.render();
                                    return;
                                }
                                ChromeHit::NewTab => {
                                    // Save current tab state pred open.
                                    {
                                        let cur = self.tabs.active_tab_mut();
                                        cur.scroll_y = self.scroll_y;
                                        cur.scroll_x = self.scroll_x;
                                        cur.html = self.html.clone();
                                        cur.css = self.css.clone();
                                        cur.url = self.base_url.clone();
                                    }
                                    self.tabs.open(crate::browser::render::tabs::Tab::empty());
                                    let t = self.tabs.active_tab().clone();
                                    self.html = t.html;
                                    self.css = t.css;
                                    self.base_url = t.url;
                                    self.scroll_y = t.scroll_y;
                                    self.scroll_x = t.scroll_x;
                                    self.cached_layout_root = None;
                                    self.cached_stylesheets = None;
                                    self.render();
                                    return;
                                }
                                ChromeHit::Back => {
                                    if self.history_idx > 0 {
                                        self.history_idx -= 1;
                                        let url = self.history[self.history_idx].clone();
                                        self.navigate_url_no_history(&url);
                                    }
                                    return;
                                }
                                ChromeHit::Forward => {
                                    if self.history_idx + 1 < self.history.len() {
                                        self.history_idx += 1;
                                        let url = self.history[self.history_idx].clone();
                                        self.navigate_url_no_history(&url);
                                    }
                                    return;
                                }
                                ChromeHit::Reload => {
                                    if let Some(u) = self.base_url.clone() {
                                        self.navigate_url_no_history(&u);
                                    }
                                    return;
                                }
                                ChromeHit::UrlBar => {
                                    self.addr_open = true;
                                    self.addr_input = crate::devtools::model::text_buffer::SimpleStringBuffer::with_text_selected(self.base_url.clone().unwrap_or_default());
                                    self.render();
                                    return;
                                }
                                ChromeHit::BookmarkClick(url) => {
                                    self.navigate_url(&url);
                                    return;
                                }
                                ChromeHit::DevtoolsToggle => {
                                    self.devtools.panel_open = !self.devtools.panel_open;
                                    self.render();
                                    return;
                                }
                                ChromeHit::BookmarkStar => {
                                    if let Some(url) = self.base_url.clone() {
                                        let bms = crate::devtools::bookmarks::load_bookmarks();
                                        if bms.iter().any(|b| b.url == url) {
                                            crate::devtools::bookmarks::remove_bookmark(&url);
                                        } else {
                                            let title = url.split('/').last().unwrap_or(&url).to_string();
                                            crate::devtools::bookmarks::add_bookmark(&url, &title);
                                        }
                                        self.render();
                                    }
                                    return;
                                }
                                ChromeHit::TabContextMenu(_) | ChromeHit::BookmarkContextMenu(_) => {
                                    // RMB only - LMB ignoruje.
                                }
                                ChromeHit::None => {}
                            }
                        }
                    }

                    // Main page scrollbar hit-test (priorita nad page click).
                    // Pozn: scrollbar je shifted by shift_command_x(-scroll_x), takze
                    // visible position = bar_x - scroll_x. Mouse_x ma scroll_x baked-in
                    // (viz CursorMoved), takze srovnani s bar_x funguje primo.
                    // Layout/scrollbar plati pro page area = viewport bez panelu.
                    if let Some(layout) = &self.layout_root {
                        let viewport_h_page = viewport_h - panel_h;
                        let mx = self.mouse_x;
                        let my_screen = self.mouse_y - self.scroll_y;
                        // Vertikalni scrollbar.
                        if layout.rect.height > viewport_h_page
                           && mx >= viewport_w - 12.0 && mx < viewport_w
                           && my_screen >= 0.0 && my_screen < viewport_h_page {
                            self.page_scrollbar_v_drag = true;
                            let max_scroll = (layout.rect.height - viewport_h_page).max(1.0);
                            let thumb_h = (viewport_h_page * viewport_h_page / layout.rect.height).max(40.0);
                            let frac = ((my_screen - thumb_h * 0.5) / (viewport_h_page - thumb_h)).clamp(0.0, 1.0);
                            self.scroll_target_y = frac * max_scroll;
                            self.page_sel_clear();
                            self.render();
                            return;
                        }
                        // Horizontalni scrollbar.
                        if layout.rect.width > viewport_w
                           && my_screen >= viewport_h_page - 12.0 && my_screen < viewport_h_page
                           && mx >= 0.0 && mx < viewport_w {
                            self.page_scrollbar_h_drag = true;
                            let max_scroll = (layout.rect.width - viewport_w).max(1.0);
                            let thumb_w = (viewport_w * viewport_w / layout.rect.width).max(40.0);
                            let frac = ((mx - thumb_w * 0.5) / (viewport_w - thumb_w)).clamp(0.0, 1.0);
                            self.scroll_target_x = frac * max_scroll;
                            self.page_sel_clear();
                            self.render();
                            return;
                        }
                    }

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
                    let in_panel = self.point_in_devtools(self.mouse_x - self.scroll_x, raw_y);
                    if self.devtools.panel_open && (in_panel || modal_active) {
                        if let Some(layout) = &self.layout_root {
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
                                    if let Some(interp) = &self.interpreter {
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
                                    if let Some(interp) = &self.interpreter {
                                        interp.debugger.borrow_mut().resume();
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                    self.rerun_paused_scripts();
                                }
                                DevtoolsHit::DebuggerStepOver => {
                                    if let Some(interp) = &self.interpreter {
                                        interp.debugger.borrow_mut().start_step(crate::interpreter::StepKind::Over);
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                }
                                DevtoolsHit::DebuggerStepInto => {
                                    if let Some(interp) = &self.interpreter {
                                        interp.debugger.borrow_mut().start_step(crate::interpreter::StepKind::Into);
                                    }
                                    self.devtools.sources.debugger_paused = false;
                                    self.devtools.sources.current_pause_location = None;
                                }
                                DevtoolsHit::DebuggerStepOut => {
                                    if let Some(interp) = &self.interpreter {
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
                                DevtoolsHit::AnimationsScrub(progress) => {
                                    use std::time::{Instant, Duration};
                                    // Scrub: shift animation_origin tak aby effective elapsed
                                    // odpovida progress * duration. Pri pri drag/click on track.
                                    self.animations_scrubber_drag = true;
                                    let dur = self.devtools.styles.computed.iter()
                                        .find(|(k, _)| k == "animation-duration")
                                        .map(|(_, v)| {
                                            let s = v.trim();
                                            if let Some(n) = s.strip_suffix("ms") {
                                                n.parse::<f32>().ok().map(|x| x / 1000.0)
                                            } else if let Some(n) = s.strip_suffix('s') {
                                                n.parse::<f32>().ok()
                                            } else { s.parse::<f32>().ok() }
                                        }).flatten().unwrap_or(1.0).max(0.1);
                                    let target_t = progress * dur;
                                    let speed = self.devtools.animations_speed.max(0.01);
                                    let now = Instant::now();
                                    self.animation_origin = now - Duration::from_secs_f32((target_t / speed).max(0.0));
                                    if self.devtools.animations_paused {
                                        self.animation_pause_start = Some(now);
                                    }
                                    // Pri scrub clear paused snapshots (force re-snapshot na novou fazi).
                                    self.paused_node_styles.clear();
                                }
                                DevtoolsHit::AnimationsAction(action) => {
                                    use std::time::{Instant, Duration};
                                    match action.as_str() {
                                        "pause" => {
                                            // Per-element pause kdyz je vybrany element. Snapshot
                                            // CURRENT (po anim) styl pri toggle - drzeni presne fazi.
                                            if let Some(sel) = self.devtools.elements.selected {
                                                if self.paused_animation_nodes.contains(&sel) {
                                                    self.paused_animation_nodes.remove(&sel);
                                                    self.paused_node_styles.remove(&sel);
                                                } else {
                                                    self.paused_animation_nodes.insert(sel);
                                                    // Snapshot current style_map[sel] z minuleho framu
                                                    // (cached). Bude reused jako frozen.
                                                    if let Some(sm) = &self.cached_style_map {
                                                        if let Some(style) = sm.get(&sel) {
                                                            self.paused_node_styles.insert(sel, style.clone());
                                                        }
                                                    }
                                                }
                                                self.devtools.animations_paused =
                                                    !self.paused_animation_nodes.is_empty();
                                            } else if !self.devtools.animations_paused {
                                                self.animation_pause_start = Some(Instant::now());
                                                self.devtools.animations_paused = true;
                                            } else {
                                                if let Some(ps) = self.animation_pause_start.take() {
                                                    let pause_dur = ps.elapsed();
                                                    self.animation_origin += pause_dur;
                                                }
                                                self.devtools.animations_paused = false;
                                            }
                                        }
                                        "speed" => {
                                            // Cycle 0.25 / 0.5 / 1 / 2 / 4. Snapshot effective_t,
                                            // pak shift origin tak aby novy speed pokracoval z teze hodnoty.
                                            let speeds = [0.25, 0.5, 1.0, 2.0, 4.0];
                                            let cur = self.devtools.animations_speed;
                                            let idx = speeds.iter().position(|s| (s - cur).abs() < 0.01).unwrap_or(2);
                                            let new_speed = speeds[(idx + 1) % speeds.len()];
                                            let now = Instant::now();
                                            let raw_now = if self.devtools.animations_paused {
                                                self.animation_pause_start.unwrap_or(now)
                                            } else { now };
                                            let cur_t = raw_now.duration_since(self.animation_origin)
                                                .as_secs_f32() * cur;
                                            // new_origin: raw_now - cur_t/new_speed.
                                            self.animation_origin = raw_now
                                                - Duration::from_secs_f32((cur_t / new_speed).max(0.0));
                                            self.devtools.animations_speed = new_speed;
                                        }
                                        "restart" => {
                                            self.active_animations.clear();
                                            self.animation_iterations.clear();
                                            self.animation_origin = Instant::now();
                                            if self.devtools.animations_paused {
                                                self.animation_pause_start = Some(Instant::now());
                                            } else {
                                                self.animation_pause_start = None;
                                            }
                                        }
                                        _ => {}
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
                                            self.interpreter.as_ref().and_then(|i| {
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
                                    self.cached_layout_root = None;
                                }
                                DevtoolsHit::ClassManagerToggle => {
                                    self.devtools.class_manager_open = !self.devtools.class_manager_open;
                                }
                                DevtoolsHit::AddNewRule => {
                                    // Pridej prazdny inline style attribut na selected node + open editor.
                                    if let Some(sel_id) = self.devtools.elements.selected {
                                        if let Some(interp) = &self.interpreter {
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
                                                self.cached_layout_root = None;
                                                self.cached_cascade_hash = 0;
                                                println!("[devtools] add rule - inline style updated");
                                            }
                                        }
                                    }
                                }
                                DevtoolsHit::ClassManagerToggleClass(cls) => {
                                    if let Some(sel_id) = self.devtools.elements.selected {
                                        if let Some(interp) = &self.interpreter {
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
                                                self.cached_layout_root = None;
                                                self.cached_cascade_hash = 0;
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
                    if self.devtools.inspect_mode {
                        if let Some(layout) = &self.layout_root {
                            if let Some(node_id) = pick_node_at_screen_pos(layout, self.mouse_x, raw_y, self.scroll_y) {
                                self.devtools.elements.selected = Some(node_id);
                                // Scroll tree pane na vybranou row.
                                if let Some(idx) = self.devtools.elements.rows.iter()
                                    .position(|r| r.node_id == node_id) {
                                    let row_y = idx as f32 * 18.0;
                                    let body_h = panel_h - 4.0 - 30.0;
                                    self.devtools.elements.scroll_y = (row_y - body_h * 0.5).max(0.0);
                                }
                                // Auto-otevri devtools pokud je zavren.
                                if !self.devtools.panel_open {
                                    self.devtools.panel_open = true;
                                    self.devtools.tab = crate::devtools::Tab::Elements;
                                }
                                println!("[inspect] selected node id=0x{:x}", node_id);
                            }
                        }
                        self.devtools.inspect_mode = false;
                        self.render();
                        return;
                    }
                    self.handle_click(self.mouse_x, self.mouse_y);
                    self.render();
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Middle, .. } => {
                    // Middle-click na tab chip = zavrit ten tab.
                    if self.shell_mode {
                        let viewport_w = self.viewport_w_logical();
                        let mx = self.mouse_x;
                        let my_screen = self.mouse_y - self.scroll_y;
                        if my_screen < self.shell_chrome_h {
                            let hit = hit_chrome(viewport_w, self.shell_chrome_h, &self.tabs, mx, my_screen);
                            if let ChromeHit::TabClick(idx) | ChromeHit::TabClose(idx) | ChromeHit::TabContextMenu(idx) = hit {
                                let pinned = self.tabs.tabs.get(idx).map(|t| t.pinned).unwrap_or(false);
                                if !pinned {
                                    self.tabs.close(idx);
                                    let t = self.tabs.active_tab().clone();
                                    self.html = t.html;
                                    self.css = t.css;
                                    self.base_url = t.url;
                                    self.cached_layout_root = None;
                                    self.cached_stylesheets = None;
                                    self.render();
                                }
                            }
                        }
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                    let raw_y = self.mouse_y - self.scroll_y;
                    let viewport_h = self.viewport_h_logical();
                    let panel_h = self.panel_h_logical();
                    // Shell chrome RMB: tab/bookmark context menu.
                    if self.shell_mode && raw_y < self.shell_chrome_h {
                        let viewport_w = self.viewport_w_logical();
                        let mx = self.mouse_x;
                        let hit = hit_chrome(viewport_w, self.shell_chrome_h, &self.tabs, mx, raw_y);
                        use crate::devtools::context_menu::{ContextMenuState, MenuItem, MenuAction};
                        match hit {
                            ChromeHit::TabClick(idx) | ChromeHit::TabClose(idx) | ChromeHit::TabContextMenu(idx) => {
                                let pinned = self.tabs.tabs.get(idx).map(|t| t.pinned).unwrap_or(false);
                                let items = vec![
                                    MenuItem::Action {
                                        label: if pinned { "Odepnout".to_string() } else { "Pripnout".to_string() },
                                        action: MenuAction::TabPinToggle(idx),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Separator,
                                    MenuItem::Action {
                                        label: "Zavrit".to_string(),
                                        action: MenuAction::TabClose(idx),
                                        enabled: !pinned, shortcut: Some("Ctrl+W".to_string()),
                                    },
                                    MenuItem::Action {
                                        label: "Zavrit ostatni".to_string(),
                                        action: MenuAction::TabCloseOthers(idx),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Duplikovat".to_string(),
                                        action: MenuAction::TabDuplicate(idx),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Separator,
                                    MenuItem::Action {
                                        label: "Skupina: Modra".to_string(),
                                        action: MenuAction::TabSetGroup(idx, Some([69, 161, 255, 255])),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Skupina: Zelena".to_string(),
                                        action: MenuAction::TabSetGroup(idx, Some([72, 191, 130, 255])),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Skupina: Zluta".to_string(),
                                        action: MenuAction::TabSetGroup(idx, Some([254, 191, 84, 255])),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Skupina: Cervena".to_string(),
                                        action: MenuAction::TabSetGroup(idx, Some([235, 87, 87, 255])),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Skupina: Fialova".to_string(),
                                        action: MenuAction::TabSetGroup(idx, Some([165, 99, 224, 255])),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Skupina: Bez".to_string(),
                                        action: MenuAction::TabSetGroup(idx, None),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Separator,
                                    MenuItem::Action {
                                        label: "Obnovit".to_string(),
                                        action: MenuAction::TabReload(idx),
                                        enabled: true, shortcut: Some("F5".to_string()),
                                    },
                                ];
                                self.devtools.context_menu = Some(ContextMenuState::new(self.mouse_x, raw_y, items));
                                self.render();
                                return;
                            }
                            ChromeHit::BookmarkClick(url) | ChromeHit::BookmarkContextMenu(url) => {
                                let items = vec![
                                    MenuItem::Action {
                                        label: "Otevrit".to_string(),
                                        action: MenuAction::BookmarkOpen(url.clone()),
                                        enabled: true, shortcut: None,
                                    },
                                    MenuItem::Action {
                                        label: "Smazat".to_string(),
                                        action: MenuAction::BookmarkDelete(url),
                                        enabled: true, shortcut: None,
                                    },
                                ];
                                self.devtools.context_menu = Some(ContextMenuState::new(self.mouse_x, raw_y, items));
                                self.render();
                                return;
                            }
                            _ => {}
                        }
                    }
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
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    // Pri kurzoru nad devtools panelem - scrolluj tree, ne stranku.
                    // mouse_x/y jsou logical (po /(zoom*scale)), srovnani s logical viewport.
                    let raw_y_logical = self.mouse_y - self.scroll_y;
                    let viewport_w = self.viewport_w_logical();
                    if self.point_in_devtools(self.mouse_x - self.scroll_x, raw_y_logical) {
                        let scroll_amount_logical = scroll_amount / (self.zoom * self.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0));
                        // Globalni scroll routing: vzdy ta cast pod kurzorem.
                        // Pri Elements: 3 zony - tree | styles | side_panel.
                        match self.devtools.tab {
                            crate::devtools::Tab::Elements => {
                                let mx_local = self.mouse_x - self.scroll_x;
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
                    let logical_scroll = scroll_amount / (self.zoom * scale).max(0.0001);
                    if self.modifiers.shift_key() {
                        self.scroll_target_x -= logical_scroll;
                        if self.scroll_target_x < 0.0 { self.scroll_target_x = 0.0; }
                    } else {
                        self.scroll_target_y -= logical_scroll;
                        if self.scroll_target_y < 0.0 { self.scroll_target_y = 0.0; }
                    }
                    self.clamp_scroll_to_layout();
                    self.render();
                }
                WindowEvent::RedrawRequested => {
                    self.render();
                    // Continual redraw pri aktivnich animacich/transition NEBO smooth
                    // scroll animation (kdyz scroll_y != scroll_target_y).
                    let has_anim = !self.active_animations.is_empty()
                        || !self.active_transitions.is_empty()
                        || (self.scroll_y - self.scroll_target_y).abs() > 0.5
                        || (self.scroll_x - self.scroll_target_x).abs() > 0.5;
                    if has_anim {
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
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
                    if key_event.state != ElementState::Pressed { return; }
                    // Esc zavre vsechny popupy (color picker / settings / class
                    // manager / tab overflow) pred ostatnim handlingom.
                    if matches!(&key_event.logical_key, Key::Named(NamedKey::Escape)) {
                        if self.handle_escape_close_popups() {
                            self.render();
                            return;
                        }
                    }
                    // Bookmark picker keyboard - typovani title/folder, Tab swap focus.
                    if self.bookmark_picker.is_some() {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(p) = self.bookmark_picker.as_mut() {
                                    match p.focus {
                                        BookmarkPickerFocus::Title => { p.title_buffer.pop(); }
                                        BookmarkPickerFocus::Folder => { p.folder_buffer.pop(); }
                                    }
                                }
                            }
                            Key::Named(NamedKey::Tab) => {
                                if let Some(p) = self.bookmark_picker.as_mut() {
                                    p.focus = match p.focus {
                                        BookmarkPickerFocus::Title => BookmarkPickerFocus::Folder,
                                        BookmarkPickerFocus::Folder => BookmarkPickerFocus::Title,
                                    };
                                }
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some(p) = self.bookmark_picker.take() {
                                    crate::devtools::bookmarks::add_bookmark_in(
                                        &p.url, &p.title_buffer, &p.folder_buffer);
                                    println!("[bookmark] added {} to folder {:?}", p.url, p.folder_buffer);
                                }
                            }
                            Key::Named(NamedKey::Escape) => {
                                self.bookmark_picker = None;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some(p) = self.bookmark_picker.as_mut() {
                                    match p.focus {
                                        BookmarkPickerFocus::Title => p.title_buffer.push(' '),
                                        BookmarkPickerFocus::Folder => p.folder_buffer.push(' '),
                                    }
                                }
                            }
                            Key::Character(s) => {
                                if let Some(p) = self.bookmark_picker.as_mut() {
                                    match p.focus {
                                        BookmarkPickerFocus::Title => p.title_buffer.push_str(s),
                                        BookmarkPickerFocus::Folder => p.folder_buffer.push_str(s),
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.render();
                        return;
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
                                    });
                                    let sel_id = self.devtools.elements.selected;
                                    if let Some(interp) = &mut self.interpreter {
                                        let result = console_eval_via_vm(&cmd, interp, sel_id);
                                        match result {
                                            Ok(v) => self.devtools.console.push_log(LogEntry {
                                                level: LogLevel::Result,
                                                text: v.pretty_print(),
                                            }),
                                            Err(e) => self.devtools.console.push_log(LogEntry {
                                                level: LogLevel::Error,
                                                text: e,
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
                        if let Some(fid) = focused_id {
                            if !self.find_open && !self.addr_open
                                && !self.devtools.focus.is_text_input()
                                && !self.modifiers.control_key()
                            {
                                let (node_opt, doc_rc) = self.interpreter.as_ref().map(|interp| {
                                    let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                                    let n = find_node_by_ptr(&doc_root, fid);
                                    (n, std::rc::Rc::clone(&interp.document))
                                }).unwrap_or((None, std::rc::Rc::new(std::cell::RefCell::new(crate::browser::dom::Document::empty(String::new())))));
                                if let Some(node) = node_opt {
                                    let tag = node.tag_name().unwrap_or_default();
                                    if matches!(tag.as_str(), "input" | "textarea") {
                                        use crate::browser::dom_input_buffer::DomInputBuffer;
                                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                                        let ctrl = self.modifiers.control_key();
                                        let shift = self.modifiers.shift_key();
                                        let mut buf = DomInputBuffer::new(node, doc_rc);
                                        let outcome = dispatch_text_key(&mut buf, &key_event.logical_key, ctrl, shift);
                                        let consumed = !matches!(outcome, TextKeyOutcome::Unhandled);
                                        if matches!(outcome, TextKeyOutcome::Submit) {
                                            // TODO form submit (najit ancestor form).
                                        }
                                        drop(buf); // Drop -> commit_back value attr.
                                        if consumed {
                                            self.cached_layout_root = None;
                                            self.render();
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Address bar typing.
                    if self.addr_open {
                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                        let ctrl = self.modifiers.control_key();
                        let shift = self.modifiers.shift_key();
                        let outcome = dispatch_text_key(&mut self.addr_input, &key_event.logical_key, ctrl, shift);
                        match outcome {
                            TextKeyOutcome::Submit => {
                                let url = std::mem::take(&mut self.addr_input.text);
                                self.addr_input.clear();
                                self.addr_open = false;
                                if !url.is_empty() {
                                    let resolved = resolve_addr_input(&url);
                                    println!("[address] navigate: {} -> {}", url, resolved);
                                    if resolved.starts_with("http://") || resolved.starts_with("https://")
                                        || resolved.starts_with("file:///") || resolved.starts_with("about:") {
                                        self.navigate_url(&resolved);
                                    } else {
                                        let p = std::path::PathBuf::from(&resolved);
                                        self.load_path(&p);
                                    }
                                }
                                self.render();
                                return;
                            }
                            TextKeyOutcome::Cancel => {
                                self.addr_open = false;
                                self.addr_input.clear();
                                self.render();
                                return;
                            }
                            TextKeyOutcome::Handled => {
                                self.render();
                                return;
                            }
                            _ => {}
                        }
                    }
                    // Find-on-page typing: pri otevrenem overlay capture chars.
                    if self.find_open {
                        use crate::browser::render::text_input::{dispatch_text_key, TextKeyOutcome};
                        let ctrl = self.modifiers.control_key();
                        let shift = self.modifiers.shift_key();
                        let outcome = dispatch_text_key(&mut self.find_query, &key_event.logical_key, ctrl, shift);
                        match outcome {
                            TextKeyOutcome::Submit => {
                                let dir = if shift { -1i32 } else { 1 };
                                self.find_step(dir);
                                return;
                            }
                            TextKeyOutcome::Cancel => {
                                self.find_open = false;
                                self.find_query.clear();
                                self.render();
                                return;
                            }
                            TextKeyOutcome::Handled => {
                                self.find_apply();
                                return;
                            }
                            _ => {}
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
                                    self.find_open = true;
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
                                // Ctrl+A: select cely document.
                                if let Some(layout) = &self.layout_root {
                                    let a = (layout.rect.x, layout.rect.y);
                                    let c = (layout.rect.x + layout.rect.width, layout.rect.y + layout.rect.height);
                                    self.page_sel_set_full(a, c);
                                    self.render();
                                }
                                return;
                            }
                            if s.as_str() == "p" || s.as_str() == "P" {
                                // Ctrl+P: export current page do PDF.
                                self.export_pdf();
                                return;
                            }
                            if s.as_str() == "l" || s.as_str() == "L" {
                                // Ctrl+L: toggle address bar.
                                self.addr_open = true;
                                self.addr_input = crate::devtools::model::text_buffer::SimpleStringBuffer::with_text_selected(self.base_url.clone().unwrap_or_default());
                                self.render();
                                return;
                            }
                            if s.as_str() == "d" || s.as_str() == "D" {
                                // Ctrl+D: open bookmark picker popup s folder selection.
                                if let Some(url) = self.base_url.clone() {
                                    let title = self.tabs.active_tab().title.clone();
                                    self.bookmark_picker = Some(BookmarkPickerState {
                                        url,
                                        title_buffer: title,
                                        folder_buffer: String::new(),
                                        focus: BookmarkPickerFocus::Title,
                                    });
                                    self.render();
                                }
                                return;
                            }
                            if s.as_str() == "h" || s.as_str() == "H" {
                                // Ctrl+H: open history page.
                                self.navigate_url("about:history");
                                return;
                            }
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
                                self.bookmarks_bar_visible = !self.bookmarks_bar_visible;
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
                                self.reading_mode_on = !self.reading_mode_on;
                                self.cached_stylesheets = None;
                                self.cached_style_map = None;
                                self.cached_pseudo_map = None;
                                self.cached_layout_root = None;
                                self.render();
                                return;
                            }
                            // Shell tab shortcuts.
                            if self.shell_mode {
                                if (s.as_str() == "T") && self.modifiers.shift_key() {
                                    // Ctrl+Shift+T = restore last closed tab.
                                    {
                                        let cur = self.tabs.active_tab_mut();
                                        cur.scroll_y = self.scroll_y;
                                        cur.scroll_x = self.scroll_x;
                                        cur.html = self.html.clone();
                                        cur.css = self.css.clone();
                                        cur.url = self.base_url.clone();
                                    }
                                    if self.tabs.restore_last_closed() {
                                        let t = self.tabs.active_tab().clone();
                                        self.html = t.html;
                                        self.css = t.css;
                                        self.base_url = t.url;
                                        self.scroll_y = t.scroll_y;
                                        self.scroll_x = t.scroll_x;
                                        self.cached_layout_root = None;
                                        self.cached_stylesheets = None;
                                        // Reload pres URL aby JS state byl fresh.
                                        if let Some(url) = self.base_url.clone() {
                                            self.navigate_url_no_history(&url);
                                        }
                                        self.render();
                                    }
                                    return;
                                }
                                if s.as_str() == "t" || s.as_str() == "T" {
                                    // Ctrl+T = new tab.
                                    {
                                        let cur = self.tabs.active_tab_mut();
                                        cur.scroll_y = self.scroll_y;
                                        cur.scroll_x = self.scroll_x;
                                        cur.html = self.html.clone();
                                        cur.css = self.css.clone();
                                        cur.url = self.base_url.clone();
                                    }
                                    self.tabs.open(crate::browser::render::tabs::Tab::empty());
                                    let t = self.tabs.active_tab().clone();
                                    self.html = t.html;
                                    self.css = t.css;
                                    self.base_url = t.url;
                                    self.scroll_y = t.scroll_y;
                                    self.scroll_x = t.scroll_x;
                                    self.cached_layout_root = None;
                                    self.cached_stylesheets = None;
                                    self.render();
                                    return;
                                }
                                if s.as_str() == "n" || s.as_str() == "N" {
                                    // Ctrl+N = open new browser window (spawn novy proces).
                                    let exe = std::env::current_exe().ok();
                                    if let Some(exe) = exe {
                                        let _ = std::process::Command::new(exe)
                                            .arg("browser")
                                            .spawn();
                                    }
                                    return;
                                }
                                if s.as_str() == "w" || s.as_str() == "W" {
                                    // Ctrl+W = close active tab.
                                    let active = self.tabs.active;
                                    self.tabs.close(active);
                                    let t = self.tabs.active_tab().clone();
                                    self.html = t.html;
                                    self.css = t.css;
                                    self.base_url = t.url;
                                    self.cached_layout_root = None;
                                    self.cached_stylesheets = None;
                                    self.render();
                                    return;
                                }
                                // Ctrl+1..9 jump to tab N-1.
                                if let Ok(n) = s.as_str().parse::<usize>() {
                                    if n >= 1 && n <= 9 {
                                        if n - 1 < self.tabs.tabs.len() {
                                            self.switch_tab_with_swap(n - 1);
                                            let t = self.tabs.active_tab().clone();
                                            self.html = t.html;
                                            self.css = t.css;
                                            self.base_url = t.url;
                                            self.cached_layout_root = None;
                                            self.cached_stylesheets = None;
                                            self.render();
                                        }
                                        return;
                                    }
                                }
                            }
                        }
                        // Ctrl+Tab = next tab.
                        if matches!(&key_event.logical_key, Key::Named(NamedKey::Tab)) && self.shell_mode {
                            let target = if self.modifiers.shift_key() {
                                if self.tabs.active == 0 { self.tabs.tabs.len() - 1 } else { self.tabs.active - 1 }
                            } else {
                                (self.tabs.active + 1) % self.tabs.tabs.len()
                            };
                            self.switch_tab_with_swap(target);
                            let t = self.tabs.active_tab().clone();
                            self.html = t.html;
                            self.css = t.css;
                            self.base_url = t.url;
                            self.cached_layout_root = None;
                            self.cached_stylesheets = None;
                            self.render();
                            return;
                        }
                    }
                    // Ctrl+= / Ctrl++ / Ctrl+- / Ctrl+0 = zoom controls.
                    if self.modifiers.control_key() {
                        if let Key::Character(s) = &key_event.logical_key {
                            match s.as_str() {
                                "+" | "=" => {
                                    self.zoom = (self.zoom * 1.1).min(5.0);
                                    self.cached_layout_root = None;
                                    self.clamp_scroll_to_layout();
                                    println!("[zoom] {:.0}%", self.zoom * 100.0);
                                    self.render();
                                    return;
                                }
                                "-" | "_" => {
                                    self.zoom = (self.zoom / 1.1).max(0.25);
                                    self.cached_layout_root = None;
                                    self.clamp_scroll_to_layout();
                                    println!("[zoom] {:.0}%", self.zoom * 100.0);
                                    self.render();
                                    return;
                                }
                                "0" => {
                                    self.zoom = 1.0;
                                    self.cached_layout_root = None;
                                    self.clamp_scroll_to_layout();
                                    println!("[zoom] 100%");
                                    self.render();
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                    match key_event.logical_key {
                        Key::Named(NamedKey::F1) => {
                            self.shortcuts_overlay_open = !self.shortcuts_overlay_open;
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                        Key::Named(NamedKey::F12) => {
                            self.devtools.panel_open = !self.devtools.panel_open;
                            if self.devtools.panel_open {
                                if let Some(interp) = &self.interpreter {
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
                            if let Some(p) = self.current_path.clone() {
                                println!("[F5 reload] {}", p.display());
                                self.load_path(&p);
                                self.render();
                            } else if let Some(url) = self.base_url.clone() {
                                println!("[F5 reload] {url}");
                                self.navigate_url_no_history(&url);
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            // Alt+Left back. Bez modifier check zatim - winit ma KeyEvent.modifiers.
                            if self.history_idx > 0 {
                                self.history_idx -= 1;
                                let url = self.history[self.history_idx].clone();
                                println!("[history back] {url}");
                                self.navigate_url_no_history(&url);
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if self.history_idx + 1 < self.history.len() {
                                self.history_idx += 1;
                                let url = self.history[self.history_idx].clone();
                                println!("[history forward] {url}");
                                self.navigate_url_no_history(&url);
                            }
                        }
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
                            self.scroll_target_y = 0.0;
                            self.scroll_target_x = 0.0;
                            self.render();
                        }
                        Key::Named(NamedKey::End) => {
                            if let (Some(layout), Some(r)) = (&self.layout_root, &self.renderer) {
                                let vh = (r.config.height as f32) / (self.zoom * r.scale_factor);
                                self.scroll_target_y = (layout.rect.height - vh).max(0.0);
                                self.render();
                            }
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
            if self.bookmark_picker.is_some() {
                self.bookmark_picker = None;
                return true;
            }
            if self.shortcuts_overlay_open {
                self.shortcuts_overlay_open = false;
                return true;
            }
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
            if self.addr_open {
                self.addr_open = false;
                self.addr_input.clear();
                return true;
            }
            if self.find_open {
                self.find_open = false;
                self.find_query.clear();
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
            self.renderer.as_ref().map(|r| (r.config.height as f32) / (self.zoom * r.scale_factor)).unwrap_or(768.0)
        }
        fn viewport_w_logical(&self) -> f32 {
            self.renderer.as_ref().map(|r| (r.config.width as f32) / (self.zoom * r.scale_factor)).unwrap_or(1024.0)
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
            if !self.shell_mode { return 0.0; }
            let bm_count = crate::devtools::bookmarks::load_bookmarks().len();
            // Base: tab strip 28 + nav 36 = 64. Bookmarks bar 24 navic (kdyz visible + nejaky bm).
            64.0 + if bm_count > 0 && self.bookmarks_bar_visible { 24.0 } else { 0.0 }
        }
        /// Page commands shift dolu o chrome height (pri shell_mode).
        fn shift_page_for_chrome(&self, list: &mut [DisplayCommand]) {
            let dy = self.shell_chrome_h_active();
            if dy < 0.5 { return; }
            for cmd in list.iter_mut() {
                use DisplayCommand::*;
                match cmd {
                    Rect { y, .. } | Text { y, .. } | Border { y, .. }
                    | Image { y, .. } | Gradient { y, .. } | Shadow { y, .. } => *y += dy,
                    _ => {}
                }
            }
        }
    }

    /// Chrome bar hit zones.
    pub enum ChromeHit {
        TabClick(usize),
        TabClose(usize),
        TabContextMenu(usize),
        NewTab,
        Back,
        Forward,
        Reload,
        UrlBar,
        DevtoolsToggle,
        BookmarkStar,
        BookmarkClick(String),
        BookmarkContextMenu(String),
        None,
    }

    pub fn hit_chrome(win_w: f32, chrome_h: f32, tabs: &tabs::TabManager,
                      mouse_x: f32, mouse_y: f32) -> ChromeHit {
        if mouse_y < 0.0 || mouse_y >= chrome_h { return ChromeHit::None; }
        let tab_h = 28.0;
        let nav_h = chrome_h - tab_h;
        if mouse_y < tab_h {
            // Tab strip.
            let tab_w = 200.0_f32.min(win_w / (tabs.tabs.len().max(1) as f32 + 0.5));
            let mut tx = 4.0;
            for (i, _) in tabs.tabs.iter().enumerate() {
                if mouse_x >= tx && mouse_x < tx + tab_w {
                    // Close X v pravej casti chipu.
                    if mouse_x >= tx + tab_w - 22.0 {
                        return ChromeHit::TabClose(i);
                    }
                    return ChromeHit::TabClick(i);
                }
                tx += tab_w + 2.0;
            }
            // + new tab.
            if mouse_x >= tx && mouse_x < tx + 22.0 {
                return ChromeHit::NewTab;
            }
            return ChromeHit::None;
        }
        // Nav bar.
        let ny = tab_h;
        if mouse_y >= ny && mouse_y < ny + nav_h {
            if mouse_x >= 8.0 && mouse_x < 28.0 { return ChromeHit::Back; }
            if mouse_x >= 28.0 && mouse_x < 48.0 { return ChromeHit::Forward; }
            if mouse_x >= 48.0 && mouse_x < 68.0 { return ChromeHit::Reload; }
            if mouse_x >= win_w - 36.0 && mouse_x < win_w - 8.0 { return ChromeHit::DevtoolsToggle; }
            if mouse_x >= win_w - 76.0 && mouse_x < win_w - 56.0 { return ChromeHit::BookmarkStar; }
            if mouse_x >= 78.0 && mouse_x < win_w - 80.0 { return ChromeHit::UrlBar; }
        }
        // Bookmarks bar (24px pod nav bar).
        let bm_y = chrome_h - 24.0;
        if mouse_y >= bm_y && mouse_y < bm_y + 24.0 {
            let bms = crate::devtools::bookmarks::load_bookmarks();
            let mut bx = 8.0;
            for bm in bms.iter().take(15) {
                let title_trunc: String = bm.title.chars().take(18).collect();
                let bw = (title_trunc.len() as f32) * 7.0 + 16.0;
                if bx + bw > win_w - 8.0 { break; }
                if mouse_x >= bx && mouse_x < bx + bw {
                    return ChromeHit::BookmarkClick(bm.url.clone());
                }
                bx += bw + 4.0;
            }
        }
        ChromeHit::None
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

    fn paint_shell_chrome_inline(list: &mut Vec<DisplayCommand>, win_w: f32, chrome_h: f32, url: &str) {
        paint_shell_chrome_full(list, win_w, chrome_h, url, None, 0);
    }

    fn paint_shell_chrome_full(list: &mut Vec<DisplayCommand>, win_w: f32, chrome_h: f32,
                                url: &str, tab_titles: Option<&[String]>, active: usize) {
        paint_shell_chrome_with_favicons(list, win_w, chrome_h, url, tab_titles, active, None);
    }

    fn paint_shell_chrome_with_favicons(list: &mut Vec<DisplayCommand>, win_w: f32, chrome_h: f32,
                                         url: &str, tab_titles: Option<&[String]>, active: usize,
                                         favicon_urls: Option<&[Option<String>]>) {
        paint_shell_chrome_with_pins(list, win_w, chrome_h, url, tab_titles, active, favicon_urls, None);
    }

    fn paint_shell_chrome_with_pins(list: &mut Vec<DisplayCommand>, win_w: f32, chrome_h: f32,
                                    url: &str, tab_titles: Option<&[String]>, active: usize,
                                    favicon_urls: Option<&[Option<String>]>,
                                    pinned: Option<&[bool]>) {
        paint_shell_chrome_with_loading(list, win_w, chrome_h, url, tab_titles, active,
                                        favicon_urls, pinned, None, 0.0);
    }

    /// Verze co podporuje per-tab loading flag + animacni fazi (sekundy od startu)
    /// pro busy spinner.
    fn paint_shell_chrome_with_loading(list: &mut Vec<DisplayCommand>, win_w: f32, chrome_h: f32,
                                       url: &str, tab_titles: Option<&[String]>, active: usize,
                                       favicon_urls: Option<&[Option<String>]>,
                                       pinned: Option<&[bool]>,
                                       loading: Option<&[bool]>, anim_t: f32) {
        paint_shell_chrome_with_groups(list, win_w, chrome_h, url, tab_titles, active,
                                       favicon_urls, pinned, loading, anim_t, None);
    }

    /// Verze s tab group colors - top-edge stripe.
    fn paint_shell_chrome_with_groups(list: &mut Vec<DisplayCommand>, win_w: f32, chrome_h: f32,
                                      url: &str, tab_titles: Option<&[String]>, active: usize,
                                      favicon_urls: Option<&[Option<String>]>,
                                      pinned: Option<&[bool]>,
                                      loading: Option<&[bool]>, anim_t: f32,
                                      groups: Option<&[Option<[u8; 4]>]>) {
        // Bookmarks bar paint pod nav bar - dalsi 24px row.
        let bms = crate::devtools::bookmarks::load_bookmarks();
        let _ = &bms; // used below ve scope.
        {
            let tab_h = 28.0;
            let nav_h = chrome_h - tab_h;
            // Chrome bg.
            list.push(DisplayCommand::Rect {
                x: 0.0, y: 0.0, w: win_w, h: chrome_h,
                color: [42, 41, 50, 255], radius: 0.0,
            });
            list.push(DisplayCommand::Rect {
                x: 0.0, y: chrome_h - 1.0, w: win_w, h: 1.0,
                color: [76, 76, 85, 255], radius: 0.0,
            });
            // Tab strip - per-tab chip.
            let titles_owned: Vec<String> = if let Some(t) = tab_titles {
                t.to_vec()
            } else {
                vec![if url.is_empty() { "Nova zalozka".to_string() }
                     else {
                         let s = url.split('/').last().unwrap_or(url).to_string();
                         if s.is_empty() { "page".to_string() } else { s }
                     }]
            };
            let n = titles_owned.len();
            let pin_count = pinned.map(|p| p.iter().filter(|x| **x).count()).unwrap_or(0);
            let avail_w = win_w - 60.0 - (pin_count as f32) * 40.0;
            let normal_count = n.saturating_sub(pin_count).max(1);
            let tab_w = 200.0_f32.min(avail_w / normal_count as f32);
            let mut tx = 4.0;
            for (i, title) in titles_owned.iter().enumerate() {
                let is_pinned = pinned.map(|p| p.get(i).copied().unwrap_or(false)).unwrap_or(false);
                let chip_w = if is_pinned { 36.0 } else { tab_w };
                let bg = if i == active { [27, 27, 35, 255] } else { [42, 41, 50, 255] };
                list.push(DisplayCommand::Rect {
                    x: tx, y: 4.0, w: chip_w, h: tab_h - 4.0,
                    color: bg, radius: 4.0,
                });
                // Group color top stripe (3px).
                if let Some(gc) = groups.and_then(|g| g.get(i)).and_then(|c| *c) {
                    list.push(DisplayCommand::Rect {
                        x: tx + 2.0, y: 4.0, w: chip_w - 4.0, h: 3.0,
                        color: gc, radius: 1.5,
                    });
                }
                if is_pinned {
                    // Pinned indicator (zluty pin v levem rohu).
                    list.push(DisplayCommand::Text {
                        x: tx + 4.0, y: 6.0, content: "📌".to_string(),
                        color: [254, 191, 84, 255],
                        font_size: 11.0, bold: false, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // Loading: rotujici dot misto favicon. Spinner = 8 prouzku v kruhu, fade dle uhlu.
                let is_loading = loading.and_then(|l| l.get(i)).copied().unwrap_or(false);
                let favicon_present = if is_pinned || is_loading { None } else {
                    favicon_urls.and_then(|fs| fs.get(i)).and_then(|f| f.clone())
                };
                let text_x_off = if is_loading && !is_pinned {
                    // Spinner: 8 prouzku v kruhu, fade dle uhlu+t.
                    let cx = tx + 14.0;
                    let cy = 16.0;
                    let bars = 8;
                    let phase = (anim_t * 8.0) as i32;
                    for b in 0..bars {
                        let ang = (b as f32) * std::f32::consts::TAU / bars as f32;
                        let dx = ang.cos();
                        let dy = ang.sin();
                        let inner = 3.5;
                        let outer = 6.5;
                        let x1 = cx + dx * inner;
                        let y1 = cy + dy * inner;
                        let x2 = cx + dx * outer;
                        let y2 = cy + dy * outer;
                        let lit = ((b as i32 - phase).rem_euclid(bars as i32)) as f32 / bars as f32;
                        let a = (60.0 + lit * 195.0) as u8;
                        list.push(DisplayCommand::Rect {
                            x: x1.min(x2) - 0.5, y: y1.min(y2) - 0.5,
                            w: (x2 - x1).abs().max(1.5),
                            h: (y2 - y1).abs().max(1.5),
                            color: [69, 161, 255, a], radius: 0.5,
                        });
                    }
                    28.0
                } else if let Some(furl) = favicon_present {
                    list.push(DisplayCommand::Image {
                        x: tx + 6.0, y: 8.0, w: 16.0, h: 16.0,
                        src: furl,
                        radius: 0.0,
                    });
                    28.0
                } else if is_pinned {
                    20.0
                } else {
                    8.0
                };
                if !is_pinned {
                    let trunc: String = title.chars().take(20).collect();
                    list.push(DisplayCommand::Text {
                        x: tx + text_x_off, y: 8.0, content: trunc,
                        color: [251, 251, 254, 255],
                        font_size: 13.0, bold: i == active, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    // Close X (jen non-pinned).
                    list.push(DisplayCommand::Rect {
                        x: tx + chip_w - 18.0, y: 6.0, w: 16.0, h: 16.0,
                        color: [56, 56, 65, 200], radius: 8.0,
                    });
                    list.push(DisplayCommand::Text {
                        x: tx + chip_w - 14.0, y: 8.0, content: "x".to_string(),
                        color: [220, 220, 230, 255],
                        font_size: 12.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                tx += chip_w + 2.0;
            }
            // + new tab button.
            list.push(DisplayCommand::Text {
                x: tx + 4.0, y: 6.0, content: "+".to_string(),
                color: [191, 191, 201, 255],
                font_size: 18.0, bold: true, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });

            // Nav bar (back/forward/reload + URL).
            let ny = tab_h;
            // Back button.
            list.push(DisplayCommand::Text {
                x: 12.0, y: ny + 8.0, content: "<".to_string(),
                color: [251, 251, 254, 255],
                font_size: 16.0, bold: true, italic: false,
                font_family: "CamingoMono".into(),
                strikethrough: false, underline: false,
            });
            // Forward.
            list.push(DisplayCommand::Text {
                x: 32.0, y: ny + 8.0, content: ">".to_string(),
                color: [251, 251, 254, 255],
                font_size: 16.0, bold: true, italic: false,
                font_family: "CamingoMono".into(),
                strikethrough: false, underline: false,
            });
            // Reload.
            list.push(DisplayCommand::Text {
                x: 52.0, y: ny + 8.0, content: "↻".to_string(),
                color: [251, 251, 254, 255],
                font_size: 14.0, bold: false, italic: false,
                font_family: "CamingoMono".into(),
                strikethrough: false, underline: false,
            });
            // Bookmark star indicator (vpravo na konci URL bar, oznaci je-li current URL v bookmarks).
            let bookmarked = !url.is_empty()
                && crate::devtools::bookmarks::load_bookmarks().iter().any(|b| b.url == url);
            let star_color = if bookmarked { [254, 191, 84, 255] } else { [109, 109, 124, 200] };
            list.push(DisplayCommand::Text {
                x: win_w - 76.0, y: ny + 8.0, content: "★".to_string(),
                color: star_color,
                font_size: 16.0, bold: false, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });
            // Devtools toggle button (na konci nav baru pred URL).
            let dt_x = win_w - 36.0;
            list.push(DisplayCommand::Rect {
                x: dt_x, y: ny + 4.0, w: 28.0, h: nav_h - 8.0,
                color: [42, 41, 50, 255], radius: 4.0,
            });
            list.push(DisplayCommand::Text {
                x: dt_x + 6.0, y: ny + 8.0, content: "F12".to_string(),
                color: [191, 191, 201, 255],
                font_size: 11.0, bold: true, italic: false,
                font_family: "Inter".into(),
                strikethrough: false, underline: false,
            });
            // URL bar.
            let url_x = 78.0;
            let url_w = win_w - url_x - 48.0;
            list.push(DisplayCommand::Rect {
                x: url_x, y: ny + 4.0, w: url_w, h: nav_h - 8.0,
                color: [27, 27, 35, 255], radius: 4.0,
            });
            list.push(DisplayCommand::Text {
                x: url_x + 8.0, y: ny + 9.0, content: url.to_string(),
                color: [251, 251, 254, 255],
                font_size: 12.0, bold: false, italic: false,
                font_family: "CamingoMono".into(),
                strikethrough: false, underline: false,
            });
            // Bookmarks bar (jen kdyz nejake jsou A chrome_h alokovala 24px navic).
            if !bms.is_empty() && chrome_h >= 88.0 {
                let bm_y = chrome_h - 24.0;
                list.push(DisplayCommand::Rect {
                    x: 0.0, y: bm_y, w: win_w, h: 24.0,
                    color: [35, 34, 43, 255], radius: 0.0,
                });
                list.push(DisplayCommand::Rect {
                    x: 0.0, y: bm_y + 23.0, w: win_w, h: 1.0,
                    color: [76, 76, 85, 255], radius: 0.0,
                });
                let mut bx_x = 8.0;
                for bm in bms.iter().take(15) {
                    let title_trunc: String = bm.title.chars().take(18).collect();
                    let bw = (title_trunc.len() as f32) * 7.0 + 16.0;
                    if bx_x + bw > win_w - 8.0 { break; }
                    list.push(DisplayCommand::Rect {
                        x: bx_x, y: bm_y + 2.0, w: bw, h: 20.0,
                        color: [42, 41, 50, 255], radius: 4.0,
                    });
                    list.push(DisplayCommand::Text {
                        x: bx_x + 6.0, y: bm_y + 5.0,
                        content: title_trunc, color: [191, 191, 201, 255],
                        font_size: 11.0, bold: false, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    bx_x += bw + 4.0;
                }
            }
        }
    }
    impl App {
        // ─── Page selection accessors (Document.selection.page_selection) ───
        // App.selection_* fields zruseny - registry je primary state.

        fn page_sel_anchor(&self) -> Option<(f32, f32)> {
            self.interpreter.as_ref()
                .and_then(|i| i.document.borrow().selection.borrow().page_selection.as_ref().map(|p| p.anchor))
        }
        fn page_sel_current(&self) -> Option<(f32, f32)> {
            self.interpreter.as_ref()
                .and_then(|i| i.document.borrow().selection.borrow().page_selection.as_ref().map(|p| p.current))
        }
        fn page_sel_dragging(&self) -> bool {
            self.interpreter.as_ref()
                .map(|i| i.document.borrow().selection.borrow().page_selection.as_ref().map(|p| p.dragging).unwrap_or(false))
                .unwrap_or(false)
        }
        fn page_sel_begin(&self, anchor: (f32, f32)) {
            let Some(interp) = &self.interpreter else { return };
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
            let Some(interp) = &self.interpreter else { return };
            let doc = interp.document.borrow();
            let mut reg = doc.selection.borrow_mut();
            if let Some(ps) = reg.page_selection.as_mut() {
                ps.current = current;
                ps.cached_text = cached;
            }
        }
        fn page_sel_end_drag(&self) {
            let Some(interp) = &self.interpreter else { return };
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
        /// Vraci SelectionPos nebo None (mimo vsech runs).
        pub fn hit_test_text_run(&self, x: f32, y: f32) -> Option<crate::browser::textrun::SelectionPos> {
            crate::browser::textrun::hit_test_runs(&self.painted_text_runs, x, y)
        }

        /// Extract text z anchor->focus SelectionPos pro Ctrl+C copy.
        /// Per-glyph precision (vs flow-based bbox extract).
        pub fn extract_text_run_selection(&self,
            anchor: crate::browser::textrun::SelectionPos,
            focus: crate::browser::textrun::SelectionPos,
        ) -> String {
            let sel = crate::browser::textrun::TextSelection { anchor, focus };
            sel.extract_text(&self.painted_text_runs)
        }

        fn page_sel_clear(&self) {
            let Some(interp) = &self.interpreter else { return };
            let doc = interp.document.borrow();
            doc.selection.borrow_mut().page_selection = None;
        }
        fn page_sel_set_full(&self, anchor: (f32, f32), current: (f32, f32)) {
            let cached = self.compute_selection_text(anchor, current);
            let Some(interp) = &self.interpreter else { return };
            let doc = interp.document.borrow();
            doc.selection.borrow_mut().page_selection = Some(crate::browser::selection::PageSelection {
                anchor, current, dragging: false, cached_text: cached,
            });
        }

        /// Flow-based text extract: anchor->current v reading order, pres
        /// wrapped lines. First line: chars od start.x; middle lines: full
        /// line; last line: chars do end.x.
        fn compute_selection_text(&self, a: (f32, f32), c: (f32, f32)) -> String {
            let Some(layout) = &self.layout_root else { return String::new() };
            let (start, end) = if a.1 < c.1 || (a.1 == c.1 && a.0 <= c.0) {
                (a, c)
            } else { (c, a) };
            if (end.0 - start.0).abs() < 1.0 && (end.1 - start.1).abs() < 1.0 { return String::new(); }
            let mut out = String::new();
            fn walk(b: &super::layout::LayoutBox, sx: f32, sy: f32, ex: f32, ey: f32, out: &mut String) {
                if let Some(text) = &b.text {
                    let bx0 = b.rect.x;
                    let by0 = b.rect.y;
                    let by1 = by0 + b.rect.height;
                    if by1 >= sy && by0 <= ey {
                        let lh = (b.line_height * b.font_size).max(b.font_size * 1.2);
                        let bold = b.bold;
                        let lines: Vec<&str> = text.split('\n').collect();
                        for (li, line) in lines.iter().enumerate() {
                            let line_y = by0 + (li as f32) * lh;
                            let line_y_end = line_y + lh;
                            if line_y_end < sy || line_y > ey { continue; }
                            let is_first_line = sy >= line_y && sy < line_y_end;
                            let is_last_line = ey >= line_y && ey < line_y_end;
                            let line_start_x = bx0;
                            let line_w: f32 = line.chars().map(|ch|
                                super::layout::measure_text_width_styled(
                                    &ch.to_string(), b.font_size, bold)).sum();
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
                            let mut acc = 0.0f32;
                            let mut start_byte: Option<usize> = None;
                            let mut end_byte: usize = line.len();
                            for (byte_off, ch) in line.char_indices() {
                                let adv = super::layout::measure_text_width_styled(
                                    &ch.to_string(), b.font_size, bold);
                                let mid = acc + adv * 0.5;
                                if start_byte.is_none() && mid >= sel_left {
                                    start_byte = Some(byte_off);
                                }
                                if mid > sel_right {
                                    end_byte = byte_off;
                                    break;
                                }
                                acc += adv;
                            }
                            let s = start_byte.unwrap_or(0);
                            if s < end_byte {
                                out.push_str(&line[s..end_byte]);
                                out.push(' ');
                            }
                        }
                    }
                }
                for ch in &b.children { walk(ch, sx, sy, ex, ey, out); }
            }
            walk(layout, start.0, start.1, end.0, end.1, &mut out);
            out.trim().to_string()
        }
        /// Centralni cursor icon dispatch - dle pozice + DOM/devtools state.
        fn compute_cursor_icon(&self, target: Option<&super::layout::LayoutBox>) -> winit::window::CursorIcon {
            use winit::window::CursorIcon;
            use crate::devtools::profile::DockPosition;
            // 1. Devtools panel hit?
            let mx_screen = self.mouse_x - self.scroll_x;
            let my_screen = self.mouse_y - self.scroll_y;
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
            // 2. Page main scrollbar -> Default.
            if let Some(layout) = &self.layout_root {
                let viewport_w = self.viewport_w_logical();
                let viewport_h = self.viewport_h_logical() - self.panel_h_logical();
                if layout.rect.height > viewport_h
                    && self.mouse_x >= viewport_w - 12.0 && self.mouse_x < viewport_w {
                    return CursorIcon::Default;
                }
            }
            // 3. Page element classify pres InteractiveKind.
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
        /// Najde vsechny pozice match v display listu textech. Vrati Vec<(y, x, w)>.
        fn find_collect_matches(&self) -> Vec<(f32, f32, f32)> {
            match &self.layout_root {
                Some(l) => find_matches_in(l, &self.find_query.text),
                None => Vec::new(),
            }
        }
        fn find_apply(&mut self) {
            let matches = self.find_collect_matches();
            if matches.is_empty() {
                self.find_match_idx = 0;
            } else if self.find_match_idx >= matches.len() {
                self.find_match_idx = 0;
            }
            self.find_scroll_to_current();
            self.render();
        }
        fn find_step(&mut self, dir: i32) {
            let matches = self.find_collect_matches();
            if matches.is_empty() { return; }
            let n = matches.len() as i32;
            let cur = self.find_match_idx as i32;
            self.find_match_idx = ((cur + dir).rem_euclid(n)) as usize;
            self.find_scroll_to_current();
            self.render();
        }
        /// Export aktualni stranky do PDF souboru. Walk LayoutBox tree, emituje
        /// text uzly + bg rects do printpdf documentu. Save do current_path
        /// directory s .pdf priponou.
        fn export_pdf(&mut self) {
            use printpdf::*;
            let layout = match &self.layout_root { Some(l) => l.clone(), None => return };
            let page_w_mm = 210.0_f32; // A4 width
            let _page_h_mm = 297.0_f32;
            // Layout coords v px -> PDF v mm. Approx 96 DPI = 1 px = 0.264 mm.
            let px_to_mm = 0.264_f32;
            let total_h_mm = layout.rect.height * px_to_mm;
            let (doc, page1, layer1) = PdfDocument::new("Page", Mm(page_w_mm), Mm(total_h_mm.max(297.0)), "Layer 1");
            let font = match doc.add_builtin_font(BuiltinFont::TimesRoman) {
                Ok(f) => f,
                Err(e) => { eprintln!("[pdf] font fail: {e}"); return; }
            };
            let layer = doc.get_page(page1).get_layer(layer1);
            // Walk LayoutBox tree, emituje text uzly s pozici (x, h-y) (PDF y-up).
            fn walk(b: &super::layout::LayoutBox, layer: &PdfLayerReference, font: &IndirectFontRef, page_h_px: f32, px_to_mm: f32) {
                if let Some(text) = &b.text {
                    if !text.trim().is_empty() {
                        let x_mm = b.rect.x * px_to_mm;
                        let y_mm = (page_h_px - b.rect.y - b.font_size) * px_to_mm;
                        let fs = b.font_size * px_to_mm * 2.83; // pt = mm * 2.83
                        layer.use_text(text, fs, Mm(x_mm), Mm(y_mm), font);
                    }
                }
                for ch in &b.children { walk(ch, layer, font, page_h_px, px_to_mm); }
            }
            walk(&layout, &layer, &font, layout.rect.height, px_to_mm);
            // Save: pri current_path pres .pdf substituce, jinak ~/page.pdf.
            let out_path = self.current_path.as_ref()
                .and_then(|p| p.to_str().map(|s| s.replace(".html", ".pdf")))
                .unwrap_or_else(|| "page.pdf".to_string());
            match std::fs::File::create(&out_path) {
                Ok(file) => {
                    let mut bw = std::io::BufWriter::new(file);
                    if let Err(e) = doc.save(&mut bw) {
                        eprintln!("[pdf] save fail: {e}");
                    } else {
                        println!("[pdf] saved: {}", out_path);
                    }
                }
                Err(e) => eprintln!("[pdf] open fail {}: {}", out_path, e),
            }
        }
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
        fn find_scroll_to_current(&mut self) {
            let matches = self.find_collect_matches();
            if let Some(&(my, _, _)) = matches.get(self.find_match_idx) {
                let vh = self.viewport_h_logical();
                self.scroll_target_y = (my - vh * 0.3).max(0.0);
                self.clamp_scroll_to_layout();
            }
        }
        fn scroll_by_y(&mut self, dy: f32) {
            // Smooth scroll: posun target. Render tick interpoluje scroll_y -> target.
            self.scroll_target_y = (self.scroll_target_y + dy).max(0.0);
            self.clamp_scroll_to_layout();
            self.render();
        }
        /// Po zoom change: clamp scroll_y/scroll_x do max scrollu pro nove
        /// layout dimensions. Pri zoomu out se layout zmensi -> overflow muze
        /// zmizet -> max_scroll = 0. Stara scroll_y > 0 by ukazovala blank.
        fn clamp_scroll_to_layout(&mut self) {
            if let (Some(layout), Some(r)) = (&self.layout_root, &self.renderer) {
                let vw = (r.config.width as f32) / (self.zoom * r.scale_factor);
                let vh = (r.config.height as f32) / (self.zoom * r.scale_factor);
                let max_y = (layout.rect.height - vh).max(0.0);
                let max_x = (layout.rect.width - vw).max(0.0);
                if self.scroll_y > max_y { self.scroll_y = max_y; }
                if self.scroll_x > max_x { self.scroll_x = max_x; }
                if self.scroll_target_y > max_y { self.scroll_target_y = max_y; }
                if self.scroll_target_x > max_x { self.scroll_target_x = max_x; }
            }
        }
        /// Smooth scroll tick. Lerp scroll_y -> scroll_target_y. Vrati true pokud
        /// stale animuje (volajici by mel request_redraw).
        fn smooth_scroll_tick(&mut self) -> bool {
            let dy = self.scroll_target_y - self.scroll_y;
            let dx = self.scroll_target_x - self.scroll_x;
            let mut animating = false;
            if dy.abs() > 0.5 {
                self.scroll_y += dy * 0.25;
                animating = true;
            } else {
                self.scroll_y = self.scroll_target_y;
            }
            if dx.abs() > 0.5 {
                self.scroll_x += dx * 0.25;
                animating = true;
            } else {
                self.scroll_x = self.scroll_target_x;
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
            self.html = html;
            self.css = css;
            self.current_path = Some(path.to_path_buf());
            self.scroll_y = 0.0;
            self.scroll_target_y = 0.0;
            self.scroll_x = 0.0;
            self.scroll_target_x = 0.0;
            self.start_time = std::time::Instant::now();
            self.prev_style_map = None;
            self.active_animations.clear();
            self.animation_iterations.clear();
            self.active_transitions.clear();
            // Restart interpreter s novym dokumentem.
            let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
            let doc = super::html_parser::parse_html(&self.html, &url);
            let mut interp = crate::interpreter::Interpreter::new();
            interp.set_document(doc);
            self.run_inline_scripts(&mut interp);
            self.interpreter = Some(interp);
            // Sync title -> active tab + window title.
            let page_title = crate::browser::render::tabs::extract_title(&self.html)
                .unwrap_or_else(|| path.file_name()
                    .and_then(|n| n.to_str()).unwrap_or("page").to_string());
            {
                let cur = self.tabs.active_tab_mut();
                cur.html = self.html.clone();
                cur.css = self.css.clone();
                cur.path = Some(path.to_path_buf());
                cur.url = Some(url.clone());
                cur.title = page_title.clone();
                cur.loading = false;
            }
            if let Some(w) = &self.window {
                w.set_title(&format_window_title(&page_title, self.tabs.tabs.len()));
            }
            // Pokud je auto_devtools zaplo, take regen + open po reload.
            if self.auto_devtools {
                self.regenerate_and_open_devtools();
            }
        }

        /// Regen devtools.html + otevri ho v default OS browseru.
        fn regenerate_and_open_devtools(&self) {
            let interp = match &self.interpreter { Some(i) => i, None => return };
            let stylesheets = vec![super::css_parser::parse_stylesheet(&self.css)];
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
            if self.interpreter.is_none() { return }
            // Vytvor novy Interpreter (cisty state) ALE zkopiruj breakpoints +
            // skip_once_line z aktualniho debuggeru aby pause logic fungovala
            // dale. DOM je v interpreter.document - zachova v novem.
            let saved_bp;
            let saved_skip;
            let saved_console;
            let saved_doc;
            {
                let interp = self.interpreter.as_ref().unwrap();
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
            self.interpreter = Some(new_interp);
            // Znovu spust scripts.
            let mut tmp = self.interpreter.take().unwrap();
            self.run_inline_scripts(&mut tmp);
            self.interpreter = Some(tmp);
            self.cached_layout_root = None;
            self.cached_style_map = None;
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
            let html = self.html.clone();
            let base_url = self.base_url.clone().unwrap_or_default();
            let bp_lines: Vec<u32> = self.devtools.sources.breakpoints.iter()
                .map(|b| b.line).collect();
            let runner = crate::devtools::debug_runner::DebugRunner::spawn(
                html, base_url, bp_lines);
            self.devtools.console.push_log(crate::devtools::model::console::LogEntry {
                level: crate::devtools::model::console::LogLevel::Info,
                text: "[debug-mode] Worker thread spustil eval JS - real freeze pause aktivni".into(),
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
                        self.devtools.console.push_log(LogEntry { level: lvl, text: msg });
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
                        });
                        self.devtools.sources.debugger_paused = false;
                        self.devtools.sources.current_pause_location = None;
                    }
                    WorkerEvent::Error(e) => {
                        self.devtools.console.push_log(LogEntry {
                            level: LogLevel::Error,
                            text: format!("[debug-mode] Error: {}", e),
                        });
                    }
                }
            }
            // Po Done event, join worker (uvolni handle).
            if runner.is_finished() {
                self.deactivate_debug_mode();
            }
        }

        fn run_inline_scripts(&mut self, interp: &mut crate::interpreter::Interpreter) {
            use crate::lexer::base::Lexer;
            use crate::parser::Parser;
            use crate::tokens::TokenKind;

            let doc_ref = interp.document.clone();
            let scripts: Vec<(String, String)> = doc_ref.borrow().root
                .get_elements_by_tag("script")
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let url = s.attr("src").unwrap_or_else(|| format!("<inline #{}>", i + 1));
                    (url, s.text_content())
                })
                .collect();

            // Registruj scripts do DevTools sources panel + try fetch source map.
            use crate::devtools::model::sources::SourceLang;
            let base = self.base_url.clone().unwrap_or_default();
            for (url, src) in &scripts {
                if src.trim().is_empty() { continue; }
                let id = self.devtools.sources.add_file(url.clone(), src.clone(), SourceLang::JavaScript);
                let resolve_base = if url.starts_with("http") || url.starts_with("file:") {
                    url.clone()
                } else { base.clone() };
                self.devtools.sources.load_source_map(id, &resolve_base,
                    |u| super::render::fetch_text_url(u));
            }

            for (_url, src) in scripts {
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
            // Po skriptech: sync document.title (mohlo jit zmenit zevnitr JS) -> active tab + window.
            let new_title = interp.document.borrow().title.clone();
            if !new_title.is_empty() {
                self.tabs.active_tab_mut().title = new_title.clone();
                if let Some(w) = &self.window {
                    w.set_title(&format_window_title(&new_title, self.tabs.tabs.len()));
                }
            }
        }

        fn trigger_autocomplete(&mut self) {
            use crate::devtools::model::console::{suggest, AutocompleteState};
            let text = self.devtools.console.input.text.clone();
            let cursor = self.devtools.console.input.cursor;
            let globals: Vec<String> = if let Some(interp) = &self.interpreter {
                interp.global.borrow().names()
            } else { Vec::new() };
            // Reflective property resolver: base ident -> own props z JsObject.
            // Hleda v env: kdyz base je top-level var, a hodnota je Object,
            // vrat keys. Jinak prazdny - fallback na hardcoded baseline.
            let interp_ref = self.interpreter.as_ref();
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
            let Some(interp) = &self.interpreter else { return };
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
            let Some(interp) = &self.interpreter else { return };
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
            let Some(interp) = &mut self.interpreter else { return };
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
            self.cached_style_map = None;
            self.cached_layout_root = None;
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
                TabClose(idx) => {
                    self.tabs.close(idx);
                    let t = self.tabs.active_tab().clone();
                    self.html = t.html;
                    self.css = t.css;
                    self.base_url = t.url;
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                }
                TabCloseOthers(idx) => {
                    let keep = self.tabs.tabs[idx].clone();
                    self.tabs.tabs.clear();
                    self.tabs.tabs.push(keep);
                    self.tabs.active = 0;
                    let t = self.tabs.active_tab().clone();
                    self.html = t.html;
                    self.css = t.css;
                    self.base_url = t.url;
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                }
                TabDuplicate(idx) => {
                    let dup = self.tabs.tabs[idx].clone();
                    self.tabs.open(dup);
                }
                TabSetGroup(idx, color) => {
                    if let Some(t) = self.tabs.tabs.get_mut(idx) {
                        t.group_color = color;
                    }
                }
                TabPinToggle(idx) => {
                    if let Some(t) = self.tabs.tabs.get_mut(idx) {
                        t.pinned = !t.pinned;
                    }
                    // Sort: pinned first.
                    self.tabs.tabs.sort_by(|a, b| b.pinned.cmp(&a.pinned));
                    if let Some(active_url) = self.base_url.clone() {
                        if let Some(new_pos) = self.tabs.tabs.iter().position(|t| t.url.as_ref() == Some(&active_url)) {
                            self.tabs.active = new_pos;
                        }
                    }
                }
                TabReload(idx) => {
                    if let Some(url) = self.tabs.tabs[idx].url.clone() {
                        let active_was = self.tabs.active;
                        self.switch_tab_with_swap(idx);
                        self.navigate_url_no_history(&url);
                        self.switch_tab_with_swap(active_was);
                    }
                }
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
                    if let Some(interp) = &self.interpreter {
                        let root = std::rc::Rc::clone(&interp.document.borrow().root);
                        if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, node_id) {
                            let txt = node.tag_name().unwrap_or_default();
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(txt);
                            }
                        }
                    }
                }
                ScrollIntoView { node_id } => {
                    if let Some(layout) = &self.layout_root {
                        if let Some(bx) = crate::browser::devtools_panel::find_layout_box(layout, node_id) {
                            self.scroll_target_y = bx.rect.y - 50.0;
                            if self.scroll_target_y < 0.0 { self.scroll_target_y = 0.0; }
                        }
                    }
                }
                ExpandAll { node_id } => {
                    let mut to_expand = Vec::new();
                    collect_subtree_ids(&self.devtools.elements.rows, node_id, &mut to_expand);
                    for id in to_expand {
                        self.devtools.elements.collapsed.remove(&id);
                    }
                    if let Some(interp) = &self.interpreter {
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
                    if let Some(interp) = &self.interpreter {
                        let root = std::rc::Rc::clone(&interp.document.borrow().root);
                        crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                    }
                }
                ClearConsole => {
                    self.devtools.console.log.clear();
                    if let Some(interp) = &self.interpreter {
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
                    } else if let Some(interp) = &self.interpreter {
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
                        .or_else(|| self.interpreter.as_ref()
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
            if let Some(interp) = &self.interpreter {
                let root = std::rc::Rc::clone(&interp.document.borrow().root);
                let hits = crate::devtools::search::search(&root, &q, mode);
                self.devtools.elements.search.matches = hits;
            }
        }

        fn jump_to_search_match(&mut self) {
            let s = &self.devtools.elements.search;
            if let Some(node_id) = s.matches.get(s.current) {
                self.devtools.elements.selected = Some(*node_id);
                // Expand vsechny ancestors aby radek byl viditelny.
                if let Some(interp) = &self.interpreter {
                    let root = std::rc::Rc::clone(&interp.document.borrow().root);
                    if let Some(node) = crate::devtools::model::elements::find_node_by_id(&root, *node_id) {
                        let mut p = node.parent.borrow().upgrade();
                        while let Some(par) = p {
                            let pid = std::rc::Rc::as_ptr(&par) as usize;
                            self.devtools.elements.collapsed.remove(&pid);
                            p = par.parent.borrow().upgrade();
                        }
                    }
                    crate::browser::devtools_panel::rebuild_tree(&mut self.devtools, &root);
                }
            }
        }

        fn handle_click(&mut self, x: f32, y: f32) {
            // Hit-test pres open dropdown popup PRED layout hit_test.
            if let Some((select_id, anchor_x, anchor_y, anchor_w)) = self.open_select {
                let popup_x = anchor_x;
                let popup_y = anchor_y + 24.0; // y v page-space (bez -scroll); klik je page-space.
                let opt_h = 24.0_f32;
                if let Some(interp) = &self.interpreter {
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
            let layout_root = match &self.layout_root { Some(l) => l, None => return };
            let interp = match &mut self.interpreter { Some(i) => i, None => return };

            // Hit test - najdi cilovy LayoutBox
            let target = layout_root.hit_test(x, y);
            if let Some(target) = target {
                if let Some(node) = &target.node {
                    // Centralni klasifikace pres InteractiveKind misto ad-hoc tag matchu.
                    let tag = node.tag_name();
                    let kind = crate::browser::interactive::classify(node);
                    use crate::browser::interactive::InteractiveKind;
                    let is_focusable = kind.is_focusable() || node.attr("tabindex").is_some();
                    if is_focusable {
                        super::cascade::set_focused_node(Some(std::rc::Rc::as_ptr(node) as usize));
                    } else {
                        super::cascade::set_focused_node(None);
                    }
                    // Form submit: kind=Button + type=submit/button (button default
                    // = submit). Driv ad-hoc match na tag.
                    let is_submit_button = matches!(kind, InteractiveKind::Button)
                        && (tag.as_deref() == Some("button")
                            || node.attr("type").as_deref().map(|t| t.eq_ignore_ascii_case("submit")).unwrap_or(false));
                    if is_submit_button {
                        if let Some(form) = find_ancestor_form(node) {
                            // Dispatch 'submit' event - browsery to delaji pred navigation.
                            // Pri preventDefault listener -> skip navigate.
                            let prevented;
                            {
                                let mut event = crate::interpreter::JsObject::new();
                                event.set("type".into(),
                                    crate::interpreter::JsValue::Str("submit".into()));
                                event.set("target".into(),
                                    crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(&form)));
                                event.set("currentTarget".into(),
                                    crate::interpreter::JsValue::DomNode(std::rc::Rc::clone(&form)));
                                event.set("bubbles".into(), crate::interpreter::JsValue::Bool(true));
                                event.set("cancelable".into(), crate::interpreter::JsValue::Bool(true));
                                let prevent_flag = std::rc::Rc::new(std::cell::RefCell::new(false));
                                let pf = std::rc::Rc::clone(&prevent_flag);
                                event.set("preventDefault".into(),
                                    crate::interpreter::helpers::native("preventDefault", move |_| {
                                        *pf.borrow_mut() = true;
                                        Ok(crate::interpreter::JsValue::Undefined)
                                    }));
                                event.set("stopPropagation".into(),
                                    crate::interpreter::helpers::native("stopPropagation",
                                        |_| Ok(crate::interpreter::JsValue::Undefined)));
                                event.set("defaultPrevented".into(),
                                    crate::interpreter::JsValue::Bool(false));
                                let event_val = crate::interpreter::JsValue::Object(
                                    std::rc::Rc::new(std::cell::RefCell::new(event)));
                                let _ = interp.dispatch_event(&form, "submit", event_val);
                                prevented = *prevent_flag.borrow();
                            }
                            if prevented {
                                println!("[form submit] prevented by listener");
                                return;
                            }
                            if let Some((url, method, body)) = build_form_request(&form, self.base_url.as_deref()) {
                                println!("[form {} submit] {url}", method);
                                if method == "post" {
                                    let body_str = body.unwrap_or_default();
                                    if let Some(html) = post_form(&url, &body_str) {
                                        // Replace HTML s response.
                                        self.html = html;
                                        self.css = String::new();
                                        self.base_url = Some(url.clone());
                                        self.scroll_y = 0.0;
            self.scroll_target_y = 0.0;
            self.scroll_x = 0.0;
            self.scroll_target_x = 0.0;
                                        let mut interp = crate::interpreter::Interpreter::new();
                                        let doc = super::html_parser::parse_html(&self.html, &url);
                                        interp.set_document(doc);
                                        self.run_inline_scripts(&mut interp);
                                        self.interpreter = Some(interp);
                                        if let Some(w) = &self.window { w.set_title(&format_window_title(&url, self.tabs.tabs.len())); }
                                        self.render();
                                    }
                                } else {
                                    self.navigate_url(&url);
                                }
                                return;
                            }
                        }
                    }
                    // Per-kind dispatch pres InteractiveKind klasifikaci.
                    match kind {
                        InteractiveKind::Select => {
                            let id = std::rc::Rc::as_ptr(node) as usize;
                            let same = self.open_select.map(|(t, ..)| t == id).unwrap_or(false);
                            if same {
                                self.open_select = None;
                            } else {
                                self.open_select = Some((id, target.rect.x, target.rect.y, target.rect.width));
                            }
                            self.render();
                            return;
                        }
                        InteractiveKind::Link => {
                            if let Some(href) = node.attr("href") {
                                if !href.starts_with('#') {
                                    let url = match &self.base_url {
                                        Some(b) => resolve_url(b, &href),
                                        None => href.clone(),
                                    };
                                    println!("[link] {url}");
                                    self.navigate_url(&url);
                                    return;
                                }
                            }
                        }
                        InteractiveKind::Checkbox | InteractiveKind::Radio => {
                            // Toggle checked attr.
                            let was = node.attr("checked").is_some();
                            if matches!(kind, InteractiveKind::Radio) {
                                // Radio - uncheck siblings se stejnym name.
                                let name = node.attr("name").unwrap_or_default();
                                if let Some(form) = find_ancestor_form(node) {
                                    fn walk_uncheck(n: &std::rc::Rc<crate::browser::dom::Node>, name: &str) {
                                        if n.tag_name().as_deref() == Some("input")
                                            && n.attr("type").as_deref().map(|t| t.eq_ignore_ascii_case("radio")).unwrap_or(false)
                                            && n.attr("name").as_deref() == Some(name) {
                                            n.attributes.borrow_mut().remove("checked");
                                        }
                                        for c in n.children.borrow().iter() { walk_uncheck(c, name); }
                                    }
                                    walk_uncheck(&form, &name);
                                }
                            }
                            if was && !matches!(kind, InteractiveKind::Radio) {
                                node.attributes.borrow_mut().remove("checked");
                            } else {
                                node.attributes.borrow_mut().insert("checked".into(), "checked".into());
                            }
                            self.cached_layout_root = None;
                            // render() volat az po fall-through dispatch click listeners
                            // (interp je borrowed mut, render volat mimo blok).
                        }
                        _ => {}
                    }
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
            } else {
                // Klik mimo - clear focus.
                super::cascade::set_focused_node(None);
            }
        }

        /// Write arbitrary style value (z editing buffer) na selected node inline style.
        /// Swap interpreter mezi App.interpreter a target_tab.stored_interpreter.
        /// Stash aktualni interp do soucasneho aktivniho tabu pred prepnutim.
        /// Cilove tab dostane App.interpreter (presune se z stored_interpreter).
        /// Pri prvni navsteve target_tab neni stored_interp -> App.interpreter
        /// = None, nasledne navigate_url ho vytvori.
        fn switch_tab_with_swap(&mut self, target_idx: usize) {
            if target_idx >= self.tabs.tabs.len() { return; }
            if target_idx == self.tabs.active { return; }
            let prev = self.tabs.active;
            // Stash current interp do prev tabu (pokud existuje).
            if let Some(interp) = self.interpreter.take() {
                self.tabs.stash_interpreter(prev, Box::new(interp));
            }
            self.tabs.switch_to(target_idx);
            // Take target's stored interp (pokud existuje).
            if let Some(boxed) = self.tabs.take_target_interpreter(target_idx) {
                self.interpreter = Some(*boxed);
            }
            // Cache invalidace - nove DOM/document.
            self.cached_layout_root = None;
            self.cached_style_map = None;
            self.cached_pseudo_map = None;
            self.cached_matched_key = None;
        }

        /// Ctrl+S = save current page HTML do $HOME/Downloads + zaznam do downloads tracker.
        fn save_page_to_downloads(&mut self) {
            let dl_dir = std::env::var("USERPROFILE").ok()
                .or_else(|| std::env::var("HOME").ok())
                .map(|h| std::path::PathBuf::from(h).join("Downloads"));
            let Some(dir) = dl_dir else { return };
            let _ = std::fs::create_dir_all(&dir);
            let url = self.base_url.clone().unwrap_or_else(|| "page".to_string());
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
            if let Err(e) = std::fs::write(&path, &self.html) {
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
            if let Some(interp) = &self.interpreter {
                let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                if let Some(node) = find_node_by_ptr(&doc_root, node_id) {
                    let mut attrs = node.attributes.borrow_mut();
                    let cur_style = attrs.get("style").cloned().unwrap_or_default();
                    let new_style = update_inline_style(&cur_style, prop, value);
                    let old_value = cur_style.clone();
                    attrs.insert("style".to_string(), new_style.clone());
                    drop(attrs);
                    self.cached_layout_root = None;
                    self.cached_cascade_hash = 0;
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
            if let Some(interp) = &self.interpreter {
                let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                if let Some(node) = find_node_by_ptr(&doc_root, node_id) {
                    let mut attrs = node.attributes.borrow_mut();
                    let cur_style = attrs.get("style").cloned().unwrap_or_default();
                    // Replace prop value v inline style nebo append.
                    let new_style = update_inline_style(&cur_style, &prop, &hex);
                    let old_value = cur_style.clone();
                    attrs.insert("style".to_string(), new_style.clone());
                    drop(attrs);
                    self.cached_layout_root = None;
                    self.cached_cascade_hash = 0;
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

        fn navigate_about(&mut self, url: &str) -> bool {
            // Internal pages: about:newtab, about:config.
            match url {
                "about:newtab" => {
                    let (html, css) = crate::browser::render::tabs::render_about_newtab();
                    self.html = html;
                    self.css = css;
                    self.base_url = Some("about:newtab".to_string());
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                    true
                }
                "about:config" => {
                    let (html, css) = crate::browser::render::tabs::render_about_config();
                    self.html = html;
                    self.css = css;
                    self.base_url = Some("about:config".to_string());
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                    true
                }
                "about:history" => {
                    let (html, css) = crate::browser::render::tabs::render_about_history();
                    self.html = html;
                    self.css = css;
                    self.base_url = Some("about:history".to_string());
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                    true
                }
                "about:bookmarks" => {
                    let (html, css) = crate::browser::render::tabs::render_about_bookmarks();
                    self.html = html;
                    self.css = css;
                    self.base_url = Some("about:bookmarks".to_string());
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                    true
                }
                "about:about" => {
                    let (html, css) = crate::browser::render::tabs::render_about_about();
                    self.html = html;
                    self.css = css;
                    self.base_url = Some("about:about".to_string());
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                    true
                }
                "about:downloads" => {
                    let (html, css) = crate::browser::render::tabs::render_about_downloads();
                    self.html = html;
                    self.css = css;
                    self.base_url = Some("about:downloads".to_string());
                    self.cached_layout_root = None;
                    self.cached_stylesheets = None;
                    true
                }
                _ => false,
            }
        }

        /// Navigate s history push (smaze forward history pri navigaci).
        fn navigate_url(&mut self, url: &str) {
            // Truncate forward history.
            self.history.truncate(self.history_idx + 1);
            self.history.push(url.to_string());
            self.history_idx = self.history.len() - 1;
            self.navigate_url_no_history(url);
            // Persist v profile history (~/.rwe/profiles/<active>/history.json).
            // Pouzij realny <title> tagu (z aktivniho tab po nav), fallback URL last segment.
            let title = self.tabs.active_tab().title.clone();
            crate::devtools::history::append_entry(&crate::devtools::history::HistoryEntry {
                url: url.to_string(),
                title,
                visited_at: crate::devtools::history::now_ts(),
            });
        }

        /// Navigate bez modifikace history (back/forward use this).
        fn navigate_url_no_history(&mut self, url: &str) {
            // Loading flag: zobrazi spinner v tab chip behem fetch.
            self.tabs.active_tab_mut().loading = true;
            // about: URL handled internally.
            if url.starts_with("about:") {
                if self.navigate_about(url) {
                    self.tabs.active_tab_mut().loading = false;
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
                self.html = html;
                self.css = css;
                self.base_url = Some(url.to_string());
                self.current_path = None;
                self.scroll_y = 0.0;
            self.scroll_target_y = 0.0;
            self.scroll_x = 0.0;
            self.scroll_target_x = 0.0;
                self.start_time = std::time::Instant::now();
                self.prev_style_map = None;
                self.active_animations.clear();
                self.animation_iterations.clear();
                self.active_transitions.clear();
                let mut interp = crate::interpreter::Interpreter::new();
                let doc = super::html_parser::parse_html(&self.html, url);
                interp.set_document(doc);
                self.run_inline_scripts(&mut interp);
                self.interpreter = Some(interp);
                let page_title = crate::browser::render::tabs::extract_title(&self.html)
                    .unwrap_or_else(|| url.to_string());
                {
                    let cur = self.tabs.active_tab_mut();
                    cur.html = self.html.clone();
                    cur.css = self.css.clone();
                    cur.url = Some(url.to_string());
                    cur.title = page_title.clone();
                    cur.loading = false;
                }
                if let Some(w) = &self.window {
                    w.set_title(&format_window_title(&page_title, self.tabs.tabs.len()));
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
            let layout_root = match &self.layout_root { Some(l) => l, None => return };
            let target = layout_root.hit_test(self.mouse_x, self.mouse_y);
            let id = target.and_then(|t| t.node.as_ref().map(|n| std::rc::Rc::as_ptr(n) as usize));
            super::cascade::set_hovered_node(id);
            // Status bar URL preview pri hover na link.
            self.status_hover_url = None;
            if let Some(t) = target {
                if let Some(node) = &t.node {
                    if node.tag_name().as_deref() == Some("a") {
                        if let Some(href) = node.attr("href") {
                            self.status_hover_url = Some(href);
                        }
                    }
                }
            }
            // Tooltip update: hover nad swatch -> hex string, var chip -> name.
            self.devtools.tooltip = None;
            if self.devtools.panel_open {
                let mx_screen = self.mouse_x - self.scroll_x;
                let my_screen = self.mouse_y - self.scroll_y;
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
            let mx_screen = self.mouse_x - self.scroll_x;
            let my_screen = self.mouse_y - self.scroll_y;
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
                self.devtools.elements.hovered = id;
            } else if tree_hover_id.is_some() {
                self.devtools.elements.hovered = tree_hover_id;
            } else if self.devtools.elements.hovered.is_some() {
                self.devtools.elements.hovered = None;
            }
            // Cursor icon stack - jeden compute_cursor_icon() s prioritou:
            // 1. Devtools panel? -> dle hit_test (search/console = Text, scrollbar/btn = Default/Pointer)
            // 2. Page form button/link/select -> Pointer
            // 3. Page input/textarea -> Text
            // 4. Page text run -> Text
            // 5. Default
            if let Some(window) = &self.window {
                let icon = self.compute_cursor_icon(target);
                window.set_cursor(icon);
            }
        }

        fn render(&mut self) {
            use super::{css_parser, cascade, layout, paint};
            let frame_start = std::time::Instant::now();
            let _perf_debug = std::env::var("PERF_DEBUG").is_ok();
            let perf_t = |label: &str, t: std::time::Instant| {
                if std::env::var("PERF_DEBUG").is_ok() {
                    eprintln!("[perf] {} {:.2} ms", label, t.elapsed().as_secs_f64() * 1000.0);
                }
            };
            let _ = perf_t;
            // Hybrid debug mode: poll worker events + sync state.
            self.poll_debug_runner();
            // Sync devtools breakpoints -> interpreter debugger.
            // Pri zmene state.sources.breakpoints (klik gutter), prepocitej set linies
            // pro current selected file a propa do interpreter.debugger.
            if let Some(interp) = &self.interpreter {
                let bp_lines: std::collections::HashSet<u32> = self.devtools.sources.breakpoints.iter()
                    .filter_map(|b| {
                        // Kdyz file_id zobrazeneho zdroje je breakpoint.file_id, hodna line.
                        // Pro ted bereme vsechny breakpoints (bez file mapping).
                        Some(b.line)
                    })
                    .collect();
                let mut dbg = interp.debugger.borrow_mut();
                if dbg.breakpoints != bp_lines {
                    dbg.breakpoints = bp_lines;
                }
                // Mirror paused_at -> devtools UI.
                if let Some(line) = dbg.paused_at {
                    if let Some(file_id) = self.devtools.sources.selected_id {
                        self.devtools.sources.current_pause_location = Some((file_id, line));
                        self.devtools.sources.debugger_paused = true;
                        self.devtools.sources.locals = dbg.locals.clone();
                    }
                } else {
                    self.devtools.sources.debugger_paused = false;
                    self.devtools.sources.locals.clear();
                }
            }
            // Mirror interpreter console_log do DevToolsState (jen nove entries).
            // Drz running counter v DevToolsState pres console.log.len() porovnani.
            if let Some(interp) = &self.interpreter {
                let logs = interp.console_log.borrow();
                let already = self.devtools.console.log.len();
                if logs.len() > already {
                    use crate::devtools::model::console::{LogEntry, LogLevel};
                    for (level, msg) in logs.iter().skip(already) {
                        let lvl = match level.as_str() {
                            "error" => LogLevel::Error,
                            "warn" => LogLevel::Warn,
                            _ => LogLevel::Info,
                        };
                        self.devtools.console.log.push(LogEntry { level: lvl, text: msg.clone() });
                    }
                    self.devtools.console.stick_to_bottom = true;
                }
            }
            // Smooth scroll tick: interpoluje scroll_y -> scroll_target_y. Pokud
            // stale animuje, na konci request_redraw pro pokracovani.
            let _scroll_animating = self.smooth_scroll_tick();
            // Extract page selection anchor/current pred renderer borrow
            // (page_sel_* metody borrowuji self.interpreter immutably).
            let self_page_sel_anchor = self.page_sel_anchor();
            let self_page_sel_current = self.page_sel_current();
            let r = match &mut self.renderer { Some(r) => r, None => return };
            // Push zoom faktor do rendereru pro vp uniform skalovani.
            r.zoom = self.zoom;

            // Pouzij document z interpreteru (po JS modifikacich)
            let document_root = match &self.interpreter {
                Some(i) => Rc::clone(&i.document.borrow().root),
                None => return,
            };

            // CSS hash pro cache invalidation.
            let css_hash = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                self.css.hash(&mut h);
                h.finish()
            };
            // Reading mode injected CSS pridano k page CSS pokud zaple.
            let reading_css = if self.reading_mode_on { READING_MODE_CSS } else { "" };
            let combined_hash = if self.reading_mode_on {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                self.css.hash(&mut h);
                "rmode".hash(&mut h);
                h.finish()
            } else { css_hash };
            if self.cached_stylesheets.is_none() || self.cached_stylesheets_hash != combined_hash {
                let combined = if self.reading_mode_on {
                    format!("{}\n{}", self.css, reading_css)
                } else { self.css.clone() };
                let parsed = vec![css_parser::parse_stylesheet(&combined)];
                for sheet in &parsed {
                    r.load_font_faces(&sheet.font_faces, self.base_url.as_deref());
                }
                // Detect if any keyframes animate layout-affecting properties.
                // EXACT match - drive starts_with("border") matchovalo "border-color"
                // (paint-only, ne layout) a triggrovalo false-positive cache invalidace.
                fn is_layout_affecting_prop(p: &str) -> bool {
                    matches!(p,
                        "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height"
                        | "padding" | "padding-top" | "padding-right" | "padding-bottom" | "padding-left"
                        | "padding-block" | "padding-block-start" | "padding-block-end"
                        | "padding-inline" | "padding-inline-start" | "padding-inline-end"
                        | "margin" | "margin-top" | "margin-right" | "margin-bottom" | "margin-left"
                        | "margin-block" | "margin-block-start" | "margin-block-end"
                        | "margin-inline" | "margin-inline-start" | "margin-inline-end"
                        | "border-width" | "border-top-width" | "border-right-width"
                        | "border-bottom-width" | "border-left-width"
                        | "font-size" | "font" | "line-height"
                        | "gap" | "row-gap" | "column-gap" | "grid-gap"
                        | "flex" | "flex-basis" | "flex-grow" | "flex-shrink" | "flex-direction" | "flex-wrap"
                        | "grid" | "grid-template-columns" | "grid-template-rows" | "grid-template-areas"
                        | "grid-auto-columns" | "grid-auto-rows" | "grid-auto-flow"
                        | "grid-area" | "grid-column" | "grid-row"
                        | "grid-column-start" | "grid-column-end" | "grid-row-start" | "grid-row-end"
                        | "top" | "left" | "right" | "bottom" | "inset"
                        | "position" | "display" | "float" | "clear"
                        | "writing-mode" | "direction"
                        | "box-sizing" | "white-space"
                        | "column-count" | "column-width" | "columns")
                }
                fn is_width_height_prop(p: &str) -> bool {
                    matches!(p, "width" | "height" | "min-width" | "max-width"
                                | "min-height" | "max-height")
                }
                self.layout_affecting_animations.clear();
                self.width_height_only_animations.clear();
                for sheet in &parsed {
                    for kf in &sheet.keyframes {
                        let mut has_layout_aff = false;
                        let mut has_other_layout_aff = false; // layout-aff but NOT width/height
                        for (_, decls) in &kf.frames {
                            for d in decls {
                                if is_layout_affecting_prop(&d.property) {
                                    has_layout_aff = true;
                                    if !is_width_height_prop(&d.property) {
                                        has_other_layout_aff = true;
                                    }
                                }
                            }
                        }
                        if has_layout_aff {
                            self.layout_affecting_animations.insert(kf.name.clone());
                            if !has_other_layout_aff {
                                // Animuje POUZE width/height - kandidat na soft layout
                                // (incremental: aplikujeme na rect, ne re-layout).
                                self.width_height_only_animations.insert(kf.name.clone());
                            }
                        }
                    }
                }
                self.animations_affect_layout = !self.layout_affecting_animations.is_empty();
                // Detekuj zda CSS obsahuje :hover/:focus selektory. Pokud ne,
                // hover/focus state nema vliv na cascade -> skip re-cascade pri
                // hover change.
                self.css_uses_hover = self.css.contains(":hover");
                self.css_uses_focus = self.css.contains(":focus");
                self.css_uses_transitions = self.css.contains("transition");
                self.css_uses_keyframes = self.css.contains("@keyframes")
                    || self.css.contains("animation:")
                    || self.css.contains("animation-name");
                // Detect viewport-dependent queries: @media, @container, vw/vh units.
                self.css_uses_viewport = self.css.contains("@media")
                    || self.css.contains("@container")
                    || self.css.contains("vw") || self.css.contains("vh")
                    || self.css.contains("vmin") || self.css.contains("vmax")
                    || self.css.contains("dvh") || self.css.contains("dvw")
                    || self.css.contains("svw") || self.css.contains("svh")
                    || self.css.contains("lvw") || self.css.contains("lvh");
                self.cached_stylesheets = Some(parsed);
                self.cached_stylesheets_hash = combined_hash;
                self.cached_style_map = None;
                self.cached_pseudo_map = None;
                self.cached_layout_root = None;
            }
            let stylesheets = self.cached_stylesheets.as_ref().unwrap();
            let cascade_hash = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (Rc::as_ptr(&document_root) as usize).hash(&mut h);
                css_hash.hash(&mut h);
                ((self.zoom * 1000.0) as i64).hash(&mut h);
                // Viewport jen pri uses_viewport (resize lag fix). Bez @media/vh
                // cascade neni viewport-dependent -> cache zustava valid pri resize.
                if self.css_uses_viewport {
                    (r.config.width as u64).hash(&mut h);
                    (r.config.height as u64).hash(&mut h);
                }
                if self.css_uses_hover {
                    cascade::get_hovered_node().unwrap_or(0).hash(&mut h);
                }
                if self.css_uses_focus {
                    cascade::get_focused_node().unwrap_or(0).hash(&mut h);
                }
                h.finish()
            };
            if self.cached_style_map.is_none() || self.cached_cascade_hash != cascade_hash {
                // Cascade s viewport pro @media + @container queries.
                let vw_logical = (r.config.width as f32) / (self.zoom * r.scale_factor);
                let vh_logical = (r.config.height as f32) / (self.zoom * r.scale_factor);
                self.cached_style_map = Some(Rc::new(cascade::cascade_with_viewport(
                    &document_root, stylesheets, vw_logical, vh_logical)));
                self.cached_pseudo_map = Some(cascade::cascade_pseudo(&document_root, stylesheets));
                self.cached_cascade_hash = cascade_hash;
            }
            let _t_clone = std::time::Instant::now();
            // PERF: Rc::clone misto deep HashMap clone. Cheap pointer copy.
            // Mutace pres Rc::make_mut (deep clones jen kdyz Rc shared > 1).
            let mut style_map: Rc<super::cascade::StyleMap> = Rc::clone(self.cached_style_map.as_ref().unwrap());
            perf_t("style_map clone (Rc)", _t_clone);
            // PERF: zachovat ref na cached pseudo map - pouzivame v layout_tree
            // call jako &PseudoStyleMap. Drive clone() byla zbytecna kazdy frame.
            let empty_pseudo: super::cascade::PseudoStyleMap = Default::default();
            let pseudo_map: &super::cascade::PseudoStyleMap = self.cached_pseudo_map.as_ref().unwrap_or(&empty_pseudo);

            // Wire computed styles + matched rules do DevTools state pri selected element.
            // PERF: cele toto je drahe (walk vsech rules x selectors); cache pres
            // (selected, cascade_hash) - bez zmeny selection ani cascade nevolat znovu.
            // Driv: tato cesta byla volana kazdy frame pri active_animations -> lag.
            let matched_cache_key = (self.devtools.elements.selected, cascade_hash);
            let need_rebuild_matched = self.cached_matched_key != Some(matched_cache_key);
            if let Some(sel) = self.devtools.elements.selected {
                if let Some(decl_map) = style_map.get(&sel) {
                    let mut entries: Vec<(String, String)> = decl_map.iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    entries.sort_by(|a, b| a.0.cmp(&b.0));
                    self.devtools.styles.computed = entries;
                } else {
                    self.devtools.styles.computed.clear();
                }
                // Matched rules: walk stylesheets - drahy O(rules*selectors).
                // Cache pres (selected, cascade_hash) - skip kdyz nezmenilo se.
                if need_rebuild_matched {
                if let Some(node) = find_node_by_ptr(&document_root, sel) {
                    use crate::devtools::model::styles::{MatchedRule, RuleSource, RuleDecl};
                    let mut matched: Vec<MatchedRule> = Vec::new();
                    // 1. Direct match na selected node. Dedup pres (selector_str + decls_hash + source).
                    let mut seen_keys: std::collections::HashSet<String> = Default::default();
                    for (sheet_idx, sheet) in stylesheets.iter().enumerate() {
                        for rule in &sheet.rules {
                            for sel_obj in &rule.selectors {
                                if super::cascade::matches_selector(&node, sel_obj) {
                                    let sel_str = sel_obj.to_string();
                                    // Skladani decls hash pres property=value parovani.
                                    let decls_repr: String = rule.declarations.iter()
                                        .map(|d| format!("{}={};", d.property, d.value))
                                        .collect();
                                    let key = format!("{}|{}|{}", sheet_idx, sel_str, decls_repr);
                                    if seen_keys.insert(key) {
                                        let decls: Vec<RuleDecl> = rule.declarations.iter()
                                            .map(|d| RuleDecl {
                                                property: d.property.clone(),
                                                value: d.value.clone(),
                                                important: d.important,
                                                overridden: false,
                                            }).collect();
                                        matched.push(MatchedRule {
                                            selector: sel_str,
                                            source: RuleSource::StyleBlock { index: sheet_idx },
                                            specificity: 0,
                                            declarations: decls,
                                            inherited_from: None,
                                        });
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    // 2. Inherited rules: walk parent chain, najdi pravidla
                    // ktera matchuji ancestor + maji inheritable property.
                    fn is_inheritable(prop: &str) -> bool {
                        matches!(prop,
                            "color" | "font-family" | "font-size" | "font-weight"
                            | "font-style" | "font-variant" | "font-stretch"
                            | "line-height" | "letter-spacing" | "word-spacing"
                            | "text-align" | "text-indent" | "text-transform"
                            | "white-space" | "visibility" | "cursor" | "direction"
                            | "list-style" | "list-style-type" | "list-style-position"
                            | "list-style-image" | "border-collapse" | "border-spacing"
                            | "caption-side" | "empty-cells" | "quotes" | "tab-size"
                            | "writing-mode" | "orphans" | "widows" | "hyphens"
                            | "word-break" | "word-wrap" | "overflow-wrap")
                    }
                    let mut current = node.parent.borrow().upgrade();
                    while let Some(parent) = current {
                        let parent_tag = parent.tag_name().unwrap_or_else(|| "node".into());
                        for (sheet_idx, sheet) in stylesheets.iter().enumerate() {
                            for rule in &sheet.rules {
                                for sel_obj in &rule.selectors {
                                    if super::cascade::matches_selector(&parent, sel_obj) {
                                        let sel_str = sel_obj.to_string();
                                        let inh_decls: Vec<RuleDecl> = rule.declarations.iter()
                                            .filter(|d| is_inheritable(&d.property))
                                            .map(|d| RuleDecl {
                                                property: d.property.clone(),
                                                value: d.value.clone(),
                                                important: d.important,
                                                overridden: false,
                                            }).collect();
                                        if !inh_decls.is_empty() {
                                            let decls_repr: String = inh_decls.iter()
                                                .map(|d| format!("{}={};", d.property, d.value))
                                                .collect();
                                            let key = format!("inh:{}|{}|{}|{}",
                                                parent_tag, sheet_idx, sel_str, decls_repr);
                                            if seen_keys.insert(key) {
                                                matched.push(MatchedRule {
                                                    selector: sel_str,
                                                    source: RuleSource::StyleBlock { index: sheet_idx },
                                                    specificity: 0,
                                                    declarations: inh_decls,
                                                    inherited_from: Some(parent_tag.clone()),
                                                });
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        current = parent.parent.borrow().upgrade();
                    }
                    self.devtools.styles.matched_rules = matched;
                } else {
                    self.devtools.styles.matched_rules.clear();
                }
                self.cached_matched_key = Some(matched_cache_key);
                } // end need_rebuild_matched
            } else {
                self.devtools.styles.computed.clear();
                self.devtools.styles.matched_rules.clear();
                self.cached_matched_key = None;
            }
            // Populate @font-face declarations.
            let mut faces = Vec::new();
            for sheet in stylesheets.iter() {
                for ff in &sheet.font_faces {
                    faces.push((ff.family.clone(), ff.src.clone(),
                                ff.weight.clone(), ff.style.clone()));
                }
            }
            self.devtools.styles.font_faces = faces;

            // Animation-effective time: respektuje pause + speed multiplikator
            // z devtools Animations panelu. Bez pause = real elapsed * speed.
            let elapsed = {
                let now = std::time::Instant::now();
                let raw_now = if self.devtools.animations_paused {
                    self.animation_pause_start.unwrap_or(now)
                } else { now };
                raw_now.duration_since(self.animation_origin).as_secs_f32()
                    * self.devtools.animations_speed
            };

            // Drainuj WebSocket events kazdy frame (dispatch onopen/onmessage/onerror/onclose).
            if let Some(interp) = &mut self.interpreter {
                let _ = interp.drain_websockets();
                interp.drain_fetches();
            }
            // Drain async background jobs (image lazy load, file IO, etc).
            self.async_jobs.drain();
            if let Some(interp) = &mut self.interpreter {
                let _ = interp; // re-borrow scope
            }

            // CSS Transitions: detekuj zmeny vs prev_style_map a vyrob aktivni transitions.
            // PERF: skip kompletne kdyz CSS neobsahuje "transition" - prev_style_map
            // je tezky clone (5000+ HashMap entries) a detect_transitions O(N) walk
            // by stejne nasel zadne diffs.
            let mut ended_transitions: Vec<(usize, String)> = Vec::new();
            if self.css_uses_transitions {
                if let Some(prev) = &self.prev_style_map {
                    let active_before = std::mem::take(&mut self.active_transitions);
                    let prev_keys: std::collections::HashSet<(usize, String)> = active_before.iter()
                        .map(|t| (t.node_id, t.property.clone())).collect();
                    self.active_transitions = cascade::detect_transitions(&**prev, &*style_map, active_before, elapsed);
                    let now_keys: std::collections::HashSet<(usize, String)> = self.active_transitions.iter()
                        .map(|t| (t.node_id, t.property.clone())).collect();
                    for k in prev_keys.difference(&now_keys) {
                        ended_transitions.push(k.clone());
                    }
                }
            }
            // Aplikuj transitions jen kdyz nejake aktivni (skip cely walk pri prazdnem).
            if !self.active_transitions.is_empty() {
                cascade::apply_transitions(Rc::make_mut(&mut style_map), &self.active_transitions, elapsed);
            }

            // Dispatch transitionend events
            for (node_id, prop) in &ended_transitions {
                if let Some(interp) = &mut self.interpreter {
                    let doc_root = Rc::clone(&interp.document.borrow().root);
                    if let Some(target) = find_node_by_ptr(&doc_root, *node_id) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(),
                            crate::interpreter::JsValue::Str("transitionend".into()));
                        event.set("propertyName".into(),
                            crate::interpreter::JsValue::Str(prop.clone()));
                        event.set("target".into(),
                            crate::interpreter::JsValue::DomNode(Rc::clone(&target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            std::rc::Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(&target, "transitionend", event_val);
                    }
                }
            }

            // Runtime CSS animation: skip cely walk pokud zadne keyframes neexistuji.
            let has_keyframes = stylesheets.iter().any(|s| !s.keyframes.is_empty());
            if has_keyframes {
                let _animating = cascade::apply_animations(Rc::make_mut(&mut style_map), stylesheets, elapsed);
                // Per-element pause: aplikuj frozen snapshot (zachytava presnou
                // fazi pri toggle). Pri prvni paused frame neni snapshot - vezmi
                // current animated styl + uloz pro dalsi framy.
                let paused_ids: Vec<usize> = self.paused_animation_nodes.iter().copied().collect();
                for id in paused_ids {
                    if let Some(snap) = self.paused_node_styles.get(&id) {
                        Rc::make_mut(&mut style_map).insert(id, snap.clone());
                    } else if let Some(cur) = style_map.get(&id).cloned() {
                        // First-time pause - snapshot animated styl ted.
                        self.paused_node_styles.insert(id, cur);
                    }
                }
                let max_scroll = (style_map.len() as f32).max(1.0);
                let scroll_progress = if max_scroll > 1.0 { self.scroll_y / max_scroll.max(1.0) } else { 0.0 };
                let _ = cascade::apply_scroll_animations(Rc::make_mut(&mut style_map), stylesheets, scroll_progress);
            }

            // Detect animation start/end + iteration events.
            // Skip cely walk pri zadnych keyframes - test na to ze stranka vubec nema animations.
            let mut current_anims: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
            let mut iter_events: Vec<(usize, String, i32)> = Vec::new();
            if has_keyframes {
                for (node_id, styles) in &*style_map {
                    if let Some(spec) = cascade::AnimationSpec::from_styles(styles) {
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

            // Dispatch animationstart / animationend events
            for (event_type, list) in [("animationstart", started), ("animationend", ended_anims)] {
                for (node_id, name) in list {
                    if let Some(interp) = &mut self.interpreter {
                        let doc_root = Rc::clone(&interp.document.borrow().root);
                        if let Some(target) = find_node_by_ptr(&doc_root, node_id) {
                            let mut event = crate::interpreter::JsObject::new();
                            event.set("type".into(), crate::interpreter::JsValue::Str(event_type.into()));
                            event.set("animationName".into(), crate::interpreter::JsValue::Str(name));
                            event.set("target".into(), crate::interpreter::JsValue::DomNode(Rc::clone(&target)));
                            let event_val = crate::interpreter::JsValue::Object(
                                std::rc::Rc::new(std::cell::RefCell::new(event)));
                            let _ = interp.dispatch_event(&target, event_type, event_val);
                        }
                    }
                }
            }

            // Dispatch animationiteration
            for (node_id, name, _iter) in iter_events {
                if let Some(interp) = &mut self.interpreter {
                    let doc_root = Rc::clone(&interp.document.borrow().root);
                    if let Some(target) = find_node_by_ptr(&doc_root, node_id) {
                        let mut event = crate::interpreter::JsObject::new();
                        event.set("type".into(), crate::interpreter::JsValue::Str("animationiteration".into()));
                        event.set("animationName".into(), crate::interpreter::JsValue::Str(name));
                        event.set("target".into(), crate::interpreter::JsValue::DomNode(Rc::clone(&target)));
                        let event_val = crate::interpreter::JsValue::Object(
                            std::rc::Rc::new(std::cell::RefCell::new(event)));
                        let _ = interp.dispatch_event(&target, "animationiteration", event_val);
                    }
                }
            }

            // Uloz current style_map pro dalsi frame (transition diff source).
            // PERF: Rc::clone (pointer copy) misto HashMap clone. Pri animations
            // mutated style_map uz byla deep-cloned via Rc::make_mut, tak ted
            // jen Rc::clone na owned variant.
            let _t_prev = std::time::Instant::now();
            if self.css_uses_transitions {
                self.prev_style_map = Some(Rc::clone(&style_map));
            } else {
                self.prev_style_map = None;
            }
            perf_t("prev_style_map clone (Rc)", _t_prev);

            // Browser zoom: logical viewport = window / zoom (-> reflow at scaled
            // size). Render shader uniform = same logical dimensions, takze layout
            // px se mapuje na scaled NDC (visualni zoom).
            let viewport_w = (r.config.width as f32) / (self.zoom * r.scale_factor);
            let viewport_h = (r.config.height as f32) / (self.zoom * r.scale_factor);
            // Layout cache: rebuild jen kdyz CSS/DOM/viewport zmenil nebo
            // animations modifikuji layout-relevant props (width/height/margin/...).
            let cached_some = self.cached_layout_root.is_some();
            // Per-name check: invalidate jen kdyz aktivni animace ma name v
            // layout_affecting_animations set. Drive: jakekoliv @keyframes
            // s width/height v CSS by spustilo invalidaci kazdy frame, i
            // kdyz se ta keyframe na zadnem elementu nespusti.
            //
            // INCREMENTAL LAYOUT (Chrome-style): pri animaci animujici POUZE
            // width/height na elementu s overflow:hidden, treat jako "soft"
            // layout - aplikujeme rect.width/height primo via apply_paint_anims
            // a NEinvalidujeme cache. Resi typewriter (overflow:hidden + anim
            // width 0->100%) - drive 121 ms layout per frame, ted 0 ms.
            fn find_box_by_node_id<'a>(b: &'a super::layout::LayoutBox, id: usize) -> Option<&'a super::layout::LayoutBox> {
                if let Some(n) = &b.node {
                    if Rc::as_ptr(n) as usize == id {
                        return Some(b);
                    }
                }
                for ch in &b.children {
                    if let Some(found) = find_box_by_node_id(ch, id) {
                        return Some(found);
                    }
                }
                None
            }
            let aff_layout = self.active_animations.iter().any(|(node_id, name)| {
                if !self.layout_affecting_animations.contains(name) { return false; }
                // Pokud width/height-only animace + element ma overflow:hidden,
                // change je self-contained = NE hard invalidace.
                if self.width_height_only_animations.contains(name) {
                    if let Some(cache_root) = self.cached_layout_root.as_ref() {
                        if let Some(bx) = find_box_by_node_id(cache_root, *node_id) {
                            let oh = bx.overflow_x.as_str() == "hidden"
                                || bx.overflow_y.as_str() == "hidden"
                                || bx.overflow_x.as_str() == "clip"
                                || bx.overflow_y.as_str() == "clip";
                            if oh { return false; } // soft - skip invalidace
                        }
                    }
                }
                true // hard layout-affecting - invalidate
            });
            // viewport_match: cached.width muze byt viewport_w NEBO viewport_w - 15
            // (scrollbar reservation). Tolerance 16 px pokryvuje obe varianty.
            // Drive check `< 0.5` zpusoboval permanentni cache invalidaci na
            // strankach co overflow vertikalne (cached = vp - 15 vs check vp).
            // height comparison: cached.rect.height casto > viewport_height
            // (full document tall). Compare jen kdyz cached < viewport (mala
            // stranka). Jinak ignore height check.
            let viewport_match = self.cached_layout_root.as_ref().map(|l| {
                let dw = (l.rect.width - viewport_w).abs();
                let dw_ok = dw < 0.5 || (dw - 15.0).abs() < 0.5;
                dw_ok
            }).unwrap_or(false);
            let layout_cache_valid = cached_some && !aff_layout && viewport_match;
            if std::env::var("PERF_DEBUG").is_ok() && !layout_cache_valid {
                let aff_names: Vec<&String> = self.active_animations.iter()
                    .filter(|(_, n)| self.layout_affecting_animations.contains(n))
                    .map(|(_, n)| n).collect();
                let all_names: Vec<&String> = self.active_animations.iter()
                    .map(|(_, n)| n).collect();
                eprintln!("[cache_invalid] cached_some={} aff_layout={} viewport_match={} (vp_w={}, vp_h={}, cached={:?}, active_anims_total={}, all_names={:?}, aff_names={:?})",
                    cached_some, aff_layout, viewport_match, viewport_w, viewport_h,
                    self.cached_layout_root.as_ref().map(|l| (l.rect.width, l.rect.height)),
                    self.active_animations.len(), all_names, aff_names);
            }
            let _t_layout = std::time::Instant::now();
            // PERF: na cache hit + mutace (animations/sticky) MUTUJEME cached_layout_root
            // PRIMO IN-PLACE misto klonu. Drive: clone celeho stromu (5000+ nodu)
            // = 1.2 ms / frame. Sticky funguje pres sticky_original_y field
            // (idempotent), paint animations vzdy prepisuji ze style_map (nove
            // kazdy frame, bez akumulace).
            let needs_layout_mut = self.layout_has_sticky
                || !self.active_animations.is_empty()
                || !self.active_transitions.is_empty();
            // Owned (na cache miss vzdy nove vytvoreny + clone do cache; na cache hit
            // BEZ klonu - mutace primo na cached).
            let mut owned_layout_root: Option<super::layout::LayoutBox> = if layout_cache_valid {
                None  // Mutace primo na cached_layout_root via reference.
            } else {
                // Per-element layout cache: pasujeme prev cached_layout_root jako
                // hint. Pri match fingerprint reuznavaji subtrees.
                let prev_root = self.cached_layout_root.as_ref();
                let lr = layout::layout_tree_with_pseudo_cached(
                    &document_root, &*style_map, pseudo_map,
                    viewport_w, viewport_h, prev_root);
                perf_t("layout_tree (rebuild)", _t_layout);
                // Detekce sticky elementu - jen jednou pri rebuild. Per-frame
                // apply_sticky walk skip kdyz false (vetsina stranek sticky nema).
                fn has_sticky_walk(b: &super::layout::LayoutBox) -> bool {
                    if matches!(b.position, super::layout::Position::Sticky) { return true; }
                    b.children.iter().any(has_sticky_walk)
                }
                self.layout_has_sticky = has_sticky_walk(&lr);
                self.cached_layout_root = Some(lr.clone());
                Some(lr)
            };
            // Mutace: bud cloned (po rebuild) NEBO cached primo (cache hit).
            // Cache hit + needs_mut: mutujeme cached_layout_root in-place.
            let lr_mut: &mut super::layout::LayoutBox = if let Some(lr) = owned_layout_root.as_mut() {
                lr
            } else {
                self.cached_layout_root.as_mut().expect("cached present")
            };
            if needs_layout_mut {
                if !self.active_animations.is_empty() || !self.active_transitions.is_empty() {
                    let _t_anim = std::time::Instant::now();
                    apply_paint_animations(lr_mut, &style_map);
                    perf_t("apply_paint_animations", _t_anim);
                }
                if self.layout_has_sticky {
                    let _t_sticky = std::time::Instant::now();
                    layout::apply_sticky(lr_mut, self.scroll_y);
                    perf_t("apply_sticky", _t_sticky);
                }
            }
            // Convenience binding: vsechny dalsi reads pres `layout_root`.
            let layout_root: &super::layout::LayoutBox = if owned_layout_root.is_some() {
                owned_layout_root.as_ref().unwrap()
            } else {
                self.cached_layout_root.as_ref().expect("cached after layout")
            };
            // Viewport culling: vyrad off-screen elementy z paint walku
            // (test stranka 7000 px, viewport 900 px = 8x mensi paint cost).
            // Reuse buffer pres frames - alloc-free.
            let _t_paint = std::time::Instant::now();
            let mut display_list = std::mem::take(&mut self.display_list_buffer);
            paint::build_display_list_culled_into(&layout_root, self.scroll_y, viewport_h, &mut display_list);
            perf_t("paint::build_display_list_culled", _t_paint);
            // Selection emit PO layout commands - flow-based row selection.
            // Anchor + current point urcuji "first" (top-left order) a "last".
            // Per text node se walked lines (\n split z flush_inline wrap),
            // first line: chars od anchor.x do konce, middle lines: full,
            // last line: chars od start do current.x. Browser-like.
            let page_sel = (self_page_sel_anchor, self_page_sel_current);
            if let (Some(a), Some(c)) = page_sel {
                // Order: ktery point je "first" v reading flow (top-to-bottom,
                // left-to-right pri stejnem y).
                let (start, end) = if a.1 < c.1 || (a.1 == c.1 && a.0 <= c.0) {
                    (a, c)
                } else {
                    (c, a)
                };
                if (end.0 - start.0).abs() > 1.0 || (end.1 - start.1).abs() > 1.0 {
                    fn collect_text_lines(
                        b: &super::layout::LayoutBox,
                        sx: f32, sy: f32, ex: f32, ey: f32,
                        out: &mut Vec<(f32, f32, f32, f32)>,
                    ) {
                        if let Some(text) = &b.text {
                            let bx0 = b.rect.x;
                            let by0 = b.rect.y;
                            let bx1 = bx0 + b.rect.width;
                            let by1 = by0 + b.rect.height;
                            // Line height = font_size * line_height.
                            let lh = (b.line_height * b.font_size).max(b.font_size * 1.2);
                            // Vertical box ne v selection rozsah -> skip.
                            if by1 < sy || by0 > ey { /* skip */ }
                            else {
                                let bold = b.bold;
                                // Lines z text (\n split). Pri flush_inline byly inserted.
                                let lines: Vec<&str> = text.split('\n').collect();
                                let n_lines = lines.len();
                                for (li, line) in lines.iter().enumerate() {
                                    let line_y = by0 + (li as f32) * lh;
                                    let line_y_end = line_y + lh;
                                    // Skip line mimo selection vertical.
                                    if line_y_end < sy || line_y > ey { continue; }
                                    // Spada start.y do tohoto radku?
                                    let is_first_line = sy >= line_y && sy < line_y_end;
                                    let is_last_line = ey >= line_y && ey < line_y_end;
                                    // Range x: first_line -> [start.x, line_end];
                                    // last_line -> [line_start, end.x]; middle -> full.
                                    let line_w = line.chars().map(|ch|
                                        super::layout::measure_text_width_styled(
                                            &ch.to_string(), b.font_size, bold)).sum::<f32>();
                                    let line_start_x = if li == 0 { bx0 } else {
                                        // Wrapped line - zacina od inner_x parentu.
                                        // Approximaceuze rect.x (typicky inner_x p).
                                        bx0
                                    };
                                    // X range pro selection na tomto radku.
                                    let (x_lo, x_hi) = if is_first_line && is_last_line {
                                        // Sel zacina + konci na tomto radku.
                                        (sx.min(ex), sx.max(ex))
                                    } else if is_first_line {
                                        // Od sx do konce radku.
                                        (sx, line_start_x + line_w)
                                    } else if is_last_line {
                                        // Od zacatku radku do ex.
                                        (line_start_x, ex)
                                    } else {
                                        // Middle line - cely radek.
                                        (line_start_x, line_start_x + line_w)
                                    };
                                    // Char-snap.
                                    let sel_left = (x_lo - line_start_x).max(0.0);
                                    let sel_right = (x_hi - line_start_x).min(line_w);
                                    if sel_right <= sel_left + 0.5 { continue; }
                                    let mut acc = 0.0f32;
                                    let mut hl_start: Option<f32> = None;
                                    let mut hl_end: f32 = line_w;
                                    for ch in line.chars() {
                                        let adv = super::layout::measure_text_width_styled(
                                            &ch.to_string(), b.font_size, bold);
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
                                let _ = (bx1, by1, n_lines);
                            }
                        }
                        for ch in &b.children {
                            collect_text_lines(ch, sx, sy, ex, ey, out);
                        }
                    }
                    let mut hits = Vec::new();
                    collect_text_lines(&layout_root, start.0, start.1, end.0, end.1, &mut hits);
                    for (hx, hy, hw, hh) in hits {
                        display_list.push(DisplayCommand::Rect {
                            x: hx, y: hy, w: hw, h: hh,
                            color: [80, 150, 255, 120], radius: 0.0,
                        });
                    }
                }
            }

            // Canvas API: emit canvas ops jako DisplayCommands.
            if let Some(interp) = &self.interpreter {
                let canvas_ops = interp.canvas_ops.borrow();
                paint_canvas_ops(&layout_root, &canvas_ops, &mut display_list);
            }
            // WebGL canvas - real GPU path je v Renderer::draw_full_frame
            // (run_webgl_frame). paint_webgl_canvases je placeholder pro
            // debug viewer / devtools kontexty bez Renderer.

            // Apply scroll: posun vsechny y o -scroll_y
            for cmd in display_list.iter_mut() {
                shift_command_y(cmd, -self.scroll_y);
            }

            // <select> open dropdown overlay - emit popup s options pri open_select.
            if let Some((select_id, anchor_x, anchor_y, anchor_w)) = self.open_select {
                if let Some(interp) = &self.interpreter {
                    let doc_root = std::rc::Rc::clone(&interp.document.borrow().root);
                    if let Some(select_node) = find_node_by_ptr(&doc_root, select_id) {
                        let opt_h = 24.0_f32;
                        let pad_l = 8.0_f32;
                        let mut idx = 0_usize;
                        let popup_x = anchor_x;
                        // Pod selectem.
                        let popup_y = anchor_y + 24.0 - self.scroll_y;
                        // Background podklad celeho dropdownu.
                        let options: Vec<std::rc::Rc<crate::browser::dom::Node>> = select_node.children.borrow()
                            .iter().filter(|c| c.tag_name().as_deref() == Some("option")).cloned().collect();
                        let popup_h = opt_h * options.len() as f32;
                        if popup_h > 0.0 {
                            display_list.push(DisplayCommand::Shadow {
                                x: popup_x, y: popup_y, w: anchor_w, h: popup_h,
                                offset_x: 0.0, offset_y: 2.0, blur: 8.0, spread: 0.0,
                                color: [0, 0, 0, 80], radius: 4.0, inset: false,
                            });
                            display_list.push(DisplayCommand::Rect {
                                x: popup_x, y: popup_y, w: anchor_w, h: popup_h,
                                color: [255, 255, 255, 255], radius: 4.0,
                            });
                            display_list.push(DisplayCommand::Border {
                                x: popup_x, y: popup_y, w: anchor_w, h: popup_h,
                                width: 1.0, color: [200, 200, 210, 255],
                            });
                        }
                        for opt in &options {
                            let opt_y = popup_y + (idx as f32) * opt_h;
                            // Hover detect - mouse_y v range.
                            let hovered = self.mouse_x >= popup_x && self.mouse_x < popup_x + anchor_w
                                && (self.mouse_y - self.scroll_y) >= opt_y && (self.mouse_y - self.scroll_y) < opt_y + opt_h;
                            if hovered {
                                display_list.push(DisplayCommand::Rect {
                                    x: popup_x, y: opt_y, w: anchor_w, h: opt_h,
                                    color: [230, 240, 255, 255], radius: 0.0,
                                });
                            }
                            let txt = opt.text_content().trim().to_string();
                            display_list.push(DisplayCommand::Text {
                                x: popup_x + pad_l, y: opt_y + 6.0,
                                content: txt,
                                color: [40, 40, 50, 255],
                                font_size: 14.0, bold: false,
                                italic: false,
                                font_family: String::new(),
                                strikethrough: false, underline: false,
                            });
                            idx += 1;
                        }
                        // Save options + popup rect pro hit-test.
                        // (Hit-test po render: handle_click najde option pres ranges.)
                        // Implementacni shortcut: pri kliku najdeme option index z mouse_y.
                    }
                }
            }

            // Aplikuj horizontalni scroll posun na page content (pred overlay split).
            // Overlay (devtools, scrollbars, addr bar, find) jsou screen-fixed a
            // shift se na ne neaplikuje.
            if self.scroll_x.abs() > 0.001 {
                for cmd in display_list.iter_mut() {
                    shift_command_x(cmd, -self.scroll_x);
                }
            }
            // Shell mode: shift page commands dolu o chrome_h aby content
            // nezacinal pod chrome bar.
            if self.shell_mode {
                let bm_count = crate::devtools::bookmarks::load_bookmarks().len();
                let dy = 64.0 + if bm_count > 0 && self.bookmarks_bar_visible { 24.0 } else { 0.0 };
                for cmd in display_list.iter_mut() {
                    use DisplayCommand::*;
                    match cmd {
                        Rect { y, .. } | Text { y, .. } | Border { y, .. }
                        | Image { y, .. } | Gradient { y, .. } | Shadow { y, .. } => *y += dy,
                        _ => {}
                    }
                }
            }
            // Split point: vsechno za timto bodem se renderuje AZ PO WebGL passu,
            // takze WebGL canvas neprekryje devtools/scrollbar/address bar/find.
            let overlay_split = display_list.len();

            // Element highlight overlay (Chrome-like padding/margin viz) -
            // jen kdyz panel_open (jinak overlay perzistuje pres zavreni).
            crate::browser::devtools_panel::paint_element_highlight(
                &mut display_list,
                &layout_root,
                &self.devtools,
                self.scroll_y,
            );
            // Match preview overlay - vykresli orange outline kolem vsech
            // elementu matchujici aktivni selector (toggle ctverec v styles).
            if let Some(sel_str) = self.devtools.match_preview_selector.clone() {
                if let Some(stylesheets) = self.cached_stylesheets.as_ref() {
                    // Najdi rule v stylesheets s tim selectorem (po to_string()).
                    let mut sel_obj_found = None;
                    for sheet in stylesheets.iter() {
                        for rule in &sheet.rules {
                            for s in &rule.selectors {
                                if s.to_string() == sel_str {
                                    sel_obj_found = Some(s.clone());
                                    break;
                                }
                            }
                            if sel_obj_found.is_some() { break; }
                        }
                        if sel_obj_found.is_some() { break; }
                    }
                    if let Some(sel) = sel_obj_found {
                        // Walk DOM, najdi vsechny matching elementy, vykresli outline.
                        let doc_root = std::rc::Rc::clone(&document_root);
                        paint_match_preview_recursive(
                            &mut display_list, &doc_root, &sel, &layout_root, self.scroll_y);
                    }
                }
            }
            // Inspector flex/grid overlays - per active OverlayDescriptor v state.
            crate::browser::devtools_panel::paint_inspector_overlays(
                &mut display_list,
                &layout_root,
                &self.devtools,
                self.scroll_y,
            );
            // Shell chrome bar (tabs + nav) - shell_mode only. Paint here misto
            // self.paint_shell_chrome (borrow konflikt s renderer mut).
            if self.shell_mode {
                let win_w_logical = (r.config.width as f32) / (self.zoom * r.scale_factor);
                let win_h_logical = (r.config.height as f32) / (self.zoom * r.scale_factor);
                // Zoom indicator pri non-100% (pravy roh status zone).
                if (self.zoom - 1.0).abs() > 0.01 {
                    display_list.push(DisplayCommand::Rect {
                        x: win_w_logical - 60.0, y: 4.0, w: 50.0, h: 20.0,
                        color: [69, 161, 255, 220], radius: 4.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: win_w_logical - 56.0, y: 6.0,
                        content: format!("{:.0}%", self.zoom * 100.0),
                        color: [255, 255, 255, 255],
                        font_size: 12.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                let titles: Vec<String> = self.tabs.tabs.iter().map(|t| t.title.clone()).collect();
                let favicons: Vec<Option<String>> = self.tabs.tabs.iter()
                    .map(|t| t.favicon_url.clone()).collect();
                let pins: Vec<bool> = self.tabs.tabs.iter().map(|t| t.pinned).collect();
                let loadings: Vec<bool> = self.tabs.tabs.iter().map(|t| t.loading).collect();
                let groups: Vec<Option<[u8; 4]>> = self.tabs.tabs.iter()
                    .map(|t| t.group_color).collect();
                let bm_count = crate::devtools::bookmarks::load_bookmarks().len();
                let chrome_h = 64.0 + if bm_count > 0 && self.bookmarks_bar_visible { 24.0 } else { 0.0 };
                let anim_t = self.start_time.elapsed().as_secs_f32();
                paint_shell_chrome_with_groups(&mut display_list, win_w_logical, chrome_h,
                                             self.base_url.as_deref().unwrap_or(""),
                                             Some(&titles), self.tabs.active, Some(&favicons),
                                             Some(&pins), Some(&loadings), anim_t, Some(&groups));
                // Status bar dole - pri hover URL preview.
                // Scroll-to-top button (pravy dolni roh) pri scroll_y > 200.
                if self.scroll_y > 200.0 {
                    let panel_h_logical_local = if self.devtools.panel_open {
                        self.devtools.panel_h.min(win_h_logical * 0.7)
                    } else { 0.0 };
                    let btn_x = win_w_logical - 40.0;
                    let btn_y = win_h_logical - panel_h_logical_local - 50.0;
                    display_list.push(DisplayCommand::Rect {
                        x: btn_x, y: btn_y, w: 32.0, h: 32.0,
                        color: [69, 161, 255, 220], radius: 16.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: btn_x + 10.0, y: btn_y + 6.0, content: "↑".to_string(),
                        color: [255, 255, 255, 255],
                        font_size: 18.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // Bookmark picker popup (Ctrl+D).
                if let Some(p) = &self.bookmark_picker {
                    let pop_w = 420.0_f32;
                    let pop_h = 240.0_f32;
                    let px = (win_w_logical - pop_w) * 0.5;
                    let py = 80.0_f32;
                    display_list.push(DisplayCommand::Rect {
                        x: 0.0, y: 0.0, w: win_w_logical, h: win_h_logical,
                        color: [0, 0, 0, 140], radius: 0.0,
                    });
                    display_list.push(DisplayCommand::Rect {
                        x: px, y: py, w: pop_w, h: pop_h,
                        color: [27, 27, 35, 250], radius: 8.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: px + 16.0, y: py + 14.0, content: "Pridat zalozku".to_string(),
                        color: [251, 251, 254, 255],
                        font_size: 16.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    // URL line.
                    display_list.push(DisplayCommand::Text {
                        x: px + 16.0, y: py + 44.0,
                        content: format!("URL: {}", p.url.chars().take(50).collect::<String>()),
                        color: [161, 161, 174, 255],
                        font_size: 11.0, bold: false, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    // Title input.
                    let title_focused = p.focus == BookmarkPickerFocus::Title;
                    display_list.push(DisplayCommand::Text {
                        x: px + 16.0, y: py + 70.0, content: "Nazev".to_string(),
                        color: [191, 191, 201, 255],
                        font_size: 12.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    let inp_y = py + 88.0;
                    let inp_h = 26.0;
                    display_list.push(DisplayCommand::Rect {
                        x: px + 16.0, y: inp_y, w: pop_w - 32.0, h: inp_h,
                        color: if title_focused { [42, 42, 56, 255] } else { [35, 35, 45, 255] },
                        radius: 4.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: px + 22.0, y: inp_y + 5.0, content: p.title_buffer.clone(),
                        color: [251, 251, 254, 255],
                        font_size: 13.0, bold: false, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    if title_focused {
                        display_list.push(DisplayCommand::Rect {
                            x: px + 22.0 + (p.title_buffer.chars().count() as f32) * 7.5,
                            y: inp_y + 6.0, w: 1.0, h: 16.0,
                            color: [254, 191, 84, 255], radius: 0.0,
                        });
                    }
                    // Folder input.
                    let folder_focused = p.focus == BookmarkPickerFocus::Folder;
                    display_list.push(DisplayCommand::Text {
                        x: px + 16.0, y: py + 124.0, content: "Slozka (volitelne)".to_string(),
                        color: [191, 191, 201, 255],
                        font_size: 12.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    let f_y = py + 142.0;
                    display_list.push(DisplayCommand::Rect {
                        x: px + 16.0, y: f_y, w: pop_w - 32.0, h: inp_h,
                        color: if folder_focused { [42, 42, 56, 255] } else { [35, 35, 45, 255] },
                        radius: 4.0,
                    });
                    let folder_disp = if p.folder_buffer.is_empty() && !folder_focused {
                        "Korenove".to_string()
                    } else { p.folder_buffer.clone() };
                    display_list.push(DisplayCommand::Text {
                        x: px + 22.0, y: f_y + 5.0, content: folder_disp,
                        color: if p.folder_buffer.is_empty() && !folder_focused {
                            [109, 109, 124, 255]
                        } else { [251, 251, 254, 255] },
                        font_size: 13.0, bold: false, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    if folder_focused {
                        display_list.push(DisplayCommand::Rect {
                            x: px + 22.0 + (p.folder_buffer.chars().count() as f32) * 7.5,
                            y: f_y + 6.0, w: 1.0, h: 16.0,
                            color: [254, 191, 84, 255], radius: 0.0,
                        });
                    }
                    // Hint dolu.
                    display_list.push(DisplayCommand::Text {
                        x: px + 16.0, y: py + pop_h - 28.0,
                        content: "Tab: zmena pole | Enter: ulozit | Esc: zrusit".to_string(),
                        color: [109, 109, 124, 255],
                        font_size: 11.0, bold: false, italic: true,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // F1 shortcuts overlay - modal pres celou viewport.
                if self.shortcuts_overlay_open {
                    let mw = 480.0_f32.min(win_w_logical - 40.0);
                    let mh = 480.0_f32.min(win_h_logical - 60.0);
                    let mx = (win_w_logical - mw) / 2.0;
                    let my = (win_h_logical - mh) / 2.0;
                    // Backdrop
                    display_list.push(DisplayCommand::Rect {
                        x: 0.0, y: 0.0, w: win_w_logical, h: win_h_logical,
                        color: [0, 0, 0, 160], radius: 0.0,
                    });
                    // Modal panel
                    display_list.push(DisplayCommand::Rect {
                        x: mx, y: my, w: mw, h: mh,
                        color: [27, 27, 35, 250], radius: 8.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: mx + 20.0, y: my + 16.0, content: "Klavesove zkratky".to_string(),
                        color: [251, 251, 254, 255],
                        font_size: 18.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                    let entries: &[(&str, &str)] = &[
                        ("Ctrl+T",        "Nova zalozka"),
                        ("Ctrl+W",        "Zavrit zalozku"),
                        ("Ctrl+Shift+T",  "Obnovit zavrenou zalozku"),
                        ("Ctrl+N",        "Nove okno"),
                        ("Ctrl+Tab",      "Dalsi zalozka"),
                        ("Ctrl+1..9",     "Skok na zalozku N"),
                        ("Ctrl+L",        "Address bar"),
                        ("Ctrl+F",        "Hledat na strance"),
                        ("Ctrl+B",        "Zalozky"),
                        ("Ctrl+H",        "Historie"),
                        ("Ctrl+J",        "Stahnuti"),
                        ("Ctrl+D",        "Pridat zalozku"),
                        ("Ctrl+P",        "Tisk do PDF"),
                        ("Ctrl+Alt+R",    "Reading mode (zen view)"),
                        ("Ctrl+0/+/-",    "Zoom reset/in/out"),
                        ("Ctrl+A",        "Vybrat vse"),
                        ("Ctrl+C",        "Kopirovat"),
                        ("F1",            "Tato napoveda"),
                        ("F12",           "DevTools panel"),
                        ("Esc",           "Zavrit overlay/popup"),
                    ];
                    for (i, (k, v)) in entries.iter().enumerate() {
                        let row_y = my + 48.0 + (i as f32) * 22.0;
                        if row_y + 16.0 > my + mh - 16.0 { break; }
                        display_list.push(DisplayCommand::Text {
                            x: mx + 20.0, y: row_y, content: k.to_string(),
                            color: [254, 191, 84, 255],
                            font_size: 13.0, bold: true, italic: false,
                            font_family: "CamingoMono".into(),
                            strikethrough: false, underline: false,
                        });
                        display_list.push(DisplayCommand::Text {
                            x: mx + 160.0, y: row_y, content: v.to_string(),
                            color: [191, 191, 201, 255],
                            font_size: 13.0, bold: false, italic: false,
                            font_family: "Inter".into(),
                            strikethrough: false, underline: false,
                        });
                    }
                    display_list.push(DisplayCommand::Text {
                        x: mx + 20.0, y: my + mh - 28.0,
                        content: "Esc zavre".to_string(),
                        color: [109, 109, 124, 255],
                        font_size: 11.0, bold: false, italic: true,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // Shell tab tooltip - kdyz mouse nad chip s truncated title.
                if let Some((txt, x, y)) = &self.shell_tab_tooltip {
                    let tw = (txt.chars().count() as f32) * 7.0 + 16.0;
                    let tx = x.min(win_w_logical - tw - 4.0).max(4.0);
                    display_list.push(DisplayCommand::Rect {
                        x: tx, y: *y, w: tw, h: 22.0,
                        color: [16, 16, 22, 240], radius: 4.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: tx + 8.0, y: *y + 4.0, content: txt.clone(),
                        color: [251, 251, 254, 255],
                        font_size: 12.0, bold: false, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // Reading mode badge v top-right pri zaple.
                if self.reading_mode_on {
                    let bx = win_w_logical - 200.0;
                    let by = 4.0;
                    display_list.push(DisplayCommand::Rect {
                        x: bx, y: by, w: 100.0, h: 20.0,
                        color: [254, 191, 84, 220], radius: 10.0,
                    });
                    display_list.push(DisplayCommand::Text {
                        x: bx + 10.0, y: by + 3.0, content: "Reading mode".to_string(),
                        color: [42, 41, 50, 255],
                        font_size: 11.0, bold: true, italic: false,
                        font_family: "Inter".into(),
                        strikethrough: false, underline: false,
                    });
                }
                // Status bar - permanentni dolni lista. Vlevo URL hover (jen pokud je),
                // vpravo zoom % indicator pri zoom != 1.0.
                let panel_h_logical = if self.devtools.panel_open {
                    self.devtools.panel_h.min(win_h_logical * 0.7)
                } else { 0.0 };
                let sb_y = win_h_logical - panel_h_logical - 22.0;
                let zoom_pct = (self.zoom * 100.0).round() as i32;
                let show_zoom = (self.zoom - 1.0).abs() > 0.01;
                if self.status_hover_url.is_some() || show_zoom {
                    if let Some(url) = &self.status_hover_url {
                        let url_short: String = url.chars().take(120).collect();
                        let sb_w = (url_short.len() as f32) * 7.0 + 24.0;
                        display_list.push(DisplayCommand::Rect {
                            x: 4.0, y: sb_y, w: sb_w.min(win_w_logical * 0.7),
                            h: 20.0, color: [27, 27, 35, 240], radius: 2.0,
                        });
                        display_list.push(DisplayCommand::Text {
                            x: 12.0, y: sb_y + 3.0, content: url_short,
                            color: [191, 191, 201, 255],
                            font_size: 12.0, bold: false, italic: false,
                            font_family: "Inter".into(),
                            strikethrough: false, underline: false,
                        });
                    }
                    if show_zoom {
                        let txt = format!("{}%", zoom_pct);
                        let zw = (txt.len() as f32) * 8.0 + 16.0;
                        display_list.push(DisplayCommand::Rect {
                            x: win_w_logical - zw - 4.0, y: sb_y, w: zw, h: 20.0,
                            color: [42, 41, 50, 240], radius: 2.0,
                        });
                        display_list.push(DisplayCommand::Text {
                            x: win_w_logical - zw + 6.0, y: sb_y + 3.0, content: txt,
                            color: [254, 191, 84, 255],
                            font_size: 12.0, bold: true, italic: false,
                            font_family: "CamingoMono".into(),
                            strikethrough: false, underline: false,
                        });
                    }
                }
            }

            // FPS counter overlay (Ctrl+Shift+F toggle nebo PERF_DEBUG=1).
            // Top-right roh, color-coded: green >50, yellow 30-50, red <30.
            if self.show_fps && !self.frame_times_ms.is_empty() {
                let avg_ms = self.frame_times_ms.iter().sum::<f32>() / self.frame_times_ms.len() as f32;
                let fps = if avg_ms > 0.01 { 1000.0 / avg_ms } else { 999.0 };
                let max_ms = self.frame_times_ms.iter().cloned().fold(0.0_f32, f32::max);
                let (rect_w, rect_h) = (130.0_f32, 36.0_f32);
                let win_w_logical = (r.config.width as f32) / (self.zoom * r.scale_factor);
                let chrome_h = self.shell_chrome_h;
                let fps_x = win_w_logical - rect_w - 8.0;
                let fps_y = chrome_h + 8.0;
                let color = if fps >= 50.0 { [80, 220, 120, 255] }
                    else if fps >= 30.0 { [240, 200, 80, 255] }
                    else { [240, 80, 80, 255] };
                display_list.push(DisplayCommand::Rect {
                    x: fps_x, y: fps_y, w: rect_w, h: rect_h,
                    color: [20, 20, 26, 220], radius: 4.0,
                });
                display_list.push(DisplayCommand::Text {
                    x: fps_x + 8.0, y: fps_y + 4.0,
                    content: format!("{:.0} FPS  {:.1}ms", fps, avg_ms),
                    color, font_size: 12.0, bold: true, italic: false,
                    font_family: "CamingoMono".into(),
                    strikethrough: false, underline: false,
                });
                display_list.push(DisplayCommand::Text {
                    x: fps_x + 8.0, y: fps_y + 20.0,
                    content: format!("max {:.1}ms", max_ms),
                    color: [180, 180, 190, 200],
                    font_size: 10.0, bold: false, italic: false,
                    font_family: "CamingoMono".into(),
                    strikethrough: false, underline: false,
                });
            }

            // In-window DevTools panel - emit pred scrollbar a po main viewport content.
            // viewport_w/h v logical px (display list je v logical, vp uniform / zoom*scale).
            let viewport_w_logical = (r.config.width as f32) / (self.zoom * r.scale_factor);
            let viewport_h_logical = (r.config.height as f32) / (self.zoom * r.scale_factor);
            self.devtools.tick_frame();
            paint_devtools_panel(
                &mut display_list,
                &layout_root,
                &self.devtools,
                self.interpreter.as_ref(),
                viewport_w_logical,
                viewport_h_logical,
                self.mouse_x - self.scroll_x,
                self.mouse_y - self.scroll_y,
            );
            // (Selection rect uz emitnuty PRED build_display_list - rendered POD textem.)
            // Address bar (Ctrl+L) overlay: input top centered + autocomplete.
            if self.addr_open {
                let vw = (r.config.width as f32) / (self.zoom * r.scale_factor);
                let bar_w: f32 = (vw - 80.0).min(800.0);
                let bar_h: f32 = 40.0;
                let bar_x = (vw - bar_w) * 0.5;
                let bar_y = 8.0;
                display_list.push(DisplayCommand::Rect {
                    x: bar_x, y: bar_y, w: bar_w, h: bar_h,
                    color: [40, 40, 40, 240], radius: 6.0,
                });
                let label = format!("URL: {}", self.addr_input.text);
                display_list.push(DisplayCommand::Text {
                    x: bar_x + 12.0, y: bar_y + 10.0,
                    content: label.clone(), color: [255, 255, 255, 255],
                    font_size: 14.0, bold: false, italic: false,
                    font_family: "Inter".into(),
                    strikethrough: false, underline: false,
                });
                // Cursor blink ~ 60 frame interval.
                if (self.devtools.frame_counter / 30) % 2 == 0 {
                    let label_w = (label.len() as f32) * 7.5;
                    display_list.push(DisplayCommand::Rect {
                        x: bar_x + 14.0 + label_w, y: bar_y + 10.0,
                        w: 2.0, h: 18.0,
                        color: [255, 255, 255, 255], radius: 0.0,
                    });
                }
                // Autocomplete suggestions z history (max 8 matchu).
                let q = self.addr_input.text.to_lowercase();
                if !q.is_empty() {
                    let history = crate::devtools::history::load_history();
                    let bookmarks = crate::devtools::bookmarks::load_bookmarks();
                    let mut suggest: Vec<(String, String, bool)> = Vec::new();
                    for b in &bookmarks {
                        if b.url.to_lowercase().contains(&q) || b.title.to_lowercase().contains(&q) {
                            suggest.push((b.url.clone(), b.title.clone(), true));
                            if suggest.len() >= 8 { break; }
                        }
                    }
                    for h in history.iter().rev() {
                        if suggest.len() >= 8 { break; }
                        if suggest.iter().any(|(u, _, _)| *u == h.url) { continue; }
                        if h.url.to_lowercase().contains(&q) || h.title.to_lowercase().contains(&q) {
                            suggest.push((h.url.clone(), h.title.clone(), false));
                        }
                    }
                    if !suggest.is_empty() {
                        let popup_y = bar_y + bar_h + 4.0;
                        let item_h = 28.0_f32;
                        let popup_h = (suggest.len() as f32) * item_h;
                        display_list.push(DisplayCommand::Rect {
                            x: bar_x, y: popup_y, w: bar_w, h: popup_h,
                            color: [40, 40, 40, 240], radius: 6.0,
                        });
                        for (i, (url, title, is_bm)) in suggest.iter().enumerate() {
                            let iy = popup_y + (i as f32) * item_h;
                            let icon = if *is_bm { "★" } else { "↻" };
                            display_list.push(DisplayCommand::Text {
                                x: bar_x + 12.0, y: iy + 6.0, content: icon.to_string(),
                                color: [191, 191, 201, 255],
                                font_size: 14.0, bold: false, italic: false,
                                font_family: "Inter".into(),
                                strikethrough: false, underline: false,
                            });
                            display_list.push(DisplayCommand::Text {
                                x: bar_x + 32.0, y: iy + 4.0, content: title.clone(),
                                color: [255, 255, 255, 255],
                                font_size: 13.0, bold: true, italic: false,
                                font_family: "Inter".into(),
                                strikethrough: false, underline: false,
                            });
                            let url_short: String = url.chars().take(80).collect();
                            display_list.push(DisplayCommand::Text {
                                x: bar_x + 32.0, y: iy + 16.0, content: url_short,
                                color: [161, 161, 174, 255],
                                font_size: 11.0, bold: false, italic: false,
                                font_family: "Inter".into(),
                                strikethrough: false, underline: false,
                            });
                        }
                    }
                }
            }
            // Find on page: highlight matches + overlay s query a counter.
            if self.find_open {
                let matches = find_matches_in(&layout_root, &self.find_query.text);
                let cur_idx = self.find_match_idx;
                for (i, &(my, mx, mw)) in matches.iter().enumerate() {
                    let color = if i == cur_idx { [255, 165, 0, 180] } else { [255, 235, 100, 130] };
                    display_list.push(DisplayCommand::Rect {
                        x: mx - self.scroll_x, y: my - self.scroll_y, w: mw, h: 18.0,
                        color, radius: 2.0,
                    });
                }
                let vw = (r.config.width as f32) / (self.zoom * r.scale_factor);
                let bar_w: f32 = 320.0;
                let bar_h: f32 = 40.0;
                let bar_x = vw - bar_w - 8.0;
                let bar_y = 8.0;
                display_list.push(DisplayCommand::Rect {
                    x: bar_x, y: bar_y, w: bar_w, h: bar_h,
                    color: [40, 40, 40, 230], radius: 6.0,
                });
                let counter = if matches.is_empty() {
                    if self.find_query.text.is_empty() { String::from("Find:") } else { String::from("0/0") }
                } else {
                    format!("{}/{}", cur_idx + 1, matches.len())
                };
                display_list.push(DisplayCommand::Text {
                    x: bar_x + 12.0, y: bar_y + 10.0,
                    content: format!("Find: {}", self.find_query.text),
                    color: [255, 255, 255, 255],
                    font_size: 14.0, bold: false, italic: false,
                    font_family: "Inter".into(),
                    strikethrough: false, underline: false,
                });
                // Counter vpravo + nav buttons.
                display_list.push(DisplayCommand::Text {
                    x: bar_x + bar_w - 100.0, y: bar_y + 10.0,
                    content: counter.clone(),
                    color: if matches.is_empty() && !self.find_query.text.is_empty() {
                        [255, 117, 100, 255]
                    } else { [191, 191, 201, 255] },
                    font_size: 12.0, bold: true, italic: false,
                    font_family: "Inter".into(),
                    strikethrough: false, underline: false,
                });
                // Up / Down arrow buttons (Shift+Enter / Enter).
                display_list.push(DisplayCommand::Text {
                    x: bar_x + bar_w - 50.0, y: bar_y + 10.0,
                    content: "↑".to_string(),
                    color: [191, 191, 201, 255],
                    font_size: 14.0, bold: false, italic: false,
                    font_family: "Inter".into(),
                    strikethrough: false, underline: false,
                });
                display_list.push(DisplayCommand::Text {
                    x: bar_x + bar_w - 32.0, y: bar_y + 10.0,
                    content: "↓".to_string(),
                    color: [191, 191, 201, 255],
                    font_size: 14.0, bold: false, italic: false,
                    font_family: "Inter".into(),
                    strikethrough: false, underline: false,
                });
            }
            // (Highlight rect uz vykreslen pres paint_element_highlight nahore.)

            // Scrollbar rendering: pri page content overflow Y emituj track + thumb.
            // Logical viewport - vertices v logical px.
            let panel_h_logical = if self.devtools.panel_open { self.devtools.panel_h.min(viewport_h_logical * 0.7) } else { 0.0 };
            let viewport_w = viewport_w_logical;
            let viewport_h = viewport_h_logical - panel_h_logical;
            let total_h = layout_root.rect.height;
            if total_h > viewport_h {
                let bar_w = 12.0_f32;
                let bar_x = viewport_w - bar_w;
                // Track (background).
                display_list.push(DisplayCommand::Rect {
                    x: bar_x, y: 0.0, w: bar_w, h: viewport_h,
                    color: [240, 240, 245, 255], radius: 0.0,
                });
                // Thumb.
                let thumb_h = (viewport_h * viewport_h / total_h).max(40.0);
                let max_scroll = (total_h - viewport_h).max(1.0);
                let thumb_y = (self.scroll_y / max_scroll) * (viewport_h - thumb_h);
                display_list.push(DisplayCommand::Rect {
                    x: bar_x + 2.0, y: thumb_y + 2.0,
                    w: bar_w - 4.0, h: thumb_h - 4.0,
                    color: [160, 160, 170, 255], radius: (bar_w - 4.0) * 0.5,
                });
            }
            // Horizontalni scrollbar (kdyz layout_root.rect.width > viewport_w).
            let total_w = layout_root.rect.width;
            if total_w > viewport_w {
                let bar_h = 12.0_f32;
                let bar_y = viewport_h - bar_h;
                display_list.push(DisplayCommand::Rect {
                    x: 0.0, y: bar_y, w: viewport_w, h: bar_h,
                    color: [240, 240, 245, 255], radius: 0.0,
                });
                let thumb_w = (viewport_w * viewport_w / total_w).max(40.0);
                let max_scroll_x = (total_w - viewport_w).max(1.0);
                let thumb_x = (self.scroll_x / max_scroll_x) * (viewport_w - thumb_w);
                display_list.push(DisplayCommand::Rect {
                    x: thumb_x + 2.0, y: bar_y + 2.0,
                    w: thumb_w - 4.0, h: bar_h - 4.0,
                    color: [160, 160, 170, 255], radius: (bar_h - 4.0) * 0.5,
                });
            }
            // Pre-rasterize vsechny glyfy do atlasu + nacti images.
            // Pri COLR color font: rasterize char jako RGBA + put do image_atlas
            // pres synthetic key "__colr:{family}:{ch}:{size}". Render path detekuje.
            for cmd in &display_list {
                match cmd {
                    DisplayCommand::Text { content, font_size, font_family, color, bold, italic, .. } => {
                        for ch in content.chars() {
                            // Pokus o color glyph rasterization.
                            let mut color_added = false;
                            if let Some(colr) = r.color_fonts.get(font_family).cloned() {
                                if let Some(font) = r.font_registry.get(font_family).cloned() {
                                    let glyph_id = font.lookup_glyph_index(ch);
                                    if glyph_id != 0 && colr.base_to_layers.contains_key(&glyph_id) {
                                        let key = format!("__colr:{}:{}:{}", font_family, ch as u32, *font_size as u32);
                                        if !r.image_atlas.contains(&key) {
                                            if let Some((w, h, _, _, rgba)) = super::emoji_fonts::rasterize_color_glyph(
                                                &font, glyph_id, *font_size, &colr, *color,
                                            ) {
                                                r.image_atlas.add(&key, w as u32, h as u32, &rgba);
                                            }
                                        }
                                        color_added = true;
                                    }
                                }
                            }
                            if !color_added {
                                // Atlas key prefixovany dle bold/italic kombinace.
                                let key_family = match (*bold, *italic) {
                                    (true, true) if r.atlas.font_bold_italic.is_some() =>
                                        format!("__bi__:{}", font_family),
                                    (false, true) if r.atlas.font_italic.is_some() =>
                                        format!("__italic__:{}", font_family),
                                    (true, _) if r.atlas.font_bold.is_some() =>
                                        format!("__bold__:{}", font_family),
                                    _ => font_family.clone(),
                                };
                                let phys = (*font_size * self.zoom).round().max(1.0) as u32;
                                r.atlas.add(&key_family, ch, phys);
                            }
                        }
                    }
                    DisplayCommand::Image { src, w, h, .. } => {
                        // Resolve relative URL proti base_url (pri http(s) nebo file:// page).
                        let resolved = match &self.base_url {
                            Some(base) => resolve_url(base, src),
                            None => src.clone(),
                        };
                        r.load_image_as(src, &resolved);
                        // Pri zoomu re-resample image na physical size = w * zoom.
                        let target_w = (*w * self.zoom).round().max(1.0) as u32;
                        let target_h = (*h * self.zoom).round().max(1.0) as u32;
                        r.resample_image_for_size(src, target_w, target_h);
                    }
                    _ => {}
                }
            }
            r.upload_atlas();
            r.upload_image_atlas();

            // Split list na page (pred WebGL) + overlay (po WebGL).
            let (page_cmds, overlay_cmds) = display_list.split_at(overlay_split);

            // Extract TextRun pole pro per-glyph selection (foundation).
            // Walks display_list a build cumulative_advances z atlas. Page cmds
            // jen (overlay nemam textovy obsah co user vybira).
            self.painted_text_runs = extract_text_runs(page_cmds, &r.atlas, r.zoom);

            // Pri WebGL canvas s pending queue, vyuzij webgl-aware draw flow.
            let webgl_states_opt = self.interpreter.as_ref().map(|i| i.webgl_states.clone());
            if let Some(states_rc) = &webgl_states_opt {
                let states = states_rc.borrow();
                r.draw_full_frame(page_cmds, overlay_cmds, &layout_root, Some(&*states), self.scroll_y);
            } else {
                r.draw_segments(&display_list);
            }

            // Ulozim layout pro hit test + vrat display_list buffer pro priste.
            // PERF: pri owned=None (immutable cesta) self.layout_root zustava
            // synced z minulosti - cached_layout_root se nemenil. Skip clone.
            // Mutation cestou (animations/sticky): musime ulozit post-mutation
            // verzi pro spravny hit-test (sticky offsets).
            if let Some(lr) = owned_layout_root {
                self.layout_root = Some(lr);
            } else if self.layout_root.is_none() {
                // First-time: pri cache hit + zadnem prev layout_root (rare),
                // klonujeme z cache jednou.
                self.layout_root = self.cached_layout_root.clone();
            }
            self.display_list_buffer = display_list;
            let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
            if frame_ms > 50.0 {
                eprintln!("[slow frame] {:.1} ms", frame_ms);
            }
            // Ring buffer poslednich 60 framov pro FPS counter.
            self.frame_times_ms.push_back(frame_ms);
            if self.frame_times_ms.len() > 60 {
                self.frame_times_ms.pop_front();
            }
            // Performance sample do DevTools.
            let dl_size = self.display_list_buffer.len() as u32;
            self.devtools.performance.push(crate::devtools::model::performance::FrameSample {
                frame_index: self.devtools.frame_counter,
                total_ms: frame_ms,
                layout_ms: 0.0,
                paint_build_ms: 0.0,
                gpu_submit_ms: 0.0,
                display_list_size: dl_size,
                vertex_count: 0,
            });
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
    let initial_url = base_url.clone();
    // Session restore (shell mode): pri startu nahraje predchozi tabs.
    let mut initial_tabs: Vec<tabs::Tab> = Vec::new();
    let mut initial_active: usize = 0;
    if shell_mode {
        if let Some(s) = crate::devtools::session::load_session() {
            for st in &s.tabs {
                let url = st.url.clone();
                let mut tab = tabs::Tab::empty();
                tab.title = st.title.clone();
                tab.url = url;
                initial_tabs.push(tab);
            }
            initial_active = s.active.min(initial_tabs.len().saturating_sub(1));
        }
    }
    if initial_tabs.is_empty() {
        initial_tabs.push(tabs::Tab::new(html.clone(), css.clone(), base_url.clone(), current_html_path.clone()));
    }
    let initial_tab = initial_tabs[initial_active].clone();
    let mut app = App {
        html, css,
        cached_stylesheets_hash: 0,
        cached_stylesheets: None,
        cached_cascade_hash: 0,
        cached_style_map: None,
        cached_matched_key: None,
        animation_origin: std::time::Instant::now(),
        animation_pause_start: None,
        paused_animation_nodes: std::collections::HashSet::new(),
        paused_node_styles: std::collections::HashMap::new(),
        animations_scrubber_drag: false,
        painted_text_runs: Vec::new(),
        async_jobs: crate::browser::async_jobs::AsyncJobsRegistry::new(),
        bookmark_picker: None,
        cached_pseudo_map: None,
        display_list_buffer: Vec::with_capacity(2048),
        cached_layout_root: None,
        animations_affect_layout: false,
        css_uses_hover: false,
        css_uses_focus: false,
        css_uses_viewport: false,
        css_uses_transitions: false,
        css_uses_keyframes: false,
        layout_has_sticky: false,
        layout_affecting_animations: std::collections::HashSet::new(),
        width_height_only_animations: std::collections::HashSet::new(),
        frame_times_ms: std::collections::VecDeque::with_capacity(60),
        show_fps: std::env::var("PERF_DEBUG").is_ok(),
        current_path: current_html_path,
        base_url,
        history: initial_url.into_iter().collect(),
        history_idx: 0,
        open_select: None,
        auto_devtools,
        window: None,
        renderer: None,
        layout_root: None,
        interpreter: None,
        mouse_x: 0.0,
        mouse_y: 0.0,
        scroll_y: 0.0,
        scroll_x: 0.0,
        start_time: std::time::Instant::now(),
        prev_style_map: None,
        active_transitions: Vec::new(),
        active_animations: std::collections::HashSet::new(),
        animation_iterations: std::collections::HashMap::new(),
        devtools: crate::devtools::DevToolsState::default(),
        devtools_resizing: false,
        last_click_time: None,
        last_click_pos: (0.0, 0.0),
        shared_debugger: std::sync::Arc::new(std::sync::Mutex::new(
            crate::interpreter::DebuggerState::default())),
        continue_signal: std::sync::Arc::new((
            std::sync::Mutex::new(false), std::sync::Condvar::new())),
        debug_runner: None,
        zoom: 1.0,
        modifiers: winit::keyboard::ModifiersState::empty(),
        find_open: false,
        find_query: crate::devtools::model::text_buffer::SimpleStringBuffer::new(),
        find_match_idx: 0,
        addr_open: false,
        addr_input: crate::devtools::model::text_buffer::SimpleStringBuffer::new(),
        scroll_target_y: 0.0,
        scroll_target_x: 0.0,
        page_scrollbar_v_drag: false,
        page_scrollbar_h_drag: false,
        tab_drag_idx: None,
        tab_drag_x_start: 0.0,
        status_hover_url: None,
        shell_tab_tooltip: None,
        shell_tab_hover_pending: None,
        shortcuts_overlay_open: false,
        reading_mode_on: false,
        bookmarks_bar_visible: true,
        shell_mode,
        shell_chrome_h: 64.0,
        tabs: {
            let mut tm = tabs::TabManager::new(initial_tab);
            // Pri session restore pridej dalsi tabs.
            if shell_mode && initial_tabs.len() > 1 {
                tm.tabs = initial_tabs;
                tm.active = initial_active;
            }
            tm
        },
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

struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Browser zoom factor. Vertex px coordinates jsou v logickem px (viewport
    /// width / zoom). Uniform vp je nastaven na (config.w / zoom, config.h /
    /// zoom) tak aby NDC mapping render-koval zoom*logical px na physical px.
    zoom: f32,
    /// HiDPI scale_factor z winit. config.width je v physical px = logical *
    /// scale_factor. CSS coords jsou logical -> NDC mapping musi pouzit logical
    /// vp = config.width / scale_factor.
    scale_factor: f32,
    pipeline: wgpu::RenderPipeline,
    /// Optional LCD pipeline pro real subpixel text - vyzaduje DUAL_SOURCE_BLENDING.
    /// None pri unsupported HW (fallback grayscale v hlavnim shaderu mode 9).
    lcd_pipeline: Option<wgpu::RenderPipeline>,
    uniform_buf: wgpu::Buffer,
    atlas_tex: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    atlas_smp: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    atlas: GlyphAtlas,
    /// Image RGBA atlas + GPU texture
    image_atlas: ImageAtlas,
    /// Cached source bytes per URL pro re-resample pri zoomu (load_image_as
    /// stores, resample_image_for_size cte).
    image_source_bytes: std::collections::HashMap<String, Vec<u8>>,
    image_tex: wgpu::Texture,
    image_view: wgpu::TextureView,
    /// @font-face loaded fonts: family -> Font.
    font_registry: std::collections::HashMap<String, fontdue::Font>,
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
    /// Blur pipeline + bind group layout (separate od main)
    blur_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform pro blur direction (0=horizontal, 1=vertical) + radius
    blur_uniform_buf: wgpu::Buffer,
    /// Compose pipeline - samples offscreen_tex a kresli do swap chain.
    /// Pouziva fullscreen triangle + scissor pro region + color matrix uniform.
    compose_pipeline: wgpu::RenderPipeline,
    compose_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform pro compose color matrix (5x vec4 = 80 bytes)
    compose_uniform_buf: wgpu::Buffer,
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
    /// Dirty region tracker - oblast ktera potrebuje prekresleni.
    /// Pouzivano pro budouci incremental rendering optimalizaci.
    pub dirty_region: DirtyRegion,
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
        // Pokus se requestnout DUAL_SOURCE_BLENDING pro real LCD subpixel text.
        // OPT-IN pres env RUST_WEB_ENGINE_LCD=1 - default OFF (bezpecny fallback).
        // WGSL @blend_src(0)/(1) attributes nemusi byt podporene v kazdem
        // wgpu/naga buildu - opt-in zabranuje crash pri shader compile.
        let lcd_opt_in = std::env::var("RUST_WEB_ENGINE_LCD")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let supports_dual_source = lcd_opt_in
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
        eprintln!("[render] dual_source_blending: {}", dual_source_blend);
        let size = window.inner_size();
        let scale_factor = window.scale_factor();
        let surface_caps = surface.get_capabilities(&adapter);
        eprintln!("[render] window inner_size = {}x{} physical, scale_factor = {} (logical = {}x{})",
            size.width, size.height, scale_factor,
            (size.width as f64 / scale_factor) as u32,
            (size.height as f64 / scale_factor) as u32);
        // Pokud winit jeste nedostal WM_SIZE na Windows, inner_size = 0. Fallback na rozumny default.
        let init_w = if size.width > 0 { size.width } else { 1280 };
        let init_h = if size.height > 0 { size.height } else { 900 };
        // Present mode: prefer Mailbox (vsync s drop-old, smoothest na high-Hz monitorech)
        // > Immediate (uncapped, mozna tearing) > Fifo (klasicky vsync 60Hz na vetsine drivers).
        // Mailbox/Immediate tracking native monitor refresh (144Hz/165Hz/240Hz).
        let preferred_modes = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo];
        let present_mode = preferred_modes.iter().copied()
            .find(|m| surface_caps.present_modes.contains(m))
            .unwrap_or(wgpu::PresentMode::Fifo);
        eprintln!("[render] present_mode = {:?} (available: {:?})", present_mode, surface_caps.present_modes);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_caps.formats[0],
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
        let atlas_smp = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            size: 80, // 5x vec4
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
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
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
        let compose_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { multiview_mask: None,
            label: Some("compose_pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
                    binding: 2, visibility: wgpu::ShaderStages::VERTEX,
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            pipeline, lcd_pipeline, uniform_buf,
            atlas_tex, atlas_view, atlas_smp, bind_group_layout, bind_group, atlas,
            image_atlas, image_tex, image_view,
            image_source_bytes: std::collections::HashMap::new(),
            font_registry: std::collections::HashMap::new(),
            loaded_font_urls: std::collections::HashSet::new(),
            color_fonts: std::collections::HashMap::new(),
            offscreen_tex, offscreen_view,
            offscreen_tex_b, offscreen_view_b,
            main_rt,
            blur_pipeline, blur_bind_group_layout, blur_uniform_buf,
            compose_pipeline, compose_bind_group_layout, compose_uniform_buf,
            transform_pipeline, transform_bind_group_layout, transform_uniform_buf,
            webgl_shader_modules: std::collections::HashMap::new(),
            webgl_pipelines: std::collections::HashMap::new(),
            webgl_buffers: std::collections::HashMap::new(),
            webgl_canvas_rts: std::collections::HashMap::new(),
            webgl_uniform_buffers: std::collections::HashMap::new(),
            webgl_uniform_bgls: std::collections::HashMap::new(),
            webgl_textures: std::collections::HashMap::new(),
            webgl_default_sampler: None,
            dirty_region: DirtyRegion::new(),
        }
    }

    /// Upload WebGL texture data do GPU. RGBA bytes (rozmer = w*h*4).
    /// Format: GL_RGBA (0x1908) -> Rgba8UnormSrgb.
    /// Idempotent reupload.
    pub fn upload_webgl_texture(&mut self, texture_id: u32, w: u32, h: u32, format: u32, data: &[u8]) -> bool {
        if w == 0 || h == 0 || data.is_empty() { return false; }
        // Format mapping. GL_RGBA = 0x1908, GL_RGB = 0x1907.
        // Pro Rgb -> dopadovat na Rgba (GPU nepodporuje 24-bit usually).
        let wgpu_format = wgpu::TextureFormat::Rgba8UnormSrgb;
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

    /// Ensure uniform buffer + bind group layout pro program (legacy - jen uniform).
    /// Pri buffer_size=0 nedela nic. Idempotent.
    pub fn ensure_webgl_uniform_resources(&mut self, program_id: u32, buffer_size: u64) {
        self.ensure_webgl_full_resources(program_id, buffer_size, None, &[], &[]);
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
    /// Pokud uniform_bgl exists pro program (po ensure_webgl_uniform_resources),
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
        let w = (bx.rect.width as u32).max(1);
        let h = (bx.rect.height as u32).max(1);
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
        // WebGL clearColor je v sRGB display space. WGPU Color pri sRGB surface
        // format ocekava LINEAR. Pri 0.18 sRGB -> linear ~= 0.025. Bez konverze
        // surface znova encoduje sRGB -> output appears "vyblite".
        fn s2l(s: f32) -> f64 {
            let v = if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
            v as f64
        }
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

        // Pass 1: horizontal RT_a -> RT_b
        let texel_x = 1.0 / self.config.width as f32;
        let params_h = [1.0_f32, 0.0, radius, texel_x];
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
        let params_v = [0.0_f32, 1.0, radius, texel_y];
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
            let url = match extract_font_url(&ff.src) { Some(u) => u, None => continue };
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
                match ureq::get(&final_url).timeout(std::time::Duration::from_secs(15)).call() {
                    Ok(resp) => {
                        let mut buf = Vec::new();
                        if resp.into_reader().read_to_end(&mut buf).is_ok() { Some(buf) } else { None }
                    }
                    Err(e) => { eprintln!("[font-face] HTTP fail {final_url}: {e}"); None }
                }
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
                if let Ok(font) = fontdue::Font::from_bytes(decoded, fontdue::FontSettings::default()) {
                    self.font_registry.insert(ff.family.clone(), font.clone());
                    // Sdilet do atlasu pro rasterize lookup
                    self.atlas.extra_fonts.insert(ff.family.clone(), font);
                    self.loaded_font_urls.insert(url);
                }
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
        let bytes_opt = fetch_image_bytes(fetch_url);
        let bytes = match bytes_opt { Some(b) => b, None => return };
        // Cache source bytes pro budouci re-resample pri zoomu.
        self.image_source_bytes.insert(cache_key.to_string(), bytes.clone());
        if let Ok(img) = image::load_from_memory(&bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let raw = rgba.into_raw();
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
        }
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
        if let Ok(img) = image::load_from_memory(&bytes) {
            let max_atlas = IMAGE_ATLAS_SIZE / 2;
            let cw = target_w.min(max_atlas);
            let ch = target_h.min(max_atlas);
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
        eprintln!("[render] resize physical = {}x{} (scale_factor={}, logical = {}x{})",
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
    }

    fn upload_atlas(&self) {
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
    }

    /// Renderuje display list s podporou filter subtree + backdrop-filter
    /// + WebGL canvas pass v ramci JEDNOHO swap chain frame.
    /// Vse kreslime do main_rt (intermediate RT), na konci compose -> swap chain.
    /// Backdrop-filter muze cist obsah main_rt (scena za elementem).
    pub fn draw_full_frame(
        &mut self,
        cmds: &[DisplayCommand],
        overlay_cmds: &[DisplayCommand],
        layout_root: &super::layout::LayoutBox,
        webgl_states: Option<&std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>>,
        scroll_y: f32,
    ) {
        // Update viewport uniform pro main pipeline
        // Browser zoom: vp uniform = logical dims (window/zoom). Vertex px coords
        // jsou v logical px (layout running at logical viewport). NDC mapping
        // px/vp pak skaluje obsah o zoom faktor pri compose do framebufferu.
        let vp = [self.config.width as f32 / (self.zoom * self.scale_factor), self.config.height as f32 / (self.zoom * self.scale_factor), self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));

        // Dirty rect: cely frame je dirty (aktualne full-redraw)
        self.dirty_region.mark_all(self.config.width as f32, self.config.height as f32);
        let _dirty = self.dirty_region.take(); // reserved pro future incremental render

        // Acquire frame
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return,
        };
        let swap_view = frame.texture.create_view(&Default::default());
        // Main RT view - sem kreslime (ne primo na swap chain)
        let main_rt_view = self.main_rt.create_view(&Default::default());

        // 1. CSS display list (page content) -> main_rt
        let had_segments = self.draw_segments_into_view(&main_rt_view, cmds);

        // 2. WebGL pass -> main_rt (po page contentu, pred overlay)
        let mut webgl_did_render = false;
        if let Some(states) = webgl_states {
            if !states.is_empty() {
                webgl_did_render = self.run_webgl_frame(layout_root, &main_rt_view, states, scroll_y);
            }
        }

        // 3. Overlay (devtools, scrollbars, addr/find bar) -> main_rt PO WebGL,
        // aby UI prvky neprekryl WebGL clear color. start_clear=false zachova
        // existujici page + WebGL obsah.
        let had_overlay = if !overlay_cmds.is_empty() {
            self.draw_segments_into_view_ext(&main_rt_view, overlay_cmds, false)
        } else { false };

        // 4. Composit main_rt -> swap chain
        if had_segments || webgl_did_render || had_overlay {
            let vw = self.config.width as f32;
            let vh = self.config.height as f32;
            self.compose_view_to_swap(&swap_view, &main_rt_view, 0.0, 0.0, vw, vh);
        } else {
            // Nic nekresleno - clear swap chain primo
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                    label: Some("frame_clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                        view: &swap_view, resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 }),
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
    fn draw_segments_into_view(&mut self, view: &wgpu::TextureView, cmds: &[DisplayCommand]) -> bool {
        self.draw_segments_into_view_ext(view, cmds, true)
    }

    /// Pri start_clear=false neclearuje texturu pri prvni passi (Load namisto
    /// Clear). Pouzite pro overlay pass po WebGL - chce zachovat existujici
    /// page + WebGL obsah, jen kreslit overlay nad nim.
    fn draw_segments_into_view_ext(&mut self, view: &wgpu::TextureView, cmds: &[DisplayCommand], start_clear: bool) -> bool {
        if cmds.is_empty() { return false; }
        let segments: Vec<Seg> = partition_filter_segments(cmds);
        if segments.is_empty() { return false; }
        let mut first_pass = start_clear;
        for seg in segments {
            match seg {
                Seg::Main(slice) => {
                    let verts = build_vertices(slice, &self.atlas, &self.image_atlas, self.zoom);
                    self.draw_main_pass(view, &verts, first_pass);
                    first_pass = false;
                }
                Seg::Filter { inner, x, y, w, h, radius, color_matrix } => {
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas, self.zoom);
                    self.draw_to_offscreen(&inner_verts);
                    if radius >= 0.5 {
                        self.run_blur_passes(radius);
                    }
                    self.compose_offscreen(view, x, y, w, h, &color_matrix, first_pass);
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
                            Seg::BackdropFilter { .. } | Seg::Mask { .. } => {
                                // Nested backdrop/mask uvnitr backdrop-filter: skip (nepodporovano)
                            }
                        }
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
        let vp = [self.config.width as f32 / (self.zoom * self.scale_factor), self.config.height as f32 / (self.zoom * self.scale_factor), self.zoom, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let had_segments = self.draw_segments_into_view(&view, cmds);
        if !had_segments {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                    label: Some("clear_only"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                        view: &view, resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 }),
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
        let load = if first {
            wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 })
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
            if !vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
                // LCD subpixel text - second pass s dual-source pipeline.
                // Filter vertices na mode == 9. Hardware si kresli per-channel
                // mask blend (real ClearType-style color fringes).
                if let Some(lcd_pipe) = &self.lcd_pipeline {
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
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let hw = w * 0.5;
        let hh = h * 0.5;
        let z = self.zoom.max(0.0001);
        let vw = self.config.width as f32 / z;
        let vh = self.config.height as f32 / z;
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
    fn compose_offscreen(&self, view: &wgpu::TextureView, x: f32, y: f32, w: f32, h: f32, color_matrix: &[f32; 20], first: bool) {
        // Upload color matrix do uniform: 5x vec4 (rgba per row + offset)
        // Layout: [row0_rgba, row1_rgba, row2_rgba, row3_rgba, offset_rgba]
        let m = color_matrix;
        let uniform_data: [f32; 20] = [
            m[0], m[1], m[2], m[3],
            m[5], m[6], m[7], m[8],
            m[10], m[11], m[12], m[13],
            m[15], m[16], m[17], m[18],
            m[4], m[9], m[14], m[19],
        ];
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
            wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 })
        } else {
            wgpu::LoadOp::Load
        };
        // Scissor: clamp do swap chain rozmeru, integer pixely.
        // x/y/w/h jsou layout (logical) px - prevedeme na physical pres
        // zoom * scale_factor (HiDPI).
        let z = (self.zoom * self.scale_factor).max(0.0001);
        let vw = self.config.width as i32;
        let vh = self.config.height as i32;
        let sx = (x * z).max(0.0) as i32;
        let sy = (y * z).max(0.0) as i32;
        let sw = ((x + w) * z).min(vw as f32) as i32 - sx;
        let sh = ((y + h) * z).min(vh as f32) as i32 - sy;
        let sw = sw.max(0);
        let sh = sh.max(0);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                label: Some("compose_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None,
                    view, resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            if sw > 0 && sh > 0 {
                pass.set_pipeline(&self.compose_pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.set_scissor_rect(sx as u32, sy as u32, sw as u32, sh as u32);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Compose offscreen RT do swap chain pres 3D transform pipeline.
    /// Vykresli quad s 4 rohy transformovanymi 4x4 matici (vc perspective).
    fn compose_transform(&self, view: &wgpu::TextureView, x: f32, y: f32, w: f32, h: f32, matrix: &[f32; 16], first: bool) {
        // UV box: jaka cast offscreen RT obsahuje element. Offscreen RT je
        // viewport size, element je v px (x..x+w, y..y+h). UV = px / viewport.
        // Pro 3D rotace: rozsirime sampling region o 1px na kazde strane, aby
        // bilinear sampler mel kde brat pri sub-pixel sampling rotovaneho
        // quadu. Kdybychom samplovali presne na hrane, edge fragmenty by
        // bledly s prilehlym transparent contentem v offscreen RT a element
        // by vypadal uzsi nez ma byt.
        // Offscreen RT je v physical px, x/y/w/h v logical px. Prevedeme.
        // Offscreen RT je v PHYSICAL px; x/y/w/h v LOGICAL px. UV mapping musi
        // pouzit physical scale = zoom * scale_factor.
        let z = (self.zoom * self.scale_factor).max(0.0001);
        let vw = self.config.width as f32;
        let vh = self.config.height as f32;
        let u0 = (x * z / vw).clamp(0.0, 1.0);
        let v0 = (y * z / vh).clamp(0.0, 1.0);
        let u1 = ((x + w) * z / vw).clamp(0.0, 1.0);
        let v1 = ((y + h) * z / vh).clamp(0.0, 1.0);
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
            wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 })
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
        let vp = [self.config.width as f32 / (self.zoom * self.scale_factor), self.config.height as f32 / (self.zoom * self.scale_factor), self.zoom, 0.0];
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
