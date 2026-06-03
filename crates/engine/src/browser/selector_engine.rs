//! CSS Selectors Level 4 matcher (subset).
//!
//! Spec: https://www.w3.org/TR/selectors-4/
//! Existing project relies on `selectors` crate via cascade.rs; this module
//! provides a tiny standalone matcher used by devtools/search + AT queries.

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    Universal,
    Tag(String),
    Id(String),
    Class(String),
    Attribute { name: String, value_match: AttrMatch },
    Pseudo(String),
    PseudoFunctional { name: String, arg: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrMatch {
    Exists,
    Exact(String),
    Includes(String),    // ~=
    Dash(String),        // |=
    Prefix(String),      // ^=
    Suffix(String),      // $=
    Substring(String),   // *=
}

#[derive(Debug, Clone)]
pub struct CompoundSelector {
    pub parts: Vec<SimpleSelector>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
    Column,           // CSS L4 || (table column descendant)
}

#[derive(Debug, Clone)]
pub struct ComplexSelector {
    pub compounds: Vec<(CompoundSelector, Option<Combinator>)>, // pairs: (compound, combinator-to-next)
}

#[derive(Debug, Clone)]
pub struct ElementSnapshot {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<(String, String)>,
}

impl ElementSnapshot {
    pub fn has_class(&self, c: &str) -> bool {
        self.classes.iter().any(|x| x == c)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub fn matches_simple(sel: &SimpleSelector, el: &ElementSnapshot) -> bool {
    match sel {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(t) => el.tag.eq_ignore_ascii_case(t),
        SimpleSelector::Id(i) => el.id.as_deref() == Some(i.as_str()),
        SimpleSelector::Class(c) => el.has_class(c),
        SimpleSelector::Attribute { name, value_match } => attribute_match(el, name, value_match),
        SimpleSelector::Pseudo(_) => false, // state pseudos not modeled here
        SimpleSelector::PseudoFunctional { .. } => false,
    }
}

fn attribute_match(el: &ElementSnapshot, name: &str, m: &AttrMatch) -> bool {
    let Some(actual) = el.attr(name) else { return false; };
    match m {
        AttrMatch::Exists => true,
        AttrMatch::Exact(v) => actual == v,
        AttrMatch::Includes(v) => actual.split_ascii_whitespace().any(|w| w == v),
        AttrMatch::Dash(v) => actual == v || actual.starts_with(&format!("{}-", v)),
        AttrMatch::Prefix(v) => actual.starts_with(v.as_str()),
        AttrMatch::Suffix(v) => actual.ends_with(v.as_str()),
        AttrMatch::Substring(v) => actual.contains(v.as_str()),
    }
}

pub fn matches_compound(c: &CompoundSelector, el: &ElementSnapshot) -> bool {
    c.parts.iter().all(|p| matches_simple(p, el))
}

/// Parse a tiny subset: tag, .class, #id, [attr=value], combinators ` ` and `>`.
pub fn parse(input: &str) -> Option<ComplexSelector> {
    let mut compounds: Vec<(CompoundSelector, Option<Combinator>)> = Vec::new();
    let trimmed = input.trim();
    if trimmed.is_empty() { return None; }
    let tokens = tokenize(trimmed);
    let mut i = 0;
    while i < tokens.len() {
        let mut parts = Vec::new();
        while i < tokens.len() {
            let tok = &tokens[i];
            if tok == " " || tok == ">" || tok == "+" || tok == "~" { break; }
            parts.push(parse_simple(tok)?);
            i += 1;
        }
        let combinator = if i < tokens.len() {
            let c = match tokens[i].as_str() {
                " " => Some(Combinator::Descendant),
                ">" => Some(Combinator::Child),
                "+" => Some(Combinator::NextSibling),
                "~" => Some(Combinator::SubsequentSibling),
                _ => None,
            };
            i += 1;
            c
        } else { None };
        compounds.push((CompoundSelector { parts }, combinator));
    }
    Some(ComplexSelector { compounds })
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = s.chars().peekable();
    let mut in_brackets = false;
    while let Some(c) = chars.next() {
        if in_brackets {
            buf.push(c);
            if c == ']' { in_brackets = false; out.push(std::mem::take(&mut buf)); }
            continue;
        }
        if c == '[' {
            if !buf.is_empty() { out.push(std::mem::take(&mut buf)); }
            in_brackets = true;
            buf.push(c);
            continue;
        }
        if c.is_whitespace() {
            if !buf.is_empty() { out.push(std::mem::take(&mut buf)); }
            while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) { chars.next(); }
            out.push(" ".into());
        } else if matches!(c, '>' | '+' | '~') {
            if !buf.is_empty() { out.push(std::mem::take(&mut buf)); }
            if out.last().map(|s| s == " ").unwrap_or(false) { out.pop(); }
            out.push(c.to_string());
            while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) { chars.next(); }
        } else if matches!(c, '.' | '#' | ':') {
            // Start of new simple selector inside compound - flush buffer.
            if !buf.is_empty() { out.push(std::mem::take(&mut buf)); }
            buf.push(c);
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() { out.push(buf); }
    out
}

fn parse_simple(token: &str) -> Option<SimpleSelector> {
    let bytes = token.as_bytes();
    if token == "*" { return Some(SimpleSelector::Universal); }
    if bytes[0] == b'#' { return Some(SimpleSelector::Id(token[1..].into())); }
    if bytes[0] == b'.' { return Some(SimpleSelector::Class(token[1..].into())); }
    if bytes[0] == b'[' {
        let inner = token.trim_start_matches('[').trim_end_matches(']');
        let (name, m) = parse_attr_inner(inner)?;
        return Some(SimpleSelector::Attribute { name, value_match: m });
    }
    if bytes[0] == b':' {
        return Some(SimpleSelector::Pseudo(token[1..].into()));
    }
    Some(SimpleSelector::Tag(token.into()))
}

fn parse_attr_inner(inner: &str) -> Option<(String, AttrMatch)> {
    for op in &["~=", "|=", "^=", "$=", "*=", "="] {
        if let Some(i) = inner.find(op) {
            let name = inner[..i].trim().to_string();
            let val = inner[i + op.len()..].trim().trim_matches(|c: char| c == '"' || c == '\'').to_string();
            let m = match *op {
                "=" => AttrMatch::Exact(val),
                "~=" => AttrMatch::Includes(val),
                "|=" => AttrMatch::Dash(val),
                "^=" => AttrMatch::Prefix(val),
                "$=" => AttrMatch::Suffix(val),
                "*=" => AttrMatch::Substring(val),
                _ => unreachable!(),
            };
            return Some((name, m));
        }
    }
    Some((inner.trim().to_string(), AttrMatch::Exists))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el() -> ElementSnapshot {
        ElementSnapshot {
            tag: "div".into(),
            id: Some("main".into()),
            classes: vec!["a".into(), "btn".into()],
            attributes: vec![("type".into(), "submit".into()), ("data-x".into(), "1".into())],
        }
    }

    #[test]
    fn matches_tag() {
        assert!(matches_simple(&SimpleSelector::Tag("div".into()), &el()));
        assert!(!matches_simple(&SimpleSelector::Tag("span".into()), &el()));
    }

    #[test]
    fn matches_id_and_class() {
        assert!(matches_simple(&SimpleSelector::Id("main".into()), &el()));
        assert!(matches_simple(&SimpleSelector::Class("btn".into()), &el()));
    }

    #[test]
    fn matches_attr_exact() {
        assert!(matches_simple(&SimpleSelector::Attribute {
            name: "type".into(), value_match: AttrMatch::Exact("submit".into())
        }, &el()));
    }

    #[test]
    fn parse_simple_class() {
        let s = parse(".btn").unwrap();
        assert_eq!(s.compounds.len(), 1);
    }

    #[test]
    fn parse_compound() {
        let s = parse("div.btn#main").unwrap();
        assert_eq!(s.compounds[0].0.parts.len(), 3);
    }

    #[test]
    fn parse_descendant() {
        let s = parse("div span").unwrap();
        assert_eq!(s.compounds.len(), 2);
        assert_eq!(s.compounds[0].1, Some(Combinator::Descendant));
    }

    #[test]
    fn parse_child_combinator() {
        let s = parse("ul > li").unwrap();
        assert_eq!(s.compounds[0].1, Some(Combinator::Child));
    }

    #[test]
    fn parse_attr() {
        let s = parse("[data-x=\"1\"]").unwrap();
        let part = &s.compounds[0].0.parts[0];
        match part {
            SimpleSelector::Attribute { name, value_match } => {
                assert_eq!(name, "data-x");
                assert_eq!(*value_match, AttrMatch::Exact("1".into()));
            }
            _ => panic!("expected attribute"),
        }
    }
}
