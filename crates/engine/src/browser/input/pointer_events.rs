//! Pointer Events Level 3 - unified mouse/touch/pen handling.
//!
//! Spec: https://www.w3.org/TR/pointerevents3/
//! pointerType: mouse | touch | pen
//! event types: pointerdown, pointermove, pointerup, pointercancel, pointerover, pointerout

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerType {
    Mouse,
    Touch,
    Pen,
    Unknown,
}

impl PointerType {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "mouse" => Self::Mouse,
            "touch" => Self::Touch,
            "pen" => Self::Pen,
            _ => Self::Unknown,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Mouse => "mouse",
            Self::Touch => "touch",
            Self::Pen => "pen",
            Self::Unknown => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEventType {
    Down,
    Move,
    Up,
    Cancel,
    Over,
    Out,
    Enter,
    Leave,
    GotPointerCapture,
    LostPointerCapture,
}

#[derive(Debug, Clone)]
pub struct PointerInfo {
    pub id: i32,
    pub kind: PointerType,
    pub is_primary: bool,
    pub width: f32,
    pub height: f32,
    pub pressure: f32,
    pub tangential_pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub twist: u16,
    pub buttons: u32,
}

#[derive(Debug, Clone)]
pub struct PointerEventDescriptor {
    pub event_type: PointerEventType,
    pub pointer: PointerInfo,
    pub client_x: f32,
    pub client_y: f32,
    pub timestamp_ms: f64,
    pub target_id: u64,
    pub coalesced_events: Vec<PointerSample>,
}

#[derive(Debug, Clone)]
pub struct PointerSample {
    pub client_x: f32,
    pub client_y: f32,
    pub pressure: f32,
    pub timestamp_ms: f64,
}

#[derive(Default)]
pub struct PointerCaptureMap {
    /// pointerId -> element id (target captures all events while held).
    pub captures: HashMap<i32, u64>,
}

impl PointerCaptureMap {
    pub fn new() -> Self { Self::default() }

    pub fn set_capture(&mut self, pointer_id: i32, element_id: u64) {
        self.captures.insert(pointer_id, element_id);
    }

    pub fn release_capture(&mut self, pointer_id: i32, element_id: u64) -> bool {
        if self.captures.get(&pointer_id) == Some(&element_id) {
            self.captures.remove(&pointer_id);
            true
        } else { false }
    }

    pub fn target_override(&self, pointer_id: i32) -> Option<u64> {
        self.captures.get(&pointer_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_type() {
        assert_eq!(PointerType::parse("mouse"), PointerType::Mouse);
        assert_eq!(PointerType::parse("touch"), PointerType::Touch);
        assert_eq!(PointerType::parse("pen"), PointerType::Pen);
        assert_eq!(PointerType::parse("xxx"), PointerType::Unknown);
    }

    #[test]
    fn capture_set_and_get() {
        let mut m = PointerCaptureMap::new();
        m.set_capture(1, 100);
        assert_eq!(m.target_override(1), Some(100));
    }

    #[test]
    fn release_only_by_owner() {
        let mut m = PointerCaptureMap::new();
        m.set_capture(1, 100);
        assert!(!m.release_capture(1, 999));
        assert!(m.release_capture(1, 100));
        assert!(m.target_override(1).is_none());
    }

    #[test]
    fn pointer_info_defaults() {
        let p = PointerInfo {
            id: 1, kind: PointerType::Mouse, is_primary: true,
            width: 1.0, height: 1.0,
            pressure: 0.5, tangential_pressure: 0.0,
            tilt_x: 0.0, tilt_y: 0.0, twist: 0, buttons: 1,
        };
        assert!(p.is_primary);
        assert_eq!(p.kind, PointerType::Mouse);
    }
}
