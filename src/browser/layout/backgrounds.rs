//! Background layer types (gradient, color, position, size, repeat, box, attachment),
//! clip-path enum + parsing, to_roman counter helper.

use super::parse_length;

/// Vsechny varianty gradientu pro background-image.
#[derive(Debug, Clone)]
pub struct BgGradient {
    pub kind: BgGradientKind,
    pub stops: Vec<(f32, [u8; 4])>,
}

#[derive(Debug, Clone)]
pub enum BgGradientKind {
    Linear { angle_deg: f32 },
    /// cx/cy/radius v procentech (0..1). radius_pct = polomer relativni k farthest-corner.
    Radial { cx_pct: f32, cy_pct: f32, radius_pct: f32 },
    /// start_angle_deg: poradi od 12 hod. cx/cy v procentech.
    Conic { cx_pct: f32, cy_pct: f32, start_angle_deg: f32 },
}

/// CSS clip-path L1 - tvar pro clipping elementu.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipPath {
    /// inset(top right bottom left [round <radius>])
    Inset { top: f32, right: f32, bottom: f32, left: f32, radius: f32 },
    /// circle(<r> [at <pos>])
    Circle { cx_pct: f32, cy_pct: f32, radius_pct: f32 },
    /// ellipse(<rx> <ry> [at <pos>])
    Ellipse { cx_pct: f32, cy_pct: f32, rx_pct: f32, ry_pct: f32 },
    /// polygon(p1, p2, ...) - kazdy bod (x_pct, y_pct).
    Polygon(Vec<(f32, f32)>),
}

pub fn parse_clip_path(s: &str) -> Option<ClipPath> {
    let s = s.trim();
    if s == "none" || s.is_empty() { return None; }
    if let Some(args) = s.strip_prefix("inset(").and_then(|s| s.strip_suffix(')')) {
        return parse_inset_clip(args);
    }
    if let Some(args) = s.strip_prefix("circle(").and_then(|s| s.strip_suffix(')')) {
        return parse_circle_clip(args);
    }
    if let Some(args) = s.strip_prefix("ellipse(").and_then(|s| s.strip_suffix(')')) {
        return parse_ellipse_clip(args);
    }
    if let Some(args) = s.strip_prefix("polygon(").and_then(|s| s.strip_suffix(')')) {
        return parse_polygon_clip(args);
    }
    None
}

fn parse_inset_clip(args: &str) -> Option<ClipPath> {
    let mut radius = 0.0;
    let main = if let Some((before, r)) = args.split_once("round ") {
        radius = parse_length(r.trim());
        before.trim()
    } else {
        args.trim()
    };
    let parts: Vec<&str> = main.split_whitespace().collect();
    let (t, r, b, l) = match parts.len() {
        1 => (parts[0], parts[0], parts[0], parts[0]),
        2 => (parts[0], parts[1], parts[0], parts[1]),
        3 => (parts[0], parts[1], parts[2], parts[1]),
        4 => (parts[0], parts[1], parts[2], parts[3]),
        _ => return None,
    };
    Some(ClipPath::Inset {
        top: parse_length(t),
        right: parse_length(r),
        bottom: parse_length(b),
        left: parse_length(l),
        radius,
    })
}

fn parse_circle_clip(args: &str) -> Option<ClipPath> {
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let mut radius_pct: f32 = 0.5;
    let (before_at, after_at) = args.split_once(" at ")
        .map(|(a, b)| (a.trim(), Some(b.trim())))
        .unwrap_or((args.trim(), None));
    if !before_at.is_empty() {
        if let Some(p) = before_at.strip_suffix('%') {
            radius_pct = p.parse::<f32>().ok()? / 100.0;
        }
    }
    if let Some(pos) = after_at {
        let pos_parts: Vec<&str> = pos.split_whitespace().collect();
        let kw = |t: &str| -> Option<f32> {
            match t {
                "left" | "top" => Some(0.0),
                "center" => Some(0.5),
                "right" | "bottom" => Some(1.0),
                s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                _ => None,
            }
        };
        if let Some(v) = pos_parts.first().and_then(|t| kw(t)) { cx_pct = v; }
        if let Some(v) = pos_parts.get(1).and_then(|t| kw(t)) { cy_pct = v; }
    }
    Some(ClipPath::Circle { cx_pct, cy_pct, radius_pct })
}

fn parse_ellipse_clip(args: &str) -> Option<ClipPath> {
    let mut cx_pct: f32 = 0.5;
    let mut cy_pct: f32 = 0.5;
    let mut rx_pct: f32 = 0.5;
    let mut ry_pct: f32 = 0.5;
    let (before_at, after_at) = args.split_once(" at ")
        .map(|(a, b)| (a.trim(), Some(b.trim())))
        .unwrap_or((args.trim(), None));
    let parts: Vec<&str> = before_at.split_whitespace().collect();
    if let Some(p) = parts.first().and_then(|t| t.strip_suffix('%'))
        .and_then(|p| p.parse::<f32>().ok()) { rx_pct = p / 100.0; }
    if let Some(p) = parts.get(1).and_then(|t| t.strip_suffix('%'))
        .and_then(|p| p.parse::<f32>().ok()) { ry_pct = p / 100.0; }
    if let Some(pos) = after_at {
        let pp: Vec<&str> = pos.split_whitespace().collect();
        let kw = |t: &str| -> Option<f32> {
            match t {
                "left" | "top" => Some(0.0),
                "center" => Some(0.5),
                "right" | "bottom" => Some(1.0),
                s if s.ends_with('%') => s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0),
                _ => None,
            }
        };
        if let Some(v) = pp.first().and_then(|t| kw(t)) { cx_pct = v; }
        if let Some(v) = pp.get(1).and_then(|t| kw(t)) { cy_pct = v; }
    }
    Some(ClipPath::Ellipse { cx_pct, cy_pct, rx_pct, ry_pct })
}

fn parse_polygon_clip(args: &str) -> Option<ClipPath> {
    // "0 0, 100% 0, 50% 100%"
    let mut points = Vec::new();
    for pair in args.split(',') {
        let parts: Vec<&str> = pair.split_whitespace().collect();
        if parts.len() < 2 { continue; }
        let x = parse_pct_or_px(parts[0]);
        let y = parse_pct_or_px(parts[1]);
        points.push((x, y));
    }
    if points.is_empty() { None } else { Some(ClipPath::Polygon(points)) }
}

/// Konvertuje cislo na rimske cislice (1..3999).
pub fn to_roman(n: i32) -> String {
    let pairs: &[(i32, &str)] = &[
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100,  "C"), (90,  "XC"), (50,  "L"), (40,  "XL"),
        (10,   "X"), (9,   "IX"), (5,   "V"), (4,   "IV"),
        (1,    "I"),
    ];
    let mut out = String::new();
    let mut n = n.max(0);
    for (val, sym) in pairs {
        while n >= *val {
            out.push_str(sym);
            n -= val;
        }
    }
    out
}

fn parse_pct_or_px(s: &str) -> f32 {
    if let Some(p) = s.strip_suffix('%') { p.parse::<f32>().unwrap_or(0.0) / 100.0 }
    else { parse_length(s) / 100.0 } // approximace - px za 100 = 1.0
}

/// CSS Backgrounds L3 - jedna vrstva.
#[derive(Debug, Clone, Default)]
pub struct BgLayer {
    pub color: Option<[u8; 4]>,
    pub image_src: Option<String>,
    pub gradient: Option<BgGradient>,
    pub position: BgPosition,
    pub size: BgSize,
    pub repeat: BgRepeat,
    pub clip: BgBox,
    pub origin: BgBox,
    pub attachment: BgAttachment,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgPosition {
    /// Px / % offset od top-left
    Px(f32, f32),
    /// Procenta (0..1)
    Pct(f32, f32),
    /// Mix - x px, y %
    Mixed { x_px: Option<f32>, x_pct: Option<f32>, y_px: Option<f32>, y_pct: Option<f32> },
}
impl Default for BgPosition {
    fn default() -> Self { BgPosition::Pct(0.0, 0.0) }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgSize {
    /// Auto - puvodni image rozmer
    Auto,
    Cover,
    Contain,
    /// Lengths: (w, h) - None = auto
    Length { w: Option<f32>, h: Option<f32> },
    /// Procenta vuci kontejneru
    Pct { w: Option<f32>, h: Option<f32> },
}
impl Default for BgSize {
    fn default() -> Self { BgSize::Auto }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgRepeat {
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
    Space,
    Round,
}
impl Default for BgRepeat {
    fn default() -> Self { BgRepeat::Repeat }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgBox {
    BorderBox,
    PaddingBox,
    ContentBox,
    /// background-clip: text - bg renderovan jen pres glyfy textu.
    Text,
}
impl Default for BgBox {
    fn default() -> Self { BgBox::BorderBox }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgAttachment {
    Scroll,
    Fixed,
    Local,
}
impl Default for BgAttachment {
    fn default() -> Self { BgAttachment::Scroll }
}

/// Parsuje background-position: "left", "center", "50%", "10px 20px", "right top".
pub fn parse_bg_position(s: &str) -> BgPosition {
    let parts: Vec<&str> = s.split_whitespace().collect();
    let kw_pct = |kw: &str| -> Option<f32> {
        match kw {
            "left" | "top" => Some(0.0),
            "center" => Some(0.5),
            "right" | "bottom" => Some(1.0),
            _ => None,
        }
    };
    let parse_axis = |t: &str| -> (Option<f32>, Option<f32>) {
        // (px, pct)
        if let Some(p) = kw_pct(t) { return (None, Some(p)); }
        if let Some(p) = t.strip_suffix('%') {
            return (None, p.parse::<f32>().ok().map(|v| v / 100.0));
        }
        let v = parse_length(t);
        (Some(v), None)
    };
    match parts.len() {
        0 => BgPosition::default(),
        1 => {
            let (px, pct) = parse_axis(parts[0]);
            BgPosition::Mixed { x_px: px, x_pct: pct, y_px: None, y_pct: Some(0.5) }
        }
        _ => {
            let (xpx, xpct) = parse_axis(parts[0]);
            let (ypx, ypct) = parse_axis(parts[1]);
            BgPosition::Mixed { x_px: xpx, x_pct: xpct, y_px: ypx, y_pct: ypct }
        }
    }
}

/// Parsuje background-size: "auto", "cover", "contain", "100px 200px", "50% auto".
pub fn parse_bg_size(s: &str) -> BgSize {
    let s = s.trim();
    match s {
        "cover" => return BgSize::Cover,
        "contain" => return BgSize::Contain,
        "auto" | "" => return BgSize::Auto,
        _ => {}
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    let parse_one = |t: &str| -> Option<f32> {
        if t == "auto" { return None; }
        if let Some(p) = t.strip_suffix('%') {
            return p.parse::<f32>().ok().map(|v| v / 100.0);
        }
        Some(parse_length(t))
    };
    if parts.len() == 1 {
        // pct path
        if parts[0].ends_with('%') {
            return BgSize::Pct { w: parse_one(parts[0]), h: None };
        }
        return BgSize::Length { w: parse_one(parts[0]), h: None };
    }
    let w = parse_one(parts[0]);
    let h = parse_one(parts[1]);
    if parts[0].ends_with('%') || parts[1].ends_with('%') {
        BgSize::Pct { w, h }
    } else {
        BgSize::Length { w, h }
    }
}

pub fn parse_bg_repeat(s: &str) -> BgRepeat {
    match s.trim() {
        "no-repeat" => BgRepeat::NoRepeat,
        "repeat-x"  => BgRepeat::RepeatX,
        "repeat-y"  => BgRepeat::RepeatY,
        "space"     => BgRepeat::Space,
        "round"     => BgRepeat::Round,
        _ => BgRepeat::Repeat,
    }
}

pub fn parse_bg_box(s: &str) -> BgBox {
    match s.trim() {
        "padding-box" => BgBox::PaddingBox,
        "content-box" => BgBox::ContentBox,
        "text"        => BgBox::Text,
        _ => BgBox::BorderBox,
    }
}

pub fn parse_bg_attachment(s: &str) -> BgAttachment {
    match s.trim() {
        "fixed" => BgAttachment::Fixed,
        "local" => BgAttachment::Local,
        _ => BgAttachment::Scroll,
    }
}
