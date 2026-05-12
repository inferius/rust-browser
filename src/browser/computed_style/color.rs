//! Color value enum + parser (CSS Color L4 superset).
//!
//! Zachovava puvodni color space (Hsl, Oklab, ...) pro:
//! - Animation interp v native space (oklab > rgb perceptual)
//! - color-mix() / relative color musi znat puvodni komponenty
//! - JS getComputedStyle round-trip (zatim NE - CSS spec L4 §15 serializace
//!   vsechny barvy normalizuje na rgb()/rgba(), takze pro JS api stale
//!   serializujem na rgb)
//!
//! Default `to_rgba_u8()` konvertuje na sRGB rgba pro renderer.

/// CSS <color> value zachovavajici puvodni space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    /// sRGB rgba u8, vsechny hex/rgb/rgba/named normalizovany sem.
    Rgba { r: u8, g: u8, b: u8, a: u8 },
    /// HSL space: h v deg [0..360), s/l v [0..1], a v [0..1].
    Hsl { h: f32, s: f32, l: f32, a: f32 },
    /// HWB space: h v deg, w/b/a v [0..1].
    Hwb { h: f32, w: f32, b: f32, a: f32 },
    /// CIE Lab: L v [0..100], a/b v [-125..125], alpha v [0..1].
    Lab { l: f32, a: f32, b: f32, alpha: f32 },
    /// CIE LCh: L v [0..100], C v [0..150], h v deg, alpha v [0..1].
    Lch { l: f32, c: f32, h: f32, alpha: f32 },
    /// Oklab: L v [0..1], a/b v [-0.4..0.4], alpha v [0..1].
    Oklab { l: f32, a: f32, b: f32, alpha: f32 },
    /// Oklch: L v [0..1], C v [0..0.5], h v deg, alpha v [0..1].
    Oklch { l: f32, c: f32, h: f32, alpha: f32 },
    /// CSS `currentColor` - resolved against parent `color` pri cascade.
    CurrentColor,
}

impl Color {
    /// sRGB rgba u8 - pro renderer (GPU prijima sRGB).
    /// Pro non-sRGB spaces dela approximaci (proper conversion = TODO).
    pub fn to_rgba_u8(self) -> [u8; 4] {
        match self {
            Color::Rgba { r, g, b, a } => [r, g, b, a],
            Color::Hsl { h, s, l, a } => hsl_to_rgba_u8(h, s, l, a),
            Color::Hwb { h, w, b, a } => hwb_to_rgba_u8(h, w, b, a),
            // Lab/Oklab/Lch/Oklch: approximation - direct linear interp do sRGB.
            // Proper xyz round-trip = TODO. Zatim treat L jako luminance.
            Color::Lab { l, a, b, alpha } => lab_approx_to_rgba_u8(l, a, b, alpha),
            Color::Lch { l, c, h, alpha } => {
                let a = c * (h.to_radians().cos());
                let b = c * (h.to_radians().sin());
                lab_approx_to_rgba_u8(l, a, b, alpha)
            }
            Color::Oklab { l, a, b, alpha } => oklab_to_rgba_u8(l, a, b, alpha),
            Color::Oklch { l, c, h, alpha } => {
                let a = c * (h.to_radians().cos());
                let b = c * (h.to_radians().sin());
                oklab_to_rgba_u8(l, a, b, alpha)
            }
            // CurrentColor by mel byt resolvovan v cascade (= parent `color`).
            // Pokud unresolved (e.g. paint pred cascade), vrati cernou.
            Color::CurrentColor => [0, 0, 0, 255],
        }
    }

    /// CSS L4 §15 standardni serializace: vsechny barvy -> `rgb(r, g, b)`
    /// nebo `rgba(r, g, b, a)` pokud alpha < 1. Pouzito pro JS
    /// `getComputedStyle().color`.
    pub fn to_css_string(self) -> String {
        let [r, g, b, a] = self.to_rgba_u8();
        if a == 255 {
            format!("rgb({}, {}, {})", r, g, b)
        } else {
            format!("rgba({}, {}, {}, {})", r, g, b, a as f32 / 255.0)
        }
    }
}

fn hsl_to_rgba_u8(h: f32, s: f32, l: f32, a: f32) -> [u8; 4] {
    // CSS Color L4 §6.1
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8, (a * 255.0).round() as u8]
}

fn hue_to_rgb(p: f32, q: f32, t: f32) -> f32 {
    let t = if t < 0.0 { t + 1.0 } else if t > 1.0 { t - 1.0 } else { t };
    if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t }
    else if t < 0.5 { q }
    else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 }
    else { p }
}

fn hwb_to_rgba_u8(h: f32, w: f32, b: f32, a: f32) -> [u8; 4] {
    // CSS Color L4 §7.2: hwb -> rgb pres mixing white/black do hue.
    if w + b >= 1.0 {
        let gray = ((w / (w + b)) * 255.0).round() as u8;
        return [gray, gray, gray, (a * 255.0).round() as u8];
    }
    let [hr, hg, hb, _] = hsl_to_rgba_u8(h, 1.0, 0.5, 1.0);
    let blend = |c: u8| -> u8 {
        let f = c as f32 / 255.0;
        ((f * (1.0 - w - b) + w) * 255.0).round() as u8
    };
    [blend(hr), blend(hg), blend(hb), (a * 255.0).round() as u8]
}

fn lab_approx_to_rgba_u8(l: f32, _a: f32, _b: f32, alpha: f32) -> [u8; 4] {
    // Approximation: L jako luminance grayscale. Proper xyz -> sRGB = TODO.
    let v = (l / 100.0 * 255.0).clamp(0.0, 255.0) as u8;
    [v, v, v, (alpha * 255.0).round() as u8]
}

fn oklab_to_rgba_u8(l: f32, a: f32, b: f32, alpha: f32) -> [u8; 4] {
    // Proper oklab -> linear sRGB -> sRGB gamma per CSS Color L4 §10.
    // M1, M2 matice z spec.
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    let r_lin = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let g_lin = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let b_lin = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;
    [linear_to_srgb_u8(r_lin), linear_to_srgb_u8(g_lin), linear_to_srgb_u8(b_lin), (alpha * 255.0).round() as u8]
}

fn linear_to_srgb_u8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let srgb = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (srgb * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_format_roundtrip() {
        // #c00 -> Rgba { 204, 0, 0, 255 }
        let c = Color::Rgba { r: 204, g: 0, b: 0, a: 255 };
        assert_eq!(c.to_rgba_u8(), [204, 0, 0, 255]);
        assert_eq!(c.to_css_string(), "rgb(204, 0, 0)");
    }

    #[test]
    fn rgba_alpha_serialize() {
        let c = Color::Rgba { r: 0, g: 200, b: 0, a: 128 };
        let s = c.to_css_string();
        assert!(s.starts_with("rgba(0, 200, 0,"));
    }

    #[test]
    fn hsl_to_rgb() {
        // hsl(120, 60%, 50%) -> Chrome rgb(51, 204, 51)
        let c = Color::Hsl { h: 120.0, s: 0.6, l: 0.5, a: 1.0 };
        let [r, g, b, a] = c.to_rgba_u8();
        assert_eq!(a, 255);
        // Tolerance 2 (round-off)
        assert!((r as i32 - 51).abs() <= 2);
        assert!((g as i32 - 204).abs() <= 2);
        assert!((b as i32 - 51).abs() <= 2);
    }

    #[test]
    fn oklab_white() {
        // oklab(1 0 0) ~ white sRGB
        let c = Color::Oklab { l: 1.0, a: 0.0, b: 0.0, alpha: 1.0 };
        let [r, g, b, a] = c.to_rgba_u8();
        assert_eq!(a, 255);
        assert!(r > 250);
        assert!(g > 250);
        assert!(b > 250);
    }

    #[test]
    fn current_color_fallback() {
        // Unresolved CurrentColor = black
        let c = Color::CurrentColor;
        assert_eq!(c.to_rgba_u8(), [0, 0, 0, 255]);
    }
}
