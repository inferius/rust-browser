//! Image format detection + lazy decoder dispatch.
//!
//! Magic byte detection per W3C MIME Sniffing:
//! https://mimesniff.spec.whatwg.org/#image-type-pattern-matching-algorithm
//!
//! Decoder backends (delegated):
//! - PNG  -> png crate
//! - JPEG -> jpeg-decoder
//! - GIF  -> gif crate
//! - WebP -> image crate (or webp-decoder)
//! - AVIF -> ravif/dav1d
//! - SVG  -> usvg parser (separate)
//! - BMP  -> image crate
//! - ICO  -> image crate

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
    Avif,
    Bmp,
    Ico,
    Svg,
    Tiff,
    Heif,
    Jxl,
    Unknown,
}

impl ImageFormat {
    pub fn mime(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Avif => "image/avif",
            Self::Bmp => "image/bmp",
            Self::Ico => "image/x-icon",
            Self::Svg => "image/svg+xml",
            Self::Tiff => "image/tiff",
            Self::Heif => "image/heif",
            Self::Jxl => "image/jxl",
            Self::Unknown => "application/octet-stream",
        }
    }
}

/// Detect from initial bytes - mimesniff alg.
pub fn detect_format(buf: &[u8]) -> ImageFormat {
    if buf.len() >= 8 && buf[..8] == [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] {
        return ImageFormat::Png;
    }
    if buf.len() >= 3 && buf[..3] == [0xff, 0xd8, 0xff] {
        return ImageFormat::Jpeg;
    }
    if buf.len() >= 6 && (buf[..6] == *b"GIF87a" || buf[..6] == *b"GIF89a") {
        return ImageFormat::Gif;
    }
    if buf.len() >= 12 && buf[..4] == *b"RIFF" && buf[8..12] == *b"WEBP" {
        return ImageFormat::WebP;
    }
    if buf.len() >= 12 && buf[4..8] == *b"ftyp" {
        // ISO BMFF: avif/heif
        let brand = &buf[8..12];
        if brand == b"avif" || brand == b"avis" {
            return ImageFormat::Avif;
        }
        if brand == b"heic" || brand == b"heix" || brand == b"mif1" {
            return ImageFormat::Heif;
        }
    }
    if buf.len() >= 2 && buf[..2] == [0x42, 0x4d] {
        return ImageFormat::Bmp;
    }
    if buf.len() >= 4 && buf[..4] == [0x00, 0x00, 0x01, 0x00] {
        return ImageFormat::Ico;
    }
    if buf.len() >= 4 && (buf[..4] == [b'I', b'I', 42, 0] || buf[..4] == [b'M', b'M', 0, 42]) {
        return ImageFormat::Tiff;
    }
    if buf.len() >= 12 && buf[..12] == [0x00, 0x00, 0x00, 0x0c, b'J', b'X', b'L', b' ', 0x0d, 0x0a, 0x87, 0x0a] {
        return ImageFormat::Jxl;
    }
    // Lossy SVG sniff: leading "<svg" or "<?xml" with svg later.
    if buf.len() >= 4 && (&buf[..4] == b"<svg" || &buf[..4] == b"<SVG") {
        return ImageFormat::Svg;
    }
    if buf.len() >= 5 && (&buf[..5] == b"<?xml") {
        let s = std::str::from_utf8(&buf[..buf.len().min(256)]).unwrap_or("");
        if s.contains("<svg") || s.contains("<SVG") { return ImageFormat::Svg; }
    }
    ImageFormat::Unknown
}

#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub format: ImageFormat,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_count: Option<u32>,
    pub has_alpha: bool,
    pub color_space: ColorSpace,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorSpace {
    Srgb,
    DisplayP3,
    Rec2020,
    Linear,
    Unknown,
}

/// Quickly probe dimensions without full decode (header only).
pub fn probe_dimensions(buf: &[u8]) -> Option<(u32, u32)> {
    match detect_format(buf) {
        ImageFormat::Png => {
            // IHDR chunk at offset 16, width = u32 BE @ 16, height @ 20
            if buf.len() < 24 { return None; }
            let w = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
            let h = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
            Some((w, h))
        }
        ImageFormat::Gif => {
            if buf.len() < 10 { return None; }
            let w = u16::from_le_bytes([buf[6], buf[7]]) as u32;
            let h = u16::from_le_bytes([buf[8], buf[9]]) as u32;
            Some((w, h))
        }
        ImageFormat::Bmp => {
            if buf.len() < 26 { return None; }
            let w = u32::from_le_bytes([buf[18], buf[19], buf[20], buf[21]]);
            let h = u32::from_le_bytes([buf[22], buf[23], buf[24], buf[25]]);
            Some((w, h))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_png() {
        let buf = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert_eq!(detect_format(&buf), ImageFormat::Png);
    }

    #[test]
    fn detect_jpeg() {
        let buf = [0xff, 0xd8, 0xff, 0xe0, 0, 0x10];
        assert_eq!(detect_format(&buf), ImageFormat::Jpeg);
    }

    #[test]
    fn detect_gif89() {
        let buf = b"GIF89a\x00\x00\x00\x00";
        assert_eq!(detect_format(buf), ImageFormat::Gif);
    }

    #[test]
    fn detect_webp() {
        let mut buf = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        buf.resize(32, 0);
        assert_eq!(detect_format(&buf), ImageFormat::WebP);
    }

    #[test]
    fn detect_avif() {
        let mut buf = vec![0u8, 0, 0, 0x20];
        buf.extend_from_slice(b"ftypavif");
        buf.resize(32, 0);
        assert_eq!(detect_format(&buf), ImageFormat::Avif);
    }

    #[test]
    fn detect_svg() {
        let buf = b"<svg xmlns='...' />";
        assert_eq!(detect_format(buf), ImageFormat::Svg);
    }

    #[test]
    fn detect_unknown() {
        let buf = b"garbage";
        assert_eq!(detect_format(buf), ImageFormat::Unknown);
    }

    #[test]
    fn probe_png_dimensions() {
        let mut buf = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        buf.extend_from_slice(&[0, 0, 0, 13]); // length
        buf.extend_from_slice(b"IHDR");
        buf.extend_from_slice(&100u32.to_be_bytes());
        buf.extend_from_slice(&200u32.to_be_bytes());
        buf.resize(40, 0);
        assert_eq!(probe_dimensions(&buf), Some((100, 200)));
    }

    #[test]
    fn probe_gif_dimensions() {
        let mut buf = b"GIF89a".to_vec();
        buf.extend_from_slice(&320u16.to_le_bytes());
        buf.extend_from_slice(&240u16.to_le_bytes());
        buf.resize(20, 0);
        assert_eq!(probe_dimensions(&buf), Some((320, 240)));
    }
}
