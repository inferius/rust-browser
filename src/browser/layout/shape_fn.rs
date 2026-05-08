//! CSS clip-path basic shapes: circle / ellipse / polygon / inset.

use super::parse_length;

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeFunction {
    /// inset(top right bottom left [round <radius>])
    Inset { top: f32, right: f32, bottom: f32, left: f32, radius: f32 },
    /// circle(radius at cx cy) - radius v procentech (0..1)
    Circle { radius_pct: f32, cx_pct: f32, cy_pct: f32 },
    /// ellipse(rx ry at cx cy)
    Ellipse { rx_pct: f32, ry_pct: f32, cx_pct: f32, cy_pct: f32 },
    /// polygon(point1, point2, ...)
    Polygon(Vec<(f32, f32)>),
}

/// Parsuje shape-outside hodnotu (circle / ellipse / polygon / inset).
pub fn parse_shape_function(value: &str) -> Option<ShapeFunction> {
    let v = value.trim();
    if let Some(inner) = v.strip_prefix("circle(").and_then(|s| s.strip_suffix(')')) {
        let inner = inner.trim();
        let (rad, cxcy) = match inner.find(" at ") {
            Some(idx) => (&inner[..idx], &inner[idx+4..]),
            None => (inner, "50% 50%"),
        };
        let rad_pct = parse_percent_or_length_pct(rad.trim()).unwrap_or(0.5);
        let cs: Vec<&str> = cxcy.split_whitespace().collect();
        let cx = cs.get(0).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let cy = cs.get(1).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        return Some(ShapeFunction::Circle { radius_pct: rad_pct, cx_pct: cx, cy_pct: cy });
    }
    if let Some(inner) = v.strip_prefix("ellipse(").and_then(|s| s.strip_suffix(')')) {
        let inner = inner.trim();
        let (rxry, cxcy) = match inner.find(" at ") {
            Some(idx) => (&inner[..idx], &inner[idx+4..]),
            None => (inner, "50% 50%"),
        };
        let rs: Vec<&str> = rxry.split_whitespace().collect();
        let rx = rs.get(0).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let ry = rs.get(1).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let cs: Vec<&str> = cxcy.split_whitespace().collect();
        let cx = cs.get(0).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        let cy = cs.get(1).and_then(|s| parse_percent_or_length_pct(s)).unwrap_or(0.5);
        return Some(ShapeFunction::Ellipse { rx_pct: rx, ry_pct: ry, cx_pct: cx, cy_pct: cy });
    }
    if let Some(inner) = v.strip_prefix("inset(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split_whitespace().collect();
        let nums: Vec<f32> = parts.iter().take_while(|p| !p.starts_with("round"))
            .filter_map(|p| parse_percent_or_length_pct(p)).collect();
        let (top, right, bottom, left) = match nums.len() {
            1 => (nums[0], nums[0], nums[0], nums[0]),
            2 => (nums[0], nums[1], nums[0], nums[1]),
            3 => (nums[0], nums[1], nums[2], nums[1]),
            n if n >= 4 => (nums[0], nums[1], nums[2], nums[3]),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        let radius = parts.iter().skip_while(|p| **p != "round").nth(1)
            .and_then(|p| parse_percent_or_length_pct(p)).unwrap_or(0.0);
        return Some(ShapeFunction::Inset { top, right, bottom, left, radius });
    }
    if let Some(inner) = v.strip_prefix("polygon(").and_then(|s| s.strip_suffix(')')) {
        let mut pts = Vec::new();
        for pair in inner.split(',') {
            let coords: Vec<&str> = pair.split_whitespace().collect();
            if coords.len() >= 2 {
                let x = parse_percent_or_length_pct(coords[0]).unwrap_or(0.0);
                let y = parse_percent_or_length_pct(coords[1]).unwrap_or(0.0);
                pts.push((x, y));
            }
        }
        return Some(ShapeFunction::Polygon(pts));
    }
    None
}

fn parse_percent_or_length_pct(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|v| v / 100.0);
    }
    if s.ends_with("px") || s.ends_with("em") || s.ends_with("rem") {
        // Pri absence box rozmeru zatim approximace: 16px = 1rem -> 1.0
        return Some(parse_length(s) / 100.0);
    }
    s.parse::<f32>().ok()
}
