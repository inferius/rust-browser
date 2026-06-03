//! Network Information API.
//!
//! Spec: https://wicg.github.io/netinfo/
//! navigator.connection.effectiveType / downlink / rtt / saveData.
//! Note: per project CLAUDE.md guidance, `effectiveType` is bandwidth-estimate based,
//! NOT physical transport.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectiveConnectionType {
    Slow2G,        // < 50 kbps
    G2,            // < 70 kbps
    G3,            // < 700 kbps
    G4,            // >= 700 kbps (incl. wired, gigabit, etc.)
    Unknown,
}

impl EffectiveConnectionType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Slow2G => "slow-2g",
            Self::G2 => "2g",
            Self::G3 => "3g",
            Self::G4 => "4g",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_downlink_kbps(kbps: f64) -> Self {
        if kbps < 50.0 { Self::Slow2G }
        else if kbps < 70.0 { Self::G2 }
        else if kbps < 700.0 { Self::G3 }
        else { Self::G4 }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NetworkConditions {
    pub downlink_mbps: f64,
    pub uplink_mbps: f64,
    pub rtt_ms: u32,
    pub save_data: bool,
    pub on_metered: bool,
}

impl Default for NetworkConditions {
    fn default() -> Self {
        Self {
            downlink_mbps: 10.0,
            uplink_mbps: 5.0,
            rtt_ms: 50,
            save_data: false,
            on_metered: false,
        }
    }
}

impl NetworkConditions {
    pub fn effective_type(&self) -> EffectiveConnectionType {
        EffectiveConnectionType::from_downlink_kbps(self.downlink_mbps * 1000.0)
    }

    /// Rounded values per spec to limit fingerprinting.
    pub fn rounded(&self) -> Self {
        Self {
            downlink_mbps: (self.downlink_mbps * 4.0).round() / 4.0,  // 0.25 Mbps buckets
            uplink_mbps: (self.uplink_mbps * 4.0).round() / 4.0,
            rtt_ms: ((self.rtt_ms / 25) * 25),                          // 25 ms buckets
            save_data: self.save_data,
            on_metered: self.on_metered,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_4g_for_fast() {
        let n = NetworkConditions { downlink_mbps: 10.0, ..Default::default() };
        assert_eq!(n.effective_type(), EffectiveConnectionType::G4);
    }

    #[test]
    fn effective_slow_2g_for_dial_up() {
        let n = NetworkConditions { downlink_mbps: 0.040, ..Default::default() };
        assert_eq!(n.effective_type(), EffectiveConnectionType::Slow2G);
    }

    #[test]
    fn name_format() {
        assert_eq!(EffectiveConnectionType::Slow2G.name(), "slow-2g");
        assert_eq!(EffectiveConnectionType::G4.name(), "4g");
    }

    #[test]
    fn rounded_buckets() {
        let n = NetworkConditions {
            downlink_mbps: 0.37, rtt_ms: 123,
            ..Default::default()
        };
        let r = n.rounded();
        // 0.37 -> nearest 0.25 = 0.25
        assert_eq!(r.downlink_mbps, 0.25);
        assert_eq!(r.rtt_ms, 100);
    }

    #[test]
    fn save_data_preserved() {
        let n = NetworkConditions { save_data: true, ..Default::default() };
        let r = n.rounded();
        assert!(r.save_data);
    }
}
