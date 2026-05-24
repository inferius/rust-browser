//! Container format detection - MP4, WebM, OGG, WAV, etc.
//!
//! Used pri <video>/<audio>/MSE source buffer setup. Plays nicely with the
//! image_decoder approach: magic byte sniff first, full demux later.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaContainer {
    Mp4,
    WebM,        // Matroska EBML
    Ogg,
    Wave,
    Flac,
    Mp3,
    Aac,         // ADTS frames
    Caf,         // Core Audio Format (Apple)
    Mpeg2Ts,
    Unknown,
}

impl MediaContainer {
    pub fn mime(&self) -> &'static str {
        match self {
            Self::Mp4 => "video/mp4",
            Self::WebM => "video/webm",
            Self::Ogg => "video/ogg",
            Self::Wave => "audio/wav",
            Self::Flac => "audio/flac",
            Self::Mp3 => "audio/mpeg",
            Self::Aac => "audio/aac",
            Self::Caf => "audio/x-caf",
            Self::Mpeg2Ts => "video/mp2t",
            Self::Unknown => "application/octet-stream",
        }
    }
}

pub fn detect(buf: &[u8]) -> MediaContainer {
    if buf.len() >= 12 && buf[4..8] == *b"ftyp" {
        return MediaContainer::Mp4;
    }
    if buf.len() >= 4 && buf[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return MediaContainer::WebM;
    }
    if buf.len() >= 4 && buf[..4] == *b"OggS" {
        return MediaContainer::Ogg;
    }
    if buf.len() >= 12 && buf[..4] == *b"RIFF" && buf[8..12] == *b"WAVE" {
        return MediaContainer::Wave;
    }
    if buf.len() >= 4 && buf[..4] == *b"fLaC" {
        return MediaContainer::Flac;
    }
    if buf.len() >= 4 && buf[..4] == *b"caff" {
        return MediaContainer::Caf;
    }
    if buf.len() >= 2 {
        // MP3: ID3v2 prefix "ID3" or frame sync 0xFF Ex
        if buf[..3] == *b"ID3" || (buf[0] == 0xFF && (buf[1] & 0xE0) == 0xE0) {
            // Distinguish ADTS AAC (0xFFF) vs MP3 (0xFFFB/FFF3/FFFA)
            if buf[1] & 0xF6 == 0xF0 {
                return MediaContainer::Aac;
            }
            return MediaContainer::Mp3;
        }
    }
    if buf.len() >= 192 && buf[0] == 0x47 && buf[188] == 0x47 {
        return MediaContainer::Mpeg2Ts;
    }
    MediaContainer::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mp4() {
        let mut buf = vec![0u8; 32];
        buf[4..8].copy_from_slice(b"ftyp");
        buf[8..12].copy_from_slice(b"mp42");
        assert_eq!(detect(&buf), MediaContainer::Mp4);
    }

    #[test]
    fn detect_webm() {
        let buf = [0x1A, 0x45, 0xDF, 0xA3, 0, 0, 0, 0];
        assert_eq!(detect(&buf), MediaContainer::WebM);
    }

    #[test]
    fn detect_wave() {
        let mut buf = b"RIFF\x00\x00\x00\x00WAVE".to_vec();
        buf.resize(32, 0);
        assert_eq!(detect(&buf), MediaContainer::Wave);
    }

    #[test]
    fn detect_ogg() {
        let buf = b"OggS....";
        assert_eq!(detect(buf), MediaContainer::Ogg);
    }

    #[test]
    fn detect_flac() {
        let buf = b"fLaC\x00\x00\x00\x22";
        assert_eq!(detect(buf), MediaContainer::Flac);
    }

    #[test]
    fn detect_mp3_id3() {
        let buf = b"ID3\x04";
        assert_eq!(detect(buf), MediaContainer::Mp3);
    }

    #[test]
    fn detect_mp3_frame_sync() {
        let buf = [0xFF, 0xFB, 0x90, 0x00];
        assert_eq!(detect(&buf), MediaContainer::Mp3);
    }

    #[test]
    fn detect_aac_adts() {
        let buf = [0xFF, 0xF1, 0x4C, 0x80];
        assert_eq!(detect(&buf), MediaContainer::Aac);
    }

    #[test]
    fn detect_unknown_buffer() {
        let buf = b"garbage";
        assert_eq!(detect(buf), MediaContainer::Unknown);
    }
}
