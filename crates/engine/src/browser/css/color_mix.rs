//! `color-mix()` + `color()` interpolation in various color spaces.
//!
//! Spec: https://www.w3.org/TR/css-color-5/
//! Examples: color-mix(in oklch, red 50%, blue), color(display-p3 1 0.5 0).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorSpaceMix {
    Srgb,
    SrgbLinear,
    Hsl,
    Hwb,
    Oklab,
    Oklch,
    Lab,
    Lch,
    DisplayP3,
}

impl ColorSpaceMix {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "srgb" => Some(Self::Srgb),
            "srgb-linear" => Some(Self::SrgbLinear),
            "hsl" => Some(Self::Hsl),
            "hwb" => Some(Self::Hwb),
            "oklab" => Some(Self::Oklab),
            "oklch" => Some(Self::Oklch),
            "lab" => Some(Self::Lab),
            "lch" => Some(Self::Lch),
            "display-p3" => Some(Self::DisplayP3),
            _ => None,
        }
    }
}

/// Mix two RGB(A) colors in linear sRGB. Coarse fallback for non-RGB spaces.
pub fn mix_rgb(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    // Convert to linear, interpolate, back to sRGB.
    fn to_linear(c: f32) -> f32 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    fn to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }
    let al = [to_linear(a[0]), to_linear(a[1]), to_linear(a[2])];
    let bl = [to_linear(b[0]), to_linear(b[1]), to_linear(b[2])];
    [
        to_srgb(al[0] * (1.0 - t) + bl[0] * t).clamp(0.0, 1.0),
        to_srgb(al[1] * (1.0 - t) + bl[1] * t).clamp(0.0, 1.0),
        to_srgb(al[2] * (1.0 - t) + bl[2] * t).clamp(0.0, 1.0),
        a[3] * (1.0 - t) + b[3] * t,
    ]
}

/// Mix in OKLab space (perceptually uniform).
pub fn mix_oklab(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let al = rgb_to_oklab([a[0], a[1], a[2]]);
    let bl = rgb_to_oklab([b[0], b[1], b[2]]);
    let mixed = [
        al[0] * (1.0 - t) + bl[0] * t,
        al[1] * (1.0 - t) + bl[1] * t,
        al[2] * (1.0 - t) + bl[2] * t,
    ];
    let rgb = oklab_to_rgb(mixed);
    [rgb[0], rgb[1], rgb[2], a[3] * (1.0 - t) + b[3] * t]
}

// Bjorn Ottosson's OKLab matrices.
fn rgb_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    fn lin(c: f32) -> f32 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    let r = lin(rgb[0]); let g = lin(rgb[1]); let b = lin(rgb[2]);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn oklab_to_rgb(lab: [f32; 3]) -> [f32; 3] {
    let l_ = lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2];
    let m_ = lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2];
    let s_ = lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2];
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    fn srgb(c: f32) -> f32 {
        if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }
    [srgb(r).clamp(0.0, 1.0), srgb(g).clamp(0.0, 1.0), srgb(b).clamp(0.0, 1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_space() {
        assert_eq!(ColorSpaceMix::parse("oklch"), Some(ColorSpaceMix::Oklch));
        assert_eq!(ColorSpaceMix::parse("garbage"), None);
    }

    #[test]
    fn mix_at_endpoints() {
        let a = [1.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 1.0, 1.0];
        let m0 = mix_rgb(a, b, 0.0);
        assert!((m0[0] - 1.0).abs() < 0.01);
        let m1 = mix_rgb(a, b, 1.0);
        assert!((m1[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn mix_alpha_lerps() {
        let a = [0.0, 0.0, 0.0, 0.0];
        let b = [0.0, 0.0, 0.0, 1.0];
        let m = mix_rgb(a, b, 0.5);
        assert!((m[3] - 0.5).abs() < 0.001);
    }

    #[test]
    fn oklab_round_trip() {
        let r = [0.5, 0.4, 0.3];
        let lab = rgb_to_oklab(r);
        let back = oklab_to_rgb(lab);
        assert!((back[0] - r[0]).abs() < 0.01);
        assert!((back[1] - r[1]).abs() < 0.01);
        assert!((back[2] - r[2]).abs() < 0.01);
    }

    #[test]
    fn mix_oklab_smooth() {
        let a = [1.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 1.0, 1.0];
        let mid = mix_oklab(a, b, 0.5);
        // Midpoint should be a purple-ish, neither pure red nor pure blue.
        assert!(mid[0] > 0.0 && mid[0] < 1.0);
        assert!(mid[2] > 0.0 && mid[2] < 1.0);
    }
}
