//! Fenced Frames - sandboxed embed s explicit info-flow restrictions.
//!
//! Spec: https://wicg.github.io/fenced-frame/
//! <fencedframe> tag - vlastni navigation context, ne sdili storage s embedderem.
//! Ureno pro retargeting/measurement bez third-party cookies (Privacy Sandbox).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FencedFrameMode {
    Default,           // Generic embedding
    OpaqueAds,         // FLEDGE/Protected Audience auctions
    SharedStorage,     // Shared Storage selectURL outputs
}

#[derive(Debug, Clone)]
pub struct FencedFrameConfig {
    pub url: Option<String>,        // None pri opaque ads
    pub mode: FencedFrameMode,
    pub container_size: Option<(u32, u32)>,
    /// Allowed reporting destinations (post-impression beacons).
    pub reporting_origins: Vec<String>,
    /// Shared storage budget consumed at navigation time.
    pub budget_cost: f64,
}

impl FencedFrameConfig {
    pub fn new(url: &str) -> Self {
        Self {
            url: Some(url.into()),
            mode: FencedFrameMode::Default,
            container_size: None,
            reporting_origins: Vec::new(),
            budget_cost: 0.0,
        }
    }

    pub fn opaque() -> Self {
        Self {
            url: None,
            mode: FencedFrameMode::OpaqueAds,
            container_size: None,
            reporting_origins: Vec::new(),
            budget_cost: 0.0,
        }
    }
}

#[derive(Default)]
pub struct FencedFrameRegistry {
    pub configs: HashMap<u64, FencedFrameConfig>,
    pub next_id: u64,
    /// Per-origin shared-storage daily budget (epsilon, like FLoC/FLEDGE).
    pub budgets: HashMap<String, f64>,
}

impl FencedFrameRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, config: FencedFrameConfig) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.configs.insert(id, config);
        id
    }

    pub fn get(&self, id: u64) -> Option<&FencedFrameConfig> { self.configs.get(&id) }

    /// Pre-navigation budget check. Returns Err pri exceed.
    pub fn consume_budget(&mut self, origin: &str, cost: f64) -> Result<f64, String> {
        let bucket = self.budgets.entry(origin.into()).or_insert(12.0); // 12 bit ~ FLEDGE default
        if *bucket < cost {
            return Err(format!("budget exceeded for {} (avail {:.2}, need {:.2})", origin, *bucket, cost));
        }
        *bucket -= cost;
        Ok(*bucket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_url() {
        let c = FencedFrameConfig::new("https://ad.example/banner");
        assert_eq!(c.url.as_deref(), Some("https://ad.example/banner"));
    }

    #[test]
    fn opaque_no_url() {
        let c = FencedFrameConfig::opaque();
        assert!(c.url.is_none());
        assert_eq!(c.mode, FencedFrameMode::OpaqueAds);
    }

    #[test]
    fn register_returns_id() {
        let mut r = FencedFrameRegistry::new();
        let id = r.register(FencedFrameConfig::opaque());
        assert!(r.get(id).is_some());
    }

    #[test]
    fn budget_consumed() {
        let mut r = FencedFrameRegistry::new();
        let left = r.consume_budget("ad.example", 5.0).unwrap();
        assert!((left - 7.0).abs() < 0.001);
    }

    #[test]
    fn budget_exceeded() {
        let mut r = FencedFrameRegistry::new();
        r.consume_budget("ad.example", 11.0).unwrap();
        assert!(r.consume_budget("ad.example", 5.0).is_err());
    }
}
