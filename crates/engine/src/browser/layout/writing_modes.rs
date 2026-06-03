//! CSS Writing Modes Level 4.
//!
//! Spec: https://www.w3.org/TR/css-writing-modes-4/
//! writing-mode: horizontal-tb (default) | vertical-rl | vertical-lr | sideways-rl | sideways-lr.
//! direction: ltr | rtl.
//! text-orientation: mixed | upright | sideways.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WritingMode {
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextOrientation {
    Mixed,
    Upright,
    Sideways,
}

impl WritingMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "vertical-rl" => Self::VerticalRl,
            "vertical-lr" => Self::VerticalLr,
            "sideways-rl" => Self::SidewaysRl,
            "sideways-lr" => Self::SidewaysLr,
            _ => Self::HorizontalTb,
        }
    }

    pub fn is_vertical(&self) -> bool {
        !matches!(self, Self::HorizontalTb)
    }

    pub fn inline_axis_is_x(&self) -> bool {
        !self.is_vertical()
    }
}

impl Direction {
    pub fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("rtl") { Self::Rtl } else { Self::Ltr }
    }
}

impl TextOrientation {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "upright" => Self::Upright,
            "sideways" => Self::Sideways,
            _ => Self::Mixed,
        }
    }
}

/// Logical-to-physical mapping. (inline_start, block_start) -> (x, y).
pub fn logical_to_physical(
    writing_mode: WritingMode,
    direction: Direction,
    inline_start: f32,
    block_start: f32,
    inline_size: f32,
    block_size: f32,
    container_w: f32,
    container_h: f32,
) -> (f32, f32, f32, f32) {
    match writing_mode {
        WritingMode::HorizontalTb => {
            let x = if direction == Direction::Ltr {
                inline_start
            } else {
                container_w - inline_start - inline_size
            };
            (x, block_start, inline_size, block_size)
        }
        WritingMode::VerticalRl | WritingMode::SidewaysRl => {
            // Block axis = x (right-to-left). Inline axis = y.
            let x = container_w - block_start - block_size;
            let y = if direction == Direction::Ltr {
                inline_start
            } else {
                container_h - inline_start - inline_size
            };
            (x, y, block_size, inline_size)
        }
        WritingMode::VerticalLr | WritingMode::SidewaysLr => {
            let x = block_start;
            let y = if direction == Direction::Ltr {
                inline_start
            } else {
                container_h - inline_start - inline_size
            };
            (x, y, block_size, inline_size)
        }
    }
}

/// Physical-to-logical inverse mapping.
pub fn physical_to_logical_size(writing_mode: WritingMode, width: f32, height: f32) -> (f32, f32) {
    if writing_mode.is_vertical() {
        // inline = height, block = width
        (height, width)
    } else {
        (width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_modes() {
        assert_eq!(WritingMode::parse("vertical-rl"), WritingMode::VerticalRl);
        assert_eq!(WritingMode::parse("garbage"), WritingMode::HorizontalTb);
    }

    #[test]
    fn vertical_detection() {
        assert!(!WritingMode::HorizontalTb.is_vertical());
        assert!(WritingMode::VerticalRl.is_vertical());
        assert!(WritingMode::VerticalLr.is_vertical());
    }

    #[test]
    fn horiz_ltr_passes_through() {
        let (x, y, w, h) = logical_to_physical(
            WritingMode::HorizontalTb, Direction::Ltr,
            10.0, 20.0, 100.0, 50.0,
            1000.0, 800.0,
        );
        assert_eq!((x, y, w, h), (10.0, 20.0, 100.0, 50.0));
    }

    #[test]
    fn horiz_rtl_reflects_x() {
        let (x, _, w, _) = logical_to_physical(
            WritingMode::HorizontalTb, Direction::Rtl,
            10.0, 0.0, 100.0, 50.0,
            1000.0, 800.0,
        );
        assert_eq!(x, 890.0);
        assert_eq!(w, 100.0);
    }

    #[test]
    fn vertical_rl_swaps_axes() {
        let (x, y, w, h) = logical_to_physical(
            WritingMode::VerticalRl, Direction::Ltr,
            0.0, 0.0, 100.0, 50.0,
            1000.0, 800.0,
        );
        // Block axis (x) starts at container_w - block_size = 950, inline (y) starts at 0
        assert_eq!(x, 950.0);
        assert_eq!(y, 0.0);
        assert_eq!(w, 50.0);
        assert_eq!(h, 100.0);
    }

    #[test]
    fn vertical_lr_block_left_aligned() {
        let (x, y, w, h) = logical_to_physical(
            WritingMode::VerticalLr, Direction::Ltr,
            0.0, 30.0, 100.0, 50.0,
            1000.0, 800.0,
        );
        assert_eq!(x, 30.0);
        assert_eq!(y, 0.0);
        assert_eq!(w, 50.0);
        assert_eq!(h, 100.0);
    }

    #[test]
    fn size_swap_in_vertical() {
        let (inline, block) = physical_to_logical_size(WritingMode::VerticalRl, 100.0, 200.0);
        assert_eq!(inline, 200.0);
        assert_eq!(block, 100.0);
    }
}
