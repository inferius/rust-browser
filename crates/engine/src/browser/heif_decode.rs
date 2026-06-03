//! HEIF/HEIC dekoder - pure-Rust pres `heic` crate (H.265/HEVC SIMD).
//!
//! Spec: ISO/IEC 23008-12 (HEIF).
//! Browser sam dekoduje - bez system libheif/dav1d/openh264.

#[derive(Debug, Clone)]
pub struct HeifDecodeError(pub String);

/// Dekoduj HEIF/HEIC bytes do RGBA8 + dims.
pub fn decode(data: &[u8]) -> Result<(u32, u32, Vec<u8>), HeifDecodeError> {
    use heic::{DecoderConfig, PixelLayout};
    let output = DecoderConfig::new()
        .decode(data, PixelLayout::Rgba8)
        .map_err(|e| HeifDecodeError(format!("heic decode failed: {:?}", e)))?;
    Ok((output.width, output.height, output.data))
}

/// Magic byte detection: ISO BMFF brand "ftypheic"/"heix"/"mif1".
pub fn is_heif(data: &[u8]) -> bool {
    if data.len() < 12 || &data[4..8] != b"ftyp" { return false; }
    let brand = &data[8..12];
    brand == b"heic" || brand == b"heix" || brand == b"mif1" || brand == b"heim" || brand == b"heis"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_heif() {
        let png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert!(!is_heif(&png));
    }

    #[test]
    fn detects_heic_brand() {
        let mut buf = vec![0u8, 0, 0, 0x20];
        buf.extend_from_slice(b"ftypheic");
        assert!(is_heif(&buf));
    }

    #[test]
    fn detects_mif1_brand() {
        let mut buf = vec![0u8, 0, 0, 0x20];
        buf.extend_from_slice(b"ftypmif1");
        assert!(is_heif(&buf));
    }

    #[test]
    fn decode_invalid_returns_error() {
        let garbage = [0u8; 100];
        assert!(decode(&garbage).is_err());
    }
}
