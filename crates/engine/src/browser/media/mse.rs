//! Media Source Extensions (MSE) - JS-pushed video streaming.
//!
//! Spec: https://www.w3.org/TR/media-source/
//! const mse = new MediaSource(); video.src = URL.createObjectURL(mse);
//! const sb = mse.addSourceBuffer("video/mp4; codecs=avc1.640028");
//! sb.appendBuffer(arrayBuffer);

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadyState {
    Closed,
    Open,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppendState {
    WaitingForSegment,
    ParsingInitSegment,
    ParsingMediaSegment,
    Idle,
}

#[derive(Debug, Clone)]
pub struct SourceBuffer {
    pub id: u64,
    pub mime: String,
    pub append_state: AppendState,
    pub buffered_ranges: Vec<(f64, f64)>,   // (start_sec, end_sec) per buffered range
    pub timestamp_offset: f64,
    pub append_window_start: f64,
    pub append_window_end: f64,
    pub updating: bool,
    pub mode: AppendMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppendMode {
    Segments,
    Sequence,
}

#[derive(Debug, Clone)]
pub struct MediaSource {
    pub ready_state: ReadyState,
    pub duration_sec: f64,
    pub source_buffers: HashMap<u64, SourceBuffer>,
    pub next_buffer_id: u64,
    pub active_buffer_ids: Vec<u64>,
}

impl MediaSource {
    pub fn new() -> Self {
        Self {
            ready_state: ReadyState::Closed,
            duration_sec: f64::NAN,
            source_buffers: HashMap::new(),
            next_buffer_id: 0,
            active_buffer_ids: Vec::new(),
        }
    }

    pub fn open(&mut self) {
        self.ready_state = ReadyState::Open;
    }

    pub fn end_of_stream(&mut self) {
        self.ready_state = ReadyState::Ended;
    }

    pub fn close(&mut self) {
        self.ready_state = ReadyState::Closed;
    }

    pub fn add_source_buffer(&mut self, mime: &str) -> Result<u64, String> {
        if self.ready_state != ReadyState::Open {
            return Err("MediaSource must be open".into());
        }
        if !is_supported_mime(mime) {
            return Err(format!("unsupported MIME '{}'", mime));
        }
        self.next_buffer_id += 1;
        let id = self.next_buffer_id;
        self.source_buffers.insert(id, SourceBuffer {
            id, mime: mime.into(),
            append_state: AppendState::WaitingForSegment,
            buffered_ranges: Vec::new(),
            timestamp_offset: 0.0,
            append_window_start: 0.0,
            append_window_end: f64::INFINITY,
            updating: false,
            mode: AppendMode::Segments,
        });
        self.active_buffer_ids.push(id);
        Ok(id)
    }

    pub fn remove_source_buffer(&mut self, id: u64) {
        self.source_buffers.remove(&id);
        self.active_buffer_ids.retain(|i| *i != id);
    }

    pub fn append_buffer(&mut self, id: u64, _data: &[u8]) -> Result<(), String> {
        let sb = self.source_buffers.get_mut(&id).ok_or("buffer missing")?;
        if sb.updating { return Err("buffer is updating".into()); }
        sb.updating = true;
        sb.append_state = AppendState::ParsingMediaSegment;
        Ok(())
    }

    /// Simulate parser progress: announce a new buffered range.
    pub fn add_buffered_range(&mut self, id: u64, start_sec: f64, end_sec: f64) {
        if let Some(sb) = self.source_buffers.get_mut(&id) {
            sb.buffered_ranges.push((start_sec, end_sec));
            sb.buffered_ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            sb.updating = false;
            sb.append_state = AppendState::Idle;
        }
    }
}

impl Default for MediaSource {
    fn default() -> Self { Self::new() }
}

pub fn is_supported_mime(mime: &str) -> bool {
    let lower = mime.to_ascii_lowercase();
    lower.starts_with("video/mp4")
    || lower.starts_with("video/webm")
    || lower.starts_with("audio/mp4")
    || lower.starts_with("audio/webm")
    || lower.starts_with("audio/aac")
    || lower.starts_with("audio/ogg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_buffer_open_succeeds() {
        let mut ms = MediaSource::new();
        ms.open();
        assert!(ms.add_source_buffer("video/mp4").is_ok());
    }

    #[test]
    fn add_buffer_closed_fails() {
        let mut ms = MediaSource::new();
        assert!(ms.add_source_buffer("video/mp4").is_err());
    }

    #[test]
    fn add_buffer_bad_mime_fails() {
        let mut ms = MediaSource::new();
        ms.open();
        assert!(ms.add_source_buffer("application/octet-stream").is_err());
    }

    #[test]
    fn append_marks_updating() {
        let mut ms = MediaSource::new();
        ms.open();
        let id = ms.add_source_buffer("video/mp4").unwrap();
        ms.append_buffer(id, b"data").unwrap();
        assert!(ms.source_buffers.get(&id).unwrap().updating);
    }

    #[test]
    fn buffered_range_recorded() {
        let mut ms = MediaSource::new();
        ms.open();
        let id = ms.add_source_buffer("video/mp4").unwrap();
        ms.append_buffer(id, b"d").unwrap();
        ms.add_buffered_range(id, 0.0, 5.0);
        let sb = ms.source_buffers.get(&id).unwrap();
        assert_eq!(sb.buffered_ranges, vec![(0.0, 5.0)]);
        assert!(!sb.updating);
    }

    #[test]
    fn end_of_stream_sets_state() {
        let mut ms = MediaSource::new();
        ms.open();
        ms.end_of_stream();
        assert_eq!(ms.ready_state, ReadyState::Ended);
    }
}
