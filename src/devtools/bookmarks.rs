//! Bookmark persistence v profile dir. ~/.rwe/profiles/<active>/bookmarks.json.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

pub fn bookmarks_path() -> Option<PathBuf> {
    let dir = super::profile::ensure_profile_dir(super::profile::active_profile())?;
    Some(dir.join("bookmarks.json"))
}

pub fn load_bookmarks() -> Vec<Bookmark> {
    let Some(path) = bookmarks_path() else { return Vec::new() };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new() };
    parse_bookmarks_json(&content)
}

pub fn save_bookmarks(bms: &[Bookmark]) {
    let Some(path) = bookmarks_path() else { return };
    let mut json = String::from("[\n");
    for (i, b) in bms.iter().enumerate() {
        let comma = if i + 1 < bms.len() { "," } else { "" };
        json.push_str(&format!(
            "  {{\"url\":\"{}\",\"title\":\"{}\"}}{}\n",
            json_escape(&b.url), json_escape(&b.title), comma
        ));
    }
    json.push_str("]\n");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, json);
}

pub fn add_bookmark(url: &str, title: &str) {
    let mut bms = load_bookmarks();
    if bms.iter().any(|b| b.url == url) { return; }
    bms.push(Bookmark { url: url.to_string(), title: title.to_string() });
    save_bookmarks(&bms);
}

pub fn remove_bookmark(url: &str) {
    let mut bms = load_bookmarks();
    bms.retain(|b| b.url != url);
    save_bookmarks(&bms);
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
     .replace('\n', "\\n").replace('\r', "")
}

fn parse_bookmarks_json(s: &str) -> Vec<Bookmark> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if let Some(open) = s[i..].find('{') {
            let abs = i + open;
            let close = s[abs..].find('}').map(|c| abs + c);
            if let Some(close) = close {
                let obj = &s[abs..=close];
                let url = extract_field(obj, "url").unwrap_or_default();
                let title = extract_field(obj, "title").unwrap_or_default();
                if !url.is_empty() {
                    out.push(Bookmark { url, title });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_bm() {
        let json = r#"[{"url":"https://example.com","title":"Example"}]"#;
        let bms = parse_bookmarks_json(json);
        assert_eq!(bms.len(), 1);
        assert_eq!(bms[0].url, "https://example.com");
        assert_eq!(bms[0].title, "Example");
    }

    #[test]
    fn parse_empty_bm() {
        assert_eq!(parse_bookmarks_json("[]").len(), 0);
    }

    #[test]
    fn parse_multi_bm() {
        let json = r#"[{"url":"a","title":"A"},{"url":"b","title":"B"}]"#;
        assert_eq!(parse_bookmarks_json(json).len(), 2);
    }
}
