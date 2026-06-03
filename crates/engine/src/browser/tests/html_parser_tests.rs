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

#[test]
fn parse_self_closing_void_element() {
    let doc = parse_html(r#"<html><body><img src="x.png"><br><hr></body></html>"#, "test://");
    let img = doc.root.get_elements_by_tag("img");
    assert_eq!(img.len(), 1);
    assert_eq!(img[0].attr("src").as_deref(), Some("x.png"));
    let br = doc.root.get_elements_by_tag("br");
    assert_eq!(br.len(), 1);
}

#[test]
fn parse_multiple_classes_in_attribute() {
    let doc = parse_html(r#"<html><body><div class="alpha beta gamma">x</div></body></html>"#, "test://");
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let class = div.attr("class").unwrap();
    assert!(class.contains("alpha"));
    assert!(class.contains("beta"));
    assert!(class.contains("gamma"));
}

#[test]
fn parse_inline_style_attribute() {
    let doc = parse_html(r#"<html><body><div style="color: red; font-size: 12px">x</div></body></html>"#, "test://");
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let style = div.attr("style").unwrap();
    assert!(style.contains("color"));
}

#[test]
fn parse_data_attributes_preserved() {
    let doc = parse_html(r#"<html><body><div data-id="42" data-info="hi">x</div></body></html>"#, "test://");
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert_eq!(div.attr("data-id").as_deref(), Some("42"));
    assert_eq!(div.attr("data-info").as_deref(), Some("hi"));
}

#[test]
fn parse_html_entities() {
    let doc = parse_html(r#"<html><body><p>A &amp; B &lt;C&gt;</p></body></html>"#, "test://");
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let text = p.text_content();
    assert!(text.contains("&"));
    assert!(text.contains("<"));
    assert!(text.contains(">"));
}

#[test]
fn parse_script_tag_kept() {
    let doc = parse_html(r#"<html><body><script>var x=1;</script></body></html>"#, "test://");
    let scripts = doc.root.get_elements_by_tag("script");
    assert_eq!(scripts.len(), 1);
}

#[test]
fn parse_form_with_input() {
    let doc = parse_html(r#"
        <html><body><form action="/submit">
            <input type="text" name="user" value="default">
            <input type="email" name="mail">
            <button type="submit">Send</button>
        </form></body></html>
    "#, "test://");
    let inputs = doc.root.get_elements_by_tag("input");
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].attr("type").as_deref(), Some("text"));
    assert_eq!(inputs[0].attr("value").as_deref(), Some("default"));
}

#[test]
fn parse_table_with_rows() {
    let doc = parse_html(r#"
        <html><body><table>
            <thead><tr><th>A</th><th>B</th></tr></thead>
            <tbody>
                <tr><td>1</td><td>2</td></tr>
                <tr><td>3</td><td>4</td></tr>
            </tbody>
        </table></body></html>
    "#, "test://");
    let trs = doc.root.get_elements_by_tag("tr");
    assert_eq!(trs.len(), 3);
    let tds = doc.root.get_elements_by_tag("td");
    assert_eq!(tds.len(), 4);
}

#[test]
fn parse_comment_skipped_from_text() {
    let doc = parse_html(r#"<html><body><p>Before<!-- comment -->After</p></body></html>"#, "test://");
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let text = p.text_content();
    // Komentar by se nemel objevit v text_content
    assert!(!text.contains("comment"));
    assert!(text.contains("Before"));
    assert!(text.contains("After"));
}

#[test]
fn parse_empty_element() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "test://");
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert_eq!(div.text_content().trim(), "");
}

#[test]
fn parse_meta_charset() {
    let doc = parse_html(r#"<html><head><meta charset="UTF-8"></head><body></body></html>"#, "test://");
    let metas = doc.root.get_elements_by_tag("meta");
    assert_eq!(metas.len(), 1);
}

#[test]
fn parse_link_stylesheet() {
    let doc = parse_html(r#"<html><head><link rel="stylesheet" href="style.css"></head><body></body></html>"#, "test://");
    let links = doc.root.get_elements_by_tag("link");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].attr("href").as_deref(), Some("style.css"));
}
