//! Media subsystem - container demux, codec selection, frame timing.
//!
//! Real implementation pipes to symphonia (audio) + dav1d/x264 (video).
//! Foundation modules expose API surface without the heavy backends.

pub mod mse;
pub mod eme;
pub mod container_sniff;
pub mod webaudio_graph;
pub mod vtt_parser;
pub mod h264_parse;
pub mod av1_parse;
pub mod srt_parser;
pub mod video_decoder;
