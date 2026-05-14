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
