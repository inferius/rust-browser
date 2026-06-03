//! Picture-in-Picture API - <video> floating window.
//!
//! Spec: https://w3c.github.io/picture-in-picture/

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PipState {
    None,
    Entering,
    Active,
    Exiting,
}

pub struct PictureInPictureWindow {
    pub video_id: usize,
    pub width: u32,
    pub height: u32,
    pub state: PipState,
}

#[derive(Default)]
pub struct PipRegistry {
    pub active: Option<PictureInPictureWindow>,
    pub disabled_videos: HashSet<usize>,
}

impl PipRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn enter(&mut self, video_id: usize, w: u32, h: u32) -> bool {
        if self.disabled_videos.contains(&video_id) { return false; }
        // Only one PiP at a time.
        if self.active.is_some() { return false; }
        self.active = Some(PictureInPictureWindow {
            video_id,
            width: w,
            height: h,
            state: PipState::Active,
        });
        true
    }

    pub fn exit(&mut self) -> bool {
        if self.active.is_some() {
            self.active = None;
            true
        } else { false }
    }

    pub fn disable(&mut self, video_id: usize) {
        self.disabled_videos.insert(video_id);
        if let Some(w) = &self.active {
            if w.video_id == video_id { self.exit(); }
        }
    }

    pub fn is_active_for(&self, video_id: usize) -> bool {
        self.active.as_ref().map(|w| w.video_id == video_id).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_and_exit() {
        let mut r = PipRegistry::new();
        assert!(r.enter(1, 320, 240));
        assert!(r.is_active_for(1));
        r.exit();
        assert!(!r.is_active_for(1));
    }

    #[test]
    fn only_one_at_a_time() {
        let mut r = PipRegistry::new();
        r.enter(1, 320, 240);
        assert!(!r.enter(2, 320, 240));
    }

    #[test]
    fn disable_prevents_enter() {
        let mut r = PipRegistry::new();
        r.disable(1);
        assert!(!r.enter(1, 320, 240));
    }

    #[test]
    fn disable_active_exits() {
        let mut r = PipRegistry::new();
        r.enter(1, 320, 240);
        r.disable(1);
        assert!(!r.is_active_for(1));
    }
}
