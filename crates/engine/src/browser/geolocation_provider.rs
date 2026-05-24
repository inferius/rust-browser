//! Geolocation API provider - OS bridge + Wi-Fi/IP fallback.

#[derive(Debug, Clone, Copy)]
pub struct GeoCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_m: f64,
    pub altitude_m: Option<f64>,
    pub altitude_accuracy_m: Option<f64>,
    pub heading_deg: Option<f64>,
    pub speed_mps: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionError {
    PermissionDenied,
    PositionUnavailable,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccuracyMode {
    Low,           // city level (IP geolocation)
    Approximate,   // a few km (Wi-Fi triangulation)
    High,          // GPS / GNSS / fused (~10 m)
}

#[derive(Debug, Clone)]
pub struct GeoOptions {
    pub enable_high_accuracy: bool,
    pub timeout_ms: u32,
    pub maximum_age_ms: u32,
}

impl Default for GeoOptions {
    fn default() -> Self {
        Self { enable_high_accuracy: false, timeout_ms: 0, maximum_age_ms: 0 }
    }
}

#[derive(Default)]
pub struct GeoProvider {
    pub last_position: Option<(GeoCoordinates, u64)>,
    pub mode: Option<AccuracyMode>,
    pub permission_granted: bool,
    pub permission_persistent: bool,
}

impl GeoProvider {
    pub fn new() -> Self { Self::default() }

    pub fn permit(&mut self, persistent: bool) {
        self.permission_granted = true;
        self.permission_persistent = persistent;
    }

    pub fn deny(&mut self) {
        self.permission_granted = false;
    }

    pub fn report_position(&mut self, coords: GeoCoordinates, mode: AccuracyMode, now: u64) {
        self.last_position = Some((coords, now));
        self.mode = Some(mode);
    }

    pub fn get_current(&self, opts: &GeoOptions, now: u64) -> Result<GeoCoordinates, PositionError> {
        if !self.permission_granted { return Err(PositionError::PermissionDenied); }
        let (coords, ts) = self.last_position.ok_or(PositionError::PositionUnavailable)?;
        if opts.maximum_age_ms > 0 && now - ts > opts.maximum_age_ms as u64 {
            return Err(PositionError::Timeout);
        }
        Ok(coords)
    }

    pub fn accuracy_meters_estimate(&self) -> f64 {
        match self.mode {
            Some(AccuracyMode::High) => 10.0,
            Some(AccuracyMode::Approximate) => 500.0,
            Some(AccuracyMode::Low) => 5000.0,
            None => f64::INFINITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coords() -> GeoCoordinates {
        GeoCoordinates {
            latitude: 50.0, longitude: 14.0,
            accuracy_m: 10.0,
            altitude_m: None, altitude_accuracy_m: None,
            heading_deg: None, speed_mps: None,
        }
    }

    #[test]
    fn permission_required() {
        let p = GeoProvider::new();
        let r = p.get_current(&GeoOptions::default(), 0);
        assert_eq!(r.unwrap_err(), PositionError::PermissionDenied);
    }

    #[test]
    fn returns_last_position() {
        let mut p = GeoProvider::new();
        p.permit(false);
        p.report_position(coords(), AccuracyMode::High, 0);
        let r = p.get_current(&GeoOptions::default(), 0);
        assert!(r.is_ok());
    }

    #[test]
    fn max_age_enforced() {
        let mut p = GeoProvider::new();
        p.permit(false);
        p.report_position(coords(), AccuracyMode::High, 0);
        let mut o = GeoOptions::default();
        o.maximum_age_ms = 1000;
        let r = p.get_current(&o, 5000);
        assert_eq!(r.unwrap_err(), PositionError::Timeout);
    }

    #[test]
    fn accuracy_estimate_per_mode() {
        let mut p = GeoProvider::new();
        p.mode = Some(AccuracyMode::High);
        assert_eq!(p.accuracy_meters_estimate(), 10.0);
        p.mode = Some(AccuracyMode::Low);
        assert_eq!(p.accuracy_meters_estimate(), 5000.0);
    }
}
