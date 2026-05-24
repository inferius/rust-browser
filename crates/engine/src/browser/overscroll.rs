//! Overscroll behavior + scroll chaining + rubber-band visuals.
//!
//! Spec: https://drafts.csswg.org/css-overscroll-1/
//! overscroll-behavior: auto | contain | none.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverscrollBehavior {
    Auto,       // chain to ancestor + perform glow/bounce
    Contain,    // chain blocked; show effect at this boundary
    None,       // no glow, no chain
}

impl OverscrollBehavior {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "contain" => Self::Contain,
            "none" => Self::None,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollAttempt {
    pub delta_x: f32,
    pub delta_y: f32,
    pub current_scroll_x: f32,
    pub current_scroll_y: f32,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
    pub behavior_x: OverscrollBehavior,
    pub behavior_y: OverscrollBehavior,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollResult {
    pub consumed_x: f32,
    pub consumed_y: f32,
    pub remainder_x: f32,
    pub remainder_y: f32,
    pub at_boundary_x: bool,
    pub at_boundary_y: bool,
}

pub fn handle(attempt: ScrollAttempt) -> ScrollResult {
    let new_x = (attempt.current_scroll_x + attempt.delta_x).clamp(0.0, attempt.max_scroll_x);
    let new_y = (attempt.current_scroll_y + attempt.delta_y).clamp(0.0, attempt.max_scroll_y);
    let consumed_x = new_x - attempt.current_scroll_x;
    let consumed_y = new_y - attempt.current_scroll_y;
    let raw_rem_x = attempt.delta_x - consumed_x;
    let raw_rem_y = attempt.delta_y - consumed_y;
    let at_boundary_x = raw_rem_x.abs() > 0.0001;
    let at_boundary_y = raw_rem_y.abs() > 0.0001;
    // Remainder chained only if behavior=Auto.
    let chain_x = if attempt.behavior_x == OverscrollBehavior::Auto { raw_rem_x } else { 0.0 };
    let chain_y = if attempt.behavior_y == OverscrollBehavior::Auto { raw_rem_y } else { 0.0 };
    ScrollResult {
        consumed_x,
        consumed_y,
        remainder_x: chain_x,
        remainder_y: chain_y,
        at_boundary_x,
        at_boundary_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_chains_remainder() {
        let r = handle(ScrollAttempt {
            delta_x: 0.0, delta_y: 100.0,
            current_scroll_x: 0.0, current_scroll_y: 1000.0,
            max_scroll_x: 0.0, max_scroll_y: 1000.0,
            behavior_x: OverscrollBehavior::Auto,
            behavior_y: OverscrollBehavior::Auto,
        });
        assert_eq!(r.consumed_y, 0.0);
        assert_eq!(r.remainder_y, 100.0);
        assert!(r.at_boundary_y);
    }

    #[test]
    fn contain_swallows_remainder() {
        let r = handle(ScrollAttempt {
            delta_x: 0.0, delta_y: 100.0,
            current_scroll_x: 0.0, current_scroll_y: 1000.0,
            max_scroll_x: 0.0, max_scroll_y: 1000.0,
            behavior_x: OverscrollBehavior::Contain,
            behavior_y: OverscrollBehavior::Contain,
        });
        assert_eq!(r.remainder_y, 0.0);
        assert!(r.at_boundary_y);
    }

    #[test]
    fn within_bounds_consumes_full() {
        let r = handle(ScrollAttempt {
            delta_x: 0.0, delta_y: 50.0,
            current_scroll_x: 0.0, current_scroll_y: 100.0,
            max_scroll_x: 0.0, max_scroll_y: 1000.0,
            behavior_x: OverscrollBehavior::Auto,
            behavior_y: OverscrollBehavior::Auto,
        });
        assert_eq!(r.consumed_y, 50.0);
        assert!(!r.at_boundary_y);
    }

    #[test]
    fn parse_behavior() {
        assert_eq!(OverscrollBehavior::parse("contain"), OverscrollBehavior::Contain);
        assert_eq!(OverscrollBehavior::parse("none"), OverscrollBehavior::None);
        assert_eq!(OverscrollBehavior::parse("auto"), OverscrollBehavior::Auto);
        assert_eq!(OverscrollBehavior::parse("garbage"), OverscrollBehavior::Auto);
    }
}
