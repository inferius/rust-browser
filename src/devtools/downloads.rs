//! Downloads tracker - persisted v profile/downloads.json.
//! Real download (Ctrl+S, save link as) zaznamenan + listed v about:downloads.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub url: String,
    pub path: String,
    pub size_bytes: u64,
    pub timestamp_ts: u64,
    pub mime: String,
}

pub fn downloads_path() -> Option<PathBuf> {
    let dir = super::profile::ensure_profile_dir(super::profile::active_profile())?;
    Some(dir.join("downloads.json"))
}

pub fn load_downloads() -> Vec<DownloadRecord> {
    let Some(path) = downloads_path() else { return Vec::new() };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new() };
    parse_downloads_json(&content)
}

pub fn save_downloads(items: &[DownloadRecord]) {
    let Some(path) = downloads_path() else { return };
    let mut json = String::from("[\n");
    for (i, d) in items.iter().enumerate() {
        let comma = if i + 1 < items.len() { "," } else { "" };
        json.push_str(&format!(
            "  {{\"url\":\"{}\",\"path\":\"{}\",\"size\":{},\"ts\":{},\"mime\":\"{}\"}}{}\n",
            json_escape(&d.url), json_escape(&d.path), d.size_bytes, d.timestamp_ts,
            json_escape(&d.mime), comma
        ));
    }
    json.push_str("]\n");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, json);
}

pub fn append_record(rec: &DownloadRecord) {
    let mut items = load_downloads();
    items.push(rec.clone());
    if items.len() > 200 { items.remove(0); }
    save_downloads(&items);
}

pub fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
     .replace('\n', "\\n").replace('\r', "")
}

fn parse_downloads_json(s: &str) -> Vec<DownloadRecord> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if let Some(open) = s[i..].find('{') {
            let abs = i + open;
            let close = s[abs..].find('}').map(|c| abs + c);
            if let Some(close) = close {
                let obj = &s[abs..=close];
                let url = extract_str(obj, "url").unwrap_or_default();
                let path = extract_str(obj, "path").unwrap_or_default();
                let size = extract_num(obj, "size").unwrap_or(0);
                let ts = extract_num(obj, "ts").unwrap_or(0);
                let mime = extract_str(obj, "mime").unwrap_or_default();
                if !url.is_empty() {
                    out.push(DownloadRecord {
                        url, path, size_bytes: size, timestamp_ts: ts, mime,
                    });
                }
                i = close + 1;
                continue;
            }
        }
        break;
    }
    out
}

fn extract_str(s: &str, key: &str) -> Option<String> {
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

fn extract_num(s: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{}\":", key);
    let idx = s.find(&pat)?;
    let after = &s[idx + pat.len()..].trim_start();
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let json = r#"[{"url":"https://x.com/a.pdf","path":"C:\\Downloads\\a.pdf","size":1024,"ts":100,"mime":"application/pdf"}]"#;
        let r = parse_downloads_json(json);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://x.com/a.pdf");
        assert_eq!(r[0].size_bytes, 1024);
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse_downloads_json("[]").len(), 0);
    }
}
