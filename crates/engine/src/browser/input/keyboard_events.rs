//! Keyboard events - KeyboardEvent.code + key + KeyboardEvent.location.
//!
//! Spec: https://www.w3.org/TR/uievents-key/ + https://www.w3.org/TR/uievents-code/.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyLocation {
    Standard,
    Left,
    Right,
    Numpad,
}

#[derive(Debug, Clone)]
pub struct KeyboardEventDescriptor {
    pub key: String,        // logical: "a", "Shift", "Enter", "F1", "Dead"...
    pub code: String,       // physical: "KeyA", "ShiftLeft", "Enter", "F1"
    pub location: KeyLocation,
    pub is_composing: bool,
    pub repeat: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub meta: bool,
}

impl KeyboardEventDescriptor {
    pub fn modifiers_active(&self) -> u8 {
        (self.alt as u8) | ((self.ctrl as u8) << 1) | ((self.shift as u8) << 2) | ((self.meta as u8) << 3)
    }
}

/// Resolve KeyboardEvent.key from raw text input + modifier state.
/// Real impl uses platform layout. This handles common cases:
/// - Single printable char -> uppercase pri Shift
/// - Named keys (Enter, Tab, ...) passed through
pub fn resolve_key(raw_text: Option<&str>, code: &str, shift: bool) -> String {
    if let Some(t) = raw_text {
        if shift {
            return t.to_uppercase();
        }
        return t.to_string();
    }
    code_to_dead_key(code).unwrap_or_else(|| code.to_string())
}

fn code_to_dead_key(code: &str) -> Option<String> {
    Some(match code {
        "Backspace" => "Backspace".into(),
        "Tab" => "Tab".into(),
        "Enter" => "Enter".into(),
        "ShiftLeft" | "ShiftRight" => "Shift".into(),
        "ControlLeft" | "ControlRight" => "Control".into(),
        "AltLeft" | "AltRight" => "Alt".into(),
        "MetaLeft" | "MetaRight" => "Meta".into(),
        "CapsLock" => "CapsLock".into(),
        "Escape" => "Escape".into(),
        "Space" => " ".into(),
        "ArrowUp" => "ArrowUp".into(),
        "ArrowDown" => "ArrowDown".into(),
        "ArrowLeft" => "ArrowLeft".into(),
        "ArrowRight" => "ArrowRight".into(),
        "Home" => "Home".into(),
        "End" => "End".into(),
        "PageUp" => "PageUp".into(),
        "PageDown" => "PageDown".into(),
        "Delete" => "Delete".into(),
        "Insert" => "Insert".into(),
        c if c.starts_with("F") && c[1..].parse::<u32>().is_ok() => c.to_string(),
        _ => return None,
    })
}

/// Detect KeyLocation by code.
pub fn key_location(code: &str) -> KeyLocation {
    if code.ends_with("Left") { KeyLocation::Left }
    else if code.ends_with("Right") { KeyLocation::Right }
    else if code.starts_with("Numpad") { KeyLocation::Numpad }
    else { KeyLocation::Standard }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_with_shift() {
        let k = resolve_key(Some("a"), "KeyA", true);
        assert_eq!(k, "A");
    }

    #[test]
    fn resolve_named_key() {
        let k = resolve_key(None, "Enter", false);
        assert_eq!(k, "Enter");
    }

    #[test]
    fn resolve_space() {
        let k = resolve_key(None, "Space", false);
        assert_eq!(k, " ");
    }

    #[test]
    fn key_location_left() {
        assert_eq!(key_location("ShiftLeft"), KeyLocation::Left);
        assert_eq!(key_location("ShiftRight"), KeyLocation::Right);
        assert_eq!(key_location("Numpad1"), KeyLocation::Numpad);
        assert_eq!(key_location("KeyA"), KeyLocation::Standard);
    }

    #[test]
    fn modifiers_bitfield() {
        let k = KeyboardEventDescriptor {
            key: "a".into(), code: "KeyA".into(),
            location: KeyLocation::Standard,
            is_composing: false, repeat: false,
            alt: false, ctrl: true, shift: true, meta: false,
        };
        assert_eq!(k.modifiers_active(), 0b0110);
    }
}
