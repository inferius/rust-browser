//! PointerEvent + TouchEvent unified model per Pointer Events L3 spec.
//!
//! Browser unifikuje vsechny vstupy (mouse, touch, pen) do PointerEvent.
//! Legacy TouchEvent + MouseEvent stale fired pro back-compat.
//!
//! PointerEvent props: pointerId, pointerType ("mouse"/"touch"/"pen"),
//! isPrimary, width, height, pressure, tangentialPressure, tiltX/Y, twist.
//!
//! Inspired by Chromium `core/input/pointer_event_manager.cc`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerType {
    Mouse,
    Touch,
    Pen,
}

#[derive(Debug, Clone, Copy)]
pub struct PointerInput {
    pub pointer_id: i32,
    pub pointer_type: PointerType,
    pub is_primary: bool,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,        // 0.0..1.0
    pub tilt_x: i32,          // -90..90 deg
    pub tilt_y: i32,
    pub twist: i32,           // 0..359 deg
    pub width: f32,           // contact ellipse width
    pub height: f32,
    pub buttons: u32,         // bitfield: 1=primary, 2=secondary, 4=middle
}

impl Default for PointerInput {
    fn default() -> Self {
        Self {
            pointer_id: 1,
            pointer_type: PointerType::Mouse,
            is_primary: true,
            x: 0.0, y: 0.0,
            pressure: 0.0,
            tilt_x: 0, tilt_y: 0,
            twist: 0,
            width: 1.0, height: 1.0,
            buttons: 0,
        }
    }
}

/// Active pointers tracking pro multi-touch. Per-id state.
#[derive(Default)]
pub struct PointerTracker {
    pub active: std::collections::HashMap<i32, PointerInput>,
}

impl PointerTracker {
    pub fn new() -> Self { Self::default() }

    /// Pointer down - register pointer.
    pub fn pointer_down(&mut self, p: PointerInput) {
        self.active.insert(p.pointer_id, p);
    }

    /// Pointer move - update existing.
    pub fn pointer_move(&mut self, p: PointerInput) {
        self.active.insert(p.pointer_id, p);
    }

    /// Pointer up - remove from active.
    pub fn pointer_up(&mut self, pointer_id: i32) {
        self.active.remove(&pointer_id);
    }

    /// Vraci primary pointer (= isPrimary=true, ten ktery generates mouse events).
    pub fn primary(&self) -> Option<&PointerInput> {
        self.active.values().find(|p| p.is_primary)
    }

    /// Pocet active pointers (multi-touch count).
    pub fn touch_count(&self) -> usize {
        self.active.values().filter(|p| p.pointer_type == PointerType::Touch).count()
    }
}

/// Mapping na JS event type names:
/// pointerdown / pointermove / pointerup / pointercancel / pointerenter / pointerleave.
pub fn pointer_event_type_down() -> &'static str { "pointerdown" }
pub fn pointer_event_type_move() -> &'static str { "pointermove" }
pub fn pointer_event_type_up() -> &'static str { "pointerup" }
pub fn pointer_event_type_cancel() -> &'static str { "pointercancel" }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_pointer_down_up() {
        let mut t = PointerTracker::new();
        t.pointer_down(PointerInput {
            pointer_id: 1, pointer_type: PointerType::Touch,
            is_primary: true, x: 10.0, y: 20.0, ..Default::default()
        });
        assert_eq!(t.touch_count(), 1);
        t.pointer_up(1);
        assert_eq!(t.touch_count(), 0);
    }

    #[test]
    fn multi_touch_tracking() {
        let mut t = PointerTracker::new();
        for i in 1..=3 {
            t.pointer_down(PointerInput {
                pointer_id: i, pointer_type: PointerType::Touch,
                is_primary: i == 1, x: 10.0 * i as f32, y: 20.0,
                ..Default::default()
            });
        }
        assert_eq!(t.touch_count(), 3);
        assert_eq!(t.primary().unwrap().pointer_id, 1);
    }

    #[test]
    fn pointer_move_updates_position() {
        let mut t = PointerTracker::new();
        let p = PointerInput { pointer_id: 1, x: 10.0, y: 20.0, ..Default::default() };
        t.pointer_down(p);
        t.pointer_move(PointerInput { pointer_id: 1, x: 50.0, y: 60.0, ..Default::default() });
        assert_eq!(t.active.get(&1).unwrap().x, 50.0);
    }
}
