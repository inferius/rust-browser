//! Gamepad API.
//!
//! Spec: https://w3c.github.io/gamepad/
//! navigator.getGamepads() + gamepadconnected/disconnected events.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Gamepad {
    pub index: u32,
    pub id: String,
    pub connected: bool,
    pub buttons: Vec<GamepadButton>,
    pub axes: Vec<f64>,         // -1.0 .. 1.0
    pub mapping: String,        // "" or "standard"
    pub timestamp_ms: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct GamepadButton {
    pub pressed: bool,
    pub touched: bool,
    pub value: f64,             // 0.0 .. 1.0 (trigger analog)
}

impl Default for GamepadButton {
    fn default() -> Self { Self { pressed: false, touched: false, value: 0.0 } }
}

#[derive(Default)]
pub struct GamepadRegistry {
    pub gamepads: HashMap<u32, Gamepad>,
}

impl GamepadRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn connect(&mut self, index: u32, id: &str, button_count: u32, axis_count: u32) {
        self.gamepads.insert(index, Gamepad {
            index, id: id.into(), connected: true,
            buttons: vec![GamepadButton::default(); button_count as usize],
            axes: vec![0.0; axis_count as usize],
            mapping: "standard".into(),
            timestamp_ms: 0.0,
        });
    }

    pub fn disconnect(&mut self, index: u32) {
        if let Some(g) = self.gamepads.get_mut(&index) {
            g.connected = false;
        }
    }

    pub fn update_button(&mut self, index: u32, button: u32, pressed: bool, value: f64) {
        if let Some(g) = self.gamepads.get_mut(&index) {
            if let Some(b) = g.buttons.get_mut(button as usize) {
                b.pressed = pressed;
                b.touched = pressed || value > 0.05;
                b.value = value.clamp(0.0, 1.0);
            }
        }
    }

    pub fn update_axis(&mut self, index: u32, axis: u32, value: f64) {
        if let Some(g) = self.gamepads.get_mut(&index) {
            if let Some(a) = g.axes.get_mut(axis as usize) {
                *a = value.clamp(-1.0, 1.0);
            }
        }
    }

    pub fn list(&self) -> Vec<&Gamepad> {
        self.gamepads.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_creates_gamepad() {
        let mut r = GamepadRegistry::new();
        r.connect(0, "Xbox Wireless", 16, 4);
        assert_eq!(r.list().len(), 1);
        assert_eq!(r.gamepads.get(&0).unwrap().buttons.len(), 16);
        assert_eq!(r.gamepads.get(&0).unwrap().axes.len(), 4);
    }

    #[test]
    fn update_button_state() {
        let mut r = GamepadRegistry::new();
        r.connect(0, "Pad", 4, 2);
        r.update_button(0, 0, true, 1.0);
        let g = r.gamepads.get(&0).unwrap();
        assert!(g.buttons[0].pressed);
        assert_eq!(g.buttons[0].value, 1.0);
    }

    #[test]
    fn update_axis_clamped() {
        let mut r = GamepadRegistry::new();
        r.connect(0, "Pad", 4, 2);
        r.update_axis(0, 1, 5.0);
        assert_eq!(r.gamepads.get(&0).unwrap().axes[1], 1.0);
        r.update_axis(0, 1, -5.0);
        assert_eq!(r.gamepads.get(&0).unwrap().axes[1], -1.0);
    }

    #[test]
    fn disconnect_marks() {
        let mut r = GamepadRegistry::new();
        r.connect(0, "Pad", 4, 2);
        r.disconnect(0);
        assert!(!r.gamepads.get(&0).unwrap().connected);
    }
}
