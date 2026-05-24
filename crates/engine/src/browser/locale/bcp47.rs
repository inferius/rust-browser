//! BCP 47 language tag parser.
//!
//! Spec: https://www.rfc-editor.org/rfc/rfc5646
//! Tag = language ["-" script] ["-" region] *["-" variant] *["-" extension]

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LanguageTag {
    pub language: String,            // ISO 639-1/3 lowercase ("en", "zh")
    pub script: Option<String>,      // ISO 15924 title-case ("Latn", "Cyrl")
    pub region: Option<String>,      // ISO 3166-1 uppercase ("US", "CZ")
    pub variants: Vec<String>,
    pub extensions: Vec<(char, Vec<String>)>,
    pub private_use: Vec<String>,
}

pub fn parse(tag: &str) -> Result<LanguageTag, String> {
    let mut t = LanguageTag::default();
    let mut iter = tag.split('-').peekable();
    let lang = iter.next().ok_or("empty tag")?;
    if lang.is_empty() || !lang.chars().all(|c| c.is_ascii_alphabetic()) || lang.len() < 2 || lang.len() > 8 {
        return Err(format!("invalid language subtag '{}'", lang));
    }
    t.language = lang.to_ascii_lowercase();
    while let Some(part) = iter.peek().copied() {
        if part == "x" {
            iter.next();
            while let Some(p) = iter.next() { t.private_use.push(p.to_string()); }
            break;
        }
        if part.len() == 1 && part.chars().next().unwrap().is_ascii_alphabetic() {
            let prefix = iter.next().unwrap().chars().next().unwrap();
            let mut group = Vec::new();
            while let Some(p) = iter.peek().copied() {
                if p.len() == 1 || p == "x" { break; }
                group.push(iter.next().unwrap().to_string());
            }
            t.extensions.push((prefix, group));
            continue;
        }
        let p = iter.next().unwrap();
        if p.len() == 4 && p.chars().all(|c| c.is_ascii_alphabetic()) && t.script.is_none() && t.region.is_none() {
            t.script = Some(title_case(p));
        } else if (p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic())
                || p.len() == 3 && p.chars().all(|c| c.is_ascii_digit())) && t.region.is_none() {
            t.region = Some(p.to_ascii_uppercase());
        } else if p.len() >= 4 || (p.len() == 5 && p.chars().next().unwrap().is_ascii_digit()) {
            t.variants.push(p.to_ascii_lowercase());
        } else {
            return Err(format!("unrecognized subtag '{}'", p));
        }
    }
    Ok(t)
}

fn title_case(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if let Some(c) = chars.get_mut(0) { *c = c.to_ascii_uppercase(); }
    for c in chars.iter_mut().skip(1) { *c = c.to_ascii_lowercase(); }
    chars.into_iter().collect()
}

impl LanguageTag {
    pub fn canonical(&self) -> String {
        let mut s = self.language.clone();
        if let Some(scr) = &self.script { s.push('-'); s.push_str(scr); }
        if let Some(r) = &self.region { s.push('-'); s.push_str(r); }
        for v in &self.variants { s.push('-'); s.push_str(v); }
        for (k, vs) in &self.extensions {
            s.push('-'); s.push(*k);
            for v in vs { s.push('-'); s.push_str(v); }
        }
        if !self.private_use.is_empty() {
            s.push_str("-x");
            for v in &self.private_use { s.push('-'); s.push_str(v); }
        }
        s
    }

    /// Lookup with locale fallback per RFC 4647.
    /// e.g. ["en-US", "en-GB", "fr-FR"] for "en-US" -> matches "en-US",
    /// for "en-CA" -> matches "en-GB" via truncation to "en".
    pub fn matches(&self, available: &[&str]) -> Option<String> {
        for tag in available {
            if tag.eq_ignore_ascii_case(&self.canonical()) {
                return Some((*tag).to_string());
            }
        }
        // Fall back: drop most-specific subtags.
        let canonical = self.canonical();
        let mut parts: Vec<&str> = canonical.split('-').collect();
        while parts.len() > 1 {
            parts.pop();
            let candidate = parts.join("-");
            for tag in available {
                if tag.split('-').take(parts.len()).collect::<Vec<_>>().join("-").eq_ignore_ascii_case(&candidate) {
                    return Some((*tag).to_string());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let t = parse("en").unwrap();
        assert_eq!(t.language, "en");
        assert!(t.region.is_none());
    }

    #[test]
    fn parse_lang_region() {
        let t = parse("en-US").unwrap();
        assert_eq!(t.language, "en");
        assert_eq!(t.region.as_deref(), Some("US"));
    }

    #[test]
    fn parse_lang_script_region() {
        let t = parse("zh-Hant-TW").unwrap();
        assert_eq!(t.language, "zh");
        assert_eq!(t.script.as_deref(), Some("Hant"));
        assert_eq!(t.region.as_deref(), Some("TW"));
    }

    #[test]
    fn parse_extension() {
        let t = parse("en-u-ca-gregory").unwrap();
        assert_eq!(t.extensions[0].0, 'u');
        assert_eq!(t.extensions[0].1, vec!["ca".to_string(), "gregory".to_string()]);
    }

    #[test]
    fn parse_private_use() {
        let t = parse("en-x-priv1-priv2").unwrap();
        assert_eq!(t.private_use, vec!["priv1".to_string(), "priv2".to_string()]);
    }

    #[test]
    fn canonical_round_trip() {
        let t = parse("zh-Hant-TW").unwrap();
        assert_eq!(t.canonical(), "zh-Hant-TW");
    }

    #[test]
    fn match_exact() {
        let t = parse("en-US").unwrap();
        assert_eq!(t.matches(&["en-US", "fr-FR"]).as_deref(), Some("en-US"));
    }

    #[test]
    fn match_fallback() {
        let t = parse("en-CA").unwrap();
        assert!(t.matches(&["en-US", "fr-FR"]).is_some());
    }

    #[test]
    fn match_none() {
        let t = parse("ja-JP").unwrap();
        assert!(t.matches(&["en-US", "fr-FR"]).is_none());
    }
}
