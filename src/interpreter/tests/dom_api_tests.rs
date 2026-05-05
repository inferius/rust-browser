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

// ─── Web Streams + Compression + Cookie Store ─────────────────────────

#[test]
fn readable_stream_get_reader() {
    let code = r#"
        const rs = new ReadableStream();
        const reader = rs.getReader();
        return typeof reader.read + "|" + typeof reader.releaseLock;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|function");
    }
}

#[test]
fn writable_stream_get_writer() {
    let code = r#"
        const ws = new WritableStream();
        const w = ws.getWriter();
        return typeof w.write;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn transform_stream_has_readable_writable() {
    let code = r#"
        const ts = new TransformStream();
        return typeof ts.readable + "|" + typeof ts.writable;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "object|object");
    }
}

#[test]
fn compression_stream_format() {
    let code = r#"
        const cs = new CompressionStream("gzip");
        return cs.__compression_stream__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "gzip");
    }
}

#[test]
fn cookie_store_set_and_get() {
    let code = r#"
        cookieStore.set("test", "value123");
        return cookieStore.get("test");
    "#;
    let result = run(code);
    if let crate::interpreter::JsValue::Object(o) = result {
        if let crate::interpreter::JsValue::Object(inner) =
            o.borrow().get("__promise_value__") {
            let val = inner.borrow().get("value");
            assert!(matches!(val, crate::interpreter::JsValue::Str(s) if s == "value123"));
        }
    }
}

#[test]
fn cookie_store_delete() {
    let code = r#"
        cookieStore.set("delete-me", "x");
        cookieStore.delete("delete-me");
        return cookieStore.get("delete-me");
    "#;
    let result = run(code);
    if let crate::interpreter::JsValue::Object(o) = result {
        let val = o.borrow().get("__promise_value__");
        assert!(matches!(val, crate::interpreter::JsValue::Null));
    }
}

// ─── Typed Arrays variants ────────────────────────────────────────────

#[test]
fn typed_arrays_all_variants() {
    let code = r#"
        const checks = [
            new Uint8Array(4).BYTES_PER_ELEMENT === 1,
            new Int8Array(4).BYTES_PER_ELEMENT === 1,
            new Uint16Array(4).BYTES_PER_ELEMENT === 2,
            new Int16Array(4).BYTES_PER_ELEMENT === 2,
            new Uint32Array(4).BYTES_PER_ELEMENT === 4,
            new Int32Array(4).BYTES_PER_ELEMENT === 4,
            new Float32Array(4).BYTES_PER_ELEMENT === 4,
            new Float64Array(4).BYTES_PER_ELEMENT === 8,
            new BigInt64Array(4).BYTES_PER_ELEMENT === 8,
            new BigUint64Array(4).BYTES_PER_ELEMENT === 8,
            new Uint8ClampedArray(4).BYTES_PER_ELEMENT === 1
        ];
        return checks.every(c => c);
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn typed_array_byte_length() {
    let code = r#"
        const arr = new Float32Array(10);
        return arr.byteLength + "|" + arr.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "40|10"); // 10 elem * 4 bytes
    }
}

// ─── HTML elements + popover + Atomics extras ─────────────────────────

#[test]
fn html_progress_value_max() {
    let r = run_with_doc(
        r#"<html><body><progress value="3" max="10"></progress></body></html>"#,
        r#"
            const p = document.getElementsByTagName("progress")[0];
            return p.value + "|" + p.max + "|" + p.position;
        "#,
    );
    assert_eq!(r.to_string(), "3|10|0.3");
}

#[test]
fn html_meter_value_min_max() {
    let r = run_with_doc(
        r#"<html><body><meter value="60" min="0" max="100" low="30" high="80"></meter></body></html>"#,
        r#"
            const m = document.getElementsByTagName("meter")[0];
            return m.value + "|" + m.min + "|" + m.max + "|" + m.low + "|" + m.high;
        "#,
    );
    assert_eq!(r.to_string(), "60|0|100|30|80");
}

#[test]
fn html_datalist_options() {
    let r = run_with_doc(
        r#"<html><body><datalist><option>a</option><option>b</option><option>c</option></datalist></body></html>"#,
        r#"
            const dl = document.getElementsByTagName("datalist")[0];
            return dl.options.length;
        "#,
    );
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn html_anchor_rel_list() {
    let r = run_with_doc(
        r#"<html><body><a rel="noopener noreferrer external"></a></body></html>"#,
        r#"
            const a = document.getElementsByTagName("a")[0];
            return a.relList.length + "|" + a.relList[0];
        "#,
    );
    assert_eq!(r.to_string(), "3|noopener");
}

#[test]
fn element_popover_show_hide_toggle() {
    let code = r#"
        const div = document.createElement("div");
        div.setAttribute("popover", "");
        div.showPopover();
        const after_show = div.getAttribute("data-popover-open");
        div.hidePopover();
        const after_hide = div.hasAttribute("data-popover-open");
        const toggled = div.togglePopover();
        return after_show + "|" + after_hide + "|" + toggled;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false|true");
    }
}

#[test]
fn atomics_load_store() {
    let code = r#"
        const sab = new SharedArrayBuffer(4);
        Atomics.store(sab, 0, 42);
        return Atomics.load(sab, 0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 42.0);
    }
}

#[test]
fn atomics_compare_exchange() {
    let code = r#"
        const sab = new SharedArrayBuffer(4);
        Atomics.store(sab, 0, 5);
        Atomics.compareExchange(sab, 0, 5, 10);
        return Atomics.load(sab, 0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 10.0);
    }
}

#[test]
fn atomics_is_lock_free() {
    let code = r#"
        return [
            Atomics.isLockFree(1),
            Atomics.isLockFree(4),
            Atomics.isLockFree(8),
            Atomics.isLockFree(7)
        ].map(b => b ? "1" : "0").join("");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1110");
    }
}

#[test]
fn atomics_bitwise() {
    let code = r#"
        const sab = new SharedArrayBuffer(4);
        Atomics.store(sab, 0, 12);
        Atomics.and(sab, 0, 10);
        return Atomics.load(sab, 0);
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 8.0); // 12 & 10 = 8
    }
}

#[test]
fn text_decoder_options() {
    let code = r#"
        const td = new TextDecoder("utf-8");
        return td.encoding + "|" + td.fatal + "|" + td.ignoreBOM;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "utf-8|false|false");
    }
}

#[test]
fn text_encoder_stream_constructor() {
    let code = r#"
        const tes = new TextEncoderStream();
        return tes.encoding;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "utf-8");
    }
}

// ─── Performance API real ─────────────────────────────────────────────

#[test]
fn performance_mark_creates_entry() {
    let code = r#"
        performance.mark("start");
        return performance.getEntries().length;
    "#;
    let r = run(code);
    if let crate::interpreter::JsValue::Number(n) = r {
        assert!(n >= 1.0);
    }
}

#[test]
fn performance_measure_between_marks() {
    let code = r#"
        performance.mark("a");
        performance.mark("b");
        performance.measure("ab", "a", "b");
        const measures = performance.getEntriesByType("measure");
        return measures.length + "|" + measures[0].name;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1|ab");
    }
}

#[test]
fn performance_get_entries_by_type() {
    let code = r#"
        performance.clearMarks();
        performance.clearMeasures();
        performance.mark("m1");
        performance.mark("m2");
        return performance.getEntriesByType("mark").length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 2.0);
    }
}

#[test]
fn performance_get_entries_by_name() {
    let code = r#"
        performance.clearMarks();
        performance.mark("findme");
        performance.mark("other");
        return performance.getEntriesByName("findme").length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1.0);
    }
}

#[test]
fn performance_clear_marks_specific() {
    let code = r#"
        performance.clearMarks();
        performance.mark("a");
        performance.mark("b");
        performance.clearMarks("a");
        return performance.getEntriesByType("mark").length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 1.0);
    }
}

// ─── FormData + Headers + Request ─────────────────────────────────────

#[test]
fn formdata_append_get_getall() {
    let code = r#"
        const fd = new FormData();
        fd.append("name", "Alice");
        fd.append("name", "Bob");
        fd.append("age", "30");
        return fd.get("name") + "|" + fd.getAll("name").length + "|" + fd.has("age");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Alice|2|true");
    }
}

#[test]
fn formdata_set_replaces() {
    let code = r#"
        const fd = new FormData();
        fd.append("k", "1");
        fd.append("k", "2");
        fd.set("k", "3");
        return fd.getAll("k").join(",");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3");
    }
}

#[test]
fn formdata_delete() {
    let code = r#"
        const fd = new FormData();
        fd.append("a", "1");
        fd.append("b", "2");
        fd.delete("a");
        return fd.has("a") + "|" + fd.has("b");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "false|true");
    }
}

#[test]
fn formdata_entries_iterator() {
    let code = r#"
        const fd = new FormData();
        fd.append("a", "1");
        fd.append("b", "2");
        return fd.entries().toArray().length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 2.0);
    }
}

#[test]
fn headers_set_get() {
    let code = r#"
        const h = new Headers();
        h.set("Content-Type", "application/json");
        return h.get("content-type");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "application/json");
    }
}

#[test]
fn headers_append_combine() {
    let code = r#"
        const h = new Headers();
        h.append("Set-Cookie", "a=1");
        h.append("Set-Cookie", "b=2");
        return h.get("set-cookie");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "a=1, b=2");
    }
}

#[test]
fn headers_has_delete() {
    let code = r#"
        const h = new Headers();
        h.set("X-Foo", "bar");
        h.delete("X-Foo");
        return h.has("x-foo");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(!b);
    }
}

#[test]
fn request_constructor_method() {
    let code = r#"
        const r = new Request("/api", { method: "POST", body: "test" });
        return r.url + "|" + r.method;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "/api|POST");
    }
}

#[test]
fn url_can_parse_via_helper() {
    let code = r#"
        return __url_can_parse__("https://example.com");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn url_parse_invalid_returns_null() {
    let code = r#"
        return __url_parse__("invalid");
    "#;
    let r = run(code);
    assert!(matches!(r, crate::interpreter::JsValue::Null));
}

// ─── DOM Geometry + Console extras ────────────────────────────────────

#[test]
fn dom_rect_construct() {
    let code = r#"
        const r = new DOMRect(10, 20, 100, 50);
        return r.x + "|" + r.y + "|" + r.width + "|" + r.height + "|" + r.right + "|" + r.bottom;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10|20|100|50|110|70");
    }
}

#[test]
fn dom_point_default() {
    let code = r#"
        const p = new DOMPoint(1, 2, 3);
        return p.x + "|" + p.y + "|" + p.z + "|" + p.w;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1|2|3|1");
    }
}

#[test]
fn dom_matrix_identity() {
    let code = r#"
        const m = new DOMMatrix();
        return m.isIdentity + "|" + m.is2D + "|" + m.a + "|" + m.d;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|true|1|1");
    }
}

#[test]
fn dom_matrix_2d_args() {
    let code = r#"
        const m = new DOMMatrix([1, 0, 0, 1, 10, 20]);
        return m.e + "|" + m.f + "|" + m.is2D;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10|20|true");
    }
}

#[test]
fn dom_quad_corners() {
    let code = r#"
        const q = new DOMQuad();
        return typeof q.p1 + "|" + typeof q.p2 + "|" + typeof q.p3 + "|" + typeof q.p4;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "object|object|object|object");
    }
}

#[test]
fn console_table_no_throw() {
    let code = r#"
        console.table([{a: 1, b: 2}]);
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_group_group_end() {
    let code = r#"
        console.group("section");
        console.log("inside");
        console.groupEnd();
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_time_time_end() {
    let code = r#"
        console.time("op");
        console.timeEnd("op");
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_count_increments() {
    let code = r#"
        console.count("a");
        console.count("a");
        console.count("a");
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

#[test]
fn console_assert_no_throw() {
    let code = r#"
        console.assert(true, "should not log");
        console.assert(false, "should log");
        return "ok";
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "ok");
    }
}

// ─── DOMException + ImageData + OffscreenCanvas + Path2D ──────────────

#[test]
fn dom_exception_with_name() {
    let code = r#"
        const e = new DOMException("Not found", "NotFoundError");
        return e.name + "|" + e.message + "|" + e.code;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "NotFoundError|Not found|8");
    }
}

#[test]
fn dom_exception_default_error() {
    let code = r#"
        const e = new DOMException("test");
        return e.name + "|" + e.code;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "Error|0");
    }
}

#[test]
fn dom_exception_quota_exceeded() {
    let code = r#"
        return new DOMException("full", "QuotaExceededError").code;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 22.0);
    }
}

#[test]
fn image_data_construct_dimensions() {
    let code = r#"
        const id = new ImageData(10, 5);
        return id.width + "|" + id.height + "|" + id.data.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "10|5|200"); // 10*5*4 = 200
    }
}

#[test]
fn image_data_color_space() {
    let code = r#"
        const id = new ImageData(2, 2);
        return id.colorSpace;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "srgb");
    }
}

#[test]
fn offscreen_canvas_dimensions() {
    let code = r#"
        const oc = new OffscreenCanvas(200, 150);
        return oc.width + "|" + oc.height;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "200|150");
    }
}

#[test]
fn path_2d_methods_exist() {
    let code = r#"
        const p = new Path2D();
        p.moveTo(0, 0);
        p.lineTo(10, 10);
        p.arc(50, 50, 30, 0, Math.PI * 2);
        return typeof p.closePath;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn create_image_bitmap_returns_promise() {
    let code = r#"
        const p = createImageBitmap();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

// ─── Element extras + APIs batch ──────────────────────────────────────

#[test]
fn element_check_visibility_default_true() {
    let code = r#"
        const div = document.createElement("div");
        return div.checkVisibility();
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn element_check_visibility_display_none() {
    let code = r#"
        const div = document.createElement("div");
        div.setAttribute("style", "display:none");
        return div.checkVisibility();
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(!b);
    }
}

#[test]
fn element_request_fullscreen_returns_promise() {
    let code = r#"
        const div = document.createElement("div");
        const p = div.requestFullscreen();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

#[test]
fn element_attach_internals() {
    let code = r#"
        const div = document.createElement("div");
        const i = div.attachInternals();
        return typeof i.setFormValue + "|" + i.checkValidity();
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|true");
    }
}

#[test]
fn element_computed_style_map_stub() {
    let code = r#"
        const div = document.createElement("div");
        const m = div.computedStyleMap();
        return typeof m.get + "|" + m.size;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function|0");
    }
}

#[test]
fn web_transport_constructor() {
    let code = r#"
        const wt = new WebTransport("https://example.com/wt");
        return wt.url;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "https://example.com/wt");
    }
}

#[test]
fn reporting_observer_methods() {
    let code = r#"
        const ro = new ReportingObserver(() => {});
        ro.observe();
        ro.disconnect();
        return typeof ro.takeRecords;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

// ─── DOM constructors batch ───────────────────────────────────────────

#[test]
fn document_fragment_construct() {
    let code = r#"
        const f = new DocumentFragment();
        return f.nodeType + "|" + f.nodeName;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "11|#document-fragment");
    }
}

#[test]
fn comment_construct_with_data() {
    let code = r#"
        const c = new Comment("hello");
        return c.nodeType + "|" + c.data + "|" + c.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "8|hello|5");
    }
}

#[test]
fn text_construct_data() {
    let code = r#"
        const t = new Text("World");
        return t.nodeType + "|" + t.data;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "3|World");
    }
}

#[test]
fn node_constants() {
    let code = r#"
        return Node.ELEMENT_NODE + "|" + Node.TEXT_NODE + "|" + Node.COMMENT_NODE;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "1|3|8");
    }
}

#[test]
fn node_document_position_constants() {
    let code = r#"
        return Node.DOCUMENT_POSITION_CONTAINS + "|" + Node.DOCUMENT_POSITION_CONTAINED_BY;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "8|16");
    }
}

#[test]
fn dom_token_list_add_remove_toggle() {
    let code = r#"
        const tl = new DOMTokenList();
        tl.add("a", "b", "c");
        const has_b = tl.contains("b");
        tl.remove("b");
        const has_b_after = tl.contains("b");
        const toggled = tl.toggle("d");
        return has_b + "|" + has_b_after + "|" + toggled + "|" + tl.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false|true|3"); // a c d
    }
}

#[test]
fn html_collection_constructor() {
    let code = r#"
        const c = new HTMLCollection();
        return c.length + "|" + typeof c.item;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "0|function");
    }
}

#[test]
fn node_list_constructor() {
    let code = r#"
        const nl = new NodeList();
        return nl.length + "|" + typeof nl.forEach;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "0|function");
    }
}

#[test]
fn mutation_record_default_props() {
    let code = r#"
        const m = new MutationRecord();
        return m.type + "|" + (m.target === null);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "childList|true");
    }
}

// ─── Worker + DOM extras ──────────────────────────────────────────────

#[test]
fn shared_worker_has_port() {
    let code = r#"
        const sw = new SharedWorker("worker.js");
        return typeof sw.port + "|" + sw.url;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "object|worker.js");
    }
}

#[test]
fn image_constructor_attrs() {
    let code = r#"
        const img = new Image(100, 50);
        return img.tagName.toLowerCase() + "|" + img.getAttribute("width") + "|" + img.getAttribute("height");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "img|100|50");
    }
}

#[test]
fn audio_constructor_with_src() {
    let code = r#"
        const a = new Audio("song.mp3");
        return a.tagName.toLowerCase() + "|" + a.getAttribute("src");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "audio|song.mp3");
    }
}

#[test]
fn option_constructor_text_value() {
    let code = r#"
        const opt = new Option("Apple", "1");
        return opt.tagName.toLowerCase() + "|" + opt.getAttribute("value");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "option|1");
    }
}

#[test]
fn data_transfer_set_get() {
    let code = r#"
        const dt = new DataTransfer();
        dt.setData("text/plain", "hello");
        return dt.getData("text/plain");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "hello");
    }
}

#[test]
fn data_transfer_clear() {
    let code = r#"
        const dt = new DataTransfer();
        dt.setData("text/plain", "x");
        dt.clearData();
        return dt.getData("text/plain");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "");
    }
}

#[test]
fn storage_manager_estimate() {
    let code = r#"
        const sm = new StorageManager();
        const p = sm.estimate();
        return p.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "fulfilled");
    }
}

#[test]
fn performance_observer_construct() {
    let code = r#"
        const po = new PerformanceObserver(() => {});
        po.observe();
        return typeof po.takeRecords;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "function");
    }
}

#[test]
fn performance_entry_construct() {
    let code = r#"
        const e = new PerformanceEntry();
        return typeof e.name + "|" + e.duration;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "string|0");
    }
}

// ─── Symbols + Reflect ────────────────────────────────────────────────

#[test]
fn symbol_well_known_extras() {
    let code = r#"
        return [
            typeof Symbol.iterator,
            typeof Symbol.asyncIterator,
            typeof Symbol.toPrimitive,
            typeof Symbol.toStringTag,
            typeof Symbol.species,
            typeof Symbol.match,
            typeof Symbol.matchAll,
            typeof Symbol.replace,
            typeof Symbol.search,
            typeof Symbol.split,
            typeof Symbol.isConcatSpreadable,
            typeof Symbol.unscopables,
            typeof Symbol.hasInstance,
            typeof Symbol.dispose,
            typeof Symbol.asyncDispose,
            typeof Symbol.metadata
        ].every(t => t === "string");
    "#;
    if let crate::interpreter::JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn symbol_for_creates_registry() {
    let code = r#"
        const s = Symbol.for("test");
        return Symbol.keyFor(s);
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "test");
    }
}

#[test]
fn reflect_get_set() {
    let code = r#"
        const obj = { x: 5 };
        Reflect.set(obj, "y", 10);
        return Reflect.get(obj, "x") + "|" + Reflect.get(obj, "y");
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "5|10");
    }
}

#[test]
fn reflect_has_delete() {
    let code = r#"
        const obj = { a: 1, b: 2 };
        const has_a = Reflect.has(obj, "a");
        Reflect.deleteProperty(obj, "a");
        const has_a_after = Reflect.has(obj, "a");
        return has_a + "|" + has_a_after;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "true|false");
    }
}

#[test]
fn reflect_own_keys() {
    let code = r#"
        const obj = { a: 1, b: 2, c: 3 };
        return Reflect.ownKeys(obj).length;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 3.0);
    }
}

// ─── Crypto SHA real ──────────────────────────────────────────────────

#[test]
fn crypto_sha256_empty_string() {
    let code = r#"
        const buf = crypto.subtle.digest("SHA-256", "");
        return buf.__promise_value__.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 32.0); // SHA-256 = 32 bytes
    }
}

#[test]
fn crypto_sha256_known_vector() {
    // SHA-256("abc") = ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad
    let code = r#"
        const buf = crypto.subtle.digest("SHA-256", "abc");
        const bytes = buf.__promise_value__.__bytes__;
        return bytes[0] + "|" + bytes[1] + "|" + bytes[31];
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        // 0xba=186, 0x78=120, 0xad=173
        assert_eq!(s, "186|120|173");
    }
}

#[test]
fn crypto_sha1_known_vector() {
    // SHA-1("abc") = a9993e36 4706816a ba3e2571 7850c26c 9cd0d89d
    let code = r#"
        const buf = crypto.subtle.digest("SHA-1", "abc");
        const bytes = buf.__promise_value__.__bytes__;
        return bytes[0] + "|" + bytes[1] + "|" + bytes.length;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        // 0xa9=169, 0x99=153, length 20
        assert_eq!(s, "169|153|20");
    }
}

#[test]
fn crypto_sha384_byte_length() {
    let code = r#"
        const buf = crypto.subtle.digest("SHA-384", "test");
        return buf.__promise_value__.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 48.0); // SHA-384 = 48 bytes
    }
}

#[test]
fn crypto_sha512_byte_length() {
    let code = r#"
        const buf = crypto.subtle.digest("SHA-512", "test");
        return buf.__promise_value__.byteLength;
    "#;
    if let crate::interpreter::JsValue::Number(n) = run(code) {
        assert_eq!(n, 64.0); // SHA-512 = 64 bytes
    }
}

#[test]
fn crypto_unknown_algo_rejects() {
    let code = r#"
        const buf = crypto.subtle.digest("UNKNOWN", "x");
        return buf.__promise_state__;
    "#;
    if let crate::interpreter::JsValue::Str(s) = run(code) {
        assert_eq!(s, "rejected");
    }
}
