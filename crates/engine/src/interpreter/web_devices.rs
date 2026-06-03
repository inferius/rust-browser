//! Web Bluetooth / USB / HID / Serial / NFC API stubs.
//!
//! Specs:
//! - Bluetooth: https://webbluetoothcg.github.io/web-bluetooth/
//! - USB: https://wicg.github.io/webusb/
//! - HID: https://wicg.github.io/webhid/
//! - Serial: https://wicg.github.io/serial/
//! - NFC: https://w3c.github.io/web-nfc/
//!
//! Foundation: device enumeration + permission flow. Real impl per OS pres
//! native APIs (btleplug Rust crate, hidapi, serialport).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceTransport {
    Bluetooth,
    Usb,
    Hid,
    Serial,
    Nfc,
}

#[derive(Debug, Clone)]
pub struct WebDevice {
    pub id: u64,
    pub transport: DeviceTransport,
    pub name: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub connected: bool,
}

#[derive(Default)]
pub struct DeviceRegistry {
    pub devices: HashMap<u64, WebDevice>,
    pub paired: HashMap<(String, DeviceTransport), Vec<u64>>, // origin -> device IDs
    pub next_id: u64,
}

impl DeviceRegistry {
    pub fn new() -> Self { Self::default() }

    /// Request device pairing - user gesto required (foundation auto-grant).
    pub fn pair(&mut self, origin: &str, transport: DeviceTransport, name: &str) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.devices.insert(id, WebDevice {
            id, transport, name: name.into(),
            vendor_id: None, product_id: None,
            connected: false,
        });
        self.paired.entry((origin.into(), transport)).or_default().push(id);
        id
    }

    pub fn connect(&mut self, id: u64) -> bool {
        if let Some(d) = self.devices.get_mut(&id) {
            d.connected = true;
            return true;
        }
        false
    }

    pub fn disconnect(&mut self, id: u64) {
        if let Some(d) = self.devices.get_mut(&id) { d.connected = false; }
    }

    pub fn get_devices(&self, origin: &str, transport: DeviceTransport) -> Vec<&WebDevice> {
        let key = (origin.to_string(), transport);
        let ids = self.paired.get(&key).cloned().unwrap_or_default();
        ids.into_iter().filter_map(|id| self.devices.get(&id)).collect()
    }

    pub fn forget(&mut self, origin: &str, id: u64) {
        for (_, list) in self.paired.iter_mut() {
            list.retain(|x| *x != id);
        }
        let _ = origin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_and_query() {
        let mut r = DeviceRegistry::new();
        let id = r.pair("https://x.com", DeviceTransport::Bluetooth, "Headset");
        let list = r.get_devices("https://x.com", DeviceTransport::Bluetooth);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
    }

    #[test]
    fn origin_isolation() {
        let mut r = DeviceRegistry::new();
        r.pair("https://a.com", DeviceTransport::Usb, "Printer");
        assert_eq!(r.get_devices("https://b.com", DeviceTransport::Usb).len(), 0);
    }

    #[test]
    fn connect_disconnect() {
        let mut r = DeviceRegistry::new();
        let id = r.pair("https://x.com", DeviceTransport::Hid, "Gamepad");
        r.connect(id);
        assert!(r.devices.get(&id).unwrap().connected);
        r.disconnect(id);
        assert!(!r.devices.get(&id).unwrap().connected);
    }

    #[test]
    fn transport_isolation() {
        let mut r = DeviceRegistry::new();
        r.pair("https://x.com", DeviceTransport::Bluetooth, "BT");
        assert_eq!(r.get_devices("https://x.com", DeviceTransport::Usb).len(), 0);
    }
}
