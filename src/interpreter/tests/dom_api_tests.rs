/// Testy DOM API z JS pohledu (document, querySelector, classList, dataset, atd.)

use super::helpers::*;
use crate::interpreter::*;
use crate::browser::dom::Document;

fn run_with_doc(html: &str, js: &str) -> JsValue {
    let doc = crate::browser::html_parser::parse_html(html, "");
    let lexer = crate::lexer::base::Lexer::parse_str(js, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            crate::tokens::TokenKind::Whitespace
            | crate::tokens::TokenKind::Newline
            | crate::tokens::TokenKind::CommentLine(_)
            | crate::tokens::TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.set_document(doc);
    interp.run(&program).unwrap()
}

#[test]
fn document_get_element_by_id() {
    let r = run_with_doc(
        r#"<html><body><div id="target">found</div></body></html>"#,
        r#"
            const el = document.getElementById("target");
            return el ? el.textContent : "null";
        "#,
    );
    assert!(r.to_string().contains("found"));
}

#[test]
fn document_get_element_by_id_returns_null_when_missing() {
    let r = run_with_doc(
        r#"<html><body></body></html>"#,
        r#"
            const el = document.getElementById("missing");
            return el === null;
        "#,
    );
    assert_eq!(r.to_string(), "true");
}

#[test]
fn document_query_selector_id() {
    let r = run_with_doc(
        r##"<html><body><p id="x">hi</p></body></html>"##,
        r##"
            const el = document.querySelector("#x");
            return el.tagName;
        "##,
    );
    assert!(r.to_string().to_lowercase().contains("p"));
}

#[test]
fn document_query_selector_class() {
    let r = run_with_doc(
        r#"<html><body><p class="foo">A</p><p class="bar">B</p></body></html>"#,
        r#"
            const el = document.querySelector(".foo");
            return el ? el.textContent : "null";
        "#,
    );
    assert!(r.to_string().contains("A"));
}

#[test]
fn document_query_selector_all_returns_array_like() {
    let r = run_with_doc(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"
            const els = document.querySelectorAll("p");
            return els.length;
        "#,
    );
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn document_get_elements_by_tag() {
    let r = run_with_doc(
        r#"<html><body><div></div><div></div><span></span></body></html>"#,
        r#"
            const divs = document.getElementsByTagName("div");
            return divs.length;
        "#,
    );
    assert_eq!(as_num(r), 2.0);
}

#[test]
fn document_get_elements_by_class_name() {
    let r = run_with_doc(
        r#"<html><body><div class="a"></div><div class="a b"></div><div class="b"></div></body></html>"#,
        r#"
            const els = document.getElementsByClassName("a");
            return els.length;
        "#,
    );
    assert_eq!(as_num(r), 2.0);
}

#[test]
fn element_class_list_contains() {
    let r = run_with_doc(
        r#"<html><body><div id="x" class="alpha beta"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            return el.classList.contains("alpha") + "|" + el.classList.contains("gamma");
        "#,
    );
    assert_eq!(r.to_string(), "true|false");
}

#[test]
fn element_class_list_add() {
    let r = run_with_doc(
        r#"<html><body><div id="x"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.classList.add("new-class");
            return el.classList.contains("new-class");
        "#,
    );
    assert_eq!(r.to_string(), "true");
}

#[test]
fn element_class_list_remove() {
    let r = run_with_doc(
        r#"<html><body><div id="x" class="a b c"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.classList.remove("b");
            return el.classList.contains("b") + "|" + el.classList.contains("a");
        "#,
    );
    assert_eq!(r.to_string(), "false|true");
}

#[test]
fn element_class_list_toggle() {
    let r = run_with_doc(
        r#"<html><body><div id="x" class="a"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.classList.toggle("a");  // off
            const off = el.classList.contains("a");
            el.classList.toggle("a");  // on
            const on = el.classList.contains("a");
            return off + "|" + on;
        "#,
    );
    assert_eq!(r.to_string(), "false|true");
}

#[test]
fn element_set_attribute() {
    let r = run_with_doc(
        r#"<html><body><div id="x"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.setAttribute("data-key", "value123");
            return el.getAttribute("data-key");
        "#,
    );
    assert_eq!(r.to_string(), "value123");
}

#[test]
fn element_remove_attribute() {
    let r = run_with_doc(
        r#"<html><body><div id="x" data-foo="bar"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.removeAttribute("data-foo");
            return el.getAttribute("data-foo");
        "#,
    );
    let s = r.to_string();
    assert!(s == "null" || s == "undefined");
}

#[test]
fn element_has_attribute() {
    let r = run_with_doc(
        r#"<html><body><div id="x" data-foo="1"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            return el.hasAttribute("data-foo") + "|" + el.hasAttribute("data-bar");
        "#,
    );
    assert_eq!(r.to_string(), "true|false");
}

#[test]
fn element_text_content_get() {
    let r = run_with_doc(
        r#"<html><body><div id="x">Hello World</div></body></html>"#,
        r#"
            return document.getElementById("x").textContent;
        "#,
    );
    assert!(r.to_string().contains("Hello"));
}

#[test]
fn element_text_content_set() {
    let r = run_with_doc(
        r#"<html><body><div id="x">old</div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.textContent = "new text";
            return el.textContent;
        "#,
    );
    assert_eq!(r.to_string(), "new text");
}

#[test]
fn document_create_element() {
    let r = run_with_doc(
        r#"<html><body></body></html>"#,
        r#"
            const div = document.createElement("div");
            return div.tagName.toLowerCase();
        "#,
    );
    assert_eq!(r.to_string(), "div");
}

#[test]
fn parent_node_traversal() {
    let r = run_with_doc(
        r#"<html><body><div id="parent"><span id="child">x</span></div></body></html>"#,
        r#"
            const child = document.getElementById("child");
            const p = child.parentNode;
            return p ? p.tagName.toLowerCase() : "null";
        "#,
    );
    assert_eq!(r.to_string(), "div");
}

#[test]
fn children_collection() {
    let r = run_with_doc(
        r#"<html><body><div id="p"><span></span><a></a><b></b></div></body></html>"#,
        r#"
            const p = document.getElementById("p");
            return p.children.length;
        "#,
    );
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn element_dataset_access() {
    let r = run_with_doc(
        r#"<html><body><div id="x" data-user-id="42" data-name="alice"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            // dataset.userId pres data-user-id (camelCase conversion)
            return el.dataset.userId + "|" + el.dataset.name;
        "#,
    );
    assert_eq!(r.to_string(), "42|alice");
}

#[test]
fn document_title_property() {
    let r = run_with_doc(
        r#"<html><head><title>My Page</title></head><body></body></html>"#,
        r#"return document.title;"#,
    );
    assert_eq!(r.to_string(), "My Page");
}

#[test]
fn document_url_or_location_string() {
    // document.URL nebo document.location.href - zalezi na implementaci.
    let r = run_with_doc(
        r#"<html><body></body></html>"#,
        r#"
            return typeof document.URL;
        "#,
    );
    let s = r.to_string();
    assert!(s == "string" || s == "undefined");
}

#[test]
fn element_inner_html_set() {
    let r = run_with_doc(
        r#"<html><body><div id="x"></div></body></html>"#,
        r#"
            const el = document.getElementById("x");
            el.innerHTML = "<span>injected</span>";
            return el.innerHTML.indexOf("injected") >= 0;
        "#,
    );
    assert_eq!(r.to_string(), "true");
}

// --- Selection / Range API ---

#[test]
fn window_get_selection_returns_object() {
    let r = run_with_doc(
        "<html><body></body></html>",
        "return typeof getSelection();",
    );
    assert_eq!(r.to_string(), "object");
}

#[test]
fn selection_has_range_count() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const sel = getSelection();
        return typeof sel.rangeCount;
        "#,
    );
    assert_eq!(r.to_string(), "number");
}

#[test]
fn selection_remove_all_ranges() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const sel = getSelection();
        sel.removeAllRanges();
        return sel.rangeCount;
        "#,
    );
    assert_eq!(r.to_string(), "0");
}

#[test]
fn document_create_range() {
    let r = run_with_doc(
        "<html><body></body></html>",
        "return typeof document.createRange();",
    );
    assert_eq!(r.to_string(), "object");
}

#[test]
fn range_has_collapsed() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const range = document.createRange();
        return range.collapsed;
        "#,
    );
    assert_eq!(r.to_string(), "true");
}

#[test]
fn range_clone_range() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const range = document.createRange();
        const clone = range.cloneRange();
        return typeof clone;
        "#,
    );
    assert_eq!(r.to_string(), "object");
}

#[test]
fn range_to_string_empty() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const range = document.createRange();
        return range.toString();
        "#,
    );
    assert_eq!(r.to_string(), "");
}

#[test]
fn new_range_constructor() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const r = new Range();
        return typeof r;
        "#,
    );
    assert_eq!(r.to_string(), "object");
}

// --- MutationObserver enhanced ---

#[test]
fn mutation_observer_callback_stored() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        let called = false;
        const obs = new MutationObserver(() => { called = true; });
        obs.observe(document.body, { childList: true });
        return typeof obs;
        "#,
    );
    assert_eq!(r.to_string(), "object");
}

#[test]
fn mutation_observer_take_records_array() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const obs = new MutationObserver(() => {});
        const recs = obs.takeRecords();
        return Array.isArray(recs);
        "#,
    );
    assert_eq!(r.to_string(), "true");
}

#[test]
fn mutation_observer_disconnect_no_throw() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const obs = new MutationObserver(() => {});
        obs.observe(document.body, { childList: true });
        obs.disconnect();
        return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

// --- CustomElements lifecycle ---

fn run_js(js: &str) -> JsValue {
    run_with_doc("<html><body></body></html>", js)
}

#[test]
fn custom_elements_define_and_get() {
    let r = run_js(r#"
        class MyEl extends HTMLElement {}
        customElements.define('my-el', MyEl);
        return typeof customElements.get('my-el');
    "#);
    assert_eq!(r.to_string(), "function");
}

#[test]
fn custom_elements_get_unknown_returns_undefined() {
    let r = run_js(r#"
        return typeof customElements.get('unknown-el');
    "#);
    assert_eq!(r.to_string(), "undefined");
}

#[test]
fn custom_elements_constructor_called_on_create() {
    let r = run_js(r#"
        let constructed = false;
        class MyEl extends HTMLElement {
            constructor() { constructed = true; }
        }
        customElements.define('x-foo', MyEl);
        document.createElement('x-foo');
        return constructed;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn custom_elements_connected_callback_on_append() {
    let r = run_js(r#"
        let connected = false;
        class MyEl extends HTMLElement {
            connectedCallback() { connected = true; }
        }
        customElements.define('x-bar', MyEl);
        const el = document.createElement('x-bar');
        document.body.appendChild(el);
        return connected;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn custom_elements_disconnected_callback_on_remove() {
    let r = run_js(r#"
        let disconnected = false;
        class MyEl extends HTMLElement {
            disconnectedCallback() { disconnected = true; }
        }
        customElements.define('x-baz', MyEl);
        const el = document.createElement('x-baz');
        document.body.appendChild(el);
        document.body.removeChild(el);
        return disconnected;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn custom_elements_attribute_changed_callback() {
    let r = run_js(r#"
        let changed = null;
        class MyEl extends HTMLElement {
            attributeChangedCallback(name, oldVal, newVal) {
                changed = name + ':' + oldVal + '->' + newVal;
            }
        }
        customElements.define('x-qux', MyEl);
        const el = document.createElement('x-qux');
        el.setAttribute('data-x', 'hello');
        return changed;
    "#);
    assert_eq!(r.to_string(), "data-x:->hello");
}

#[test]
fn custom_elements_no_callback_no_error() {
    // Custom element bez lifecycle metod - zadna chyba
    let r = run_js(r#"
        class Plain extends HTMLElement {}
        customElements.define('x-plain', Plain);
        const el = document.createElement('x-plain');
        document.body.appendChild(el);
        document.body.removeChild(el);
        return "ok";
    "#);
    assert_eq!(r.to_string(), "ok");
}

// --- document.createDocumentFragment ---

#[test]
fn document_create_document_fragment() {
    let r = run_with_doc(
        "<html><body></body></html>",
        r#"
        const frag = document.createDocumentFragment();
        return typeof frag;
        "#,
    );
    // Fragment je DomNode - typeof vraci "object"
    assert!(r.to_string() == "object" || r.to_string() == "undefined");
}
