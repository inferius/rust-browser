//! Polygon math + clipping helpers.

/// Cross product (b - a) x (c - a) v 2D (z-component).
pub(super) fn poly_cross(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// Test ze bod p je uvnitr trojuhelniku (a, b, c) - barycentric.
pub(super) fn point_in_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
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

// Ex-helper multi-stop linear gradientu - nahrazen px-space clipem primo v
// primitives.rs (normalized-space projekce mela aspect skew). Ponechan pro testy.
#[allow(dead_code)]
pub(super) fn clip_unit_square_to_axis_range(dx: f32, dy: f32, t_min: f32, t_max: f32) -> Vec<(f32, f32)> {
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

pub(super) fn clip_polygon<F, G>(poly: &[(f32, f32)], inside: F, intersect: G) -> Vec<(f32, f32)>
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

