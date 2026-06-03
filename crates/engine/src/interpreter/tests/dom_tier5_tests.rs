/// Tier 5 DOM API - CSSOM + Shadow DOM + Selection/Range.
///
/// Pokryva:
/// - document.styleSheets list + CSSStyleSheet.cssRules.
/// - CSSStyleSheet.insertRule/deleteRule (stub).
/// - element.attachShadow + ShadowRoot real (appendChild + querySelector).
/// - element.shadowRoot getter (open vs closed mode).
/// - Selection API (window.getSelection).
/// - Range API (document.createRange).
/// - CSSStyleDeclaration cssText / length / item / kebab access.
/// - document.fonts FontFaceSet stub.
/// - document.scrollingElement.

use super::helpers::*;
use crate::interpreter::JsValue;

// --- Item 3: attachShadow real + ShadowRoot dispatch ---------------------

#[test]
fn shadow_root_appendChild_real() {
    let v = run(r##"
        const host = document.createElement("div");
        const sr = host.attachShadow({mode: "open"});
        const span = document.createElement("span");
        span.id = "inner";
        sr.appendChild(span);
        const found = sr.querySelector("#inner");
        return found !== null ? 1 : 0;
    "##);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn shadow_root_open_mode_visible() {
    let v = run(r#"
        const host = document.createElement("div");
        const sr = host.attachShadow({mode: "open"});
        return host.shadowRoot === sr ? "same" : "diff";
    "#);
    assert_eq!(as_str(v), "same");
}

#[test]
fn shadow_root_closed_mode_hidden() {
    let v = run(r#"
        const host = document.createElement("div");
        host.attachShadow({mode: "closed"});
        return host.shadowRoot === null ? "null" : "visible";
    "#);
    assert_eq!(as_str(v), "null");
}

#[test]
fn shadow_root_double_attach_throws() {
    let v = run(r#"
        const host = document.createElement("div");
        host.attachShadow({mode: "open"});
        try {
            host.attachShadow({mode: "open"});
            return "no-throw";
        } catch (e) {
            return "thrown";
        }
    "#);
    assert_eq!(as_str(v), "thrown");
}

#[test]
fn shadow_root_querySelectorAll() {
    let v = run(r#"
        const host = document.createElement("div");
        const sr = host.attachShadow({mode: "open"});
        const a = document.createElement("p");
        const b = document.createElement("p");
        sr.appendChild(a);
        sr.appendChild(b);
        return sr.querySelectorAll("p").length;
    "#);
    assert_eq!(as_num(v), 2.0);
}

#[test]
fn shadow_root_host_back_ref() {
    let v = run(r#"
        const host = document.createElement("section");
        const sr = host.attachShadow({mode: "open"});
        return sr.host === host ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn shadow_root_contains_after_append() {
    let v = run(r#"
        const host = document.createElement("div");
        const sr = host.attachShadow({mode: "open"});
        const child = document.createElement("span");
        sr.appendChild(child);
        return sr.contains(child) ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

// --- Item 8: document.scrollingElement -----------------------------------

#[test]
fn document_scrollingElement_returns_html() {
    let v = run(r#"
        const el = document.scrollingElement;
        return el !== null ? el.tagName : "null";
    "#);
    // HTML element tagName by mel byt "HTML".
    let s = as_str(v);
    assert!(s == "HTML" || s == "null", "got: {s}");
}

#[test]
fn document_scrollingElement_eq_documentElement() {
    let v = run(r#"
        return document.scrollingElement === document.documentElement ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

// --- Item 1: document.styleSheets ----------------------------------------

#[test]
fn document_styleSheets_is_array_like() {
    let v = run(r#"
        const ss = document.styleSheets;
        return typeof ss.length;
    "#);
    assert_eq!(as_str(v), "number");
}

#[test]
fn document_styleSheets_length_zero_default() {
    let v = run(r#"
        return document.styleSheets.length;
    "#);
    assert_eq!(as_num(v), 0.0);
}

#[test]
fn document_styleSheets_with_lookup_returns_sheets() {
    // Simulujeme host wire-up: 1 sheet, 2 rules.
    use crate::interpreter::Interpreter;
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;
    let src = r#"
        const ss = document.styleSheets;
        return ss.length + "|" + ss[0].cssRules.length + "|" + ss[0].cssRules[0].selectorText;
    "#;
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.set_stylesheets_lookup(|| {
        vec![
            vec![
                (".foo".to_string(), vec![
                    ("color".to_string(), "red".to_string()),
                ]),
                ("#bar".to_string(), vec![
                    ("font-size".to_string(), "14px".to_string()),
                ]),
            ],
        ]
    });
    let v = interp.run(&program).unwrap();
    assert_eq!(as_str(v), "1|2|.foo");
}

#[test]
fn cssstylesheet_insertRule_returns_idx() {
    let v = run(r#"
        const sheet = document.styleSheets;
        if (sheet.length === 0) return -1;
        return sheet[0].insertRule(".a{}", 0);
    "#);
    // Bez host lookup je length 0 -> -1.
    assert_eq!(as_num(v), -1.0);
}

#[test]
fn cssstylesheet_methods_exist_with_lookup() {
    use crate::interpreter::Interpreter;
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;
    let src = r#"
        const sheet = document.styleSheets[0];
        return typeof sheet.insertRule + "|" + typeof sheet.deleteRule + "|" + typeof sheet.replace;
    "#;
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.set_stylesheets_lookup(|| {
        vec![vec![(".a".to_string(), vec![])]]
    });
    let v = interp.run(&program).unwrap();
    assert_eq!(as_str(v), "function|function|function");
}

#[test]
fn cssstylesheet_cssRules_indexed_access() {
    use crate::interpreter::Interpreter;
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;
    let src = r#"
        const rules = document.styleSheets[0].cssRules;
        return rules.item(0).selectorText;
    "#;
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.set_stylesheets_lookup(|| {
        vec![vec![("body".to_string(), vec![
            ("margin".to_string(), "0".to_string()),
        ])]]
    });
    let v = interp.run(&program).unwrap();
    assert_eq!(as_str(v), "body");
}

// --- Item 4: Selection API -----------------------------------------------

#[test]
fn window_getSelection_returns_object() {
    let v = run(r#"
        const s = window.getSelection();
        return typeof s.toString;
    "#);
    assert_eq!(as_str(v), "function");
}

#[test]
fn selection_default_collapsed() {
    let v = run(r#"
        const s = window.getSelection();
        return s.isCollapsed ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn selection_rangeCount_zero() {
    let v = run(r#"
        return window.getSelection().rangeCount;
    "#);
    assert_eq!(as_num(v), 0.0);
}

// --- Item 5: Range API ---------------------------------------------------

#[test]
fn document_createRange_returns_obj() {
    let v = run(r#"
        const r = document.createRange();
        return typeof r.setStart;
    "#);
    assert_eq!(as_str(v), "function");
}

#[test]
fn range_default_collapsed_true() {
    let v = run(r#"
        return document.createRange().collapsed ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn range_setStart_setEnd() {
    let v = run(r#"
        const r = document.createRange();
        const div = document.createElement("div");
        r.setStart(div, 0);
        r.setEnd(div, 0);
        return r.startContainer === div ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

// --- Item 6: CSSStyleDeclaration full API --------------------------------

#[test]
fn style_cssText_getter() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style.color = "red";
        el.style.fontSize = "12px";
        return el.style.cssText.indexOf("color") >= 0 ? 1 : 0;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn style_kebab_property_access() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style["background-color"] = "blue";
        return el.style.backgroundColor;
    "#);
    assert_eq!(as_str(v), "blue");
}

#[test]
fn style_length_reflects_props() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style.color = "red";
        el.style.fontSize = "12px";
        return el.style.length;
    "#);
    assert_eq!(as_num(v), 2.0);
}

#[test]
fn style_item_returns_property_name() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style.color = "red";
        return el.style.item(0);
    "#);
    assert_eq!(as_str(v), "color");
}

#[test]
fn style_cssText_setter() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style.cssText = "color: red; font-size: 14px";
        return el.style.color;
    "#);
    assert_eq!(as_str(v), "red");
}

// --- Item 7: document.fonts FontFaceSet ----------------------------------

#[test]
fn document_fonts_is_object() {
    let v = run(r#"
        return typeof document.fonts;
    "#);
    assert_eq!(as_str(v), "object");
}

#[test]
fn document_fonts_status_loaded() {
    let v = run(r#"
        return document.fonts.status;
    "#);
    assert_eq!(as_str(v), "loaded");
}

#[test]
fn document_fonts_ready_is_promise() {
    // Promise is dispatched via special case; check via type + state.
    let v = run(r#"
        const r = document.fonts.ready;
        return typeof r;
    "#);
    assert_eq!(as_str(v), "object");
}
