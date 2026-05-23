//! ECMA-262 RegExp - flags + compile + execute (basic patterns).
//!
//! Real impl delegates to fancy-regex crate. This module models the JS-side
//! state (lastIndex, exec result groups) + flags parsing.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegExpFlag {
    Global,
    IgnoreCase,
    Multiline,
    DotAll,
    Sticky,
    Unicode,
    UnicodeSets,
    HasIndices,
}

impl RegExpFlag {
    pub fn letter(&self) -> char {
        match self {
            Self::Global => 'g',
            Self::IgnoreCase => 'i',
            Self::Multiline => 'm',
            Self::DotAll => 's',
            Self::Sticky => 'y',
            Self::Unicode => 'u',
            Self::UnicodeSets => 'v',
            Self::HasIndices => 'd',
        }
    }
}

pub fn parse_flags(flags: &str) -> Result<Vec<RegExpFlag>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for c in flags.chars() {
        let f = match c {
            'g' => RegExpFlag::Global,
            'i' => RegExpFlag::IgnoreCase,
            'm' => RegExpFlag::Multiline,
            's' => RegExpFlag::DotAll,
            'y' => RegExpFlag::Sticky,
            'u' => RegExpFlag::Unicode,
            'v' => RegExpFlag::UnicodeSets,
            'd' => RegExpFlag::HasIndices,
            _ => return Err(format!("invalid regex flag '{}'", c)),
        };
        if !seen.insert(f) {
            return Err(format!("duplicate flag '{}'", c));
        }
        out.push(f);
    }
    // u + v mutually exclusive.
    if out.contains(&RegExpFlag::Unicode) && out.contains(&RegExpFlag::UnicodeSets) {
        return Err("u and v flags are mutually exclusive".into());
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct RegExpInstance {
    pub source: String,
    pub flags: Vec<RegExpFlag>,
    pub last_index: usize,
    pub named_groups: HashMap<String, u32>,
}

impl RegExpInstance {
    pub fn new(source: &str, flags: &str) -> Result<Self, String> {
        Ok(Self {
            source: source.into(),
            flags: parse_flags(flags)?,
            last_index: 0,
            named_groups: HashMap::new(),
        })
    }

    pub fn has(&self, flag: RegExpFlag) -> bool {
        self.flags.contains(&flag)
    }

    pub fn flag_string(&self) -> String {
        let mut s = String::new();
        for f in [
            RegExpFlag::HasIndices,
            RegExpFlag::Global,
            RegExpFlag::IgnoreCase,
            RegExpFlag::Multiline,
            RegExpFlag::DotAll,
            RegExpFlag::Unicode,
            RegExpFlag::UnicodeSets,
            RegExpFlag::Sticky,
        ] {
            if self.flags.contains(&f) { s.push(f.letter()); }
        }
        s
    }
}

#[derive(Debug, Clone)]
pub struct MatchGroup {
    pub start: usize,
    pub end: usize,
    pub captured: String,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub index: usize,
    pub input: String,
    pub groups: Vec<Option<MatchGroup>>,
    pub named: HashMap<String, MatchGroup>,
    pub indices_array: Option<Vec<Option<(usize, usize)>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_flags() {
        let f = parse_flags("gim").unwrap();
        assert!(f.contains(&RegExpFlag::Global));
        assert!(f.contains(&RegExpFlag::IgnoreCase));
        assert!(f.contains(&RegExpFlag::Multiline));
    }

    #[test]
    fn duplicate_flag_errors() {
        assert!(parse_flags("gg").is_err());
    }

    #[test]
    fn u_v_mutually_exclusive() {
        assert!(parse_flags("uv").is_err());
    }

    #[test]
    fn invalid_flag_errors() {
        assert!(parse_flags("x").is_err());
    }

    #[test]
    fn flag_string_canonical_order() {
        let r = RegExpInstance::new("abc", "yig").unwrap();
        assert_eq!(r.flag_string(), "giy");
    }

    #[test]
    fn instance_has_query() {
        let r = RegExpInstance::new(".", "gi").unwrap();
        assert!(r.has(RegExpFlag::Global));
        assert!(r.has(RegExpFlag::IgnoreCase));
        assert!(!r.has(RegExpFlag::Multiline));
    }
}
