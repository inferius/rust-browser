//! JPEG XL (.jxl) dekoder - pure-Rust pres `jxl-oxide`.
//!
//! Spec: ISO/IEC 18181-1 (JPEG XL).
//! Browser sam dekoduje - bez system deps.

#[derive(Debug, Clone)]
pub struct JxlDecodeError(pub String);

/// Dekoduj JPEG XL bytes do RGBA8 + dims.
pub fn decode(data: &[u8]) -> Result<(u32, u32, Vec<u8>), JxlDecodeError> {
    let mut image = jxl_oxide::JxlImage::builder()
        .read(std::io::Cursor::new(data))
        .map_err(|e| JxlDecodeError(format!("jxl read failed: {:?}", e)))?;
    let header = image.image_header();
    let w = header.size.width;
    let h = header.size.height;
    let frame = image.render_frame(0)
        .map_err(|e| JxlDecodeError(format!("jxl render failed: {:?}", e)))?;
    let stream = frame.stream();
    let channels = stream.channels() as usize;
    let mut samples: Vec<f32> = vec![0.0; (w as usize) * (h as usize) * channels];
    let mut stream = stream;
    stream.write_to_buffer(&mut samples);
    // Convert f32 [0,1] to u8 [0,255] RGBA. If only 3 channels (RGB), pad alpha=255.
    let mut rgba: Vec<u8> = Vec::with_capacity((w as usize) * (h as usize) * 4);
    let pixels = (w as usize) * (h as usize);
    for i in 0..pixels {
        let base = i * channels;
        let r = (samples[base].clamp(0.0, 1.0) * 255.0) as u8;
        let g = if channels >= 2 { (samples[base + 1].clamp(0.0, 1.0) * 255.0) as u8 } else { r };
        let b = if channels >= 3 { (samples[base + 2].clamp(0.0, 1.0) * 255.0) as u8 } else { r };
        let a = if channels >= 4 { (samples[base + 3].clamp(0.0, 1.0) * 255.0) as u8 } else { 255 };
        rgba.push(r); rgba.push(g); rgba.push(b); rgba.push(a);
    }
    Ok((w, h, rgba))
}

/// Magic byte detection per ISO/IEC 18181: codestream nebo box container.
pub fn is_jxl(data: &[u8]) -> bool {
    // Codestream: 0xFF 0x0A
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0x0A { return true; }
    // ISO BMFF box: 0x00 0x00 0x00 0x0C "JXL " 0x0D 0x0A 0x87 0x0A
    if data.len() >= 12 && data[..12] == [0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A] {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_jxl() {
        let png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert!(!is_jxl(&png));
    }

    #[test]
    fn detects_codestream() {
        let buf = [0xFF, 0x0A, 0, 0];
        assert!(is_jxl(&buf));
    }

    #[test]
    fn detects_box_container() {
        let buf = [0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A];
        assert!(is_jxl(&buf));
    }

    #[test]
    fn decode_invalid_returns_error() {
        let garbage = [0u8; 100];
        assert!(decode(&garbage).is_err());
    }
}
