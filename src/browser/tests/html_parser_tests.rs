/// Testy HTML parseru.

use crate::browser::html_parser::*;
use crate::browser::dom::NodeKind;

#[test]
fn parse_basic_html() {
    let doc = parse_html("<html><body><h1>Hello</h1></body></html>", "test://");
    assert!(doc.body().is_some());
    assert!(doc.html_element().is_some());
}

#[test]
fn parse_extracts_title() {
    let doc = parse_html("<html><head><title>My Page</title></head><body></body></html>", "test://");
    assert_eq!(doc.title, "My Page");
}

#[test]
fn parse_text_content() {
    let doc = parse_html("<html><body><p>Hello World</p></body></html>", "test://");
    let body = doc.body().unwrap();
    assert!(body.text_content().contains("Hello World"));
}

#[test]
fn parse_attributes() {
    let doc = parse_html(r#"<html><body><div id="main" class="container">x</div></body></html>"#, "test://");
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert_eq!(div.attr("id").as_deref(), Some("main"));
    assert_eq!(div.attr("class").as_deref(), Some("container"));
}

#[test]
fn parse_nested_elements() {
    let doc = parse_html(r#"
        <html><body>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
                <li>Item 3</li>
            </ul>
        </body></html>
    "#, "test://");
    let ul = doc.root.find(|n| n.tag_name().as_deref() == Some("ul")).unwrap();
    let lis: Vec<_> = ul.children.borrow().iter()
        .filter(|c| c.tag_name().as_deref() == Some("li"))
        .cloned().collect();
    assert_eq!(lis.len(), 3);
}

#[test]
fn parse_doctype() {
    let doc = parse_html("<!DOCTYPE html><html></html>", "test://");
    let has_doctype = doc.root.find(|n| matches!(n.kind, NodeKind::DocType(_))).is_some();
    assert!(has_doctype);
}

#[test]
fn get_element_by_id() {
    let doc = parse_html(r#"<html><body><div id="target">found</div></body></html>"#, "test://");
    let el = doc.root.get_element_by_id("target").unwrap();
    assert_eq!(el.text_content().trim(), "found");
}

#[test]
fn get_elements_by_tag() {
    let doc = parse_html(r#"
        <html><body>
            <p>one</p><p>two</p><p>three</p>
        </body></html>
    "#, "test://");
    let ps = doc.root.get_elements_by_tag("p");
    assert_eq!(ps.len(), 3);
}

#[test]
fn get_elements_by_class() {
    let doc = parse_html(r#"
        <html><body>
            <div class="a">1</div>
            <div class="a b">2</div>
            <div class="b">3</div>
        </body></html>
    "#, "test://");
    let a = doc.root.get_elements_by_class("a");
    assert_eq!(a.len(), 2);
    let b = doc.root.get_elements_by_class("b");
    assert_eq!(b.len(), 2);
}
