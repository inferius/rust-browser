//! WebVTT (.vtt) cue file parser.
//!
//! Spec: https://www.w3.org/TR/webvtt1/

#[derive(Debug, Clone, Default)]
pub struct VttFile {
    pub header: String,                        // "WEBVTT" + optional comment
    pub cues: Vec<VttCue>,
    pub regions: Vec<VttRegion>,
    pub styles: Vec<String>,                   // ::cue { ... } blocks
}

#[derive(Debug, Clone, Default)]
pub struct VttCue {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub settings: CueSettings,
}

#[derive(Debug, Clone, Default)]
pub struct CueSettings {
    pub vertical: Option<String>,              // "rl" or "lr"
    pub line: Option<String>,                  // percent or auto
    pub position: Option<String>,
    pub size: Option<String>,
    pub align: Option<String>,
    pub region_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VttRegion {
    pub id: String,
    pub width_percent: f32,
    pub lines: u32,
    pub region_anchor: (f32, f32),
    pub viewport_anchor: (f32, f32),
    pub scroll: bool,
}

pub fn parse(input: &str) -> Result<VttFile, String> {
    let mut file = VttFile::default();
    let mut lines = input.lines().peekable();
    let header = lines.next().ok_or("empty file")?.trim();
    if !header.starts_with("WEBVTT") {
        return Err("missing WEBVTT signature".into());
    }
    file.header = header.into();

    while let Some(_) = lines.peek() {
        // Skip blank lines.
        while lines.peek().map(|l| l.trim().is_empty()).unwrap_or(false) {
            lines.next();
        }
        let Some(first) = lines.peek().copied() else { break; };
        let first = first.trim();
        if first.starts_with("NOTE") {
            // consume note block until blank line
            for l in &mut lines {
                if l.trim().is_empty() { break; }
            }
            continue;
        }
        if first.starts_with("STYLE") {
            lines.next();
            let mut block = String::new();
            for l in &mut lines {
                if l.trim().is_empty() { break; }
                block.push_str(l);
                block.push('\n');
            }
            file.styles.push(block);
            continue;
        }
        // Cue: optional id line, then timing, then text lines.
        let mut id = String::new();
        let mut timing_line = first.to_string();
        if !first.contains("-->") {
            id = lines.next().unwrap().to_string();
            timing_line = lines.next().map(|l| l.to_string()).ok_or("missing timing")?;
        } else {
            lines.next();
        }
        let (start_ms, end_ms, settings) = parse_timing(&timing_line)?;
        let mut text = String::new();
        for l in &mut lines {
            if l.trim().is_empty() { break; }
            if !text.is_empty() { text.push('\n'); }
            text.push_str(l);
        }
        file.cues.push(VttCue { id, start_ms, end_ms, text, settings });
    }
    Ok(file)
}

fn parse_timing(line: &str) -> Result<(u64, u64, CueSettings), String> {
    let parts: Vec<&str> = line.splitn(2, "-->").collect();
    if parts.len() != 2 { return Err("missing -->".into()); }
    let start_ms = parse_timestamp(parts[0].trim())?;
    let rest = parts[1].trim();
    let (end_str, settings_str) = match rest.find(|c: char| c.is_whitespace()) {
        Some(i) => (rest[..i].to_string(), rest[i..].trim().to_string()),
        None => (rest.to_string(), String::new()),
    };
    let end_ms = parse_timestamp(&end_str)?;
    let mut settings = CueSettings::default();
    for tok in settings_str.split_ascii_whitespace() {
        if let Some((k, v)) = tok.split_once(':') {
            match k {
                "vertical" => settings.vertical = Some(v.into()),
                "line" => settings.line = Some(v.into()),
                "position" => settings.position = Some(v.into()),
                "size" => settings.size = Some(v.into()),
                "align" => settings.align = Some(v.into()),
                "region" => settings.region_id = Some(v.into()),
                _ => {}
            }
        }
    }
    Ok((start_ms, end_ms, settings))
}

fn parse_timestamp(s: &str) -> Result<u64, String> {
    // HH:MM:SS.mmm or MM:SS.mmm
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec_str) = match parts.len() {
        2 => (0, parts[0].parse().map_err(|_| "bad mm")?, parts[1]),
        3 => (
            parts[0].parse().map_err(|_| "bad hh")?,
            parts[1].parse().map_err(|_| "bad mm")?,
            parts[2],
        ),
        _ => return Err("bad timestamp".into()),
    };
    let (sec, ms) = match sec_str.split_once('.') {
        Some((s, m)) => (s.parse().map_err(|_| "bad ss")?, m.parse().map_err(|_| "bad ms")?),
        None => (sec_str.parse().map_err(|_| "bad ss")?, 0u64),
    };
    let h: u64 = h;
    let m: u64 = m;
    let sec: u64 = sec;
    Ok(h * 3_600_000 + m * 60_000 + sec * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_cue() {
        let src = "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nHello world\n";
        let file = parse(src).unwrap();
        assert_eq!(file.cues.len(), 1);
        assert_eq!(file.cues[0].start_ms, 0);
        assert_eq!(file.cues[0].end_ms, 5000);
        assert_eq!(file.cues[0].text, "Hello world");
    }

    #[test]
    fn parses_two_cues() {
        let src = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nA\n\n00:00:02.500 --> 00:00:03.000\nB\n";
        let file = parse(src).unwrap();
        assert_eq!(file.cues.len(), 2);
    }

    #[test]
    fn parses_with_id() {
        let src = "WEBVTT\n\nmyid\n00:00:01.000 --> 00:00:02.000\nA\n";
        let file = parse(src).unwrap();
        assert_eq!(file.cues[0].id, "myid");
    }

    #[test]
    fn parses_settings() {
        let src = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000 align:start line:50%\nText\n";
        let file = parse(src).unwrap();
        assert_eq!(file.cues[0].settings.align.as_deref(), Some("start"));
        assert_eq!(file.cues[0].settings.line.as_deref(), Some("50%"));
    }

    #[test]
    fn missing_signature_fails() {
        assert!(parse("NOT WEBVTT\n\n").is_err());
    }

    #[test]
    fn timestamp_hh_mm_ss_ms() {
        assert_eq!(parse_timestamp("01:30:45.500").unwrap(), 5_445_500);
    }

    #[test]
    fn timestamp_mm_ss_ms() {
        assert_eq!(parse_timestamp("01:30.250").unwrap(), 90_250);
    }
}
