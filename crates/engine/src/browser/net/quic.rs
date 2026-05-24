//! QUIC transport - varint encoding + packet header parsing.
//!
//! Spec: RFC 9000
//! Variable-length integer encoding (2-bit prefix => 1/2/4/8 bytes).
//! Long header (initial/handshake/0-RTT/retry) vs short header (1-RTT).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LongPacketType {
    Initial,
    ZeroRtt,
    Handshake,
    Retry,
}

#[derive(Debug, Clone)]
pub struct LongHeader {
    pub packet_type: LongPacketType,
    pub version: u32,
    pub dst_cid: Vec<u8>,
    pub src_cid: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ShortHeader {
    pub dst_cid: Vec<u8>,
    pub spin_bit: bool,
    pub key_phase: bool,
}

/// QUIC variable-length integer (RFC 9000 16).
/// 2-bit length prefix: 00=1B, 01=2B, 10=4B, 11=8B
pub fn encode_varint(value: u64) -> Vec<u8> {
    if value < 1 << 6 {
        vec![value as u8]
    } else if value < 1 << 14 {
        let v = (value as u16) | 0x4000;
        v.to_be_bytes().to_vec()
    } else if value < 1 << 30 {
        let v = (value as u32) | 0x8000_0000;
        v.to_be_bytes().to_vec()
    } else if value < 1 << 62 {
        let v = value | 0xc000_0000_0000_0000;
        v.to_be_bytes().to_vec()
    } else {
        panic!("varint value {} exceeds 62-bit max", value);
    }
}

pub fn decode_varint(buf: &[u8]) -> Option<(u64, usize)> {
    if buf.is_empty() { return None; }
    let prefix = buf[0] >> 6;
    match prefix {
        0 => Some(((buf[0] & 0x3f) as u64, 1)),
        1 => {
            if buf.len() < 2 { return None; }
            let v = u16::from_be_bytes([buf[0] & 0x3f, buf[1]]) as u64;
            Some((v, 2))
        }
        2 => {
            if buf.len() < 4 { return None; }
            let mut bytes = [0u8; 4];
            bytes[0] = buf[0] & 0x3f;
            bytes[1..4].copy_from_slice(&buf[1..4]);
            Some((u32::from_be_bytes(bytes) as u64, 4))
        }
        3 => {
            if buf.len() < 8 { return None; }
            let mut bytes = [0u8; 8];
            bytes[0] = buf[0] & 0x3f;
            bytes[1..8].copy_from_slice(&buf[1..8]);
            Some((u64::from_be_bytes(bytes), 8))
        }
        _ => unreachable!(),
    }
}

/// Parse long header. Returns parsed + cursor advanced.
/// Assumes long-header bit (0x80) is set in buf[0].
pub fn parse_long_header(buf: &[u8]) -> Option<(LongHeader, usize)> {
    if buf.len() < 7 { return None; }
    if buf[0] & 0x80 == 0 { return None; }
    let pt = (buf[0] >> 4) & 0x3;
    let packet_type = match pt {
        0 => LongPacketType::Initial,
        1 => LongPacketType::ZeroRtt,
        2 => LongPacketType::Handshake,
        3 => LongPacketType::Retry,
        _ => unreachable!(),
    };
    let version = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
    let dcil = buf[5] as usize;
    let mut idx = 6;
    if buf.len() < idx + dcil + 1 { return None; }
    let dcid = buf[idx..idx + dcil].to_vec();
    idx += dcil;
    let scil = buf[idx] as usize;
    idx += 1;
    if buf.len() < idx + scil { return None; }
    let scid = buf[idx..idx + scil].to_vec();
    idx += scil;
    Some((LongHeader { packet_type, version, dst_cid: dcid, src_cid: scid }, idx))
}

pub fn parse_short_header(buf: &[u8], dcid_len: usize) -> Option<(ShortHeader, usize)> {
    if buf.is_empty() { return None; }
    if buf[0] & 0x80 != 0 { return None; }
    if buf.len() < 1 + dcid_len { return None; }
    let spin = (buf[0] & 0x20) != 0;
    let key_phase = (buf[0] & 0x04) != 0;
    let dcid = buf[1..1 + dcid_len].to_vec();
    Some((ShortHeader { dst_cid: dcid, spin_bit: spin, key_phase }, 1 + dcid_len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_1_byte() {
        let buf = encode_varint(37);
        assert_eq!(buf, vec![37]);
        assert_eq!(decode_varint(&buf), Some((37, 1)));
    }

    #[test]
    fn varint_2_byte() {
        let buf = encode_varint(15293);
        assert_eq!(buf.len(), 2);
        assert_eq!(decode_varint(&buf), Some((15293, 2)));
    }

    #[test]
    fn varint_4_byte() {
        let buf = encode_varint(494878333);
        assert_eq!(buf.len(), 4);
        assert_eq!(decode_varint(&buf), Some((494878333, 4)));
    }

    #[test]
    fn varint_8_byte() {
        let buf = encode_varint(151288809941952652);
        assert_eq!(buf.len(), 8);
        assert_eq!(decode_varint(&buf), Some((151288809941952652, 8)));
    }

    #[test]
    fn long_header_initial() {
        // Initial packet: 0xc0 (long, type 0), version 0x00000001, dcil 8, dcid 8 bytes, scil 0
        let mut buf = vec![0xc0, 0x00, 0x00, 0x00, 0x01, 8];
        buf.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        buf.push(0);
        let (h, n) = parse_long_header(&buf).unwrap();
        assert_eq!(h.packet_type, LongPacketType::Initial);
        assert_eq!(h.version, 1);
        assert_eq!(h.dst_cid.len(), 8);
        assert_eq!(h.src_cid.len(), 0);
        assert_eq!(n, 15);
    }

    #[test]
    fn short_header_spin() {
        let buf = vec![0x60, 1, 2, 3, 4];
        let (h, n) = parse_short_header(&buf, 4).unwrap();
        assert!(h.spin_bit);
        assert!(!h.key_phase);
        assert_eq!(h.dst_cid, vec![1, 2, 3, 4]);
        assert_eq!(n, 5);
    }

    #[test]
    fn short_header_rejects_long() {
        let buf = vec![0xc0, 1, 2, 3, 4];
        assert!(parse_short_header(&buf, 4).is_none());
    }
}
