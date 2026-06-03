//! CSS Floats L1 - left/right float positioning + clear.
//!
//! Spec: https://drafts.csswg.org/css2/#floats
//! Foundation: float context tracking + line box adjustment.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatSide {
    None,
    Left,
    Right,
    InlineStart,  // logical (RTL aware)
    InlineEnd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClearSide {
    None,
    Left,
    Right,
    Both,
    InlineStart,
    InlineEnd,
}

impl FloatSide {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "left" => Self::Left,
            "right" => Self::Right,
            "inline-start" => Self::InlineStart,
            "inline-end" => Self::InlineEnd,
            _ => Self::None,
        }
    }
}

impl ClearSide {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "left" => Self::Left,
            "right" => Self::Right,
            "both" => Self::Both,
            "inline-start" => Self::InlineStart,
            "inline-end" => Self::InlineEnd,
            _ => Self::None,
        }
    }
}

/// Float container - tracking left + right floats stack v BFC.
#[derive(Default, Debug)]
pub struct FloatContext {
    pub left_floats: Vec<FloatBox>,
    pub right_floats: Vec<FloatBox>,
    pub container_width: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct FloatBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl FloatContext {
    pub fn new(container_width: f32) -> Self {
        Self {
            left_floats: Vec::new(),
            right_floats: Vec::new(),
            container_width,
        }
    }

    /// Find next vertical position kde se float fits.
    pub fn place_float(&mut self, side: FloatSide, width: f32, height: f32, top: f32) -> FloatBox {
        let mut y = top;
        loop {
            let (left_at_y, right_at_y) = self.bounds_at_y(y);
            let available = (right_at_y - left_at_y).max(0.0);
            if available >= width {
                let x = match side {
                    FloatSide::Left | FloatSide::InlineStart => left_at_y,
                    FloatSide::Right | FloatSide::InlineEnd => right_at_y - width,
                    FloatSide::None => left_at_y,
                };
                let fb = FloatBox { x, y, width, height };
                match side {
                    FloatSide::Left | FloatSide::InlineStart => self.left_floats.push(fb),
                    FloatSide::Right | FloatSide::InlineEnd => self.right_floats.push(fb),
                    FloatSide::None => {}
                }
                return fb;
            }
            // Move y down past next float edge.
            y = self.next_clear_y(y);
            if y > 1e6 { return FloatBox { x: 0.0, y: top, width, height }; }
        }
    }

    /// Bounds (left, right) v dany y - exclusion zones z floats.
    pub fn bounds_at_y(&self, y: f32) -> (f32, f32) {
        let mut left = 0.0_f32;
        for f in &self.left_floats {
            if y >= f.y && y < f.y + f.height {
                let right_edge = f.x + f.width;
                if right_edge > left { left = right_edge; }
            }
        }
        let mut right = self.container_width;
        for f in &self.right_floats {
            if y >= f.y && y < f.y + f.height {
                if f.x < right { right = f.x; }
            }
        }
        (left, right)
    }

    fn next_clear_y(&self, current_y: f32) -> f32 {
        let mut next = f32::INFINITY;
        for f in self.left_floats.iter().chain(self.right_floats.iter()) {
            let edge = f.y + f.height;
            if edge > current_y && edge < next { next = edge; }
        }
        next
    }

    /// Apply clear - posunout pozici dolu pod relevantni floats.
    pub fn apply_clear(&self, clear: ClearSide, top: f32) -> f32 {
        let mut y = top;
        match clear {
            ClearSide::Left | ClearSide::InlineStart => {
                for f in &self.left_floats {
                    let edge = f.y + f.height;
                    if edge > y { y = edge; }
                }
            }
            ClearSide::Right | ClearSide::InlineEnd => {
                for f in &self.right_floats {
                    let edge = f.y + f.height;
                    if edge > y { y = edge; }
                }
            }
            ClearSide::Both => {
                for f in self.left_floats.iter().chain(self.right_floats.iter()) {
                    let edge = f.y + f.height;
                    if edge > y { y = edge; }
                }
            }
            ClearSide::None => {}
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_left_float_at_top() {
        let mut ctx = FloatContext::new(300.0);
        let f = ctx.place_float(FloatSide::Left, 100.0, 50.0, 0.0);
        assert_eq!(f.x, 0.0);
        assert_eq!(f.y, 0.0);
    }

    #[test]
    fn place_right_float() {
        let mut ctx = FloatContext::new(300.0);
        let f = ctx.place_float(FloatSide::Right, 100.0, 50.0, 0.0);
        assert_eq!(f.x, 200.0);
    }

    #[test]
    fn stacked_left_floats_advance_y() {
        let mut ctx = FloatContext::new(300.0);
        ctx.place_float(FloatSide::Left, 200.0, 50.0, 0.0); // takes most width
        let f = ctx.place_float(FloatSide::Left, 200.0, 50.0, 0.0);
        // Second float can't fit beside, so wraps to y=50.
        assert_eq!(f.y, 50.0);
    }

    #[test]
    fn bounds_excluded_by_float() {
        let mut ctx = FloatContext::new(300.0);
        ctx.place_float(FloatSide::Left, 100.0, 50.0, 0.0);
        let (l, r) = ctx.bounds_at_y(25.0);
        assert_eq!(l, 100.0);
        assert_eq!(r, 300.0);
    }

    #[test]
    fn clear_both_skips_all() {
        let mut ctx = FloatContext::new(300.0);
        ctx.place_float(FloatSide::Left, 100.0, 100.0, 0.0);
        ctx.place_float(FloatSide::Right, 100.0, 200.0, 0.0);
        let y = ctx.apply_clear(ClearSide::Both, 0.0);
        assert_eq!(y, 200.0);
    }

    #[test]
    fn parse_float_keywords() {
        assert_eq!(FloatSide::parse("left"), FloatSide::Left);
        assert_eq!(FloatSide::parse("none"), FloatSide::None);
        assert_eq!(ClearSide::parse("both"), ClearSide::Both);
    }
}
