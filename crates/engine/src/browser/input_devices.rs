//! Physical input device enumeration + capability flags.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Touchscreen,
    Touchpad,
    Pen,
    Joystick,
    Gamepad,
    Eye,                // eye-tracking
    Voice,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: u64,
    pub kind: DeviceKind,
    pub name: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub supports_pressure: bool,
    pub supports_tilt: bool,
    pub max_touch_points: u32,
    pub primary: bool,
}

#[derive(Default)]
pub struct DeviceRegistry {
    pub devices: Vec<DeviceInfo>,
}

impl DeviceRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, dev: DeviceInfo) {
        if let Some(existing) = self.devices.iter_mut().find(|d| d.id == dev.id) {
            *existing = dev;
        } else {
            self.devices.push(dev);
        }
    }

    pub fn unregister(&mut self, id: u64) {
        self.devices.retain(|d| d.id != id);
    }

    pub fn by_kind(&self, kind: DeviceKind) -> Vec<&DeviceInfo> {
        self.devices.iter().filter(|d| d.kind == kind).collect()
    }

    pub fn has_touch(&self) -> bool {
        self.devices.iter().any(|d| d.kind == DeviceKind::Touchscreen)
    }

    pub fn primary_pointing(&self) -> Option<&DeviceInfo> {
        self.devices.iter().find(|d| d.primary && matches!(d.kind, DeviceKind::Mouse | DeviceKind::Touchpad | DeviceKind::Pen))
    }

    /// Implements navigator.maxTouchPoints.
    pub fn max_touch_points(&self) -> u32 {
        self.devices.iter()
            .filter(|d| d.kind == DeviceKind::Touchscreen)
            .map(|d| d.max_touch_points)
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: u64, kind: DeviceKind) -> DeviceInfo {
        DeviceInfo {
            id, kind,
            name: "test".into(),
            vendor_id: None, product_id: None,
            supports_pressure: false, supports_tilt: false,
            max_touch_points: 0, primary: false,
        }
    }

    #[test]
    fn register_unique() {
        let mut r = DeviceRegistry::new();
        r.register(dev(1, DeviceKind::Mouse));
        r.register(dev(1, DeviceKind::Mouse));
        assert_eq!(r.devices.len(), 1);
    }

    #[test]
    fn unregister() {
        let mut r = DeviceRegistry::new();
        r.register(dev(1, DeviceKind::Mouse));
        r.unregister(1);
        assert!(r.devices.is_empty());
    }

    #[test]
    fn touch_capability() {
        let mut r = DeviceRegistry::new();
        let mut t = dev(1, DeviceKind::Touchscreen);
        t.max_touch_points = 10;
        r.register(t);
        assert!(r.has_touch());
        assert_eq!(r.max_touch_points(), 10);
    }

    #[test]
    fn primary_pointing() {
        let mut r = DeviceRegistry::new();
        let mut m = dev(1, DeviceKind::Mouse);
        m.primary = true;
        r.register(m);
        assert!(r.primary_pointing().is_some());
    }

    #[test]
    fn by_kind_filter() {
        let mut r = DeviceRegistry::new();
        r.register(dev(1, DeviceKind::Mouse));
        r.register(dev(2, DeviceKind::Keyboard));
        r.register(dev(3, DeviceKind::Keyboard));
        assert_eq!(r.by_kind(DeviceKind::Keyboard).len(), 2);
    }
}
