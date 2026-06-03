//! Battery Status API.
//!
//! Spec: https://www.w3.org/TR/battery-status/
//! Note: deprecated due to fingerprinting; many browsers no longer expose detailed info.

#[derive(Debug, Clone, Copy)]
pub struct BatteryStatus {
    pub charging: bool,
    pub charging_time_sec: Option<f64>,    // INFINITY when not charging
    pub discharging_time_sec: Option<f64>, // INFINITY when charging
    pub level: f32,                        // 0.0 - 1.0
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self {
            charging: true,
            charging_time_sec: Some(0.0),
            discharging_time_sec: None,
            level: 1.0,
        }
    }
}

impl BatteryStatus {
    pub fn from_os(charging: bool, level_percent: u8, charging_time: Option<f64>, discharging_time: Option<f64>) -> Self {
        Self {
            charging,
            charging_time_sec: charging_time,
            discharging_time_sec: discharging_time,
            level: (level_percent as f32 / 100.0).clamp(0.0, 1.0),
        }
    }

    /// Anonymize per spec: cap precision + bucket level.
    pub fn anonymized(&self) -> Self {
        let bucketed = (self.level * 100.0).round() / 100.0;
        let bucketed = (bucketed * 10.0).round() / 10.0;          // 10% buckets
        Self {
            charging: self.charging,
            charging_time_sec: self.charging_time_sec.map(|t| (t / 60.0).round() * 60.0),
            discharging_time_sec: self.discharging_time_sec.map(|t| (t / 60.0).round() * 60.0),
            level: bucketed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_os_clamps_level() {
        let b = BatteryStatus::from_os(false, 150, None, Some(1000.0));
        assert!(b.level <= 1.0);
    }

    #[test]
    fn anonymize_buckets() {
        let b = BatteryStatus::from_os(false, 73, None, Some(125.0));
        let a = b.anonymized();
        // 0.73 -> nearest 0.1 = 0.7
        assert!((a.level - 0.7).abs() < 0.01);
        // 125 sec -> nearest minute = 120
        assert_eq!(a.discharging_time_sec, Some(120.0));
    }

    #[test]
    fn default_is_charged() {
        let b = BatteryStatus::default();
        assert!(b.charging);
        assert_eq!(b.level, 1.0);
    }
}
