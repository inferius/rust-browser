//! WebSocket frame parser - binary frames per RFC 6455.
//!
//! Frame layout:
//! - byte 0: FIN(1) + RSV(3) + opcode(4)
//! - byte 1: MASK(1) + payload_len(7)
//! - extended length (0/2/8 bytes pri 126/127)
//! - mask key (0/4 bytes)
//! - payload

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
    Unknown,
}

impl Opcode {
    pub fn from_u8(b: u8) -> Self {
        match b & 0x0F {
            0x0 => Self::Continuation,
            0x1 => Self::Text,
            0x2 => Self::Binary,
            0x8 => Self::Close,
            0x9 => Self::Ping,
            0xA => Self::Pong,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug)]
pub struct WsFrame {
    pub fin: bool,
    pub opcode: Opcode,
    pub masked: bool,
    pub payload: Vec<u8>,
}

/// Parse WS frame z buffer. Vraci (frame, consumed_bytes) nebo None pri short.
pub fn parse_frame(buf: &[u8]) -> Option<(WsFrame, usize)> {
    if buf.len() < 2 { return None; }
    let b0 = buf[0];
    let b1 = buf[1];
    let fin = b0 & 0x80 != 0;
    let opcode = Opcode::from_u8(b0);
    let masked = b1 & 0x80 != 0;
    let raw_len = (b1 & 0x7F) as u64;
    let mut idx = 2;
    let payload_len = match raw_len {
        126 => {
            if buf.len() < idx + 2 { return None; }
            let l = u16::from_be_bytes([buf[idx], buf[idx+1]]) as u64;
            idx += 2; l
        }
        127 => {
            if buf.len() < idx + 8 { return None; }
            let l = u64::from_be_bytes([
                buf[idx], buf[idx+1], buf[idx+2], buf[idx+3],
                buf[idx+4], buf[idx+5], buf[idx+6], buf[idx+7],
            ]);
            idx += 8; l
        }
        n => n,
    };
    let mask = if masked {
        if buf.len() < idx + 4 { return None; }
        let m = [buf[idx], buf[idx+1], buf[idx+2], buf[idx+3]];
        idx += 4; Some(m)
    } else { None };
    if buf.len() < idx + payload_len as usize { return None; }
    let payload_raw = &buf[idx..idx + payload_len as usize];
    let payload = match mask {
        Some(m) => payload_raw.iter().enumerate()
            .map(|(i, b)| b ^ m[i % 4])
            .collect(),
        None => payload_raw.to_vec(),
    };
    let consumed = idx + payload_len as usize;
    Some((WsFrame { fin, opcode, masked, payload }, consumed))
}

/// Build outgoing frame (client must mask).
pub fn build_frame(opcode: Opcode, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | (opcode as u8)); // FIN + opcode
    let masked_bit = if mask.is_some() { 0x80 } else { 0x00 };
    let len = payload.len();
    if len < 126 {
        out.push(masked_bit | len as u8);
    } else if len < 65536 {
        out.push(masked_bit | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(masked_bit | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    if let Some(m) = mask {
        out.extend_from_slice(&m);
        for (i, b) in payload.iter().enumerate() {
            out.push(b ^ m[i % 4]);
        }
    } else {
        out.extend_from_slice(payload);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_text_frame() {
        let frame = [0x81, 0x05, b'h', b'e', b'l', b'l', b'o'];
        let (f, c) = parse_frame(&frame).unwrap();
        assert!(f.fin);
        assert_eq!(f.opcode, Opcode::Text);
        assert_eq!(f.payload, b"hello");
        assert_eq!(c, 7);
    }

    #[test]
    fn parse_masked_frame() {
        let mask = [0xDE, 0xAD, 0xBE, 0xEF];
        let payload = b"hi";
        let mut frame = vec![0x81, 0x82, mask[0], mask[1], mask[2], mask[3]];
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        let (f, _) = parse_frame(&frame).unwrap();
        assert_eq!(f.payload, b"hi");
        assert!(f.masked);
    }

    #[test]
    fn build_unmasked() {
        let bytes = build_frame(Opcode::Text, b"hi", None);
        assert_eq!(bytes[0], 0x81);
        assert_eq!(bytes[1], 0x02);
        assert_eq!(&bytes[2..], b"hi");
    }

    #[test]
    fn build_masked_roundtrip() {
        let bytes = build_frame(Opcode::Binary, &[1, 2, 3, 4], Some([0x11, 0x22, 0x33, 0x44]));
        let (f, _) = parse_frame(&bytes).unwrap();
        assert_eq!(f.payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn extended_length_16bit() {
        let payload = vec![0u8; 200];
        let bytes = build_frame(Opcode::Binary, &payload, None);
        assert_eq!(bytes[1] & 0x7F, 126);
        let (f, _) = parse_frame(&bytes).unwrap();
        assert_eq!(f.payload.len(), 200);
    }
}
