use super::*;
use crate::browser::html_parser::parse_html;

fn doc_root(html: &str) -> std::rc::Rc<crate::browser::dom::NodeData> {
    parse_html(html, "about:blank").root
}

#[test]
fn search_tag_simple() {
    let root = doc_root("<html><body><p>a</p><p>b</p><div>c</div></body></html>");
    let hits = search(&root, "p", SearchMode::Tag);
    assert_eq!(hits.len(), 2);
}

#[test]
fn search_class() {
    let root = doc_root(r#"<div class="foo"></div><span class="foo bar"></span><i class="other"></i>"#);
    let hits = search(&root, ".foo", SearchMode::Auto);
    assert_eq!(hits.len(), 2);
}

#[test]
fn search_id() {
    let root = doc_root(r#"<div id="main"></div><span></span>"#);
    let hits = search(&root, "#main", SearchMode::Auto);
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_tag_class() {
    let root = doc_root(r#"<div class="x"></div><span class="x"></span>"#);
    let hits = search(&root, "div.x", SearchMode::Auto);
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_attr_present() {
    let root = doc_root(r#"<a href="/a"></a><a></a>"#);
    let hits = search(&root, "[href]", SearchMode::Auto);
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_attr_eq() {
    let root = doc_root(r#"<input type="text"><input type="number"><input type="text">"#);
    let hits = search(&root, "input[type=text]", SearchMode::Auto);
    assert_eq!(hits.len(), 2);
}

#[test]
fn search_descendant() {
    let root = doc_root(r#"<div><p>x</p></div><p>y</p>"#);
    let hits = search(&root, "div p", SearchMode::Auto);
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_xpath_descendant() {
    let root = doc_root("<html><body><p>a</p><p>b</p></body></html>");
    let hits = search(&root, "//p", SearchMode::XPath);
    assert_eq!(hits.len(), 2);
}

#[test]
fn search_xpath_attr() {
    let root = doc_root(r#"<a href="/a"></a><a></a>"#);
    let hits = search(&root, "//a[@href]", SearchMode::XPath);
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_xpath_attr_value() {
    let root = doc_root(r#"<a href="/a"></a><a href="/b"></a>"#);
    let hits = search(&root, r#"//a[@href="/a"]"#, SearchMode::XPath);
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_empty_query() {
    let root = doc_root("<p></p>");
    let hits = search(&root, "", SearchMode::Auto);
    assert!(hits.is_empty());
}
