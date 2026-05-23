//! Virtual Keyboard API - on-screen kbd state + layout adjustment.
//!
//! Spec: https://w3c.github.io/virtual-keyboard/
//! navigator.virtualKeyboard.overlaysContent - z layout VK overlay content
//! ne resize viewport. + geometrychange event s rect.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VirtualKeyboardPolicy {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy)]
pub struct VirtualKeyboardRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
pub struct VirtualKeyboardService {
    pub overlays_content: bool,
    pub policy: VirtualKeyboardPolicy,
    pub bounds: VirtualKeyboardRect,
    pub visible: bool,
}

impl Default for VirtualKeyboardPolicy {
    fn default() -> Self { VirtualKeyboardPolicy::Auto }
}

impl Default for VirtualKeyboardRect {
    fn default() -> Self { Self { x: 0, y: 0, width: 0, height: 0 } }
}

impl VirtualKeyboardService {
    pub fn new() -> Self { Self::default() }

    pub fn show(&mut self) -> bool {
        if self.policy == VirtualKeyboardPolicy::Auto { return false; } // jen pri Manual
        self.visible = true;
        true
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.bounds = VirtualKeyboardRect::default();
    }

    pub fn set_overlays_content(&mut self, overlays: bool) {
        self.overlays_content = overlays;
    }

    pub fn set_bounds(&mut self, rect: VirtualKeyboardRect) {
        self.bounds = rect;
        self.visible = rect.width > 0 && rect.height > 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_show() {
        let mut s = VirtualKeyboardService::new();
        s.policy = VirtualKeyboardPolicy::Manual;
        assert!(s.show());
        assert!(s.visible);
    }

    #[test]
    fn auto_show_blocked() {
        let mut s = VirtualKeyboardService::new();
        assert!(!s.show()); // auto policy = needs OS focus to trigger
    }

    #[test]
    fn set_bounds_marks_visible() {
        let mut s = VirtualKeyboardService::new();
        s.set_bounds(VirtualKeyboardRect { x: 0, y: 500, width: 800, height: 300 });
        assert!(s.visible);
    }

    #[test]
    fn hide_clears_bounds() {
        let mut s = VirtualKeyboardService::new();
        s.set_bounds(VirtualKeyboardRect { x: 0, y: 500, width: 800, height: 300 });
        s.hide();
        assert!(!s.visible);
        assert_eq!(s.bounds.height, 0);
    }
}
