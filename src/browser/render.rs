/// wgpu renderer + winit window + frame loop.
///
/// Real implementace - vertex buffer s rectangly + glyph atlas pro text.
/// Display list (paint::DisplayCommand) -> vertex data -> GPU.

use super::paint::DisplayCommand;
use super::devtools_panel::{paint_devtools_panel, find_box_rect_by_id, devtools_hit_test, DevtoolsHit, pick_node_at_screen_pos};
// Re-export pro back-compat (testy + jine moduly).
pub use super::webgl_helpers::{
    webgl_compute_stride, webgl_attrib_to_vertex_format, webgl_serialize_uniforms,
    webgl_extract_pending, webgl_effective_clear, webgl_count_draws, webgl_count_clears,
    webgl_linked_program_ids, webgl_layout_has_canvas, webgl_canvas_count,
};
use bytemuck::{Pod, Zeroable};
use std::rc::Rc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],     // pixel coords
    color: [f32; 4],   // RGBA 0..1
    uv: [f32; 2],      // texture coords (pro text) nebo gradient pos (pro gradient)
    /// Mode: 0=solid, 1=text, 2=gradient, 3=shadow blur
    mode: f32,
    /// Local coords v ramci rectanglu (centered)
    local: [f32; 2],
    /// Half size pro SDF
    half_size: [f32; 2],
    radius: f32,
    /// Druha barva pro gradient (interpolovana z color->color2 podle uv.x)
    color2: [f32; 4],
    /// Blur radius pro shadow
    blur: f32,
}

/// Separable Gaussian blur shader - 2 pass (horizontal + vertical).
/// Sample 9 tapu s gauss vahami.
const BLUR_SHADER: &str = r#"
struct BlurParams {
    /// direction.x = 1 horizontal, .y = 1 vertical
    direction: vec2<f32>,
    /// blur radius in pixels
    radius: f32,
    /// texel size 1/width or 1/height
    texel: f32,
};
@group(0) @binding(0) var<uniform> params: BlurParams;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_smp: sampler;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    // Fullscreen triangle
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VertexOut;
    out.clip = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = uv[idx];
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // 9-tap Gaussian (sigma ~ radius/3). Unrolled - WGSL nepovoluje dynamic
    // indexing var/let array v Naga validation (jen const-indexable v uniform/storage).
    let w0: f32 = 0.227027;
    let w1: f32 = 0.1945946;
    let w2: f32 = 0.1216216;
    let w3: f32 = 0.054054;
    let w4: f32 = 0.016216;
    let step = params.direction * params.texel * params.radius * 0.3;
    var color = textureSample(src_tex, src_smp, in.uv) * w0;
    let off1 = step * 1.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off1) * w1;
    color = color + textureSample(src_tex, src_smp, in.uv - off1) * w1;
    let off2 = step * 2.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off2) * w2;
    color = color + textureSample(src_tex, src_smp, in.uv - off2) * w2;
    let off3 = step * 3.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off3) * w3;
    color = color + textureSample(src_tex, src_smp, in.uv - off3) * w3;
    let off4 = step * 4.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off4) * w4;
    color = color + textureSample(src_tex, src_smp, in.uv - off4) * w4;
    return color;
}
"#;

/// 3D Transform compose shader. Renderuje 4-vertex quad transformovany 4x4
/// matici (vc perspective) v px space, sample z offscreen_tex pres uv region.
const TRANSFORM_SHADER: &str = r#"
struct TransformParams {
    /// 4x4 row-major matrix (CSS transform incl. parent perspective).
    /// V WGSL ulozeno jako 4 vec4 (po radkach).
    row0: vec4<f32>,
    row1: vec4<f32>,
    row2: vec4<f32>,
    row3: vec4<f32>,
    /// (cx, cy, hw, hh) - center bbox v px + half-size.
    center: vec4<f32>,
    /// (viewport_w, viewport_h, _, _)
    viewport: vec4<f32>,
    /// (u0, v0, u1, v1) - region z offscreen RT k samplovani.
    uv_box: vec4<f32>,
};
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var<uniform> tp: TransformParams;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VOut {
    // 6 vrcholu pro dva trianlges (0,1,2 + 0,2,3)
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 4>(
        vec2<f32>(tp.uv_box.x, tp.uv_box.y),
        vec2<f32>(tp.uv_box.z, tp.uv_box.y),
        vec2<f32>(tp.uv_box.z, tp.uv_box.w),
        vec2<f32>(tp.uv_box.x, tp.uv_box.w),
    );
    var ix = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let i = ix[idx];
    let c = corners[i];
    // Local px space (centered)
    let lx = c.x * tp.center.z;
    let ly = c.y * tp.center.w;
    let p = vec4<f32>(lx, ly, 0.0, 1.0);
    // Apply matrix (row-major: dot s row vec4)
    let tx = dot(tp.row0, p);
    let ty = dot(tp.row1, p);
    let tz = dot(tp.row2, p);
    let tw = dot(tp.row3, p);
    // Perspective divide
    let inv_w = 1.0 / max(tw, 0.0001);
    let px = tx * inv_w + tp.center.x;
    let py = ty * inv_w + tp.center.y;
    let nx = (px / tp.viewport.x) * 2.0 - 1.0;
    let ny = 1.0 - (py / tp.viewport.y) * 2.0;
    let nz = clamp(tz * inv_w, -1.0, 1.0);
    var out: VOut;
    out.clip = vec4<f32>(nx, ny, nz, 1.0);
    out.uv = uvs[i];
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_smp, in.uv);
}
"#;

/// Compose shader - samples offscreen_tex (mid-blur result) a kresli do swap chain.
/// Aplikuje 4x5 color matrix (vetsina filter operaci) - identity = passthrough.
const COMPOSE_SHADER: &str = r#"
struct ComposeParams {
    /// 4x5 color matrix (4 vec4 row + 4-element offset vector).
    /// row[i] = m[i*5..i*5+4], offset[i] = m[i*5+4].
    /// WGSL: dva pole pres 4x mat4x4 by stacilo, ale pripravime 5 vec4 (16+16 bytes navic):
    /// row0(rgba), row1(rgba), row2(rgba), row3(rgba), offset(rgba).
    row0: vec4<f32>,
    row1: vec4<f32>,
    row2: vec4<f32>,
    row3: vec4<f32>,
    offset: vec4<f32>,
};
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var<uniform> params: ComposeParams;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VertexOut;
    out.clip = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = uv[idx];
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let src = textureSample(src_tex, src_smp, in.uv);
    let r = dot(params.row0, src) + params.offset.x;
    let g = dot(params.row1, src) + params.offset.y;
    let b = dot(params.row2, src) + params.offset.z;
    let a = dot(params.row3, src) + params.offset.w;
    return vec4<f32>(r, g, b, a);
}
"#;

const RECT_SHADER: &str = r#"
struct Uniforms {
    viewport: vec2<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_smp: sampler;
@group(0) @binding(3) var image_tex: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mode: f32,
    @location(4) local: vec2<f32>,
    @location(5) half_size: vec2<f32>,
    @location(6) radius: f32,
    @location(7) color2: vec4<f32>,
    @location(8) blur: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mode: f32,
    @location(3) local: vec2<f32>,
    @location(4) half_size: vec2<f32>,
    @location(5) radius: f32,
    @location(6) color2: vec4<f32>,
    @location(7) blur: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let x = (in.pos.x / u.viewport.x) * 2.0 - 1.0;
    let y = 1.0 - (in.pos.y / u.viewport.y) * 2.0;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    out.mode = in.mode;
    out.local = in.local;
    out.half_size = in.half_size;
    out.radius = in.radius;
    out.color2 = in.color2;
    out.blur = in.blur;
    return out;
}

/// Signed distance to rounded rectangle.
fn sdf_rounded_box(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Mode 1: text - sample atlas
    if (in.mode > 0.5 && in.mode < 1.5) {
        let alpha = textureSample(atlas_tex, atlas_smp, in.uv).r;
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Mode 2: linear gradient - lerp color->color2 podle uv.x (pre-rotated)
    if (in.mode > 1.5 && in.mode < 2.5) {
        let t = clamp(in.uv.x, 0.0, 1.0);
        var rgba = mix(in.color, in.color2, t);
        if (in.radius > 0.5) {
            let d = sdf_rounded_box(in.local, in.half_size, in.radius);
            let aa = 1.0 - smoothstep(-1.0, 1.0, d);
            rgba = vec4<f32>(rgba.rgb, rgba.a * aa);
        }
        return rgba;
    }
    // Mode 3: shadow with blur (Gaussian-like fade)
    if (in.mode > 2.5 && in.mode < 3.5) {
        let blur = max(in.blur, 1.0);
        let d = sdf_rounded_box(in.local, in.half_size, in.radius);
        let alpha = 1.0 - smoothstep(-blur, blur, d);
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Mode 6: radial gradient - t = dist(local, grad_center) / grad_radius
    if (in.mode > 5.5 && in.mode < 6.5) {
        let d = length(in.local - in.half_size);
        let t = clamp(d / max(in.blur, 1.0), 0.0, 1.0);
        var rgba = mix(in.color, in.color2, t);
        if (in.radius > 0.5) {
            // Pro border-radius musim recover pravy half_size - aproximace:
            // local je relativni k box stredu, takze max abs hodnota je half_size.
            let bbox = vec2<f32>(abs(in.local.x), abs(in.local.y));
            let approx_hs = vec2<f32>(max(bbox.x, in.radius * 2.0), max(bbox.y, in.radius * 2.0));
            let dd = sdf_rounded_box(in.local, approx_hs, in.radius);
            let aa = 1.0 - smoothstep(-1.0, 1.0, dd);
            rgba = vec4<f32>(rgba.rgb, rgba.a * aa);
        }
        return rgba;
    }
    // Mode 7: conic gradient - t = (atan2(p.y, p.x) - start) / 2pi
    if (in.mode > 6.5 && in.mode < 7.5) {
        let p = in.local - in.half_size;
        var ang = atan2(p.y, p.x) - in.blur;
        // Normalize do 0..2pi (-> 0..1)
        let two_pi = 6.28318530718;
        ang = ang - floor(ang / two_pi) * two_pi;
        let t = clamp(ang / two_pi, 0.0, 1.0);
        return mix(in.color, in.color2, t);
    }
    // Mode 5: inset shadow - kresli uvnitr boxu, fade smerem dovnitr od okraju
    if (in.mode > 4.5 && in.mode < 5.5) {
        let blur = max(in.blur, 1.0);
        // Offset shift sample center: pri offset (ox, oy) shadow se posune v opacnem smeru
        let p = in.local - vec2<f32>(in.color2.x, in.color2.y);
        let d = sdf_rounded_box(p, in.half_size, in.radius);
        // Pozitivni d = vne boxu = nezobrazi (mimo cliping kvadr)
        // Negativni d = uvnitr -> alpha podle vzdalenosti od okraje
        let alpha = smoothstep(-blur, blur, d);
        // Clip mimo box
        let outer = sdf_rounded_box(in.local, in.half_size, in.radius);
        let clip = 1.0 - smoothstep(-1.0, 1.0, outer);
        return vec4<f32>(in.color.rgb, in.color.a * alpha * clip);
    }
    // Mode 8: blurred solid - smoothstep s blur radius na okrajich
    if (in.mode > 7.5 && in.mode < 8.5) {
        let blur = max(in.blur, 0.5);
        let d = sdf_rounded_box(in.local, in.half_size, in.radius);
        // Inside box (d < -blur): full alpha
        // Outside (d > blur): zero alpha
        // Edge: smoothstep
        let alpha = 1.0 - smoothstep(-blur, blur, d);
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Mode 4: image - sample RGBA z image atlasu
    if (in.mode > 3.5 && in.mode < 4.5) {
        var rgba = textureSample(image_tex, atlas_smp, in.uv);
        rgba.a = rgba.a * in.color.a;
        if (in.radius > 0.5) {
            let d = sdf_rounded_box(in.local, in.half_size, in.radius);
            let aa = 1.0 - smoothstep(-1.0, 1.0, d);
            rgba.a = rgba.a * aa;
        }
        return rgba;
    }
    // Mode 0: solid s rounded corners
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
fn build_vertices(commands: &[DisplayCommand], atlas: &GlyphAtlas, image_atlas: &ImageAtlas) -> Vec<Vertex> {
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
            DisplayCommand::Text { x, y, content, color, font_size, bold: _, italic, font_family, strikethrough, underline } => {
                let c = normalize_color(color);
                let mut pen_x = *x;
                let pen_y = *y + *font_size;
                // Italic = fake skew transform na glyph quady (x += y * 0.2).
                let italic_skew: f32 = if *italic { 0.2 } else { 0.0 };
                let start_x = pen_x;
                for ch in content.chars() {
                    // Color glyph (COLR) check pres synthetic image_atlas key.
                    let colr_key = format!("__colr:{}:{}:{}", font_family, ch as u32, *font_size as u32);
                    if let Some(info) = image_atlas.get(&colr_key) {
                        // Place color glyph jako Image quad. Y baseline-aligned.
                        let gx = pen_x;
                        let gy = pen_y - info.height;
                        push_image(&mut verts, gx, gy, info.width, info.height, info.uv0, info.uv1, 0.0);
                        pen_x += info.width;
                        continue;
                    }
                    if let Some(g) = atlas.get(font_family, ch, *font_size as u32) {
                        let gx = pen_x + g.bearing_x;
                        let gy = pen_y - g.bearing_y;
                        if italic_skew != 0.0 {
                            let baseline_offset = (pen_y - gy) * italic_skew;
                            push_rect_uv(&mut verts, gx + baseline_offset, gy, g.width, g.height, c, g.uv0, g.uv1, 1.0);
                        } else {
                            push_rect_uv(&mut verts, gx, gy, g.width, g.height, c, g.uv0, g.uv1, 1.0);
                        }
                        pen_x += g.advance;
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
                    // Fallback: placeholder seda kdyz image neni v atlase
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
            }
        }
    }
    verts
}

/// Emituje DisplayCommands pro canvas tag z canvas_ops storage.
fn paint_canvas_ops(
    bx: &super::layout::LayoutBox,
    ops_storage: &std::collections::HashMap<usize, Vec<super::paint::CanvasOp>>,
    cmds: &mut Vec<super::paint::DisplayCommand>,
) {
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if let Some(ops) = ops_storage.get(&ptr) {
                use super::paint::{CanvasOp, DisplayCommand};
                let mut current_fill: [u8; 4] = [0, 0, 0, 255];
                let mut current_stroke: [u8; 4] = [0, 0, 0, 255];
                let mut current_lw: f32 = 1.0;
                let mut current_font_size: f32 = 14.0;
                // Path state
                let mut path_points: Vec<(f32, f32)> = Vec::new();
                let mut path_arcs: Vec<(f32, f32, f32)> = Vec::new(); // (cx, cy, r)
                let ox = bx.rect.x;
                let oy = bx.rect.y;
                for op in ops {
                    match op {
                        CanvasOp::FillStyle(c) => current_fill = *c,
                        CanvasOp::StrokeStyle(c) => current_stroke = *c,
                        CanvasOp::LineWidth(w) => current_lw = *w,
                        CanvasOp::Font { size, .. } => current_font_size = *size,
                        CanvasOp::BeginPath => {
                            path_points.clear();
                            path_arcs.clear();
                        }
                        CanvasOp::MoveTo { x, y } | CanvasOp::LineTo { x, y } => {
                            path_points.push((ox + *x, oy + *y));
                        }
                        CanvasOp::Arc { cx, cy, r, .. } => {
                            path_arcs.push((ox + *cx, oy + *cy, *r));
                        }
                        CanvasOp::ClosePath => {
                            if let (Some(first), Some(last)) = (path_points.first().copied(), path_points.last().copied()) {
                                if first != last { path_points.push(first); }
                            }
                        }
                        CanvasOp::Stroke => {
                            // Pro path_points: kresli ax-aligned line segmenty (zjednoduseni)
                            for w in path_points.windows(2) {
                                let (x1, y1) = w[0];
                                let (x2, y2) = w[1];
                                if (y1 - y2).abs() < 0.5 {
                                    cmds.push(super::paint::DisplayCommand::Rect {
                                        x: x1.min(x2), y: y1 - current_lw / 2.0,
                                        w: (x1 - x2).abs(), h: current_lw,
                                        color: current_stroke, radius: 0.0,
                                    });
                                } else if (x1 - x2).abs() < 0.5 {
                                    cmds.push(super::paint::DisplayCommand::Rect {
                                        x: x1 - current_lw / 2.0, y: y1.min(y2),
                                        w: current_lw, h: (y1 - y2).abs(),
                                        color: current_stroke, radius: 0.0,
                                    });
                                } else {
                                    // Diagonal - aproximace pres axis-aligned mensich segmentu
                                    let dx = x2 - x1; let dy = y2 - y1;
                                    let steps = ((dx.abs() + dy.abs()) / 2.0).max(1.0) as i32;
                                    for i in 0..steps {
                                        let t = i as f32 / steps as f32;
                                        let x = x1 + dx * t;
                                        let y = y1 + dy * t;
                                        cmds.push(super::paint::DisplayCommand::Rect {
                                            x: x - current_lw / 2.0, y: y - current_lw / 2.0,
                                            w: current_lw, h: current_lw,
                                            color: current_stroke, radius: 0.0,
                                        });
                                    }
                                }
                            }
                            // Arcs jako rounded rect outline aproximace
                            for (cx, cy, r) in &path_arcs {
                                cmds.push(super::paint::DisplayCommand::Border {
                                    x: cx - r, y: cy - r,
                                    w: 2.0 * r, h: 2.0 * r,
                                    width: current_lw, color: current_stroke,
                                });
                            }
                        }
                        CanvasOp::Fill => {
                            // Fill: pro arc - emit rect s plnym radius
                            for (cx, cy, r) in &path_arcs {
                                cmds.push(super::paint::DisplayCommand::Rect {
                                    x: cx - r, y: cy - r,
                                    w: 2.0 * r, h: 2.0 * r,
                                    color: current_fill, radius: *r,
                                });
                            }
                            // Polygon fill: bounding box approx
                            if path_points.len() >= 3 {
                                let xs: Vec<f32> = path_points.iter().map(|p| p.0).collect();
                                let ys: Vec<f32> = path_points.iter().map(|p| p.1).collect();
                                let xmin = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                                let xmax = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                                let ymin = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                                let ymax = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                                cmds.push(super::paint::DisplayCommand::Rect {
                                    x: xmin, y: ymin,
                                    w: xmax - xmin, h: ymax - ymin,
                                    color: current_fill, radius: 0.0,
                                });
                            }
                        }
                        CanvasOp::FillRect { x, y, w, h } => {
                            cmds.push(DisplayCommand::Rect {
                                x: ox + *x, y: oy + *y, w: *w, h: *h,
                                color: current_fill, radius: 0.0,
                            });
                        }
                        CanvasOp::StrokeRect { x, y, w, h } => {
                            cmds.push(DisplayCommand::Border {
                                x: ox + *x, y: oy + *y, w: *w, h: *h,
                                width: current_lw, color: current_stroke,
                            });
                        }
                        CanvasOp::ClearRect { x, y, w, h } => {
                            // Clear = bg cerny (canvas default)
                            cmds.push(DisplayCommand::Rect {
                                x: ox + *x, y: oy + *y, w: *w, h: *h,
                                color: [0, 0, 0, 255], radius: 0.0,
                            });
                        }
                        CanvasOp::FillText { text, x, y } => {
                            cmds.push(DisplayCommand::Text {
                                x: ox + *x, y: oy + *y - current_font_size,
                                content: text.clone(),
                                color: current_fill,
                                font_size: current_font_size,
                                bold: false,
                                italic: false,
                                font_family: String::new(),
                                strikethrough: false, underline: false,
                            });
                        }
                        // Nove operace - paint stub (state pres apply uvnitr render pipeline TODO)
                        CanvasOp::DrawImage { src, dx, dy, dw, dh } => {
                            cmds.push(DisplayCommand::Image {
                                x: bx.rect.x + dx, y: bx.rect.y + dy,
                                w: *dw, h: *dh,
                                src: src.clone(),
                                radius: 0.0,
                            });
                        }
                        CanvasOp::DrawImageSrc { src, dx, dy, dw, dh, .. } => {
                            // Sub-rect varianta - zatim ignorujeme src crop, kresleme cely image do dest
                            cmds.push(DisplayCommand::Image {
                                x: bx.rect.x + dx, y: bx.rect.y + dy,
                                w: *dw, h: *dh,
                                src: src.clone(),
                                radius: 0.0,
                            });
                        }
                        CanvasOp::PathRect { x, y, w, h } => {
                            // Pridame 4 body do path (alternativa k MoveTo/LineTo)
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                        }
                        CanvasOp::RoundRect { x, y, w, h, radius: _ } => {
                            // Approximace bez radius zatim
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                        }
                        CanvasOp::Ellipse { cx, cy, rx, ry, .. } => {
                            // Approximace 16 bodu
                            for i in 0..=16 {
                                let t = (i as f32) * std::f32::consts::TAU / 16.0;
                                let px = bx.rect.x + cx + rx * t.cos();
                                let py = bx.rect.y + cy + ry * t.sin();
                                path_points.push((px, py));
                            }
                        }
                        CanvasOp::QuadraticCurveTo { x, y, .. }
                        | CanvasOp::BezierCurveTo { x, y, .. }
                        | CanvasOp::ArcTo { x2: x, y2: y, .. } => {
                            // Approximace - end point only (TODO: skutecna interpolace)
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                        }
                        CanvasOp::StrokeText { text, x, y } => {
                            cmds.push(DisplayCommand::Text {
                                x: bx.rect.x + x,
                                y: bx.rect.y + y,
                                content: text.clone(),
                                color: current_stroke,
                                font_size: current_font_size,
                                bold: false,
                                italic: false,
                                font_family: String::new(),
                                strikethrough: false, underline: false,
                            });
                        }
                        // State / transform / styling ops - state-only, render je read-only zatim
                        CanvasOp::Save | CanvasOp::Restore
                        | CanvasOp::Translate { .. } | CanvasOp::Rotate { .. }
                        | CanvasOp::Scale { .. } | CanvasOp::SetTransform { .. }
                        | CanvasOp::Transform { .. } | CanvasOp::ResetTransform
                        | CanvasOp::GlobalAlpha(_) | CanvasOp::GlobalCompositeOperation(_)
                        | CanvasOp::Clip
                        | CanvasOp::LineCap(_) | CanvasOp::LineJoin(_)
                        | CanvasOp::MiterLimit(_) | CanvasOp::LineDash(_)
                        | CanvasOp::LineDashOffset(_)
                        | CanvasOp::TextAlign(_) | CanvasOp::TextBaseline(_)
                        | CanvasOp::ShadowColor(_) | CanvasOp::ShadowBlur(_)
                        | CanvasOp::ShadowOffsetX(_) | CanvasOp::ShadowOffsetY(_)
                        | CanvasOp::FillStyleLinearGradient { .. }
                        | CanvasOp::FillStyleRadialGradient { .. } => {
                            // No-op v render-stub. Plna impl by drzela state per-op.
                        }
                    }
                }
            }
        }
    }
    for child in &bx.children {
        paint_canvas_ops(child, ops_storage, cmds);
    }
}

/// Walk layout tree + pro kazdy canvas tag, pokud existuje WebGLState,
/// drainuje queue a emituje display commands. Phase 3b: jen Clear color
/// jako solid Rect bg + DrawArrays stripe overlay placeholder.
/// Pro real GPU draw integration phase 3c5+ vyzaduje refactor (dual path
/// konflict s run_webgl_frame).
pub fn paint_webgl_canvases(
    bx: &super::layout::LayoutBox,
    webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
    cmds: &mut Vec<super::paint::DisplayCommand>,
) {
    use crate::interpreter::WebGLDrawCmd;
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if let Some(state_rc) = webgl_states.get(&ptr) {
                let mut state = state_rc.borrow_mut();
                // Drain queue. Effektivni clear color = posledni ClearColor command.
                // Pri Clear command s COLOR_BUFFER_BIT (0x4000), vyplnime canvas barvou.
                let mut last_clear_color: Option<[f32; 4]> = None;
                let mut had_clear = false;
                let mut draw_commands_count: usize = 0;
                for cmd in state.draw_queue.drain(..) {
                    match cmd {
                        WebGLDrawCmd::ClearColor(c) => last_clear_color = Some(c),
                        WebGLDrawCmd::Clear(mask) => {
                            if mask & 0x4000 != 0 {
                                had_clear = true;
                            }
                        }
                        WebGLDrawCmd::DrawArrays { .. } | WebGLDrawCmd::DrawElements { .. } => {
                            draw_commands_count += 1;
                        }
                    }
                }
                // Aplikace: pokud bylo Clear + last_clear_color, fill canvas.
                let bg_color = if had_clear {
                    last_clear_color.or(Some(state.clear_color))
                } else {
                    None
                };
                if let Some(c) = bg_color {
                    let rgba: [u8; 4] = [
                        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                    ];
                    cmds.push(super::paint::DisplayCommand::Rect {
                        x: bx.rect.x, y: bx.rect.y,
                        w: bx.rect.width, h: bx.rect.height,
                        color: rgba, radius: 0.0,
                    });
                }
                // Phase 3c placeholder: pri DrawArrays/DrawElements, emitujem
                // overlay rect (semi-transparent stripes) jako vizualni indikator
                // ze JS volal draw call. Real wgpu pipeline v dalsi fazi.
                if draw_commands_count > 0 {
                    let stripe_count = (draw_commands_count.min(8)) as i32;
                    let stripe_h = bx.rect.height / stripe_count.max(1) as f32;
                    for i in 0..stripe_count {
                        let alpha = ((i as f32 + 1.0) / stripe_count as f32 * 80.0) as u8;
                        cmds.push(super::paint::DisplayCommand::Rect {
                            x: bx.rect.x,
                            y: bx.rect.y + (i as f32) * stripe_h,
                            w: bx.rect.width,
                            h: stripe_h * 0.5,
                            color: [255, 255, 255, alpha],
                            radius: 0.0,
                        });
                    }
                }
                // Diagnostic - draw_commands_count se uchova ve state pro test access.
                state.draw_call_count = state.draw_call_count.saturating_add(draw_commands_count as u32);
            }
        }
    }
    for child in &bx.children {
        paint_webgl_canvases(child, webgl_states, cmds);
    }
}

/// Najdi DOM node v stromu podle Rc::as_ptr hodnoty (use ve cascade).
/// Aplikuje animation/transition hodnoty z style_map na cached layout boxes.
/// Pouziva se kdyz cache je valid pro layout struktury, ale paint props
/// (transform/opacity/color/filter) se menily kazdy frame pres animations.
fn apply_paint_animations(box_: &mut crate::browser::layout::LayoutBox,
                           style_map: &crate::browser::cascade::StyleMap) {
    let node_id = box_.node.as_ref().map(|n| Rc::as_ptr(n) as usize).unwrap_or(0);
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
    }
    for ch in &mut box_.children {
        apply_paint_animations(ch, style_map);
    }
}

/// Eval JS via bytecode VM s globals z Interpreter env. Pri compile failure
/// nebo runtime error vrati Err s message.
fn console_eval_via_vm(src: &str, interp: &crate::interpreter::Interpreter) -> Result<crate::interpreter::JsValue, String> {
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::interpreter::bytecode::{compile_program, VM};

    let lex = Lexer::parse_str(src, "<console>").map_err(|e| format!("Lexer: {:?}", e))?;
    let mut parser = Parser::new(lex.tokens.clone());
    let program = parser.parse().map_err(|e| format!("Parser: {:?}", e))?;
    let code = compile_program(&program.body).map_err(|e| format!("Compile: {}", e))?;
    let mut vm = VM::with_env(interp.global.clone());
    vm.run(&code).map_err(|e| format!("Runtime: {}", e))
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


/// Segment displej listu pro renderer: Main = normal, Filter = RT-mediated.
pub enum Seg<'a> {
    Main(&'a [DisplayCommand]),
    Filter {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        radius: f32,
        color_matrix: [f32; 20],
    },
    Transform3D {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        matrix: [f32; 16],
    },
    /// Backdrop-filter: snapshotne main_rt (scenu za elementem), aplikuje
    /// blur + color matrix, composit jako podklad, pak inner obsah nahoru.
    BackdropFilter {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        radius: f32,
        color_matrix: [f32; 20],
    },
    /// mask-image subtree: render content do RT, aplikuj alpha mask, composit.
    Mask {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        mask_src: String,
    },
}

/// Rozdeli display list na Main + Filter + Transform3D segmenty pres
/// FilterBegin/End a TransformBegin/End markery. Pri vnoreni: prvni Begin
/// marker urci typ top-level segmentu, vnorene Begin/End jineho typu
/// jsou soucasti inner cmds (ne novy segment).
/// Symetricnost markeru je predpokladana.
pub fn partition_filter_segments(cmds: &[DisplayCommand]) -> Vec<Seg<'_>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind { Filter, Transform, Backdrop, Mask }
    let mut segments: Vec<Seg> = Vec::new();
    let mut depth: i32 = 0;
    let mut active_kind: Option<Kind> = None;
    let mut cursor: usize = 0;
    let mut seg_start: usize = 0;
    let mut filter_params: (f32, f32, f32, f32, f32, [f32; 20]) =
        (0.0, 0.0, 0.0, 0.0, 0.0, [0.0; 20]);
    let mut tx_params: (f32, f32, f32, f32, [f32; 16]) =
        (0.0, 0.0, 0.0, 0.0, [0.0; 16]);
    let mut backdrop_params: (f32, f32, f32, f32, f32, [f32; 20]) =
        (0.0, 0.0, 0.0, 0.0, 0.0, [0.0; 20]);
    let mut mask_params: (f32, f32, f32, f32, String) =
        (0.0, 0.0, 0.0, 0.0, String::new());
    for i in 0..cmds.len() {
        match &cmds[i] {
            DisplayCommand::FilterBegin { x, y, w, h, blur_radius, color_matrix } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    filter_params = (*x, *y, *w, *h, *blur_radius, *color_matrix);
                    active_kind = Some(Kind::Filter);
                }
                if active_kind == Some(Kind::Filter) { depth += 1; }
            }
            DisplayCommand::FilterEnd => {
                if active_kind == Some(Kind::Filter) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, r, m) = filter_params;
                        segments.push(Seg::Filter {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, radius: r, color_matrix: m,
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            DisplayCommand::TransformBegin { x, y, w, h, matrix } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    tx_params = (*x, *y, *w, *h, *matrix);
                    active_kind = Some(Kind::Transform);
                }
                if active_kind == Some(Kind::Transform) { depth += 1; }
            }
            DisplayCommand::TransformEnd => {
                if active_kind == Some(Kind::Transform) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, m) = tx_params;
                        segments.push(Seg::Transform3D {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, matrix: m,
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            DisplayCommand::BackdropFilterBegin { x, y, w, h, blur_radius, color_matrix } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    backdrop_params = (*x, *y, *w, *h, *blur_radius, *color_matrix);
                    active_kind = Some(Kind::Backdrop);
                }
                if active_kind == Some(Kind::Backdrop) { depth += 1; }
            }
            DisplayCommand::BackdropFilterEnd => {
                if active_kind == Some(Kind::Backdrop) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, r, m) = backdrop_params;
                        segments.push(Seg::BackdropFilter {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, radius: r, color_matrix: m,
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            DisplayCommand::MaskBegin { x, y, w, h, mask_src } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    mask_params = (*x, *y, *w, *h, mask_src.clone());
                    active_kind = Some(Kind::Mask);
                }
                if active_kind == Some(Kind::Mask) { depth += 1; }
            }
            DisplayCommand::MaskEnd => {
                if active_kind == Some(Kind::Mask) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, ref src) = mask_params;
                        segments.push(Seg::Mask {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, mask_src: src.clone(),
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            _ => {}
        }
    }
    if cursor < cmds.len() {
        segments.push(Seg::Main(&cmds[cursor..]));
    }
    segments
}

/// Posune Y souradnice display command (pro scroll).
fn shift_command_y(cmd: &mut DisplayCommand, dy: f32) {
    match cmd {
        DisplayCommand::Rect { y, .. }
        | DisplayCommand::Border { y, .. }
        | DisplayCommand::Text { y, .. }
        | DisplayCommand::Gradient { y, .. }
        | DisplayCommand::Shadow { y, .. }
        | DisplayCommand::Image { y, .. }
        | DisplayCommand::BlurredRect { y, .. }
        | DisplayCommand::FilterBegin { y, .. }
        | DisplayCommand::BackdropFilterBegin { y, .. }
        | DisplayCommand::TransformBegin { y, .. }
        | DisplayCommand::MaskBegin { y, .. } => *y += dy,
        DisplayCommand::ClippedRect { points, .. } => {
            for (_, py) in points.iter_mut() {
                *py += dy;
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd => {}
    }
}

/// Push rect with rounded corners (SDF rendering).
fn push_rect_rounded(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                     color: [f32; 4], radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    let make = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [0.0, 0.0],
            mode: 0.0,
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
            color2: [0.0; 4],
            blur: 0.0,
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
             color: [f32; 4], uv: [f32; 2], mode: f32) {
    push_rect_uv(verts, x, y, w, h, color, uv, [uv[0], uv[1]], mode);
}

fn push_rect_uv(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                color: [f32; 4], uv0: [f32; 2], uv1: [f32; 2], mode: f32) {
    let mk = |px: f32, py: f32, u: f32, v: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [u, v],
            mode,
            local: [0.0, 0.0],
            half_size: [0.0, 0.0],
            radius: 0.0,
            color2: [0.0; 4],
            blur: 0.0,
        }
    };
    let tl = mk(x,     y,     uv0[0], uv0[1]);
    let tr = mk(x + w, y,     uv1[0], uv0[1]);
    let bl = mk(x,     y + h, uv0[0], uv1[1]);
    let br = mk(x + w, y + h, uv1[0], uv1[1]);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Cross product (b - a) x (c - a) v 2D (z-component).
fn poly_cross(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// Test ze bod p je uvnitr trojuhelniku (a, b, c) - barycentric.
fn point_in_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let d1 = poly_cross(p, a, b);
    let d2 = poly_cross(p, b, c);
    let d3 = poly_cross(p, c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

/// Spocita signed area polygonu - standardni shoelace formula.
/// V screen-space (y down): CW polygon -> kladne, CCW -> zaporne.
/// V matematickem (y up) je obracene.
pub fn polygon_signed_area(points: &[(f32, f32)]) -> f32 {
    if points.len() < 3 { return 0.0; }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];
        sum += p1.0 * p2.1 - p2.0 * p1.1;
    }
    sum * 0.5
}

/// Ear-clipping triangulace. Vraci trojuhelniky jako (P0, P1, P2) tuples.
/// Funguje pro convex i concave (simple) polygon. Pro self-intersecting ne.
/// Pri failure (degenerate) fallback na fan triangulation.
pub fn triangulate_polygon(points: &[(f32, f32)]) -> Vec<((f32, f32), (f32, f32), (f32, f32))> {
    if points.len() < 3 { return Vec::new(); }
    if points.len() == 3 {
        return vec![(points[0], points[1], points[2])];
    }
    let mut remaining: Vec<(f32, f32)> = points.to_vec();
    let mut triangles = Vec::new();
    // Detekce winding pro ear convexity check.
    // V screen-space (y down): CW polygon -> signed_area > 0, CCW -> < 0.
    // Convex ear cross sign musi sledovat winding znamenku.
    let area_sign = if polygon_signed_area(&remaining) >= 0.0 { 1.0 } else { -1.0 };
    let max_iter = remaining.len() * remaining.len();
    let mut iter_count = 0;
    while remaining.len() > 3 && iter_count < max_iter {
        iter_count += 1;
        let n = remaining.len();
        let mut found_ear: Option<usize> = None;
        for i in 0..n {
            let prev = remaining[(i + n - 1) % n];
            let curr = remaining[i];
            let next = remaining[(i + 1) % n];
            // Convex check vzhledem k winding.
            let cross_v = poly_cross(prev, curr, next);
            // Pri CW screen polygonu (area > 0): convex ear ma cross > 0.
            // Pri CCW (area < 0): cross < 0.
            if cross_v * area_sign <= 0.0 { continue; }
            // No other vertex inside triangle
            let mut contains = false;
            for j in 0..n {
                if j == i || j == (i + n - 1) % n || j == (i + 1) % n { continue; }
                if point_in_triangle(remaining[j], prev, curr, next) {
                    contains = true;
                    break;
                }
            }
            if !contains {
                found_ear = Some(i);
                break;
            }
        }
        match found_ear {
            Some(i) => {
                let n = remaining.len();
                let prev = remaining[(i + n - 1) % n];
                let curr = remaining[i];
                let next = remaining[(i + 1) % n];
                triangles.push((prev, curr, next));
                remaining.remove(i);
            }
            None => {
                // Failed - fallback fan na zbytek
                let p0 = remaining[0];
                for k in 1..remaining.len() - 1 {
                    triangles.push((p0, remaining[k], remaining[k + 1]));
                }
                return triangles;
            }
        }
    }
    if remaining.len() == 3 {
        triangles.push((remaining[0], remaining[1], remaining[2]));
    }
    triangles
}

/// Push 3-vertex triangle pro polygon clip-path (mode 0 = solid).
fn push_triangle(verts: &mut Vec<Vertex>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), color: [f32; 4]) {
    let mk = |p: (f32, f32)| -> Vertex {
        Vertex {
            pos: [p.0, p.1],
            color,
            uv: [0.0, 0.0],
            mode: 0.0,
            local: [0.0, 0.0],
            half_size: [0.0, 0.0],
            radius: 0.0,
            color2: [0.0; 4],
            blur: 0.0,
        }
    };
    verts.push(mk(p0));
    verts.push(mk(p1));
    verts.push(mk(p2));
}

/// Blurred rect: mode 8, solid color s smoothstep blur edge.
fn push_blurred_rect(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                     color: [f32; 4], radius: f32, blur: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    // Rozsirit quad o blur radius pro smoothstep prostor
    let extra = blur + 4.0;
    let mk = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [0.0, 0.0],
            mode: 8.0,
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
            color2: [0.0; 4],
            blur,
        }
    };
    let tl = mk(x - extra,     y - extra);
    let tr = mk(x + w + extra, y - extra);
    let bl = mk(x - extra,     y + h + extra);
    let br = mk(x + w + extra, y + h + extra);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Image quad: mode 4, sample z image atlasu pres UV. SDF rounded corners pri radius>0.
fn push_image(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
              uv0: [f32; 2], uv1: [f32; 2], radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    let mk = |px: f32, py: f32, u: f32, v: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color: [1.0, 1.0, 1.0, 1.0],  // alpha multiplier
            uv: [u, v],
            mode: 4.0,
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
            color2: [0.0; 4],
            blur: 0.0,
        }
    };
    let tl = mk(x,     y,     uv0[0], uv0[1]);
    let tr = mk(x + w, y,     uv1[0], uv0[1]);
    let bl = mk(x,     y + h, uv0[0], uv1[1]);
    let br = mk(x + w, y + h, uv1[0], uv1[1]);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Gradient quad: kazdy vertex ma uv.x = projekce na gradient axis (0..1).
fn push_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                 angle_deg: f32, c0: [f32; 4], c1: [f32; 4], radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    let rad = (angle_deg - 90.0).to_radians();
    let dir_x = rad.cos();
    let dir_y = rad.sin();
    let project = |px: f32, py: f32| -> f32 {
        let lx = (px - cx) / w + 0.5;
        let ly = (py - cy) / h + 0.5;
        let proj = (lx - 0.5) * dir_x + (ly - 0.5) * dir_y;
        (proj + 0.5).clamp(0.0, 1.0)
    };
    let mk = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color: c0,
            uv: [project(px, py), 0.0],
            mode: 2.0,
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
            color2: c1,
            blur: 0.0,
        }
    };
    let tl = mk(x,     y);
    let tr = mk(x + w, y);
    let bl = mk(x,     y + h);
    let br = mk(x + w, y + h);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Radial gradient quad. Mode 6.
/// V shaderu: t = distance(local, gradient_center) / gradient_radius.
/// gradient_center se predava jako half_size (reuse pole), gradient_radius jako blur.
fn push_radial_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                        gcx: f32, gcy: f32, grad_r: f32,
                        c0: [f32; 4], c1: [f32; 4], radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let box_cx = x + hw;
    let box_cy = y + hh;
    let mk = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color: c0,
            uv: [0.0, 0.0],
            mode: 6.0,
            local: [px - box_cx, py - box_cy],
            // half_size reuse: ulozim relativni gradient center (gcx-box_cx, gcy-box_cy)
            half_size: [gcx - box_cx, gcy - box_cy],
            radius,
            color2: c1,
            blur: grad_r,
        }
    };
    let tl = mk(x,     y);
    let tr = mk(x + w, y);
    let bl = mk(x,     y + h);
    let br = mk(x + w, y + h);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Conic gradient quad. Mode 7.
/// V shaderu: t = (atan2(local.y - gcy, local.x - gcx) - start) / 2pi.
fn push_conic_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                       gcx: f32, gcy: f32, start_deg: f32,
                       c0: [f32; 4], c1: [f32; 4], radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let box_cx = x + hw;
    let box_cy = y + hh;
    let start_rad = start_deg.to_radians();
    let mk = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color: c0,
            uv: [0.0, 0.0],
            mode: 7.0,
            local: [px - box_cx, py - box_cy],
            half_size: [gcx - box_cx, gcy - box_cy],
            radius,
            color2: c1,
            blur: start_rad,
        }
    };
    let tl = mk(x,     y);
    let tr = mk(x + w, y);
    let bl = mk(x,     y + h);
    let br = mk(x + w, y + h);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Multi-stop linear gradient pres CPU tesselaci.
/// Pro kazdy par stops[i], stops[i+1] orize jednotkovy ctverec [0,1]x[0,1] na region
/// kde axis-projekce je v [s_a, s_b], a vyplni ho 2-color gradientem c_a->c_b s uv.x lokalizovanou.
fn push_multi_stop_linear_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                                    angle_deg: f32, stops: &[(f32, [f32; 4])], radius: f32) {
    if stops.len() < 2 { return; }
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx_full = x + hw;
    let cy_full = y + hh;
    let rad = (angle_deg - 90.0).to_radians();
    let dx = rad.cos();
    let dy = rad.sin();
    // Projekce normalizovaneho bodu (nx, ny) v [0,1]^2 na osu - 0.5
    let proj_centered = |p: (f32, f32)| (p.0 - 0.5) * dx + (p.1 - 0.5) * dy;
    let project_norm = |p: (f32, f32)| proj_centered(p) + 0.5;
    let map_to_screen = |np: (f32, f32)| (x + np.0 * w, y + np.1 * h);

    for seg in 0..stops.len() - 1 {
        let s_a = stops[seg].0.clamp(0.0, 1.0);
        let s_b = stops[seg + 1].0.clamp(0.0, 1.0);
        if s_b <= s_a + 1e-6 { continue; }
        let c_a = stops[seg].1;
        let c_b = stops[seg + 1].1;
        let poly = clip_unit_square_to_axis_range(dx, dy, s_a, s_b);
        if poly.len() < 3 { continue; }
        // Triangulace fan z poly[0]
        for i in 1..poly.len() - 1 {
            let triplet = [poly[0], poly[i], poly[i + 1]];
            for &np in &triplet {
                let t_global = project_norm(np);
                let t_local = ((t_global - s_a) / (s_b - s_a)).clamp(0.0, 1.0);
                let (px, py) = map_to_screen(np);
                verts.push(Vertex {
                    pos: [px, py],
                    color: c_a,
                    uv: [t_local, 0.0],
                    mode: 2.0,
                    local: [px - cx_full, py - cy_full],
                    half_size: [hw, hh],
                    radius,
                    color2: c_b,
                    blur: 0.0,
                });
            }
        }
    }
}

/// Multi-stop radial gradient pres CPU tesselaci na soustredne mezikruzi.
/// Pro kazdy par stops[i], stops[i+1] generuje annulus z r_a*grad_r do r_b*grad_r.
/// K=48 segmentu kolem dokola. Mode 0 (solid s lokalni interpolaci) per-vertex.
fn push_multi_stop_radial_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                                    gcx: f32, gcy: f32, grad_r: f32,
                                    stops: &[(f32, [f32; 4])], radius: f32) {
    if stops.len() < 2 { return; }
    let hw = w * 0.5;
    let hh = h * 0.5;
    let box_cx = x + hw;
    let box_cy = y + hh;
    const K: usize = 48;
    // Mix per-vertex: kazdy vertex dostane svoji barvu uz vypoctenou (mode 0 = solid).
    // Box clip pres SDF radius v shaderu - misto toho clipneme na CPU pres axis-aligned bbox.
    // Ale annulus muze vyckat za box - to nevadi, framebuffer alpha-overdraw je OK pokud zustaneme
    // v ramci aktualniho clip rectu. Pouzijem mode 0 a vlozime barvu primo do vertex.color.
    let interp_color = |t: f32| -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        // Najdeme segment
        for i in 0..stops.len() - 1 {
            let a = stops[i].0;
            let b = stops[i + 1].0;
            if t >= a && t <= b + 1e-6 {
                let local = if b > a { (t - a) / (b - a) } else { 0.0 };
                let ca = stops[i].1;
                let cb = stops[i + 1].1;
                return [
                    ca[0] + (cb[0] - ca[0]) * local,
                    ca[1] + (cb[1] - ca[1]) * local,
                    ca[2] + (cb[2] - ca[2]) * local,
                    ca[3] + (cb[3] - ca[3]) * local,
                ];
            }
        }
        stops.last().unwrap().1
    };
    // Triangle fan z centra pro prvni stop
    let center_color = interp_color(0.0);
    let outer_color = interp_color(1.0);
    let _ = outer_color;
    // Stratujeme: mezi dvema sousednimi stop offsety vykreslime mezikruzi K segmentu
    // + vnitrek prvniho stop offsetu jako disk.
    let inner_r0 = stops[0].0.clamp(0.0, 1.0) * grad_r;
    if inner_r0 > 0.001 {
        // Disk od centra do inner_r0 - cely v c_a barve stops[0].
        for k in 0..K {
            let a0 = (k as f32) / (K as f32) * std::f32::consts::TAU;
            let a1 = ((k + 1) as f32) / (K as f32) * std::f32::consts::TAU;
            let p_center = (gcx, gcy);
            let p_a = (gcx + a0.cos() * inner_r0, gcy + a0.sin() * inner_r0);
            let p_b = (gcx + a1.cos() * inner_r0, gcy + a1.sin() * inner_r0);
            for &p in &[p_center, p_a, p_b] {
                verts.push(Vertex {
                    pos: [p.0, p.1],
                    color: center_color,
                    uv: [0.0, 0.0],
                    mode: 0.0,
                    local: [p.0 - box_cx, p.1 - box_cy],
                    half_size: [hw, hh],
                    radius,
                    color2: [0.0; 4],
                    blur: 0.0,
                });
            }
        }
    }
    // Annuli mezi stop pary
    for seg in 0..stops.len() - 1 {
        let t_a = stops[seg].0.clamp(0.0, 1.0);
        let t_b = stops[seg + 1].0.clamp(0.0, 1.0);
        if t_b <= t_a + 1e-6 { continue; }
        let r_a = t_a * grad_r;
        let r_b = t_b * grad_r;
        let c_a = stops[seg].1;
        let c_b = stops[seg + 1].1;
        for k in 0..K {
            let a0 = (k as f32) / (K as f32) * std::f32::consts::TAU;
            let a1 = ((k + 1) as f32) / (K as f32) * std::f32::consts::TAU;
            let p_inner_0 = (gcx + a0.cos() * r_a, gcy + a0.sin() * r_a);
            let p_inner_1 = (gcx + a1.cos() * r_a, gcy + a1.sin() * r_a);
            let p_outer_0 = (gcx + a0.cos() * r_b, gcy + a0.sin() * r_b);
            let p_outer_1 = (gcx + a1.cos() * r_b, gcy + a1.sin() * r_b);
            // 2 trojuhelniky: (inner_0, outer_0, outer_1) a (inner_0, outer_1, inner_1)
            let push_v = |verts: &mut Vec<Vertex>, p: (f32, f32), c: [f32; 4]| {
                verts.push(Vertex {
                    pos: [p.0, p.1],
                    color: c,
                    uv: [0.0, 0.0],
                    mode: 0.0,
                    local: [p.0 - box_cx, p.1 - box_cy],
                    half_size: [hw, hh],
                    radius,
                    color2: [0.0; 4],
                    blur: 0.0,
                });
            };
            push_v(verts, p_inner_0, c_a);
            push_v(verts, p_outer_0, c_b);
            push_v(verts, p_outer_1, c_b);
            push_v(verts, p_inner_0, c_a);
            push_v(verts, p_outer_1, c_b);
            push_v(verts, p_inner_1, c_a);
        }
    }
}

/// Multi-stop conic gradient: K=128 angularnich slicu, kazdy ma color z interp_color(angle/TAU).
fn push_multi_stop_conic_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                                   gcx: f32, gcy: f32, start_deg: f32,
                                   stops: &[(f32, [f32; 4])], radius: f32) {
    if stops.len() < 2 { return; }
    let hw = w * 0.5;
    let hh = h * 0.5;
    let box_cx = x + hw;
    let box_cy = y + hh;
    let start_rad = start_deg.to_radians();
    // Polomer dosahnout vsechny rohy boxu od (gcx, gcy)
    let r_max = {
        let dx_max = (gcx - x).abs().max((x + w - gcx).abs());
        let dy_max = (gcy - y).abs().max((y + h - gcy).abs());
        (dx_max * dx_max + dy_max * dy_max).sqrt() * 1.2
    };
    const K: usize = 128;
    let interp_color = |t: f32| -> [f32; 4] {
        let t = t.rem_euclid(1.0);
        for i in 0..stops.len() - 1 {
            let a = stops[i].0;
            let b = stops[i + 1].0;
            if t >= a && t <= b + 1e-6 {
                let local = if b > a { (t - a) / (b - a) } else { 0.0 };
                let ca = stops[i].1;
                let cb = stops[i + 1].1;
                return [
                    ca[0] + (cb[0] - ca[0]) * local,
                    ca[1] + (cb[1] - ca[1]) * local,
                    ca[2] + (cb[2] - ca[2]) * local,
                    ca[3] + (cb[3] - ca[3]) * local,
                ];
            }
        }
        stops.last().unwrap().1
    };
    for k in 0..K {
        let frac0 = (k as f32) / (K as f32);
        let frac1 = ((k + 1) as f32) / (K as f32);
        let a0 = start_rad + frac0 * std::f32::consts::TAU;
        let a1 = start_rad + frac1 * std::f32::consts::TAU;
        let c0 = interp_color(frac0);
        let c1 = interp_color(frac1);
        let p_center = (gcx, gcy);
        let p_a = (gcx + a0.cos() * r_max, gcy + a0.sin() * r_max);
        let p_b = (gcx + a1.cos() * r_max, gcy + a1.sin() * r_max);
        let push_v = |verts: &mut Vec<Vertex>, p: (f32, f32), c: [f32; 4]| {
            verts.push(Vertex {
                pos: [p.0, p.1],
                color: c,
                uv: [0.0, 0.0],
                mode: 0.0,
                local: [p.0 - box_cx, p.1 - box_cy],
                half_size: [hw, hh],
                radius,
                color2: [0.0; 4],
                blur: 0.0,
            });
        };
        push_v(verts, p_center, interp_color(0.0));
        push_v(verts, p_a, c0);
        push_v(verts, p_b, c1);
    }
}

/// Sutherland-Hodgman polygon clip + axis range clip helpers.
fn clip_unit_square_to_axis_range(dx: f32, dy: f32, t_min: f32, t_max: f32) -> Vec<(f32, f32)> {
    let mut poly = vec![(0.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let project = move |p: (f32, f32)| (p.0 - 0.5) * dx + (p.1 - 0.5) * dy;

    let thresh_min = t_min - 0.5;
    poly = clip_polygon(&poly, |p| project(p) >= thresh_min - 1e-6, |a, b| {
        let pa = project(a) - thresh_min;
        let pb = project(b) - thresh_min;
        let denom = pa - pb;
        let t = if denom.abs() < 1e-9 { 0.0 } else { pa / denom };
        (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
    });

    let thresh_max = t_max - 0.5;
    poly = clip_polygon(&poly, |p| project(p) <= thresh_max + 1e-6, |a, b| {
        let pa = thresh_max - project(a);
        let pb = thresh_max - project(b);
        let denom = pa - pb;
        let t = if denom.abs() < 1e-9 { 0.0 } else { pa / denom };
        (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
    });

    poly
}

fn clip_polygon<F, G>(poly: &[(f32, f32)], inside: F, intersect: G) -> Vec<(f32, f32)>
where
    F: Fn((f32, f32)) -> bool,
    G: Fn((f32, f32), (f32, f32)) -> (f32, f32),
{
    if poly.is_empty() { return vec![]; }
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(poly.len() + 2);
    let n = poly.len();
    for i in 0..n {
        let cur = poly[i];
        let prev = poly[(i + n - 1) % n];
        let cur_in = inside(cur);
        let prev_in = inside(prev);
        if cur_in {
            if !prev_in {
                out.push(intersect(prev, cur));
            }
            out.push(cur);
        } else if prev_in {
            out.push(intersect(prev, cur));
        }
    }
    out
}

fn push_shadow(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
               color: [f32; 4], blur: f32, radius: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    // Rozsirit quad o blur aby fade nepretekal
    let extra = blur + 4.0;
    let mk = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [0.0, 0.0],
            mode: 3.0,
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
            color2: [0.0; 4],
            blur,
        }
    };
    let tl = mk(x - extra,     y - extra);
    let tr = mk(x + w + extra, y - extra);
    let bl = mk(x - extra,     y + h + extra);
    let br = mk(x + w + extra, y + h + extra);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

/// Inset box-shadow: stin uvnitr boxu, fade smerem dovnitr od okraju + offset.
/// Quad presne na rozmer boxu (clip), color2.xy = (offset_x, offset_y).
fn push_inset_shadow(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                     color: [f32; 4], blur: f32, radius: f32,
                     offset_x: f32, offset_y: f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    let mk = |px: f32, py: f32| -> Vertex {
        Vertex {
            pos: [px, py],
            color,
            uv: [0.0, 0.0],
            mode: 5.0,  // mode 5 = inset shadow
            local: [px - cx, py - cy],
            half_size: [hw, hh],
            radius,
            // color2.xy = offset, .zw = padding
            color2: [offset_x, offset_y, 0.0, 0.0],
            blur,
        }
    };
    let tl = mk(x,     y);
    let tr = mk(x + w, y);
    let bl = mk(x,     y + h);
    let br = mk(x + w, y + h);
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
    /// Default font (fallback pri family lookup miss)
    font: fontdue::Font,
    /// @font-face loaded fonty: family name -> Font
    extra_fonts: std::collections::HashMap<String, fontdue::Font>,
    /// Atlas pixely (shedy: 0=transparent, 255=opaque)
    pixels: Vec<u8>,
    /// (family, char, font_size) -> glyph info. Family "" = default.
    cache: std::collections::HashMap<(String, char, u32), GlyphInfo>,
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
            extra_fonts: std::collections::HashMap::new(),
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            cache: std::collections::HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    /// Vrati referenci na font dle family. "" nebo neznamy -> default.
    fn font_for(&self, family: &str) -> &fontdue::Font {
        if family.is_empty() { return &self.font; }
        self.extra_fonts.get(family).unwrap_or(&self.font)
    }

    fn get(&self, family: &str, ch: char, size: u32) -> Option<&GlyphInfo> {
        self.cache.get(&(family.to_string(), ch, size))
    }

    /// Rasterize glyph and add to atlas.
    fn add(&mut self, family: &str, ch: char, size: u32) {
        let key = (family.to_string(), ch, size);
        if self.cache.contains_key(&key) { return; }
        let font = self.font_for(family);
        let (metrics, bitmap) = font.rasterize(ch, size as f32);
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
        self.cache.insert(key, info);
        self.cursor_x += w + 1;
        self.row_height = self.row_height.max(h);
    }
}

// ─── Image atlas (RGBA8 packed) ─────────────────────────────────────────

/// Velikost RGBA atlasu - 2048x2048 = 16 MB. Dost pro typickou stranku.
const IMAGE_ATLAS_SIZE: u32 = 2048;

/// Fetch text resource (HTML/CSS) z URL nebo FS path.
/// Pro http(s):// pres ureq, jinak std::fs::read_to_string.
/// Default User-Agent identifikuje engine.
pub fn fetch_text_url(url: &str) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        match ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 RustWebEngine/0.1 (custom layout + JS interpreter)")
            .set("Accept", "text/html,application/xhtml+xml,application/xml,text/css,*/*;q=0.8")
            .set("Accept-Language", "en-US,en;q=0.9,cs;q=0.8")
            .timeout(std::time::Duration::from_secs(15))
            .call()
        {
            Ok(resp) => resp.into_string().ok(),
            Err(e) => {
                eprintln!("[fetch] {url}: {e}");
                None
            }
        }
    } else if let Some(rest) = url.strip_prefix("file:///") {
        std::fs::read_to_string(rest.replace('/', std::path::MAIN_SEPARATOR_STR)).ok()
    } else {
        std::fs::read_to_string(url).ok()
    }
}

/// Resolve relative URL proti base URL. Vraci absolutni URL.
/// `base` muze byt http(s)://... nebo file:///....
pub fn resolve_url(base: &str, relative: &str) -> String {
    // Absolute URL/data uri - return as-is.
    if relative.starts_with("http://") || relative.starts_with("https://")
        || relative.starts_with("data:") || relative.starts_with("file:") {
        return relative.to_string();
    }
    // Protocol-relative: //example.com/path
    if let Some(stripped) = relative.strip_prefix("//") {
        let scheme = if base.starts_with("https://") { "https:" } else { "http:" };
        return format!("{scheme}//{stripped}");
    }
    // Find base scheme + host root.
    let (scheme_host, base_path) = if let Some(rest) = base.strip_prefix("https://") {
        let path_pos = rest.find('/').unwrap_or(rest.len());
        (format!("https://{}", &rest[..path_pos]), rest[path_pos..].to_string())
    } else if let Some(rest) = base.strip_prefix("http://") {
        let path_pos = rest.find('/').unwrap_or(rest.len());
        (format!("http://{}", &rest[..path_pos]), rest[path_pos..].to_string())
    } else if let Some(rest) = base.strip_prefix("file:///") {
        // file path - relative se resolvuje proti dir.
        let last_slash = rest.rfind('/').unwrap_or(0);
        let dir = &rest[..last_slash];
        if relative.starts_with('/') {
            return format!("file:///{relative}");
        }
        return format!("file:///{dir}/{relative}");
    } else {
        // Neznamy base - return relative as-is.
        return relative.to_string();
    };
    // Absolute path: /foo/bar
    if relative.starts_with('/') {
        return format!("{scheme_host}{relative}");
    }
    // Relative path: resolve proti directory v base_path.
    let last_slash = base_path.rfind('/').unwrap_or(0);
    let base_dir = &base_path[..=last_slash.min(base_path.len().saturating_sub(1))];
    let mut combined = format!("{scheme_host}{base_dir}{relative}");
    // Resolve .. and . segments.
    if combined.contains("/../") || combined.contains("/./") {
        let scheme_end = combined.find("://").map(|p| p + 3).unwrap_or(0);
        let (prefix, path_part) = combined.split_at(scheme_end + combined[scheme_end..].find('/').unwrap_or(0));
        let mut segs: Vec<&str> = Vec::new();
        for s in path_part.split('/') {
            if s == ".." { segs.pop(); }
            else if s == "." || s.is_empty() { /* skip empty */ }
            else { segs.push(s); }
        }
        combined = format!("{prefix}/{}", segs.join("/"));
    }
    combined
}

/// Fetch image bytes - podporuje http(s)://, data: URI, FS path.
/// Vrati None pri chybe (timeout, neplatny format, IO error).
pub fn fetch_image_bytes(src: &str) -> Option<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        // HTTP fetch pres ureq sync
        match ureq::get(src).timeout(std::time::Duration::from_secs(10)).call() {
            Ok(resp) => {
                let mut buf = Vec::new();
                if resp.into_reader().read_to_end(&mut buf).is_ok() {
                    return Some(buf);
                }
                None
            }
            Err(_) => None,
        }
    } else if let Some(rest) = src.strip_prefix("data:") {
        // data:[<mime>][;base64],<payload>
        let comma = rest.find(',')?;
        let header = &rest[..comma];
        let payload = &rest[comma+1..];
        if header.contains(";base64") {
            decode_base64(payload)
        } else {
            // URL-encoded text - vratit bytes (image neni typicky raw text)
            Some(payload.as_bytes().to_vec())
        }
    } else {
        // FS path
        let path = if src.starts_with('/') {
            src.to_string()
        } else {
            format!("static/{src}")
        };
        std::fs::read(&path).ok()
    }
}

/// Decode base64 string -> bytes. Self-contained, bez external crate.
fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let chars: Vec<char> = s.chars().collect();
    let val = |c: char| -> Option<u8> {
        match c {
            'A'..='Z' => Some(c as u8 - b'A'),
            'a'..='z' => Some(c as u8 - b'a' + 26),
            '0'..='9' => Some(c as u8 - b'0' + 52),
            '+' | '-' => Some(62),
            '/' | '_' => Some(63),
            '=' => Some(0),
            _ => None,
        }
    };
    let mut i = 0;
    while i + 3 < chars.len() {
        let a = val(chars[i])?;
        let b = val(chars[i+1])?;
        let c = val(chars[i+2])?;
        let d = val(chars[i+3])?;
        out.push((a << 2) | (b >> 4));
        if chars[i+2] != '=' { out.push(((b & 0xF) << 4) | (c >> 2)); }
        if chars[i+3] != '=' { out.push(((c & 0x3) << 6) | d); }
        i += 4;
    }
    Some(out)
}

#[derive(Clone, Copy)]
pub struct ImageInfo {
    /// UV coords v atlasu (0..1)
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub width: f32,
    pub height: f32,
}

pub struct ImageAtlas {
    /// RGBA pixely (4 byte per pixel)
    pub pixels: Vec<u8>,
    /// src URL/path -> ImageInfo
    cache: std::collections::HashMap<String, ImageInfo>,
    /// Shelf packing kurzor
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    /// Dirty flag - byly pridany nove obrazky -> potreba upload
    pub dirty: bool,
}

impl ImageAtlas {
    pub fn new() -> Self {
        ImageAtlas {
            pixels: vec![0u8; (IMAGE_ATLAS_SIZE * IMAGE_ATLAS_SIZE * 4) as usize],
            cache: std::collections::HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    pub fn get(&self, src: &str) -> Option<&ImageInfo> {
        self.cache.get(src)
    }

    /// Test helper: count cached images.
    pub fn cache_size(&self) -> usize { self.cache.len() }

    /// Test helper: get UV bounds for src - (uv0, uv1) v 0..1 atlas range.
    pub fn uv_bounds(&self, src: &str) -> Option<([f32; 2], [f32; 2])> {
        self.cache.get(src).map(|i| (i.uv0, i.uv1))
    }

    /// Test helper: check if src is in cache.
    pub fn contains(&self, src: &str) -> bool { self.cache.contains_key(src) }

    /// Vlozi RGBA bitmap do atlasu. Pri overflow vrati false.
    pub fn add(&mut self, src: &str, w: u32, h: u32, rgba: &[u8]) -> bool {
        if self.cache.contains_key(src) { return true; }
        if w == 0 || h == 0 { return false; }
        // Obrazek vetsi nez cely atlas - nelze
        if w > IMAGE_ATLAS_SIZE || h > IMAGE_ATLAS_SIZE { return false; }

        // Shelf packing: novy radek pri preteceni X
        if self.cursor_x + w > IMAGE_ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h > IMAGE_ATLAS_SIZE {
            return false; // atlas full
        }

        // Copy RGBA bytes do atlasu
        for row in 0..h {
            let src_off = (row * w * 4) as usize;
            let dst_off = (((self.cursor_y + row) * IMAGE_ATLAS_SIZE + self.cursor_x) * 4) as usize;
            let len = (w * 4) as usize;
            if src_off + len <= rgba.len() && dst_off + len <= self.pixels.len() {
                self.pixels[dst_off..dst_off + len].copy_from_slice(&rgba[src_off..src_off + len]);
            }
        }

        let info = ImageInfo {
            uv0: [self.cursor_x as f32 / IMAGE_ATLAS_SIZE as f32,
                  self.cursor_y as f32 / IMAGE_ATLAS_SIZE as f32],
            uv1: [(self.cursor_x + w) as f32 / IMAGE_ATLAS_SIZE as f32,
                  (self.cursor_y + h) as f32 / IMAGE_ATLAS_SIZE as f32],
            width: w as f32,
            height: h as f32,
        };
        self.cache.insert(src.to_string(), info);
        self.cursor_x += w + 1;
        self.row_height = self.row_height.max(h);
        self.dirty = true;
        true
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
    run_window_with_options(html, css, None, false, None)
}

/// Spusti okno s dodatecnymi options.
/// - `current_html_path`: pri Some umozni reload pres drag-drop (relativni paths v HTML)
/// - `auto_devtools`: pri true vygeneruje devtools.html a otevre v OS default browser
/// - `base_url`: page URL pro relative resolution (http(s)://... nebo file:///...).
///   Pri None se odvodi z `current_html_path`.
pub fn run_window_with_options(html: String, css: String, current_html_path: Option<std::path::PathBuf>, auto_devtools: bool, base_url: Option<String>) -> Result<(), String> {
    use winit::application::ApplicationHandler;
    use winit::event::{WindowEvent, MouseButton, ElementState};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
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
        /// Cache cascade output (DOM root ptr hash -> StyleMap).
        cached_cascade_hash: u64,
        cached_style_map: Option<super::cascade::StyleMap>,
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
        start_time: std::time::Instant,
        /// Predchozi cascaded styles - pro detekci transitions.
        prev_style_map: Option<super::cascade::StyleMap>,
        /// Track running animations per (node_id, anim_name) - pro dispatch animationstart/end
        active_animations: std::collections::HashSet<(usize, String)>,
        /// Iteration counter per animation pro animationiteration event.
        animation_iterations: std::collections::HashMap<(usize, String), i32>,
        /// Aktivni CSS transitions.
        active_transitions: Vec<super::cascade::ActiveTransition>,
        /// In-browser DevTools panel state.
        devtools_open: bool,
        /// Aktualne vybrany element v devtools tree (raw ptr DOM node).
        devtools_selected: Option<usize>,
        /// Tab v devtools panelu: 0=Elements, 1=Console, 2=Network.
        devtools_tab: u8,
        /// Vyska devtools panelu v px (resize-able).
        devtools_height: f32,
        /// Scroll v Elements tree.
        devtools_tree_scroll: f32,
        /// Inspect mode: pri hover na main viewport zvyrazni element + click vybira v tree.
        devtools_inspect_mode: bool,
        /// Console input buffer (typed JS).
        devtools_console_input: String,
        /// True kdyz user drze LMB na resize grip a tahne.
        devtools_resizing: bool,
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
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        r.resize(size.width.max(1), size.height.max(1));
                    }
                    self.render();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let new_x = position.x as f32;
                    let new_y = position.y as f32 + self.scroll_y;
                    // Skip update kdyz se pozice nezmenila (deduplicate winit spam).
                    if (new_x - self.mouse_x).abs() < 0.5 && (new_y - self.mouse_y).abs() < 0.5 {
                        return;
                    }
                    self.mouse_x = new_x;
                    self.mouse_y = new_y;
                    // Resize drag: aktualizuj devtools_height na zaklade pozice.
                    if self.devtools_resizing {
                        let win_h = self.renderer.as_ref().map(|r| r.config.height as f32).unwrap_or(0.0);
                        let raw_y = new_y - self.scroll_y;
                        let new_height = (win_h - raw_y).max(60.0).min(win_h * 0.9);
                        self.devtools_height = new_height;
                        self.render();
                        return;
                    }
                    self.update_hover();
                    if self.open_select.is_some() {
                        self.render();
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                    if self.devtools_resizing {
                        self.devtools_resizing = false;
                        self.render();
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                    // Devtools panel hit-test: pri kliku v panelu vyhodnocujeme nejdriv tam.
                    let raw_y = self.mouse_y - self.scroll_y;
                    let win_h = self.renderer.as_ref().map(|r| r.config.height as f32).unwrap_or(0.0);
                    let win_w = self.renderer.as_ref().map(|r| r.config.width as f32).unwrap_or(0.0);
                    let panel_h = if self.devtools_open { self.devtools_height.min(win_h * 0.7) } else { 0.0 };
                    if self.devtools_open && raw_y >= win_h - panel_h {
                        if let Some(layout) = &self.layout_root {
                            match devtools_hit_test(layout, self.devtools_tab, self.devtools_tree_scroll,
                                                    win_w, win_h, panel_h, self.mouse_x, raw_y) {
                                DevtoolsHit::TabClick(t) => { self.devtools_tab = t; }
                                DevtoolsHit::TreeRow(node_id) => {
                                    self.devtools_selected = Some(node_id);
                                }
                                DevtoolsHit::InspectToggle => {
                                    self.devtools_inspect_mode = !self.devtools_inspect_mode;
                                }
                                DevtoolsHit::ResizeGrip => {
                                    self.devtools_resizing = true;
                                }
                                DevtoolsHit::ClearConsole => {
                                    if let Some(interp) = &mut self.interpreter {
                                        interp.console_log.borrow_mut().clear();
                                    }
                                }
                                DevtoolsHit::None => {}
                            }
                        }
                        self.render();
                        return;
                    }
                    // Inspect mode: kliknuti na main viewport vybira node v tree.
                    if self.devtools_inspect_mode {
                        if let Some(layout) = &self.layout_root {
                            if let Some(node_id) = pick_node_at_screen_pos(layout, self.mouse_x, raw_y, self.scroll_y) {
                                self.devtools_selected = Some(node_id);
                                println!("[inspect] selected node id=0x{:x}", node_id);
                            }
                        }
                        // V inspect modu nepropaguj click do stranky (jen vybira).
                        self.devtools_inspect_mode = false;
                        self.render();
                        return;
                    }
                    self.handle_click(self.mouse_x, self.mouse_y);
                    self.render();
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    // Pri kurzoru nad devtools panelem - scrolluj tree, ne stranku.
                    let raw_y = self.mouse_y - self.scroll_y;
                    let win_h = self.renderer.as_ref().map(|r| r.config.height as f32).unwrap_or(0.0);
                    let panel_h = if self.devtools_open { self.devtools_height.min(win_h * 0.7) } else { 0.0 };
                    if self.devtools_open && raw_y >= win_h - panel_h {
                        self.devtools_tree_scroll -= scroll_amount;
                        if self.devtools_tree_scroll < 0.0 { self.devtools_tree_scroll = 0.0; }
                        self.render();
                        return;
                    }
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
                    // Continual redraw JEN pri aktivnich animacich/transition (jinak idle).
                    let has_anim = !self.active_animations.is_empty()
                        || !self.active_transitions.is_empty();
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
                // F12 = regenerace devtools.html + open v default browseru.
                // F5 / Ctrl+R = reload current file.
                // Alt+Left = back, Alt+Right = forward (browser history).
                WindowEvent::KeyboardInput { event: key_event, .. } => {
                    if key_event.state != ElementState::Pressed { return; }
                    // Pri otevrenem devtools + Console tab + zadanym text -> append do console_input.
                    if self.devtools_open && self.devtools_tab == 1 {
                        match &key_event.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                self.devtools_console_input.pop();
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                let cmd = std::mem::take(&mut self.devtools_console_input);
                                if !cmd.is_empty() {
                                    println!("[devtools console] eval: {}", cmd);
                                    if let Some(interp) = &mut self.interpreter {
                                        interp.console_log.borrow_mut().push(("info".to_string(), format!("> {}", cmd)));
                                        // Real eval pres bytecode VM (rychlejsi nez tree-walker pro
                                        // jednoduche vyrazy + dovoluje rychly pristup k vsem opcodes).
                                        let result = console_eval_via_vm(&cmd, interp);
                                        match result {
                                            Ok(v) => {
                                                interp.console_log.borrow_mut().push(("info".to_string(), v.to_string()));
                                            }
                                            Err(e) => {
                                                interp.console_log.borrow_mut().push(("error".to_string(), e));
                                            }
                                        }
                                    }
                                }
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                            Key::Character(s) => {
                                self.devtools_console_input.push_str(s);
                                if let Some(w) = &self.window { w.request_redraw(); }
                                return;
                            }
                            _ => {}
                        }
                    }
                    match key_event.logical_key {
                        Key::Named(NamedKey::F12) => {
                            // Toggle in-window devtools panel.
                            self.devtools_open = !self.devtools_open;
                            println!("[F12] devtools panel = {}", if self.devtools_open { "ON" } else { "OFF" });
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }
                        Key::Named(NamedKey::F11) => {
                            // F11 = old behavior = open static devtools.html
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
                        _ => {}
                    }
                }
                _ => {}
            }
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
            // Hledej co-located CSS pres .css extenzion.
            let css_path = path.with_extension("css");
            let css = std::fs::read_to_string(&css_path).unwrap_or_default();
            self.html = html;
            self.css = css;
            self.current_path = Some(path.to_path_buf());
            self.scroll_y = 0.0;
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
            // Update window title.
            if let Some(w) = &self.window {
                w.set_title(&format!("Rust Web Engine - {}", path.display()));
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
                    // Set focus na klik (HTML interactive elements: input/textarea/select/button/a).
                    let tag = node.tag_name();
                    let is_focusable = matches!(tag.as_deref(),
                        Some("input") | Some("textarea") | Some("select") | Some("button") | Some("a")
                    ) || node.attr("tabindex").is_some();
                    if is_focusable {
                        super::cascade::set_focused_node(Some(std::rc::Rc::as_ptr(node) as usize));
                    } else {
                        super::cascade::set_focused_node(None);
                    }
                    // Form submit: <button type=submit> / <input type=submit> klik nebo
                    // <a href> klik -> navigate.
                    let is_submit_button = matches!(tag.as_deref(), Some("button") | Some("input"))
                        && node.attr("type").as_deref().map(|t| t.eq_ignore_ascii_case("submit")).unwrap_or(matches!(tag.as_deref(), Some("button")));
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
                                        let mut interp = crate::interpreter::Interpreter::new();
                                        let doc = super::html_parser::parse_html(&self.html, &url);
                                        interp.set_document(doc);
                                        self.run_inline_scripts(&mut interp);
                                        self.interpreter = Some(interp);
                                        if let Some(w) = &self.window { w.set_title(&format!("Rust Web Engine - {url}")); }
                                        self.render();
                                    }
                                } else {
                                    self.navigate_url(&url);
                                }
                                return;
                            }
                        }
                    }
                    // <select> click: toggle open dropdown.
                    if tag.as_deref() == Some("select") {
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
                    // <a href="..."> click -> navigate.
                    if tag.as_deref() == Some("a") {
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

        /// Navigate s history push (smaze forward history pri navigaci).
        fn navigate_url(&mut self, url: &str) {
            // Truncate forward history.
            self.history.truncate(self.history_idx + 1);
            self.history.push(url.to_string());
            self.history_idx = self.history.len() - 1;
            self.navigate_url_no_history(url);
        }

        /// Navigate bez modifikace history (back/forward use this).
        fn navigate_url_no_history(&mut self, url: &str) {
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
                if let Some(w) = &self.window {
                    w.set_title(&format!("Rust Web Engine - {url}"));
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
        }

        fn render(&mut self) {
            use super::{css_parser, cascade, layout, paint};
            let frame_start = std::time::Instant::now();
            let r = match &mut self.renderer { Some(r) => r, None => return };

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
            if self.cached_stylesheets.is_none() || self.cached_stylesheets_hash != css_hash {
                let parsed = vec![css_parser::parse_stylesheet(&self.css)];
                for sheet in &parsed {
                    r.load_font_faces(&sheet.font_faces, self.base_url.as_deref());
                }
                // Detect if any keyframes animate layout-affecting properties.
                // Layout-affecting: width/height/padding/margin/border-width/border-radius
                // /font-size/line-height/gap/flex-*/grid-*/top/left/right/bottom/position/display.
                let layout_props = ["width", "height", "padding", "margin", "border", "font-size",
                                    "line-height", "gap", "flex", "grid", "top", "left", "right",
                                    "bottom", "position", "display", "min-width", "max-width",
                                    "min-height", "max-height"];
                self.animations_affect_layout = parsed.iter().any(|sheet| {
                    sheet.keyframes.iter().any(|kf| {
                        kf.frames.iter().any(|(_, decls)| {
                            decls.iter().any(|d| {
                                layout_props.iter().any(|p| d.property.starts_with(p))
                            })
                        })
                    })
                });
                self.cached_stylesheets = Some(parsed);
                self.cached_stylesheets_hash = css_hash;
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
                h.finish()
            };
            if self.cached_style_map.is_none() || self.cached_cascade_hash != cascade_hash {
                self.cached_style_map = Some(cascade::cascade(&document_root, stylesheets));
                self.cached_pseudo_map = Some(cascade::cascade_pseudo(&document_root, stylesheets));
                self.cached_cascade_hash = cascade_hash;
            }
            let mut style_map = self.cached_style_map.as_ref().unwrap().clone();
            let pseudo_map = self.cached_pseudo_map.as_ref().cloned().unwrap_or_default();

            let elapsed = self.start_time.elapsed().as_secs_f32();

            // Drainuj WebSocket events kazdy frame (dispatch onopen/onmessage/onerror/onclose).
            if let Some(interp) = &mut self.interpreter {
                let _ = interp.drain_websockets();
            }

            // CSS Transitions: detekuj zmeny vs prev_style_map a vyrob aktivni transitions.
            let mut ended_transitions: Vec<(usize, String)> = Vec::new();
            if let Some(prev) = &self.prev_style_map {
                let active_before = std::mem::take(&mut self.active_transitions);
                let prev_keys: std::collections::HashSet<(usize, String)> = active_before.iter()
                    .map(|t| (t.node_id, t.property.clone())).collect();
                self.active_transitions = cascade::detect_transitions(prev, &style_map, active_before, elapsed);
                let now_keys: std::collections::HashSet<(usize, String)> = self.active_transitions.iter()
                    .map(|t| (t.node_id, t.property.clone())).collect();
                for k in prev_keys.difference(&now_keys) {
                    ended_transitions.push(k.clone());
                }
            }
            // Aplikuj transitions jen kdyz nejake aktivni (skip cely walk pri prazdnem).
            if !self.active_transitions.is_empty() {
                cascade::apply_transitions(&mut style_map, &self.active_transitions, elapsed);
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
                let _animating = cascade::apply_animations(&mut style_map, stylesheets, elapsed);
                let max_scroll = (style_map.len() as f32).max(1.0);
                let scroll_progress = if max_scroll > 1.0 { self.scroll_y / max_scroll.max(1.0) } else { 0.0 };
                let _ = cascade::apply_scroll_animations(&mut style_map, stylesheets, scroll_progress);
            }

            // Detect animation start/end + iteration events.
            // Skip cely walk pri zadnych keyframes - test na to ze stranka vubec nema animations.
            let mut current_anims: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
            let mut iter_events: Vec<(usize, String, i32)> = Vec::new();
            if has_keyframes {
                for (node_id, styles) in &style_map {
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

            // Uloz current style_map pro dalsi frame (transition diff source)
            self.prev_style_map = Some(style_map.clone());

            let viewport_w = r.config.width as f32;
            let viewport_h = r.config.height as f32;
            // Layout cache: rebuild jen kdyz CSS/DOM/viewport zmenil nebo
            // animations modifikuji layout-relevant props (width/height/margin/...).
            let layout_cache_valid = self.cached_layout_root.is_some()
                && !self.animations_affect_layout
                && self.cached_layout_root.as_ref().map(|l| {
                    (l.rect.width - viewport_w).abs() < 0.5 && (l.rect.height - viewport_h).abs() < 0.5
                }).unwrap_or(false);
            let mut layout_root = if layout_cache_valid {
                self.cached_layout_root.as_ref().unwrap().clone()
            } else {
                let lr = layout::layout_tree_with_pseudo(&document_root, &style_map, &pseudo_map, viewport_w, viewport_h);
                self.cached_layout_root = Some(lr.clone());
                lr
            };
            // Post-pass: aplikuj animation values na cached layout boxes
            // (transforms, opacity, colors, filter - paint-only props ktere se
            // za zivota cache mohou menit kazdy frame).
            if layout_cache_valid {
                apply_paint_animations(&mut layout_root, &style_map);
            }
            // Apply position: sticky pri current scroll
            layout::apply_sticky(&mut layout_root, self.scroll_y);
            // Viewport culling: vyrad off-screen elementy z paint walku
            // (test stranka 7000 px, viewport 900 px = 8x mensi paint cost).
            // Reuse buffer pres frames - alloc-free.
            let mut display_list = std::mem::take(&mut self.display_list_buffer);
            paint::build_display_list_culled_into(&layout_root, self.scroll_y, viewport_h, &mut display_list);

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

            // In-window DevTools panel - emit pred scrollbar a po main viewport content.
            // Devtools panel zabira spodni cast okna (devtools_height px).
            // Hlavni viewport_h je redukovany.
            let win_h = r.config.height as f32;
            let panel_h = if self.devtools_open { self.devtools_height.min(win_h * 0.7) } else { 0.0 };
            if self.devtools_open {
                paint_devtools_panel(
                    &mut display_list,
                    &layout_root,
                    self.devtools_selected,
                    self.devtools_tab,
                    self.devtools_tree_scroll,
                    self.devtools_inspect_mode,
                    &self.devtools_console_input,
                    self.interpreter.as_ref(),
                    r.config.width as f32,
                    win_h,
                    panel_h,
                    self.mouse_x, self.mouse_y,
                );
            }
            // Highlight rect pro vybrany element v DevTools.
            if let Some(sel_id) = self.devtools_selected {
                if let Some(rect) = find_box_rect_by_id(&layout_root, sel_id, self.scroll_y) {
                    // Polopruhledne modre highlight + border.
                    display_list.push(DisplayCommand::Rect {
                        x: rect.0, y: rect.1, w: rect.2, h: rect.3,
                        color: [100, 150, 255, 70], radius: 0.0,
                    });
                    display_list.push(DisplayCommand::Border {
                        x: rect.0, y: rect.1, w: rect.2, h: rect.3,
                        width: 2.0, color: [50, 100, 220, 255],
                    });
                }
            }

            // Scrollbar rendering: pri page content overflow Y emituj track + thumb.
            let viewport_w = r.config.width as f32;
            let viewport_h = (r.config.height as f32) - panel_h;
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

            // Pre-rasterize vsechny glyfy do atlasu + nacti images.
            // Pri COLR color font: rasterize char jako RGBA + put do image_atlas
            // pres synthetic key "__colr:{family}:{ch}:{size}". Render path detekuje.
            for cmd in &display_list {
                match cmd {
                    DisplayCommand::Text { content, font_size, font_family, color, .. } => {
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
                                r.atlas.add(font_family, ch, *font_size as u32);
                            }
                        }
                    }
                    DisplayCommand::Image { src, .. } => {
                        // Resolve relative URL proti base_url (pri http(s) nebo file:// page).
                        let resolved = match &self.base_url {
                            Some(base) => resolve_url(base, src),
                            None => src.clone(),
                        };
                        r.load_image_as(src, &resolved);
                    }
                    _ => {}
                }
            }
            r.upload_atlas();
            r.upload_image_atlas();

            // Pri WebGL canvas s pending queue, vyuzij webgl-aware draw flow.
            let webgl_states_opt = self.interpreter.as_ref().map(|i| i.webgl_states.clone());
            if let Some(states_rc) = &webgl_states_opt {
                let states = states_rc.borrow();
                r.draw_full_frame(&display_list, &layout_root, Some(&*states));
            } else {
                r.draw_segments(&display_list);
            }

            // Ulozim layout pro hit test + vrat display_list buffer pro priste.
            self.layout_root = Some(layout_root);
            self.display_list_buffer = display_list;
            let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
            if frame_ms > 50.0 {
                eprintln!("[slow frame] {:.1} ms", frame_ms);
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
    let initial_url = base_url.clone();
    let mut app = App {
        html, css,
        cached_stylesheets_hash: 0,
        cached_stylesheets: None,
        cached_cascade_hash: 0,
        cached_style_map: None,
        cached_pseudo_map: None,
        display_list_buffer: Vec::with_capacity(2048),
        cached_layout_root: None,
        animations_affect_layout: false,
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
        start_time: std::time::Instant::now(),
        prev_style_map: None,
        active_transitions: Vec::new(),
        active_animations: std::collections::HashSet::new(),
        animation_iterations: std::collections::HashMap::new(),
        devtools_open: false,
        devtools_selected: None,
        devtools_tab: 0,
        devtools_height: 320.0,
        devtools_tree_scroll: 0.0,
        devtools_inspect_mode: false,
        devtools_console_input: String::new(),
        devtools_resizing: false,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    Ok(())
}

/// Najde nejblizsi <form> ancestor.
fn find_ancestor_form(node: &std::rc::Rc<crate::browser::dom::Node>) -> Option<std::rc::Rc<crate::browser::dom::Node>> {
    let mut current = Some(std::rc::Rc::clone(node));
    while let Some(n) = current {
        if n.tag_name().as_deref() == Some("form") { return Some(n); }
        current = n.parent.borrow().upgrade();
    }
    None
}

/// Vrati (resolved_url, method, querystring_or_body).
/// Pri GET: body=None, params kombinovany do URL ?k=v&k=v.
/// Pri POST: vrati url cely + body separately. Caller posli pres ureq POST.
fn build_form_request(form: &std::rc::Rc<crate::browser::dom::Node>, base_url: Option<&str>) -> Option<(String, String, Option<String>)> {
    let action = form.attr("action").unwrap_or_default();
    let method = form.attr("method").unwrap_or_default().to_lowercase();
    let method = if method.is_empty() { "get".to_string() } else { method };
    let action_resolved = if action.is_empty() {
        base_url.unwrap_or("").to_string()
    } else if let Some(b) = base_url {
        resolve_url(b, &action)
    } else { action };
    let mut params: Vec<(String, String)> = Vec::new();
    fn collect(node: &std::rc::Rc<crate::browser::dom::Node>, out: &mut Vec<(String, String)>) {
        if matches!(node.kind, crate::browser::dom::NodeKind::Element(_)) {
            if let Some(tag) = node.tag_name() {
                if matches!(tag.as_str(), "input" | "select" | "textarea") {
                    let name = node.attr("name").unwrap_or_default();
                    if !name.is_empty() {
                        let val = match tag.as_str() {
                            "input" => {
                                let t = node.attr("type").unwrap_or_default().to_lowercase();
                                if t == "checkbox" || t == "radio" {
                                    if node.attr("checked").is_some() {
                                        node.attr("value").unwrap_or_else(|| "on".to_string())
                                    } else { return; }
                                } else if t == "submit" || t == "button" || t == "reset" || t == "image" {
                                    return; // skip submit-type inputs from data
                                } else {
                                    node.attr("value").unwrap_or_default()
                                }
                            }
                            "select" => {
                                let mut selected: Option<String> = None;
                                let mut first: Option<String> = None;
                                for ch in node.children.borrow().iter() {
                                    if ch.tag_name().as_deref() == Some("option") {
                                        let v = ch.attr("value").unwrap_or_else(|| ch.text_content().trim().to_string());
                                        if first.is_none() { first = Some(v.clone()); }
                                        if ch.attr("selected").is_some() { selected = Some(v); break; }
                                    }
                                }
                                selected.or(first).unwrap_or_default()
                            }
                            "textarea" => node.text_content(),
                            _ => String::new(),
                        };
                        out.push((name, val));
                    }
                }
            }
        }
        for ch in node.children.borrow().iter() {
            collect(ch, out);
        }
    }
    collect(form, &mut params);
    let qs: Vec<String> = params.into_iter().map(|(k, v)|
        format!("{}={}", url_encode(&k), url_encode(&v))).collect();
    let body = qs.join("&");
    if method == "post" {
        Some((action_resolved, method, Some(body)))
    } else {
        let separator = if action_resolved.contains('?') { "&" } else { "?" };
        let url = if body.is_empty() { action_resolved }
                  else { format!("{action_resolved}{separator}{body}") };
        Some((url, method, None))
    }
}

/// Backward-compat helper - jen GET URL.
fn build_form_get_url(form: &std::rc::Rc<crate::browser::dom::Node>, base_url: Option<&str>) -> Option<String> {
    let (url, method, body) = build_form_request(form, base_url)?;
    if method == "get" && body.is_none() { Some(url) } else {
        // POST je return URL bez body - caller potrebuje volat POST flow.
        // Caller by mel pouzit build_form_request misto teto fce kdyz chce body.
        Some(url)
    }
}

/// POST request s url-encoded form body. Vrati response HTML.
fn post_form(url: &str, body: &str) -> Option<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        eprintln!("[form POST] non-http URL: {url}");
        return None;
    }
    match ureq::post(url)
        .set("User-Agent", "Mozilla/5.0 RustWebEngine/0.1")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .timeout(std::time::Duration::from_secs(15))
        .send_string(body)
    {
        Ok(resp) => resp.into_string().ok(),
        Err(e) => { eprintln!("[form POST] {url}: {e}"); None }
    }
}

#[allow(dead_code)]
fn old_build_form_get_url(form: &std::rc::Rc<crate::browser::dom::Node>, base_url: Option<&str>) -> Option<String> {
    let action = form.attr("action").unwrap_or_default();
    let method = form.attr("method").unwrap_or_default().to_lowercase();
    if method == "post" {
        eprintln!("[form] POST submit zatim neimplementovano - skip");
        return None;
    }
    // Resolve action proti base.
    let action_resolved = if action.is_empty() {
        // Default = current page URL.
        base_url.unwrap_or("").to_string()
    } else if let Some(b) = base_url {
        resolve_url(b, &action)
    } else {
        action
    };
    // Collect form fields (input/select/textarea) descendants.
    let mut params: Vec<(String, String)> = Vec::new();
    fn collect(node: &std::rc::Rc<crate::browser::dom::Node>, out: &mut Vec<(String, String)>) {
        if matches!(node.kind, crate::browser::dom::NodeKind::Element(_)) {
            if let Some(tag) = node.tag_name() {
                let is_input = matches!(tag.as_str(), "input" | "select" | "textarea");
                if is_input {
                    let name = node.attr("name").unwrap_or_default();
                    if !name.is_empty() {
                        let val = match tag.as_str() {
                            "input" => {
                                let t = node.attr("type").unwrap_or_default().to_lowercase();
                                if t == "checkbox" || t == "radio" {
                                    if node.attr("checked").is_some() {
                                        node.attr("value").unwrap_or_else(|| "on".to_string())
                                    } else {
                                        return;
                                    }
                                } else {
                                    node.attr("value").unwrap_or_default()
                                }
                            }
                            "select" => {
                                // Najdi selected option value.
                                let mut selected: Option<String> = None;
                                let mut first: Option<String> = None;
                                for ch in node.children.borrow().iter() {
                                    if ch.tag_name().as_deref() == Some("option") {
                                        let v = ch.attr("value").unwrap_or_else(|| ch.text_content().trim().to_string());
                                        if first.is_none() { first = Some(v.clone()); }
                                        if ch.attr("selected").is_some() { selected = Some(v); break; }
                                    }
                                }
                                selected.or(first).unwrap_or_default()
                            }
                            "textarea" => node.text_content().trim().to_string(),
                            _ => String::new(),
                        };
                        out.push((name, val));
                    }
                }
            }
        }
        for ch in node.children.borrow().iter() {
            collect(ch, out);
        }
    }
    collect(form, &mut params);
    // URL encode params + concat.
    let qs: Vec<String> = params.into_iter().map(|(k, v)| {
        format!("{}={}", url_encode(&k), url_encode(&v))
    }).collect();
    let qs_joined = qs.join("&");
    let separator = if action_resolved.contains('?') { "&" } else { "?" };
    if qs_joined.is_empty() {
        Some(action_resolved)
    } else {
        Some(format!("{action_resolved}{separator}{qs_joined}"))
    }
}

/// Minimal URL encoder - escape non-ASCII + reserved chars.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
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

// ─── Dirty rect tracking ────────────────────────────────────────────────

/// Sleduje obdelnikovou oblast ktera potrebuje prekresleni.
/// `None` = vse ciste (zadna zmena). `Some([x,y,w,h])` = dirty oblast.
/// Slucovani: unionem s novou dirty oblast.
#[derive(Debug, Clone, Default)]
pub struct DirtyRegion {
    pub rect: Option<[f32; 4]>,
}

impl DirtyRegion {
    pub fn new() -> Self { DirtyRegion { rect: None } }

    /// Oznaci oblast jako dirty. Slucuje s existujici dirty oblasti (union).
    pub fn mark(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rect = Some(match self.rect {
            None => [x, y, w, h],
            Some([ox, oy, ow, oh]) => {
                let nx = ox.min(x);
                let ny = oy.min(y);
                let nw = (ox + ow).max(x + w) - nx;
                let nh = (oy + oh).max(y + h) - ny;
                [nx, ny, nw, nh]
            }
        });
    }

    /// Vymaze dirty stav. Vraci oblast ktera byla dirty (pro render).
    pub fn take(&mut self) -> Option<[f32; 4]> {
        self.rect.take()
    }

    pub fn is_dirty(&self) -> bool { self.rect.is_some() }

    /// Nastavi dirty na cele viewport.
    pub fn mark_all(&mut self, w: f32, h: f32) {
        self.mark(0.0, 0.0, w, h);
    }
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
    /// Image RGBA atlas + GPU texture
    image_atlas: ImageAtlas,
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
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
            None,
        )).expect("device");
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
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
        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pl"),
            bind_group_layouts: &[&blur_bind_group_layout],
            push_constant_ranges: &[],
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader, entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[], // fullscreen triangle, no vertex buffer
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader, entry_point: "fs_main",
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
            multiview: None,
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
        let compose_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compose_pl"),
            bind_group_layouts: &[&compose_bind_group_layout],
            push_constant_ranges: &[],
        });
        let compose_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compose_pipeline"),
            layout: Some(&compose_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &compose_shader, entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &compose_shader, entry_point: "fs_main",
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
        let transform_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("transform_pl"),
            bind_group_layouts: &[&transform_bind_group_layout],
            push_constant_ranges: &[],
        });
        let transform_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("transform_pipeline"),
            layout: Some(&transform_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &transform_shader, entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &transform_shader, entry_point: "fs_main",
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

        Renderer {
            surface, device, queue, config, pipeline, uniform_buf,
            atlas_tex, atlas_view, atlas_smp, bind_group_layout, bind_group, atlas,
            image_atlas, image_tex, image_view,
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
            wgpu::ImageCopyTexture {
                texture: &tex, mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba_data[..expected_size],
            wgpu::ImageDataLayout {
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
                mipmap_filter: wgpu::FilterMode::Linear,
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
                mipmap_filter: wgpu::FilterMode::Linear,
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
        let bgl_refs: Vec<&wgpu::BindGroupLayout> = if let Some(bgl) = self.webgl_uniform_bgls.get(&program_id) {
            vec![bgl]
        } else {
            Vec::new()
        };
        let pl_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("webgl_pl_{program_id}")),
            bind_group_layouts: &bgl_refs,
            push_constant_ranges: &[],
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("webgl_pipeline_{program_id}")),
            layout: Some(&pl_layout),
            vertex: wgpu::VertexState {
                module: &modules.0, entry_point: "main",
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &modules.1, entry_point: "main",
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
            multiview: None,
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("webgl_draw_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
    ) -> bool {
        let mut any = false;
        self.walk_webgl(root, swap_view, webgl_states, &mut any);
        any
    }

    fn walk_webgl(
        &mut self,
        bx: &super::layout::LayoutBox,
        swap_view: &wgpu::TextureView,
        webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
        any: &mut bool,
    ) {
        if bx.tag.as_deref() == Some("canvas") {
            if let Some(node) = &bx.node {
                let ptr = std::rc::Rc::as_ptr(node) as usize;
                if let Some(state_rc) = webgl_states.get(&ptr) {
                    if self.execute_webgl_canvas(ptr, state_rc, bx, swap_view) {
                        *any = true;
                    }
                }
            }
        }
        for ch in &bx.children {
            self.walk_webgl(ch, swap_view, webgl_states, any);
        }
    }

    fn execute_webgl_canvas(
        &mut self,
        canvas_ptr: usize,
        state_rc: &std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>,
        bx: &super::layout::LayoutBox,
        swap_view: &wgpu::TextureView,
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

        // Composit canvas RT region do swap chain
        if had_render {
            // Vyrobit novy view z texture (TextureView neni Clone, ale Texture umi vyrobit dalsi view).
            let new_view = self.webgl_canvas_rts.get(&canvas_ptr).map(|(tex, _, _, _)| {
                tex.create_view(&Default::default())
            });
            if let Some(view) = new_view {
                self.compose_view_to_swap(swap_view, &view, bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height);
            }
        }
        had_render
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("webgl_draw_elements"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("webgl_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: color[0] as f64, g: color[1] as f64,
                            b: color[2] as f64, a: color[3] as f64,
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur_h"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
            let mut pass = encoder2.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur_v"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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

    /// Upload image atlas do GPU, jen pokud byly pridany nove obrazky.
    fn upload_image_atlas(&mut self) {
        if !self.image_atlas.dirty { return; }
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.image_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.image_atlas.pixels,
            wgpu::ImageDataLayout {
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

    /// Renderuje display list s podporou filter subtree + backdrop-filter
    /// + WebGL canvas pass v ramci JEDNOHO swap chain frame.
    /// Vse kreslime do main_rt (intermediate RT), na konci compose -> swap chain.
    /// Backdrop-filter muze cist obsah main_rt (scena za elementem).
    pub fn draw_full_frame(
        &mut self,
        cmds: &[DisplayCommand],
        layout_root: &super::layout::LayoutBox,
        webgl_states: Option<&std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>>,
    ) {
        // Update viewport uniform pro main pipeline
        let vp = [self.config.width as f32, self.config.height as f32, 0.0, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));

        // Dirty rect: cely frame je dirty (aktualne full-redraw)
        self.dirty_region.mark_all(self.config.width as f32, self.config.height as f32);
        let _dirty = self.dirty_region.take(); // reserved pro future incremental render

        // Acquire frame
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let swap_view = frame.texture.create_view(&Default::default());
        // Main RT view - sem kreslime (ne primo na swap chain)
        let main_rt_view = self.main_rt.create_view(&Default::default());

        // 1. CSS display list -> main_rt
        let had_segments = self.draw_segments_into_view(&main_rt_view, cmds);

        // 2. WebGL pass -> main_rt
        let mut webgl_did_render = false;
        if let Some(states) = webgl_states {
            if !states.is_empty() {
                webgl_did_render = self.run_webgl_frame(layout_root, &main_rt_view, states);
            }
        }

        // 3. Composit main_rt -> swap chain
        if had_segments || webgl_did_render {
            let vw = self.config.width as f32;
            let vh = self.config.height as f32;
            self.compose_view_to_swap(&swap_view, &main_rt_view, 0.0, 0.0, vw, vh);
        } else {
            // Nic nekresleno - clear swap chain primo
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frame_clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
        let segments: Vec<Seg> = partition_filter_segments(cmds);
        let mut first_pass = true;
        for seg in segments {
            match seg {
                Seg::Main(slice) => {
                    let verts = build_vertices(slice, &self.atlas, &self.image_atlas);
                    self.draw_main_pass(view, &verts, first_pass);
                    first_pass = false;
                }
                Seg::Filter { inner, x, y, w, h, radius, color_matrix } => {
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas);
                    self.draw_to_offscreen(&inner_verts);
                    if radius >= 0.5 {
                        self.run_blur_passes(radius);
                    }
                    self.compose_offscreen(view, x, y, w, h, &color_matrix, first_pass);
                    first_pass = false;
                }
                Seg::Transform3D { inner, x, y, w, h, matrix } => {
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas);
                    self.draw_to_offscreen(&inner_verts);
                    self.compose_transform(view, x, y, w, h, &matrix, first_pass);
                    first_pass = false;
                }
                Seg::Mask { inner, x, y, w, h, mask_src } => {
                    // 1. Render obsah do offscreen RT
                    let inner_verts = build_vertices(inner, &self.atlas, &self.image_atlas);
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
                        wgpu::ImageCopyTexture {
                            texture: &self.main_rt,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::ImageCopyTexture {
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
                                let v = build_vertices(s, &self.atlas, &self.image_atlas);
                                self.draw_main_pass(view, &v, false);
                            }
                            Seg::Filter { inner: fi, x: fx, y: fy, w: fw, h: fh, radius: fr, color_matrix: fm } => {
                                let iv = build_vertices(fi, &self.atlas, &self.image_atlas);
                                self.draw_to_offscreen(&iv);
                                if fr >= 0.5 { self.run_blur_passes(fr); }
                                self.compose_offscreen(view, fx, fy, fw, fh, &fm, false);
                            }
                            Seg::Transform3D { inner: ti, x: tx, y: ty, w: tw, h: th, matrix: tm } => {
                                let iv = build_vertices(ti, &self.atlas, &self.image_atlas);
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
        !first_pass
    }

    /// Legacy wrapper - draw_segments bez WebGL pass.
    /// Pro App::render se preferuje draw_full_frame ktera handluje WebGL.
    fn draw_segments(&mut self, cmds: &[DisplayCommand]) {
        // Update viewport uniform
        let vp = [self.config.width as f32, self.config.height as f32, 0.0, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let had_segments = self.draw_segments_into_view(&view, cmds);
        if !had_segments {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear_only"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_seg"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen_subtree"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let hw = w * 0.5;
        let hh = h * 0.5;
        let vw = self.config.width as f32;
        let vh = self.config.height as f32;
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose_view_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
        // Scissor: clamp do swap chain rozmeru, integer pixely
        let vw = self.config.width as i32;
        let vh = self.config.height as i32;
        let sx = x.max(0.0) as i32;
        let sy = y.max(0.0) as i32;
        let sw = ((x + w).min(vw as f32) as i32 - sx).max(0);
        let sh = ((y + h).min(vh as f32) as i32 - sy).max(0);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
        let vw = self.config.width as f32;
        let vh = self.config.height as f32;
        let u0 = (x / vw).clamp(0.0, 1.0);
        let v0 = (y / vh).clamp(0.0, 1.0);
        let u1 = ((x + w) / vw).clamp(0.0, 1.0);
        let v1 = ((y + h) / vh).clamp(0.0, 1.0);
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
            // center (cx, cy, hw, hh)
            cx, cy, hw, hh,
            // viewport
            vw, vh, 0.0, 0.0,
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transform_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
