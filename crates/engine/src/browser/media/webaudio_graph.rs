//! Web Audio API node graph topology + sample-accurate scheduling.
//!
//! Spec: https://www.w3.org/TR/webaudio/
//! AudioContext.create*(...) returns nodes; connect() builds the graph;
//! ScriptProcessor / AudioWorklet feed buffers; OfflineAudioContext renders to buffer.
//!
//! This module holds graph + scheduling state. The real audio backend (cpal/rodio)
//! consumes it from the audio thread.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    Source,                 // BufferSource / Oscillator / MediaElement / MediaStream
    Gain,
    Delay,
    BiquadFilter,
    Convolver,
    Analyser,
    Panner,
    StereoPanner,
    DynamicsCompressor,
    WaveShaper,
    IirFilter,
    ChannelSplitter,
    ChannelMerger,
    Worklet,
    Destination,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AutomationRamp {
    Set,                    // instant value
    LinearRamp,
    ExponentialRamp,
    SetTargetAtTime,
    SetValueCurve,
}

#[derive(Debug, Clone)]
pub struct AudioParamEvent {
    pub time_sec: f64,
    pub value: f32,
    pub ramp: AutomationRamp,
    pub time_constant: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct AudioNode {
    pub id: u64,
    pub kind: NodeKind,
    pub channel_count: u32,
    pub params: HashMap<String, AudioParam>,
    pub input_count: u32,
    pub output_count: u32,
}

#[derive(Debug, Clone)]
pub struct AudioParam {
    pub default_value: f32,
    pub min_value: f32,
    pub max_value: f32,
    pub current_value: f32,
    pub events: Vec<AudioParamEvent>,
}

impl AudioParam {
    pub fn new(default: f32, min: f32, max: f32) -> Self {
        Self { default_value: default, min_value: min, max_value: max, current_value: default, events: Vec::new() }
    }

    pub fn set_value_at_time(&mut self, value: f32, time_sec: f64) {
        self.events.push(AudioParamEvent { time_sec, value: value.clamp(self.min_value, self.max_value),
            ramp: AutomationRamp::Set, time_constant: None });
        self.events.sort_by(|a, b| a.time_sec.partial_cmp(&b.time_sec).unwrap());
    }

    pub fn linear_ramp(&mut self, value: f32, time_sec: f64) {
        self.events.push(AudioParamEvent { time_sec, value: value.clamp(self.min_value, self.max_value),
            ramp: AutomationRamp::LinearRamp, time_constant: None });
        self.events.sort_by(|a, b| a.time_sec.partial_cmp(&b.time_sec).unwrap());
    }

    /// Sample param at given time per automation events.
    pub fn value_at(&self, t: f64) -> f32 {
        if self.events.is_empty() { return self.current_value; }
        let mut last_value = self.current_value;
        let mut last_time = -f64::INFINITY;
        for e in &self.events {
            if t < e.time_sec {
                if e.ramp == AutomationRamp::LinearRamp && last_time.is_finite() {
                    let p = ((t - last_time) / (e.time_sec - last_time)) as f32;
                    return last_value + (e.value - last_value) * p.clamp(0.0, 1.0);
                }
                return last_value;
            }
            last_value = e.value;
            last_time = e.time_sec;
        }
        last_value
    }
}

#[derive(Default)]
pub struct AudioGraph {
    pub nodes: HashMap<u64, AudioNode>,
    pub edges: Vec<(u64, u32, u64, u32)>,    // (src, src_out, dst, dst_in)
    pub destination_id: u64,
    pub next_id: u64,
    pub current_time_sec: f64,
    pub sample_rate: u32,
}

impl AudioGraph {
    pub fn new(sample_rate: u32) -> Self {
        let mut g = Self { sample_rate, ..Self::default() };
        // Destination always exists.
        let dest = g.create_node(NodeKind::Destination);
        g.destination_id = dest;
        g
    }

    pub fn create_node(&mut self, kind: NodeKind) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let mut node = AudioNode {
            id, kind, channel_count: 2,
            input_count: if matches!(kind, NodeKind::Source) { 0 } else { 1 },
            output_count: if matches!(kind, NodeKind::Destination) { 0 } else { 1 },
            params: HashMap::new(),
        };
        match kind {
            NodeKind::Gain => { node.params.insert("gain".into(), AudioParam::new(1.0, -f32::INFINITY, f32::INFINITY)); }
            NodeKind::Delay => { node.params.insert("delayTime".into(), AudioParam::new(0.0, 0.0, 1.0)); }
            NodeKind::BiquadFilter => {
                node.params.insert("frequency".into(), AudioParam::new(350.0, 10.0, 22050.0));
                node.params.insert("Q".into(), AudioParam::new(1.0, 0.0001, 1000.0));
                node.params.insert("gain".into(), AudioParam::new(0.0, -40.0, 40.0));
            }
            _ => {}
        }
        self.nodes.insert(id, node);
        id
    }

    pub fn connect(&mut self, src: u64, src_out: u32, dst: u64, dst_in: u32) -> Result<(), String> {
        if !self.nodes.contains_key(&src) || !self.nodes.contains_key(&dst) {
            return Err("node missing".into());
        }
        self.edges.push((src, src_out, dst, dst_in));
        Ok(())
    }

    pub fn disconnect(&mut self, src: u64) {
        self.edges.retain(|e| e.0 != src);
    }

    /// Returns true if there's a path src -> destination.
    pub fn reaches_destination(&self, src: u64) -> bool {
        let mut stack = vec![src];
        let mut visited = std::collections::HashSet::new();
        while let Some(id) = stack.pop() {
            if id == self.destination_id { return true; }
            if !visited.insert(id) { continue; }
            for e in &self.edges {
                if e.0 == id { stack.push(e.2); }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_has_destination() {
        let g = AudioGraph::new(48000);
        assert!(g.nodes.contains_key(&g.destination_id));
    }

    #[test]
    fn create_gain_has_param() {
        let mut g = AudioGraph::new(48000);
        let id = g.create_node(NodeKind::Gain);
        assert!(g.nodes[&id].params.contains_key("gain"));
    }

    #[test]
    fn connect_creates_edge() {
        let mut g = AudioGraph::new(48000);
        let src = g.create_node(NodeKind::Source);
        let gain = g.create_node(NodeKind::Gain);
        g.connect(src, 0, gain, 0).unwrap();
        g.connect(gain, 0, g.destination_id, 0).unwrap();
        assert!(g.reaches_destination(src));
    }

    #[test]
    fn disconnect_breaks_path() {
        let mut g = AudioGraph::new(48000);
        let src = g.create_node(NodeKind::Source);
        g.connect(src, 0, g.destination_id, 0).unwrap();
        g.disconnect(src);
        assert!(!g.reaches_destination(src));
    }

    #[test]
    fn linear_ramp_interpolates() {
        let mut p = AudioParam::new(0.0, 0.0, 1.0);
        p.set_value_at_time(0.0, 0.0);
        p.linear_ramp(1.0, 1.0);
        let v = p.value_at(0.5);
        assert!((v - 0.5).abs() < 0.001);
    }

    #[test]
    fn set_value_clamps_to_range() {
        let mut p = AudioParam::new(0.0, -1.0, 1.0);
        p.set_value_at_time(5.0, 0.0);
        assert!((p.events[0].value - 1.0).abs() < 0.001);
    }

    #[test]
    fn missing_node_connect_errors() {
        let mut g = AudioGraph::new(48000);
        assert!(g.connect(999, 0, g.destination_id, 0).is_err());
    }
}
