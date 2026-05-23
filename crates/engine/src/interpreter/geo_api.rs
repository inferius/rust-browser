//! Geolocation API foundation.
//!
//! Spec: https://www.w3.org/TR/geolocation/
//! Foundation: API surface + permission state. Real GPS access = OS-specific
//! (CoreLocation/Windows.Devices.Geolocation/dbus geoclue).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeoPermissionState {
    Default,
    Granted,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeolocationPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub accuracy: f64,            // meters
    pub altitude_accuracy: Option<f64>,
    pub heading: Option<f64>,     // degrees from north
    pub speed: Option<f64>,       // m/s
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeoError {
    PermissionDenied,
    PositionUnavailable,
    Timeout,
}

#[derive(Default)]
pub struct GeolocationService {
    pub permission: GeoPermissionState,
    /// Stub fixed position (real = OS API).
    pub last_position: Option<GeolocationPosition>,
}

impl GeolocationService {
    pub fn new() -> Self { Self::default() }

    pub fn grant(&mut self) { self.permission = GeoPermissionState::Granted; }
    pub fn deny(&mut self) { self.permission = GeoPermissionState::Denied; }

    /// `getCurrentPosition` - vraci position nebo error.
    pub fn get_current_position(&self) -> Result<GeolocationPosition, GeoError> {
        if self.permission == GeoPermissionState::Denied {
            return Err(GeoError::PermissionDenied);
        }
        if self.permission != GeoPermissionState::Granted {
            return Err(GeoError::PermissionDenied);
        }
        self.last_position.ok_or(GeoError::PositionUnavailable)
    }

    pub fn set_stub_position(&mut self, lat: f64, lng: f64) {
        self.last_position = Some(GeolocationPosition {
            latitude: lat,
            longitude: lng,
            altitude: None,
            accuracy: 50.0,
            altitude_accuracy: None,
            heading: None,
            speed: None,
            timestamp_ms: now_ms(),
        });
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_without_grant() {
        let s = GeolocationService::new();
        assert_eq!(s.get_current_position(), Err(GeoError::PermissionDenied));
    }

    #[test]
    fn granted_returns_position() {
        let mut s = GeolocationService::new();
        s.grant();
        s.set_stub_position(50.0, 14.5);
        let p = s.get_current_position().unwrap();
        assert_eq!(p.latitude, 50.0);
    }

    #[test]
    fn deny_blocks_access() {
        let mut s = GeolocationService::new();
        s.deny();
        s.set_stub_position(0.0, 0.0);
        assert!(s.get_current_position().is_err());
    }
}

impl Default for GeoPermissionState {
    fn default() -> Self { GeoPermissionState::Default }
}
