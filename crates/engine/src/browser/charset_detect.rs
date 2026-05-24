//! Charset detection from BOM / Content-Type / meta tag / heuristics.
//!
//! Spec: https://html.spec.whatwg.org/multipage/parsing.html#determining-the-character-encoding

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectedCharset {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf8Bom,
    Windows1252,
    Iso8859_1,
    GBK,
    Big5,
    ShiftJis,
    EucJp,
    EucKr,
    Unknown,
}

impl DetectedCharset {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Utf8 | Self::Utf8Bom => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Windows1252 => "windows-1252",
            Self::Iso8859_1 => "ISO-8859-1",
            Self::GBK => "GBK",
            Self::Big5 => "Big5",
            Self::ShiftJis => "Shift_JIS",
            Self::EucJp => "EUC-JP",
            Self::EucKr => "EUC-KR",
            Self::Unknown => "",
        }
    }
}

pub fn detect_bom(buf: &[u8]) -> Option<DetectedCharset> {
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) { return Some(DetectedCharset::Utf8Bom); }
    if buf.starts_with(&[0xFE, 0xFF]) { return Some(DetectedCharset::Utf16Be); }
    if buf.starts_with(&[0xFF, 0xFE]) { return Some(DetectedCharset::Utf16Le); }
    None
}

/// Extract charset from a Content-Type header value like "text/html; charset=UTF-8".
pub fn from_content_type(value: &str) -> Option<DetectedCharset> {
    let lower = value.to_ascii_lowercase();
    let pos = lower.find("charset=")?;
    let cs = &lower[pos + 8..];
    let cs = cs.trim_matches('"').trim_matches('\'');
    let end = cs.find(|c: char| c == ';' || c.is_whitespace()).unwrap_or(cs.len());
    parse_label(&cs[..end])
}

/// Scan first 1024 bytes for `<meta charset>` / `<meta http-equiv>`.
pub fn from_meta_prescan(buf: &[u8]) -> Option<DetectedCharset> {
    let window = &buf[..buf.len().min(1024)];
    let s = std::str::from_utf8(window).ok()?.to_ascii_lowercase();
    if let Some(i) = s.find("charset=") {
        let rest = &s[i + 8..];
        let end = rest.find(|c: char| c == '\'' || c == '"' || c == '>' || c == ' ' || c == ';').unwrap_or(rest.len());
        return parse_label(rest[..end].trim_matches('"').trim_matches('\''));
    }
    None
}

fn parse_label(label: &str) -> Option<DetectedCharset> {
    match label.trim().to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => Some(DetectedCharset::Utf8),
        "utf-16le" | "utf-16" => Some(DetectedCharset::Utf16Le),
        "utf-16be" => Some(DetectedCharset::Utf16Be),
        "windows-1252" => Some(DetectedCharset::Windows1252),
        "iso-8859-1" | "latin1" => Some(DetectedCharset::Iso8859_1),
        "gbk" | "gb2312" | "gb18030" => Some(DetectedCharset::GBK),
        "big5" => Some(DetectedCharset::Big5),
        "shift_jis" | "sjis" => Some(DetectedCharset::ShiftJis),
        "euc-jp" => Some(DetectedCharset::EucJp),
        "euc-kr" => Some(DetectedCharset::EucKr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_utf8() {
        assert_eq!(detect_bom(&[0xEF, 0xBB, 0xBF, 0x41]), Some(DetectedCharset::Utf8Bom));
    }

    #[test]
    fn bom_utf16le() {
        assert_eq!(detect_bom(&[0xFF, 0xFE, 0x41, 0x00]), Some(DetectedCharset::Utf16Le));
    }

    #[test]
    fn content_type_utf8() {
        assert_eq!(from_content_type("text/html; charset=UTF-8"), Some(DetectedCharset::Utf8));
    }

    #[test]
    fn content_type_quoted() {
        assert_eq!(from_content_type("text/html; charset=\"windows-1252\""), Some(DetectedCharset::Windows1252));
    }

    #[test]
    fn meta_prescan_finds_charset() {
        let html = b"<!doctype html><meta charset=utf-8><title>x</title>";
        assert_eq!(from_meta_prescan(html), Some(DetectedCharset::Utf8));
    }

    #[test]
    fn meta_prescan_inside_quotes() {
        let html = b"<meta http-equiv=\"Content-Type\" content=\"text/html; charset=gbk\">";
        let r = from_meta_prescan(html);
        assert!(r.is_some());
    }
}
