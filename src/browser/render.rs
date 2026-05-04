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
    // 9-tap Gaussian (sigma ~ radius/3)
    let weights = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    let step = params.direction * params.texel * params.radius * 0.3;
    var color = textureSample(src_tex, src_smp, in.uv) * weights[0];
    for (var i = 1; i < 5; i = i + 1) {
        let off = step * f32(i);
        color = color + textureSample(src_tex, src_smp, in.uv + off) * weights[i];
        color = color + textureSample(src_tex, src_smp, in.uv - off) * weights[i];
    }
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
            DisplayCommand::Text { x, y, content, color, font_size, bold: _, font_family } => {
                let c = normalize_color(color);
                let mut pen_x = *x;
                let pen_y = *y + *font_size;
                for ch in content.chars() {
                    if let Some(g) = atlas.get(font_family, ch, *font_size as u32) {
                        let gx = pen_x + g.bearing_x;
                        let gy = pen_y - g.bearing_y;
                        push_rect_uv(&mut verts, gx, gy, g.width, g.height, c, g.uv0, g.uv1, 1.0);
                        pen_x += g.advance;
                    } else {
                        pen_x += font_size * 0.5;
                    }
                }
            }
            DisplayCommand::Gradient { x, y, w, h, kind, stops, radius } => {
                if stops.len() >= 2 {
                    let c0 = normalize_color(&stops[0].1);
                    let c1 = normalize_color(&stops.last().unwrap().1);
                    use crate::browser::paint::GradientKind;
                    match kind {
                        GradientKind::Linear { angle_deg } => {
                            push_gradient(&mut verts, *x, *y, *w, *h, *angle_deg, c0, c1, *radius);
                        }
                        GradientKind::Radial { cx, cy, radius: grad_r } => {
                            push_radial_gradient(&mut verts, *x, *y, *w, *h, *cx, *cy, *grad_r, c0, c1, *radius);
                        }
                        GradientKind::Conic { cx, cy, start_angle_deg } => {
                            push_conic_gradient(&mut verts, *x, *y, *w, *h, *cx, *cy, *start_angle_deg, c0, c1, *radius);
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
            | DisplayCommand::TransformBegin { .. } | DisplayCommand::TransformEnd => {
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
                                font_family: String::new(),
                            });
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

/// Serialize uniformy z WebGLState dle program uniform layout do bytes.
/// Buffer alokovan na uniform_buffer_size, prazdne sloty zustavaji 0.
pub fn webgl_serialize_uniforms(
    layout: &[crate::interpreter::UniformSlot],
    values: &std::collections::HashMap<String, crate::interpreter::WebGLUniformValue>,
    buffer_size: u64,
) -> Vec<u8> {
    use crate::interpreter::{UniformSlotKind, WebGLUniformValue};
    let mut out = vec![0u8; buffer_size as usize];
    for slot in layout {
        let val = match values.get(&slot.name) {
            Some(v) => v,
            None => continue,
        };
        let off = slot.offset as usize;
        if off + slot.size as usize > out.len() { continue; }
        match (slot.kind, val) {
            (UniformSlotKind::Float, WebGLUniformValue::Float(v)) => {
                if let Some(&x) = v.first() {
                    out[off..off+4].copy_from_slice(&x.to_le_bytes());
                }
            }
            (UniformSlotKind::Vec2, WebGLUniformValue::Float(v)) => {
                if v.len() >= 2 {
                    out[off..off+4].copy_from_slice(&v[0].to_le_bytes());
                    out[off+4..off+8].copy_from_slice(&v[1].to_le_bytes());
                }
            }
            (UniformSlotKind::Vec3, WebGLUniformValue::Float(v)) => {
                if v.len() >= 3 {
                    out[off..off+4].copy_from_slice(&v[0].to_le_bytes());
                    out[off+4..off+8].copy_from_slice(&v[1].to_le_bytes());
                    out[off+8..off+12].copy_from_slice(&v[2].to_le_bytes());
                    // Vec3 v WGSL std140 ma 4-component padding (16 byte size).
                }
            }
            (UniformSlotKind::Vec4, WebGLUniformValue::Float(v)) => {
                for i in 0..4.min(v.len()) {
                    out[off+i*4..off+i*4+4].copy_from_slice(&v[i].to_le_bytes());
                }
            }
            (UniformSlotKind::Int, WebGLUniformValue::Int(v)) => {
                if let Some(&x) = v.first() {
                    out[off..off+4].copy_from_slice(&x.to_le_bytes());
                }
            }
            (UniformSlotKind::Mat2, WebGLUniformValue::Mat(v)) => {
                for i in 0..4.min(v.len()) {
                    out[off+i*4..off+i*4+4].copy_from_slice(&v[i].to_le_bytes());
                }
            }
            (UniformSlotKind::Mat3, WebGLUniformValue::Mat(v)) => {
                // mat3x3 v WGSL std140: 3 vec3 s padding (3 * 16 = 48 bytes)
                for col in 0..3 {
                    for row in 0..3 {
                        let src_idx = col * 3 + row;
                        if src_idx < v.len() {
                            let dst = off + col * 16 + row * 4;
                            if dst + 4 <= out.len() {
                                out[dst..dst+4].copy_from_slice(&v[src_idx].to_le_bytes());
                            }
                        }
                    }
                }
            }
            (UniformSlotKind::Mat4, WebGLUniformValue::Mat(v)) => {
                for i in 0..16.min(v.len()) {
                    out[off+i*4..off+i*4+4].copy_from_slice(&v[i].to_le_bytes());
                }
            }
            _ => {}
        }
    }
    out
}

/// WebGL component type -> velikost v bytech.
fn webgl_type_size(ctype: u32) -> u32 {
    match ctype {
        0x1400 => 1,  // BYTE
        0x1401 => 1,  // UNSIGNED_BYTE
        0x1402 => 2,  // SHORT
        0x1403 => 2,  // UNSIGNED_SHORT
        0x1404 => 4,  // INT
        0x1405 => 4,  // UNSIGNED_INT
        0x1406 => 4,  // FLOAT
        _ => 4,
    }
}

/// Mapuje WebGL (size, type) na wgpu::VertexFormat.
/// Vraci None pri nepodporovanem formatu.
pub fn webgl_attrib_to_vertex_format(size: i32, ctype: u32) -> Option<wgpu::VertexFormat> {
    use wgpu::VertexFormat as VF;
    match (size, ctype) {
        (1, 0x1406) => Some(VF::Float32),       // FLOAT
        (2, 0x1406) => Some(VF::Float32x2),
        (3, 0x1406) => Some(VF::Float32x3),
        (4, 0x1406) => Some(VF::Float32x4),
        (2, 0x1404) => Some(VF::Sint32x2),      // INT
        (4, 0x1404) => Some(VF::Sint32x4),
        (2, 0x1405) => Some(VF::Uint32x2),      // UNSIGNED_INT
        (4, 0x1405) => Some(VF::Uint32x4),
        _ => None,
    }
}

/// Snapshot z WebGLState extrahovany pro processing.
/// Pure data - nepotrebuje wgpu Device, lze testovat unit.
pub struct WebGLPendingFrame {
    pub commands: Vec<crate::interpreter::WebGLDrawCmd>,
    pub buffers: std::collections::HashMap<u32, Vec<u8>>,
    /// program_id -> (vertex_wgsl, fragment_wgsl)
    pub programs: std::collections::HashMap<u32, (Option<String>, Option<String>)>,
    pub default_clear: [f32; 4],
}

/// Drain queue + clone buffers + extract WGSL strings z linked programs.
/// Po volani je state.draw_queue prazdne. Buffers a programs zustavaji
/// nezmeneny (jen clone).
pub fn webgl_extract_pending(state: &mut crate::interpreter::WebGLState) -> WebGLPendingFrame {
    let commands: Vec<_> = state.draw_queue.drain(..).collect();
    let buffers = state.buffers.clone();
    let programs: std::collections::HashMap<u32, (Option<String>, Option<String>)> = state.programs.iter()
        .map(|(k, p)| (*k, (p.vertex_wgsl.clone(), p.fragment_wgsl.clone())))
        .collect();
    let default_clear = state.clear_color;
    WebGLPendingFrame { commands, buffers, programs, default_clear }
}

/// Vypocita efektivni clear color z command sequence.
/// Vraci Some(color) pokud queue obsahuje Clear s COLOR_BUFFER_BIT (0x4000).
/// Color = posledni ClearColor pred Clear, nebo default pri zadnym ClearColor.
/// None pokud Clear bit chybi.
pub fn webgl_effective_clear(commands: &[crate::interpreter::WebGLDrawCmd], default: [f32; 4]) -> Option<[f32; 4]> {
    use crate::interpreter::WebGLDrawCmd;
    let mut last_cc: Option<[f32; 4]> = None;
    let mut had_clear = false;
    for cmd in commands {
        match cmd {
            WebGLDrawCmd::ClearColor(c) => last_cc = Some(*c),
            WebGLDrawCmd::Clear(mask) => {
                if mask & 0x4000 != 0 { had_clear = true; }
            }
            _ => {}
        }
    }
    if had_clear { Some(last_cc.unwrap_or(default)) } else { None }
}

/// Pocet draw calls (DrawArrays + DrawElements) v command sequence.
pub fn webgl_count_draws(commands: &[crate::interpreter::WebGLDrawCmd]) -> usize {
    use crate::interpreter::WebGLDrawCmd;
    commands.iter().filter(|c| matches!(c,
        WebGLDrawCmd::DrawArrays { .. } | WebGLDrawCmd::DrawElements { .. }
    )).count()
}

/// Pocet clear calls v sequence (jen Clear, ne ClearColor).
pub fn webgl_count_clears(commands: &[crate::interpreter::WebGLDrawCmd]) -> usize {
    use crate::interpreter::WebGLDrawCmd;
    commands.iter().filter(|c| matches!(c, WebGLDrawCmd::Clear(_))).count()
}

/// Vraci IDs vsech linkovanych programu (s vertex + fragment WGSL).
pub fn webgl_linked_program_ids(state: &crate::interpreter::WebGLState) -> Vec<u32> {
    state.programs.iter()
        .filter(|(_, p)| p.linked && p.vertex_wgsl.is_some() && p.fragment_wgsl.is_some())
        .map(|(k, _)| *k)
        .collect()
}

/// True pokud layout tree obsahuje canvas tag s WebGL state pres webgl_states map.
/// Walk celym tree, vraci pri prvni hit.
pub fn webgl_layout_has_canvas(
    bx: &super::layout::LayoutBox,
    webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
) -> bool {
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if webgl_states.contains_key(&ptr) {
                return true;
            }
        }
    }
    bx.children.iter().any(|ch| webgl_layout_has_canvas(ch, webgl_states))
}

/// Spocita pocet WebGL canvases v layout tree.
pub fn webgl_canvas_count(
    bx: &super::layout::LayoutBox,
    webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
) -> usize {
    let mut count = 0;
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if webgl_states.contains_key(&ptr) {
                count += 1;
            }
        }
    }
    for ch in &bx.children {
        count += webgl_canvas_count(ch, webgl_states);
    }
    count
}

/// Spocita stride pro vertex layout pokud slot.stride == 0 (tightly packed).
pub fn webgl_compute_stride(attribs: &[(u32, crate::interpreter::WebGLAttribSlot)]) -> u64 {
    if let Some((_, slot)) = attribs.first() {
        if slot.stride > 0 {
            return slot.stride as u64;
        }
    }
    // Tightly packed: suma sizes * type_size
    attribs.iter().map(|(_, s)| {
        (s.size as u32 * webgl_type_size(s.component_type)) as u64
    }).sum()
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
}

/// Rozdeli display list na Main + Filter + Transform3D segmenty pres
/// FilterBegin/End a TransformBegin/End markery. Pri vnoreni: prvni Begin
/// marker urci typ top-level segmentu, vnorene Begin/End jineho typu
/// jsou soucasti inner cmds (ne novy segment).
/// Symetricnost markeru je predpokladana.
pub fn partition_filter_segments(cmds: &[DisplayCommand]) -> Vec<Seg<'_>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind { Filter, Transform }
    let mut segments: Vec<Seg> = Vec::new();
    let mut depth: i32 = 0;
    let mut active_kind: Option<Kind> = None;
    let mut cursor: usize = 0;
    let mut seg_start: usize = 0;
    let mut filter_params: (f32, f32, f32, f32, f32, [f32; 20]) =
        (0.0, 0.0, 0.0, 0.0, 0.0, [0.0; 20]);
    let mut tx_params: (f32, f32, f32, f32, [f32; 16]) =
        (0.0, 0.0, 0.0, 0.0, [0.0; 16]);
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
        | DisplayCommand::TransformBegin { y, .. } => *y += dy,
        DisplayCommand::ClippedRect { points, .. } => {
            for (_, py) in points.iter_mut() {
                *py += dy;
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::TransformEnd => {}
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

#[derive(Clone, Copy)]
struct ImageInfo {
    /// UV coords v atlasu (0..1)
    uv0: [f32; 2],
    uv1: [f32; 2],
    width: f32,
    height: f32,
}

struct ImageAtlas {
    /// RGBA pixely (4 byte per pixel)
    pixels: Vec<u8>,
    /// src URL/path -> ImageInfo
    cache: std::collections::HashMap<String, ImageInfo>,
    /// Shelf packing kurzor
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    /// Dirty flag - byly pridany nove obrazky -> potreba upload
    dirty: bool,
}

impl ImageAtlas {
    fn new() -> Self {
        ImageAtlas {
            pixels: vec![0u8; (IMAGE_ATLAS_SIZE * IMAGE_ATLAS_SIZE * 4) as usize],
            cache: std::collections::HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    fn get(&self, src: &str) -> Option<&ImageInfo> {
        self.cache.get(src)
    }

    /// Vlozi RGBA bitmap do atlasu. Pri overflow vrati false.
    fn add(&mut self, src: &str, w: u32, h: u32, rgba: &[u8]) -> bool {
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
        start_time: std::time::Instant,
        /// Predchozi cascaded styles - pro detekci transitions.
        prev_style_map: Option<super::cascade::StyleMap>,
        /// Track running animations per (node_id, anim_name) - pro dispatch animationstart/end
        active_animations: std::collections::HashSet<(usize, String)>,
        /// Iteration counter per animation pro animationiteration event.
        animation_iterations: std::collections::HashMap<(usize, String), i32>,
        /// Aktivni CSS transitions.
        active_transitions: Vec<super::cascade::ActiveTransition>,
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
                    // Trigger nasledujici frame (real animation loop)
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
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
            // Nacti @font-face fonty (idempotentni - skip uz loaded)
            for sheet in &stylesheets {
                r.load_font_faces(&sheet.font_faces);
            }
            let mut style_map = cascade::cascade(&document_root, &stylesheets);
            let pseudo_map = cascade::cascade_pseudo(&document_root, &stylesheets);

            let elapsed = self.start_time.elapsed().as_secs_f32();

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
            // Aplikuj transitions na current style map (override hodnoty)
            cascade::apply_transitions(&mut style_map, &self.active_transitions, elapsed);

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

            // Runtime CSS animation: aplikuj @keyframes na elementy s `animation: ...`
            let _animating = cascade::apply_animations(&mut style_map, &stylesheets, elapsed);
            // Scroll-driven animations - pri animation-timeline: scroll() pouzij scroll progress
            let max_scroll = (style_map.len() as f32).max(1.0); // approx; lepsi z layout
            let scroll_progress = if max_scroll > 1.0 { self.scroll_y / max_scroll.max(1.0) } else { 0.0 };
            let _ = cascade::apply_scroll_animations(&mut style_map, &stylesheets, scroll_progress);

            // Detect animation start/end + iteration events
            let mut current_anims: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
            let mut iter_events: Vec<(usize, String, i32)> = Vec::new();
            for (node_id, styles) in &style_map {
                if let Some(spec) = cascade::AnimationSpec::from_styles(styles) {
                    let t = elapsed - spec.delay_secs;
                    if t >= 0.0 && (spec.iteration_count.is_infinite() || t / spec.duration_secs < spec.iteration_count) {
                        let key = (*node_id, spec.name.clone());
                        current_anims.insert(key.clone());
                        // Iteration count
                        let cur_iter = (t / spec.duration_secs).floor() as i32;
                        let prev_iter = self.animation_iterations.get(&key).copied().unwrap_or(-1);
                        if cur_iter > prev_iter && cur_iter > 0 {
                            iter_events.push((*node_id, spec.name.clone(), cur_iter));
                        }
                        self.animation_iterations.insert(key, cur_iter);
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
            let mut layout_root = layout::layout_tree_with_pseudo(&document_root, &style_map, &pseudo_map, viewport_w, viewport_h);
            // Apply position: sticky pri current scroll
            layout::apply_sticky(&mut layout_root, self.scroll_y);
            let mut display_list = paint::build_display_list(&layout_root);

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

            // Pre-rasterize vsechny glyfy do atlasu + nacti images
            for cmd in &display_list {
                match cmd {
                    DisplayCommand::Text { content, font_size, font_family, .. } => {
                        for ch in content.chars() {
                            r.atlas.add(font_family, ch, *font_size as u32);
                        }
                    }
                    DisplayCommand::Image { src, .. } => {
                        r.load_image(src);
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
        start_time: std::time::Instant::now(),
        prev_style_map: None,
        active_transitions: Vec::new(),
        active_animations: std::collections::HashSet::new(),
        animation_iterations: std::collections::HashMap::new(),
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
    /// Image RGBA atlas + GPU texture
    image_atlas: ImageAtlas,
    image_tex: wgpu::Texture,
    image_view: wgpu::TextureView,
    /// @font-face loaded fonts: family -> Font.
    font_registry: std::collections::HashMap<String, fontdue::Font>,
    /// Loaded font URLs (skip re-load).
    loaded_font_urls: std::collections::HashSet<String>,
    /// Offscreen RT pro filter blur / view-transitions (RGBA8 viewport size).
    offscreen_tex: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    /// Druhy RT pro blur 2-pass (ping-pong)
    offscreen_tex_b: wgpu::Texture,
    offscreen_view_b: wgpu::TextureView,
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
    /// 3D transform compose pipeline (samples offscreen RT, kresli quad transformovany matici)
    transform_pipeline: wgpu::RenderPipeline,
    transform_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform pro transform matrix + center + viewport + uv_box (8x vec4 = 128 bytes)
    transform_uniform_buf: wgpu::Buffer,
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
            offscreen_tex, offscreen_view,
            offscreen_tex_b, offscreen_view_b,
            blur_pipeline, blur_bind_group_layout, blur_uniform_buf,
            compose_pipeline, compose_bind_group_layout, compose_uniform_buf,
            transform_pipeline, transform_bind_group_layout, transform_uniform_buf,
            webgl_shader_modules: std::collections::HashMap::new(),
            webgl_pipelines: std::collections::HashMap::new(),
            webgl_buffers: std::collections::HashMap::new(),
            webgl_canvas_rts: std::collections::HashMap::new(),
            webgl_uniform_buffers: std::collections::HashMap::new(),
            webgl_uniform_bgls: std::collections::HashMap::new(),
        }
    }

    /// Ensure uniform buffer + bind group layout pro program.
    /// Pri buffer_size=0 nedela nic. Idempotent.
    pub fn ensure_webgl_uniform_resources(&mut self, program_id: u32, buffer_size: u64) {
        if buffer_size == 0 { return; }
        if !self.webgl_uniform_buffers.contains_key(&program_id) {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("webgl_uniform_buf_{program_id}")),
                size: buffer_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.webgl_uniform_buffers.insert(program_id, buf);
        }
        if !self.webgl_uniform_bgls.contains_key(&program_id) {
            let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("webgl_uniform_bgl_{program_id}")),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false, min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
            self.webgl_uniform_bgls.insert(program_id, bgl);
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

    /// Encode wgpu draw call do canvas RT.
    /// Pipeline + buffer musi byt cached. Vraci true pokud emit success.
    /// Pokud uniform_bytes neni prazdny + uniform buffer exists, write +
    /// bind group set.
    pub fn webgl_encode_draw_arrays(
        &self,
        canvas_ptr: usize,
        program_id: u32,
        first: i32,
        count: i32,
        attribs: &[(u32, crate::interpreter::WebGLAttribSlot)],
        clear_color: Option<[f32; 4]>,
        uniform_bytes: &[u8],
    ) -> bool {
        let view = match self.webgl_canvas_rts.get(&canvas_ptr) {
            Some((_, v, _, _)) => v,
            None => return false,
        };
        let pipeline = match self.webgl_pipelines.get(&program_id) {
            Some(p) => p,
            None => return false,
        };
        // Pre-write uniform buffer + bind group create (pokud uniformy)
        let bind_group = if !uniform_bytes.is_empty() {
            if let (Some(buf), Some(bgl)) = (self.webgl_uniform_buffers.get(&program_id),
                                              self.webgl_uniform_bgls.get(&program_id)) {
                self.queue.write_buffer(buf, 0, uniform_bytes);
                Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("webgl_uniform_bg_{program_id}")),
                    layout: bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                    ],
                }))
            } else { None }
        } else { None };
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
        let (cmds, buffers_data, programs_data, default_clear): (
            Vec<WebGLDrawCmd>,
            std::collections::HashMap<u32, Vec<u8>>,
            std::collections::HashMap<u32, (Option<String>, Option<String>, Vec<crate::interpreter::UniformSlot>, u64)>,
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
                )))
                .collect();
            let cc = state.clear_color;
            (cmds, buffers, programs, cc)
        };

        // Upload buffers
        for (id, data) in &buffers_data {
            if !self.webgl_buffers.contains_key(id) && !data.is_empty() {
                self.upload_webgl_buffer(*id, data);
            }
        }
        // Build shader modules + uniform resources pro linked programs
        for (pid, (vs, fs, _layout, buffer_size)) in &programs_data {
            if let (Some(v), Some(f)) = (vs, fs) {
                self.build_webgl_shader_modules(*pid, v, f);
            }
            if *buffer_size > 0 {
                self.ensure_webgl_uniform_resources(*pid, *buffer_size);
            }
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
                        // Serialize uniformy dle program layout
                        let bytes = if let Some((_, _, layout, size)) = programs_data.get(&pid) {
                            if *size > 0 {
                                webgl_serialize_uniforms(layout, &uniforms, *size)
                            } else { Vec::new() }
                        } else { Vec::new() };
                        if self.ensure_webgl_pipeline(pid, &attribs) {
                            let cc = pending_clear.take();
                            self.webgl_encode_draw_arrays(canvas_ptr, pid, first, count, &attribs, cc, &bytes);
                            had_render = true;
                        }
                    }
                }
                WebGLDrawCmd::DrawElements { program_id, mode, count, index_type, offset, index_buffer_id, attribs, uniforms, viewport: _ } => {
                    let _ = mode;
                    if let (Some(pid), Some(ibo)) = (program_id, index_buffer_id) {
                        let bytes = if let Some((_, _, layout, size)) = programs_data.get(&pid) {
                            if *size > 0 {
                                webgl_serialize_uniforms(layout, &uniforms, *size)
                            } else { Vec::new() }
                        } else { Vec::new() };
                        if self.ensure_webgl_pipeline(pid, &attribs) {
                            let cc = pending_clear.take();
                            self.webgl_encode_draw_elements(canvas_ptr, pid, count, index_type, offset, ibo, &attribs, cc, &bytes);
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
        let bind_group = if !uniform_bytes.is_empty() {
            if let (Some(buf), Some(bgl)) = (self.webgl_uniform_buffers.get(&program_id),
                                              self.webgl_uniform_bgls.get(&program_id)) {
                self.queue.write_buffer(buf, 0, uniform_bytes);
                Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("webgl_uniform_bg_{program_id}_idx")),
                    layout: bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                    ],
                }))
            } else { None }
        } else { None };
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
    /// Skip uz nahrane URL. FS only (HTTP TODO).
    fn load_font_faces(&mut self, font_faces: &[crate::browser::css_parser::FontFace]) {
        use crate::browser::css_parser::extract_font_url;
        for ff in font_faces {
            let url = match extract_font_url(&ff.src) { Some(u) => u, None => continue };
            if self.loaded_font_urls.contains(&url) { continue; }
            // FS path: relative -> static/<url>
            let path = if url.starts_with('/') {
                url.clone()
            } else if url.starts_with("http://") || url.starts_with("https://") {
                continue; // HTTP TODO
            } else {
                format!("static/{url}")
            };
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                    self.font_registry.insert(ff.family.clone(), font.clone());
                    // Sdilet do atlasu pro rasterize lookup
                    self.atlas.extra_fonts.insert(ff.family.clone(), font);
                    self.loaded_font_urls.insert(url);
                }
            }
        }
    }

    /// Nacte image ze souboru a vlozi do RGBA atlasu (pokud neni jiz cached).
    fn load_image(&mut self, src: &str) {
        if self.image_atlas.cache.contains_key(src) { return; }
        // FS load (HTTP zatim skip)
        let path = if src.starts_with("http://") || src.starts_with("https://") {
            return;
        } else if src.starts_with('/') {
            src.to_string()
        } else {
            format!("static/{src}")
        };
        if let Ok(bytes) = std::fs::read(&path) {
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
                        self.image_atlas.add(src, new_w, new_h, &small_rgba.into_raw());
                        return;
                    }
                }
                self.image_atlas.add(src, w, h, &raw);
            }
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

    /// Renderuje display list s podporou filter subtree (blur + color matrix)
    /// + WebGL canvas pass v ramci JEDNOHO swap chain frame.
    /// Phase 3c6: single-frame integration - pred frame.present() vola
    /// run_webgl_frame s acquired view (zadne 2 framy s overlay).
    pub fn draw_full_frame(
        &mut self,
        cmds: &[DisplayCommand],
        layout_root: &super::layout::LayoutBox,
        webgl_states: Option<&std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>>,
    ) {
        // Update viewport uniform pro main pipeline
        let vp = [self.config.width as f32, self.config.height as f32, 0.0, 0.0];
        self.queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&vp));

        // Acquire frame
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());

        // 1. Main display list (segments: Main / Filter / Transform3D)
        let had_segments = self.draw_segments_into_view(&view, cmds);

        // 2. WebGL pass (pokud nejaky canvas s pending queue)
        let mut webgl_did_render = false;
        if let Some(states) = webgl_states {
            if !states.is_empty() {
                webgl_did_render = self.run_webgl_frame(layout_root, &view, states);
            }
        }

        // 3. Pokud nic, alespon clear (aby frame nezustal undefined)
        if !had_segments && !webgl_did_render {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frame_clear"),
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

        // 4. Single present
        frame.present();
    }

    /// Renderuje display list segmenty do existujici TextureView (bez acquire/present).
    /// Vraci true pokud aspon jedna pass byla provedena (pro frame fallback clear).
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
