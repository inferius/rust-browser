//! URLSearchParams - x-www-form-urlencoded parser/serializer.
//!
//! Spec: https://url.spec.whatwg.org/#interface-urlsearchparams

#[derive(Debug, Clone, Default)]
pub struct UrlSearchParams {
    /// Order-preserving entries; duplicates allowed.
    pub entries: Vec<(String, String)>,
}

impl UrlSearchParams {
    pub fn new() -> Self { Self::default() }

    pub fn parse(input: &str) -> Self {
        let mut params = Self::new();
        let input = input.trim_start_matches('?');
        for pair in input.split('&') {
            if pair.is_empty() { continue; }
            let (k, v) = match pair.find('=') {
                Some(i) => (&pair[..i], &pair[i + 1..]),
                None => (pair, ""),
            };
            params.entries.push((url_decode(k), url_decode(v)));
        }
        params
    }

    pub fn serialize(&self) -> String {
        self.entries.iter().map(|(k, v)| {
            format!("{}={}", url_encode(k), url_encode(v))
        }).collect::<Vec<_>>().join("&")
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn get_all(&self, name: &str) -> Vec<&str> {
        self.entries.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == name)
    }

    pub fn append(&mut self, name: &str, value: &str) {
        self.entries.push((name.into(), value.into()));
    }

    pub fn set(&mut self, name: &str, value: &str) {
        let mut found = false;
        self.entries.retain(|(k, _)| {
            if k == name {
                if found { false } else { found = true; true }
            } else { true }
        });
        if found {
            // Replace value of the first match in place.
            for (k, v) in self.entries.iter_mut() {
                if k == name { *v = value.into(); break; }
            }
        } else {
            self.entries.push((name.into(), value.into()));
        }
    }

    pub fn delete(&mut self, name: &str) {
        self.entries.retain(|(k, _)| k != name);
    }

    pub fn sort(&mut self) {
        // Stable sort by key (preserve insertion order for equal keys).
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => { out.push(b' '); i += 1; }
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v); i += 3;
                } else {
                    out.push(bytes[i]); i += 1;
                }
            }
            _ => { out.push(bytes[i]); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let p = UrlSearchParams::parse("a=1&b=2");
        assert_eq!(p.get("a"), Some("1"));
        assert_eq!(p.get("b"), Some("2"));
    }

    #[test]
    fn parse_strip_leading_question() {
        let p = UrlSearchParams::parse("?a=1");
        assert_eq!(p.get("a"), Some("1"));
    }

    #[test]
    fn parse_url_decode() {
        let p = UrlSearchParams::parse("q=a+b%20c");
        assert_eq!(p.get("q"), Some("a b c"));
    }

    #[test]
    fn serialize_round_trip() {
        let p = UrlSearchParams::parse("name=a%20b&x=1");
        assert!(p.serialize().contains("name=a+b"));
    }

    #[test]
    fn get_all_finds_duplicates() {
        let p = UrlSearchParams::parse("k=1&k=2&k=3");
        assert_eq!(p.get_all("k"), vec!["1", "2", "3"]);
    }

    #[test]
    fn set_replaces_first_removes_rest() {
        let mut p = UrlSearchParams::parse("k=a&k=b&k=c");
        p.set("k", "X");
        assert_eq!(p.get_all("k"), vec!["X"]);
    }

    #[test]
    fn sort_stable() {
        let mut p = UrlSearchParams::parse("b=2&a=1&b=3");
        p.sort();
        assert_eq!(p.entries[0].0, "a");
        // b=2 stays before b=3.
        assert_eq!(p.entries[1].1, "2");
    }

    #[test]
    fn delete_removes_all() {
        let mut p = UrlSearchParams::parse("k=1&k=2");
        p.delete("k");
        assert!(!p.has("k"));
    }
}
