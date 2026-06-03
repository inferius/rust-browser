//! Viewport state + visual/layout viewport split + zoom/pinch tracking.
//!
//! Layout viewport = CSS px area that CSS sees (used by @media + vw/vh).
//! Visual viewport = current visible portion after pinch-zoom + URL bar collapse.

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub layout_width: f32,
    pub layout_height: f32,
    pub visual_width: f32,
    pub visual_height: f32,
    pub offset_x: f32,             // visual offset within layout viewport
    pub offset_y: f32,
    pub pinch_scale: f32,          // visual zoom (1.0 default)
    pub page_zoom: f32,             // user zoom (Ctrl+/-), affects layout
    pub device_pixel_ratio: f32,
    pub orientation: ViewportOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportOrientation {
    Portrait,
    Landscape,
}

impl Viewport {
    pub fn new(width: f32, height: f32, dpr: f32) -> Self {
        Self {
            layout_width: width, layout_height: height,
            visual_width: width, visual_height: height,
            offset_x: 0.0, offset_y: 0.0,
            pinch_scale: 1.0, page_zoom: 1.0,
            device_pixel_ratio: dpr,
            orientation: if width > height { ViewportOrientation::Landscape } else { ViewportOrientation::Portrait },
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.layout_width = width;
        self.layout_height = height;
        self.visual_width = width / self.pinch_scale;
        self.visual_height = height / self.pinch_scale;
        self.orientation = if width > height { ViewportOrientation::Landscape } else { ViewportOrientation::Portrait };
    }

    pub fn pinch(&mut self, new_scale: f32, focus_x: f32, focus_y: f32) {
        let old = self.pinch_scale;
        let s = new_scale.clamp(1.0, 5.0);
        self.pinch_scale = s;
        self.visual_width = self.layout_width / s;
        self.visual_height = self.layout_height / s;
        // Adjust offset so focus point stays put.
        let dx = focus_x * (1.0 / s - 1.0 / old);
        let dy = focus_y * (1.0 / s - 1.0 / old);
        self.offset_x = (self.offset_x - dx).max(0.0).min(self.layout_width - self.visual_width);
        self.offset_y = (self.offset_y - dy).max(0.0).min(self.layout_height - self.visual_height);
    }

    pub fn page_zoom_in(&mut self) {
        self.page_zoom = (self.page_zoom * 1.1).min(5.0);
    }

    pub fn page_zoom_out(&mut self) {
        self.page_zoom = (self.page_zoom / 1.1).max(0.25);
    }

    pub fn page_zoom_reset(&mut self) {
        self.page_zoom = 1.0;
    }

    /// Layout px -> device px (physical framebuffer).
    pub fn to_device_px(&self, css_px: f32) -> f32 {
        css_px * self.page_zoom * self.device_pixel_ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_landscape() {
        let v = Viewport::new(1200.0, 800.0, 1.0);
        assert_eq!(v.orientation, ViewportOrientation::Landscape);
    }

    #[test]
    fn orientation_portrait() {
        let v = Viewport::new(400.0, 800.0, 1.0);
        assert_eq!(v.orientation, ViewportOrientation::Portrait);
    }

    #[test]
    fn pinch_zooms_visual() {
        let mut v = Viewport::new(1000.0, 1000.0, 1.0);
        v.pinch(2.0, 500.0, 500.0);
        assert_eq!(v.pinch_scale, 2.0);
        assert_eq!(v.visual_width, 500.0);
    }

    #[test]
    fn pinch_clamped_to_5() {
        let mut v = Viewport::new(1000.0, 1000.0, 1.0);
        v.pinch(10.0, 0.0, 0.0);
        assert_eq!(v.pinch_scale, 5.0);
    }

    #[test]
    fn page_zoom_steps() {
        let mut v = Viewport::new(1000.0, 1000.0, 1.0);
        v.page_zoom_in();
        assert!((v.page_zoom - 1.1).abs() < 0.001);
        v.page_zoom_reset();
        assert_eq!(v.page_zoom, 1.0);
    }

    #[test]
    fn css_to_device_includes_zoom_and_dpr() {
        let v = Viewport::new(1000.0, 1000.0, 2.0);
        let mut v2 = v;
        v2.page_zoom = 1.5;
        // 100 CSS * 1.5 zoom * 2 dpr = 300
        assert_eq!(v2.to_device_px(100.0), 300.0);
    }
}
