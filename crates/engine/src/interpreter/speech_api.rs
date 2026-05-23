//! Web Speech API - SpeechSynthesis (TTS) + SpeechRecognition (STT).
//!
//! Spec: https://w3c.github.io/speech-api/
//! Foundation: queue + state. Real audio = OS speech engines (SAPI Windows,
//! NSSpeechSynthesizer mac, eSpeak Linux).

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Utterance {
    pub text: String,
    pub lang: String,
    pub voice: Option<String>,
    pub volume: f32,  // 0..1
    pub rate: f32,    // 0.1..10
    pub pitch: f32,   // 0..2
}

impl Utterance {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.into(),
            lang: "en-US".into(),
            voice: None,
            volume: 1.0,
            rate: 1.0,
            pitch: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpeechVoice {
    pub name: String,
    pub lang: String,
    pub default: bool,
    pub local_service: bool,
}

#[derive(Default)]
pub struct SpeechSynthesis {
    pub queue: VecDeque<Utterance>,
    pub speaking: bool,
    pub paused: bool,
    pub voices: Vec<SpeechVoice>,
}

impl SpeechSynthesis {
    pub fn new() -> Self { Self::default() }

    pub fn speak(&mut self, u: Utterance) {
        self.queue.push_back(u);
        self.speaking = true;
    }

    pub fn cancel(&mut self) {
        self.queue.clear();
        self.speaking = false;
        self.paused = false;
    }

    pub fn pause(&mut self) { self.paused = true; }
    pub fn resume(&mut self) { self.paused = false; }

    pub fn get_voices(&self) -> &[SpeechVoice] {
        &self.voices
    }

    /// Drain - vraci next utterance ready k speak.
    pub fn next(&mut self) -> Option<Utterance> {
        if self.paused { return None; }
        let u = self.queue.pop_front();
        if self.queue.is_empty() { self.speaking = false; }
        u
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecognitionState {
    Inactive,
    Listening,
    Processing,
}

pub struct SpeechRecognition {
    pub lang: String,
    pub continuous: bool,
    pub interim_results: bool,
    pub max_alternatives: u32,
    pub state: RecognitionState,
    pub results: Vec<RecognitionResult>,
}

#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub transcript: String,
    pub confidence: f32,
    pub is_final: bool,
}

impl Default for SpeechRecognition {
    fn default() -> Self {
        Self {
            lang: "en-US".into(),
            continuous: false,
            interim_results: false,
            max_alternatives: 1,
            state: RecognitionState::Inactive,
            results: Vec::new(),
        }
    }
}

impl SpeechRecognition {
    pub fn new() -> Self { Self::default() }
    pub fn start(&mut self) { self.state = RecognitionState::Listening; }
    pub fn stop(&mut self) { self.state = RecognitionState::Inactive; }
    pub fn abort(&mut self) {
        self.state = RecognitionState::Inactive;
        self.results.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_speak_queues() {
        let mut s = SpeechSynthesis::new();
        s.speak(Utterance::new("hello"));
        assert!(s.speaking);
        assert_eq!(s.queue.len(), 1);
    }

    #[test]
    fn synthesis_cancel_clears() {
        let mut s = SpeechSynthesis::new();
        s.speak(Utterance::new("a"));
        s.speak(Utterance::new("b"));
        s.cancel();
        assert!(!s.speaking);
        assert_eq!(s.queue.len(), 0);
    }

    #[test]
    fn pause_blocks_next() {
        let mut s = SpeechSynthesis::new();
        s.speak(Utterance::new("hi"));
        s.pause();
        assert!(s.next().is_none());
        s.resume();
        assert!(s.next().is_some());
    }

    #[test]
    fn recognition_lifecycle() {
        let mut r = SpeechRecognition::new();
        assert_eq!(r.state, RecognitionState::Inactive);
        r.start();
        assert_eq!(r.state, RecognitionState::Listening);
        r.stop();
        assert_eq!(r.state, RecognitionState::Inactive);
    }
}
