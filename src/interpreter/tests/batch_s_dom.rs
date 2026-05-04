/// Batch S - DOM bridge (document, Element, Event, window).

use super::helpers::*;
use crate::interpreter::JsValue;

// ─── document.createElement ──────────────────────────────────────────────

#[test]
fn create_element() {
    let v = run(r#"
        const el = document.createElement("div");
        return el.tagName;
    "#);
    assert_eq!(as_str(v), "DIV");
}

#[test]
fn create_text_node() {
    let v = run(r#"
        const t = document.createTextNode("hello");
        return t.textContent;
    "#);
    assert_eq!(as_str(v), "hello");
}

// ─── Element attributes ──────────────────────────────────────────────────

#[test]
fn set_get_attribute() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("data-id", "42");
        return el.getAttribute("data-id");
    "#);
    assert_eq!(as_str(v), "42");
}

#[test]
fn get_missing_attribute_returns_null() {
    assert!(matches!(run(r#"
        return document.createElement("div").getAttribute("missing");
    "#), JsValue::Null));
}

#[test]
fn has_attribute() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("foo", "bar");
        return el.hasAttribute("foo") + ":" + el.hasAttribute("baz");
    "#);
    assert_eq!(as_str(v), "true:false");
}

#[test]
fn remove_attribute() {
    let v = run(r#"
        const el = document.createElement("a");
        el.setAttribute("href", "url");
        el.removeAttribute("href");
        return el.hasAttribute("href");
    "#);
    assert_eq!(as_bool(v), false);
}

#[test]
fn set_id_attribute_promotes_to_property() {
    let v = run(r#"
        const el = document.createElement("div");
        el.setAttribute("id", "myid");
        return el.id;
    "#);
    assert_eq!(as_str(v), "myid");
}

// ─── DOM tree manipulace ─────────────────────────────────────────────────

#[test]
fn append_child() {
    let v = run(r#"
        const parent = document.createElement("div");
        const child = document.createElement("span");
        parent.appendChild(child);
        return parent.childNodes.length;
    "#);
    assert_eq!(as_num(v), 1.0);
}

#[test]
fn append_remove_child() {
    let v = run(r#"
        const parent = document.createElement("div");
        const a = document.createElement("p");
        const b = document.createElement("p");
        parent.appendChild(a);
        parent.appendChild(b);
        parent.removeChild(a);
        return parent.childNodes.length;
    "#);
    assert_eq!(as_num(v), 1.0);
}

// ─── Events ──────────────────────────────────────────────────────────────

#[test]
fn add_listener_returns_undefined() {
    // addEventListener je stub (real callback dispatch je TODO)
    let v = run(r#"
        const el = document.createElement("button");
        return typeof el.addEventListener("click", () => {});
    "#);
    assert_eq!(as_str(v), "undefined");
}

#[test]
fn add_listener_dispatch() {
    // Real event dispatch s callback registry
    let v = run(r#"
        const el = document.createElement("button");
        let clicked = false;
        el.addEventListener("click", () => { clicked = true; });
        el.dispatchEvent(new Event("click"));
        return clicked;
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn click_method_fires_listener() {
    // .click() programaticky vyvola listener
    let v = run(r#"
        const el = document.createElement("button");
        let count = 0;
        el.addEventListener("click", () => { count++; });
        el.click();
        el.click();
        el.click();
        return count;
    "#);
    assert_eq!(as_num(v), 3.0);
}

#[test]
fn click_passes_event_target() {
    // event.target je DomNode
    let v = run(r#"
        const el = document.createElement("button");
        el.setAttribute("id", "btn");
        let target_id = "";
        el.addEventListener("click", (e) => { target_id = e.target.id; });
        el.click();
        return target_id;
    "#);
    assert_eq!(as_str(v), "btn");
}

#[test]
fn event_prevent_default() {
    let v = run(r#"
        const e = new Event("click", { cancelable: true });
        e.preventDefault();
        return e.defaultPrevented;
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn text_content_setter() {
    let v = run(r#"
        const el = document.createElement("div");
        el.textContent = "ahoj svete";
        return el.textContent;
    "#);
    assert_eq!(as_str(v), "ahoj svete");
}

#[test]
fn inner_html_setter() {
    let v = run(r#"
        const el = document.createElement("div");
        el.innerHTML = "<p>vnoreny obsah</p>";
        return el.children.length;
    "#);
    assert!(as_num(v) >= 1.0);
}

#[test]
fn input_value_get_set() {
    let v = run(r#"
        const inp = document.createElement("input");
        inp.value = "ahoj";
        return inp.value;
    "#);
    assert_eq!(as_str(v), "ahoj");
}

#[test]
fn input_checked_get_set() {
    let v = run(r#"
        const inp = document.createElement("input");
        inp.checked = true;
        const a = inp.checked;
        inp.checked = false;
        const b = inp.checked;
        return a + ":" + b;
    "#);
    assert_eq!(as_str(v), "true:false");
}

#[test]
fn href_attribute_get() {
    let v = run(r#"
        const a = document.createElement("a");
        a.setAttribute("href", "/page");
        return a.href;
    "#);
    assert_eq!(as_str(v), "/page");
}

#[test]
fn id_setter_changes_attribute() {
    let v = run(r#"
        const el = document.createElement("div");
        el.id = "my-element";
        return el.getAttribute("id");
    "#);
    assert_eq!(as_str(v), "my-element");
}

#[test]
fn multiple_listeners_all_fire() {
    let v = run(r#"
        const el = document.createElement("div");
        let calls = "";
        el.addEventListener("custom", () => { calls += "a"; });
        el.addEventListener("custom", () => { calls += "b"; });
        el.addEventListener("custom", () => { calls += "c"; });
        el.dispatchEvent(new Event("custom"));
        return calls;
    "#);
    assert_eq!(as_str(v), "abc");
}

#[test]
fn event_type() {
    let v = run(r#"
        const e = new Event("custom");
        return e.type;
    "#);
    assert_eq!(as_str(v), "custom");
}

#[test]
fn custom_event_detail() {
    let v = run(r#"
        const e = new CustomEvent("data", { detail: 42 });
        return e.detail;
    "#);
    assert_eq!(as_num(v), 42.0);
}

// ─── window ──────────────────────────────────────────────────────────────

#[test]
fn window_inner_dimensions() {
    let v = run(r#"
        return window.innerWidth + ":" + window.innerHeight;
    "#);
    assert_eq!(as_str(v), "1024:768");
}

#[test]
fn window_location() {
    let v = run(r#"return window.location.pathname;"#);
    assert_eq!(as_str(v), "/");
}

// ─── document.body / documentElement ─────────────────────────────────────

#[test]
fn document_body_exists() {
    let v = run(r#"return document.body.tagName;"#);
    assert_eq!(as_str(v), "BODY");
}

#[test]
fn document_html_exists() {
    let v = run(r#"return document.documentElement.tagName;"#);
    assert_eq!(as_str(v), "HTML");
}

// ─── Form properties ─────────────────────────────────────────────────────

#[test]
fn form_action_method_default() {
    let v = run(r#"
        const f = document.createElement("form");
        f.setAttribute("action", "/api/submit");
        return f.action + ":" + f.method;
    "#);
    assert_eq!(as_str(v), "/api/submit:GET");
}

#[test]
fn form_elements_returns_inputs() {
    let v = run(r#"
        const f = document.createElement("form");
        const i1 = document.createElement("input");
        const i2 = document.createElement("input");
        const div = document.createElement("div");
        f.appendChild(i1);
        f.appendChild(div);
        f.appendChild(i2);
        return f.elements.length;
    "#);
    assert_eq!(as_num(v), 2.0);
}
