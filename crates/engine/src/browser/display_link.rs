//! DisplayLink / refresh-rate detection across multi-monitor + VRR setups.

#[derive(Debug, Clone, Copy)]
pub struct DisplayLinkInfo {
    pub primary_hz: f32,
    pub secondary_hz: Option<f32>,
    pub vrr_supported: bool,           // variable refresh rate
    pub vrr_min_hz: Option<f32>,
    pub vrr_max_hz: Option<f32>,
}

impl Default for DisplayLinkInfo {
    fn default() -> Self {
        Self { primary_hz: 60.0, secondary_hz: None, vrr_supported: false, vrr_min_hz: None, vrr_max_hz: None }
    }
}

impl DisplayLinkInfo {
    pub fn frame_period_ms(&self) -> f32 {
        1000.0 / self.primary_hz
    }

    pub fn target_hz_for_workload(&self, workload_hz: f32) -> f32 {
        if !self.vrr_supported { return self.primary_hz; }
        let lo = self.vrr_min_hz.unwrap_or(self.primary_hz);
        let hi = self.vrr_max_hz.unwrap_or(self.primary_hz);
        workload_hz.clamp(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_60hz() {
        let d = DisplayLinkInfo::default();
        assert!((d.frame_period_ms() - 16.666).abs() < 0.01);
    }

    #[test]
    fn vrr_clamps_to_range() {
        let d = DisplayLinkInfo {
            primary_hz: 144.0,
            secondary_hz: None,
            vrr_supported: true,
            vrr_min_hz: Some(48.0),
            vrr_max_hz: Some(165.0),
        };
        assert!((d.target_hz_for_workload(30.0) - 48.0).abs() < 0.01);
        assert!((d.target_hz_for_workload(200.0) - 165.0).abs() < 0.01);
    }

    #[test]
    fn no_vrr_uses_primary() {
        let d = DisplayLinkInfo::default();
        assert!((d.target_hz_for_workload(120.0) - 60.0).abs() < 0.01);
    }
}
