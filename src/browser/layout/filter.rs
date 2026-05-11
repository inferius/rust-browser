//! CSS filter chain parsing + apply + color matrix compose.

use super::{parse_color, parse_length, split_top_level_whitespace_str};

/// CSS Filter Effects L1 - jednotliva operace.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Blur(f32),                   // pixelu
    Brightness(f32),             // 0..1+
    Contrast(f32),               // 0..1+
    Grayscale(f32),              // 0..1
    HueRotate(f32),              // ve stupnich
    Invert(f32),                 // 0..1
    Saturate(f32),               // 0..1+
    Sepia(f32),                  // 0..1
    Opacity(f32),                // 0..1
    /// drop-shadow(offset_x offset_y blur color)
    DropShadow { ox: f32, oy: f32, blur: f32, color: [u8; 4] },
}

/// Parsuje filter / backdrop-filter property na chain operaci.
/// "filter: blur(2px) brightness(1.2) hue-rotate(45deg)" -> 3 ops.
pub fn parse_filter_chain(s: &str) -> Vec<FilterOp> {
    let s = s.trim();
    if s.is_empty() || s == "none" { return Vec::new(); }
    let mut out = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some(&(start, _)) = chars.peek() {
        // Skip whitespace
        while let Some(&(_, c)) = chars.peek() {
            if c.is_whitespace() { chars.next(); } else { break; }
        }
        let start_idx = match chars.peek() { Some(&(i, _)) => i, None => break };
        // Read function name do `(`
        let mut name_end = start_idx;
        while let Some(&(i, c)) = chars.peek() {
            if c == '(' { name_end = i; chars.next(); break; }
            if c.is_whitespace() {
                // Mozna keyword bez argumentu - skip
                break;
            }
            chars.next();
            name_end = i + c.len_utf8();
        }
        let name = &s[start_idx..name_end];
        if name.is_empty() { break; }
        // Read args do `)` - respektovat nesteni
        let arg_start = match chars.peek() { Some(&(i, _)) => i, None => break };
        let _ = start;
        let mut depth = 1;
        let mut arg_end = arg_start;
        while let Some(&(i, c)) = chars.peek() {
            arg_end = i;
            if c == '(' { depth += 1; }
            if c == ')' { depth -= 1; if depth == 0 { chars.next(); break; } }
            chars.next();
        }
        let args = &s[arg_start..arg_end];
        if let Some(op) = parse_filter_one(name, args) {
            out.push(op);
        }
    }
    out
}

fn parse_filter_one(name: &str, args: &str) -> Option<FilterOp> {
    let args = args.trim();
    let parse_pct_or_num = |s: &str| -> Option<f32> {
        let s = s.trim();
        if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f32>().ok().map(|v| v / 100.0) }
        else { s.parse().ok() }
    };
    Some(match name {
        "blur" => FilterOp::Blur(parse_length(args)),
        "brightness" => FilterOp::Brightness(parse_pct_or_num(args)?),
        "contrast"   => FilterOp::Contrast(parse_pct_or_num(args)?),
        "grayscale"  => FilterOp::Grayscale(parse_pct_or_num(args)?),
        "hue-rotate" => {
            // deg / rad / turn
            if let Some(v) = args.strip_suffix("deg") { FilterOp::HueRotate(v.trim().parse().ok()?) }
            else if let Some(v) = args.strip_suffix("rad") {
                FilterOp::HueRotate(v.trim().parse::<f32>().ok()?.to_degrees())
            }
            else if let Some(v) = args.strip_suffix("turn") {
                FilterOp::HueRotate(v.trim().parse::<f32>().ok()? * 360.0)
            }
            else { FilterOp::HueRotate(args.parse().ok()?) }
        }
        "invert"   => FilterOp::Invert(parse_pct_or_num(args)?),
        "saturate" => FilterOp::Saturate(parse_pct_or_num(args)?),
        "sepia"    => FilterOp::Sepia(parse_pct_or_num(args)?),
        "opacity"  => FilterOp::Opacity(parse_pct_or_num(args)?),
        "drop-shadow" => {
            // "<ox> <oy> [<blur>] <color>"
            let parts: Vec<&str> = split_top_level_whitespace_str(args);
            if parts.len() < 3 { return None; }
            let ox = parse_length(parts[0]);
            let oy = parse_length(parts[1]);
            let mut blur = 0.0f32;
            let mut color_idx = 2;
            if parts[2].chars().next().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false)
                || parts[2].ends_with("px") || parts[2].ends_with("em")
            {
                blur = parse_length(parts[2]);
                color_idx = 3;
            }
            if color_idx >= parts.len() { return None; }
            let rest: String = parts[color_idx..].join(" ");
            let color = parse_color(&rest)?;
            FilterOp::DropShadow { ox, oy, blur, color }
        }
        _ => return None,
    })
}

/// Aplikuje filter chain na barvu (CPU). Kazdy filter modifikuje RGBA postupne.
/// Implementace: brightness/contrast/grayscale/sepia/invert/saturate/hue-rotate/opacity.
/// Blur a drop-shadow vyzaduji multi-pass - tady ignorovany (ne-color manipulation).
pub fn apply_filter_chain(rgba: [u8; 4], chain: &[FilterOp]) -> [u8; 4] {
    if chain.is_empty() { return rgba; }
    let mut r = rgba[0] as f32 / 255.0;
    let mut g = rgba[1] as f32 / 255.0;
    let mut b = rgba[2] as f32 / 255.0;
    let mut a = rgba[3] as f32 / 255.0;

    for op in chain {
        match *op {
            FilterOp::Brightness(v) => {
                r *= v; g *= v; b *= v;
            }
            FilterOp::Contrast(c) => {
                r = (r - 0.5) * c + 0.5;
                g = (g - 0.5) * c + 0.5;
                b = (b - 0.5) * c + 0.5;
            }
            FilterOp::Grayscale(amount) => {
                let amt = amount.clamp(0.0, 1.0);
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                r = r * (1.0 - amt) + lum * amt;
                g = g * (1.0 - amt) + lum * amt;
                b = b * (1.0 - amt) + lum * amt;
            }
            FilterOp::Sepia(amount) => {
                let amt = amount.clamp(0.0, 1.0);
                let nr = 0.393 * r + 0.769 * g + 0.189 * b;
                let ng = 0.349 * r + 0.686 * g + 0.168 * b;
                let nb = 0.272 * r + 0.534 * g + 0.131 * b;
                r = r * (1.0 - amt) + nr * amt;
                g = g * (1.0 - amt) + ng * amt;
                b = b * (1.0 - amt) + nb * amt;
            }
            FilterOp::Invert(amount) => {
                let amt = amount.clamp(0.0, 1.0);
                r = r * (1.0 - amt) + (1.0 - r) * amt;
                g = g * (1.0 - amt) + (1.0 - g) * amt;
                b = b * (1.0 - amt) + (1.0 - b) * amt;
            }
            FilterOp::Saturate(s) => {
                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                r = lum + (r - lum) * s;
                g = lum + (g - lum) * s;
                b = lum + (b - lum) * s;
            }
            FilterOp::HueRotate(deg) => {
                let rad = deg.to_radians();
                let cos = rad.cos();
                let sin = rad.sin();
                // Standardni hue rotation matrix (NTSC luminance basis)
                let lr = 0.213; let lg = 0.715; let lb = 0.072;
                let m = [
                    lr + cos*(1.0-lr) + sin*(-lr),     lg + cos*(-lg) + sin*(-lg),       lb + cos*(-lb) + sin*(1.0-lb),
                    lr + cos*(-lr) + sin*(0.143),      lg + cos*(1.0-lg) + sin*(0.140),  lb + cos*(-lb) + sin*(-0.283),
                    lr + cos*(-lr) + sin*(-(1.0-lr)),  lg + cos*(-lg) + sin*(lg),        lb + cos*(1.0-lb) + sin*(lb),
                ];
                let nr = m[0]*r + m[1]*g + m[2]*b;
                let ng = m[3]*r + m[4]*g + m[5]*b;
                let nb = m[6]*r + m[7]*g + m[8]*b;
                r = nr; g = ng; b = nb;
            }
            FilterOp::Opacity(o) => {
                a *= o.clamp(0.0, 1.0);
            }
            FilterOp::Blur(_) | FilterOp::DropShadow { .. } => {
                // Multi-pass / shape effects - aktualne ignorujem (TODO render-to-texture)
            }
        }
    }
    [
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
        (a.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}


/// Vypocita 4x5 row-major color matrix z filter chain.
/// Format: vystup[i] = sum_j(m[i*5 + j] * input[j]) + m[i*5 + 4]; j=0..3 (rgba).
/// Vraci identity (1,0,0,0,0; 0,1,0,0,0; 0,0,1,0,0; 0,0,0,1,0) pro prazdny chain.
/// Blur a DropShadow se ignoruji (jine fazy pipeline).
pub fn compute_color_matrix(chain: &[FilterOp]) -> [f32; 20] {
    // PERF: fast-path pro prazdny filter chain (99% elementu). Return identity
    // bez compose loop. Drive vola paint_box 2× per element (filter+backdrop).
    const IDENTITY: [f32; 20] = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    if chain.is_empty() { return IDENTITY; }
    let mut m: [f32; 20] = IDENTITY;
    // Compose: new = filter_matrix * current
    let compose = |m: &mut [f32; 20], f: [f32; 20]| {
        let mut r = [0.0_f32; 20];
        for row in 0..4 {
            for col in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += f[row * 5 + k] * m[k * 5 + col];
                }
                r[row * 5 + col] = s;
            }
            // Offset: f[row*5+4] + sum_k(f[row*5+k] * m[k*5+4])
            let mut s = f[row * 5 + 4];
            for k in 0..4 {
                s += f[row * 5 + k] * m[k * 5 + 4];
            }
            r[row * 5 + 4] = s;
        }
        *m = r;
    };
    let identity = || -> [f32; 20] {
        [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    };
    for op in chain {
        match op {
            FilterOp::Brightness(v) => {
                let b = *v;
                let mut f = identity();
                f[0] = b; f[6] = b; f[12] = b;
                compose(&mut m, f);
            }
            FilterOp::Contrast(c) => {
                let off = 0.5 * (1.0 - c);
                let mut f = identity();
                f[0] = *c; f[4] = off;
                f[6] = *c; f[9] = off;
                f[12] = *c; f[14] = off;
                compose(&mut m, f);
            }
            FilterOp::Invert(i) => {
                let coef = 1.0 - 2.0 * i;
                let mut f = identity();
                f[0] = coef; f[4] = *i;
                f[6] = coef; f[9] = *i;
                f[12] = coef; f[14] = *i;
                compose(&mut m, f);
            }
            FilterOp::Grayscale(amount) => {
                let a = *amount;
                let inv = 1.0 - a;
                // SVG luminance coeffs 0.2126/0.7152/0.0722
                let lr = 0.2126_f32; let lg = 0.7152_f32; let lb = 0.0722_f32;
                let f: [f32; 20] = [
                    inv + a*lr, a*lg,       a*lb,       0.0, 0.0,
                    a*lr,       inv + a*lg, a*lb,       0.0, 0.0,
                    a*lr,       a*lg,       inv + a*lb, 0.0, 0.0,
                    0.0,        0.0,        0.0,        1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::Sepia(amount) => {
                let a = *amount;
                let inv = 1.0 - a;
                // Sepia coeffs from W3C
                let f: [f32; 20] = [
                    inv + a*0.393, a*0.769,       a*0.189,       0.0, 0.0,
                    a*0.349,       inv + a*0.686, a*0.168,       0.0, 0.0,
                    a*0.272,       a*0.534,       inv + a*0.131, 0.0, 0.0,
                    0.0,           0.0,           0.0,           1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::Saturate(s) => {
                // Standard SVG saturate matrix
                let inv = 1.0 - s;
                let lr = 0.213_f32; let lg = 0.715_f32; let lb = 0.072_f32;
                let f: [f32; 20] = [
                    lr*inv + s, lg*inv,     lb*inv,     0.0, 0.0,
                    lr*inv,     lg*inv + s, lb*inv,     0.0, 0.0,
                    lr*inv,     lg*inv,     lb*inv + s, 0.0, 0.0,
                    0.0,        0.0,        0.0,        1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::HueRotate(deg) => {
                let rad = deg.to_radians();
                let c = rad.cos();
                let s = rad.sin();
                // SVG hue-rotate matrix (luma-preserving)
                let f: [f32; 20] = [
                    0.213 + c*0.787  + s*-0.213, 0.715 + c*-0.715 + s*-0.715, 0.072 + c*-0.072 + s*0.928,  0.0, 0.0,
                    0.213 + c*-0.213 + s*0.143,  0.715 + c*0.285  + s*0.140,  0.072 + c*-0.072 + s*-0.283, 0.0, 0.0,
                    0.213 + c*-0.213 + s*-0.787, 0.715 + c*-0.715 + s*0.715,  0.072 + c*0.928  + s*0.072,  0.0, 0.0,
                    0.0,                         0.0,                         0.0,                         1.0, 0.0,
                ];
                compose(&mut m, f);
            }
            FilterOp::Opacity(o) => {
                let mut f = identity();
                f[18] = *o;
                compose(&mut m, f);
            }
            FilterOp::Blur(_) | FilterOp::DropShadow { .. } => {
                // Jine faze pipeline - skip
            }
        }
    }
    m
}

/// True pokud color matrix neni identity (do epsilonu).
pub fn is_identity_matrix(m: &[f32; 20]) -> bool {
    let id: [f32; 20] = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    for i in 0..20 {
        if (m[i] - id[i]).abs() > 1e-4 { return false; }
    }
    true
}
