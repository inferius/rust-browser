//! SVG <filter> primitives - feGaussianBlur, feColorMatrix, feMerge, ...

#[derive(Debug, Clone)]
pub enum SvgFilterPrimitive {
    GaussianBlur { std_dev_x: f32, std_dev_y: f32, in_id: String, result_id: String },
    ColorMatrix { matrix: [f32; 20], in_id: String, result_id: String },
    Offset { dx: f32, dy: f32, in_id: String, result_id: String },
    Merge { input_ids: Vec<String>, result_id: String },
    Composite { operator: CompositeOp, in_id: String, in2_id: String, result_id: String },
    Flood { color_rgba: [u8; 4], result_id: String },
    ComponentTransfer { funcs: Vec<TransferFunc>, in_id: String, result_id: String },
    Turbulence { base_freq_x: f32, base_freq_y: f32, num_octaves: u8, seed: i32, result_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompositeOp {
    Over,
    In,
    Out,
    Atop,
    Xor,
    Arithmetic(f32, f32, f32, f32),
}

#[derive(Debug, Clone)]
pub enum TransferFunc {
    Identity,
    Table(Vec<f32>),
    Discrete(Vec<f32>),
    Linear { slope: f32, intercept: f32 },
    Gamma { amplitude: f32, exponent: f32, offset: f32 },
}

#[derive(Debug, Clone, Default)]
pub struct SvgFilter {
    pub id: String,
    pub x: f32, pub y: f32, pub width: f32, pub height: f32,
    pub primitives: Vec<SvgFilterPrimitive>,
    pub filter_units: FilterUnits,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

impl Default for FilterUnits {
    fn default() -> Self { FilterUnits::ObjectBoundingBox }
}

/// Identity color matrix (no change).
pub fn identity_color_matrix() -> [f32; 20] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

/// Apply a color matrix to RGBA channels.
pub fn apply_color_matrix(rgba: [f32; 4], m: &[f32; 20]) -> [f32; 4] {
    let r = m[0] * rgba[0] + m[1] * rgba[1] + m[2] * rgba[2] + m[3] * rgba[3] + m[4];
    let g = m[5] * rgba[0] + m[6] * rgba[1] + m[7] * rgba[2] + m[8] * rgba[3] + m[9];
    let b = m[10] * rgba[0] + m[11] * rgba[1] + m[12] * rgba[2] + m[13] * rgba[3] + m[14];
    let a = m[15] * rgba[0] + m[16] * rgba[1] + m[17] * rgba[2] + m[18] * rgba[3] + m[19];
    [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a.clamp(0.0, 1.0)]
}

/// Saturate matrix per spec: takes 0..1.
pub fn saturate_matrix(s: f32) -> [f32; 20] {
    let s = s.max(0.0);
    [
        0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s, 0.0, 0.0,
        0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s, 0.0, 0.0,
        0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_leaves_color_unchanged() {
        let m = identity_color_matrix();
        let out = apply_color_matrix([0.5, 0.4, 0.3, 1.0], &m);
        assert!((out[0] - 0.5).abs() < 0.001);
        assert!((out[1] - 0.4).abs() < 0.001);
        assert!((out[2] - 0.3).abs() < 0.001);
        assert!((out[3] - 1.0).abs() < 0.001);
    }

    #[test]
    fn saturate_zero_is_grayscale() {
        let m = saturate_matrix(0.0);
        let red = apply_color_matrix([1.0, 0.0, 0.0, 1.0], &m);
        let green = apply_color_matrix([0.0, 1.0, 0.0, 1.0], &m);
        // Per Rec. 709: 0.213 from R, 0.715 from G
        assert!((red[0] - 0.213).abs() < 0.001);
        assert!((green[0] - 0.715).abs() < 0.001);
    }

    #[test]
    fn saturate_one_is_identity_chrome() {
        let m = saturate_matrix(1.0);
        let out = apply_color_matrix([0.5, 0.5, 0.5, 1.0], &m);
        assert!((out[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn clamps_overflow() {
        let m = [2.0, 0.0, 0.0, 0.0, 0.0,
                 0.0, 1.0, 0.0, 0.0, 0.0,
                 0.0, 0.0, 1.0, 0.0, 0.0,
                 0.0, 0.0, 0.0, 1.0, 0.0];
        let out = apply_color_matrix([1.0, 0.5, 0.5, 1.0], &m);
        assert_eq!(out[0], 1.0);
    }
}
