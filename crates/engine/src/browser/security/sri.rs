//! Subresource Integrity (SRI) - script/link integrity attribute verification.
//!
//! Spec: https://www.w3.org/TR/SRI/
//! `<script integrity="sha384-..." crossorigin="anonymous">`
//! Verify hash matches downloaded body before execution.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SriAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl SriAlgorithm {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sha256" => Some(Self::Sha256),
            "sha384" => Some(Self::Sha384),
            "sha512" => Some(Self::Sha512),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntegrityHash {
    pub algorithm: SriAlgorithm,
    pub b64_digest: String,
}

/// Parse integrity="..." attribute (whitespace-separated hashes).
pub fn parse_integrity_attr(s: &str) -> Vec<IntegrityHash> {
    s.split_ascii_whitespace().filter_map(|token| {
        let (alg, b64) = token.split_once('-')?;
        let algorithm = SriAlgorithm::parse(alg)?;
        Some(IntegrityHash { algorithm, b64_digest: b64.into() })
    }).collect()
}

/// SRI verification: at least one hash from the integrity attr must match a digest.
/// Caller passes pre-computed digest dle algoritmu.
pub fn verify(hashes: &[IntegrityHash], computed: &[(SriAlgorithm, String)]) -> bool {
    if hashes.is_empty() { return true; } // no integrity requirement
    // Spec: when multiple hashes provided, use only the strongest algorithm group.
    let strongest = hashes.iter().map(|h| h.algorithm).max_by_key(|a| match a {
        SriAlgorithm::Sha256 => 0,
        SriAlgorithm::Sha384 => 1,
        SriAlgorithm::Sha512 => 2,
    }).unwrap();
    let candidates: Vec<&IntegrityHash> = hashes.iter().filter(|h| h.algorithm == strongest).collect();
    candidates.iter().any(|h| {
        computed.iter().any(|(a, d)| *a == h.algorithm && d == &h.b64_digest)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single() {
        let h = parse_integrity_attr("sha384-abc123");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].algorithm, SriAlgorithm::Sha384);
        assert_eq!(h[0].b64_digest, "abc123");
    }

    #[test]
    fn parse_multiple() {
        let h = parse_integrity_attr("sha256-aaa sha384-bbb sha512-ccc");
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn parse_unknown_algo_skipped() {
        let h = parse_integrity_attr("md5-xxx sha256-yyy");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].algorithm, SriAlgorithm::Sha256);
    }

    #[test]
    fn verify_match() {
        let hashes = parse_integrity_attr("sha256-abc123");
        let computed = vec![(SriAlgorithm::Sha256, "abc123".to_string())];
        assert!(verify(&hashes, &computed));
    }

    #[test]
    fn verify_strongest_only() {
        // sha256 hash present but ignored when sha384 listed
        let hashes = parse_integrity_attr("sha256-WRONG sha384-RIGHT");
        let computed = vec![
            (SriAlgorithm::Sha256, "WRONG".into()),
            (SriAlgorithm::Sha384, "WRONG".into()), // mismatched
        ];
        assert!(!verify(&hashes, &computed));
    }

    #[test]
    fn empty_passes() {
        assert!(verify(&[], &[]));
    }
}
