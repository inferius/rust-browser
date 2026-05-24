//! Absolute / fixed / sticky / relative positioning helpers.
//!
//! Spec: https://www.w3.org/TR/CSS22/visuren.html#choose-position
//! https://drafts.csswg.org/css-position-3/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionKind {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PositionOffsets {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct PositionedRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Resolve an absolute position rect against its containing block rect.
pub fn resolve_absolute(
    cb: PositionedRect,
    offsets: PositionOffsets,
    inline_size: f32,
    block_size: f32,
) -> PositionedRect {
    let mut x = cb.x;
    let mut y = cb.y;
    let mut width = inline_size;
    let mut height = block_size;
    let cb_right = cb.x + cb.width;
    let cb_bottom = cb.y + cb.height;

    match (offsets.left, offsets.right) {
        (Some(l), Some(r)) => {
            x = cb.x + l;
            width = (cb_right - r - x).max(0.0);
        }
        (Some(l), None) => x = cb.x + l,
        (None, Some(r)) => x = cb_right - r - width,
        (None, None) => {}
    }
    match (offsets.top, offsets.bottom) {
        (Some(t), Some(b)) => {
            y = cb.y + t;
            height = (cb_bottom - b - y).max(0.0);
        }
        (Some(t), None) => y = cb.y + t,
        (None, Some(b)) => y = cb_bottom - b - height,
        (None, None) => {}
    }
    PositionedRect { x, y, width, height }
}

/// Sticky: rect stays in flow until viewport scrolls past constrained edge.
pub fn resolve_sticky(
    flow_rect: PositionedRect,
    scroll_container: PositionedRect,
    offsets: PositionOffsets,
    scroll_y: f32,
) -> PositionedRect {
    let mut y = flow_rect.y;
    let viewport_top = scroll_container.y + scroll_y;
    let viewport_bottom = viewport_top + scroll_container.height;
    if let Some(top) = offsets.top {
        let constrained_top = viewport_top + top;
        if y < constrained_top {
            y = constrained_top;
        }
    }
    if let Some(bottom) = offsets.bottom {
        let constrained_bottom = viewport_bottom - bottom;
        if y + flow_rect.height > constrained_bottom {
            y = constrained_bottom - flow_rect.height;
        }
    }
    // Constrain to flow_rect <= y <= scroll_container_bottom - height
    PositionedRect { x: flow_rect.x, y, width: flow_rect.width, height: flow_rect.height }
}

/// Apply relative offset to a rect (non-replaced layout flow rect).
pub fn apply_relative(rect: PositionedRect, offsets: PositionOffsets) -> PositionedRect {
    let dx = match (offsets.left, offsets.right) {
        (Some(l), _) => l,
        (None, Some(r)) => -r,
        (None, None) => 0.0,
    };
    let dy = match (offsets.top, offsets.bottom) {
        (Some(t), _) => t,
        (None, Some(b)) => -b,
        (None, None) => 0.0,
    };
    PositionedRect { x: rect.x + dx, y: rect.y + dy, ..rect }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cb() -> PositionedRect { PositionedRect { x: 0.0, y: 0.0, width: 1000.0, height: 800.0 } }

    #[test]
    fn absolute_left_top_anchors() {
        let off = PositionOffsets { left: Some(20.0), top: Some(30.0), ..Default::default() };
        let r = resolve_absolute(cb(), off, 100.0, 50.0);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.y, 30.0);
    }

    #[test]
    fn absolute_right_bottom_anchors() {
        let off = PositionOffsets { right: Some(10.0), bottom: Some(20.0), ..Default::default() };
        let r = resolve_absolute(cb(), off, 100.0, 50.0);
        assert_eq!(r.x, 890.0); // 1000 - 10 - 100
        assert_eq!(r.y, 730.0); // 800 - 20 - 50
    }

    #[test]
    fn absolute_both_sides_stretches() {
        let off = PositionOffsets { left: Some(50.0), right: Some(50.0), ..Default::default() };
        let r = resolve_absolute(cb(), off, 100.0, 50.0);
        assert_eq!(r.x, 50.0);
        assert_eq!(r.width, 900.0);
    }

    #[test]
    fn relative_dx_dy() {
        let r = apply_relative(PositionedRect { x: 100.0, y: 100.0, width: 50.0, height: 50.0 },
            PositionOffsets { left: Some(10.0), top: Some(20.0), ..Default::default() });
        assert_eq!(r.x, 110.0);
        assert_eq!(r.y, 120.0);
    }

    #[test]
    fn relative_right_is_negative() {
        let r = apply_relative(PositionedRect { x: 100.0, y: 100.0, width: 50.0, height: 50.0 },
            PositionOffsets { right: Some(15.0), ..Default::default() });
        assert_eq!(r.x, 85.0);
    }

    #[test]
    fn sticky_clamps_to_top() {
        let flow = PositionedRect { x: 0.0, y: 200.0, width: 100.0, height: 50.0 };
        let container = PositionedRect { x: 0.0, y: 0.0, width: 500.0, height: 600.0 };
        // viewport_top = 0 + scroll_y(300). flow.y (200) < viewport_top + top(10) = 310.
        // Constrain to 310.
        let r = resolve_sticky(flow, container, PositionOffsets { top: Some(10.0), ..Default::default() }, 300.0);
        assert_eq!(r.y, 310.0);
    }

    #[test]
    fn sticky_in_natural_position() {
        let flow = PositionedRect { x: 0.0, y: 200.0, width: 100.0, height: 50.0 };
        let container = PositionedRect { x: 0.0, y: 0.0, width: 500.0, height: 600.0 };
        let r = resolve_sticky(flow, container, PositionOffsets { top: Some(10.0), ..Default::default() }, 0.0);
        assert_eq!(r.y, 200.0);
    }
}
