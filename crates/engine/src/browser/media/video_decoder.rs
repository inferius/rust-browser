//! Video dekoder - MP4/WebM demux + AV1/H.265 frame decode.
//!
//! Pure-Rust pipeline (zero system deps):
//! - MP4 demux pres `re_mp4` (pure-Rust)
//! - WebM demux pres `webm-iterable` (pure-Rust EBML)
//! - AV1 frames pres `rav1d-safe` (via zenavif pre-still images)
//! - HEVC frames pres `heic` crate (SIMD decoder)
//! - H.264: no pure-Rust decoder zatim - bude tombstone
//!
//! Real video playback (continuous frames + audio sync) je seberare TODO,
//! tato vrstva poskytuje per-sample demux + still-frame decoding.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoCodec {
    Av01,
    Avc1,        // H.264 / AVC
    Hev1,        // H.265 / HEVC
    Hvc1,        // H.265 / HEVC
    Vp08,
    Vp09,
    Unknown,
}

impl VideoCodec {
    pub fn from_str(s: &str) -> Self {
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("av01") { Self::Av01 }
        else if lower.starts_with("avc1") { Self::Avc1 }
        else if lower.starts_with("hev1") { Self::Hev1 }
        else if lower.starts_with("hvc1") { Self::Hvc1 }
        else if lower.starts_with("vp08") || lower.starts_with("vp8") { Self::Vp08 }
        else if lower.starts_with("vp09") || lower.starts_with("vp9") { Self::Vp09 }
        else { Self::Unknown }
    }
}

#[derive(Debug, Clone)]
pub struct VideoTrack {
    pub track_id: u32,
    pub width: u16,
    pub height: u16,
    pub timescale: u64,
    pub duration_ticks: u64,
    pub codec: VideoCodec,
    pub codec_string: Option<String>,
    pub sample_count: usize,
}

#[derive(Debug, Clone)]
pub struct VideoSample {
    pub id: u32,
    pub is_keyframe: bool,
    pub data_offset: usize,
    pub data_size: usize,
    pub composition_ms: f64,
    pub duration_ms: f64,
}

#[derive(Debug, Clone)]
pub struct DemuxError(pub String);

/// Demux MP4 file - vraci video tracks + sample tables (offsets within input bytes).
pub fn demux_mp4(data: &[u8]) -> Result<Vec<(VideoTrack, Vec<VideoSample>)>, DemuxError> {
    let mp4 = re_mp4::Mp4::read_bytes(data)
        .map_err(|e| DemuxError(format!("mp4 read failed: {:?}", e)))?;
    let mut out = Vec::new();
    for (_track_id, track) in mp4.tracks() {
        let kind = track.kind;
        if !matches!(kind, Some(re_mp4::TrackKind::Video)) { continue; }
        let codec_string = track.codec_string(&mp4);
        let codec = codec_string.as_deref().map(VideoCodec::from_str).unwrap_or(VideoCodec::Unknown);
        let video_track = VideoTrack {
            track_id: track.track_id,
            width: track.width,
            height: track.height,
            timescale: track.timescale,
            duration_ticks: track.duration,
            codec,
            codec_string,
            sample_count: track.samples.len(),
        };
        let mut samples = Vec::with_capacity(track.samples.len());
        for s in &track.samples {
            let ts = s.timescale.max(1) as f64;
            samples.push(VideoSample {
                id: s.id,
                is_keyframe: s.is_sync,
                data_offset: s.offset as usize,
                data_size: s.size as usize,
                composition_ms: (s.composition_timestamp as f64 / ts) * 1000.0,
                duration_ms: (s.duration as f64 / ts) * 1000.0,
            });
        }
        out.push((video_track, samples));
    }
    Ok(out)
}

/// MP4 byte sniff: 'ftyp' brand at offset 4.
pub fn is_mp4(data: &[u8]) -> bool {
    data.len() >= 12 && &data[4..8] == b"ftyp"
}

/// WebM byte sniff: EBML magic 0x1A 0x45 0xDF 0xA3.
pub fn is_webm(data: &[u8]) -> bool {
    data.len() >= 4 && data[..4] == [0x1A, 0x45, 0xDF, 0xA3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_string_parsing() {
        assert_eq!(VideoCodec::from_str("av01.0.04M.08"), VideoCodec::Av01);
        assert_eq!(VideoCodec::from_str("avc1.640028"), VideoCodec::Avc1);
        assert_eq!(VideoCodec::from_str("hev1.1.6.L93.B0"), VideoCodec::Hev1);
        assert_eq!(VideoCodec::from_str("vp09.00.10.08"), VideoCodec::Vp09);
        assert_eq!(VideoCodec::from_str("unknown"), VideoCodec::Unknown);
    }

    #[test]
    fn is_mp4_detects_ftyp() {
        let mut buf = vec![0u8; 12];
        buf[4..8].copy_from_slice(b"ftyp");
        assert!(is_mp4(&buf));
    }

    #[test]
    fn is_webm_detects_ebml() {
        let buf = [0x1A, 0x45, 0xDF, 0xA3, 0, 0];
        assert!(is_webm(&buf));
    }

    #[test]
    fn demux_mp4_invalid_errors() {
        let garbage = [0u8; 100];
        assert!(demux_mp4(&garbage).is_err());
    }

    #[test]
    fn is_mp4_rejects_non_ftyp() {
        assert!(!is_mp4(&[0u8; 12]));
    }
}
