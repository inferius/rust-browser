//! Vertex push primitives: rect, gradient, shadow, image, text glyph, polygon.
//!
//! Kazdy push_* helper appne 6 vertices (2 trianguly per quad) do verts vec.
//! Vertex.mode urci shader path: 0=solid, 1=text, 2=gradient, 3=shadow, 4=image,
//! 5=multi-stop, 6=radial, 7=conic, 8=blurred, atd.

use super::Vertex;
use super::polygon::{polygon_signed_area, clip_unit_square_to_axis_range};

pub(super) fn push_rect_rounded(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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

pub(super) fn push_rect(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
             color: [f32; 4], uv: [f32; 2], mode: f32) {
    push_rect_uv(verts, x, y, w, h, color, uv, [uv[0], uv[1]], mode);
}

/// Italic skewed glyph quad - horni 2 vertices x-posunute o `skew`, dolni
/// zachovavaji puvodni gx. Vysledek = sklonene glyfy (fake italic). UV
/// stejne ako u rect_uv (texture sample dle puvodnich corners).
pub(super) fn push_skewed_quad(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
                     skew: f32, color: [f32; 4], uv0: [f32; 2], uv1: [f32; 2]) {
    let mk = |px: f32, py: f32, u: f32, v: f32| -> Vertex {
        Vertex {
            pos: [px, py], color, uv: [u, v], mode: 1.0,
            local: [0.0, 0.0], half_size: [0.0, 0.0], radius: 0.0,
            color2: [0.0; 4], blur: 0.0,
        }
    };
    let tl = mk(x + skew,     y,     uv0[0], uv0[1]);
    let tr = mk(x + skew + w, y,     uv1[0], uv0[1]);
    let bl = mk(x,            y + h, uv0[0], uv1[1]);
    let br = mk(x + w,        y + h, uv1[0], uv1[1]);
    verts.push(tl); verts.push(tr); verts.push(bl);
    verts.push(bl); verts.push(tr); verts.push(br);
}

pub(super) fn push_rect_uv(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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


/// Push 3-vertex triangle pro polygon clip-path (mode 0 = solid).
pub(super) fn push_triangle(verts: &mut Vec<Vertex>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), color: [f32; 4]) {
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

/// Pro polygon hrany emit 1px outward feather strip (alpha 1.0 -> 0.0).
/// Mode 0 plain color s per-vertex alpha; GPU bilinear interpoluje -> AA edge.
/// Outward normal smer urceny dle winding (signed area).
pub(super) fn push_polygon_edge_aa(verts: &mut Vec<Vertex>, points: &[(f32, f32)], color: [f32; 4], zoom: f32) {
    if points.len() < 3 { return; }
    // V screen-space (y down): CW area > 0, CCW < 0.
    // Outward normal: pro CW edge (p0 -> p1) je vlevo od smeru (-dy, dx).
    // Pro CCW je vpravo (dy, -dx).
    let area = polygon_signed_area(points);
    let cw = area > 0.0;
    // Feather = 1 physical px = 1/zoom logical px (sharp at any zoom level).
    let feather: f32 = 1.0 / zoom.max(0.0001);
    let mk = |p: (f32, f32), a: f32| -> Vertex {
        Vertex {
            pos: [p.0, p.1],
            color: [color[0], color[1], color[2], color[3] * a],
            uv: [0.0, 0.0],
            mode: 0.0,
            local: [0.0, 0.0],
            half_size: [0.0, 0.0],
            radius: 0.0,
            color2: [0.0; 4],
            blur: 0.0,
        }
    };
    let n = points.len();
    for i in 0..n {
        let p0 = points[i];
        let p1 = points[(i + 1) % n];
        let dx = p1.0 - p0.0;
        let dy = p1.1 - p0.1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 { continue; }
        // CW polygon v screen-space (y-down): outward normal je VPRAVO od edge
        // direction. Vector (dx, dy), rotace 90 CW v screen-y-down -> (dy, -dx).
        // CCW: opacne (-dy, dx).
        let (nx, ny) = if cw {
            (dy / len, -dx / len)
        } else {
            (-dy / len, dx / len)
        };
        let p0_out = (p0.0 + nx * feather, p0.1 + ny * feather);
        let p1_out = (p1.0 + nx * feather, p1.1 + ny * feather);
        // Strip: (p0, p1, p1_out) + (p0, p1_out, p0_out)
        verts.push(mk(p0, 1.0));
        verts.push(mk(p1, 1.0));
        verts.push(mk(p1_out, 0.0));
        verts.push(mk(p0, 1.0));
        verts.push(mk(p1_out, 0.0));
        verts.push(mk(p0_out, 0.0));
    }
}

/// Blurred rect: mode 8, solid color s smoothstep blur edge.
pub(super) fn push_blurred_rect(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_image(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_radial_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_conic_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_multi_stop_linear_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_multi_stop_radial_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_multi_stop_conic_gradient(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
        // Center vertex: pouzij midpoint barvy slice (ne fixni 0.0). Konic
        // gradient = konstantni barva podel radiusu, varies podel uhlu.
        // Pri fixed center color triangle interpoluje radial->angle = chybne.
        let c_center = interp_color((frac0 + frac1) * 0.5);
        push_v(verts, p_center, c_center);
        push_v(verts, p_a, c0);
        push_v(verts, p_b, c1);
    }
}

/// Sutherland-Hodgman polygon clip + axis range clip helpers.
pub(super) fn push_shadow(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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
pub(super) fn push_inset_shadow(verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32,
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

/// Normalizuje sRGB byte barvu na linear-space [0..1] floats.
/// Surface format je Rgba8UnormSrgb / Bgra8UnormSrgb - shader pisi LINEAR
/// values, GPU dela linear->sRGB encoding na pixel write. CSS hex barvy jsou
/// sRGB (display values), takze je nutne sRGB->linear convert pred shaderem.
/// Bez tohoto se sRGB byte trated jako linear a surface re-encoduje pres
/// gamma 2.2 = barvy "vyblednou" (svetlejsi nez ma byt).
pub(super) fn normalize_color(c: &[u8; 4]) -> [f32; 4] {
    fn srgb_to_linear(s: f32) -> f32 {
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    }
    [
        srgb_to_linear(c[0] as f32 / 255.0),
        srgb_to_linear(c[1] as f32 / 255.0),
        srgb_to_linear(c[2] as f32 / 255.0),
        c[3] as f32 / 255.0, // Alpha je nepotrebuje gamma korekci.
    ]
}
