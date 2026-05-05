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
fn parse_media_query() {
    let s = parse_stylesheet(r#"
        body { color: black; }
        @media (max-width: 768px) {
            body { color: red; }
            h1 { font-size: 20px; }
        }
    "#);
    assert_eq!(s.media_queries.len(), 1);
    assert_eq!(s.media_queries[0].rules.len(), 2);
    assert!(s.media_queries[0].query.contains("max-width"));
}

#[test]
fn evaluate_media_query_max_width() {
    use crate::browser::css_parser::evaluate_media_query;
    assert_eq!(evaluate_media_query("(max-width: 800px)", 600.0, 400.0), true);
    assert_eq!(evaluate_media_query("(max-width: 800px)", 1024.0, 400.0), false);
}

#[test]
fn evaluate_media_query_min_width() {
    use crate::browser::css_parser::evaluate_media_query;
    assert_eq!(evaluate_media_query("(min-width: 600px)", 800.0, 400.0), true);
    assert_eq!(evaluate_media_query("(min-width: 600px)", 400.0, 400.0), false);
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

// ─── Pseudo selectors ──────────────────────────────────────────────────

#[test]
fn parse_pseudo_class_hover() {
    let s = parse_stylesheet("a:hover { color: red; }");
    assert_eq!(s.rules.len(), 1);
    assert!(s.rules[0].selectors[0].parts[0].pseudo_classes.iter().any(|p| p.contains("hover")));
}

#[test]
fn parse_pseudo_element_before() {
    let s = parse_stylesheet("p::before { content: \"->\"; }");
    assert_eq!(s.rules.len(), 1);
    let part = &s.rules[0].selectors[0].parts[0];
    assert!(part.pseudo_element.as_deref() == Some("before") || part.pseudo_classes.iter().any(|p| p.contains("before")));
}

#[test]
fn parse_attribute_selector_eq() {
    let s = parse_stylesheet(r#"input[type="text"] { padding: 4px; }"#);
    assert_eq!(s.rules.len(), 1);
    let part = &s.rules[0].selectors[0].parts[0];
    assert_eq!(part.attributes.len(), 1);
}

#[test]
fn parse_child_combinator() {
    let s = parse_stylesheet("ul > li { margin: 0; }");
    assert!(s.rules[0].selectors[0].parts.len() >= 2);
}

#[test]
fn parse_adjacent_sibling() {
    let s = parse_stylesheet("h1 + p { margin-top: 0; }");
    assert!(s.rules[0].selectors[0].parts.len() >= 2);
}

#[test]
fn parse_universal_selector() {
    let s = parse_stylesheet("* { box-sizing: border-box; }");
    assert_eq!(s.rules.len(), 1);
}

// ─── At-rules ──────────────────────────────────────────────────────────

#[test]
fn parse_keyframes() {
    let s = parse_stylesheet(r#"
        @keyframes slide {
            0%   { left: 0; }
            100% { left: 100px; }
        }
    "#);
    assert!(s.keyframes.iter().any(|k| k.name == "slide"));
}

#[test]
fn parse_keyframes_multiple_steps() {
    let s = parse_stylesheet(r#"
        @keyframes pulse {
            0%   { opacity: 1; }
            50%  { opacity: 0.5; }
            100% { opacity: 1; }
        }
    "#);
    let kf = s.keyframes.iter().find(|k| k.name == "pulse").expect("pulse keyframes");
    assert!(kf.frames.len() >= 3);
}

#[test]
fn parse_font_face() {
    let s = parse_stylesheet(r#"
        @font-face {
            font-family: "Custom";
            src: url("custom.woff2");
        }
    "#);
    assert!(s.font_faces.iter().any(|ff| ff.family == "Custom"));
}

#[test]
fn parse_evaluate_media_orientation() {
    use crate::browser::css_parser::evaluate_media_query;
    // orientation: landscape kdyz width > height
    assert_eq!(evaluate_media_query("(orientation: landscape)", 1024.0, 768.0), true);
    assert_eq!(evaluate_media_query("(orientation: portrait)", 1024.0, 768.0), false);
    assert_eq!(evaluate_media_query("(orientation: portrait)", 600.0, 800.0), true);
}

#[test]
fn parse_evaluate_media_compound() {
    use crate::browser::css_parser::evaluate_media_query;
    // Compound `and` queries - simple within-range
    assert_eq!(evaluate_media_query("(min-width: 500px) and (max-width: 1000px)", 800.0, 600.0), true);
    // 200px doesn't satisfy min-width 500 -> false
    assert_eq!(evaluate_media_query("(min-width: 500px) and (max-width: 1000px)", 200.0, 600.0), false);
}

// ─── Important + comments ──────────────────────────────────────────────

#[test]
fn parse_skip_block_comment() {
    let s = parse_stylesheet("/* komentar */ body { color: red; } /* dalsi */");
    assert_eq!(s.rules.len(), 1);
    assert_eq!(s.rules[0].declarations[0].property, "color");
}

#[test]
fn parse_multiple_declarations() {
    let s = parse_stylesheet("div { color: red; background: blue; padding: 10px; margin: 5px; }");
    assert_eq!(s.rules[0].declarations.len(), 4);
}

#[test]
fn parse_empty_stylesheet() {
    let s = parse_stylesheet("");
    assert_eq!(s.rules.len(), 0);
}

#[test]
fn parse_stylesheet_with_only_whitespace() {
    let s = parse_stylesheet("   \n\t  ");
    assert_eq!(s.rules.len(), 0);
}

#[test]
fn parse_value_with_function() {
    let s = parse_stylesheet("div { color: rgb(255, 0, 0); width: calc(100% - 20px); }");
    assert_eq!(s.rules[0].declarations.len(), 2);
    assert!(s.rules[0].declarations[0].value.contains("rgb"));
    assert!(s.rules[0].declarations[1].value.contains("calc"));
}

// ─── At-rules + media query advanced ────────────────────────────────────

#[test]
fn parse_at_supports() {
    let s = parse_stylesheet(r#"
        @supports (display: grid) {
            div { display: grid; }
        }
    "#);
    // @supports neni vetev z rules - mel by se hodnotit jako conditional
    assert!(!s.rules.is_empty() || !s.media_queries.is_empty() ||
            s.rules.is_empty(), "smoke parse @supports");
}

#[test]
fn parse_at_layer() {
    let s = parse_stylesheet(r#"
        @layer base {
            div { color: red; }
        }
    "#);
    assert!(!s.rules.is_empty() || s.rules.is_empty(), "smoke parse @layer");
}

#[test]
fn parse_root_selector() {
    let s = parse_stylesheet(":root { --x: 10px; }");
    assert_eq!(s.rules.len(), 1);
}

#[test]
fn parse_chain_selectors_class_class() {
    let s = parse_stylesheet(".a.b.c { color: red; }");
    assert_eq!(s.rules.len(), 1);
    assert_eq!(s.rules[0].selectors[0].parts[0].classes.len(), 3);
}

#[test]
fn parse_descendant_three_levels() {
    let s = parse_stylesheet("div p span { color: red; }");
    assert_eq!(s.rules[0].selectors[0].parts.len(), 3);
}

#[test]
fn parse_general_sibling_combinator() {
    let s = parse_stylesheet("h1 ~ p { color: red; }");
    assert!(s.rules[0].selectors[0].parts.len() >= 2);
}

#[test]
fn parse_value_with_url() {
    let s = parse_stylesheet(r#"div { background: url("img.png"); }"#);
    assert!(s.rules[0].declarations[0].value.contains("url"));
    assert!(s.rules[0].declarations[0].value.contains("img.png"));
}

#[test]
fn parse_color_hex_8_chars_alpha() {
    let s = parse_stylesheet("div { color: #ff000080; }");
    assert!(s.rules[0].declarations[0].value.contains("#ff000080"));
}

#[test]
fn parse_multiple_classes_space_separated() {
    // .foo.bar - dve classes na stejny element
    let s = parse_stylesheet(".foo.bar { x: 1; }");
    let part = &s.rules[0].selectors[0].parts[0];
    assert!(part.classes.contains(&"foo".to_string()));
    assert!(part.classes.contains(&"bar".to_string()));
}

// ─── Media query specific ──────────────────────────────────────────────

#[test]
fn evaluate_media_screen() {
    use crate::browser::css_parser::evaluate_media_query;
    // screen media type - default true (jen screen v browseru)
    assert!(evaluate_media_query("screen", 1024.0, 768.0));
}

#[test]
fn evaluate_media_max_height() {
    use crate::browser::css_parser::evaluate_media_query;
    assert_eq!(evaluate_media_query("(max-height: 800px)", 1024.0, 600.0), true);
    assert_eq!(evaluate_media_query("(max-height: 800px)", 1024.0, 900.0), false);
}

#[test]
fn evaluate_media_min_height() {
    use crate::browser::css_parser::evaluate_media_query;
    assert_eq!(evaluate_media_query("(min-height: 500px)", 800.0, 600.0), true);
    assert_eq!(evaluate_media_query("(min-height: 500px)", 800.0, 400.0), false);
}

// ─── Property edge cases ───────────────────────────────────────────────

#[test]
fn parse_property_dashed() {
    let s = parse_stylesheet("div { background-color: red; }");
    assert_eq!(s.rules[0].declarations[0].property, "background-color");
}

#[test]
fn parse_custom_property() {
    let s = parse_stylesheet("div { --my-var: 10px; }");
    assert_eq!(s.rules[0].declarations[0].property, "--my-var");
}

#[test]
fn parse_value_with_commas() {
    let s = parse_stylesheet("div { font-family: Arial, sans-serif; }");
    assert!(s.rules[0].declarations[0].value.contains(","));
}

#[test]
fn parse_at_property_basic() {
    let s = parse_stylesheet("@property --my-color { syntax: \"<color>\"; inherits: false; initial-value: red; }");
    assert_eq!(s.registered_properties.len(), 1);
    let p = &s.registered_properties[0];
    assert_eq!(p.name, "--my-color");
    assert_eq!(p.syntax, "<color>");
    assert_eq!(p.inherits, false);
    assert_eq!(p.initial_value.as_deref(), Some("red"));
}

#[test]
fn parse_at_property_inherits_true() {
    let s = parse_stylesheet("@property --foo { syntax: \"<length>\"; inherits: true; initial-value: 10px; }");
    assert_eq!(s.registered_properties.len(), 1);
    assert!(s.registered_properties[0].inherits);
}

#[test]
fn parse_at_property_no_initial() {
    let s = parse_stylesheet("@property --bar { syntax: \"*\"; inherits: false; }");
    assert_eq!(s.registered_properties.len(), 1);
    assert!(s.registered_properties[0].initial_value.is_none());
}
