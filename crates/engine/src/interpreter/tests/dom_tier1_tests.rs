/// Tier 1 DOM API - kompletni standard DOM funkcionalita.
///
/// Pokryva: element.style real, getBoundingClientRect, getComputedStyle,
/// offsetWidth/Height/Top/Left, matches/closest, contains, Event constructors,
/// window.addEventListener.

use super::helpers::*;
use crate::interpreter::JsValue;

// ─── Item 1: element.style real ─────────────────────────────────────────

#[test]
fn style_direct_set_persists() {
    // Klasicke MDN chovani: el.style.display = 'none' meni inline style.
    let v = run(r#"
        const el = document.createElement("div");
        el.style.display = 'none';
        return el.getAttribute("style");
    "#);
    let s = as_str(v);
    assert!(s.contains("display") && s.contains("none"), "ocekavano 'display: none', dostal: {s:?}");
}

#[test]
fn style_get_returns_cached_object() {
    // 2x sahnuti na el.style musi vratit stejny objekt (== reference).
    let v = run(r#"
        const el = document.createElement("div");
        const s1 = el.style;
        const s2 = el.style;
        return s1 === s2;
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn style_setter_camel_to_kebab() {
    // el.style.backgroundColor = 'red' -> attr "background-color: red".
    let v = run(r#"
        const el = document.createElement("div");
        el.style.backgroundColor = 'red';
        return el.getAttribute("style");
    "#);
    let s = as_str(v);
    assert!(s.contains("background-color") && s.contains("red"),
        "ocekavano 'background-color: red', dostal: {s:?}");
}

#[test]
fn style_get_after_setattribute() {
    // Pokud nekdo nastavi setAttribute("style", ...), el.style.x musi vratit nove hodnoty.
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("style", "color: blue; font-size: 14px");
        return el.style.color + "|" + el.style.fontSize;
    "#);
    assert_eq!(as_str(v), "blue|14px");
}

#[test]
fn style_set_property_method() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style.setProperty("color", "green");
        return el.style.getPropertyValue("color");
    "#);
    assert_eq!(as_str(v), "green");
}

// ─── Item 2: element.getBoundingClientRect() ─────────────────────────────

#[test]
fn bounding_rect_default_zero() {
    // Bez layout_lookup vrati vse 0.
    let v = run(r#"
        const el = document.createElement("div");
        const r = el.getBoundingClientRect();
        return r.x + ":" + r.y + ":" + r.width + ":" + r.height;
    "#);
    assert_eq!(as_str(v), "0:0:0:0");
}

#[test]
fn bounding_rect_has_all_keys() {
    let v = run(r#"
        const el = document.createElement("div");
        const r = el.getBoundingClientRect();
        return [r.x, r.y, r.width, r.height, r.top, r.left, r.right, r.bottom].join(",");
    "#);
    assert_eq!(as_str(v), "0,0,0,0,0,0,0,0");
}

#[test]
fn bounding_rect_with_layout_lookup() {
    use crate::interpreter::Interpreter;
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;

    let src = r#"
        const el = document.createElement("div");
        document.body.appendChild(el);
        const r = el.getBoundingClientRect();
        return r.x + ":" + r.y + ":" + r.width + ":" + r.height;
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
    // Mock layout_lookup ktery vraci pevny rect pro vsechny nody.
    interp.set_layout_lookup(|_node_ptr| Some((10.0, 20.0, 100.0, 50.0)));
    let v = interp.run(&program).unwrap();
    assert_eq!(as_str(v), "10:20:100:50");
}

// ─── Item 3: window.getComputedStyle(el) ─────────────────────────────────

#[test]
fn computed_style_fallback_to_inline() {
    // Bez cascade_lookup vraci hodnoty z inline style atributu.
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("style", "color: red; font-size: 14px");
        const cs = window.getComputedStyle(el);
        return cs.color + "|" + cs.fontSize;
    "#);
    assert_eq!(as_str(v), "red|14px");
}

#[test]
fn computed_style_get_property_value() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("style", "margin-top: 10px");
        const cs = window.getComputedStyle(el);
        return cs.getPropertyValue("margin-top");
    "#);
    assert_eq!(as_str(v), "10px");
}

#[test]
fn computed_style_with_cascade_lookup() {
    use crate::interpreter::Interpreter;
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;
    use std::collections::HashMap;

    let src = r#"
        const el = document.createElement("div");
        const cs = window.getComputedStyle(el);
        return cs.color + "|" + cs.getPropertyValue("background-color");
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
    interp.set_cascade_lookup(|_| {
        let mut m = HashMap::new();
        m.insert("color".to_string(), "rgb(0, 0, 0)".to_string());
        m.insert("background-color".to_string(), "rgb(255, 255, 255)".to_string());
        m
    });
    let v = interp.run(&program).unwrap();
    assert_eq!(as_str(v), "rgb(0, 0, 0)|rgb(255, 255, 255)");
}

#[test]
fn match_media_returns_object() {
    let v = run(r#"
        const mm = window.matchMedia("(max-width: 600px)");
        return mm.media;
    "#);
    assert_eq!(as_str(v), "(max-width: 600px)");
}

#[test]
fn get_client_rects_returns_array() {
    let v = run(r#"
        const el = document.createElement("div");
        const rects = el.getClientRects();
        return Array.isArray(rects) ? rects.length : -1;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn style_remove_property() {
    let v = run(r#"
        const el = document.createElement("div");
        el.style.color = 'red';
        el.style.removeProperty("color");
        return el.getAttribute("style") || "";
    "#);
    // Po remove muze byt prazdne nebo neobsahuje color
    let s = as_str(v);
    assert!(!s.contains("color"), "style attr by nemel obsahovat color: {s:?}");
}
