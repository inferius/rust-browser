//! linear-gradient / radial-gradient / conic-gradient parsing.

use super::{parse_color, BgGradient, BgGradientKind, split_top_level_commas};

/// Parsuje linear-gradient / radial-gradient / conic-gradient.
pub fn parse_any_gradient(s: &str) -> Option<BgGradient> {
    let s = s.trim();
    // repeating-linear/radial/conic-gradient: drive vracelo None (parser prefix
    // neznal) -> 11,12 se nevykreslily vubec. Strip 'repeating-' a parsuj jako
    // neopakovany (aspon prvni cyklus). Plne opakovani (fract v shaderu) = TODO.
    let s = s.strip_prefix("repeating-").unwrap_or(s);
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
/// Parse jeden gradient stop -> 1 nebo 2 stopy.
/// - "color"            -> 1 stop, default offset (rovnomerne dle i/n)
/// - "color 50%"        -> 1 stop na 50%
/// - "color 33% 66%"    -> 2 stopy (CSS hard stop / band) - color@33% + color@66%
/// Bez double-position handlingu se "color P1 P2" parsoval jako color="color P1"
/// (invalid) -> cely stop DROPNUT = chybejici barevny pas v gradientu.
fn parse_stop_part(trimmed: &str, i: usize, n: usize) -> Vec<(f32, [u8; 4])> {
    // Conic deg pozice (color 90deg / color 90deg 180deg) - drive jen %, deg stopy
    // se dropovaly (parse_color("color 90deg") selhal) -> conic-quadrant prazdny.
    // deg/360 = fraction (0..1).
    if trimmed.contains("deg") {
        let toks: Vec<&str> = trimmed.split_whitespace().collect();
        let degs: Vec<f32> = toks.iter()
            .filter_map(|t| t.strip_suffix("deg").and_then(|d| d.trim().parse::<f32>().ok()))
            .map(|d| (d / 360.0).clamp(0.0, 1.0))
            .collect();
        if !degs.is_empty() {
            let color_str: String = toks.iter().take_while(|t| !t.ends_with("deg"))
                .cloned().collect::<Vec<_>>().join(" ");
            if let Some(c) = parse_color(color_str.trim()) {
                return degs.into_iter().map(|d| (d, c)).collect();
            }
        }
    }
    let pct_positions: Vec<usize> = trimmed.match_indices('%').map(|(idx, _)| idx).collect();
    if pct_positions.len() >= 2 {
        let first = pct_positions[0];
        let last = *pct_positions.last().unwrap();
        let p1_start = trimmed[..first].rfind(char::is_whitespace).map(|x| x + 1).unwrap_or(0);
        let p2_start = trimmed[..last].rfind(char::is_whitespace).map(|x| x + 1).unwrap_or(0);
        let p1: f32 = trimmed[p1_start..first].trim().parse().unwrap_or(0.0) / 100.0;
        let p2: f32 = trimmed[p2_start..last].trim().parse().unwrap_or(0.0) / 100.0;
        let color_str = trimmed[..p1_start].trim();
        return match parse_color(color_str) {
            Some(c) => vec![(p1, c), (p2, c)],
            None => vec![],
        };
    }
    let (color_str, offset) = if let Some(percent_idx) = trimmed.rfind('%') {
        let space_idx = trimmed[..percent_idx].rfind(' ').unwrap_or(0);
        let pct: f32 = trimmed[space_idx..percent_idx].trim().parse().unwrap_or(0.0);
        (trimmed[..space_idx].trim().to_string(), pct / 100.0)
    } else {
        let default_offset = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
        // Pixelove pozice ("color px" / "color px1 px2"): nelze resolvnout na
        // fraction bez velikosti boxu (gradient line length) -> extrahuj jen
        // color (leading tokeny pred prvnim cislem/px) + default offset. Bez
        // tohoto parse_color celeho "#000 0 10px" selhal -> stop dropnut ->
        // repeating-linear/radial s px = blank box.
        let toks: Vec<&str> = trimmed.split_whitespace().collect();
        let color_end = toks.iter().position(|t| {
            t.ends_with("px")
                || t.chars().next().map(|c| c.is_ascii_digit() || c == '-' || c == '.').unwrap_or(false)
        }).unwrap_or(toks.len());
        if color_end == 0 {
            (trimmed.to_string(), default_offset)
        } else {
            (toks[..color_end].join(" "), default_offset)
        }
    };
    match parse_color(&color_str) {
        Some(c) => vec![(offset, c)],
        None => vec![],
    }
}

fn parse_gradient_stops(parts: &[&str]) -> Vec<(f32, [u8; 4])> {
    let mut stops: Vec<(f32, [u8; 4])> = Vec::new();
    let n = parts.len();
    for (i, p) in parts.iter().enumerate() {
        stops.extend(parse_stop_part(p.trim(), i, n));
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
        // "red", "red 50%" nebo "red 33% 66%" (hard stop = 2 stopy) - sdileny parser.
        stops.extend(parse_stop_part(p.trim(), i, n));
    }
    if stops.is_empty() { return None; }
    Some((angle, stops))
}

#[cfg(test)]
mod px_stop_tests {
    use super::*;

    #[test]
    fn repeating_linear_with_px_stops_parses_colors_not_blank() {
        // px pozice nelze resolvnout bez box size, ale color musi byt extrahovan
        // (jinak parse_color("#000 0 10px") selze -> 0 stops -> blank box).
        let g = parse_any_gradient("repeating-linear-gradient(45deg, #000000 0 10px, #ffffff 10px 20px)")
            .expect("px-stop gradient se musi naparsovat");
        assert!(g.stops.len() >= 2, "musi mit aspon 2 stops (color extrahovan z px), ma {}", g.stops.len());
        // Prvni stop cerny, posledni bily.
        assert_eq!(g.stops.first().unwrap().1, [0, 0, 0, 255]);
        assert_eq!(g.stops.last().unwrap().1, [255, 255, 255, 255]);
    }

    #[test]
    fn linear_px_single_stop_extracts_color() {
        let g = parse_linear_gradient("linear-gradient(to right, red 0px, blue 100px)")
            .expect("px linear parse");
        assert_eq!(g.1.len(), 2, "2 barevne stopy");
    }
}
