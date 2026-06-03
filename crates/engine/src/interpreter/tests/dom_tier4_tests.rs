/// Tier 4 DOM API - observers + DOMRect.toJSON + DOMTokenList rozsireni.
///
/// Pokryva:
/// - MutationObserver childList + attributes + subtree (real dispatch).
/// - IntersectionObserver stub-level (observe / unobserve / disconnect).
/// - ResizeObserver stub-level.
/// - DOMRect toJSON() return all 8 fields.
/// - DOMTokenList: length, item(i), [n], Symbol.iterator, replace, value get/set.

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

// --- Item 4: MutationObserver real dispatch ------------------------------

#[test]
fn mutation_observer_appendChild_fires_records() {
    let v = run(r#"
        let total = 0;
        const parent = document.createElement("div");
        const obs = new MutationObserver((records) => {
            for (const r of records) {
                if (r.type === "childList") total += 1;
            }
        });
        obs.observe(parent, { childList: true });
        const a = document.createElement("span");
        const b = document.createElement("p");
        parent.appendChild(a);
        parent.appendChild(b);
        return total;
    "#);
    assert_eq!(as_num(v), 2.0);
}

#[test]
fn mutation_observer_setAttribute_fires_records() {
    let v = run(r#"
        let names = [];
        const el = document.createElement("div");
        const obs = new MutationObserver((records) => {
            for (const r of records) names.push(r.attributeName);
        });
        obs.observe(el, { attributes: true });
        el.setAttribute("data-a", "1");
        el.setAttribute("data-b", "2");
        return names.join(",");
    "#);
    assert_eq!(as_str(v), "data-a,data-b");
}

#[test]
fn mutation_observer_removeAttribute_fires() {
    let v = run(r#"
        let count = 0;
        const el = document.createElement("div");
        el.setAttribute("x", "1");
        const obs = new MutationObserver(() => { count++; });
        obs.observe(el, { attributes: true });
        el.removeAttribute("x");
        return count;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn mutation_observer_subtree_picks_descendants() {
    let v = run(r#"
        let count = 0;
        const root = document.createElement("div");
        const child = document.createElement("span");
        root.appendChild(child);
        const obs = new MutationObserver(() => { count++; });
        obs.observe(root, { attributes: true, subtree: true });
        child.setAttribute("foo", "bar");
        return count;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn mutation_observer_oldValue_preserved() {
    let v = run(r#"
        let oldVal = "";
        const el = document.createElement("div");
        el.setAttribute("x", "old");
        const obs = new MutationObserver((records) => {
            oldVal = records[0].oldValue;
        });
        obs.observe(el, { attributes: true });
        el.setAttribute("x", "new");
        return oldVal;
    "#);
    assert_eq!(as_str(v), "old");
}

// --- Item 5: IntersectionObserver stub-level ----------------------------

#[test]
fn intersection_observer_constructs() {
    let v = run(r#"
        const io = new IntersectionObserver(() => {});
        return typeof io;
    "#);
    assert_eq!(as_str(v), "object");
}

#[test]
fn intersection_observer_observe_no_throw() {
    let v = run(r#"
        const io = new IntersectionObserver(() => {});
        const el = document.createElement("div");
        io.observe(el);
        return "ok";
    "#);
    assert_eq!(as_str(v), "ok");
}

#[test]
fn intersection_observer_options_read() {
    let v = run(r#"
        const io = new IntersectionObserver(() => {}, { rootMargin: "10px" });
        return io.rootMargin;
    "#);
    assert_eq!(as_str(v), "10px");
}

#[test]
fn intersection_observer_disconnect_no_throw() {
    let v = run(r#"
        const io = new IntersectionObserver(() => {});
        const el = document.createElement("div");
        io.observe(el);
        io.unobserve(el);
        io.disconnect();
        return "ok";
    "#);
    assert_eq!(as_str(v), "ok");
}

#[test]
fn intersection_observer_take_records_array() {
    let v = run(r#"
        const io = new IntersectionObserver(() => {});
        return Array.isArray(io.takeRecords());
    "#);
    assert_eq!(as_bool(v), true);
}

// --- Item 6: ResizeObserver stub-level ----------------------------------

#[test]
fn resize_observer_constructs() {
    let v = run(r#"
        const ro = new ResizeObserver(() => {});
        return typeof ro;
    "#);
    assert_eq!(as_str(v), "object");
}

#[test]
fn resize_observer_observe_no_throw() {
    let v = run(r#"
        const ro = new ResizeObserver(() => {});
        const el = document.createElement("div");
        ro.observe(el);
        ro.unobserve(el);
        ro.disconnect();
        return "ok";
    "#);
    assert_eq!(as_str(v), "ok");
}

// --- Item 7: DOMRect.toJSON() ------------------------------------------

#[test]
fn dom_rect_to_json_returns_all_fields() {
    let v = run_with_interp_setup(r#"
        const el = document.createElement("div");
        document.body.appendChild(el);
        const r = el.getBoundingClientRect();
        const j = r.toJSON();
        return j.x + ":" + j.y + ":" + j.width + ":" + j.height +
               ":" + j.top + ":" + j.left + ":" + j.right + ":" + j.bottom;
    "#, |interp| {
        interp.set_layout_lookup(|_| Some((10.0, 20.0, 100.0, 50.0)));
    });
    // x=10, y=20, w=100, h=50, top=y=20, left=x=10, right=x+w=110, bottom=y+h=70
    assert_eq!(as_str(v), "10:20:100:50:20:10:110:70");
}

#[test]
fn dom_rect_to_json_default_zero() {
    let v = run(r#"
        const el = document.createElement("div");
        const r = el.getBoundingClientRect();
        const j = r.toJSON();
        return j.x + ":" + j.width;
    "#);
    assert_eq!(as_str(v), "0:0");
}

#[test]
fn dom_rect_to_json_serializable() {
    let v = run_with_interp_setup(r#"
        const el = document.createElement("div");
        document.body.appendChild(el);
        const r = el.getBoundingClientRect();
        // JSON.stringify zavola toJSON metodu implicitly. Vystup zalezi
        // na implementaci stringify, ale `toJSON()` jako primy callable
        // musi vratit object s 8 fieldy. Testujeme jen volani primo.
        const j = r.toJSON();
        return j.width;
    "#, |interp| {
        interp.set_layout_lookup(|_| Some((0.0, 0.0, 200.0, 80.0)));
    });
    assert_eq!(as_num(v), 200.0);
}

// --- Item 8: DOMTokenList rozsireni ------------------------------------

#[test]
fn class_list_length() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "foo bar baz");
        return el.classList.length;
    "#);
    assert_eq!(as_num(v), 3.0);
}

#[test]
fn class_list_length_empty() {
    let v = run(r#"
        const el = document.createElement("div");
        return el.classList.length;
    "#);
    assert_eq!(as_num(v), 0.0);
}

#[test]
fn class_list_item_returns_token() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "a b c");
        return el.classList.item(0) + ":" + el.classList.item(1) + ":" + el.classList.item(2);
    "#);
    assert_eq!(as_str(v), "a:b:c");
}

#[test]
fn class_list_item_out_of_range_returns_null() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "x");
        return el.classList.item(5);
    "#);
    assert!(matches!(v, JsValue::Null));
}

#[test]
fn class_list_indexed_access() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "foo bar");
        return el.classList[0] + ":" + el.classList[1];
    "#);
    assert_eq!(as_str(v), "foo:bar");
}

#[test]
fn class_list_value_getter() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "alpha beta");
        return el.classList.value;
    "#);
    assert_eq!(as_str(v), "alpha beta");
}

#[test]
fn class_list_value_setter() {
    let v = run(r#"
        const el = document.createElement("div");
        el.classList.value = "new tokens";
        return el.getAttribute("class");
    "#);
    assert_eq!(as_str(v), "new tokens");
}

#[test]
fn class_list_replace_existing() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "old middle end");
        const changed = el.classList.replace("middle", "MID");
        return changed + ":" + el.getAttribute("class");
    "#);
    assert_eq!(as_str(v), "true:old MID end");
}

#[test]
fn class_list_replace_nonexisting_returns_false() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "a b");
        const changed = el.classList.replace("nope", "x");
        return changed + ":" + el.getAttribute("class");
    "#);
    assert_eq!(as_str(v), "false:a b");
}

#[test]
fn class_list_iterator_via_array_from() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "foo bar baz");
        const arr = Array.from(el.classList);
        return arr.length + ":" + arr.join(",");
    "#);
    assert_eq!(as_str(v), "3:foo,bar,baz");
}

#[test]
fn class_list_for_of_iteration() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("class", "x y z");
        let out = [];
        for (const t of el.classList) out.push(t);
        return out.join("|");
    "#);
    assert_eq!(as_str(v), "x|y|z");
}
