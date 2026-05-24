//! SVG gradients - linearGradient / radialGradient (referenced via fill="url(#id)").

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpreadMethod {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

#[derive(Debug, Clone)]
pub struct GradientStop {
    pub offset: f32,         // 0..1
    pub color_rgba: [u8; 4],
    pub stop_opacity: f32,
}

#[derive(Debug, Clone)]
pub struct LinearGradient {
    pub id: String,
    pub x1: f32, pub y1: f32,
    pub x2: f32, pub y2: f32,
    pub stops: Vec<GradientStop>,
    pub spread: SpreadMethod,
    pub units: GradientUnits,
    pub transform: [f32; 6],
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            id: String::new(),
            x1: 0.0, y1: 0.0, x2: 1.0, y2: 0.0,
            stops: Vec::new(),
            spread: SpreadMethod::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone)]
pub struct RadialGradient {
    pub id: String,
    pub cx: f32, pub cy: f32, pub r: f32,
    pub fx: Option<f32>, pub fy: Option<f32>, pub fr: f32,
    pub stops: Vec<GradientStop>,
    pub spread: SpreadMethod,
    pub units: GradientUnits,
    pub transform: [f32; 6],
}

impl Default for RadialGradient {
    fn default() -> Self {
        Self {
            id: String::new(),
            cx: 0.5, cy: 0.5, r: 0.5,
            fx: None, fy: None, fr: 0.0,
            stops: Vec::new(),
            spread: SpreadMethod::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

/// Sample gradient at 0..1.
pub fn sample_linear(stops: &[GradientStop], t: f32) -> [u8; 4] {
    if stops.is_empty() { return [0, 0, 0, 255]; }
    if t <= stops[0].offset { return stops[0].color_rgba; }
    for w in stops.windows(2) {
        if t >= w[0].offset && t <= w[1].offset {
            let span = (w[1].offset - w[0].offset).max(1e-6);
            let r = (t - w[0].offset) / span;
            return blend(w[0].color_rgba, w[1].color_rgba, r);
        }
    }
    stops.last().unwrap().color_rgba
}

fn blend(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        ((a[0] as f32) * (1.0 - t) + (b[0] as f32) * t) as u8,
        ((a[1] as f32) * (1.0 - t) + (b[1] as f32) * t) as u8,
        ((a[2] as f32) * (1.0 - t) + (b[2] as f32) * t) as u8,
        ((a[3] as f32) * (1.0 - t) + (b[3] as f32) * t) as u8,
    ]
}

/// Apply spread method to map a t value outside [0,1] back into the gradient.
pub fn apply_spread(t: f32, spread: SpreadMethod) -> f32 {
    if (0.0..=1.0).contains(&t) { return t; }
    match spread {
        SpreadMethod::Pad => t.clamp(0.0, 1.0),
        SpreadMethod::Repeat => t - t.floor(),
        SpreadMethod::Reflect => {
            let f = t.abs();
            let period = (f as i32) % 2;
            let frac = f - f.floor();
            if period == 0 { frac } else { 1.0 - frac }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops() -> Vec<GradientStop> {
        vec![
            GradientStop { offset: 0.0, color_rgba: [255, 0, 0, 255], stop_opacity: 1.0 },
            GradientStop { offset: 1.0, color_rgba: [0, 0, 255, 255], stop_opacity: 1.0 },
        ]
    }

    #[test]
    fn sample_at_endpoints() {
        let s = stops();
        assert_eq!(sample_linear(&s, 0.0), [255, 0, 0, 255]);
        assert_eq!(sample_linear(&s, 1.0), [0, 0, 255, 255]);
    }

    #[test]
    fn sample_midway() {
        let s = stops();
        let c = sample_linear(&s, 0.5);
        // Mix: r 127, b 127
        assert!((c[0] as i32 - 127).abs() <= 2);
        assert!((c[2] as i32 - 127).abs() <= 2);
    }

    #[test]
    fn spread_pad_clamps() {
        assert_eq!(apply_spread(-0.5, SpreadMethod::Pad), 0.0);
        assert_eq!(apply_spread(1.5, SpreadMethod::Pad), 1.0);
    }

    #[test]
    fn spread_repeat() {
        let v = apply_spread(2.3, SpreadMethod::Repeat);
        assert!((v - 0.3).abs() < 0.01);
    }

    #[test]
    fn spread_reflect() {
        let v = apply_spread(1.3, SpreadMethod::Reflect);
        // period = 1, frac = 0.3 -> 1 - 0.3 = 0.7
        assert!((v - 0.7).abs() < 0.01);
    }
}
