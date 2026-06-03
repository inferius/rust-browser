//! LCD sub-pixel anti-aliasing pro text rendering.
//!
//! Standard grayscale AA = 1 alpha per pixel. LCD AA = 3 sub-pixels (R/G/B
//! channels) - text vypada ostreji na LCD displays.
//!
//! Implementacne:
//! - fontdue `rasterize_subpixel` vrati 3x sirkovy bitmap (RGB per pixel)
//! - GPU dual-source blend: src.rgb = lcd coverage, src1 = alpha per channel
//! - WGPU feature DUAL_SOURCE_BLENDING required
//!
//! Foundation: detekce orientation + gamma correction config + dual-source
//! blend state. Real wire = aktivace wgpu feature + shader update.
//!
//! Inspired by Chromium `third_party/skia/src/gpu/graphite/text/`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LcdOrientation {
    /// Standard horizontal RGB stripes.
    Rgb,
    /// Reverse (some monitors).
    Bgr,
    /// Vertical (rotated).
    VRgb,
    VBgr,
}

#[derive(Debug, Clone, Copy)]
pub struct LcdConfig {
    pub orientation: LcdOrientation,
    /// Gamma correction (typicky 1.4-1.8). Vyssi = darker.
    pub gamma: f32,
    /// Enable jen pri size <= threshold (mensi text = vetsi benefit).
    pub max_size_px: f32,
}

impl Default for LcdConfig {
    fn default() -> Self {
        Self {
            orientation: LcdOrientation::Rgb,
            gamma: 1.4,
            max_size_px: 24.0,
        }
    }
}

impl LcdConfig {
    /// Should LCD AA be used pro given font size?
    pub fn enabled_for(&self, size_px: f32) -> bool {
        size_px <= self.max_size_px
    }
}

/// Convert subpixel coverage (3 floats R/G/B alpha) na linear RGBA s gamma.
/// Inspired by Skia FreeType LCD blending.
pub fn apply_gamma(coverage: [f32; 3], gamma: f32) -> [f32; 3] {
    let g = gamma.max(0.1);
    [
        coverage[0].powf(1.0 / g),
        coverage[1].powf(1.0 / g),
        coverage[2].powf(1.0 / g),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcd_enabled_small_text() {
        let c = LcdConfig::default();
        assert!(c.enabled_for(12.0));
        assert!(c.enabled_for(20.0));
        assert!(!c.enabled_for(48.0));
    }

    #[test]
    fn gamma_correction() {
        let out = apply_gamma([0.5, 0.5, 0.5], 2.0);
        // 0.5^(1/2) = 0.707
        assert!((out[0] - 0.707).abs() < 0.01);
    }

    #[test]
    fn gamma_1_passthrough() {
        let out = apply_gamma([0.3, 0.5, 0.7], 1.0);
        assert_eq!(out, [0.3, 0.5, 0.7]);
    }
}
