//! CSS Transitions Level 1 + Animations Level 1 shared timing primitives.
//!
//! Spec: https://www.w3.org/TR/css-transitions-1/
//!       https://www.w3.org/TR/css-animations-1/
//! Cubic-bezier easing solver per CSS Easing 1.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EasingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    Steps(u32, StepPosition),
    CubicBezier(f32, f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepPosition {
    JumpStart,
    JumpEnd,
    JumpNone,
    JumpBoth,
}

impl EasingFunction {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        Some(match s.to_ascii_lowercase().as_str() {
            "linear" => Self::Linear,
            "ease" => Self::Ease,
            "ease-in" => Self::EaseIn,
            "ease-out" => Self::EaseOut,
            "ease-in-out" => Self::EaseInOut,
            x if x.starts_with("cubic-bezier(") => {
                let inner = x.trim_start_matches("cubic-bezier(").trim_end_matches(')');
                let parts: Vec<f32> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if parts.len() != 4 { return None; }
                Self::CubicBezier(parts[0], parts[1], parts[2], parts[3])
            }
            x if x.starts_with("steps(") => {
                let inner = x.trim_start_matches("steps(").trim_end_matches(')');
                let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
                let n: u32 = parts.first()?.parse().ok()?;
                let pos = match parts.get(1).copied() {
                    Some("jump-start") | Some("start") => StepPosition::JumpStart,
                    Some("jump-none") => StepPosition::JumpNone,
                    Some("jump-both") => StepPosition::JumpBoth,
                    _ => StepPosition::JumpEnd,
                };
                Self::Steps(n, pos)
            }
            _ => return None,
        })
    }

    /// Apply easing to t in [0,1].
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => cubic_bezier_y(t, 0.25, 0.1, 0.25, 1.0),
            Self::EaseIn => cubic_bezier_y(t, 0.42, 0.0, 1.0, 1.0),
            Self::EaseOut => cubic_bezier_y(t, 0.0, 0.0, 0.58, 1.0),
            Self::EaseInOut => cubic_bezier_y(t, 0.42, 0.0, 0.58, 1.0),
            Self::CubicBezier(x1, y1, x2, y2) => cubic_bezier_y(t, *x1, *y1, *x2, *y2),
            Self::Steps(n, pos) => {
                let n = (*n as f32).max(1.0);
                let mut step = (t * n).floor();
                match pos {
                    StepPosition::JumpStart => { step += 1.0; }
                    StepPosition::JumpEnd => {}
                    StepPosition::JumpNone => { step = (t * (n - 1.0)).round(); }
                    StepPosition::JumpBoth => { step += 1.0; }
                }
                let divisor = match pos {
                    StepPosition::JumpNone => (n - 1.0).max(1.0),
                    StepPosition::JumpBoth => n + 1.0,
                    _ => n,
                };
                (step / divisor).clamp(0.0, 1.0)
            }
        }
    }
}

/// Solve cubic-bezier control(x1,y1)(x2,y2) for y given x via Newton-Raphson.
pub fn cubic_bezier_y(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    fn bezier(t: f32, p1: f32, p2: f32) -> f32 {
        let it = 1.0 - t;
        3.0 * it * it * t * p1 + 3.0 * it * t * t * p2 + t * t * t
    }
    fn dbezier(t: f32, p1: f32, p2: f32) -> f32 {
        let it = 1.0 - t;
        3.0 * it * it * p1 + 6.0 * it * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
    }
    // Newton iteration on t such that bezier(t, x1, x2) == x.
    let mut t = x;
    for _ in 0..8 {
        let fx = bezier(t, x1, x2) - x;
        let dfx = dbezier(t, x1, x2);
        if dfx.abs() < 1e-6 { break; }
        t -= fx / dfx;
        t = t.clamp(0.0, 1.0);
        if fx.abs() < 1e-5 { break; }
    }
    bezier(t, y1, y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_linear() {
        assert_eq!(EasingFunction::parse("linear"), Some(EasingFunction::Linear));
    }

    #[test]
    fn parse_cubic_bezier() {
        let e = EasingFunction::parse("cubic-bezier(0.25, 0.1, 0.25, 1.0)").unwrap();
        match e {
            EasingFunction::CubicBezier(a, b, c, d) => {
                assert!((a - 0.25).abs() < 0.001);
                assert!((b - 0.1).abs() < 0.001);
                assert!((c - 0.25).abs() < 0.001);
                assert!((d - 1.0).abs() < 0.001);
            }
            _ => panic!("expected cubic-bezier"),
        }
    }

    #[test]
    fn linear_identity() {
        assert!((EasingFunction::Linear.apply(0.5) - 0.5).abs() < 0.001);
    }

    #[test]
    fn ease_in_starts_slow() {
        let mid = EasingFunction::EaseIn.apply(0.5);
        assert!(mid < 0.5);
    }

    #[test]
    fn ease_out_ends_slow() {
        let mid = EasingFunction::EaseOut.apply(0.5);
        assert!(mid > 0.5);
    }

    #[test]
    fn endpoints_preserved() {
        for e in &[EasingFunction::Linear, EasingFunction::Ease, EasingFunction::EaseIn,
                   EasingFunction::EaseOut, EasingFunction::EaseInOut] {
            assert!(e.apply(0.0).abs() < 0.01);
            assert!((e.apply(1.0) - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn steps_jump_end() {
        let e = EasingFunction::Steps(4, StepPosition::JumpEnd);
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(0.99), 0.75);
        assert_eq!(e.apply(1.0), 1.0);
    }
}
