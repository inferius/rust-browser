//! WebCodecs API foundation - low-level video/audio encode/decode.
//!
//! Spec: https://w3c.github.io/webcodecs/
//! VideoEncoder, VideoDecoder, AudioEncoder, AudioDecoder + VideoFrame,
//! AudioData. Pre-existing buffer manipulation pre encoded -> decoded.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodecState {
    Unconfigured,
    Configured,
    Closed,
}

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: VideoPixelFormat,
    pub timestamp_us: i64,
    pub duration_us: Option<i64>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoPixelFormat {
    I420,
    I422,
    I444,
    Nv12,
    Rgba,
    Rgbx,
    Bgra,
    Bgrx,
}

#[derive(Debug, Clone)]
pub struct EncodedVideoChunk {
    pub data: Vec<u8>,
    pub timestamp_us: i64,
    pub kind: ChunkKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChunkKind {
    Key,
    Delta,
}

pub struct VideoEncoder {
    pub state: CodecState,
    pub codec: String,        // "vp8", "vp9", "avc1.42E01F", "av01.0.04M.08"
    pub queue_size: u32,
    pub output_queue: Vec<EncodedVideoChunk>,
}

impl Default for VideoEncoder {
    fn default() -> Self {
        Self {
            state: CodecState::Unconfigured,
            codec: String::new(),
            queue_size: 0,
            output_queue: Vec::new(),
        }
    }
}

impl VideoEncoder {
    pub fn new() -> Self { Self::default() }

    pub fn configure(&mut self, codec: &str) {
        self.codec = codec.into();
        self.state = CodecState::Configured;
    }

    /// Encode VideoFrame (foundation: passthrough as Key chunk).
    pub fn encode(&mut self, frame: VideoFrame) -> bool {
        if self.state != CodecState::Configured { return false; }
        self.output_queue.push(EncodedVideoChunk {
            data: frame.data.clone(),
            timestamp_us: frame.timestamp_us,
            kind: ChunkKind::Key,
        });
        self.queue_size += 1;
        true
    }

    pub fn close(&mut self) {
        self.state = CodecState::Closed;
        self.output_queue.clear();
    }
}

pub struct VideoDecoder {
    pub state: CodecState,
    pub codec: String,
    pub output_queue: Vec<VideoFrame>,
}

impl Default for VideoDecoder {
    fn default() -> Self {
        Self {
            state: CodecState::Unconfigured,
            codec: String::new(),
            output_queue: Vec::new(),
        }
    }
}

impl VideoDecoder {
    pub fn new() -> Self { Self::default() }

    pub fn configure(&mut self, codec: &str) {
        self.codec = codec.into();
        self.state = CodecState::Configured;
    }

    pub fn decode(&mut self, chunk: EncodedVideoChunk) -> bool {
        if self.state != CodecState::Configured { return false; }
        // Foundation passthrough.
        self.output_queue.push(VideoFrame {
            width: 1280, height: 720,
            format: VideoPixelFormat::I420,
            timestamp_us: chunk.timestamp_us,
            duration_us: None,
            data: chunk.data,
        });
        true
    }

    pub fn close(&mut self) {
        self.state = CodecState::Closed;
        self.output_queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_configure_encode() {
        let mut e = VideoEncoder::new();
        e.configure("vp8");
        assert_eq!(e.state, CodecState::Configured);
        let frame = VideoFrame {
            width: 640, height: 480,
            format: VideoPixelFormat::I420,
            timestamp_us: 0,
            duration_us: None,
            data: vec![0u8; 100],
        };
        assert!(e.encode(frame));
        assert_eq!(e.output_queue.len(), 1);
    }

    #[test]
    fn encoder_unconfigured_rejects() {
        let mut e = VideoEncoder::new();
        let frame = VideoFrame {
            width: 1, height: 1, format: VideoPixelFormat::Rgba,
            timestamp_us: 0, duration_us: None, data: vec![0u8; 4],
        };
        assert!(!e.encode(frame));
    }

    #[test]
    fn decoder_roundtrip_through_encode() {
        let mut enc = VideoEncoder::new();
        enc.configure("vp9");
        let frame_data = vec![1, 2, 3, 4];
        enc.encode(VideoFrame {
            width: 1, height: 1, format: VideoPixelFormat::Rgba,
            timestamp_us: 100, duration_us: None,
            data: frame_data.clone(),
        });
        let chunk = enc.output_queue.remove(0);
        let mut dec = VideoDecoder::new();
        dec.configure("vp9");
        dec.decode(chunk);
        assert_eq!(dec.output_queue[0].data, frame_data);
    }
}
