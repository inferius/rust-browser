//! Per-site zoom level persistence.
//!
//! Chrome maps zoom factor logarithmically; integer steps via formula:
//! zoom_factor = pow(1.2, n) where n in [-10, 14].

use std::collections::HashMap;

#[derive(Default)]
pub struct ZoomLevelStore {
    /// Per-host zoom level (integer step; 0 = default 100%).
    pub per_host: HashMap<String, i32>,
    pub default_step: i32,
    pub min_step: i32,
    pub max_step: i32,
}

impl ZoomLevelStore {
    pub fn new() -> Self {
        Self {
            per_host: HashMap::new(),
            default_step: 0,
            min_step: -10,
            max_step: 14,
        }
    }

    pub fn step_for(&self, host: &str) -> i32 {
        *self.per_host.get(host).unwrap_or(&self.default_step)
    }

    pub fn set_step(&mut self, host: &str, step: i32) {
        let s = step.clamp(self.min_step, self.max_step);
        if s == self.default_step {
            self.per_host.remove(host);
        } else {
            self.per_host.insert(host.into(), s);
        }
    }

    pub fn zoom_in(&mut self, host: &str) -> i32 {
        let s = self.step_for(host) + 1;
        self.set_step(host, s);
        self.step_for(host)
    }

    pub fn zoom_out(&mut self, host: &str) -> i32 {
        let s = self.step_for(host) - 1;
        self.set_step(host, s);
        self.step_for(host)
    }

    pub fn reset(&mut self, host: &str) {
        self.per_host.remove(host);
    }

    pub fn factor_for(&self, host: &str) -> f32 {
        let s = self.step_for(host);
        zoom_factor_for_step(s)
    }
}

/// Spec: factor = 1.2^step.
pub fn zoom_factor_for_step(step: i32) -> f32 {
    1.2f32.powi(step)
}

/// Inverse: zoom factor -> nearest integer step.
pub fn step_for_factor(factor: f32) -> i32 {
    if factor <= 0.0 { return 0; }
    (factor.ln() / 1.2f32.ln()).round() as i32
}

/// Round to common percentages (25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500).
pub fn percentage_for_step(step: i32) -> u32 {
    let factor = zoom_factor_for_step(step);
    (factor * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_step_zero() {
        let s = ZoomLevelStore::new();
        assert_eq!(s.step_for("x.com"), 0);
        assert_eq!(s.factor_for("x.com"), 1.0);
    }

    #[test]
    fn zoom_in_increments() {
        let mut s = ZoomLevelStore::new();
        s.zoom_in("x.com");
        assert_eq!(s.step_for("x.com"), 1);
        let f = s.factor_for("x.com");
        assert!((f - 1.2).abs() < 0.001);
    }

    #[test]
    fn zoom_clamped() {
        let mut s = ZoomLevelStore::new();
        for _ in 0..20 { s.zoom_in("x.com"); }
        assert!(s.step_for("x.com") <= s.max_step);
    }

    #[test]
    fn reset_removes() {
        let mut s = ZoomLevelStore::new();
        s.zoom_in("x.com");
        s.reset("x.com");
        assert!(!s.per_host.contains_key("x.com"));
    }

    #[test]
    fn step_factor_roundtrip() {
        let f = zoom_factor_for_step(3);
        let s = step_for_factor(f);
        assert_eq!(s, 3);
    }

    #[test]
    fn percentage_at_zero() {
        assert_eq!(percentage_for_step(0), 100);
    }
}
