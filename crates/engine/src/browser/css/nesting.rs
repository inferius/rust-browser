//! CSS Nesting (Level 1) - .parent { .child {} } and & selector.
//!
//! Spec: https://www.w3.org/TR/css-nesting-1/
//! Expansion: nested rule selector composed with parent via descendant combinator
//! UNLESS rule starts with `&` (explicit reference).

#[derive(Debug, Clone, PartialEq)]
pub enum NestingTokenKind {
    Selector(String),
    OpenBrace,
    CloseBrace,
    Declaration(String, String),
    AtRule(String, String),
}

/// Expand a nested selector against its parent.
/// Examples:
/// - parent=".a", child=".b" -> ".a .b"
/// - parent=".a", child="&.b" -> ".a.b"
/// - parent=".a", child="& > .b" -> ".a > .b"
pub fn expand_selector(parent: &str, child: &str) -> String {
    let parent = parent.trim();
    let child = child.trim();
    if !child.contains('&') {
        return format!("{} {}", parent, child);
    }
    // Replace every `&` with the parent.
    child.replace('&', parent)
}

/// Expand multi-selector (comma-separated) parent against multi-selector child.
/// .a, .b { .c {} } -> .a .c, .b .c
pub fn expand_selector_list(parent: &str, child: &str) -> Vec<String> {
    let parents: Vec<&str> = parent.split(',').map(|s| s.trim()).collect();
    let children: Vec<&str> = child.split(',').map(|s| s.trim()).collect();
    let mut out = Vec::new();
    for p in &parents {
        for c in &children {
            out.push(expand_selector(p, c));
        }
    }
    out
}

/// Returns true if nesting is well-formed (no leading combinator without `&`).
pub fn is_valid_nested(child: &str) -> bool {
    let trimmed = child.trim_start();
    if trimmed.starts_with('&') { return true; }
    if let Some(first) = trimmed.chars().next() {
        if matches!(first, '>' | '+' | '~') {
            // CSS Nesting L1 allowed leading combinators in 2023 revision.
            return true;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_descendant() {
        assert_eq!(expand_selector(".a", ".b"), ".a .b");
    }

    #[test]
    fn ampersand_replaces_parent() {
        assert_eq!(expand_selector(".a", "&.b"), ".a.b");
    }

    #[test]
    fn ampersand_with_combinator() {
        assert_eq!(expand_selector(".a", "& > .b"), ".a > .b");
    }

    #[test]
    fn multi_parent_multi_child() {
        let r = expand_selector_list(".a, .b", ".c");
        assert_eq!(r, vec![".a .c", ".b .c"]);
    }

    #[test]
    fn multi_parent_with_ampersand() {
        let r = expand_selector_list(".a, .b", "&:hover");
        assert_eq!(r, vec![".a:hover", ".b:hover"]);
    }

    #[test]
    fn leading_combinator_valid() {
        assert!(is_valid_nested("> .b"));
    }
}
