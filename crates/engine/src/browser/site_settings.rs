//! Per-site permissions + content settings.
//!
//! Chromium reference: components/content_settings.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentSetting {
    JavaScript,
    Images,
    Cookies,
    Popups,
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    MidiSysex,
    FullScreen,
    MouseLock,
    Bluetooth,
    Usb,
    Serial,
    Hid,
    Nfc,
    ClipboardRead,
    ClipboardWrite,
    PaymentHandler,
    PersistentStorage,
    BackgroundSync,
    AmbientLightSensor,
    AccelerometerSensor,
    GyroscopeSensor,
    MagnetometerSensor,
    AutoplaySound,
    ProtectedMediaIdentifier,
    StorageAccess,
    IdleDetection,
    FileSystemWrite,
    WindowManagement,
    LocalFonts,
    FedCm,
    Vr,
    Ar,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingValue {
    Allow,
    Block,
    Ask,
    SessionOnly,
    Default,
}

#[derive(Default)]
pub struct SiteSettings {
    /// Global default per setting.
    pub global_default: HashMap<ContentSetting, SettingValue>,
    /// Per-origin override.
    pub per_origin: HashMap<(String, ContentSetting), SettingValue>,
}

impl SiteSettings {
    pub fn new() -> Self {
        let mut s = Self::default();
        // Sensible defaults: media APIs ask, others allow.
        s.global_default.insert(ContentSetting::Camera, SettingValue::Ask);
        s.global_default.insert(ContentSetting::Microphone, SettingValue::Ask);
        s.global_default.insert(ContentSetting::Geolocation, SettingValue::Ask);
        s.global_default.insert(ContentSetting::Notifications, SettingValue::Ask);
        s.global_default.insert(ContentSetting::ClipboardRead, SettingValue::Ask);
        s.global_default.insert(ContentSetting::Popups, SettingValue::Block);
        s.global_default.insert(ContentSetting::JavaScript, SettingValue::Allow);
        s.global_default.insert(ContentSetting::Images, SettingValue::Allow);
        s.global_default.insert(ContentSetting::Cookies, SettingValue::Allow);
        s
    }

    pub fn set_origin(&mut self, origin: &str, setting: ContentSetting, value: SettingValue) {
        self.per_origin.insert((origin.into(), setting), value);
    }

    pub fn set_global(&mut self, setting: ContentSetting, value: SettingValue) {
        self.global_default.insert(setting, value);
    }

    pub fn effective(&self, origin: &str, setting: ContentSetting) -> SettingValue {
        if let Some(v) = self.per_origin.get(&(origin.into(), setting)) {
            return *v;
        }
        self.global_default.get(&setting).copied().unwrap_or(SettingValue::Default)
    }

    pub fn clear_origin(&mut self, origin: &str) {
        self.per_origin.retain(|(o, _), _| o != origin);
    }

    pub fn clear_setting(&mut self, setting: ContentSetting) {
        self.per_origin.retain(|(_, s), _| *s != setting);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_asks() {
        let s = SiteSettings::new();
        assert_eq!(s.effective("https://x.com", ContentSetting::Camera), SettingValue::Ask);
    }

    #[test]
    fn per_origin_overrides_global() {
        let mut s = SiteSettings::new();
        s.set_origin("https://x.com", ContentSetting::Camera, SettingValue::Allow);
        assert_eq!(s.effective("https://x.com", ContentSetting::Camera), SettingValue::Allow);
        assert_eq!(s.effective("https://y.com", ContentSetting::Camera), SettingValue::Ask);
    }

    #[test]
    fn clear_origin() {
        let mut s = SiteSettings::new();
        s.set_origin("https://x.com", ContentSetting::Camera, SettingValue::Block);
        s.clear_origin("https://x.com");
        assert_eq!(s.effective("https://x.com", ContentSetting::Camera), SettingValue::Ask);
    }

    #[test]
    fn clear_setting_only_for_setting() {
        let mut s = SiteSettings::new();
        s.set_origin("https://x.com", ContentSetting::Camera, SettingValue::Block);
        s.set_origin("https://x.com", ContentSetting::Notifications, SettingValue::Block);
        s.clear_setting(ContentSetting::Camera);
        assert_eq!(s.effective("https://x.com", ContentSetting::Camera), SettingValue::Ask);
        assert_eq!(s.effective("https://x.com", ContentSetting::Notifications), SettingValue::Block);
    }
}
