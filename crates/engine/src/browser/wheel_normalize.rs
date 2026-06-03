//! Mouse wheel delta normalization across OS / mice / trackpads.
//!
//! WheelEvent.deltaMode: PIXEL (0), LINE (1), PAGE (2).
//! On Windows, raw wheel reports lines * WHEEL_DELTA (120) per notch.
//! On Mac, magic mice/trackpads send sub-pixel pixel deltas.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeltaMode {
    Pixel,
    Line,
    Page,
}

#[derive(Debug, Clone, Copy)]
pub struct RawWheel {
    pub delta_x: f32,
    pub delta_y: f32,
    pub delta_mode: DeltaMode,
    pub from_trackpad: bool,
    pub line_height_px: f32,            // default 16
    pub page_height_px: f32,            // viewport height
}

#[derive(Debug, Clone, Copy)]
pub struct NormalizedWheel {
    pub pixel_dx: f32,
    pub pixel_dy: f32,
    pub kinetic: bool,                  // true if part of momentum scroll
    pub accelerated: bool,
}

pub fn normalize(input: RawWheel) -> NormalizedWheel {
    let (mut dx, mut dy) = (input.delta_x, input.delta_y);
    match input.delta_mode {
        DeltaMode::Pixel => {}
        DeltaMode::Line => {
            dx *= input.line_height_px;
            dy *= input.line_height_px;
        }
        DeltaMode::Page => {
            dx *= input.page_height_px;
            dy *= input.page_height_px;
        }
    }
    // Trackpads: usually already pixel deltas with momentum tail.
    let kinetic = input.from_trackpad && dy.abs() < 5.0;
    let accelerated = !input.from_trackpad && dy.abs() > 100.0;
    NormalizedWheel { pixel_dx: dx, pixel_dy: dy, kinetic, accelerated }
}

/// Apply a non-linear acceleration curve to mouse wheel deltas.
pub fn accelerate(pixel_dy: f32, multiplier: f32) -> f32 {
    if pixel_dy.abs() < 1.0 { return pixel_dy; }
    let sign = pixel_dy.signum();
    let abs = pixel_dy.abs();
    sign * abs.powf(1.1) * multiplier
}

/// Smooth scroll using time-based decay; returns reduced velocity for next frame.
pub fn velocity_decay(velocity: f32, dt_sec: f32, friction: f32) -> f32 {
    velocity * (1.0 - friction * dt_sec).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_mode_passthrough() {
        let r = normalize(RawWheel {
            delta_x: 0.0, delta_y: 50.0,
            delta_mode: DeltaMode::Pixel,
            from_trackpad: false,
            line_height_px: 16.0, page_height_px: 600.0,
        });
        assert_eq!(r.pixel_dy, 50.0);
    }

    #[test]
    fn line_mode_multiplies() {
        let r = normalize(RawWheel {
            delta_x: 0.0, delta_y: 3.0,
            delta_mode: DeltaMode::Line,
            from_trackpad: false,
            line_height_px: 20.0, page_height_px: 600.0,
        });
        assert_eq!(r.pixel_dy, 60.0);
    }

    #[test]
    fn page_mode_multiplies() {
        let r = normalize(RawWheel {
            delta_x: 0.0, delta_y: 1.0,
            delta_mode: DeltaMode::Page,
            from_trackpad: false,
            line_height_px: 16.0, page_height_px: 800.0,
        });
        assert_eq!(r.pixel_dy, 800.0);
    }

    #[test]
    fn kinetic_detected() {
        let r = normalize(RawWheel {
            delta_x: 0.0, delta_y: 2.0,
            delta_mode: DeltaMode::Pixel,
            from_trackpad: true,
            line_height_px: 16.0, page_height_px: 600.0,
        });
        assert!(r.kinetic);
    }

    #[test]
    fn acceleration_curve() {
        let v = accelerate(100.0, 1.0);
        assert!(v > 100.0);
        // Sign preserved
        assert!(accelerate(-100.0, 1.0) < 0.0);
    }

    #[test]
    fn velocity_decays() {
        let v0 = 1000.0;
        let v1 = velocity_decay(v0, 0.1, 1.0);
        assert!(v1 < v0);
        // After enough time, velocity collapses to 0.
        let v2 = velocity_decay(v0, 10.0, 1.0);
        assert_eq!(v2, 0.0);
    }
}
