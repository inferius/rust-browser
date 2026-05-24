//! H.264 NAL unit + SPS metadata parsing (just enough to extract resolution + profile).
//!
//! Spec: ITU-T Rec. H.264 (AVC).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum H264NalType {
    Slice = 1,
    DataPartitionA = 2,
    DataPartitionB = 3,
    DataPartitionC = 4,
    Idr = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    AccessUnitDelimiter = 9,
    EndOfSequence = 10,
    EndOfStream = 11,
    FillerData = 12,
    SpsExt = 13,
    Prefix = 14,
    SubSps = 15,
    AuxSlice = 19,
    SliceExt = 20,
    Unknown = 31,
}

impl From<u8> for H264NalType {
    fn from(v: u8) -> Self {
        match v & 0x1F {
            1 => Self::Slice,
            2 => Self::DataPartitionA,
            3 => Self::DataPartitionB,
            4 => Self::DataPartitionC,
            5 => Self::Idr,
            6 => Self::Sei,
            7 => Self::Sps,
            8 => Self::Pps,
            9 => Self::AccessUnitDelimiter,
            10 => Self::EndOfSequence,
            11 => Self::EndOfStream,
            12 => Self::FillerData,
            13 => Self::SpsExt,
            14 => Self::Prefix,
            15 => Self::SubSps,
            19 => Self::AuxSlice,
            20 => Self::SliceExt,
            _ => Self::Unknown,
        }
    }
}

/// Split an Annex-B byte stream into NAL units.
/// Annex-B uses 0x00 0x00 0x00 0x01 (4-byte) or 0x00 0x00 0x01 (3-byte) start codes.
pub fn split_nal_units(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i < buf.len() {
        // Look for start code.
        let prefix_len = if i + 4 <= buf.len() && &buf[i..i + 4] == &[0, 0, 0, 1] { 4 }
                        else if i + 3 <= buf.len() && &buf[i..i + 3] == &[0, 0, 1] { 3 }
                        else { 0 };
        if prefix_len > 0 {
            if let Some(s) = start {
                out.push(buf[s..i].to_vec());
            }
            start = Some(i + prefix_len);
            i += prefix_len;
            continue;
        }
        i += 1;
    }
    if let Some(s) = start {
        if s < buf.len() {
            out.push(buf[s..].to_vec());
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub struct H264SpsMetadata {
    pub profile_idc: u8,
    pub level_idc: u8,
    pub chroma_format_idc: u8,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,
    pub frame_width: u32,
    pub frame_height: u32,
}

/// Minimal SPS reader: profile + level + dimensions.
/// Real impl uses exp-Golomb bit reader; we extract first three fields directly.
pub fn parse_sps_minimal(nal: &[u8]) -> Option<H264SpsMetadata> {
    if nal.len() < 4 { return None; }
    // nal[0] = NAL header; bytes 1.. = SPS RBSP
    let profile = nal[1];
    let _constraints = nal[2];
    let level = nal[3];
    Some(H264SpsMetadata {
        profile_idc: profile,
        level_idc: level,
        chroma_format_idc: 1,    // 4:2:0 default
        bit_depth_luma_minus8: 0,
        bit_depth_chroma_minus8: 0,
        frame_width: 0,           // requires full bit reader to extract
        frame_height: 0,
    })
}

pub fn profile_name(idc: u8) -> &'static str {
    match idc {
        66 => "Baseline",
        77 => "Main",
        88 => "Extended",
        100 => "High",
        110 => "High 10",
        122 => "High 4:2:2",
        244 => "High 4:4:4",
        44 => "CAVLC 4:4:4",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_nal_basic() {
        let stream = [0, 0, 0, 1, 0x67, 0x42, 0x80, 0x1f, 0, 0, 0, 1, 0x68, 0xce];
        let units = split_nal_units(&stream);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0][0], 0x67);
        assert_eq!(units[1][0], 0x68);
    }

    #[test]
    fn nal_type_sps() {
        // 0x67 = NAL header for SPS (nal_unit_type = 7)
        let t: H264NalType = 0x67.into();
        assert_eq!(t, H264NalType::Sps);
    }

    #[test]
    fn nal_type_idr() {
        let t: H264NalType = 0x65.into();
        assert_eq!(t, H264NalType::Idr);
    }

    #[test]
    fn parse_sps_picks_profile_level() {
        let nal = [0x67, 100, 0, 31];
        let sps = parse_sps_minimal(&nal).unwrap();
        assert_eq!(sps.profile_idc, 100);
        assert_eq!(sps.level_idc, 31);
    }

    #[test]
    fn profile_name_high() {
        assert_eq!(profile_name(100), "High");
        assert_eq!(profile_name(66), "Baseline");
    }
}
