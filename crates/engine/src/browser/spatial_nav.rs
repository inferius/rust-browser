//! Spatial Navigation - arrow-key focus movement (TV/console-style).
//!
//! Spec: https://www.w3.org/TR/css-nav-1/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavDirection { Up, Down, Left, Right }

#[derive(Debug, Clone)]
pub struct FocusableTarget {
    pub id: u64,
    pub rect: (f32, f32, f32, f32),    // x, y, w, h in viewport
    pub disabled: bool,
}

/// Pick the next focus target based on spatial geometry.
/// Algorithm: choose candidate with smallest distance + alignment score
/// in the requested direction.
pub fn next_focus(
    current_rect: (f32, f32, f32, f32),
    candidates: &[FocusableTarget],
    direction: NavDirection,
) -> Option<u64> {
    let (cx, cy) = center(current_rect);
    let mut best: Option<(&FocusableTarget, f32)> = None;
    for c in candidates {
        if c.disabled { continue; }
        if c.rect == current_rect { continue; }
        let (tx, ty) = center(c.rect);
        let dx = tx - cx;
        let dy = ty - cy;
        let in_dir = match direction {
            NavDirection::Up => dy < -1.0,
            NavDirection::Down => dy > 1.0,
            NavDirection::Left => dx < -1.0,
            NavDirection::Right => dx > 1.0,
        };
        if !in_dir { continue; }
        // Weighted distance: penalize off-axis displacement.
        let primary = match direction {
            NavDirection::Up | NavDirection::Down => dy.abs(),
            NavDirection::Left | NavDirection::Right => dx.abs(),
        };
        let secondary = match direction {
            NavDirection::Up | NavDirection::Down => dx.abs(),
            NavDirection::Left | NavDirection::Right => dy.abs(),
        };
        let score = primary + secondary * 2.0;
        if best.map(|(_, b)| score < b).unwrap_or(true) {
            best = Some((c, score));
        }
    }
    best.map(|(c, _)| c.id)
}

fn center(r: (f32, f32, f32, f32)) -> (f32, f32) {
    (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: u64, x: f32, y: f32) -> FocusableTarget {
        FocusableTarget { id, rect: (x, y, 100.0, 50.0), disabled: false }
    }

    #[test]
    fn moves_right() {
        let cur = (0.0, 0.0, 100.0, 50.0);
        let cands = vec![target(1, 200.0, 0.0), target(2, -200.0, 0.0)];
        assert_eq!(next_focus(cur, &cands, NavDirection::Right), Some(1));
    }

    #[test]
    fn moves_down() {
        let cur = (0.0, 0.0, 100.0, 50.0);
        let cands = vec![target(1, 0.0, 200.0), target(2, 0.0, -200.0)];
        assert_eq!(next_focus(cur, &cands, NavDirection::Down), Some(1));
    }

    #[test]
    fn prefers_aligned() {
        let cur = (0.0, 0.0, 100.0, 50.0);
        let aligned = target(1, 200.0, 0.0);
        let mut off = target(2, 200.0, 500.0);
        off.id = 2;
        let cands = vec![aligned, off];
        assert_eq!(next_focus(cur, &cands, NavDirection::Right), Some(1));
    }

    #[test]
    fn skips_disabled() {
        let cur = (0.0, 0.0, 100.0, 50.0);
        let mut t = target(1, 200.0, 0.0);
        t.disabled = true;
        let cands = vec![t];
        assert!(next_focus(cur, &cands, NavDirection::Right).is_none());
    }

    #[test]
    fn no_candidate_in_direction() {
        let cur = (1000.0, 0.0, 100.0, 50.0);
        let cands = vec![target(1, 500.0, 0.0)];
        assert!(next_focus(cur, &cands, NavDirection::Right).is_none());
    }
}
