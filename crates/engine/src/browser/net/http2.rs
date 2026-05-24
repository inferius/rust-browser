//! HTTP/2 frame parser foundation.
//!
//! RFC 7540. Binary framing: frame header 9 bytes + payload. Streams pres
//! frame.stream_id. Multiplexing pres prev jeden TCP connection.
//!
//! Foundation: frame types + parse. Real impl pres `h2` crate ci `hyper` =
//! next session (vyzaduje tokio runtime).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameType {
    Data = 0x0,
    Headers = 0x1,
    Priority = 0x2,
    RstStream = 0x3,
    Settings = 0x4,
    PushPromise = 0x5,
    Ping = 0x6,
    GoAway = 0x7,
    WindowUpdate = 0x8,
    Continuation = 0x9,
    Unknown,
}

impl FrameType {
    pub fn from_u8(b: u8) -> Self {
        match b {
            0x0 => Self::Data,
            0x1 => Self::Headers,
            0x2 => Self::Priority,
            0x3 => Self::RstStream,
            0x4 => Self::Settings,
            0x5 => Self::PushPromise,
            0x6 => Self::Ping,
            0x7 => Self::GoAway,
            0x8 => Self::WindowUpdate,
            0x9 => Self::Continuation,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameHeader {
    pub length: u32,        // 24-bit
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_id: u32,     // 31-bit (highest bit reserved)
}

/// Parse 9-byte frame header. Vraci None pri kratky buf.
pub fn parse_frame_header(buf: &[u8]) -> Option<FrameHeader> {
    if buf.len() < 9 { return None; }
    let length = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);
    let frame_type = FrameType::from_u8(buf[3]);
    let flags = buf[4];
    let stream_id = (((buf[5] as u32) << 24) | ((buf[6] as u32) << 16)
        | ((buf[7] as u32) << 8) | (buf[8] as u32)) & 0x7FFF_FFFF;
    Some(FrameHeader { length, frame_type, flags, stream_id })
}

/// HTTP/2 connection preface (24 bytes).
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Settings frame parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsParameter {
    HeaderTableSize = 0x1,
    EnablePush = 0x2,
    MaxConcurrentStreams = 0x3,
    InitialWindowSize = 0x4,
    MaxFrameSize = 0x5,
    MaxHeaderListSize = 0x6,
}

#[derive(Debug, Default)]
pub struct Http2Settings {
    pub header_table_size: u32,        // 4096
    pub enable_push: u32,              // 1
    pub max_concurrent_streams: u32,   // unlimited (set to high)
    pub initial_window_size: u32,      // 65535
    pub max_frame_size: u32,           // 16384
    pub max_header_list_size: u32,     // unlimited
}

impl Http2Settings {
    pub fn defaults() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: 1,
            max_concurrent_streams: 100,
            initial_window_size: 65535,
            max_frame_size: 16384,
            max_header_list_size: u32::MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_settings_frame_header() {
        // length=18, type=Settings(0x4), flags=0, stream_id=0.
        let buf = [0, 0, 18, 0x4, 0, 0, 0, 0, 0];
        let h = parse_frame_header(&buf).unwrap();
        assert_eq!(h.length, 18);
        assert_eq!(h.frame_type, FrameType::Settings);
        assert_eq!(h.stream_id, 0);
    }

    #[test]
    fn parse_data_frame_header() {
        let buf = [0, 0, 0x42, 0x0, 0x1, 0, 0, 0, 0x5];
        let h = parse_frame_header(&buf).unwrap();
        assert_eq!(h.length, 0x42);
        assert_eq!(h.frame_type, FrameType::Data);
        assert_eq!(h.stream_id, 5);
        assert_eq!(h.flags, 0x1); // END_STREAM
    }

    #[test]
    fn short_buf_returns_none() {
        let buf = [0u8; 5];
        assert!(parse_frame_header(&buf).is_none());
    }

    #[test]
    fn reserved_bit_masked() {
        let buf = [0, 0, 0, 0x0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
        let h = parse_frame_header(&buf).unwrap();
        // Highest bit cleared.
        assert_eq!(h.stream_id, 0x7FFF_FFFF);
    }

    #[test]
    fn default_settings_values() {
        let s = Http2Settings::defaults();
        assert_eq!(s.initial_window_size, 65535);
        assert_eq!(s.max_frame_size, 16384);
    }
}
