//! Forced colors mode - Windows High Contrast, accessibility theme override.
//!
//! Spec: CSS Color Adjust L1.
//!
//! Pri `@media (forced-colors: active)` browser override pages colors s
//! system color palette (CanvasText, Canvas, ButtonFace, ButtonText, ...).

#[derive(Debug, Clone, Copy)]
pub struct SystemColors {
    pub canvas: [u8; 4],
    pub canvas_text: [u8; 4],
    pub link_text: [u8; 4],
    pub visited_text: [u8; 4],
    pub active_text: [u8; 4],
    pub button_face: [u8; 4],
    pub button_text: [u8; 4],
    pub field: [u8; 4],
    pub field_text: [u8; 4],
    pub highlight: [u8; 4],
    pub highlight_text: [u8; 4],
    pub gray_text: [u8; 4],
}

impl SystemColors {
    pub fn light_default() -> Self {
        Self {
            canvas: [255, 255, 255, 255],
            canvas_text: [0, 0, 0, 255],
            link_text: [0, 0, 238, 255],
            visited_text: [85, 26, 139, 255],
            active_text: [255, 0, 0, 255],
            button_face: [240, 240, 240, 255],
            button_text: [0, 0, 0, 255],
            field: [255, 255, 255, 255],
            field_text: [0, 0, 0, 255],
            highlight: [51, 153, 255, 255],
            highlight_text: [255, 255, 255, 255],
            gray_text: [128, 128, 128, 255],
        }
    }

    pub fn dark_default() -> Self {
        Self {
            canvas: [0, 0, 0, 255],
            canvas_text: [255, 255, 255, 255],
            link_text: [110, 168, 254, 255],
            visited_text: [184, 110, 254, 255],
            active_text: [255, 100, 100, 255],
            button_face: [45, 45, 45, 255],
            button_text: [255, 255, 255, 255],
            field: [30, 30, 30, 255],
            field_text: [255, 255, 255, 255],
            highlight: [10, 132, 255, 255],
            highlight_text: [255, 255, 255, 255],
            gray_text: [160, 160, 160, 255],
        }
    }

    /// High Contrast Black (Windows) - pure black/white/yellow/blue.
    pub fn high_contrast_black() -> Self {
        Self {
            canvas: [0, 0, 0, 255],
            canvas_text: [255, 255, 255, 255],
            link_text: [255, 255, 0, 255],
            visited_text: [128, 255, 255, 255],
            active_text: [255, 0, 0, 255],
            button_face: [0, 0, 0, 255],
            button_text: [255, 255, 255, 255],
            field: [0, 0, 0, 255],
            field_text: [255, 255, 255, 255],
            highlight: [0, 0, 128, 255],
            highlight_text: [255, 255, 0, 255],
            gray_text: [128, 128, 128, 255],
        }
    }

    /// Lookup CSS system color by name.
    pub fn lookup(&self, name: &str) -> Option<[u8; 4]> {
        match name.to_lowercase().as_str() {
            "canvas" => Some(self.canvas),
            "canvastext" => Some(self.canvas_text),
            "linktext" => Some(self.link_text),
            "visitedtext" => Some(self.visited_text),
            "activetext" => Some(self.active_text),
            "buttonface" => Some(self.button_face),
            "buttontext" => Some(self.button_text),
            "field" => Some(self.field),
            "fieldtext" => Some(self.field_text),
            "highlight" => Some(self.highlight),
            "highlighttext" => Some(self.highlight_text),
            "graytext" => Some(self.gray_text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_default_canvas_white() {
        let s = SystemColors::light_default();
        assert_eq!(s.canvas, [255, 255, 255, 255]);
    }

    #[test]
    fn high_contrast_canvas_black() {
        let s = SystemColors::high_contrast_black();
        assert_eq!(s.canvas, [0, 0, 0, 255]);
        assert_eq!(s.canvas_text, [255, 255, 255, 255]);
    }

    #[test]
    fn lookup_by_name() {
        let s = SystemColors::light_default();
        assert_eq!(s.lookup("CanvasText"), Some([0, 0, 0, 255]));
        assert_eq!(s.lookup("Highlight"), Some([51, 153, 255, 255]));
        assert!(s.lookup("nonexistent").is_none());
    }
}
