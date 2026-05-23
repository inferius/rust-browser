//! Web Audio API foundation - AudioContext + AudioNode graph.
//!
//! Spec: https://www.w3.org/TR/webaudio/
//!
//! Foundation: node graph structures + connection model. Real audio = cpal /
//! rodio integration = next session.

use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioContextState {
    Suspended,
    Running,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioNodeKind {
    Oscillator,
    Gain,
    Delay,
    BiquadFilter,
    Convolver,
    AnalyserNode,
    Destination,
    BufferSource,
    Panner,
    StereoPanner,
}

#[derive(Debug)]
pub struct AudioNode {
    pub id: u32,
    pub kind: AudioNodeKind,
    pub channel_count: u32,
    /// Connections: dst node_id -> input channel.
    pub outputs: Vec<u32>,
    pub params: std::collections::HashMap<String, f32>,
}

impl AudioNode {
    pub fn new(id: u32, kind: AudioNodeKind) -> Self {
        Self {
            id, kind,
            channel_count: 2,
            outputs: Vec::new(),
            params: std::collections::HashMap::new(),
        }
    }
}

pub struct AudioContext {
    pub sample_rate: f32,
    pub state: AudioContextState,
    pub nodes: Vec<Rc<RefCell<AudioNode>>>,
    pub destination_id: u32,
    pub next_id: u32,
}

impl AudioContext {
    pub fn new(sample_rate: f32) -> Self {
        let mut ctx = Self {
            sample_rate,
            state: AudioContextState::Suspended,
            nodes: Vec::new(),
            destination_id: 0,
            next_id: 1,
        };
        let dest = ctx.create_node(AudioNodeKind::Destination);
        ctx.destination_id = dest.borrow().id;
        ctx
    }

    pub fn create_node(&mut self, kind: AudioNodeKind) -> Rc<RefCell<AudioNode>> {
        let id = self.next_id;
        self.next_id += 1;
        let node = Rc::new(RefCell::new(AudioNode::new(id, kind)));
        self.nodes.push(Rc::clone(&node));
        node
    }

    pub fn connect(&mut self, src_id: u32, dst_id: u32) -> bool {
        if let Some(src) = self.nodes.iter().find(|n| n.borrow().id == src_id) {
            src.borrow_mut().outputs.push(dst_id);
            return true;
        }
        false
    }

    pub fn disconnect(&mut self, src_id: u32) -> bool {
        if let Some(src) = self.nodes.iter().find(|n| n.borrow().id == src_id) {
            src.borrow_mut().outputs.clear();
            return true;
        }
        false
    }

    pub fn resume(&mut self) { self.state = AudioContextState::Running; }
    pub fn suspend(&mut self) { self.state = AudioContextState::Suspended; }
    pub fn close(&mut self) { self.state = AudioContextState::Closed; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_has_destination() {
        let ctx = AudioContext::new(44100.0);
        assert!(ctx.nodes.iter().any(|n| n.borrow().kind == AudioNodeKind::Destination));
    }

    #[test]
    fn create_oscillator_and_connect() {
        let mut ctx = AudioContext::new(48000.0);
        let osc = ctx.create_node(AudioNodeKind::Oscillator);
        let gain = ctx.create_node(AudioNodeKind::Gain);
        let osc_id = osc.borrow().id;
        let gain_id = gain.borrow().id;
        ctx.connect(osc_id, gain_id);
        ctx.connect(gain_id, ctx.destination_id);
        assert_eq!(osc.borrow().outputs, vec![gain_id]);
    }

    #[test]
    fn state_transitions() {
        let mut ctx = AudioContext::new(44100.0);
        assert_eq!(ctx.state, AudioContextState::Suspended);
        ctx.resume();
        assert_eq!(ctx.state, AudioContextState::Running);
        ctx.suspend();
        assert_eq!(ctx.state, AudioContextState::Suspended);
        ctx.close();
        assert_eq!(ctx.state, AudioContextState::Closed);
    }

    #[test]
    fn disconnect_clears_outputs() {
        let mut ctx = AudioContext::new(44100.0);
        let osc = ctx.create_node(AudioNodeKind::Oscillator);
        let osc_id = osc.borrow().id;
        ctx.connect(osc_id, ctx.destination_id);
        ctx.disconnect(osc_id);
        assert!(osc.borrow().outputs.is_empty());
    }
}
