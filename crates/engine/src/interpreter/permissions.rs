//! Permissions API per-feature state + query.
//!
//! Spec: https://www.w3.org/TR/permissions/

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionName {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    Push,
    Midi,
    Speaker,
    ClipboardRead,
    ClipboardWrite,
    BackgroundSync,
    BackgroundFetch,
    DisplayCapture,
    PeriodicBackgroundSync,
    Accelerometer,
    Gyroscope,
    Magnetometer,
    AmbientLightSensor,
    Bluetooth,
    Usb,
    Hid,
    Serial,
    StorageAccess,
    PaymentHandler,
    Persistent,
    XrSpatialTracking,
    WindowManagement,
    Idle,
    NfcStorage,
    Unknown,
}

impl PermissionName {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "geolocation" => Self::Geolocation,
            "notifications" => Self::Notifications,
            "camera" => Self::Camera,
            "microphone" => Self::Microphone,
            "push" => Self::Push,
            "midi" => Self::Midi,
            "speaker" => Self::Speaker,
            "clipboard-read" => Self::ClipboardRead,
            "clipboard-write" => Self::ClipboardWrite,
            "background-sync" => Self::BackgroundSync,
            "background-fetch" => Self::BackgroundFetch,
            "display-capture" => Self::DisplayCapture,
            "periodic-background-sync" => Self::PeriodicBackgroundSync,
            "accelerometer" => Self::Accelerometer,
            "gyroscope" => Self::Gyroscope,
            "magnetometer" => Self::Magnetometer,
            "ambient-light-sensor" => Self::AmbientLightSensor,
            "bluetooth" => Self::Bluetooth,
            "usb" => Self::Usb,
            "hid" => Self::Hid,
            "serial" => Self::Serial,
            "storage-access" => Self::StorageAccess,
            "payment-handler" => Self::PaymentHandler,
            "persistent-storage" => Self::Persistent,
            "xr-spatial-tracking" => Self::XrSpatialTracking,
            "window-management" => Self::WindowManagement,
            "idle-detection" => Self::Idle,
            "nfc" => Self::NfcStorage,
            _ => Self::Unknown,
        }
    }
}

#[derive(Default)]
pub struct PermissionsRegistry {
    /// (origin, permission) -> state
    pub states: HashMap<(String, PermissionName), PermissionState>,
}

impl PermissionsRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn query(&self, origin: &str, name: PermissionName) -> PermissionState {
        self.states.get(&(origin.into(), name)).copied().unwrap_or(PermissionState::Prompt)
    }

    pub fn grant(&mut self, origin: &str, name: PermissionName) {
        self.states.insert((origin.into(), name), PermissionState::Granted);
    }

    pub fn deny(&mut self, origin: &str, name: PermissionName) {
        self.states.insert((origin.into(), name), PermissionState::Denied);
    }

    pub fn revoke(&mut self, origin: &str, name: PermissionName) {
        self.states.remove(&(origin.into(), name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_prompt() {
        let r = PermissionsRegistry::new();
        assert_eq!(r.query("https://x.com", PermissionName::Geolocation), PermissionState::Prompt);
    }

    #[test]
    fn grant_then_query() {
        let mut r = PermissionsRegistry::new();
        r.grant("https://x.com", PermissionName::Camera);
        assert_eq!(r.query("https://x.com", PermissionName::Camera), PermissionState::Granted);
    }

    #[test]
    fn origin_isolation() {
        let mut r = PermissionsRegistry::new();
        r.grant("https://a.com", PermissionName::Notifications);
        assert_eq!(r.query("https://b.com", PermissionName::Notifications), PermissionState::Prompt);
    }

    #[test]
    fn revoke_resets_to_prompt() {
        let mut r = PermissionsRegistry::new();
        r.grant("https://x.com", PermissionName::Push);
        r.revoke("https://x.com", PermissionName::Push);
        assert_eq!(r.query("https://x.com", PermissionName::Push), PermissionState::Prompt);
    }

    #[test]
    fn parse_known_names() {
        assert_eq!(PermissionName::parse("geolocation"), PermissionName::Geolocation);
        assert_eq!(PermissionName::parse("clipboard-read"), PermissionName::ClipboardRead);
        assert_eq!(PermissionName::parse("xyzzy"), PermissionName::Unknown);
    }
}
