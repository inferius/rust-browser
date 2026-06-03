//! Screen Orientation API + lock state.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenOrientationType {
    PortraitPrimary,
    PortraitSecondary,
    LandscapePrimary,
    LandscapeSecondary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrientationLockType {
    Any,
    Natural,
    Landscape,
    LandscapePrimary,
    LandscapeSecondary,
    Portrait,
    PortraitPrimary,
    PortraitSecondary,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenOrientationState {
    pub current: ScreenOrientationType,
    pub angle: u16,                    // 0 / 90 / 180 / 270
    pub locked: Option<OrientationLockType>,
}

impl Default for ScreenOrientationState {
    fn default() -> Self {
        Self {
            current: ScreenOrientationType::PortraitPrimary,
            angle: 0,
            locked: None,
        }
    }
}

impl ScreenOrientationState {
    pub fn lock(&mut self, target: OrientationLockType) -> Result<(), String> {
        self.locked = Some(target);
        Ok(())
    }

    pub fn unlock(&mut self) {
        self.locked = None;
    }

    /// Apply a device rotation; honor lock if active.
    pub fn update(&mut self, new: ScreenOrientationType, new_angle: u16) -> bool {
        if let Some(lock) = self.locked {
            if !lock_allows(lock, new) {
                return false;
            }
        }
        let changed = self.current != new;
        self.current = new;
        self.angle = new_angle;
        changed
    }
}

pub fn lock_allows(lock: OrientationLockType, current: ScreenOrientationType) -> bool {
    use ScreenOrientationType::*;
    match lock {
        OrientationLockType::Any | OrientationLockType::Natural => true,
        OrientationLockType::Landscape =>
            matches!(current, LandscapePrimary | LandscapeSecondary),
        OrientationLockType::Portrait =>
            matches!(current, PortraitPrimary | PortraitSecondary),
        OrientationLockType::LandscapePrimary => matches!(current, LandscapePrimary),
        OrientationLockType::LandscapeSecondary => matches!(current, LandscapeSecondary),
        OrientationLockType::PortraitPrimary => matches!(current, PortraitPrimary),
        OrientationLockType::PortraitSecondary => matches!(current, PortraitSecondary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_unlock() {
        let mut s = ScreenOrientationState::default();
        s.lock(OrientationLockType::Landscape).unwrap();
        assert!(s.locked.is_some());
        s.unlock();
        assert!(s.locked.is_none());
    }

    #[test]
    fn lock_blocks_perpendicular_rotation() {
        let mut s = ScreenOrientationState::default();
        s.lock(OrientationLockType::Portrait).unwrap();
        let changed = s.update(ScreenOrientationType::LandscapePrimary, 90);
        assert!(!changed);
    }

    #[test]
    fn lock_permits_compatible_rotation() {
        let mut s = ScreenOrientationState::default();
        s.lock(OrientationLockType::Landscape).unwrap();
        let changed = s.update(ScreenOrientationType::LandscapeSecondary, 270);
        assert!(changed);
        assert_eq!(s.current, ScreenOrientationType::LandscapeSecondary);
    }

    #[test]
    fn any_lock_allows_all() {
        assert!(lock_allows(OrientationLockType::Any, ScreenOrientationType::PortraitPrimary));
        assert!(lock_allows(OrientationLockType::Any, ScreenOrientationType::LandscapeSecondary));
    }
}
