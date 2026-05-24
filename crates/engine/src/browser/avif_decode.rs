//! AVIF dekoder - pure-Rust pres `zenavif` crate (rav1d AV1 decoder port).
//!
//! Spec: ISO/IEC 23000-22 (AVIF) + AV1 bitstream (AOMediaCodec).
//! Browser sam dekoduje - bez system libdav1d, bez NASM, bez external installs.
//!
//! Returns (width, height, rgba8_bytes). rgba8_bytes je RGBA8888 v row-major
//! order (= co `image::ImageBuffer::into_raw()` produces).

#[derive(Debug, Clone)]
pub struct AvifDecodeError(pub String);

/// Dekoduj AVIF bytes do RGBA8 + dims.
///
/// Pri uspechu vrati (width, height, rgba_bytes) kde rgba_bytes.len() == w*h*4.
pub fn decode(data: &[u8]) -> Result<(u32, u32, Vec<u8>), AvifDecodeError> {
    use zenpixels_convert::PixelBufferConvertTypedExt;
    let buffer = zenavif::decode(data)
        .map_err(|e| AvifDecodeError(format!("zenavif decode error: {:?}", e)))?;
    let rgba = buffer.to_rgba8();
    let w = rgba.width();
    let h = rgba.height();
    let bytes = rgba.copy_to_contiguous_bytes();
    Ok((w, h, bytes))
}

/// Magic-byte sniff pro AVIF. ISO BMFF brand "ftypavif" v box[4..12].
pub fn is_avif(data: &[u8]) -> bool {
    data.len() >= 12 && &data[4..8] == b"ftyp" && &data[8..12] == b"avif"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_avif() {
        let png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert!(!is_avif(&png));
    }

    #[test]
    fn detects_avif_brand() {
        let mut buf = vec![0u8, 0, 0, 0x20];
        buf.extend_from_slice(b"ftypavif");
        assert!(is_avif(&buf));
    }

    #[test]
    fn decode_invalid_returns_error() {
        let garbage = [0u8; 100];
        assert!(decode(&garbage).is_err());
    }

    #[test]
    fn decode_truncated_returns_error() {
        let truncated = [0, 0, 0, 0x20, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f'];
        assert!(decode(&truncated).is_err());
    }
}
