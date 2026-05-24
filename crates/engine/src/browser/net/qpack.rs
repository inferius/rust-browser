//! QPACK - Header Compression for HTTP/3.
//!
//! Spec: RFC 9204
//! Like HPACK but reordered for QUIC streams: separate encoder/decoder streams.
//! Static table different + larger (99 entries).

/// QPACK static table - subset of common entries (RFC 9204 Appendix A).
pub const QPACK_STATIC: &[(&str, &str)] = &[
    (":authority", ""),
    (":path", "/"),
    ("age", "0"),
    ("content-disposition", ""),
    ("content-length", "0"),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("referer", ""),
    ("set-cookie", ""),
    (":method", "CONNECT"),
    (":method", "DELETE"),
    (":method", "GET"),
    (":method", "HEAD"),
    (":method", "OPTIONS"),
    (":method", "POST"),
    (":method", "PUT"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "103"),
    (":status", "200"),
    (":status", "304"),
    (":status", "404"),
    (":status", "503"),
    ("accept", "*/*"),
    ("accept", "application/dns-message"),
    ("accept-encoding", "gzip, deflate, br"),
    ("accept-ranges", "bytes"),
    ("access-control-allow-headers", "cache-control"),
    ("access-control-allow-headers", "content-type"),
    ("access-control-allow-origin", "*"),
    ("cache-control", "max-age=0"),
    ("cache-control", "max-age=2592000"),
    ("cache-control", "max-age=604800"),
    ("cache-control", "no-cache"),
    ("cache-control", "no-store"),
    ("cache-control", "public, max-age=31536000"),
    ("content-encoding", "br"),
    ("content-encoding", "gzip"),
    ("content-type", "application/dns-message"),
    ("content-type", "application/javascript"),
    ("content-type", "application/json"),
    ("content-type", "application/x-www-form-urlencoded"),
    ("content-type", "image/gif"),
    ("content-type", "image/jpeg"),
    ("content-type", "image/png"),
    ("content-type", "text/css"),
    ("content-type", "text/html; charset=utf-8"),
    ("content-type", "text/plain"),
    ("content-type", "text/plain;charset=utf-8"),
    ("range", "bytes=0-"),
    ("strict-transport-security", "max-age=31536000"),
    ("strict-transport-security", "max-age=31536000; includesubdomains"),
    ("strict-transport-security", "max-age=31536000; includesubdomains; preload"),
    ("vary", "accept-encoding"),
    ("vary", "origin"),
    ("x-content-type-options", "nosniff"),
    ("x-xss-protection", "1; mode=block"),
];

pub fn qpack_static_lookup(name: &str, value: &str) -> Option<usize> {
    QPACK_STATIC.iter().position(|(n, v)| *n == name && *v == value)
}

pub fn qpack_static_lookup_name(name: &str) -> Option<usize> {
    QPACK_STATIC.iter().position(|(n, _)| *n == name)
}

/// Prefixed integer encoding (RFC 9204 4.1.1) - identical scheme as HPACK 5.1.
pub fn encode_prefixed_int(value: u64, prefix_bits: u8) -> Vec<u8> {
    let max = (1u64 << prefix_bits) - 1;
    let mut out = Vec::new();
    if value < max {
        out.push(value as u8);
        return out;
    }
    out.push(max as u8);
    let mut v = value - max;
    while v >= 128 {
        out.push(((v & 0x7f) | 0x80) as u8);
        v >>= 7;
    }
    out.push(v as u8);
    out
}

pub fn decode_prefixed_int(buf: &[u8], prefix_bits: u8) -> Option<(u64, usize)> {
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
        if m >= 64 { return None; }
    }
    Some((value, idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_method_get() {
        assert!(qpack_static_lookup(":method", "GET").is_some());
    }

    #[test]
    fn static_status_404() {
        assert!(qpack_static_lookup(":status", "404").is_some());
    }

    #[test]
    fn static_unknown_pair_misses() {
        assert!(qpack_static_lookup(":method", "PATCH").is_none());
    }

    #[test]
    fn integer_round_trip() {
        for v in [0u64, 1, 7, 8, 15, 16, 1337, 1_000_000] {
            let buf = encode_prefixed_int(v, 4);
            let (back, _) = decode_prefixed_int(&buf, 4).unwrap();
            assert_eq!(back, v);
        }
    }
}
