/// Tier 2 DOM API - manipulation + advanced events.
///
/// Pokryva: insertBefore, replaceChild, insertAdjacentElement,
/// cloneNode (deep + shallow real), removeEventListener real,
/// document.activeElement, createDocumentFragment real.

use super::helpers::*;
use crate::interpreter::JsValue;

// --- Item 1: insertBefore ------------------------------------------------

#[test]
fn insert_before_inserts_at_idx() {
    let v = run(r#"
        const parent = document.createElement("div");
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("span"); b.setAttribute("data-mark","b");
        const c = document.createElement("span"); c.setAttribute("data-mark","c");
        parent.appendChild(a);
        parent.appendChild(c);
        parent.insertBefore(b, c);
        return parent.children.length + ":" +
            parent.children[0].getAttribute("data-mark") + "," +
            parent.children[1].getAttribute("data-mark") + "," +
            parent.children[2].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "3:a,b,c");
}

#[test]
fn insert_before_null_appends() {
    // refNode == null -> append na konec.
    let v = run(r#"
        const parent = document.createElement("div");
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("span"); b.setAttribute("data-mark","b");
        parent.appendChild(a);
        parent.insertBefore(b, null);
        return parent.children.length + ":" +
            parent.children[0].getAttribute("data-mark") + "," +
            parent.children[1].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "2:a,b");
}

#[test]
fn insert_before_returns_new_node() {
    let v = run(r#"
        const parent = document.createElement("div");
        const child = document.createElement("span");
        const refn = document.createElement("p");
        parent.appendChild(refn);
        const ret = parent.insertBefore(child, refn);
        return ret === child;
    "#);
    assert_eq!(as_bool(v), true);
}

// --- Item 2: replaceChild ------------------------------------------------

#[test]
fn replace_child_swaps_node() {
    let v = run(r#"
        const parent = document.createElement("div");
        const oldn = document.createElement("span"); oldn.setAttribute("data-mark","old");
        const newn = document.createElement("p"); newn.setAttribute("data-mark","new");
        parent.appendChild(oldn);
        const ret = parent.replaceChild(newn, oldn);
        return parent.children.length + ":" +
            parent.children[0].tagName + ":" + (ret === oldn);
    "#);
    assert_eq!(as_str(v), "1:P:true");
}

#[test]
fn replace_child_preserves_position() {
    let v = run(r#"
        const parent = document.createElement("div");
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("span"); b.setAttribute("data-mark","b");
        const c = document.createElement("span"); c.setAttribute("data-mark","c");
        const x = document.createElement("p"); x.setAttribute("data-mark","x");
        parent.appendChild(a); parent.appendChild(b); parent.appendChild(c);
        parent.replaceChild(x, b);
        return parent.children[0].getAttribute("data-mark") + "," +
            parent.children[1].getAttribute("data-mark") + "," +
            parent.children[2].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "a,x,c");
}

// --- Item 3: insertAdjacentHTML (existing - smoke test) ------------------

#[test]
fn insert_adjacent_html_beforeend() {
    let v = run(r#"
        const parent = document.createElement("div");
        parent.insertAdjacentHTML("beforeend", "<span>hello</span>");
        return parent.children.length + ":" + parent.children[0].tagName;
    "#);
    assert_eq!(as_str(v), "1:SPAN");
}

// --- Item 4: insertAdjacentElement ---------------------------------------

#[test]
fn insert_adjacent_element_beforeend() {
    let v = run(r#"
        const parent = document.createElement("div");
        const child = document.createElement("span");
        child.setAttribute("data-mark","x");
        parent.insertAdjacentElement("beforeend", child);
        return parent.children.length + ":" +
            parent.children[0].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "1:x");
}

#[test]
fn insert_adjacent_element_afterbegin() {
    let v = run(r#"
        const parent = document.createElement("div");
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("span"); b.setAttribute("data-mark","b");
        parent.appendChild(a);
        parent.insertAdjacentElement("afterbegin", b);
        return parent.children[0].getAttribute("data-mark") + "," +
            parent.children[1].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "b,a");
}

#[test]
fn insert_adjacent_element_before_after() {
    let v = run(r#"
        const root = document.createElement("div");
        const ref_n = document.createElement("span"); ref_n.setAttribute("data-mark","R");
        const before = document.createElement("p"); before.setAttribute("data-mark","B");
        const after = document.createElement("p"); after.setAttribute("data-mark","A");
        root.appendChild(ref_n);
        ref_n.insertAdjacentElement("beforebegin", before);
        ref_n.insertAdjacentElement("afterend", after);
        return root.children[0].getAttribute("data-mark") + "," +
            root.children[1].getAttribute("data-mark") + "," +
            root.children[2].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "B,R,A");
}

// --- Item 5: cloneNode (deep + shallow) ---------------------------------

#[test]
fn clone_node_shallow_no_children() {
    let v = run(r#"
        const orig = document.createElement("div");
        orig.setAttribute("id", "x");
        const child = document.createElement("span");
        orig.appendChild(child);
        const c = orig.cloneNode(false);
        return c.tagName + ":" + c.getAttribute("id") + ":" + c.children.length;
    "#);
    assert_eq!(as_str(v), "DIV:x:0");
}

#[test]
fn clone_node_deep_includes_children() {
    let v = run(r#"
        const orig = document.createElement("div");
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("p"); b.setAttribute("data-mark","b");
        orig.appendChild(a); orig.appendChild(b);
        const c = orig.cloneNode(true);
        return c.children.length + ":" +
            c.children[0].getAttribute("data-mark") + "," +
            c.children[1].getAttribute("data-mark");
    "#);
    assert_eq!(as_str(v), "2:a,b");
}

#[test]
fn clone_node_deep_modify_original_unaffected() {
    let v = run(r#"
        const orig = document.createElement("div");
        orig.setAttribute("id", "orig");
        const c = orig.cloneNode(true);
        orig.setAttribute("id", "changed");
        return c.getAttribute("id");
    "#);
    assert_eq!(as_str(v), "orig");
}

#[test]
fn clone_node_attrs_preserved() {
    let v = run(r#"
        const orig = document.createElement("div");
        orig.setAttribute("class", "foo bar");
        orig.setAttribute("data-x", "42");
        const c = orig.cloneNode(false);
        return c.getAttribute("class") + ":" + c.getAttribute("data-x");
    "#);
    assert_eq!(as_str(v), "foo bar:42");
}

// --- Item 6: removeEventListener real ------------------------------------

#[test]
fn remove_event_listener_prevents_dispatch() {
    let v = run(r#"
        let calls = 0;
        const el = document.createElement("div");
        function handler() { calls++; }
        el.addEventListener("click", handler);
        el.removeEventListener("click", handler);
        el.dispatchEvent({ type: "click" });
        return calls;
    "#);
    assert_eq!(v.to_number() as i64, 0);
}

#[test]
fn remove_event_listener_only_removes_matched() {
    let v = run(r#"
        let a_calls = 0;
        let b_calls = 0;
        const el = document.createElement("div");
        function h_a() { a_calls++; }
        function h_b() { b_calls++; }
        el.addEventListener("click", h_a);
        el.addEventListener("click", h_b);
        el.removeEventListener("click", h_a);
        el.dispatchEvent({ type: "click" });
        return a_calls + ":" + b_calls;
    "#);
    assert_eq!(as_str(v), "0:1");
}

// --- Item 7: document.activeElement -------------------------------------

#[test]
fn active_element_default_body() {
    // Pred jakymkoli focus, activeElement = body (DOM spec default).
    let v = run(r#"
        return document.activeElement.tagName;
    "#);
    assert_eq!(as_str(v), "BODY");
}

#[test]
fn active_element_after_focus() {
    let v = run(r#"
        const inp = document.createElement("input");
        document.body.appendChild(inp);
        inp.focus();
        return document.activeElement === inp;
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn active_element_after_blur() {
    let v = run(r#"
        const inp = document.createElement("input");
        document.body.appendChild(inp);
        inp.focus();
        inp.blur();
        return document.activeElement.tagName;
    "#);
    assert_eq!(as_str(v), "BODY");
}

// --- Item 8: createDocumentFragment real --------------------------------

#[test]
fn document_fragment_basic() {
    let v = run(r#"
        const frag = document.createDocumentFragment();
        return frag.nodeType + ":" + frag.nodeName;
    "#);
    assert_eq!(as_str(v), "11:#document-fragment");
}

#[test]
fn document_fragment_append_child() {
    let v = run(r#"
        const frag = document.createDocumentFragment();
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("p"); b.setAttribute("data-mark","b");
        frag.appendChild(a);
        frag.appendChild(b);
        return frag.children.length;
    "#);
    assert_eq!(v.to_number() as i64, 2);
}

#[test]
fn document_fragment_inserts_children_into_parent() {
    // appendChild(fragment) presune deti fragmentu do parenta (spec).
    let v = run(r#"
        const parent = document.createElement("div");
        const frag = document.createDocumentFragment();
        const a = document.createElement("span"); a.setAttribute("data-mark","a");
        const b = document.createElement("p"); b.setAttribute("data-mark","b");
        frag.appendChild(a); frag.appendChild(b);
        parent.appendChild(frag);
        return parent.children.length + ":" +
            parent.children[0].getAttribute("data-mark") + "," +
            parent.children[1].getAttribute("data-mark") + ":" +
            frag.children.length;
    "#);
    // Po insertu by mel frag byt prazdny.
    assert_eq!(as_str(v), "2:a,b:0");
}
