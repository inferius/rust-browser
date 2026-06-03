//! Device sensor APIs - DeviceMotionEvent, DeviceOrientationEvent, Battery,
//! NetworkInformation.
//!
//! Specs:
//! - DeviceOrientation: https://w3c.github.io/deviceorientation/
//! - Battery: https://www.w3.org/TR/battery-status/
//! - Network Information: https://wicg.github.io/netinfo/

#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceOrientation {
    pub alpha: Option<f64>,  // rotation around z (0..360)
    pub beta: Option<f64>,   // rotation around x (-180..180)
    pub gamma: Option<f64>,  // rotation around y (-90..90)
    pub absolute: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceMotion {
    pub acceleration: (f64, f64, f64),
    pub acceleration_including_gravity: (f64, f64, f64),
    pub rotation_rate: (f64, f64, f64),
    pub interval_ms: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct BatteryStatus {
    pub charging: bool,
    pub charging_time_seconds: f64,    // f64::INFINITY pri unknown
    pub discharging_time_seconds: f64,
    pub level: f32,                    // 0.0..1.0
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self {
            charging: true,
            charging_time_seconds: f64::INFINITY,
            discharging_time_seconds: f64::INFINITY,
            level: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectiveConnectionType {
    Slow2g,
    Type2g,
    Type3g,
    Type4g,
}

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub effective_type: EffectiveConnectionType,
    pub downlink_mbps: f32,
    pub rtt_ms: u32,
    pub save_data: bool,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            effective_type: EffectiveConnectionType::Type4g,
            downlink_mbps: 10.0,
            rtt_ms: 50,
            save_data: false,
        }
    }
}

impl EffectiveConnectionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Slow2g => "slow-2g",
            Self::Type2g => "2g",
            Self::Type3g => "3g",
            Self::Type4g => "4g",
        }
    }
}

#[derive(Default)]
pub struct DeviceServices {
    pub orientation: DeviceOrientation,
    pub motion: DeviceMotion,
    pub battery: BatteryStatus,
    pub network: NetworkInfo,
}

impl DeviceServices {
    pub fn new() -> Self { Self::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_default_full_charging() {
        let b = BatteryStatus::default();
        assert!(b.charging);
        assert_eq!(b.level, 1.0);
    }

    #[test]
    fn network_default_4g() {
        let n = NetworkInfo::default();
        assert_eq!(n.effective_type, EffectiveConnectionType::Type4g);
        assert_eq!(n.effective_type.as_str(), "4g");
    }

    #[test]
    fn orientation_defaults_none() {
        let o = DeviceOrientation::default();
        assert!(o.alpha.is_none());
        assert!(!o.absolute);
    }

    #[test]
    fn ect_string_mapping() {
        assert_eq!(EffectiveConnectionType::Slow2g.as_str(), "slow-2g");
        assert_eq!(EffectiveConnectionType::Type3g.as_str(), "3g");
    }
}
