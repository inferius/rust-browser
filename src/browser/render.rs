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

/// Posune Y souradnice display command (pro scroll).
fn shift_command_y(cmd: &mut DisplayCommand, dy: f32) {
    match cmd {
        DisplayCommand::Rect { y, .. }
        | DisplayCommand::Border { y, .. }
        | DisplayCommand::Text { y, .. }
        | DisplayCommand::Gradient { y, .. }
        | DisplayCommand::Shadow { y, .. }
        | DisplayCommand::Image { y, .. }
        | DisplayCommand::BlurredRect { y, .. } => *y += dy,
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

            let verts = build_vertices(&display_list, &r.atlas, &r.image_atlas);
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
    /// Pripravene k 2-pass gauss + composit. Aktualne neaktivni.
    offscreen_tex: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
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

        // Offscreen RT - viewport size, RGBA8UnormSrgb
        let offscreen_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_rt"),
            size: wgpu::Extent3d {
                width: config.width.max(1), height: config.height.max(1), depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let offscreen_view = offscreen_tex.create_view(&Default::default());

        Renderer {
            surface, device, queue, config, pipeline, uniform_buf,
            atlas_tex, atlas_view, atlas_smp, bind_group_layout, bind_group, atlas,
            image_atlas, image_tex, image_view,
            font_registry: std::collections::HashMap::new(),
            loaded_font_urls: std::collections::HashSet::new(),
            offscreen_tex, offscreen_view,
        }
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
        // Recreate offscreen RT na novou velikost
        self.offscreen_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_rt"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.offscreen_view = self.offscreen_tex.create_view(&Default::default());
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
