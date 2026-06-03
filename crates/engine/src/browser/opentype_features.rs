//! OpenType feature tags (`font-feature-settings`, `font-variant-*`).
//!
//! Spec: https://www.w3.org/TR/css-fonts-4/#feature-tag-value
//! OpenType GSUB/GPOS feature toggling: liga, kern, smcp, onum, frac, etc.

use std::collections::HashMap;

/// 4-char ASCII OpenType feature tag (stored as u32 BE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureTag(pub [u8; 4]);

impl FeatureTag {
    pub fn parse(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 4 { return None; }
        let mut out = [0u8; 4];
        for (i, b) in bytes.iter().enumerate() {
            if !b.is_ascii_alphanumeric() && *b != b' ' { return None; }
            out[i] = *b;
        }
        Some(FeatureTag(out))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("????")
    }

    pub fn liga() -> Self { Self([b'l', b'i', b'g', b'a']) }
    pub fn kern() -> Self { Self([b'k', b'e', b'r', b'n']) }
    pub fn smcp() -> Self { Self([b's', b'm', b'c', b'p']) }
    pub fn onum() -> Self { Self([b'o', b'n', b'u', b'm']) }
    pub fn lnum() -> Self { Self([b'l', b'n', b'u', b'm']) }
    pub fn tnum() -> Self { Self([b't', b'n', b'u', b'm']) }
    pub fn frac() -> Self { Self([b'f', b'r', b'a', b'c']) }
    pub fn dlig() -> Self { Self([b'd', b'l', b'i', b'g']) }
    pub fn salt() -> Self { Self([b's', b'a', b'l', b't']) }
    pub fn ss(n: u8) -> Self {
        let nstr = format!("{:02}", n.min(99));
        let b = nstr.as_bytes();
        Self([b's', b's', b[0], b[1]])
    }
}

#[derive(Debug, Clone, Default)]
pub struct FeatureSettings {
    pub features: HashMap<FeatureTag, u32>,    // tag -> alternate index (1 = on, 0 = off)
}

impl FeatureSettings {
    pub fn new() -> Self { Self::default() }

    pub fn enable(&mut self, tag: FeatureTag) {
        self.features.insert(tag, 1);
    }

    pub fn disable(&mut self, tag: FeatureTag) {
        self.features.insert(tag, 0);
    }

    pub fn set_alt(&mut self, tag: FeatureTag, alt: u32) {
        self.features.insert(tag, alt);
    }

    pub fn is_enabled(&self, tag: FeatureTag) -> bool {
        self.features.get(&tag).map(|v| *v > 0).unwrap_or(false)
    }

    /// Parse `font-feature-settings` value.
    /// e.g. `"liga" on, "smcp" off, "ss03" 2`
    pub fn parse_css(input: &str) -> Self {
        let mut s = Self::new();
        for entry in input.split(',') {
            let parts: Vec<&str> = entry.trim().split_ascii_whitespace().collect();
            if parts.is_empty() { continue; }
            let tag_str = parts[0].trim_matches('"').trim_matches('\'');
            let Some(tag) = FeatureTag::parse(tag_str) else { continue; };
            let value = match parts.get(1).copied().unwrap_or("on") {
                "on" => 1,
                "off" => 0,
                n => n.parse().unwrap_or(1),
            };
            s.features.insert(tag, value);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag() {
        let t = FeatureTag::parse("liga").unwrap();
        assert_eq!(t.as_str(), "liga");
    }

    #[test]
    fn parse_invalid_length() {
        assert!(FeatureTag::parse("lig").is_none());
        assert!(FeatureTag::parse("liga1").is_none());
    }

    #[test]
    fn standard_tags() {
        assert_eq!(FeatureTag::liga().as_str(), "liga");
        assert_eq!(FeatureTag::kern().as_str(), "kern");
        assert_eq!(FeatureTag::smcp().as_str(), "smcp");
    }

    #[test]
    fn stylistic_set_tag() {
        assert_eq!(FeatureTag::ss(3).as_str(), "ss03");
        assert_eq!(FeatureTag::ss(12).as_str(), "ss12");
    }

    #[test]
    fn enable_disable() {
        let mut s = FeatureSettings::new();
        s.enable(FeatureTag::liga());
        assert!(s.is_enabled(FeatureTag::liga()));
        s.disable(FeatureTag::liga());
        assert!(!s.is_enabled(FeatureTag::liga()));
    }

    #[test]
    fn parse_css_value() {
        let s = FeatureSettings::parse_css("\"liga\" on, \"smcp\" off, \"ss03\" 2");
        assert!(s.is_enabled(FeatureTag::liga()));
        assert!(!s.is_enabled(FeatureTag::smcp()));
        assert_eq!(s.features.get(&FeatureTag::ss(3)).copied(), Some(2));
    }
}
