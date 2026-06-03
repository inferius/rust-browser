//! DNS resolver foundation - cache + query/response parsing.
//!
//! Spec: RFC 1035 (DNS) + RFC 8484 (DoH).
//! Real impl uses OS resolver via std::net::ToSocketAddrs; this provides cache + DoH plumbing.

use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Debug, Clone)]
pub struct DnsEntry {
    pub host: String,
    pub addrs: Vec<IpAddr>,
    pub expires_unix_ms: u64,
    pub source: DnsSource,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DnsSource {
    System,
    DnsOverHttps,
    DnsOverTls,
    Static,
}

#[derive(Default)]
pub struct DnsCache {
    pub entries: HashMap<String, DnsEntry>,
    /// Negative cache - "this host did not resolve" with shorter TTL.
    pub negatives: HashMap<String, u64>, // host -> expires_unix_ms
}

impl DnsCache {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, entry: DnsEntry) {
        self.negatives.remove(&entry.host);
        self.entries.insert(entry.host.clone(), entry);
    }

    pub fn insert_negative(&mut self, host: &str, expires_unix_ms: u64) {
        self.negatives.insert(host.into(), expires_unix_ms);
    }

    pub fn lookup(&self, host: &str, now_unix_ms: u64) -> Option<&DnsEntry> {
        let entry = self.entries.get(host)?;
        if entry.expires_unix_ms <= now_unix_ms { return None; }
        Some(entry)
    }

    pub fn is_negative(&self, host: &str, now_unix_ms: u64) -> bool {
        self.negatives.get(host).map(|t| *t > now_unix_ms).unwrap_or(false)
    }

    pub fn evict_expired(&mut self, now_unix_ms: u64) {
        self.entries.retain(|_, e| e.expires_unix_ms > now_unix_ms);
        self.negatives.retain(|_, t| *t > now_unix_ms);
    }
}

/// Build DNS-over-HTTPS query body (RFC 8484, wire format identical to RFC 1035).
/// Returns binary message + id.
pub fn build_doh_query(host: &str, id: u16, qtype: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&id.to_be_bytes());
    buf.extend_from_slice(&[0x01, 0x00]); // flags: standard query + RD
    buf.extend_from_slice(&1u16.to_be_bytes()); // qdcount
    buf.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // an/ns/ar count
    for part in host.split('.') {
        if part.is_empty() { continue; }
        let len = part.len().min(63) as u8;
        buf.push(len);
        buf.extend_from_slice(&part.as_bytes()[..len as usize]);
    }
    buf.push(0); // root
    buf.extend_from_slice(&qtype.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes()); // class IN
    buf
}

/// Parse minimal DoH/UDP response - returns first A/AAAA ip + ttl.
pub fn parse_dns_a_response(buf: &[u8]) -> Option<(Vec<IpAddr>, u32)> {
    if buf.len() < 12 { return None; }
    let qd = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let an = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    let mut idx = 12;
    // Skip questions
    for _ in 0..qd {
        while idx < buf.len() && buf[idx] != 0 {
            let l = buf[idx] as usize;
            if l & 0xc0 == 0xc0 { idx += 2; break; }
            idx += 1 + l;
        }
        if idx < buf.len() && buf[idx] == 0 { idx += 1; }
        idx += 4; // qtype + qclass
    }
    let mut ips = Vec::new();
    let mut min_ttl = u32::MAX;
    for _ in 0..an {
        // Name: pointer or label list - skip
        if idx >= buf.len() { break; }
        if buf[idx] & 0xc0 == 0xc0 { idx += 2; }
        else {
            while idx < buf.len() && buf[idx] != 0 {
                let l = buf[idx] as usize;
                idx += 1 + l;
            }
            idx += 1;
        }
        if idx + 10 > buf.len() { break; }
        let rtype = u16::from_be_bytes([buf[idx], buf[idx + 1]]);
        idx += 2;
        idx += 2; // class
        let ttl = u32::from_be_bytes([buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]]);
        idx += 4;
        let rdlen = u16::from_be_bytes([buf[idx], buf[idx + 1]]) as usize;
        idx += 2;
        if idx + rdlen > buf.len() { break; }
        match rtype {
            1 if rdlen == 4 => {
                ips.push(IpAddr::from([buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]]));
                if ttl < min_ttl { min_ttl = ttl; }
            }
            28 if rdlen == 16 => {
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&buf[idx..idx + 16]);
                ips.push(IpAddr::from(octets));
                if ttl < min_ttl { min_ttl = ttl; }
            }
            _ => {}
        }
        idx += rdlen;
    }
    if ips.is_empty() { None } else { Some((ips, if min_ttl == u32::MAX { 60 } else { min_ttl })) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn cache_lookup_respects_ttl() {
        let mut c = DnsCache::new();
        c.insert(DnsEntry {
            host: "x.com".into(),
            addrs: vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))],
            expires_unix_ms: 1000,
            source: DnsSource::System,
        });
        assert!(c.lookup("x.com", 500).is_some());
        assert!(c.lookup("x.com", 2000).is_none());
    }

    #[test]
    fn negative_cache() {
        let mut c = DnsCache::new();
        c.insert_negative("missing.test", 1000);
        assert!(c.is_negative("missing.test", 500));
        assert!(!c.is_negative("missing.test", 2000));
    }

    #[test]
    fn evict_drops_stale() {
        let mut c = DnsCache::new();
        c.insert(DnsEntry {
            host: "x.com".into(),
            addrs: vec![],
            expires_unix_ms: 100,
            source: DnsSource::System,
        });
        c.insert_negative("y.com", 100);
        c.evict_expired(200);
        assert!(c.entries.is_empty());
        assert!(c.negatives.is_empty());
    }

    #[test]
    fn build_doh_query_has_correct_id() {
        let q = build_doh_query("example.com", 0x1234, 1);
        assert_eq!(q[0], 0x12);
        assert_eq!(q[1], 0x34);
        // example: ascii names embedded
        assert!(q.iter().any(|b| *b == b'e'));
    }

    #[test]
    fn parse_short_truncated_returns_none() {
        assert!(parse_dns_a_response(&[0u8; 5]).is_none());
    }
}
