//! mix-blend-mode + backdrop-filter compositor support.
//!
//! Spec: https://www.w3.org/TR/compositing-1/
//! 16 blend modes (normal, multiply, screen, overlay, darken, lighten, color-dodge,
//! color-burn, hard-light, soft-light, difference, exclusion, hue, saturation,
//! color, luminosity).
//!
//! Implementation in shader (WGSL): per-pixel blend(src, dst).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusLighter,         // Apple/CSS Color Level 4
}

impl BlendMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "color-dodge" => Self::ColorDodge,
            "color-burn" => Self::ColorBurn,
            "hard-light" => Self::HardLight,
            "soft-light" => Self::SoftLight,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "hue" => Self::Hue,
            "saturation" => Self::Saturation,
            "color" => Self::Color,
            "luminosity" => Self::Luminosity,
            "plus-lighter" => Self::PlusLighter,
            _ => Self::Normal,
        }
    }

    pub fn shader_id(&self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
            Self::Screen => 2,
            Self::Overlay => 3,
            Self::Darken => 4,
            Self::Lighten => 5,
            Self::ColorDodge => 6,
            Self::ColorBurn => 7,
            Self::HardLight => 8,
            Self::SoftLight => 9,
            Self::Difference => 10,
            Self::Exclusion => 11,
            Self::Hue => 12,
            Self::Saturation => 13,
            Self::Color => 14,
            Self::Luminosity => 15,
            Self::PlusLighter => 16,
        }
    }
}

/// CPU reference implementation. Returns blended RGB (each in [0,1]).
/// alpha pak combine pres porter-duff normal.
pub fn blend_rgb(mode: BlendMode, src: (f32, f32, f32), dst: (f32, f32, f32)) -> (f32, f32, f32) {
    fn each(mode: BlendMode, s: f32, d: f32) -> f32 {
        match mode {
            BlendMode::Multiply => s * d,
            BlendMode::Screen => 1.0 - (1.0 - s) * (1.0 - d),
            BlendMode::Overlay => if d < 0.5 { 2.0 * s * d } else { 1.0 - 2.0 * (1.0 - s) * (1.0 - d) },
            BlendMode::Darken => s.min(d),
            BlendMode::Lighten => s.max(d),
            BlendMode::ColorDodge => if s >= 1.0 { 1.0 } else { (d / (1.0 - s)).min(1.0) },
            BlendMode::ColorBurn => if s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - d) / s).min(1.0) },
            BlendMode::HardLight => if s < 0.5 { 2.0 * s * d } else { 1.0 - 2.0 * (1.0 - s) * (1.0 - d) },
            BlendMode::SoftLight => {
                if s < 0.5 { d - (1.0 - 2.0 * s) * d * (1.0 - d) }
                else {
                    let g = if d <= 0.25 { ((16.0 * d - 12.0) * d + 4.0) * d } else { d.sqrt() };
                    d + (2.0 * s - 1.0) * (g - d)
                }
            }
            BlendMode::Difference => (s - d).abs(),
            BlendMode::Exclusion => s + d - 2.0 * s * d,
            BlendMode::PlusLighter => (s + d).min(1.0),
            _ => s, // Hue/Sat/Color/Luminosity nelze udelat per-kanal
        }
    }
    if matches!(mode, BlendMode::Normal) { return src; }
    if matches!(mode, BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity) {
        return blend_hsl(mode, src, dst);
    }
    (each(mode, src.0, dst.0), each(mode, src.1, dst.1), each(mode, src.2, dst.2))
}

fn lum(c: (f32, f32, f32)) -> f32 { 0.3 * c.0 + 0.59 * c.1 + 0.11 * c.2 }

fn sat(c: (f32, f32, f32)) -> f32 { c.0.max(c.1).max(c.2) - c.0.min(c.1).min(c.2) }

fn set_lum(c: (f32, f32, f32), l: f32) -> (f32, f32, f32) {
    let d = l - lum(c);
    let r = (c.0 + d).clamp(0.0, 1.0);
    let g = (c.1 + d).clamp(0.0, 1.0);
    let b = (c.2 + d).clamp(0.0, 1.0);
    (r, g, b)
}

fn set_sat(c: (f32, f32, f32), s: f32) -> (f32, f32, f32) {
    // Spec algorithm: sort channels, set max/mid/min then unsort.
    let mut arr = [(0, c.0), (1, c.1), (2, c.2)];
    arr.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let (i_min, _) = arr[0];
    let (i_mid, v_mid) = arr[1];
    let (i_max, v_max) = arr[2];
    let mut out = [0.0; 3];
    if v_max > arr[0].1 {
        out[i_mid] = ((v_mid - arr[0].1) * s) / (v_max - arr[0].1);
        out[i_max] = s;
    }
    out[i_min] = 0.0;
    (out[0], out[1], out[2])
}

fn blend_hsl(mode: BlendMode, src: (f32, f32, f32), dst: (f32, f32, f32)) -> (f32, f32, f32) {
    match mode {
        BlendMode::Hue => set_lum(set_sat(src, sat(dst)), lum(dst)),
        BlendMode::Saturation => set_lum(set_sat(dst, sat(src)), lum(dst)),
        BlendMode::Color => set_lum(src, lum(dst)),
        BlendMode::Luminosity => set_lum(dst, lum(src)),
        _ => src,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_returns_src() {
        let b = blend_rgb(BlendMode::Normal, (0.5, 0.5, 0.5), (1.0, 0.0, 0.0));
        assert_eq!(b, (0.5, 0.5, 0.5));
    }

    #[test]
    fn multiply_darkens() {
        let b = blend_rgb(BlendMode::Multiply, (0.5, 0.5, 0.5), (0.5, 0.5, 0.5));
        assert!((b.0 - 0.25).abs() < 0.001);
    }

    #[test]
    fn screen_lightens() {
        let b = blend_rgb(BlendMode::Screen, (0.5, 0.5, 0.5), (0.5, 0.5, 0.5));
        assert!((b.0 - 0.75).abs() < 0.001);
    }

    #[test]
    fn darken_picks_min() {
        let b = blend_rgb(BlendMode::Darken, (0.3, 0.8, 0.2), (0.7, 0.2, 0.5));
        assert!((b.0 - 0.3).abs() < 0.001);
        assert!((b.1 - 0.2).abs() < 0.001);
        assert!((b.2 - 0.2).abs() < 0.001);
    }

    #[test]
    fn lighten_picks_max() {
        let b = blend_rgb(BlendMode::Lighten, (0.3, 0.8, 0.2), (0.7, 0.2, 0.5));
        assert!((b.0 - 0.7).abs() < 0.001);
    }

    #[test]
    fn plus_lighter_clamps() {
        let b = blend_rgb(BlendMode::PlusLighter, (0.8, 0.0, 0.0), (0.8, 0.0, 0.0));
        assert!((b.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_returns_shader_id() {
        assert_eq!(BlendMode::parse("multiply").shader_id(), 1);
        assert_eq!(BlendMode::parse("garbage").shader_id(), 0);
    }

    #[test]
    fn difference_abs() {
        let b = blend_rgb(BlendMode::Difference, (0.3, 0.5, 0.2), (0.8, 0.5, 0.6));
        assert!((b.0 - 0.5).abs() < 0.001);
        assert!((b.1 - 0.0).abs() < 0.001);
        assert!((b.2 - 0.4).abs() < 0.001);
    }
}
