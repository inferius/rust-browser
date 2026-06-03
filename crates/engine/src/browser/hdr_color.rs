//! HDR / wide-gamut color management foundation.
//!
//! CSS Color L4: display-p3, rec2020, oklab, lab, lch. wgpu color space conversion.
//! Spec: https://www.w3.org/TR/css-color-4/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorSpace {
    Srgb,
    DisplayP3,
    Rec2020,
    A98Rgb,
    ProphotoRgb,
    XyzD50,
    XyzD65,
    OklabSpace,
    LabSpace,
    OklchSpace,
    LchSpace,
}

impl ColorSpace {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "srgb" | "srgb-linear" => Some(Self::Srgb),
            "display-p3" => Some(Self::DisplayP3),
            "rec2020" => Some(Self::Rec2020),
            "a98-rgb" => Some(Self::A98Rgb),
            "prophoto-rgb" => Some(Self::ProphotoRgb),
            "xyz" | "xyz-d65" => Some(Self::XyzD65),
            "xyz-d50" => Some(Self::XyzD50),
            "oklab" => Some(Self::OklabSpace),
            "lab" => Some(Self::LabSpace),
            "oklch" => Some(Self::OklchSpace),
            "lch" => Some(Self::LchSpace),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WideColor {
    pub space: ColorSpace,
    pub components: [f32; 3],
    pub alpha: f32,
}

impl WideColor {
    pub fn srgb(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { space: ColorSpace::Srgb, components: [r, g, b], alpha: a }
    }

    /// Convert to sRGB linear [0..1]. Foundation - identity for sRGB.
    /// Real conversion pres matrix transformations (display-p3 -> sRGB lossy
    /// pri out-of-gamut colors).
    pub fn to_srgb_linear(&self) -> [f32; 4] {
        match self.space {
            ColorSpace::Srgb => [self.components[0], self.components[1], self.components[2], self.alpha],
            ColorSpace::DisplayP3 => {
                // P3 -> sRGB approx matrix (real: D65 reference white).
                let r = self.components[0];
                let g = self.components[1];
                let b = self.components[2];
                let sr = (1.225 * r - 0.225 * g + 0.0 * b).clamp(0.0, 1.0);
                let sg = (-0.042 * r + 1.042 * g + 0.0 * b).clamp(0.0, 1.0);
                let sb = (-0.020 * r - 0.079 * g + 1.099 * b).clamp(0.0, 1.0);
                [sr, sg, sb, self.alpha]
            }
            _ => [self.components[0], self.components[1], self.components[2], self.alpha],
        }
    }
}

/// Detect display HDR capability - foundation stub.
pub fn display_supports_hdr() -> bool {
    // Real: query OS - Windows DisplayConfig + DXGI HDR metadata, mac
    // CGDisplayCopyColorSpace, Linux EDID parse.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_spaces() {
        assert_eq!(ColorSpace::parse("display-p3"), Some(ColorSpace::DisplayP3));
        assert_eq!(ColorSpace::parse("rec2020"), Some(ColorSpace::Rec2020));
        assert_eq!(ColorSpace::parse("oklab"), Some(ColorSpace::OklabSpace));
    }

    #[test]
    fn srgb_identity() {
        let c = WideColor::srgb(0.5, 0.3, 0.7, 1.0);
        let lin = c.to_srgb_linear();
        assert_eq!(lin, [0.5, 0.3, 0.7, 1.0]);
    }

    #[test]
    fn p3_to_srgb_clamps_oog() {
        let c = WideColor {
            space: ColorSpace::DisplayP3,
            components: [1.0, 0.0, 0.0],
            alpha: 1.0,
        };
        let lin = c.to_srgb_linear();
        // P3 red s extended gamut -> sRGB blue mensi value. Vsechno [0..1].
        assert!(lin[0] <= 1.0 && lin[0] >= 0.0);
    }
}
