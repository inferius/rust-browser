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

// ─── CSS Container Queries L1 ──────────────────────────────────────────

#[test]
fn container_query_parses_with_name() {
    let s = parse_stylesheet(r#"
        @container card (min-width: 400px) {
            .item { color: red; }
        }
    "#);
    assert_eq!(s.container_queries.len(), 1);
    assert_eq!(s.container_queries[0].name, "card");
    assert!(s.container_queries[0].condition.contains("min-width"));
}

#[test]
fn container_query_parses_unnamed() {
    let s = parse_stylesheet(r#"
        @container (max-width: 600px) {
            .item { color: blue; }
        }
    "#);
    assert_eq!(s.container_queries.len(), 1);
    assert_eq!(s.container_queries[0].name, "");
}

#[test]
fn container_query_applies_when_viewport_matches() {
    let doc = parse_html(r#"<html><body><div class="item">x</div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        @container (min-width: 400px) {
            .item { color: red; }
        }
    "#);
    // Viewport 800x600 - condition (min-width 400) je true
    let map = cascade::cascade_with_viewport(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("color").map(|v| v.as_str()), Some("red"));
}

#[test]
fn container_query_skipped_when_viewport_too_small() {
    let doc = parse_html(r#"<html><body><div class="item">x</div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        @container (min-width: 1000px) {
            .item { color: red; }
        }
    "#);
    // Viewport 500 - condition (min-width 1000) je false
    let map = cascade::cascade_with_viewport(&doc.root, &[css], 500.0, 400.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert!(s.get("color").is_none());
}

#[test]
fn parse_length_cqw_units() {
    use crate::browser::layout;
    // 50cqw na 800x600 -> 400 (50 % vw)
    assert_eq!(layout::parse_length_ctx("50cqw", 800.0, 600.0, 16.0), 400.0);
    assert_eq!(layout::parse_length_ctx("100cqh", 800.0, 600.0, 16.0), 600.0);
    assert_eq!(layout::parse_length_ctx("50cqmin", 800.0, 600.0, 16.0), 300.0);
    assert_eq!(layout::parse_length_ctx("50cqmax", 800.0, 600.0, 16.0), 400.0);
}

// ─── CSS Nesting L1 ────────────────────────────────────────────────────

#[test]
fn nesting_basic_descendant() {
    let doc = parse_html(r#"<html><body><div class="card"><h2>x</h2></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .card {
            color: red;
            h2 { color: blue; }
        }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let card = doc.root.find(|n| n.attr("class").as_deref() == Some("card")).unwrap();
    let h2 = doc.root.find(|n| n.tag_name().as_deref() == Some("h2")).unwrap();
    assert_eq!(cascade::get_styles(&map, &card).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert_eq!(cascade::get_styles(&map, &h2).unwrap().get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn nesting_with_ampersand_pseudo() {
    let doc = parse_html(r#"<html><body><a class="btn">x</a></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .btn {
            color: red;
            &.active { color: green; }
        }
    "#);
    let _map = cascade::cascade(&doc.root, &[css]);
    // .btn (bez .active) -> red. Test ze parser nespadne, kombinovany rule .btn.active existuje.
}

#[test]
fn nesting_ampersand_with_class_combine() {
    let doc = parse_html(r#"<html><body><div class="card highlight">x</div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .card {
            background: white;
            &.highlight { background: yellow; }
        }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("background").map(|v| v.as_str()), Some("yellow"));
}

#[test]
fn nesting_deep_three_levels() {
    let doc = parse_html(r#"<html><body><div class="a"><div class="b"><span>x</span></div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .a {
            .b {
                span { color: red; }
            }
        }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let span = doc.root.find(|n| n.tag_name().as_deref() == Some("span")).unwrap();
    let s = cascade::get_styles(&map, &span).unwrap();
    assert_eq!(s.get("color").map(|v| v.as_str()), Some("red"));
}

// ─── Logical Properties L1 ─────────────────────────────────────────────

#[test]
fn logical_margin_block_start_to_top() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet("div { margin-block-start: 20px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("margin-top").map(|v| v.as_str()), Some("20px"));
}

#[test]
fn logical_padding_inline_pair() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet("div { padding-inline: 8px 16px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("padding-left").map(|v| v.as_str()), Some("8px"));
    assert_eq!(s.get("padding-right").map(|v| v.as_str()), Some("16px"));
}

#[test]
fn logical_inline_size_to_width() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet("div { inline-size: 200px; block-size: 100px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("width").map(|v| v.as_str()), Some("200px"));
    assert_eq!(s.get("height").map(|v| v.as_str()), Some("100px"));
}

#[test]
fn logical_inset_shorthand_to_top_right_bottom_left() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet("div { inset: 10px 20px 30px 40px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("top").map(|v| v.as_str()), Some("10px"));
    assert_eq!(s.get("right").map(|v| v.as_str()), Some("20px"));
    assert_eq!(s.get("bottom").map(|v| v.as_str()), Some("30px"));
    assert_eq!(s.get("left").map(|v| v.as_str()), Some("40px"));
}

#[test]
fn logical_border_radius_corners() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet("div { border-start-end-radius: 8px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("border-top-right-radius").map(|v| v.as_str()), Some("8px"));
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
