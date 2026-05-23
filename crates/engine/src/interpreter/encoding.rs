//! TextEncoder / TextDecoder API.
//!
//! Spec: https://encoding.spec.whatwg.org/
//! TextEncoder always UTF-8. TextDecoder supports many labels.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncodingLabel {
    Utf8,
    Utf16Be,
    Utf16Le,
    Iso8859_1,    // alias windows-1252 per spec
    Windows1252,
    Ascii,
    GB18030,
    Big5,
    EucJp,
    ShiftJis,
    EucKr,
}

impl EncodingLabel {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "utf-8" | "utf8" | "unicode-1-1-utf-8" => Some(Self::Utf8),
            "utf-16be" => Some(Self::Utf16Be),
            "utf-16le" | "utf-16" => Some(Self::Utf16Le),
            "iso-8859-1" | "latin1" | "csisolatin1" => Some(Self::Iso8859_1),
            "windows-1252" | "cp1252" | "x-cp1252" => Some(Self::Windows1252),
            "ascii" | "us-ascii" => Some(Self::Ascii),
            "gb18030" | "gbk" | "csgb2312" => Some(Self::GB18030),
            "big5" => Some(Self::Big5),
            "euc-jp" => Some(Self::EucJp),
            "shift_jis" | "sjis" => Some(Self::ShiftJis),
            "euc-kr" => Some(Self::EucKr),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Be => "utf-16be",
            Self::Utf16Le => "utf-16le",
            Self::Iso8859_1 => "iso-8859-1",
            Self::Windows1252 => "windows-1252",
            Self::Ascii => "ascii",
            Self::GB18030 => "gb18030",
            Self::Big5 => "big5",
            Self::EucJp => "euc-jp",
            Self::ShiftJis => "shift_jis",
            Self::EucKr => "euc-kr",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextEncoderOutput {
    pub written: usize,
    pub read: usize,
}

/// UTF-8 only encoder per spec.
pub fn encode_utf8(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

/// `encodeInto` style: write to fixed-size buffer, return chars read + bytes written.
pub fn encode_into(s: &str, dest: &mut [u8]) -> TextEncoderOutput {
    let mut read = 0;
    let mut written = 0;
    for c in s.chars() {
        let len = c.len_utf8();
        if written + len > dest.len() { break; }
        c.encode_utf8(&mut dest[written..]);
        written += len;
        read += 1;
    }
    TextEncoderOutput { read, written }
}

/// Decode bytes per encoding, with fatal/replacement behavior.
pub fn decode(bytes: &[u8], enc: EncodingLabel, fatal: bool) -> Result<String, String> {
    match enc {
        EncodingLabel::Utf8 => {
            if fatal {
                std::str::from_utf8(bytes).map(|s| s.to_string())
                    .map_err(|e| format!("utf8 decode error: {}", e))
            } else {
                Ok(String::from_utf8_lossy(bytes).to_string())
            }
        }
        EncodingLabel::Ascii => {
            if bytes.iter().any(|b| *b > 0x7f) && fatal {
                return Err("non-ascii byte".into());
            }
            Ok(bytes.iter().map(|b| if *b <= 0x7f { *b as char } else { '\u{FFFD}' }).collect())
        }
        EncodingLabel::Iso8859_1 | EncodingLabel::Windows1252 => {
            // Latin-1 = each byte to U+00XX (windows-1252 has different 80..9F mapping; we approximate).
            Ok(bytes.iter().map(|b| *b as char).collect())
        }
        EncodingLabel::Utf16Le => {
            if bytes.len() % 2 != 0 && fatal { return Err("odd byte count for utf-16le".into()); }
            let u16s: Vec<u16> = bytes.chunks(2).map(|c| {
                if c.len() == 2 { u16::from_le_bytes([c[0], c[1]]) } else { 0xFFFD }
            }).collect();
            String::from_utf16(&u16s).map_err(|_| "utf16 surrogate".to_string())
        }
        EncodingLabel::Utf16Be => {
            if bytes.len() % 2 != 0 && fatal { return Err("odd byte count for utf-16be".into()); }
            let u16s: Vec<u16> = bytes.chunks(2).map(|c| {
                if c.len() == 2 { u16::from_be_bytes([c[0], c[1]]) } else { 0xFFFD }
            }).collect();
            String::from_utf16(&u16s).map_err(|_| "utf16 surrogate".to_string())
        }
        _ => {
            // Other multi-byte encodings: real impl needs tables; fall back to lossy UTF-8.
            Ok(String::from_utf8_lossy(bytes).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_round_trip() {
        let v = encode_utf8("hello");
        assert_eq!(v, b"hello");
    }

    #[test]
    fn encode_into_partial() {
        let mut buf = [0u8; 5];
        let r = encode_into("hello world", &mut buf);
        assert_eq!(r.read, 5);
        assert_eq!(r.written, 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn encode_into_multibyte() {
        let mut buf = [0u8; 4];
        // U+00E1 (a-acute) = 2 bytes; "aab" = 1+1+1
        let r = encode_into("a\u{00E1}b", &mut buf);
        assert_eq!(r.read, 3);
        assert_eq!(r.written, 4);
    }

    #[test]
    fn decode_utf8_lossy() {
        let r = decode(b"hello", EncodingLabel::Utf8, false).unwrap();
        assert_eq!(r, "hello");
    }

    #[test]
    fn decode_utf8_fatal_errors() {
        assert!(decode(&[0xff, 0xfe], EncodingLabel::Utf8, true).is_err());
    }

    #[test]
    fn decode_latin1() {
        let r = decode(&[0xc1, 0xe9], EncodingLabel::Iso8859_1, false).unwrap();
        assert_eq!(r, "\u{00C1}\u{00E9}");
    }

    #[test]
    fn decode_utf16le() {
        let r = decode(&[b'h', 0, b'i', 0], EncodingLabel::Utf16Le, false).unwrap();
        assert_eq!(r, "hi");
    }

    #[test]
    fn label_parse() {
        assert_eq!(EncodingLabel::parse("UTF-8"), Some(EncodingLabel::Utf8));
        assert_eq!(EncodingLabel::parse("latin1"), Some(EncodingLabel::Iso8859_1));
        assert_eq!(EncodingLabel::parse("garbage"), None);
    }
}
