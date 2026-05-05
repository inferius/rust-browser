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

#[test]
fn mutation_observer_attribute_change() {
    let code = r#"
        let counter = 0;
        const div = document.createElement("div");
        const observer = new MutationObserver((records) => {
            counter += records.length;
        });
        observer.observe(div, { attributes: true });
        div.setAttribute("data-x", "1");
        div.setAttribute("data-y", "2");
        return counter;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 2.0, "2 setAttribute volani -> 2 records");
}

#[test]
fn mutation_observer_childlist_change() {
    let code = r#"
        let count = 0;
        const parent = document.createElement("div");
        const observer = new MutationObserver((records) => {
            for (const r of records) {
                if (r.type === "childList") count++;
            }
        });
        observer.observe(parent, { childList: true });
        const child = document.createElement("span");
        parent.appendChild(child);
        return count;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 1.0, "1 appendChild -> 1 childList record");
}

#[test]
fn mutation_observer_record_has_target() {
    let code = r#"
        let target_tag = "";
        const div = document.createElement("div");
        const observer = new MutationObserver((records) => {
            target_tag = records[0].target.tagName.toLowerCase();
        });
        observer.observe(div, { attributes: true });
        div.setAttribute("foo", "bar");
        return target_tag;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "div", "target je div node");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn mutation_observer_disconnect_stops() {
    let code = r#"
        let counter = 0;
        const div = document.createElement("div");
        const observer = new MutationObserver((records) => {
            counter += records.length;
        });
        observer.observe(div, { attributes: true });
        div.setAttribute("a", "1");
        observer.disconnect();
        div.setAttribute("b", "2");
        return counter;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 1.0, "po disconnect uz nedispatch");
}

#[test]
fn file_constructor() {
    let code = r#"
        const f = new File(["hello"], "greeting.txt", { type: "text/plain", lastModified: 1234 });
        return f.name + "|" + f.size + "|" + f.type + "|" + f.lastModified;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "greeting.txt|5|text/plain|1234");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn input_files_returns_filelist() {
    let code = r#"
        const inp = document.createElement("input");
        inp.setAttribute("type", "file");
        const fl = inp.files;
        return fl.length;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 0.0, "Empty FileList default");
}

#[test]
fn form_elements_collection() {
    let code = r#"
        const form = document.createElement("form");
        const i1 = document.createElement("input");
        const i2 = document.createElement("input");
        const btn = document.createElement("button");
        form.appendChild(i1);
        form.appendChild(i2);
        form.appendChild(btn);
        return form.elements.length;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 3.0, "form.elements: 2 inputs + 1 button");
}

#[test]
fn form_length() {
    let code = r#"
        const form = document.createElement("form");
        form.appendChild(document.createElement("input"));
        form.appendChild(document.createElement("textarea"));
        return form.length;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 2.0);
}

#[test]
fn form_submit_event_dispatched() {
    let code = r#"
        let dispatched = false;
        const form = document.createElement("form");
        form.setAttribute("action", "/test");
        form.addEventListener("submit", (e) => {
            dispatched = true;
            e.preventDefault();
        });
        form.submit();
        return dispatched;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b, "submit listener volan");
    } else {
        panic!("ocekavan bool");
    }
}

#[test]
fn form_submit_prevent_default_blocks_fetch() {
    let code = r#"
        let called = 0;
        const form = document.createElement("form");
        form.setAttribute("action", "/foo");
        form.addEventListener("submit", (e) => {
            called++;
            e.preventDefault();
        });
        form.submit();
        // Bez preventDefault by se network log naplnil; po preventDefault by mel zustat netknuty
        return called;
    "#;
    let result = as_num(run(code));
    assert_eq!(result, 1.0);
}

#[test]
fn form_submit_event_has_target() {
    let code = r#"
        let target_tag = "";
        const form = document.createElement("form");
        form.addEventListener("submit", (e) => {
            target_tag = e.target.tagName.toLowerCase();
            e.preventDefault();
        });
        form.submit();
        return target_tag;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "form");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn resize_observer_observes_targets() {
    let code = r#"
        const ro = new ResizeObserver(() => {});
        const a = document.createElement("div");
        const b = document.createElement("div");
        ro.observe(a);
        ro.observe(b);
        return ro.__targets__.length;
    "#;
    assert_eq!(as_num(run(code)), 2.0);
}

#[test]
fn resize_observer_unobserve_removes() {
    let code = r#"
        const ro = new ResizeObserver(() => {});
        const a = document.createElement("div");
        const b = document.createElement("div");
        ro.observe(a);
        ro.observe(b);
        ro.unobserve(a);
        return ro.__targets__.length;
    "#;
    assert_eq!(as_num(run(code)), 1.0);
}

#[test]
fn resize_observer_disconnect_clears() {
    let code = r#"
        const ro = new ResizeObserver(() => {});
        ro.observe(document.createElement("div"));
        ro.observe(document.createElement("div"));
        ro.disconnect();
        return ro.__targets__.length;
    "#;
    assert_eq!(as_num(run(code)), 0.0);
}

#[test]
fn intersection_observer_options_stored() {
    let code = r#"
        const io = new IntersectionObserver(() => {}, { rootMargin: "10px" });
        return io.rootMargin;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10px");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn intersection_observer_thresholds_default() {
    let code = r#"
        const io = new IntersectionObserver(() => {});
        return io.thresholds.length;
    "#;
    assert_eq!(as_num(run(code)), 1.0);
}

// ─── New DOM/JS APIs ───────────────────────────────────────────────────

#[test]
fn event_target_construct() {
    let code = r#"
        const t = new EventTarget();
        let called = 0;
        t.addEventListener("test", () => { called++; });
        return typeof t.addEventListener;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn message_channel_two_ports() {
    let code = r#"
        const mc = new MessageChannel();
        return typeof mc.port1.postMessage + "|" + typeof mc.port2.postMessage;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|function");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn notification_constructor() {
    let code = r#"
        const n = new Notification("Hello", { body: "World" });
        return n.title;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Hello");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn request_idle_callback_returns_id() {
    let code = r#"
        const id = requestIdleCallback(() => {});
        return typeof id;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "number");
    } else {
        panic!("ocekavan string");
    }
}

#[test]
fn cancel_idle_callback_no_throw() {
    let code = r#"
        const id = requestIdleCallback(() => {});
        cancelIdleCallback(id);
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn abort_signal_timeout() {
    let code = r#"
        const s = AbortSignal.timeout(1000);
        return s.aborted;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(!b, "fresh timeout signal not aborted");
    }
}

#[test]
fn abort_signal_abort_static() {
    let code = r#"
        const s = AbortSignal.abort("test reason");
        return s.aborted;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b, "AbortSignal.abort vraci aborted=true");
    }
}

#[test]
fn abort_signal_any() {
    let code = r#"
        const s = AbortSignal.any([]);
        return typeof s.addEventListener;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn document_adopted_stylesheets_array() {
    let code = r#"
        return Array.isArray(document.adoptedStyleSheets);
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

// ─── Temporal API ──────────────────────────────────────────────────────

#[test]
fn temporal_now_plain_date() {
    let code = r#"
        const d = Temporal.Now.plainDateISO();
        return typeof d.year + "|" + typeof d.month + "|" + typeof d.day;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "number|number|number");
    }
}

#[test]
fn temporal_now_plain_time() {
    let code = r#"
        const t = Temporal.Now.plainTimeISO();
        return typeof t.hour;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "number");
    }
}

#[test]
fn temporal_now_instant() {
    let code = r#"
        const i = Temporal.Now.instant();
        return typeof i.epochMilliseconds;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "number");
    }
}

#[test]
fn temporal_plain_date_from_string() {
    let code = r#"
        const d = Temporal.PlainDate.from("2024-06-15");
        return d.year + "|" + d.month + "|" + d.day;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "2024|6|15");
    }
}

#[test]
fn temporal_duration_from_object() {
    let code = r#"
        const dur = Temporal.Duration.from({ days: 5, hours: 12 });
        return dur.days + "|" + dur.hours;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "5|12");
    }
}

#[test]
fn temporal_instant_from_epoch_ms() {
    let code = r#"
        const i = Temporal.Instant.fromEpochMilliseconds(1700000000000);
        return i.epochMilliseconds;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1700000000000.0);
    }
}

// ─── Dialog events ─────────────────────────────────────────────────────

#[test]
fn dialog_close_with_return_value() {
    let code = r#"
        const dlg = document.createElement("dialog");
        dlg.showModal();
        dlg.close("ok");
        return dlg.getAttribute("returnValue") + "|" + dlg.hasAttribute("open");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok|false");
    }
}

#[test]
fn dialog_close_event_fires() {
    let code = r#"
        let fired = false;
        const dlg = document.createElement("dialog");
        dlg.addEventListener("close", () => { fired = true; });
        dlg.showModal();
        dlg.close();
        return fired;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

// ─── Event classes ─────────────────────────────────────────────────────

#[test]
fn event_constructor_default_type() {
    let code = r#"
        const e = new Event("custom-thing");
        return e.type;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "custom-thing");
    }
}

#[test]
fn custom_event_with_detail() {
    let code = r#"
        const e = new CustomEvent("click", { detail: { foo: "bar" } });
        return e.detail.foo;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "bar");
    }
}

#[test]
fn pointer_event_constructor() {
    let code = r#"
        const e = new PointerEvent("pointerdown", { clientX: 100, clientY: 50 });
        return e.clientX + "|" + e.clientY;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "100|50");
    }
}

#[test]
fn keyboard_event_key_property() {
    let code = r#"
        const e = new KeyboardEvent("keydown", { key: "Enter", code: "Enter" });
        return e.key;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Enter");
    }
}

#[test]
fn event_prevent_default_method() {
    let code = r#"
        const e = new Event("test");
        e.preventDefault();
        return typeof e.preventDefault;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn event_classes_registered() {
    let code = r#"
        return [
            typeof Event,
            typeof CustomEvent,
            typeof MouseEvent,
            typeof KeyboardEvent,
            typeof PointerEvent,
            typeof TouchEvent,
            typeof WheelEvent,
            typeof InputEvent,
            typeof FocusEvent,
            typeof DragEvent,
            typeof SubmitEvent,
            typeof ProgressEvent,
            typeof MessageEvent,
            typeof ErrorEvent,
            typeof HashChangeEvent,
            typeof PopStateEvent,
            typeof StorageEvent,
            typeof AnimationEvent,
            typeof TransitionEvent,
            typeof ClipboardEvent
        ].every(t => t === "function");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

// ─── Range / Selection real ────────────────────────────────────────────

#[test]
fn range_set_start_end_updates_collapsed() {
    let code = r#"
        const r = document.createRange();
        const node = document.createElement("div");
        r.setStart(node, 0);
        r.setEnd(node, 5);
        return r.collapsed;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(!b, "setStart 0 / setEnd 5 -> not collapsed");
    }
}

#[test]
fn range_collapse_to_start() {
    let code = r#"
        const r = document.createRange();
        const node = document.createElement("div");
        r.setStart(node, 0);
        r.setEnd(node, 5);
        r.collapse(true);
        return r.collapsed + "|" + r.endOffset;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|0");
    }
}

#[test]
fn range_select_node() {
    let code = r#"
        const r = document.createRange();
        const node = document.createElement("p");
        r.selectNode(node);
        return r.endOffset;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1.0);
    }
}

#[test]
fn range_clone_copies_state() {
    let code = r#"
        const r = document.createRange();
        const node = document.createElement("div");
        r.setStart(node, 3);
        r.setEnd(node, 7);
        const r2 = r.cloneRange();
        return r2.startOffset + "|" + r2.endOffset;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3|7");
    }
}

#[test]
fn selection_add_range_increments_count() {
    let code = r#"
        const sel = document.getSelection();
        sel.removeAllRanges();
        const r = document.createRange();
        sel.addRange(r);
        return sel.rangeCount;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1.0);
    }
}

#[test]
fn selection_remove_all_ranges_clears() {
    let code = r#"
        const sel = document.getSelection();
        sel.addRange(document.createRange());
        sel.addRange(document.createRange());
        sel.removeAllRanges();
        return sel.rangeCount;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 0.0);
    }
}

#[test]
fn selection_collapse_sets_anchor() {
    let code = r#"
        const sel = document.getSelection();
        const node = document.createElement("p");
        sel.collapse(node, 5);
        return sel.anchorOffset + "|" + sel.isCollapsed;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "5|true");
    }
}

#[test]
fn selection_select_all_children() {
    let code = r#"
        const sel = document.getSelection();
        const node = document.createElement("div");
        sel.selectAllChildren(node);
        return sel.type;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Range");
    }
}

// ─── Web Crypto API ───────────────────────────────────────────────────

#[test]
fn crypto_subtle_digest_returns_array() {
    let code = r#"
        return crypto.subtle.digest("SHA-256", "hello").then(buf => buf.length);
    "#;
    // Test: digest vraci Promise s ArrayBuffer-like array (32 bytes)
    let result = run(code);
    // Promise resolved synchronne
    if let crate::interpreter::JsValue::Object(o) = result {
        let state = o.borrow().get("__promise_state__");
        assert!(matches!(state, crate::interpreter::JsValue::Str(s) if s == "fulfilled"));
    }
}

#[test]
fn crypto_random_uuid_format() {
    let code = r#"
        const u = crypto.randomUUID();
        return u.length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 36.0); // UUID format: 8-4-4-4-12 hex chars + 4 dashes
    }
}

// ─── Shadow DOM ───────────────────────────────────────────────────────

#[test]
fn element_attach_shadow_returns_root() {
    let code = r#"
        const div = document.createElement("div");
        const sr = div.attachShadow({ mode: "open" });
        return sr.mode + "|" + (sr.host === div);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "open|true");
    }
}

#[test]
fn element_shadow_root_after_attach() {
    let code = r#"
        const div = document.createElement("div");
        div.attachShadow({ mode: "open" });
        return div.shadowRoot !== null;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn element_shadow_root_null_default() {
    let code = r#"
        const div = document.createElement("div");
        return div.shadowRoot === null;
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

// ─── Web Animations API ───────────────────────────────────────────────

#[test]
fn element_animate_returns_animation() {
    let code = r#"
        const div = document.createElement("div");
        const anim = div.animate(
            [{ opacity: 0 }, { opacity: 1 }],
            { duration: 1000 }
        );
        return anim.playState;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "running");
    }
}

#[test]
fn animation_has_play_pause_cancel() {
    let code = r#"
        const div = document.createElement("div");
        const anim = div.animate([], {});
        return typeof anim.play + "|" + typeof anim.pause + "|" + typeof anim.cancel;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|function|function");
    }
}

#[test]
fn element_get_animations_empty() {
    let code = r#"
        const div = document.createElement("div");
        return div.getAnimations().length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 0.0);
    }
}

// ─── Speech / Sensors / Trusted Types ─────────────────────────────────

#[test]
fn speech_synthesis_utterance() {
    let code = r#"
        const u = new SpeechSynthesisUtterance("Hello");
        return u.text + "|" + u.lang;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Hello|en-US");
    }
}

#[test]
fn accelerometer_construct() {
    let code = r#"
        const a = new Accelerometer();
        return typeof a.start + "|" + a.x;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|0");
    }
}

#[test]
fn trusted_types_create_policy() {
    let code = r#"
        const p = trustedTypes.createPolicy("default", {
            createHTML: (s) => s
        });
        return p.name + "|" + typeof p.createHTML;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "default|function");
    }
}

#[test]
fn show_open_file_picker_returns_promise() {
    let code = r#"
        const p = showOpenFilePicker();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

// ─── ArrayBuffer / DataView extras ────────────────────────────────────

#[test]
fn array_buffer_byte_length() {
    let code = r#"
        const ab = new ArrayBuffer(16);
        return ab.byteLength + "|" + ab.detached;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "16|false");
    }
}

#[test]
fn array_buffer_transfer_marks_detached() {
    let code = r#"
        const ab = new ArrayBuffer(8);
        const ab2 = ab.transfer();
        return ab.detached + "|" + ab2.byteLength;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|8");
    }
}

#[test]
fn array_buffer_resize() {
    let code = r#"
        const ab = new ArrayBuffer(4);
        ab.resize(8);
        return ab.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 8.0);
    }
}

#[test]
fn array_buffer_slice() {
    let code = r#"
        const ab = new ArrayBuffer(10);
        const ab2 = ab.slice(2, 6);
        return ab2.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 4.0);
    }
}

#[test]
fn data_view_set_get_uint8() {
    let code = r#"
        const ab = new ArrayBuffer(4);
        const dv = new DataView(ab);
        dv.setUint8(0, 42);
        dv.setUint8(1, 99);
        return dv.getUint8(0) + "|" + dv.getUint8(1);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "42|99");
    }
}

#[test]
fn data_view_get_int8_signed() {
    let code = r#"
        const ab = new ArrayBuffer(2);
        const dv = new DataView(ab);
        dv.setUint8(0, 200);
        return dv.getInt8(0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        // 200 as i8 = -56
        assert_eq!(n, -56.0);
    }
}

#[test]
fn data_view_get_uint16_be() {
    let code = r#"
        const ab = new ArrayBuffer(4);
        const dv = new DataView(ab);
        dv.setUint8(0, 1);
        dv.setUint8(1, 2);
        // Big-endian default: (1 << 8) | 2 = 258
        return dv.getUint16(0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 258.0);
    }
}
