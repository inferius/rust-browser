//! Unified image decoder interface - JPEG, PNG, WebP, AVIF, GIF, BMP, ICO.
//!
//! Foundation: format detection z magic bytes + dispatch table. Real decode
//! pres `image` crate (existing) + dalsi codec crates.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Avif,
    Bmp,
    Ico,
    Svg,
    Unknown,
}

/// Detekce z magic bytes (first 16 bytes typicky).
pub fn detect_format(bytes: &[u8]) -> ImageFormat {
    if bytes.len() < 4 { return ImageFormat::Unknown; }
    // PNG: 89 50 4E 47
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) { return ImageFormat::Png; }
    // JPEG: FF D8 FF
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) { return ImageFormat::Jpeg; }
    // GIF: GIF87a / GIF89a
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { return ImageFormat::Gif; }
    // WebP: "RIFF" + 4 size + "WEBP"
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" { return ImageFormat::Webp; }
    // AVIF: ftyp box s avif brand (offset 4)
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if brand == b"avif" || brand == b"avis" || brand == b"mif1" { return ImageFormat::Avif; }
    }
    // BMP: BM
    if bytes.starts_with(b"BM") { return ImageFormat::Bmp; }
    // ICO: 00 00 01 00
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) { return ImageFormat::Ico; }
    // SVG: XML / "<svg" sniff first 256 bytes.
    let head = std::str::from_utf8(&bytes[..bytes.len().min(256)]).unwrap_or("");
    if head.trim_start().starts_with("<?xml") || head.contains("<svg") { return ImageFormat::Svg; }
    ImageFormat::Unknown
}

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub format: ImageFormat,
}

/// Decode foundation dispatch. Real impl pres `image` crate `load_from_memory_with_format`.
/// Foundation: format detect + size sniff bez actual decode (vraci empty rgba).
pub fn decode(bytes: &[u8]) -> Result<DecodedImage, String> {
    let fmt = detect_format(bytes);
    if fmt == ImageFormat::Unknown {
        return Err("unknown image format".into());
    }
    // Foundation - return size = 1x1 transparent. Real decode by:
    // image::load_from_memory(bytes)?.to_rgba8().
    Ok(DecodedImage {
        width: 1, height: 1,
        rgba: vec![0, 0, 0, 0],
        format: fmt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_png() {
        let bytes = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_format(bytes), ImageFormat::Png);
    }

    #[test]
    fn detect_jpeg() {
        let bytes = &[0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_format(bytes), ImageFormat::Jpeg);
    }

    #[test]
    fn detect_webp() {
        let mut bytes = vec![b'R', b'I', b'F', b'F'];
        bytes.extend(&[0, 0, 0, 0]);
        bytes.extend(b"WEBP");
        bytes.extend(b"VP8 ");
        assert_eq!(detect_format(&bytes), ImageFormat::Webp);
    }

    #[test]
    fn detect_avif() {
        let mut bytes = vec![0, 0, 0, 0x20];
        bytes.extend(b"ftyp");
        bytes.extend(b"avif");
        assert_eq!(detect_format(&bytes), ImageFormat::Avif);
    }

    #[test]
    fn detect_svg_xml() {
        let bytes = b"<?xml version=\"1.0\"?><svg></svg>";
        assert_eq!(detect_format(bytes), ImageFormat::Svg);
    }

    #[test]
    fn unknown_format() {
        let bytes = b"garbage data";
        assert_eq!(detect_format(bytes), ImageFormat::Unknown);
    }

    #[test]
    fn decode_known_returns_image() {
        let png = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let img = decode(png).unwrap();
        assert_eq!(img.format, ImageFormat::Png);
    }
}
