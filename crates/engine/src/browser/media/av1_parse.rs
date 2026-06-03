//! AV1 OBU (Open Bitstream Unit) detection + sequence header decoding.
//!
//! Spec: AV1 Bitstream & Decoding Process (AOMedia).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Av1ObuType {
    Reserved = 0,
    SequenceHeader = 1,
    TemporalDelimiter = 2,
    FrameHeader = 3,
    TileGroup = 4,
    Metadata = 5,
    Frame = 6,
    RedundantFrameHeader = 7,
    TileList = 8,
    Padding = 15,
}

impl From<u8> for Av1ObuType {
    fn from(v: u8) -> Self {
        match (v >> 3) & 0xF {
            1 => Self::SequenceHeader,
            2 => Self::TemporalDelimiter,
            3 => Self::FrameHeader,
            4 => Self::TileGroup,
            5 => Self::Metadata,
            6 => Self::Frame,
            7 => Self::RedundantFrameHeader,
            8 => Self::TileList,
            15 => Self::Padding,
            _ => Self::Reserved,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Av1SequenceHeader {
    pub profile: u8,                // 0 = Main, 1 = High, 2 = Professional
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub still_picture: bool,
    pub reduced_still_picture: bool,
    pub bit_depth: u8,
    pub monochrome: bool,
}

pub fn profile_name(profile: u8) -> &'static str {
    match profile {
        0 => "Main",
        1 => "High",
        2 => "Professional",
        _ => "Unknown",
    }
}

/// Variable-length encoding ('leb128' alike) used in OBU headers.
pub fn leb128_decode(buf: &[u8]) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    let mut idx = 0;
    loop {
        if idx >= buf.len() || shift >= 64 { return None; }
        let b = buf[idx];
        value |= ((b & 0x7F) as u64) << shift;
        idx += 1;
        if (b & 0x80) == 0 { break; }
        shift += 7;
    }
    Some((value, idx))
}

pub fn leb128_encode(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut b = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 { b |= 0x80; }
        out.push(b);
        if value == 0 { break; }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obu_type_sequence_header() {
        let t: Av1ObuType = 0b0000_1000.into();
        assert_eq!(t, Av1ObuType::SequenceHeader);
    }

    #[test]
    fn obu_type_frame() {
        let t: Av1ObuType = 0b0011_0000.into();
        assert_eq!(t, Av1ObuType::Frame);
    }

    #[test]
    fn profile_names() {
        assert_eq!(profile_name(0), "Main");
        assert_eq!(profile_name(2), "Professional");
        assert_eq!(profile_name(9), "Unknown");
    }

    #[test]
    fn leb128_round_trip() {
        for v in [0u64, 1, 127, 128, 1337, 1_000_000] {
            let buf = leb128_encode(v);
            let (back, _) = leb128_decode(&buf).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn leb128_decode_truncated() {
        // Bytes with continuation bit but no end -> None
        assert!(leb128_decode(&[0x80, 0x80]).is_none());
    }
}
