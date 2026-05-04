/// Testy CSS parseru.

use crate::browser::css_parser::*;

#[test]
fn parse_simple_rule() {
    let s = parse_stylesheet("body { color: red; }");
    assert_eq!(s.rules.len(), 1);
    assert_eq!(s.rules[0].selectors.len(), 1);
    assert_eq!(s.rules[0].declarations.len(), 1);
    assert_eq!(s.rules[0].declarations[0].property, "color");
}

#[test]
fn parse_multiple_rules() {
    let s = parse_stylesheet(r#"
        body { background: white; }
        h1 { color: blue; font-size: 24px; }
        p { margin: 10px; }
    "#);
    assert_eq!(s.rules.len(), 3);
}

#[test]
fn parse_class_selector() {
    let s = parse_stylesheet(".container { width: 100px; }");
    assert_eq!(s.rules[0].selectors[0].parts[0].classes, vec!["container"]);
}

#[test]
fn parse_id_selector() {
    let s = parse_stylesheet("#main { padding: 5px; }");
    assert_eq!(s.rules[0].selectors[0].parts[0].id.as_deref(), Some("main"));
}

#[test]
fn parse_combined_selector() {
    let s = parse_stylesheet("div.box#unique { color: red; }");
    let part = &s.rules[0].selectors[0].parts[0];
    assert_eq!(part.tag.as_deref(), Some("div"));
    assert_eq!(part.id.as_deref(), Some("unique"));
    assert_eq!(part.classes, vec!["box"]);
}

#[test]
fn parse_descendant_selector() {
    let s = parse_stylesheet("div p { margin: 0; }");
    assert_eq!(s.rules[0].selectors[0].parts.len(), 2);
}

#[test]
fn parse_multiple_selectors() {
    let s = parse_stylesheet("h1, h2, h3 { font-weight: bold; }");
    assert_eq!(s.rules[0].selectors.len(), 3);
}

#[test]
fn parse_important_declaration() {
    let s = parse_stylesheet("p { color: red !important; }");
    assert_eq!(s.rules[0].declarations[0].important, true);
}

#[test]
fn specificity_levels() {
    use crate::browser::css_parser::specificity;
    let s_id = parse_stylesheet("#a { x: 1; }");
    assert_eq!(specificity(&s_id.rules[0].selectors[0]), (1, 0, 0));
    let s_cls = parse_stylesheet(".a { x: 1; }");
    assert_eq!(specificity(&s_cls.rules[0].selectors[0]), (0, 1, 0));
    let s_tag = parse_stylesheet("div { x: 1; }");
    assert_eq!(specificity(&s_tag.rules[0].selectors[0]), (0, 0, 1));
}
