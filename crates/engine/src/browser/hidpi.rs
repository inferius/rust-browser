//! HiDPI scaling helpers - per-monitor DPR, snap to pixel grid.

#[derive(Debug, Clone, Copy)]
pub struct HiDpiContext {
    pub device_pixel_ratio: f32,
    pub backing_store_ratio: f32,    // Retina retina backing store, typically same as DPR
    pub forced_dpr: Option<f32>,     // dev override
}

impl Default for HiDpiContext {
    fn default() -> Self {
        Self { device_pixel_ratio: 1.0, backing_store_ratio: 1.0, forced_dpr: None }
    }
}

impl HiDpiContext {
    pub fn effective_dpr(&self) -> f32 {
        self.forced_dpr.unwrap_or(self.device_pixel_ratio)
    }

    /// Snap a CSS px value to physical-pixel grid.
    /// Avoids subpixel artifacts on lines/rects.
    pub fn snap_px(&self, css_px: f32) -> f32 {
        let phys = css_px * self.effective_dpr();
        let snapped = phys.round();
        snapped / self.effective_dpr()
    }

    /// Snap a rect (x, y, w, h) so that both edges align to device pixels.
    pub fn snap_rect(&self, rect: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let dpr = self.effective_dpr();
        let x_phys = (rect.0 * dpr).round();
        let y_phys = (rect.1 * dpr).round();
        let right_phys = ((rect.0 + rect.2) * dpr).round();
        let bottom_phys = ((rect.1 + rect.3) * dpr).round();
        (
            x_phys / dpr,
            y_phys / dpr,
            (right_phys - x_phys) / dpr,
            (bottom_phys - y_phys) / dpr,
        )
    }
}

/// HiDPI bucket for font atlas keys: scale font size + dpr -> integer key.
pub fn atlas_key_for_font(font_size_css: f32, dpr: f32) -> u32 {
    ((font_size_css * dpr).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_px_at_1x() {
        let c = HiDpiContext { device_pixel_ratio: 1.0, ..Default::default() };
        assert_eq!(c.snap_px(10.4), 10.0);
        assert_eq!(c.snap_px(10.6), 11.0);
    }

    #[test]
    fn snap_px_at_2x() {
        let c = HiDpiContext { device_pixel_ratio: 2.0, ..Default::default() };
        // 10.2 * 2 = 20.4 -> 20 -> /2 = 10.0
        assert_eq!(c.snap_px(10.2), 10.0);
        // 10.3 * 2 = 20.6 -> 21 -> /2 = 10.5
        assert_eq!(c.snap_px(10.3), 10.5);
    }

    #[test]
    fn snap_rect_preserves_size() {
        let c = HiDpiContext { device_pixel_ratio: 1.0, ..Default::default() };
        let r = c.snap_rect((0.4, 0.4, 9.6, 9.6));
        assert_eq!(r.2, 10.0);
    }

    #[test]
    fn forced_dpr_overrides() {
        let c = HiDpiContext {
            device_pixel_ratio: 1.0,
            forced_dpr: Some(2.0),
            backing_store_ratio: 1.0,
        };
        assert_eq!(c.effective_dpr(), 2.0);
    }

    #[test]
    fn atlas_key_integer_round() {
        assert_eq!(atlas_key_for_font(16.0, 1.5), 24);
        assert_eq!(atlas_key_for_font(0.1, 1.0), 1);
    }
}
