/// Testy pro DOM manipulaci API (NodeData mutace, traversal).

use crate::browser::dom::*;
use crate::browser::html_parser::parse_html;
use std::collections::HashMap;
use std::rc::Rc;

#[test]
fn new_element_has_tag() {
    let el = NodeData::new_element("div", HashMap::new());
    assert_eq!(el.tag_name().as_deref(), Some("div"));
}

#[test]
fn new_text_no_tag() {
    let t = NodeData::new_text("hello");
    assert!(t.tag_name().is_none());
    assert_eq!(t.text_content(), "hello");
}

#[test]
fn append_child_preserves_order() {
    let parent = NodeData::new_element("div", HashMap::new());
    let a = NodeData::new_element("a", HashMap::new());
    let b = NodeData::new_element("b", HashMap::new());
    let c = NodeData::new_element("c", HashMap::new());
    parent.append_child(a);
    parent.append_child(b);
    parent.append_child(c);
    let kids = parent.children.borrow();
    assert_eq!(kids.len(), 3);
    assert_eq!(kids[0].tag_name().as_deref(), Some("a"));
    assert_eq!(kids[1].tag_name().as_deref(), Some("b"));
    assert_eq!(kids[2].tag_name().as_deref(), Some("c"));
}

#[test]
fn set_attr_visible_via_attr_get() {
    let el = NodeData::new_element("div", HashMap::new());
    el.set_attr("id", "main");
    assert_eq!(el.attr("id").as_deref(), Some("main"));
}

#[test]
fn remove_attr_clears_value() {
    let mut attrs = HashMap::new();
    attrs.insert("class".into(), "foo".into());
    let el = NodeData::new_element("div", attrs);
    assert_eq!(el.attr("class").as_deref(), Some("foo"));
    el.remove_attr("class");
    assert!(el.attr("class").is_none());
}

#[test]
fn has_attr_works() {
    let mut attrs = HashMap::new();
    attrs.insert("data-x".into(), "1".into());
    let el = NodeData::new_element("div", attrs);
    assert!(el.has_attr("data-x"));
    assert!(!el.has_attr("data-y"));
}

#[test]
fn text_content_concatenates_descendants() {
    let parent = NodeData::new_element("div", HashMap::new());
    let t1 = NodeData::new_text("Hello ");
    let span = NodeData::new_element("span", HashMap::new());
    let t2 = NodeData::new_text("world");
    span.append_child(t2);
    parent.append_child(t1);
    parent.append_child(span);
    let text = parent.text_content();
    assert!(text.contains("Hello"));
    assert!(text.contains("world"));
}

#[test]
fn set_text_content_replaces_children() {
    let parent = NodeData::new_element("div", HashMap::new());
    parent.append_child(NodeData::new_element("span", HashMap::new()));
    parent.append_child(NodeData::new_element("a", HashMap::new()));
    parent.set_text_content("only text");
    assert_eq!(parent.text_content(), "only text");
}

#[test]
fn find_returns_first_match() {
    let doc = parse_html(r#"<html><body><div>1</div><div>2</div></body></html>"#, "");
    let first_div = doc.root.find(|n| n.tag_name().as_deref() == Some("div"));
    assert!(first_div.is_some());
    assert_eq!(first_div.unwrap().text_content().trim(), "1");
}

#[test]
fn find_returns_none_when_no_match() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let res = doc.root.find(|n| n.tag_name().as_deref() == Some("section"));
    assert!(res.is_none());
}

#[test]
fn walk_visits_all_descendants() {
    let doc = parse_html(r#"<html><body><div><p>x</p><span>y</span></div></body></html>"#, "");
    let mut count = 0;
    doc.root.walk(&mut |_| count += 1);
    assert!(count >= 5, "walk visit aspon 5 nodu, got {count}");
}

#[test]
fn get_elements_by_tag_recursive() {
    let doc = parse_html(r#"
        <html><body>
            <div><p>a</p></div>
            <p>b</p>
            <section><p>c</p></section>
        </body></html>
    "#, "");
    let ps = doc.root.get_elements_by_tag("p");
    assert_eq!(ps.len(), 3);
}

#[test]
fn get_elements_by_class_handles_multi_class() {
    let doc = parse_html(r#"<html><body><div class="x y z"></div></body></html>"#, "");
    assert_eq!(doc.root.get_elements_by_class("x").len(), 1);
    assert_eq!(doc.root.get_elements_by_class("y").len(), 1);
    assert_eq!(doc.root.get_elements_by_class("z").len(), 1);
    assert_eq!(doc.root.get_elements_by_class("nope").len(), 0);
}

#[test]
fn get_element_by_id_unique() {
    let doc = parse_html(r#"
        <html><body>
            <div id="a">A</div>
            <div id="b">B</div>
        </body></html>
    "#, "");
    let a = doc.root.get_element_by_id("a").unwrap();
    assert_eq!(a.text_content().trim(), "A");
    let b = doc.root.get_element_by_id("b").unwrap();
    assert_eq!(b.text_content().trim(), "B");
    assert!(doc.root.get_element_by_id("nope").is_none());
}

#[test]
fn document_helpers_html_body_head() {
    let doc = parse_html(r#"<html><head><title>T</title></head><body><p>x</p></body></html>"#, "");
    assert!(doc.html_element().is_some());
    assert!(doc.body().is_some());
    assert!(doc.head().is_some());
}

#[test]
fn document_title_extracted() {
    let doc = parse_html(r#"<html><head><title>Hello World</title></head><body></body></html>"#, "");
    assert_eq!(doc.title, "Hello World");
}

#[test]
fn empty_html_creates_minimal_doc() {
    let doc = parse_html("", "");
    // Mel by stale fungovat - prinejmensim html element po html5ever auto-fixu
    let _ = doc.root;
}

#[test]
fn rc_strong_count_after_append() {
    let parent = NodeData::new_element("div", HashMap::new());
    let child = NodeData::new_element("span", HashMap::new());
    let count_before = Rc::strong_count(&child);
    parent.append_child(Rc::clone(&child));
    let count_after = Rc::strong_count(&child);
    assert!(count_after > count_before, "append zvysuje refcount");
}
