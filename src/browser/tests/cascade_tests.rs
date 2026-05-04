/// Testy CSS cascade.

use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};

#[test]
fn cascade_simple_match() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);

    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("red"));
}

#[test]
fn cascade_id_overrides_class() {
    let doc = parse_html(r#"<html><body><div id="main" class="box">x</div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .box { color: red; }
        #main { color: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn cascade_inline_overrides_external() {
    let doc = parse_html(r#"<html><body><p style="color: green;">x</p></body></html>"#, "");
    let css = parse_stylesheet("p { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("green"));
}

#[test]
fn cascade_descendant_selector() {
    let doc = parse_html("<html><body><div><p>x</p></div></body></html>", "");
    let css = parse_stylesheet("div p { color: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn cascade_universal_selector() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("* { color: green; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("green"));
}

#[test]
fn cascade_attribute_exists() {
    let doc = parse_html(r#"<html><body><a href="/x">link</a></body></html>"#, "");
    let css = parse_stylesheet("[href] { color: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let a = doc.root.find(|n| n.tag_name().as_deref() == Some("a")).unwrap();
    let styles = cascade::get_styles(&map, &a).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn cascade_attribute_equals() {
    let doc = parse_html(r#"<html><body><input type="text"/></body></html>"#, "");
    let css = parse_stylesheet(r#"[type="text"] { background: yellow; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let inp = doc.root.find(|n| n.tag_name().as_deref() == Some("input")).unwrap();
    let styles = cascade::get_styles(&map, &inp).unwrap();
    assert_eq!(styles.get("background").map(|s| s.as_str()), Some("yellow"));
}

#[test]
fn cascade_pseudo_first_child() {
    let doc = parse_html("<html><body><p>first</p><p>second</p></body></html>", "");
    let css = parse_stylesheet("p:first-child { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    // Najdi prvni a druhe p
    let body = doc.root.find(|n| n.tag_name().as_deref() == Some("body")).unwrap();
    let ps: Vec<_> = body.children.borrow().iter()
        .filter(|c| c.tag_name().as_deref() == Some("p"))
        .cloned().collect();
    let first_styles = cascade::get_styles(&map, &ps[0]).unwrap();
    assert_eq!(first_styles.get("color").map(|s| s.as_str()), Some("red"));
    let second_styles = cascade::get_styles(&map, &ps[1]);
    assert!(second_styles.map(|s| !s.contains_key("color")).unwrap_or(true));
}

#[test]
fn cascade_no_match() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("h1 { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert!(styles.get("color").is_none());
}
