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
fn cascade_css_variable() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet(r#"
        :root { --primary: blue; }
        p { color: var(--primary); }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn cascade_css_variable_fallback() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet(r#"
        p { color: var(--missing, red); }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("red"));
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

// ─── Selectors L4 ──────────────────────────────────────────────────────

#[test]
fn selector_is_matches_any() {
    let doc = parse_html("<html><body><p>a</p><h1>b</h1><div>c</div></body></html>", "");
    let css = parse_stylesheet(":is(p, h1) { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let h1 = doc.root.find(|n| n.tag_name().as_deref() == Some("h1")).unwrap();
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert_eq!(cascade::get_styles(&map, &p).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert_eq!(cascade::get_styles(&map, &h1).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert!(cascade::get_styles(&map, &div).unwrap().get("color").is_none());
}

#[test]
fn selector_where_zero_specificity() {
    // :where(.high) ma specificitu 0, takze .low (specificita 1) vyhraje
    let doc = parse_html(r#"<html><body><p class="low">x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        :where(p) { color: red; }
        .low { color: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p).unwrap();
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn selector_not_excludes() {
    let doc = parse_html(r#"<html><body><p class="a">a</p><p>b</p></body></html>"#, "");
    let css = parse_stylesheet("p:not(.a) { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let ps: Vec<_> = doc.root.get_elements_by_tag("p");
    let with_class = ps.iter().find(|p| p.attr("class").as_deref() == Some("a")).unwrap();
    let without = ps.iter().find(|p| p.attr("class").is_none()).unwrap();
    assert!(cascade::get_styles(&map, with_class).unwrap().get("color").is_none());
    assert_eq!(cascade::get_styles(&map, without).unwrap().get("color").map(|s| s.as_str()), Some("red"));
}

#[test]
fn selector_has_descendant() {
    let doc = parse_html("<html><body><div><span>x</span></div><div>y</div></body></html>", "");
    let css = parse_stylesheet("div:has(span) { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let divs = doc.root.get_elements_by_tag("div");
    let with_span = divs.iter().find(|d| !d.get_elements_by_tag("span").is_empty()).unwrap();
    let without = divs.iter().find(|d| d.get_elements_by_tag("span").is_empty()).unwrap();
    assert_eq!(cascade::get_styles(&map, with_span).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert!(cascade::get_styles(&map, without).unwrap().get("color").is_none());
}

#[test]
fn selector_general_sibling() {
    let doc = parse_html("<html><body><h1>x</h1><p>1</p><span>s</span><p>2</p></body></html>", "");
    let css = parse_stylesheet("h1 ~ p { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let ps = doc.root.get_elements_by_tag("p");
    for p in &ps {
        assert_eq!(cascade::get_styles(&map, p).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    }
}

#[test]
fn selector_nth_child() {
    let doc = parse_html("<html><body><ul><li>1</li><li>2</li><li>3</li><li>4</li></ul></body></html>", "");
    let css = parse_stylesheet("li:nth-child(2n) { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let lis = doc.root.get_elements_by_tag("li");
    // 2. a 4. li (1-based) maji byt cervene
    assert!(cascade::get_styles(&map, &lis[0]).unwrap().get("color").is_none());
    assert_eq!(cascade::get_styles(&map, &lis[1]).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert!(cascade::get_styles(&map, &lis[2]).unwrap().get("color").is_none());
    assert_eq!(cascade::get_styles(&map, &lis[3]).unwrap().get("color").map(|s| s.as_str()), Some("red"));
}

#[test]
fn selector_first_of_type() {
    let doc = parse_html("<html><body><h1>a</h1><p>1</p><p>2</p></body></html>", "");
    let css = parse_stylesheet("p:first-of-type { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let ps = doc.root.get_elements_by_tag("p");
    assert_eq!(cascade::get_styles(&map, &ps[0]).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert!(cascade::get_styles(&map, &ps[1]).unwrap().get("color").is_none());
}

// ─── Values L4: min/max/clamp/env ──────────────────────────────────────

#[test]
fn resolve_min_picks_smallest() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("min(20px, 50px, 30px)", &vars);
    assert_eq!(r, "20px");
}

#[test]
fn resolve_max_picks_largest() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("max(20px, 50px, 30px)", &vars);
    assert_eq!(r, "50px");
}

#[test]
fn resolve_clamp_within_range() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("clamp(10px, 20px, 30px)", &vars);
    assert_eq!(r, "20px");
}

#[test]
fn resolve_clamp_above_max() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("clamp(10px, 100px, 30px)", &vars);
    assert_eq!(r, "30px");
}

#[test]
fn resolve_clamp_below_min() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("clamp(10px, 5px, 30px)", &vars);
    assert_eq!(r, "10px");
}

#[test]
fn resolve_min_with_var() {
    let mut vars = std::collections::HashMap::new();
    vars.insert("--small".to_string(), "10px".to_string());
    let r = cascade::resolve_value("min(var(--small), 50px)", &vars);
    assert_eq!(r, "10px");
}

#[test]
fn resolve_env_fallback() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("env(safe-area-inset-top, 12px)", &vars);
    assert_eq!(r, "12px");
}

#[test]
fn resolve_env_no_fallback_returns_zero() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("env(unknown)", &vars);
    assert_eq!(r, "0px");
}

#[test]
fn resolve_nested_clamp_inside_calc() {
    let vars = std::collections::HashMap::new();
    // calc(10px + clamp(5px, 20px, 30px)) -> calc(10px + 20px) -> 30px
    let r = cascade::resolve_value("calc(10px + clamp(5px, 20px, 30px))", &vars);
    assert_eq!(r, "30px");
}

#[test]
fn selector_empty() {
    let doc = parse_html("<html><body><div></div><div>x</div></body></html>", "");
    let css = parse_stylesheet("div:empty { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let divs = doc.root.get_elements_by_tag("div");
    assert_eq!(cascade::get_styles(&map, &divs[0]).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert!(cascade::get_styles(&map, &divs[1]).unwrap().get("color").is_none());
}
