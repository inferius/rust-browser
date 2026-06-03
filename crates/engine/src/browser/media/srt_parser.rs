//! SubRip (.srt) cue file parser.
//!
//! De-facto format predating WebVTT; many videos still ship SRT subtitles.

#[derive(Debug, Clone)]
pub struct SrtCue {
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub fn parse(input: &str) -> Result<Vec<SrtCue>, String> {
    let mut out = Vec::new();
    let mut lines = input.lines().peekable();
    while lines.peek().is_some() {
        // Skip blanks
        while lines.peek().map(|l| l.trim().is_empty()).unwrap_or(false) {
            lines.next();
        }
        let Some(idx_line) = lines.next() else { break; };
        let index: u32 = idx_line.trim().parse().map_err(|_| format!("bad index '{}'", idx_line))?;
        let Some(timing) = lines.next() else { return Err("missing timing".into()); };
        let (start_ms, end_ms) = parse_timing(timing)?;
        let mut text = String::new();
        for l in &mut lines {
            if l.trim().is_empty() { break; }
            if !text.is_empty() { text.push('\n'); }
            text.push_str(l);
        }
        out.push(SrtCue { index, start_ms, end_ms, text });
    }
    Ok(out)
}

fn parse_timing(line: &str) -> Result<(u64, u64), String> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 { return Err("missing -->".into()); }
    Ok((parse_timestamp(parts[0].trim())?, parse_timestamp(parts[1].trim())?))
}

fn parse_timestamp(s: &str) -> Result<u64, String> {
    // HH:MM:SS,mmm
    let main = s.replace(',', ".");
    let parts: Vec<&str> = main.split(':').collect();
    if parts.len() != 3 { return Err("expected HH:MM:SS,mmm".into()); }
    let h: u64 = parts[0].parse().map_err(|_| "bad hh")?;
    let m: u64 = parts[1].parse().map_err(|_| "bad mm")?;
    let (sec, ms) = match parts[2].split_once('.') {
        Some((s, m)) => (s.parse::<u64>().map_err(|_| "bad ss")?, m.parse::<u64>().map_err(|_| "bad ms")?),
        None => (parts[2].parse::<u64>().map_err(|_| "bad ss")?, 0u64),
    };
    Ok(h * 3_600_000 + m * 60_000 + sec * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_cue() {
        let s = "1\n00:00:01,000 --> 00:00:03,500\nHello\nworld\n";
        let cues = parse(s).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 3500);
        assert_eq!(cues[0].text, "Hello\nworld");
    }

    #[test]
    fn parses_multiple_cues() {
        let s = "1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:03,000 --> 00:00:04,000\nB\n";
        let cues = parse(s).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[1].text, "B");
    }

    #[test]
    fn bad_index_errors() {
        let s = "abc\n00:00:01,000 --> 00:00:02,000\nA\n";
        assert!(parse(s).is_err());
    }

    #[test]
    fn timestamp_at_2h() {
        let s = "1\n02:00:00,500 --> 02:00:01,000\nx\n";
        let cues = parse(s).unwrap();
        assert_eq!(cues[0].start_ms, 7_200_500);
    }
}
