//! CSS transform-chain parsing - tokenize transform string -> Vec<TransformOp>.

use super::{TransformOp, parse_length};

pub fn parse_transform_chain(s: &str) -> Vec<TransformOp> {
    let mut out = Vec::new();
    let s = s.trim();
    if s == "none" || s.is_empty() { return out; }
    let mut chars = s.char_indices().peekable();
    while let Some(&(_, c)) = chars.peek() {
        if c.is_whitespace() { chars.next(); continue; }
        // Najdi name + (...)
        let start = chars.peek().map(|&(i, _)| i).unwrap_or(0);
        // Read az do '('
        while let Some(&(_, c)) = chars.peek() {
            if c == '(' { break; }
            if c.is_whitespace() { break; }
            chars.next();
        }
        // Pokracuj az '('
        while let Some(&(_, c)) = chars.peek() {
            if c == '(' { break; }
            chars.next();
        }
        // Sosa do matching ')' - vyvazene
        let mut end = start;
        if let Some(&(i, _)) = chars.peek() {
            chars.next(); // '('
            let mut depth = 1;
            while let Some(&(j, c)) = chars.peek() {
                end = j;
                if c == '(' { depth += 1; }
                if c == ')' { depth -= 1; if depth == 0 { chars.next(); break; } }
                chars.next();
            }
            let _ = i;
        }
        let name_args = &s[start..=end.min(s.len()-1)];
        if let Some(op) = parse_transform(name_args) {
            out.push(op);
        }
    }
    out
}

pub fn parse_transform(s: &str) -> Option<TransformOp> {
    let s = s.trim();
    if s == "none" { return Some(TransformOp::None); }
    if let Some(inner) = s.strip_prefix("translate(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let tx = parts.first().map(|p| parse_length(p)).unwrap_or(0.0);
        let ty = parts.get(1).map(|p| parse_length(p)).unwrap_or(0.0);
        return Some(TransformOp::Translate(tx, ty));
    }
    if let Some(inner) = s.strip_prefix("translateX(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Translate(parse_length(inner), 0.0));
    }
    if let Some(inner) = s.strip_prefix("translateY(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Translate(0.0, parse_length(inner)));
    }
    if let Some(inner) = s.strip_prefix("rotate(").and_then(|x| x.strip_suffix(')')) {
        let trimmed = inner.trim();
        let deg: f32 = if let Some(d) = trimmed.strip_suffix("deg") {
            d.trim().parse().unwrap_or(0.0)
        } else if let Some(r) = trimmed.strip_suffix("rad") {
            return Some(TransformOp::Rotate(r.trim().parse().unwrap_or(0.0)));
        } else { 0.0 };
        return Some(TransformOp::Rotate(deg.to_radians()));
    }
    if let Some(inner) = s.strip_prefix("scale(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let sx = parts.first().and_then(|p| p.parse::<f32>().ok()).unwrap_or(1.0);
        let sy = parts.get(1).and_then(|p| p.parse::<f32>().ok()).unwrap_or(sx);
        return Some(TransformOp::Scale(sx, sy));
    }
    if let Some(inner) = s.strip_prefix("scaleX(").and_then(|x| x.strip_suffix(')')) {
        let sx = inner.trim().parse::<f32>().unwrap_or(1.0);
        return Some(TransformOp::Scale(sx, 1.0));
    }
    if let Some(inner) = s.strip_prefix("scaleY(").and_then(|x| x.strip_suffix(')')) {
        let sy = inner.trim().parse::<f32>().unwrap_or(1.0);
        return Some(TransformOp::Scale(1.0, sy));
    }
    // skew(ax[, ay]) / skewX / skewY - ulozeno jako tan(uhel) (2D shear).
    if let Some(inner) = s.strip_prefix("skewX(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Skew(parse_angle_tan(inner), 0.0));
    }
    if let Some(inner) = s.strip_prefix("skewY(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Skew(0.0, parse_angle_tan(inner)));
    }
    if let Some(inner) = s.strip_prefix("skew(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let ax = parts.first().map(|p| parse_angle_tan(p)).unwrap_or(0.0);
        let ay = parts.get(1).map(|p| parse_angle_tan(p)).unwrap_or(0.0);
        return Some(TransformOp::Skew(ax, ay));
    }
    // 3D varianty
    if let Some(inner) = s.strip_prefix("translate3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let x = parts.first().map(|p| parse_length(p)).unwrap_or(0.0);
        let y = parts.get(1).map(|p| parse_length(p)).unwrap_or(0.0);
        let z = parts.get(2).map(|p| parse_length(p)).unwrap_or(0.0);
        return Some(TransformOp::Translate3D { x, y, z });
    }
    if let Some(inner) = s.strip_prefix("translateZ(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Translate3D { x: 0.0, y: 0.0, z: parse_length(inner) });
    }
    if let Some(inner) = s.strip_prefix("rotate3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let x = parts.first().and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
        let y = parts.get(1).and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
        let z = parts.get(2).and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0);
        let ang = parts.get(3).and_then(|p| {
            if let Some(d) = p.strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()) }
            else if let Some(r) = p.strip_suffix("rad") { r.parse().ok() }
            else { None }
        }).unwrap_or(0.0);
        return Some(TransformOp::Rotate3D { x, y, z, angle_rad: ang });
    }
    if let Some(inner) = s.strip_prefix("rotateX(").and_then(|x| x.strip_suffix(')')) {
        let ang = if let Some(d) = inner.trim().strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()).unwrap_or(0.0) }
            else { 0.0 };
        return Some(TransformOp::Rotate3D { x: 1.0, y: 0.0, z: 0.0, angle_rad: ang });
    }
    if let Some(inner) = s.strip_prefix("rotateY(").and_then(|x| x.strip_suffix(')')) {
        let ang = if let Some(d) = inner.trim().strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()).unwrap_or(0.0) }
            else { 0.0 };
        return Some(TransformOp::Rotate3D { x: 0.0, y: 1.0, z: 0.0, angle_rad: ang });
    }
    if let Some(inner) = s.strip_prefix("rotateZ(").and_then(|x| x.strip_suffix(')')) {
        let ang = if let Some(d) = inner.trim().strip_suffix("deg") { d.parse::<f32>().ok().map(|v| v.to_radians()).unwrap_or(0.0) }
            else { 0.0 };
        return Some(TransformOp::Rotate(ang));
    }
    if let Some(inner) = s.strip_prefix("scale3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let x = parts.first().and_then(|p| p.parse().ok()).unwrap_or(1.0);
        let y = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(1.0);
        let z = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(1.0);
        return Some(TransformOp::Scale3D { x, y, z });
    }
    if let Some(inner) = s.strip_prefix("matrix3d(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<f32> = inner.split(',')
            .filter_map(|p| p.trim().parse().ok())
            .collect();
        if parts.len() == 16 {
            let mut m = [0.0; 16];
            m.copy_from_slice(&parts);
            return Some(TransformOp::Matrix3D(m));
        }
    }
    if let Some(inner) = s.strip_prefix("matrix(").and_then(|x| x.strip_suffix(')')) {
        // matrix(a,b,c,d,e,f) 2D affine: x'=a*x+c*y+e, y'=b*x+d*y+f.
        // Expand na 4x4 ROW-major (= transform_op_matrix konvence, M*v).
        let p: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if p.len() == 6 {
            let (a, b, c, d, e, f) = (p[0], p[1], p[2], p[3], p[4], p[5]);
            return Some(TransformOp::Matrix3D([
                a,   c,   0.0, e,
                b,   d,   0.0, f,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]));
        }
    }
    if let Some(inner) = s.strip_prefix("perspective(").and_then(|x| x.strip_suffix(')')) {
        return Some(TransformOp::Perspective(parse_length(inner)));
    }
    None
}

/// Parse CSS uhel (deg/rad/bare=deg) a vrat jeho tangens (pro skew shear).
fn parse_angle_tan(s: &str) -> f32 {
    let s = s.trim();
    let rad = if let Some(d) = s.strip_suffix("deg") {
        d.trim().parse::<f32>().unwrap_or(0.0).to_radians()
    } else if let Some(r) = s.strip_suffix("rad") {
        r.trim().parse::<f32>().unwrap_or(0.0)
    } else {
        s.parse::<f32>().unwrap_or(0.0).to_radians()
    };
    rad.tan()
}
