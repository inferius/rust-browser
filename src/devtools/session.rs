//! Session restore - ulozi stav otevrenych zalozek pri quit, nahraje pri start.
//! ~/.rwe/profiles/<active>/session.json.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SessionTab {
    pub url: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
}

pub fn session_path() -> Option<PathBuf> {
    let dir = super::profile::ensure_profile_dir(super::profile::active_profile())?;
    Some(dir.join("session.json"))
}

pub fn save_session(s: &Session) {
    let Some(path) = session_path() else { return };
    let mut json = String::from("{\n  \"active\": ");
    json.push_str(&s.active.to_string());
    json.push_str(",\n  \"tabs\": [\n");
    for (i, t) in s.tabs.iter().enumerate() {
        let comma = if i + 1 < s.tabs.len() { "," } else { "" };
        let url = t.url.clone().unwrap_or_default();
        json.push_str(&format!(
            "    {{\"url\":\"{}\",\"title\":\"{}\"}}{}\n",
            json_escape(&url), json_escape(&t.title), comma
        ));
    }
    json.push_str("  ]\n}\n");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, json);
}

pub fn load_session() -> Option<Session> {
    let path = session_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    parse_session(&content)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
     .replace('\n', "\\n").replace('\r', "")
}

fn parse_session(s: &str) -> Option<Session> {
    let active = extract_num_field(s, "active").unwrap_or(0) as usize;
    // Parse tabs array.
    let mut tabs = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if let Some(open) = s[i..].find('{') {
            let abs = i + open;
            // Skip prvni { (root object).
            if abs < 5 { i = abs + 1; continue; }
            let close = s[abs..].find('}').map(|c| abs + c)?;
            let obj = &s[abs..=close];
            let url = extract_field(obj, "url").filter(|s| !s.is_empty());
            let title = extract_field(obj, "title").unwrap_or_default();
            tabs.push(SessionTab { url, title });
            i = close + 1;
        } else { break; }
    }
    if tabs.is_empty() { return None; }
    Some(Session { tabs, active })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_simple() {
        let json = r#"{
            "active": 1,
            "tabs": [
                {"url":"a.com","title":"A"},
                {"url":"b.com","title":"B"}
            ]
        }"#;
        let s = parse_session(json).unwrap();
        assert_eq!(s.active, 1);
        assert_eq!(s.tabs.len(), 2);
        assert_eq!(s.tabs[0].url.as_deref(), Some("a.com"));
        assert_eq!(s.tabs[1].title, "B");
    }

    #[test]
    fn parse_session_empty_tabs() {
        let json = r#"{"active":0,"tabs":[]}"#;
        assert!(parse_session(json).is_none());
    }
}
