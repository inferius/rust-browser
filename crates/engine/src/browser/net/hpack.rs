//! HPACK - Header Compression for HTTP/2.
//!
//! Spec: RFC 7541
//! Static table + dynamic table + Huffman encoding for binary headers.

/// HPACK static table - RFC 7541 Appendix A. Indices 1..61.
pub const STATIC_TABLE: &[(&str, &str)] = &[
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

#[derive(Debug, Clone)]
pub struct HpackHeader {
    pub name: String,
    pub value: String,
    pub never_indexed: bool,
}

#[derive(Default)]
pub struct DynamicTable {
    pub entries: Vec<HpackHeader>,
    pub max_size: usize,
    pub current_size: usize,
}

impl DynamicTable {
    pub fn new() -> Self {
        Self { entries: Vec::new(), max_size: 4096, current_size: 0 }
    }

    pub fn insert(&mut self, header: HpackHeader) {
        let size = entry_size(&header);
        // Evict from end until fits
        while self.current_size + size > self.max_size && !self.entries.is_empty() {
            let last = self.entries.pop().unwrap();
            self.current_size -= entry_size(&last);
        }
        if size > self.max_size { return; } // too big, drop
        self.entries.insert(0, header);
        self.current_size += size;
    }

    pub fn get(&self, idx_within_dyn: usize) -> Option<&HpackHeader> {
        self.entries.get(idx_within_dyn)
    }

    pub fn set_max_size(&mut self, size: usize) {
        self.max_size = size;
        while self.current_size > self.max_size && !self.entries.is_empty() {
            let last = self.entries.pop().unwrap();
            self.current_size -= entry_size(&last);
        }
    }
}

pub fn entry_size(h: &HpackHeader) -> usize {
    h.name.len() + h.value.len() + 32
}

/// Integer encoding per RFC 7541 5.1.
pub fn encode_integer(value: u64, prefix_bits: u8) -> Vec<u8> {
    let max_prefix = (1u64 << prefix_bits) - 1;
    let mut out = Vec::new();
    if value < max_prefix {
        out.push(value as u8);
        return out;
    }
    out.push(max_prefix as u8);
    let mut v = value - max_prefix;
    while v >= 128 {
        out.push(((v & 0x7f) | 0x80) as u8);
        v >>= 7;
    }
    out.push(v as u8);
    out
}

pub fn decode_integer(buf: &[u8], prefix_bits: u8) -> Option<(u64, usize)> {
    if buf.is_empty() { return None; }
    let mask = (1u8 << prefix_bits) - 1;
    let mut value = (buf[0] & mask) as u64;
    if value < mask as u64 {
        return Some((value, 1));
    }
    let mut idx = 1;
    let mut m = 0;
    loop {
        if idx >= buf.len() { return None; }
        let b = buf[idx];
        idx += 1;
        value += ((b & 0x7f) as u64) << m;
        m += 7;
        if (b & 0x80) == 0 { break; }
        if m >= 64 { return None; } // overflow guard
    }
    Some((value, idx))
}

/// Look up name+value in static table; returns index (1-based).
pub fn static_lookup(name: &str, value: &str) -> Option<usize> {
    STATIC_TABLE.iter().position(|(n, v)| *n == name && *v == value).map(|i| i + 1)
}

/// Look up name only in static table; returns index (1-based).
pub fn static_lookup_name(name: &str) -> Option<usize> {
    STATIC_TABLE.iter().position(|(n, _)| *n == name).map(|i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_small_fits_prefix() {
        let buf = encode_integer(10, 5);
        assert_eq!(buf, vec![10]);
    }

    #[test]
    fn integer_overflow_prefix() {
        // value = 1337, prefix_bits = 5; max_prefix = 31
        // 1337 - 31 = 1306; 1306 = 0x51A; encoded as 154, 10
        let buf = encode_integer(1337, 5);
        assert_eq!(buf[0], 31);
        // 1306 = 0b 0000 1010 0001 1010 -> 7-bit chunks: 0011010 (26), 0001010 (10)
        // -> first byte 26|0x80=154, then 10
        assert_eq!(buf[1], 154);
        assert_eq!(buf[2], 10);
    }

    #[test]
    fn integer_round_trip() {
        for &v in &[0u64, 5, 30, 31, 100, 1337, 16_777_215] {
            let buf = encode_integer(v, 5);
            let (decoded, _) = decode_integer(&buf, 5).unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn static_lookup_known() {
        assert_eq!(static_lookup(":method", "GET"), Some(2));
        assert_eq!(static_lookup(":status", "200"), Some(8));
    }

    #[test]
    fn static_lookup_name_only() {
        assert_eq!(static_lookup_name("date"), Some(33));
    }

    #[test]
    fn dynamic_table_inserts_at_front() {
        let mut t = DynamicTable::new();
        t.insert(HpackHeader { name: "x".into(), value: "1".into(), never_indexed: false });
        t.insert(HpackHeader { name: "y".into(), value: "2".into(), never_indexed: false });
        assert_eq!(t.entries[0].name, "y");
        assert_eq!(t.entries[1].name, "x");
    }

    #[test]
    fn dynamic_table_evicts() {
        let mut t = DynamicTable::new();
        t.max_size = 100;
        // Each header ~ 34 bytes (1 char + 1 char + 32 overhead). 3 entries = 102, evict 1.
        for c in 0..3 {
            t.insert(HpackHeader { name: format!("a{}", c), value: format!("v{}", c), never_indexed: false });
        }
        assert!(t.current_size <= t.max_size);
    }
}
