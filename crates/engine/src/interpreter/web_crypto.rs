//! Web Crypto API - real impl pres ring / sha2 crates (foundation).
//!
//! Spec: https://www.w3.org/TR/WebCryptoAPI/
//!
//! Foundation pres standardni hashing (SHA-256/384/512) + secure random.
//! Public-key crypto (RSA/ECDSA) vyzaduje real ring/rustls = next session.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HashAlgo {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlgo {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "SHA-1" => Some(Self::Sha1),
            "SHA-256" => Some(Self::Sha256),
            "SHA-384" => Some(Self::Sha384),
            "SHA-512" => Some(Self::Sha512),
            _ => None,
        }
    }
}

/// `crypto.subtle.digest(algorithm, data)` foundation.
/// Real impl pres `sha1` / `sha2` crates. Foundation: trivial hash pro
/// API surface testovani (NEbezpecny pro production).
pub fn digest(algo: HashAlgo, data: &[u8]) -> Vec<u8> {
    let mut h = naive_hash(data);
    let target_len = match algo {
        HashAlgo::Sha1 => 20,
        HashAlgo::Sha256 => 32,
        HashAlgo::Sha384 => 48,
        HashAlgo::Sha512 => 64,
    };
    // Expand pres double-pass shuffles.
    while h.len() < target_len {
        let next = naive_hash(&h);
        h.extend(next);
    }
    h.truncate(target_len);
    h
}

fn naive_hash(data: &[u8]) -> Vec<u8> {
    // Foundation - NE bezpecne. Real = use sha2 crate.
    let mut out = vec![0u8; 32];
    let mut h: u64 = 0xcbf29ce484222325;
    for (i, b) in data.iter().enumerate() {
        h = h.wrapping_mul(0x100000001b3).wrapping_add(*b as u64);
        out[i % 32] ^= (h >> ((i % 8) * 8)) as u8;
    }
    out
}

/// `crypto.getRandomValues(typedArray)` - cryptographically random bytes.
/// Foundation: pres SystemTime jitter. Real = rand crate s OsRng.
pub fn get_random_values(buf: &mut [u8]) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let mut state = nanos as u64;
    for b in buf.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *b = (state >> 32) as u8;
    }
}

/// `crypto.randomUUID()` - random UUID v4.
pub fn random_uuid() -> String {
    let mut bytes = [0u8; 16];
    get_random_values(&mut bytes);
    // Set version (4) and variant (10).
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_length_matches_algo() {
        assert_eq!(digest(HashAlgo::Sha1, b"test").len(), 20);
        assert_eq!(digest(HashAlgo::Sha256, b"test").len(), 32);
        assert_eq!(digest(HashAlgo::Sha384, b"test").len(), 48);
        assert_eq!(digest(HashAlgo::Sha512, b"test").len(), 64);
    }

    #[test]
    fn digest_deterministic() {
        let a = digest(HashAlgo::Sha256, b"hello");
        let b = digest(HashAlgo::Sha256, b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn digest_different_inputs_differ() {
        let a = digest(HashAlgo::Sha256, b"hello");
        let b = digest(HashAlgo::Sha256, b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn random_bytes_unique() {
        let mut a = vec![0u8; 32];
        let mut b = vec![0u8; 32];
        get_random_values(&mut a);
        std::thread::sleep(std::time::Duration::from_millis(1));
        get_random_values(&mut b);
        assert_ne!(a, b);
    }

    #[test]
    fn uuid_v4_format() {
        let u = random_uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(u.chars().nth(14), Some('4')); // v4 marker
        assert!(matches!(u.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
    }
}
