//! Web Animations API (WAAPI) - element.animate(keyframes, options).
//!
//! Spec: https://www.w3.org/TR/web-animations-1/
//! Document.getAnimations(), Animation.play/pause/cancel/finish, AnimationEffect.

use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayState {
    Idle,
    Running,
    Paused,
    Finished,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct KeyframeStop {
    pub offset: Option<f32>,        // 0..1 nebo None (auto)
    pub properties: HashMap<String, String>,
    pub easing: String,             // "linear", "ease", "cubic-bezier(...)"
}

#[derive(Debug, Clone)]
pub struct EffectTiming {
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,            // f64::INFINITY pro forever
    pub easing: String,
    pub direction: String,          // "normal", "reverse", "alternate", "alternate-reverse"
    pub fill: String,               // "none", "forwards", "backwards", "both", "auto"
    pub end_delay_ms: f64,
    pub iteration_start: f64,
}

impl Default for EffectTiming {
    fn default() -> Self {
        Self {
            duration_ms: 0.0,
            delay_ms: 0.0,
            iterations: 1.0,
            easing: "linear".into(),
            direction: "normal".into(),
            fill: "auto".into(),
            end_delay_ms: 0.0,
            iteration_start: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct Animation {
    pub id: u64,
    pub element_id: usize,
    pub keyframes: Vec<KeyframeStop>,
    pub timing: EffectTiming,
    pub state: PlayState,
    pub current_time_ms: f64,
    pub playback_rate: f64,
}

#[derive(Default)]
pub struct AnimationRegistry {
    pub animations: HashMap<u64, Animation>,
    pub next_id: u64,
}

impl AnimationRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn animate(&mut self, element_id: usize, keyframes: Vec<KeyframeStop>, timing: EffectTiming) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.animations.insert(id, Animation {
            id, element_id, keyframes, timing,
            state: PlayState::Running,
            current_time_ms: 0.0,
            playback_rate: 1.0,
        });
        id
    }

    pub fn play(&mut self, id: u64) {
        if let Some(a) = self.animations.get_mut(&id) {
            a.state = PlayState::Running;
        }
    }

    pub fn pause(&mut self, id: u64) {
        if let Some(a) = self.animations.get_mut(&id) {
            a.state = PlayState::Paused;
        }
    }

    pub fn cancel(&mut self, id: u64) {
        if let Some(a) = self.animations.get_mut(&id) {
            a.state = PlayState::Cancelled;
            a.current_time_ms = 0.0;
        }
    }

    pub fn finish(&mut self, id: u64) {
        if let Some(a) = self.animations.get_mut(&id) {
            a.state = PlayState::Finished;
            a.current_time_ms = a.timing.duration_ms * a.timing.iterations;
        }
    }

    pub fn get_animations_for(&self, element_id: usize) -> Vec<&Animation> {
        self.animations.values().filter(|a| a.element_id == element_id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animate_creates() {
        let mut r = AnimationRegistry::new();
        let kf = vec![
            KeyframeStop { offset: Some(0.0), properties: HashMap::new(), easing: "linear".into() },
            KeyframeStop { offset: Some(1.0), properties: HashMap::new(), easing: "linear".into() },
        ];
        let id = r.animate(1, kf, EffectTiming::default());
        assert!(r.animations.contains_key(&id));
    }

    #[test]
    fn state_transitions() {
        let mut r = AnimationRegistry::new();
        let id = r.animate(1, vec![], EffectTiming::default());
        assert_eq!(r.animations.get(&id).unwrap().state, PlayState::Running);
        r.pause(id);
        assert_eq!(r.animations.get(&id).unwrap().state, PlayState::Paused);
        r.play(id);
        assert_eq!(r.animations.get(&id).unwrap().state, PlayState::Running);
        r.cancel(id);
        assert_eq!(r.animations.get(&id).unwrap().state, PlayState::Cancelled);
    }

    #[test]
    fn finish_advances_time() {
        let mut r = AnimationRegistry::new();
        let id = r.animate(1, vec![], EffectTiming {
            duration_ms: 1000.0,
            iterations: 2.0,
            ..Default::default()
        });
        r.finish(id);
        assert_eq!(r.animations.get(&id).unwrap().current_time_ms, 2000.0);
    }

    #[test]
    fn get_for_element() {
        let mut r = AnimationRegistry::new();
        r.animate(1, vec![], EffectTiming::default());
        r.animate(1, vec![], EffectTiming::default());
        r.animate(2, vec![], EffectTiming::default());
        assert_eq!(r.get_animations_for(1).len(), 2);
        assert_eq!(r.get_animations_for(2).len(), 1);
    }
}
