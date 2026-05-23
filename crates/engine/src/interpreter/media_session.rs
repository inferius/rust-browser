//! Media Session API - lock screen + system media keys integration.
//!
//! Spec: https://w3c.github.io/mediasession/
//! navigator.mediaSession.metadata = new MediaMetadata({...})
//! + setActionHandler('play'|'pause'|'previoustrack'|...).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaAction {
    Play,
    Pause,
    Stop,
    SeekBackward,
    SeekForward,
    SeekTo,
    PreviousTrack,
    NextTrack,
    SkipAd,
    EnterPictureInPicture,
    TogglePictureInPicture,
}

impl MediaAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "stop" => Some(Self::Stop),
            "seekbackward" => Some(Self::SeekBackward),
            "seekforward" => Some(Self::SeekForward),
            "seekto" => Some(Self::SeekTo),
            "previoustrack" => Some(Self::PreviousTrack),
            "nexttrack" => Some(Self::NextTrack),
            "skipad" => Some(Self::SkipAd),
            "enterpictureinpicture" => Some(Self::EnterPictureInPicture),
            "togglepictureinpicture" => Some(Self::TogglePictureInPicture),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MediaMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: Vec<MediaArtwork>,
}

#[derive(Debug, Clone)]
pub struct MediaArtwork {
    pub src: String,
    pub sizes: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    None,
    Paused,
    Playing,
}

pub struct MediaSession {
    pub metadata: MediaMetadata,
    pub playback_state: PlaybackState,
    /// action -> handler callback id (opaque).
    pub handlers: HashMap<MediaAction, usize>,
    pub position_seconds: f64,
    pub duration_seconds: f64,
    pub playback_rate: f32,
}

impl Default for MediaSession {
    fn default() -> Self {
        Self {
            metadata: MediaMetadata::default(),
            playback_state: PlaybackState::None,
            handlers: HashMap::new(),
            position_seconds: 0.0,
            duration_seconds: 0.0,
            playback_rate: 1.0,
        }
    }
}

impl MediaSession {
    pub fn new() -> Self { Self::default() }

    pub fn set_action_handler(&mut self, action: MediaAction, handler_id: Option<usize>) {
        match handler_id {
            Some(id) => { self.handlers.insert(action, id); }
            None => { self.handlers.remove(&action); }
        }
    }

    pub fn set_position_state(&mut self, position: f64, duration: f64, rate: f32) {
        self.position_seconds = position.clamp(0.0, duration);
        self.duration_seconds = duration.max(0.0);
        self.playback_rate = rate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_handler_and_query() {
        let mut s = MediaSession::new();
        s.set_action_handler(MediaAction::Play, Some(42));
        assert_eq!(s.handlers.get(&MediaAction::Play), Some(&42));
    }

    #[test]
    fn clear_handler() {
        let mut s = MediaSession::new();
        s.set_action_handler(MediaAction::Pause, Some(1));
        s.set_action_handler(MediaAction::Pause, None);
        assert!(s.handlers.get(&MediaAction::Pause).is_none());
    }

    #[test]
    fn position_clamped_to_duration() {
        let mut s = MediaSession::new();
        s.set_position_state(150.0, 100.0, 1.0);
        assert_eq!(s.position_seconds, 100.0);
    }

    #[test]
    fn parse_action_names() {
        assert_eq!(MediaAction::parse("play"), Some(MediaAction::Play));
        assert_eq!(MediaAction::parse("seekTo"), Some(MediaAction::SeekTo));
        assert_eq!(MediaAction::parse("invalid"), None);
    }
}
