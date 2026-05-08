//! linear-gradient / radial-gradient / conic-gradient parsing.

use super::{parse_color, BgGradient, BgGradientKind, split_top_level_commas};

/// Parsuje linear-gradient / radial-gradient / conic-gradient.
pub fn parse_any_gradient(s: &str) -> Option<BgGradient> {
    let s = s.trim();
    if s.starts_with("linear-gradient(") {
        let (angle, stops) = parse_linear_gradient(s)?;
        return Some(BgGradient {
            kind: BgGradientKind::Linear { angle_deg: angle },
            stops,
        });
    }
    if s.starts_with("radial-gradient(") {
        return parse_radial_gradient(s);
    }
    if s.starts_with("conic-gradient(") {
        return parse_conic_gradient(s);
    }
    None
}

/// Parse radial-gradient([circle|ellipse] [at <position>], color1, color2, ...).
/// Zjednoduseno: bere "circle" jako default, position default center, radius
/// = farthest-corner. Tezi az `at <pos>` syntax. Vsechny dalsi tokeny pred
/// prvnim color jsou ignored.
pub fn parse_radial_gradient(s: &str) -> Option<BgGradient> {
    let inner = s.trim().strip_prefix("radial-gradient(")?.strip_suffix(')')?;
    let parts = split_top_level_commas(inner);
    if parts.len() < 2 { return None; }

    // Pocatecni "config" cast (pres pozici, tvar, velikost). Heuristika:
    // pokud prvni cast neobsahuje barvu, je to config.
    let mut start_idx = 0;
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let radius_pct: f32 = 1.0; // farthest-corner default

    let first = parts[0].trim();
    if parse_color(first).is_none() && !first.contains('%') || first.contains("at ") {
        // Detekuj "at <pos>"
        if let Some(at_idx) = first.find("at ") {
            let pos = first[at_idx + 3..].trim();
            let pos_parts: Vec<&str> = pos.split_whitespace().collect();
            // Pozice: keyword (left/center/right/top/bottom) nebo procenta
            let pct_or_kw = |kw: &str| -> Option<f32> {
                match kw {
                    "left" | "top" => Some(0.0),
                    "center" => Some(0.5),
                    "right" | "bottom" => Some(1.0),
                    s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                    _ => None,
                }
            };
            if pos_parts.len() >= 1 {
                if let Some(v) = pct_or_kw(pos_parts[0]) { cx_pct = v; }
            }
            if pos_parts.len() >= 2 {
                if let Some(v) = pct_or_kw(pos_parts[1]) { cy_pct = v; }
            }
        }
        start_idx = 1;
    }

    let stops = parse_gradient_stops(&parts[start_idx..]);
    if stops.is_empty() { return None; }
    Some(BgGradient {
        kind: BgGradientKind::Radial { cx_pct, cy_pct, radius_pct },
        stops,
    })
}

/// Parse conic-gradient([from <angle>] [at <pos>], color1, color2, ...).
pub fn parse_conic_gradient(s: &str) -> Option<BgGradient> {
    let inner = s.trim().strip_prefix("conic-gradient(")?.strip_suffix(')')?;
    let parts = split_top_level_commas(inner);
    if parts.len() < 2 { return None; }

    let mut start_idx = 0;
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let mut start_angle_deg: f32 = 0.0;

    let first = parts[0].trim();
    if parse_color(first).is_none() {
        // "from 30deg at center" / "from 0deg" / "at 50% 50%"
        if let Some(from_idx) = first.find("from ") {
            let after = &first[from_idx + 5..];
            let token: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
            if let Some(deg) = token.strip_suffix("deg") {
                start_angle_deg = deg.parse().unwrap_or(0.0);
            }
        }
        if let Some(at_idx) = first.find("at ") {
            let pos = first[at_idx + 3..].trim();
            let pos_parts: Vec<&str> = pos.split_whitespace().collect();
            let pct_or_kw = |kw: &str| -> Option<f32> {
                match kw {
                    "left" | "top" => Some(0.0),
                    "center" => Some(0.5),
                    "right" | "bottom" => Some(1.0),
                    s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                    _ => None,
                }
            };
            if pos_parts.len() >= 1 {
                if let Some(v) = pct_or_kw(pos_parts[0]) { cx_pct = v; }
            }
            if pos_parts.len() >= 2 {
                if let Some(v) = pct_or_kw(pos_parts[1]) { cy_pct = v; }
            }
        }
        start_idx = 1;
    }

    let stops = parse_gradient_stops(&parts[start_idx..]);
    if stops.is_empty() { return None; }
    Some(BgGradient {
        kind: BgGradientKind::Conic { cx_pct, cy_pct, start_angle_deg },
        stops,
    })
}

/// Parsuje gradient stops "red 50%" / "red" do (offset 0..1, color).
fn parse_gradient_stops(parts: &[&str]) -> Vec<(f32, [u8; 4])> {
    let mut stops: Vec<(f32, [u8; 4])> = Vec::new();
    let n = parts.len();
    for (i, p) in parts.iter().enumerate() {
        let trimmed = p.trim();
        let (color_str, offset) = if let Some(percent_idx) = trimmed.rfind('%') {
            let space_idx = trimmed[..percent_idx].rfind(' ').unwrap_or(0);
            let pct: f32 = trimmed[space_idx..percent_idx].trim().parse().unwrap_or(0.0);
            (trimmed[..space_idx].trim().to_string(), pct / 100.0)
        } else {
            let default_offset = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
            (trimmed.to_string(), default_offset)
        };
        if let Some(c) = parse_color(&color_str) {
            stops.push((offset, c));
        }
    }
    stops
}

/// Parse linear-gradient(angle, color, color, ...) -> (angle_deg, stops).
pub fn parse_linear_gradient(s: &str) -> Option<(f32, Vec<(f32, [u8; 4])>)> {
    let s = s.trim();
    let inner = s.strip_prefix("linear-gradient(")?.strip_suffix(')')?;
    // Split na comma respektujici parentheses
    let parts = split_top_level_commas(inner);
    if parts.len() < 2 { return None; }

    let mut angle = 180.0; // default top->bottom
    let mut start_idx = 0;
    let first = parts[0].trim();
    if let Some(deg_str) = first.strip_suffix("deg") {
        if let Ok(a) = deg_str.trim().parse::<f32>() {
            angle = a;
            start_idx = 1;
        }
    } else if first.starts_with("to ") {
        angle = match first.trim_start_matches("to ").trim() {
            "top"    => 0.0,
            "right"  => 90.0,
            "bottom" => 180.0,
            "left"   => 270.0,
            "top right" | "right top" => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left" | "left bottom" => 225.0,
            "top left" | "left top" => 315.0,
            _ => 180.0,
        };
        start_idx = 1;
    }

    let mut stops: Vec<(f32, [u8; 4])> = Vec::new();
    let n = parts.len() - start_idx;
    for (i, p) in parts[start_idx..].iter().enumerate() {
        // "red 50%" nebo jen "red"
        let trimmed = p.trim();
        let (color_str, offset) = if let Some(percent_idx) = trimmed.rfind('%') {
            // Najdi mezeru pred %
            let space_idx = trimmed[..percent_idx].rfind(' ').unwrap_or(0);
            let pct: f32 = trimmed[space_idx..percent_idx].trim().parse().unwrap_or(0.0);
            (trimmed[..space_idx].trim().to_string(), pct / 100.0)
        } else {
            let default_offset = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
            (trimmed.to_string(), default_offset)
        };
        if let Some(c) = parse_color(&color_str) {
            stops.push((offset, c));
        }
    }
    if stops.is_empty() { return None; }
    Some((angle, stops))
}
