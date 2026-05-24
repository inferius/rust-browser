//! CSS Anchor Positioning Level 1 - anchor() + anchor-name + position-try.
//!
//! Spec: https://www.w3.org/TR/css-anchor-position-1/
//! anchor(--my-anchor top) returns positions of the anchored element.
//! position-try-fallbacks: --top, --bottom, ... try in order until fits in viewport.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorEdge {
    Top,
    Right,
    Bottom,
    Left,
    Center,
    Start,
    End,
    SelfStart,
    SelfEnd,
}

impl AnchorEdge {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "top" => Some(Self::Top),
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "start" => Some(Self::Start),
            "end" => Some(Self::End),
            "self-start" => Some(Self::SelfStart),
            "self-end" => Some(Self::SelfEnd),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchorRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Compute value of `anchor(<anchor-name> <edge>)` per Level 1 spec.
pub fn resolve_anchor_value(anchor: &AnchorRect, edge: AnchorEdge, axis: Axis) -> f32 {
    match (edge, axis) {
        (AnchorEdge::Top, _) => anchor.y,
        (AnchorEdge::Bottom, _) => anchor.y + anchor.height,
        (AnchorEdge::Left, _) => anchor.x,
        (AnchorEdge::Right, _) => anchor.x + anchor.width,
        (AnchorEdge::Center, Axis::Horizontal) => anchor.x + anchor.width / 2.0,
        (AnchorEdge::Center, Axis::Vertical) => anchor.y + anchor.height / 2.0,
        (AnchorEdge::Start, Axis::Horizontal) => anchor.x,
        (AnchorEdge::Start, Axis::Vertical) => anchor.y,
        (AnchorEdge::End, Axis::Horizontal) => anchor.x + anchor.width,
        (AnchorEdge::End, Axis::Vertical) => anchor.y + anchor.height,
        (AnchorEdge::SelfStart, _) | (AnchorEdge::SelfEnd, _) => anchor.x,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Default)]
pub struct AnchorRegistry {
    pub anchors: HashMap<String, AnchorRect>,
}

impl AnchorRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, name: &str, rect: AnchorRect) {
        self.anchors.insert(name.into(), rect);
    }

    pub fn get(&self, name: &str) -> Option<&AnchorRect> {
        self.anchors.get(name)
    }
}

/// Check rect fits inside viewport. Otherwise position-try-fallbacks must be applied.
pub fn fits_in_viewport(rect: &AnchorRect, vw: f32, vh: f32) -> bool {
    rect.x >= 0.0 && rect.y >= 0.0
    && rect.x + rect.width <= vw
    && rect.y + rect.height <= vh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> AnchorRect { AnchorRect { x: 100.0, y: 200.0, width: 50.0, height: 80.0 } }

    #[test]
    fn anchor_edge_top() {
        assert_eq!(resolve_anchor_value(&rect(), AnchorEdge::Top, Axis::Vertical), 200.0);
    }

    #[test]
    fn anchor_edge_bottom() {
        assert_eq!(resolve_anchor_value(&rect(), AnchorEdge::Bottom, Axis::Vertical), 280.0);
    }

    #[test]
    fn anchor_edge_left() {
        assert_eq!(resolve_anchor_value(&rect(), AnchorEdge::Left, Axis::Horizontal), 100.0);
    }

    #[test]
    fn anchor_edge_right() {
        assert_eq!(resolve_anchor_value(&rect(), AnchorEdge::Right, Axis::Horizontal), 150.0);
    }

    #[test]
    fn anchor_center_h() {
        assert_eq!(resolve_anchor_value(&rect(), AnchorEdge::Center, Axis::Horizontal), 125.0);
    }

    #[test]
    fn parse_edge() {
        assert_eq!(AnchorEdge::parse("top"), Some(AnchorEdge::Top));
        assert_eq!(AnchorEdge::parse("self-start"), Some(AnchorEdge::SelfStart));
        assert_eq!(AnchorEdge::parse("wat"), None);
    }

    #[test]
    fn registry_stores() {
        let mut r = AnchorRegistry::new();
        r.register("--popover", rect());
        assert!(r.get("--popover").is_some());
    }

    #[test]
    fn fits_in_viewport_check() {
        assert!(fits_in_viewport(&rect(), 1000.0, 1000.0));
        assert!(!fits_in_viewport(&rect(), 120.0, 1000.0));
    }
}
