//! Origin Trials - browser feature opt-in via signed token.
//!
//! Spec: https://github.com/GoogleChrome/OriginTrials
//! `<meta http-equiv="origin-trial" content="...">` or HTTP header.
//! Token encodes (origin, feature, expiry, signature).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct OriginTrialToken {
    pub feature: String,
    pub origin: String,
    pub expires_unix_ms: u64,
    pub usage_restriction: TokenUsage,
    pub is_third_party: bool,
    pub signature_b64: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenUsage {
    None,
    Subset,            // limit % of requests
    NoneRestricted,    // experimental
}

#[derive(Default)]
pub struct OriginTrialRegistry {
    /// (origin, feature) -> token.
    pub tokens: HashMap<(String, String), OriginTrialToken>,
    /// Public-key allowlist for signature verification.
    pub trusted_public_keys: Vec<Vec<u8>>,
}

impl OriginTrialRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, token: OriginTrialToken) -> Result<(), String> {
        if !verify_signature_format(&token.signature_b64) {
            return Err("invalid signature format".into());
        }
        self.tokens.insert((token.origin.clone(), token.feature.clone()), token);
        Ok(())
    }

    pub fn is_enabled(&self, origin: &str, feature: &str, now_unix_ms: u64) -> bool {
        let Some(t) = self.tokens.get(&(origin.into(), feature.into())) else { return false; };
        t.expires_unix_ms > now_unix_ms
    }

    pub fn all_features_for(&self, origin: &str, now_unix_ms: u64) -> Vec<&str> {
        self.tokens.iter()
            .filter(|((o, _), t)| o == origin && t.expires_unix_ms > now_unix_ms)
            .map(|((_, f), _)| f.as_str())
            .collect()
    }

    pub fn revoke(&mut self, origin: &str, feature: &str) {
        self.tokens.remove(&(origin.into(), feature.into()));
    }
}

fn verify_signature_format(sig: &str) -> bool {
    // Real impl: ed25519 verify against trusted_public_keys.
    !sig.is_empty() && sig.bytes().all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'='))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(origin: &str, feature: &str, expires: u64) -> OriginTrialToken {
        OriginTrialToken {
            feature: feature.into(),
            origin: origin.into(),
            expires_unix_ms: expires,
            usage_restriction: TokenUsage::None,
            is_third_party: false,
            signature_b64: "abc123==".into(),
        }
    }

    #[test]
    fn register_and_check() {
        let mut r = OriginTrialRegistry::new();
        r.register(token("https://x.com", "ScrollEnd", 10000)).unwrap();
        assert!(r.is_enabled("https://x.com", "ScrollEnd", 5000));
    }

    #[test]
    fn expired_token_disabled() {
        let mut r = OriginTrialRegistry::new();
        r.register(token("https://x.com", "ScrollEnd", 1000)).unwrap();
        assert!(!r.is_enabled("https://x.com", "ScrollEnd", 5000));
    }

    #[test]
    fn invalid_signature_rejected() {
        let mut r = OriginTrialRegistry::new();
        let mut t = token("https://x.com", "F", 10000);
        t.signature_b64 = "<bad>".into();
        assert!(r.register(t).is_err());
    }

    #[test]
    fn all_features_lists() {
        let mut r = OriginTrialRegistry::new();
        r.register(token("https://x.com", "A", 10000)).unwrap();
        r.register(token("https://x.com", "B", 10000)).unwrap();
        r.register(token("https://y.com", "C", 10000)).unwrap();
        assert_eq!(r.all_features_for("https://x.com", 5000).len(), 2);
    }

    #[test]
    fn revoke_drops_token() {
        let mut r = OriginTrialRegistry::new();
        r.register(token("https://x.com", "F", 10000)).unwrap();
        r.revoke("https://x.com", "F");
        assert!(!r.is_enabled("https://x.com", "F", 5000));
    }
}
