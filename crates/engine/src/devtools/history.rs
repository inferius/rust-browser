//! Browser history persistence v profile dir.
//!
//! Format: ~/.rwe/profiles/<active>/history.json - JSON array of HistoryEntry.
//! Append per navigation, load pri startu shell mode.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visited_at: u64, // Unix timestamp seconds.
}

pub fn history_path() -> Option<PathBuf> {
    let dir = super::profile::ensure_profile_dir(super::profile::active_profile())?;
    Some(dir.join("history.json"))
}

pub fn load_history() -> Vec<HistoryEntry> {
    let Some(path) = history_path() else { return Vec::new() };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new() };
    parse_history_json(&content)
}

pub fn append_entry(entry: &HistoryEntry) {
    let mut existing = load_history();
    // Dedup: pokud posledni entry ma stejny URL v poslednich 60s, skip.
    if let Some(last) = existing.last() {
        if last.url == entry.url && entry.visited_at - last.visited_at < 60 {
            return;
        }
    }
    existing.push(entry.clone());
    // Trim na 1000 nejnovejsich.
    if existing.len() > 1000 {
        existing.drain(0..existing.len() - 1000);
    }
    save_history(&existing);
}

pub fn save_history(entries: &[HistoryEntry]) {
    let Some(path) = history_path() else { return };
    let mut json = String::from("[\n");
    for (i, e) in entries.iter().enumerate() {
        let comma = if i + 1 < entries.len() { "," } else { "" };
        json.push_str(&format!(
            "  {{\"url\":\"{}\",\"title\":\"{}\",\"visited_at\":{}}}{}\n",
            json_escape(&e.url), json_escape(&e.title), e.visited_at, comma
        ));
    }
    json.push_str("]\n");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, json);
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
     .replace('\n', "\\n").replace('\r', "")
}

fn parse_history_json(s: &str) -> Vec<HistoryEntry> {
    // Lite parser - hleda objekty {"url":"...","title":"...","visited_at":N}.
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if let Some(open) = s[i..].find('{') {
            let abs = i + open;
            let close = s[abs..].find('}').map(|c| abs + c);
            if let Some(close) = close {
                let obj = &s[abs..=close];
                let url = extract_field(obj, "url").unwrap_or_default();
                let title = extract_field(obj, "title").unwrap_or_default();
                let visited_at = extract_num_field(obj, "visited_at").unwrap_or(0);
                if !url.is_empty() {
                    out.push(HistoryEntry { url, title, visited_at });
                }
                i = close + 1;
                continue;
            }
        }
        break;
    }
    out
}

fn extract_field(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let idx = s.find(&pat)?;
    let after = &s[idx + pat.len()..];
    let colon = after.find(':')?;
    let after = &after[colon + 1..];
    let q1 = after.find('"')?;
    let after = &after[q1 + 1..];
    let q2 = after.find('"')?;
    Some(after[..q2].replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\"))
}

fn extract_num_field(s: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{}\"", key);
    let idx = s.find(&pat)?;
    let after = &s[idx + pat.len()..];
    let colon = after.find(':')?;
    let after = &after[colon + 1..].trim_start();
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}

pub fn now_ts() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let json = r#"[
            {"url":"https://example.com","title":"Example","visited_at":1234567890}
        ]"#;
        let entries = parse_history_json(json);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://example.com");
        assert_eq!(entries[0].title, "Example");
        assert_eq!(entries[0].visited_at, 1234567890);
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse_history_json("[]").len(), 0);
        assert_eq!(parse_history_json("").len(), 0);
    }

    #[test]
    fn parse_multiple() {
        let json = r#"[
            {"url":"a.com","title":"A","visited_at":1},
            {"url":"b.com","title":"B","visited_at":2}
        ]"#;
        let entries = parse_history_json(json);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn json_escape_quote() {
        assert_eq!(json_escape("a\"b"), "a\\\"b");
        assert_eq!(json_escape("a\\b"), "a\\\\b");
    }
}
