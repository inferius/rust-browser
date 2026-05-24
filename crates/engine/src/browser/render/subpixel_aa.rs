//! Sub-pixel AA - LCD anti-aliasing for glyph rendering.
//!
//! Pattern: target pixel is one of {R-G-B, B-G-R, vertical RGB, vertical BGR}.
//! Per-subpixel coverage uses 3x oversampling -> assign to R/G/B channel.
//! Plus filter: 1/9, 2/9, 3/9, 2/9, 1/9 [-2,-1,0,+1,+2] to reduce color fringing.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LcdOrder {
    RgbHorizontal,
    BgrHorizontal,
    RgbVertical,
    BgrVertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AntialiasMode {
    None,
    Grayscale,
    Lcd(LcdOrder),
}

/// Apply the FreeType-style 1-3-5-3-1 LCD filter to a 5-subpixel window.
/// Returns filtered center subpixel value in [0,1].
pub fn lcd_filter_5tap(window: [f32; 5]) -> f32 {
    let v = window[0] * 1.0 / 13.0
          + window[1] * 3.0 / 13.0
          + window[2] * 5.0 / 13.0
          + window[3] * 3.0 / 13.0
          + window[4] * 1.0 / 13.0;
    v.clamp(0.0, 1.0)
}

/// Resolve 3x oversampled coverage into RGB triplet.
/// `samples` indexed in source order (no reorder).
/// Returns (R, G, B) coverage [0,1] for the target subpixel order.
pub fn samples_to_rgb(samples: [f32; 3], order: LcdOrder) -> (f32, f32, f32) {
    match order {
        LcdOrder::RgbHorizontal | LcdOrder::RgbVertical => (samples[0], samples[1], samples[2]),
        LcdOrder::BgrHorizontal | LcdOrder::BgrVertical => (samples[2], samples[1], samples[0]),
    }
}

/// Compute the gamma-corrected source/dst blend for an LCD subpixel.
/// Returns target framebuffer triplet given foreground color, background, and per-channel coverage.
pub fn lcd_blend(
    fg: (f32, f32, f32),
    bg: (f32, f32, f32),
    coverage: (f32, f32, f32),
    gamma: f32,
) -> (f32, f32, f32) {
    fn apply(fg: f32, bg: f32, cov: f32, g: f32) -> f32 {
        let fg_lin = fg.powf(g);
        let bg_lin = bg.powf(g);
        let out_lin = fg_lin * cov + bg_lin * (1.0 - cov);
        out_lin.powf(1.0 / g)
    }
    (
        apply(fg.0, bg.0, coverage.0, gamma),
        apply(fg.1, bg.1, coverage.1, gamma),
        apply(fg.2, bg.2, coverage.2, gamma),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_constant_passthrough() {
        let v = lcd_filter_5tap([0.5; 5]);
        assert!((v - 0.5).abs() < 0.001);
    }

    #[test]
    fn filter_clamps() {
        let v = lcd_filter_5tap([10.0; 5]);
        assert!(v <= 1.0);
    }

    #[test]
    fn rgb_order_passthrough() {
        let (r, g, b) = samples_to_rgb([0.1, 0.2, 0.3], LcdOrder::RgbHorizontal);
        assert!((r - 0.1).abs() < 0.001);
        assert!((g - 0.2).abs() < 0.001);
        assert!((b - 0.3).abs() < 0.001);
    }

    #[test]
    fn bgr_order_reverses() {
        let (r, g, b) = samples_to_rgb([0.1, 0.2, 0.3], LcdOrder::BgrHorizontal);
        assert!((r - 0.3).abs() < 0.001);
        assert!((g - 0.2).abs() < 0.001);
        assert!((b - 0.1).abs() < 0.001);
    }

    #[test]
    fn full_coverage_returns_fg() {
        let res = lcd_blend((0.1, 0.2, 0.3), (0.9, 0.8, 0.7), (1.0, 1.0, 1.0), 2.2);
        assert!((res.0 - 0.1).abs() < 0.005);
        assert!((res.1 - 0.2).abs() < 0.005);
        assert!((res.2 - 0.3).abs() < 0.005);
    }

    #[test]
    fn zero_coverage_returns_bg() {
        let res = lcd_blend((0.1, 0.2, 0.3), (0.9, 0.8, 0.7), (0.0, 0.0, 0.0), 2.2);
        assert!((res.0 - 0.9).abs() < 0.005);
    }
}
