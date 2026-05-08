//! parse_color (CSS Color L4 superset) + colorspace conversions.
//!
//! Vraci [u8; 4] = sRGB byte rgba. Podpora: hex (#fff/#ffffff/#ffff/#ffffffff),
//! named (red/blue/...), rgb()/rgba(), hsl()/hsla(), hwb(), oklab(), lab(),
//! color() s namespace, color-mix(), contrast(), relative color (rgb from var).

use super::split_top_level_commas;

pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim().to_lowercase();

    // Hex #RGB / #RRGGBB / #RRGGBBAA / #RGBA (4 char)
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some([r, g, b, 255]);
        }
        if hex.len() == 4 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
            return Some([r, g, b, a]);
        }
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some([r, g, b, 255]);
        }
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            return Some([r, g, b, a]);
        }
    }

    // CSS Color L4 - color() function s namespace
    // color(srgb r g b [/ a]), color(display-p3 r g b), color(rec2020 r g b), atd.
    // Pro nase ucely vsechny ColorSpace mapujeme primo na sRGB (pro rec2020/p3 by se mela
    // udelat gamut mapping, ale to je out-of-scope - vetsina renderer fallbackuje stejne).
    if let Some(inner) = s.strip_prefix("color(").and_then(|s| s.strip_suffix(')')) {
        return parse_color_function(inner);
    }

    // CSS Color L5 - relative color: rgb(from <color> r g b [/ alpha])
    // Vyhodnoti zdrojovou barvu, jeji slozky pristupne jako r/g/b/alpha keywordy.
    if let Some(inner) = s.strip_prefix("rgb(from ").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("rgba(from ").and_then(|s| s.strip_suffix(')'))) {
        return parse_relative_rgb(inner);
    }
    if let Some(inner) = s.strip_prefix("hsl(from ").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("hsla(from ").and_then(|s| s.strip_suffix(')'))) {
        return parse_relative_hsl(inner);
    }

    // rgb()/rgba() - legacy (carky) i modern (mezery + lomitko alpha)
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')'))) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let r = parse_color_byte(parts[0])?;
            let g = parse_color_byte(parts[1])?;
            let b = parse_color_byte(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            return Some([r, g, b, a]);
        }
    }

    // hsl()/hsla()
    if let Some(inner) = s.strip_prefix("hsl(").and_then(|s| s.strip_suffix(')'))
        .or_else(|| s.strip_prefix("hsla(").and_then(|s| s.strip_suffix(')'))) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let h = parse_angle_deg(parts[0])?;
            let sat = parse_percent_or_num(parts[1])?;
            let lit = parse_percent_or_num(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = hsl_to_rgb(h, sat, lit);
            return Some([r, g, b, a]);
        }
    }

    // hwb(h w% b%)
    if let Some(inner) = s.strip_prefix("hwb(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let h = parse_angle_deg(parts[0])?;
            let w = parse_percent_or_num(parts[1])?;
            let bl = parse_percent_or_num(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = hwb_to_rgb(h, w, bl);
            return Some([r, g, b, a]);
        }
    }

    // lab(l a b)
    if let Some(inner) = s.strip_prefix("lab(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 100.0)?;
            let a_lab = parts[1].parse::<f32>().ok()?;
            let b_lab = parts[2].parse::<f32>().ok()?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = lab_to_rgb(l, a_lab, b_lab);
            return Some([r, g, b, a]);
        }
    }

    // lch(l c h)
    if let Some(inner) = s.strip_prefix("lch(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 100.0)?;
            let c = parts[1].parse::<f32>().ok()?;
            let h = parse_angle_deg(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (la, lb) = (c * h.to_radians().cos(), c * h.to_radians().sin());
            let (r, g, b) = lab_to_rgb(l, la, lb);
            return Some([r, g, b, a]);
        }
    }

    // oklab(l a b) - L je 0..1 (nebo 0%..100%)
    if let Some(inner) = s.strip_prefix("oklab(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 1.0)?;
            let a_ok = parts[1].parse::<f32>().ok()?;
            let b_ok = parts[2].parse::<f32>().ok()?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (r, g, b) = oklab_to_rgb(l, a_ok, b_ok);
            return Some([r, g, b, a]);
        }
    }

    // oklch(l c h)
    if let Some(inner) = s.strip_prefix("oklch(").and_then(|s| s.strip_suffix(')')) {
        let (parts, alpha) = split_color_args(inner);
        if parts.len() >= 3 {
            let l = parse_percent_or_num_scaled(parts[0], 1.0)?;
            let c = parts[1].parse::<f32>().ok()?;
            let h = parse_angle_deg(parts[2])?;
            let a = alpha.or_else(|| parts.get(3).and_then(|p| parse_alpha(p))).unwrap_or(255);
            let (la, lb) = (c * h.to_radians().cos(), c * h.to_radians().sin());
            let (r, g, b) = oklab_to_rgb(l, la, lb);
            return Some([r, g, b, a]);
        }
    }

    // color-mix(in <space>, c1 [<%>], c2 [<%>])
    if let Some(inner) = s.strip_prefix("color-mix(").and_then(|s| s.strip_suffix(')')) {
        return parse_color_mix(inner);
    }

    // CSS Color L5 - contrast(<color> vs <list>) - vrati nejvic kontrast color
    if let Some(inner) = s.strip_prefix("contrast(").and_then(|s| s.strip_suffix(')')) {
        return parse_contrast(inner);
    }

    // CSS Color L5 - contrast-color(<color>) - vrati black/white podle pozadi
    if let Some(inner) = s.strip_prefix("contrast-color(").and_then(|s| s.strip_suffix(')')) {
        // Vyhodnoti zda bg je svetly nebo tmavy a vrati opacny
        let bg = parse_color(inner.trim())?;
        let luma = (0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32) / 255.0;
        return Some(if luma > 0.5 {
            [0, 0, 0, 255]   // tmavy text na svetlem bg
        } else {
            [255, 255, 255, 255]  // svetly text na tmavem bg
        });
    }

    // CSS Color L5 - light-dark(<light>, <dark>) - vrati prvni v light mode, druhy v dark
    // (Bez color scheme detekci - vraci prvni)
    if let Some(inner) = s.strip_prefix("light-dark(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if let Some(first) = parts.first() {
            return parse_color(first.trim());
        }
    }

    // Named colors (subset)
    match s.as_str() {
        "black"   => Some([0, 0, 0, 255]),
        "white"   => Some([255, 255, 255, 255]),
        "red"     => Some([255, 0, 0, 255]),
        "green"   => Some([0, 128, 0, 255]),
        "blue"    => Some([0, 0, 255, 255]),
        "yellow"  => Some([255, 255, 0, 255]),
        "cyan" | "aqua" => Some([0, 255, 255, 255]),
        "magenta" | "fuchsia" => Some([255, 0, 255, 255]),
        "gray" | "grey" => Some([128, 128, 128, 255]),
        "lightgray" | "lightgrey" => Some([211, 211, 211, 255]),
        "darkgray"  | "darkgrey"  => Some([169, 169, 169, 255]),
        "silver" => Some([192, 192, 192, 255]),
        "maroon" => Some([128, 0, 0, 255]),
        "navy"   => Some([0, 0, 128, 255]),
        "teal"   => Some([0, 128, 128, 255]),
        "olive"  => Some([128, 128, 0, 255]),
        "lime"   => Some([0, 255, 0, 255]),
        "orange"  => Some([255, 165, 0, 255]),
        "orangered" => Some([255, 69, 0, 255]),
        "coral"  => Some([255, 127, 80, 255]),
        "tomato" => Some([255, 99, 71, 255]),
        "salmon" => Some([250, 128, 114, 255]),
        "gold"   => Some([255, 215, 0, 255]),
        "khaki"  => Some([240, 230, 140, 255]),
        "purple"  => Some([128, 0, 128, 255]),
        "violet" => Some([238, 130, 238, 255]),
        "indigo" => Some([75, 0, 130, 255]),
        "pink"    => Some([255, 192, 203, 255]),
        "hotpink" | "deeppink" => Some([255, 20, 147, 255]),
        "brown"   => Some([165, 42, 42, 255]),
        "chocolate" => Some([210, 105, 30, 255]),
        "sienna"  => Some([160, 82, 45, 255]),
        "tan"     => Some([210, 180, 140, 255]),
        "beige"   => Some([245, 245, 220, 255]),
        "ivory"   => Some([255, 255, 240, 255]),
        "snow"    => Some([255, 250, 250, 255]),
        "lavender" => Some([230, 230, 250, 255]),
        "skyblue" | "lightskyblue" => Some([135, 206, 235, 255]),
        "steelblue" => Some([70, 130, 180, 255]),
        "royalblue" => Some([65, 105, 225, 255]),
        "dodgerblue" => Some([30, 144, 255, 255]),
        "deepskyblue" => Some([0, 191, 255, 255]),
        "cornflowerblue" => Some([100, 149, 237, 255]),
        "cadetblue" => Some([95, 158, 160, 255]),
        "turquoise" | "mediumturquoise" => Some([64, 224, 208, 255]),
        "lightblue" => Some([173, 216, 230, 255]),
        "powderblue" => Some([176, 224, 230, 255]),
        "mintcream" => Some([245, 255, 250, 255]),
        "honeydew" => Some([240, 255, 240, 255]),
        "palegreen" | "lightgreen" => Some([144, 238, 144, 255]),
        "mediumseagreen" => Some([60, 179, 113, 255]),
        "seagreen"  => Some([46, 139, 87, 255]),
        "forestgreen" => Some([34, 139, 34, 255]),
        "darkgreen"  => Some([0, 100, 0, 255]),
        "yellowgreen" => Some([154, 205, 50, 255]),
        "lawngreen"   => Some([124, 252, 0, 255]),
        "chartreuse"  => Some([127, 255, 0, 255]),
        "springgreen" => Some([0, 255, 127, 255]),
        "mediumspringgreen" => Some([0, 250, 154, 255]),
        "limegreen"   => Some([50, 205, 50, 255]),
        "crimson"     => Some([220, 20, 60, 255]),
        "darkred"     => Some([139, 0, 0, 255]),
        "firebrick"   => Some([178, 34, 34, 255]),
        "indianred"   => Some([205, 92, 92, 255]),
        "rosybrown"   => Some([188, 143, 143, 255]),
        "lightcoral"  => Some([240, 128, 128, 255]),
        "mistyrose"   => Some([255, 228, 225, 255]),
        "antiquewhite" => Some([250, 235, 215, 255]),
        "linen"        => Some([250, 240, 230, 255]),
        "bisque"       => Some([255, 228, 196, 255]),
        "peachpuff"    => Some([255, 218, 185, 255]),
        "moccasin"     => Some([255, 228, 181, 255]),
        "papayawhip"   => Some([255, 239, 213, 255]),
        "blanchedalmond" => Some([255, 235, 205, 255]),
        "wheat"        => Some([245, 222, 179, 255]),
        "burlywood"    => Some([222, 184, 135, 255]),
        "sandybrown"   => Some([244, 164, 96, 255]),
        "goldenrod"    => Some([218, 165, 32, 255]),
        "darkgoldenrod" => Some([184, 134, 11, 255]),
        "peru"         => Some([205, 133, 63, 255]),
        "saddlebrown"  => Some([139, 69, 19, 255]),
        "darkslategray" | "darkslategrey" => Some([47, 79, 79, 255]),
        "slategray" | "slategrey" => Some([112, 128, 144, 255]),
        "lightslategray" | "lightslategrey" => Some([119, 136, 153, 255]),
        "dimgray" | "dimgrey" => Some([105, 105, 105, 255]),
        "gainsboro" => Some([220, 220, 220, 255]),
        "whitesmoke" => Some([245, 245, 245, 255]),
        "aliceblue"  => Some([240, 248, 255, 255]),
        "ghostwhite" => Some([248, 248, 255, 255]),
        "seashell"   => Some([255, 245, 238, 255]),
        "floralwhite" => Some([255, 250, 240, 255]),
        "oldlace"    => Some([253, 245, 230, 255]),
        "cornsilk"   => Some([255, 248, 220, 255]),
        "lemonchiffon" => Some([255, 250, 205, 255]),
        "lightyellow" => Some([255, 255, 224, 255]),
        "lightgoldenrodyellow" => Some([250, 250, 210, 255]),
        "palegoldenrod" => Some([238, 232, 170, 255]),
        "darkkhaki"  => Some([189, 183, 107, 255]),
        "rebeccapurple" => Some([102, 51, 153, 255]),
        "mediumpurple"  => Some([147, 112, 219, 255]),
        "blueviolet"    => Some([138, 43, 226, 255]),
        "darkviolet"    => Some([148, 0, 211, 255]),
        "darkorchid"    => Some([153, 50, 204, 255]),
        "mediumorchid"  => Some([186, 85, 211, 255]),
        "orchid"        => Some([218, 112, 214, 255]),
        "plum"          => Some([221, 160, 221, 255]),
        "thistle"       => Some([216, 191, 216, 255]),
        "darkmagenta"   => Some([139, 0, 139, 255]),
        "midnightblue"  => Some([25, 25, 112, 255]),
        "darkblue"      => Some([0, 0, 139, 255]),
        "mediumblue"    => Some([0, 0, 205, 255]),
        "darkslateblue" => Some([72, 61, 139, 255]),
        "slateblue"     => Some([106, 90, 205, 255]),
        "mediumslateblue" => Some([123, 104, 238, 255]),
        "lightsteelblue" => Some([176, 196, 222, 255]),
        "azure"         => Some([240, 255, 255, 255]),
        "aquamarine"    => Some([127, 255, 212, 255]),
        "mediumaquamarine" => Some([102, 205, 170, 255]),
        "darkcyan"      => Some([0, 139, 139, 255]),
        "lightcyan"     => Some([224, 255, 255, 255]),
        "paleturquoise" => Some([175, 238, 238, 255]),
        "darkturquoise" => Some([0, 206, 209, 255]),
        "lightseagreen" => Some([32, 178, 170, 255]),
        "mediumvioletred" => Some([199, 21, 133, 255]),
        "palevioletred"   => Some([219, 112, 147, 255]),
        "darksalmon"    => Some([233, 150, 122, 255]),
        "lightsalmon"   => Some([255, 160, 122, 255]),
        "transparent" => Some([0, 0, 0, 0]),
        // CSS Color L4 system-color keywords (light mode defaults, CSS spec sRGB hodnoty)
        "canvas"           => Some([255, 255, 255, 255]),
        "canvastext"       => Some([0, 0, 0, 255]),
        "linktext"         => Some([0, 0, 238, 255]),
        "visitedtext"      => Some([85, 26, 139, 255]),
        "activetext"       => Some([255, 0, 0, 255]),
        "buttonface"       => Some([240, 240, 240, 255]),
        "buttontext"       => Some([0, 0, 0, 255]),
        "buttonborder"     => Some([173, 173, 173, 255]),
        "field"            => Some([255, 255, 255, 255]),
        "fieldtext"        => Some([0, 0, 0, 255]),
        "highlight"        => Some([0, 120, 215, 255]),
        "highlighttext"    => Some([255, 255, 255, 255]),
        "selecteditem"     => Some([0, 120, 215, 255]),
        "selecteditemtext" => Some([255, 255, 255, 255]),
        "mark"             => Some([255, 255, 0, 255]),
        "marktext"         => Some([0, 0, 0, 255]),
        "graytext"         => Some([109, 109, 109, 255]),
        "accentcolor"      => Some([0, 120, 215, 255]),
        "accentcolortext"  => Some([255, 255, 255, 255]),
        _ => None,
    }
}

/// Split argumentu barvy: respektuje modern syntax `r g b / a` (lomitko = alpha).
/// Vrati (positional_args, optional_alpha_byte).
fn split_color_args(inner: &str) -> (Vec<&str>, Option<u8>) {
    // Pokud obsahuje '/', alpha je za nim
    let (main, alpha_str) = match inner.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (inner, None),
    };
    // Modern: mezery; legacy: carky. Tolerujem oboje.
    let parts: Vec<&str> = if main.contains(',') {
        main.split(',').map(str::trim).collect()
    } else {
        main.split_whitespace().collect()
    };
    let alpha = alpha_str.and_then(parse_alpha);
    (parts, alpha)
}

fn parse_color_byte(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.trim().parse().ok()?;
        return Some((v.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8);
    }
    let v: f32 = s.parse().ok()?;
    Some(v.clamp(0.0, 255.0).round() as u8)
}

fn parse_alpha(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.trim().parse().ok()?;
        return Some((v.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8);
    }
    let v: f32 = s.parse().ok()?;
    Some((v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// CSS Color L4 - color() function s namespace.
/// Format: `color(<colorspace> <c1> <c2> <c3> [/ <alpha>])`.
/// Podporovane spaces: srgb, srgb-linear, display-p3, rec2020, a98-rgb, prophoto-rgb,
/// xyz, xyz-d50, xyz-d65. Pro rasterizaci vsechny mapovane primo do sRGB byte (clamp).
fn parse_color_function(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (space, rest) = split_first_color_token(trimmed)?;
    let (parts, alpha) = split_balanced_args(rest);
    if parts.len() < 3 { return None; }
    // V default 0..1 range pro vsechny color spaces. Procenta = 0..100%.
    let parse_chan = |s: &str| -> Option<f32> {
        let s = s.trim();
        if s == "none" { return Some(0.0); }
        if let Some(p) = s.strip_suffix('%') { return p.trim().parse::<f32>().ok().map(|v| v / 100.0); }
        s.parse::<f32>().ok()
    };
    let c1 = parse_chan(parts[0])?;
    let c2 = parse_chan(parts[1])?;
    let c3 = parse_chan(parts[2])?;
    // Pro display-p3 / rec2020 / a98-rgb proste pouzijeme hodnoty jako sRGB
    // (gamut mapping nedelame - vetsina cili je v sRGB ranged).
    let (r, g, b) = match space {
        "srgb-linear" | "srgb" | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020" => {
            ((c1 * 255.0).round() as u8, (c2 * 255.0).round() as u8, (c3 * 255.0).round() as u8)
        }
        "xyz" | "xyz-d50" | "xyz-d65" => {
            // XYZ -> sRGB matrix (D65)
            let r = 3.2404542 * c1 - 1.5371385 * c2 - 0.4985314 * c3;
            let g = -0.9692660 * c1 + 1.8760108 * c2 + 0.0415560 * c3;
            let b = 0.0556434 * c1 - 0.2040259 * c2 + 1.0572252 * c3;
            ((r.clamp(0.0, 1.0) * 255.0).round() as u8,
             (g.clamp(0.0, 1.0) * 255.0).round() as u8,
             (b.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
        _ => return None,
    };
    let a = if let Some(byte) = alpha {
        byte
    } else if let Some(s) = parts.get(3) {
        parse_alpha(s.trim()).unwrap_or(255)
    } else { 255 };
    Some([r, g, b, a])
}

/// CSS Color L5 relative color: rgb(from <color> r g b [/ a]).
/// Slozky source barvy dostupne jako r/g/b/alpha keywordy (0..255 / 0..1).
/// Podporuje cisla, procenta, none, calc(r * 0.5).
fn parse_relative_rgb(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (src_str, rest) = split_first_color_token(trimmed)?;
    let src = parse_color(src_str)?;
    let rs = src[0] as f32; let gs = src[1] as f32; let bs = src[2] as f32;
    let alpha_s = src[3] as f32 / 255.0;
    let (parts, explicit_alpha) = split_balanced_args(rest);
    if parts.len() < 3 { return None; }
    let r = eval_color_component(parts[0], &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s * 255.0)], 255.0)?;
    let g = eval_color_component(parts[1], &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s * 255.0)], 255.0)?;
    let b = eval_color_component(parts[2], &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s * 255.0)], 255.0)?;
    let a: u8 = if let Some(byte) = explicit_alpha {
        byte
    } else if let Some(alpha_str) = parts.get(3) {
        let val = eval_color_component(alpha_str, &[("r", rs), ("g", gs), ("b", bs), ("alpha", alpha_s)], 1.0)?;
        (val.clamp(0.0, 1.0) * 255.0).round() as u8
    } else { src[3] };
    Some([r as u8, g as u8, b as u8, a])
}

/// CSS Color L5 relative HSL: hsl(from <color> h s l [/ a]).
fn parse_relative_hsl(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (src_str, rest) = split_first_color_token(trimmed)?;
    let src = parse_color(src_str)?;
    let (h, sat, lit) = rgb_to_hsl(src[0], src[1], src[2]);
    let alpha_s = src[3] as f32;
    let (parts, explicit_alpha) = split_balanced_args(rest);
    if parts.len() < 3 { return None; }
    let new_h = eval_color_component(parts[0], &[("h", h), ("s", sat * 100.0), ("l", lit * 100.0), ("alpha", alpha_s)], 360.0)?;
    let new_s = eval_color_component(parts[1], &[("h", h), ("s", sat * 100.0), ("l", lit * 100.0), ("alpha", alpha_s)], 100.0)?;
    let new_l = eval_color_component(parts[2], &[("h", h), ("s", sat * 100.0), ("l", lit * 100.0), ("alpha", alpha_s)], 100.0)?;
    let (r, g, b) = hsl_to_rgb(new_h, new_s / 100.0, new_l / 100.0);
    let a = if let Some(byte) = explicit_alpha { byte } else { src[3] };
    Some([r, g, b, a])
}

/// Vyhodnoti slozku barvy v relative color: cislo, "none", procento,
/// keyword (r/g/b/h/s/l/alpha), calc(<keyword> * 0.5).
fn eval_color_component(s: &str, vars: &[(&str, f32)], scale: f32) -> Option<f32> {
    let s = s.trim();
    if s == "none" { return Some(0.0); }
    // Keyword substituce
    for (name, val) in vars {
        if s == *name { return Some(*val); }
    }
    // calc(...) - jednoduchy: keyword * num nebo num * keyword
    if let Some(inner) = s.strip_prefix("calc(").and_then(|x| x.strip_suffix(')')) {
        return eval_simple_calc(inner.trim(), vars);
    }
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.trim().parse().ok()?;
        return Some(v / 100.0 * scale);
    }
    s.parse::<f32>().ok()
}

/// Mini-calc pro relative color: <a> <op> <b> kde a/b muze byt keyword nebo cislo.
fn eval_simple_calc(s: &str, vars: &[(&str, f32)]) -> Option<f32> {
    for op in ['+', '-', '*', '/'] {
        if let Some(idx) = s.rfind(op) {
            let (l, r) = s.split_at(idx);
            let r = &r[1..];
            let lv = eval_color_component(l.trim(), vars, 255.0)?;
            let rv = eval_color_component(r.trim(), vars, 255.0)?;
            return Some(match op {
                '+' => lv + rv, '-' => lv - rv,
                '*' => lv * rv, '/' => lv / rv,
                _ => return None,
            });
        }
    }
    eval_color_component(s, vars, 255.0)
}

/// Split argumentu pro relative color - respektuje zavorky (calc(...)).
/// Vrati (positional, optional_alpha_byte). Splituje na top-level mezerach.
fn split_balanced_args(inner: &str) -> (Vec<&str>, Option<u8>) {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut alpha_split: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'/' if depth == 0 => { alpha_split = Some(i); break; }
            b' ' | b'\t' if depth == 0 => {
                if start < i { parts.push(inner[start..i].trim()); }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let main_end = alpha_split.unwrap_or(inner.len());
    if start < main_end { parts.push(inner[start..main_end].trim()); }
    parts.retain(|p| !p.is_empty());
    let alpha = alpha_split.and_then(|idx| parse_alpha(inner[idx+1..].trim()));
    (parts, alpha)
}

/// Vrati (color_token, zbytek) - color je prvni token (mozna funkcni s zavorkami).
fn split_first_color_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ' ' | '\t' if depth == 0 => return Some((&s[..i], s[i..].trim())),
            _ => {}
        }
    }
    Some((s, ""))
}

/// RGB -> HSL: H 0..360, S 0..1, L 0..1.
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == rf { ((gf - bf) / d) + (if gf < bf { 6.0 } else { 0.0 }) }
            else if max == gf { ((bf - rf) / d) + 2.0 }
            else { ((rf - gf) / d) + 4.0 };
    (h * 60.0, s, l)
}

fn parse_angle_deg(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("deg") { v.trim().parse().ok() }
    else if let Some(v) = s.strip_suffix("rad") {
        v.trim().parse::<f32>().ok().map(|r| r.to_degrees())
    }
    else if let Some(v) = s.strip_suffix("turn") {
        v.trim().parse::<f32>().ok().map(|t| t * 360.0)
    }
    else { s.parse().ok() }
}

fn parse_percent_or_num(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f32>().ok().map(|v| v / 100.0) }
    else { s.parse().ok() }
}

/// Pro Lab/Oklab L: percent -> scale (lab je 0..100, oklab 0..1).
fn parse_percent_or_num_scaled(s: &str, scale: f32) -> Option<f32> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f32>().ok().map(|v| v / 100.0 * scale) }
    else { s.parse().ok() }
}

/// HSL -> RGB. h v stupnich, s/l v 0..1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0 / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

/// HWB -> RGB. h v stupnich, w/b v 0..1.
fn hwb_to_rgb(h: f32, w: f32, b: f32) -> (u8, u8, u8) {
    let mut w = w.clamp(0.0, 1.0);
    let mut b = b.clamp(0.0, 1.0);
    if w + b > 1.0 {
        let sum = w + b;
        w /= sum;
        b /= sum;
    }
    let (r, g, bl) = hsl_to_rgb(h, 1.0, 0.5);
    let f = |v: u8| {
        let x = v as f32 / 255.0;
        let out = x * (1.0 - w - b) + w;
        (out * 255.0).clamp(0.0, 255.0).round() as u8
    };
    (f(r), f(g), f(bl))
}

/// OkLab -> sRGB (Bjorn Ottosson algoritmus).
fn oklab_to_rgb(l: f32, a: f32, b: f32) -> (u8, u8, u8) {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    let r =  4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let b_ = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;
    (linear_to_srgb_u8(r), linear_to_srgb_u8(g), linear_to_srgb_u8(b_))
}

/// CIE Lab -> sRGB. Vstup: L 0..100, a/b ~ -128..127.
fn lab_to_rgb(l: f32, a: f32, b: f32) -> (u8, u8, u8) {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let f_inv = |t: f32| {
        let d = 6.0 / 29.0;
        if t > d { t * t * t } else { 3.0 * d * d * (t - 4.0 / 29.0) }
    };
    // D65 illuminant
    let xn = 0.95047; let yn = 1.0; let zn = 1.08883;
    let x = xn * f_inv(fx);
    let y = yn * f_inv(fy);
    let z = zn * f_inv(fz);
    // XYZ -> linear sRGB (D65)
    let r =  3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let g = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
    let b_ = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
    (linear_to_srgb_u8(r), linear_to_srgb_u8(g), linear_to_srgb_u8(b_))
}

/// Linear -> sRGB gamma encoding + clamp + round to u8.
fn linear_to_srgb_u8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).clamp(0.0, 255.0).round() as u8
}

fn srgb_u8_to_linear(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// sRGB -> OkLab.
fn rgb_to_oklab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = srgb_u8_to_linear(r);
    let g = srgb_u8_to_linear(g);
    let b = srgb_u8_to_linear(b);
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    (
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    )
}

/// color-mix(in <space>, color1 [<%>], color2 [<%>]).
/// Podpora space: srgb, oklab, oklch, lab, lch, hsl, hwb.
/// CSS Color L5 contrast() function - vyhrava pres comparing colory s background.
/// Format: contrast(<bg> vs <c1>, <c2>, ...) - vrati color s nejvyssim contrast vs bg.
/// Pouziva relative luminance.
fn parse_contrast(inner: &str) -> Option<[u8; 4]> {
    let trimmed = inner.trim();
    let (bg_str, candidates_str) = if let Some(idx) = trimmed.find(" vs ") {
        (&trimmed[..idx], &trimmed[idx+4..])
    } else {
        // Forma "contrast(bg)" -> vrati black/white podle bg luma
        let bg = parse_color(trimmed)?;
        let luma = (0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32) / 255.0;
        return Some(if luma > 0.5 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
    };
    let bg = parse_color(bg_str.trim())?;
    let bg_luma = (0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32) / 255.0;
    let mut best: Option<[u8; 4]> = None;
    let mut best_contrast = -1.0_f32;
    for cand_str in candidates_str.split(',') {
        if let Some(c) = parse_color(cand_str.trim()) {
            let cl = (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0;
            let l1 = bg_luma.max(cl);
            let l2 = bg_luma.min(cl);
            let contrast = (l1 + 0.05) / (l2 + 0.05);
            if contrast > best_contrast {
                best_contrast = contrast;
                best = Some(c);
            }
        }
    }
    best
}

fn parse_color_mix(inner: &str) -> Option<[u8; 4]> {
    // Format: "in <space>, c1 [<%>], c2 [<%>]"
    let parts = split_top_level_commas(inner);
    if parts.len() < 3 { return None; }
    let in_clause = parts[0].trim();
    let space = in_clause.strip_prefix("in ")?.trim().to_lowercase();
    // Cisla mohou byt napr. "red 30%" / "red"
    let parse_color_pct = |s: &str| -> Option<([u8; 4], f32)> {
        let s = s.trim();
        // Najdi posledni "<num>%" - znaci vahu
        if let Some(pct_idx) = s.rfind('%') {
            let space_idx = s[..pct_idx].rfind(char::is_whitespace);
            if let Some(si) = space_idx {
                let pct: f32 = s[si..pct_idx].trim().parse().ok()?;
                let col = parse_color(s[..si].trim())?;
                return Some((col, pct / 100.0));
            }
        }
        let col = parse_color(s)?;
        Some((col, -1.0)) // signal: nezadana vaha
    };
    let (c1, p1) = parse_color_pct(&parts[1])?;
    let (c2, p2) = parse_color_pct(&parts[2])?;

    // Normalize percent vah
    let (w1, w2) = match (p1, p2) {
        (-1.0, -1.0) => (0.5, 0.5),
        (a, -1.0)    => (a, 1.0 - a),
        (-1.0, b)    => (1.0 - b, b),
        (a, b) => {
            let sum = a + b;
            if sum > 0.0 { (a / sum, b / sum) } else { (0.5, 0.5) }
        }
    };

    // Mix dle prostoru
    let (r, g, b) = match space.as_str() {
        "srgb" => (
            (c1[0] as f32 * w1 + c2[0] as f32 * w2).round() as u8,
            (c1[1] as f32 * w1 + c2[1] as f32 * w2).round() as u8,
            (c1[2] as f32 * w1 + c2[2] as f32 * w2).round() as u8,
        ),
        "oklab" | "oklch" => {
            let (l1, a1, b1) = rgb_to_oklab(c1[0], c1[1], c1[2]);
            let (l2, a2, b2) = rgb_to_oklab(c2[0], c2[1], c2[2]);
            let l = l1 * w1 + l2 * w2;
            let a = a1 * w1 + a2 * w2;
            let b = b1 * w1 + b2 * w2;
            oklab_to_rgb(l, a, b)
        }
        _ => {
            // Fallback: srgb
            (
                (c1[0] as f32 * w1 + c2[0] as f32 * w2).round() as u8,
                (c1[1] as f32 * w1 + c2[1] as f32 * w2).round() as u8,
                (c1[2] as f32 * w1 + c2[2] as f32 * w2).round() as u8,
            )
        }
    };
    let alpha = (c1[3] as f32 * w1 + c2[3] as f32 * w2).round() as u8;
    Some([r, g, b, alpha])
}
