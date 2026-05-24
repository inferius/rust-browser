//! HTML quirks mode detection from DOCTYPE.
//!
//! Spec: https://html.spec.whatwg.org/multipage/parsing.html#the-initial-insertion-mode

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocumentMode {
    Standards,
    Quirks,
    LimitedQuirks,
}

#[derive(Debug, Clone)]
pub struct DoctypeSpec {
    pub name: String,
    pub public_id: String,
    pub system_id: String,
}

impl Default for DoctypeSpec {
    fn default() -> Self {
        Self { name: String::new(), public_id: String::new(), system_id: String::new() }
    }
}

const QUIRKS_PUBLIC_PREFIXES: &[&str] = &[
    "-//IETF//DTD HTML",
    "-//W3C//DTD HTML 3",
    "-//W3C//DTD HTML 3.2",
    "html",
    "-//W3O//DTD W3 HTML Strict 3.0//EN//",
];

const QUIRKS_PUBLIC_IDS: &[&str] = &[
    "-/W3C/DTD HTML 4.0 Transitional/EN",
];

const LIMITED_QUIRKS_PREFIXES: &[&str] = &[
    "-//W3C//DTD XHTML 1.0 Frameset//",
    "-//W3C//DTD XHTML 1.0 Transitional//",
];

pub fn classify(spec: &DoctypeSpec) -> DocumentMode {
    let public_lower = spec.public_id.to_ascii_lowercase();
    if public_lower.is_empty() && spec.system_id.is_empty() && spec.name.eq_ignore_ascii_case("html") {
        return DocumentMode::Standards;
    }
    if QUIRKS_PUBLIC_PREFIXES.iter().any(|p| public_lower.starts_with(&p.to_ascii_lowercase())) {
        return DocumentMode::Quirks;
    }
    if QUIRKS_PUBLIC_IDS.iter().any(|p| public_lower == p.to_ascii_lowercase()) {
        return DocumentMode::Quirks;
    }
    if LIMITED_QUIRKS_PREFIXES.iter().any(|p| public_lower.starts_with(&p.to_ascii_lowercase())) {
        return DocumentMode::LimitedQuirks;
    }
    // System ID "about:legacy-compat" or other valid -> standards.
    DocumentMode::Standards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html5_doctype_standards() {
        let d = DoctypeSpec { name: "html".into(), ..Default::default() };
        assert_eq!(classify(&d), DocumentMode::Standards);
    }

    #[test]
    fn html_4_loose_quirks() {
        let d = DoctypeSpec {
            name: "HTML".into(),
            public_id: "-//IETF//DTD HTML//EN".into(),
            system_id: "".into(),
        };
        assert_eq!(classify(&d), DocumentMode::Quirks);
    }

    #[test]
    fn xhtml_frameset_limited_quirks() {
        let d = DoctypeSpec {
            name: "html".into(),
            public_id: "-//W3C//DTD XHTML 1.0 Frameset//EN".into(),
            system_id: "".into(),
        };
        assert_eq!(classify(&d), DocumentMode::LimitedQuirks);
    }

    #[test]
    fn empty_doctype_standards() {
        let d = DoctypeSpec::default();
        // Empty name -> standards by default (no quirks-prefixed public id)
        assert_eq!(classify(&d), DocumentMode::Standards);
    }
}
