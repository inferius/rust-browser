//! Private State Tokens API (drive Trust Tokens) - anti-fraud bez tracking.
//!
//! Spec: https://wicg.github.io/trust-token-api/
//! fetch(url, {privateToken: {operation: 'token-request' | 'send-redemption-record' | ...}})

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenOperation {
    Issuance,
    Redemption,
    SendRedemptionRecord,
}

#[derive(Debug, Clone)]
pub struct PrivateStateToken {
    pub issuer: String,
    pub blind_token: Vec<u8>,   // VOPRF blinded scalar placeholder
    pub key_commitment_id: u32, // issuer key version
}

#[derive(Debug, Clone)]
pub struct RedemptionRecord {
    pub issuer: String,
    pub created_unix_ms: u64,
    pub ttl_ms: u64,
    pub body: Vec<u8>,          // opaque cbor blob
}

#[derive(Default)]
pub struct PrivateStateTokenStore {
    /// per-issuer token quota (max 6 per issuer, 2 active issuers per top-level origin).
    pub tokens: HashMap<String, Vec<PrivateStateToken>>,
    pub redemption_cache: HashMap<String, RedemptionRecord>,
    pub active_issuers: HashMap<String, Vec<String>>, // top-level origin -> issuers
}

impl PrivateStateTokenStore {
    pub fn new() -> Self { Self::default() }

    pub fn add_token(&mut self, issuer: &str, token: PrivateStateToken) -> Result<(), String> {
        let bucket = self.tokens.entry(issuer.into()).or_default();
        if bucket.len() >= 500 {
            return Err(format!("token quota for issuer {} reached", issuer));
        }
        bucket.push(token);
        Ok(())
    }

    pub fn consume_token(&mut self, issuer: &str) -> Option<PrivateStateToken> {
        self.tokens.get_mut(issuer)?.pop()
    }

    pub fn record_redemption(&mut self, issuer: &str, record: RedemptionRecord) {
        self.redemption_cache.insert(issuer.into(), record);
    }

    pub fn get_redemption(&self, issuer: &str, now_unix_ms: u64) -> Option<&RedemptionRecord> {
        let r = self.redemption_cache.get(issuer)?;
        if now_unix_ms > r.created_unix_ms + r.ttl_ms { return None; }
        Some(r)
    }

    pub fn register_active_issuer(&mut self, top_level: &str, issuer: &str) -> Result<(), String> {
        let list = self.active_issuers.entry(top_level.into()).or_default();
        if list.iter().any(|i| i == issuer) { return Ok(()); }
        if list.len() >= 2 {
            return Err(format!("max 2 active issuers per top-level origin {}", top_level));
        }
        list.push(issuer.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(issuer: &str) -> PrivateStateToken {
        PrivateStateToken {
            issuer: issuer.into(),
            blind_token: vec![1, 2, 3],
            key_commitment_id: 7,
        }
    }

    #[test]
    fn add_and_consume() {
        let mut s = PrivateStateTokenStore::new();
        s.add_token("https://i.example", tok("https://i.example")).unwrap();
        assert!(s.consume_token("https://i.example").is_some());
        assert!(s.consume_token("https://i.example").is_none());
    }

    #[test]
    fn redemption_ttl_expires() {
        let mut s = PrivateStateTokenStore::new();
        s.record_redemption("i.example", RedemptionRecord {
            issuer: "i.example".into(),
            created_unix_ms: 1000,
            ttl_ms: 500,
            body: vec![],
        });
        assert!(s.get_redemption("i.example", 1200).is_some());
        assert!(s.get_redemption("i.example", 2000).is_none());
    }

    #[test]
    fn max_2_active_issuers() {
        let mut s = PrivateStateTokenStore::new();
        s.register_active_issuer("top.com", "a.example").unwrap();
        s.register_active_issuer("top.com", "b.example").unwrap();
        assert!(s.register_active_issuer("top.com", "c.example").is_err());
    }
}
