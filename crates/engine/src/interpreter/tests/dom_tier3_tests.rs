/// Tier 3 DOM API - scroll + focus + view.
///
/// Pokryva:
/// - element.scrollIntoView(options)
/// - window.scrollTo / scrollBy / scroll
/// - window.pageXOffset / pageYOffset / scrollX / scrollY
/// - element.focus() / element.blur() real impl (uz s focus/blur eventy)

use super::helpers::*;
use crate::interpreter::{Interpreter, JsValue};
use crate::lexer::base::Lexer;
use crate::parser::Parser;
use crate::tokens::TokenKind;

fn run_with_interp_setup<F>(src: &str, setup: F) -> JsValue
where F: FnOnce(&mut Interpreter)
{
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    setup(&mut interp);
    interp.run(&program).unwrap()
}

// --- Item 1: scrollIntoView ---------------------------------------------

#[test]
fn scroll_into_view_default_no_layout_noop() {
    // Bez layout_lookup je rect nedostupny -> scroll_pos zustane (0,0).
    let v = run(r#"
        const el = document.createElement("div");
        el.scrollIntoView();
        return window.pageYOffset + ":" + window.pageXOffset;
    "#);
    assert_eq!(as_str(v), "0:0");
}

#[test]
fn scroll_into_view_moves_scroll_to_element() {
    let v = run_with_interp_setup(r#"
        const el = document.createElement("div");
        document.body.appendChild(el);
        el.scrollIntoView();
        return window.pageYOffset + ":" + window.pageXOffset;
    "#, |interp| {
        interp.set_layout_lookup(|_ptr| Some((50.0, 500.0, 100.0, 80.0)));
    });
    // block=start (default) -> scroll = (50, 500)
    assert_eq!(as_str(v), "500:50");
}

#[test]
fn scroll_into_view_block_end() {
    let v = run_with_interp_setup(r#"
        const el = document.createElement("div");
        el.scrollIntoView({ block: "end" });
        return window.pageYOffset;
    "#, |interp| {
        interp.set_layout_lookup(|_ptr| Some((0.0, 800.0, 100.0, 80.0)));
    });
    // block=end -> target_y = y + h - 600 = 800 + 80 - 600 = 280
    assert_eq!(as_num(v), 280.0);
}

// --- Item 2: window.scrollTo / scrollBy ---------------------------------

#[test]
fn window_scroll_to_xy() {
    let v = run(r#"
        window.scrollTo(50, 100);
        return window.pageXOffset + ":" + window.pageYOffset;
    "#);
    assert_eq!(as_str(v), "50:100");
}

#[test]
fn window_scroll_to_options_obj() {
    let v = run(r#"
        window.scrollTo({ left: 40, top: 60, behavior: "smooth" });
        return window.scrollX + ":" + window.scrollY;
    "#);
    assert_eq!(as_str(v), "40:60");
}

#[test]
fn window_scroll_by_increments() {
    let v = run(r#"
        window.scrollTo(10, 20);
        window.scrollBy(5, 30);
        return window.pageXOffset + ":" + window.pageYOffset;
    "#);
    assert_eq!(as_str(v), "15:50");
}

#[test]
fn window_scroll_by_options_obj() {
    let v = run(r#"
        window.scrollTo(0, 100);
        window.scrollBy({ left: 25, top: 15 });
        return window.pageXOffset + ":" + window.pageYOffset;
    "#);
    assert_eq!(as_str(v), "25:115");
}

#[test]
fn window_scroll_alias() {
    let v = run(r#"
        window.scrollTo(0, 0);
        window.scroll(7, 9);
        return window.scrollX + ":" + window.scrollY;
    "#);
    assert_eq!(as_str(v), "7:9");
}

#[test]
fn window_scroll_clamps_negative_to_zero() {
    let v = run(r#"
        window.scrollTo(-50, -10);
        return window.pageXOffset + ":" + window.pageYOffset;
    "#);
    assert_eq!(as_str(v), "0:0");
}

// --- Item 3: element.focus() / blur() real impl --------------------------

#[test]
fn focus_dispatches_focus_event() {
    // Po focus() -> listener "focus" musi byt zavolan.
    let v = run(r#"
        let fired = [];
        const inp = document.createElement("input");
        inp.addEventListener("focus", (e) => { fired.push("focus:" + e.type); });
        inp.focus();
        return fired.length + ":" + (fired[0] || "");
    "#);
    assert_eq!(as_str(v), "1:focus:focus");
}

#[test]
fn blur_dispatches_blur_event() {
    let v = run(r#"
        let fired = [];
        const inp = document.createElement("input");
        inp.addEventListener("blur", (e) => { fired.push("blur"); });
        inp.focus();
        inp.blur();
        return fired.length + ":" + (fired[0] || "");
    "#);
    assert_eq!(as_str(v), "1:blur");
}

#[test]
fn focus_blur_active_element_updates() {
    let v = run(r#"
        const a = document.createElement("input");
        const b = document.createElement("input");
        document.body.appendChild(a);
        document.body.appendChild(b);
        a.focus();
        const t1 = document.activeElement === a;
        b.focus();
        const t2 = document.activeElement === b;
        b.blur();
        const t3 = document.activeElement.tagName;
        return t1 + ":" + t2 + ":" + t3;
    "#);
    // a focused -> true, b focused -> true, b blur -> activeElement = BODY
    assert_eq!(as_str(v), "true:true:BODY");
}

#[test]
fn focus_does_not_fire_when_already_focused() {
    // Spec: focus() na already-focused element -> NEvyvolat dalsi event.
    let v = run(r#"
        let count = 0;
        const inp = document.createElement("input");
        inp.addEventListener("focus", () => { count++; });
        inp.focus();
        inp.focus();
        return count;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn blur_only_fires_when_currently_focused() {
    let v = run(r#"
        let count = 0;
        const inp = document.createElement("input");
        inp.addEventListener("blur", () => { count++; });
        inp.blur();
        inp.blur();
        return count;
    "#);
    assert_eq!(as_num(v), 0.0);
}
