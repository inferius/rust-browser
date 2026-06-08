//! Parse text-shadow + box-shadow.

use super::{parse_color, parse_length, split_top_level_commas};

/// Parse text-shadow - vsechny vrstvy (oddelene comma). Kazda vrstva =
/// "offset_x offset_y [blur] color". Multi-layer glow/3D = vice vrstev.
pub fn parse_text_shadow(s: &str) -> Vec<(f32, f32, f32, [u8; 4])> {
    let s = s.trim();
    if s == "none" || s.is_empty() { return Vec::new(); }
    split_top_level_commas(s).iter()
        .filter_map(|layer| parse_text_shadow_layer(layer.trim()))
        .collect()
}

/// Parse jedna text-shadow vrstva: "offset_x offset_y blur color" / "ox oy color".
fn parse_text_shadow_layer(s: &str) -> Option<(f32, f32, f32, [u8; 4])> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 3 { return None; }
    let ox = parse_length(parts[0]);
    let oy = parse_length(parts[1]);
    let mut blur = 0.0f32;
    let mut color_idx = 2;
    if parts[2].chars().next().map(|c| c.is_ascii_digit() || c == '.' || c == '-').unwrap_or(false)
        || parts[2].ends_with("px") || parts[2].ends_with("em")
    {
        blur = parse_length(parts[2]);
        color_idx = 3;
    }
    if color_idx >= parts.len() { return None; }
    let rest: String = parts[color_idx..].join(" ");
    let color = parse_color(&rest)?;
    Some((ox, oy, blur, color))
}

/// Parse box-shadow: "[inset] offset_x offset_y blur spread color".
pub fn parse_box_shadow(s: &str) -> Option<(f32, f32, f32, f32, [u8; 4], bool)> {
    let s = s.trim();
    if s == "none" { return None; }
    // Detect "inset" kdekoliv v retezci - odeber + zaznamenej.
    let mut inset = false;
    let cleaned: String = s.split_whitespace()
        .filter(|w| {
            if *w == "inset" { inset = true; false } else { true }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() < 3 { return None; }
    let ox = parse_length(parts[0]);
    let oy = parse_length(parts[1]);
    let mut blur = 0.0f32;
    let mut spread = 0.0f32;
    let mut color = [0u8, 0, 0, 128];
    let mut i = 2;
    if i < parts.len() && (parts[i].ends_with("px") || parts[i].ends_with("em") || parts[i].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
        blur = parse_length(parts[i]);
        i += 1;
    }
    if i < parts.len() && (parts[i].ends_with("px") || parts[i].ends_with("em") || parts[i].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
        spread = parse_length(parts[i]);
        i += 1;
    }
    if i < parts.len() {
        // Zbyle parts spojeny - barva (mohla obsahovat "rgb(...)")
        let rest: String = parts[i..].join(" ");
        if let Some(c) = parse_color(&rest) {
            color = c;
        }
    }
    Some((ox, oy, blur, spread, color, inset))
}
