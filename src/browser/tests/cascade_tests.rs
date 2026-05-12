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
fn selector_nth_child_negative_a() {
    // :nth-child(-n+3) = prvni 3 deti. Test ze parse_an_plus_b zvlada
    // "-n+3" pattern (a_str = "-", a = -1; rest = "+3", b = 3).
    let doc = parse_html("<html><body><ul><li>1</li><li>2</li><li>3</li><li>4</li><li>5</li></ul></body></html>", "");
    let css = parse_stylesheet("li:nth-child(-n+3) { color: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let lis = doc.root.get_elements_by_tag("li");
    assert_eq!(cascade::get_styles(&map, &lis[0]).unwrap().get("color").map(|s| s.as_str()), Some("blue"));
    assert_eq!(cascade::get_styles(&map, &lis[1]).unwrap().get("color").map(|s| s.as_str()), Some("blue"));
    assert_eq!(cascade::get_styles(&map, &lis[2]).unwrap().get("color").map(|s| s.as_str()), Some("blue"));
    assert!(cascade::get_styles(&map, &lis[3]).unwrap().get("color").is_none());
    assert!(cascade::get_styles(&map, &lis[4]).unwrap().get("color").is_none());
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

// ─── place-* + gap shorthandy ──────────────────────────────────────────

#[test]
fn place_items_shorthand_expands() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { place-items: center start; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("align-items").map(|v| v.as_str()), Some("center"));
    assert_eq!(s.get("justify-items").map(|v| v.as_str()), Some("start"));
}

#[test]
fn place_content_single_value_both() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { place-content: center; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("align-content").map(|v| v.as_str()), Some("center"));
    assert_eq!(s.get("justify-content").map(|v| v.as_str()), Some("center"));
}

#[test]
fn gap_shorthand_two_values() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { gap: 10px 20px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let s = cascade::get_styles(&map, &div).unwrap();
    assert_eq!(s.get("row-gap").map(|v| v.as_str()), Some("10px"));
    assert_eq!(s.get("column-gap").map(|v| v.as_str()), Some("20px"));
}

// ─── Form pseudo-classes ───────────────────────────────────────────────

#[test]
fn pseudo_required_matches_required_input() {
    let doc = parse_html(r#"<html><body><input required><input></body></html>"#, "");
    let css = parse_stylesheet("input:required { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let inputs = doc.root.get_elements_by_tag("input");
    let req = &inputs[0];
    let opt = &inputs[1];
    assert_eq!(cascade::get_styles(&map, req).unwrap().get("color").map(|s| s.as_str()), Some("red"));
    assert!(cascade::get_styles(&map, opt).unwrap().get("color").is_none());
}

#[test]
fn pseudo_disabled_matches_disabled() {
    let doc = parse_html(r#"<html><body><button disabled>x</button><button>y</button></body></html>"#, "");
    let css = parse_stylesheet("button:disabled { color: gray; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let buttons = doc.root.get_elements_by_tag("button");
    assert_eq!(cascade::get_styles(&map, &buttons[0]).unwrap().get("color").map(|s| s.as_str()), Some("gray"));
    assert!(cascade::get_styles(&map, &buttons[1]).unwrap().get("color").is_none());
}

#[test]
fn pseudo_checked_matches_input() {
    let doc = parse_html(r#"<html><body><input type="checkbox" checked><input type="checkbox"></body></html>"#, "");
    let css = parse_stylesheet("input:checked { color: green; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let inputs = doc.root.get_elements_by_tag("input");
    assert_eq!(cascade::get_styles(&map, &inputs[0]).unwrap().get("color").map(|s| s.as_str()), Some("green"));
    assert!(cascade::get_styles(&map, &inputs[1]).unwrap().get("color").is_none());
}

#[test]
fn pseudo_placeholder_shown_matches_empty_value() {
    let doc = parse_html(r#"<html><body>
        <input placeholder="hint" value="">
        <input placeholder="hint" value="filled">
    </body></html>"#, "");
    let css = parse_stylesheet(r#"input:placeholder-shown { color: gray; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let inputs = doc.root.get_elements_by_tag("input");
    assert_eq!(cascade::get_styles(&map, &inputs[0]).unwrap().get("color").map(|s| s.as_str()), Some("gray"));
    assert!(cascade::get_styles(&map, &inputs[1]).unwrap().get("color").is_none());
}

// ─── @media L4/L5 prefers-* ────────────────────────────────────────────

#[test]
fn media_prefers_color_scheme_light_default() {
    use crate::browser::css_parser::evaluate_media_query;
    // Default = light, takze dark fail
    assert!(!evaluate_media_query("(prefers-color-scheme: dark)", 1024.0, 768.0));
    assert!(evaluate_media_query("(prefers-color-scheme: light)", 1024.0, 768.0));
}

#[test]
fn media_hover_default_available() {
    use crate::browser::css_parser::evaluate_media_query;
    assert!(evaluate_media_query("(hover: hover)", 1024.0, 768.0));
    assert!(!evaluate_media_query("(hover: none)", 1024.0, 768.0));
}

#[test]
fn media_pointer_fine_default() {
    use crate::browser::css_parser::evaluate_media_query;
    assert!(evaluate_media_query("(pointer: fine)", 1024.0, 768.0));
    assert!(!evaluate_media_query("(pointer: coarse)", 1024.0, 768.0));
    assert!(!evaluate_media_query("(pointer: none)", 1024.0, 768.0));
}

#[test]
fn media_reduced_motion_default_false() {
    use crate::browser::css_parser::evaluate_media_query;
    assert!(!evaluate_media_query("(prefers-reduced-motion: reduce)", 1024.0, 768.0));
    assert!(evaluate_media_query("(prefers-reduced-motion: no-preference)", 1024.0, 768.0));
}

// ─── Cascade Layers @layer ─────────────────────────────────────────────

#[test]
fn cascade_layer_order_declared() {
    let s = parse_stylesheet(r#"
        @layer reset, theme, components;
    "#);
    assert_eq!(s.layer_order, vec!["reset", "theme", "components"]);
}

#[test]
fn cascade_layer_block_rules_lower_prio_than_unlayered() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        @layer base {
            p { color: red; }
        }
        p { color: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let s = cascade::get_styles(&map, &p).unwrap();
    // Unlayered ma vyssi prio
    assert_eq!(s.get("color").map(|v| v.as_str()), Some("blue"));
}

#[test]
fn cascade_layer_later_wins_over_earlier() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        @layer first {
            p { color: red; }
        }
        @layer second {
            p { color: green; }
        }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let s = cascade::get_styles(&map, &p).unwrap();
    // second je pozdejsi -> vyssi prio v rame layeru
    assert_eq!(s.get("color").map(|v| v.as_str()), Some("green"));
}

// ─── @font-face ────────────────────────────────────────────────────────

#[test]
fn font_face_basic_parse() {
    let s = parse_stylesheet(r#"
        @font-face {
            font-family: "MyFont";
            src: url("foo.woff2") format("woff2");
            font-weight: 700;
        }
    "#);
    assert_eq!(s.font_faces.len(), 1);
    assert_eq!(s.font_faces[0].family, "MyFont");
    assert!(s.font_faces[0].src.contains("foo.woff2"));
    assert_eq!(s.font_faces[0].weight, "700");
}

#[test]
fn font_face_extract_url() {
    use crate::browser::css_parser::extract_font_url;
    let url = extract_font_url(r#"url("foo.woff2") format("woff2")"#).unwrap();
    assert_eq!(url, "foo.woff2");
    let url2 = extract_font_url(r#"url(/fonts/bar.ttf)"#).unwrap();
    assert_eq!(url2, "/fonts/bar.ttf");
}

// ─── CSS Pseudo-Elements ::before / ::after ────────────────────────────

#[test]
fn pseudo_before_styles_separate_from_element() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        p { color: black; }
        p::before { content: "->"; color: red; }
    "#);
    let map = cascade::cascade(&doc.root, &[css.clone()]);
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    // Element p ma color: black
    assert_eq!(cascade::get_styles(&map, &p).unwrap().get("color").map(|s| s.as_str()), Some("black"));
    // Pseudo ::before ma content + color: red
    let before = cascade::get_pseudo_styles(&pmap, &p, "before").unwrap();
    assert_eq!(before.get("content").map(|s| s.as_str()), Some("\"->\""));
    assert_eq!(before.get("color").map(|s| s.as_str()), Some("red"));
}

#[test]
fn pseudo_after_only_when_matched() {
    let doc = parse_html(r#"<html><body><p>x</p><div>y</div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        p::after { content: "!"; }
    "#);
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert!(cascade::get_pseudo_styles(&pmap, &p, "after").is_some());
    assert!(cascade::get_pseudo_styles(&pmap, &div, "after").is_none());
}

#[test]
fn pseudo_legacy_single_colon_syntax() {
    // CSS2 :before je legacy - povolime ho
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        p:before { content: "x"; }
    "#);
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    assert!(cascade::get_pseudo_styles(&pmap, &p, "before").is_some());
}

#[test]
fn pseudo_specificity_cascades() {
    let doc = parse_html(r#"<html><body><p class="x">y</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        p::before { color: red; }
        p.x::before { color: blue; }
    "#);
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let before = cascade::get_pseudo_styles(&pmap, &p, "before").unwrap();
    // .x specificita > p, color = blue
    assert_eq!(before.get("color").map(|s| s.as_str()), Some("blue"));
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
fn resolve_abs_returns_positive() {
    let v = std::collections::HashMap::new();
    assert_eq!(cascade::resolve_value("abs(-15px)", &v), "15px");
}

#[test]
fn resolve_sqrt_unitless() {
    let v = std::collections::HashMap::new();
    assert_eq!(cascade::resolve_value("sqrt(16)", &v), "4");
}

#[test]
fn resolve_pow_two_args() {
    let v = std::collections::HashMap::new();
    assert_eq!(cascade::resolve_value("pow(2, 10)", &v), "1024");
}

#[test]
fn resolve_round_to_int() {
    let v = std::collections::HashMap::new();
    assert_eq!(cascade::resolve_value("round(15.7px)", &v), "16px");
}

#[test]
fn resolve_sin_zero_returns_zero() {
    let v = std::collections::HashMap::new();
    let r = cascade::resolve_value("sin(0deg)", &v);
    let parsed: f32 = r.parse().unwrap_or(0.0);
    assert!(parsed.abs() < 1e-3);
}

#[test]
fn resolve_cos_zero_returns_one() {
    let v = std::collections::HashMap::new();
    let r = cascade::resolve_value("cos(0deg)", &v);
    let parsed: f32 = r.parse().unwrap_or(0.0);
    assert!((parsed - 1.0).abs() < 1e-3);
}

#[test]
fn resolve_hypot_3_4_returns_5() {
    let v = std::collections::HashMap::new();
    assert_eq!(cascade::resolve_value("hypot(3, 4)", &v), "5");
}

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

// ─── Doplnujici cascade testy ──────────────────────────────────────────

#[test]
fn cascade_first_child_pseudo() {
    let doc = parse_html("<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>", "");
    let css = parse_stylesheet("li:first-child { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let lis = doc.root.get_elements_by_tag("li");
    let first_color = cascade::get_styles(&map, &lis[0]).and_then(|s| s.get("color").cloned());
    assert_eq!(first_color.as_deref(), Some("red"));
    let third_color = cascade::get_styles(&map, &lis[2]).and_then(|s| s.get("color").cloned());
    assert!(third_color.is_none(), "non-first nezaujme");
}

#[test]
fn cascade_last_child_pseudo() {
    let doc = parse_html("<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>", "");
    let css = parse_stylesheet("li:last-child { color: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let lis = doc.root.get_elements_by_tag("li");
    let last_color = cascade::get_styles(&map, &lis[2]).and_then(|s| s.get("color").cloned());
    assert_eq!(last_color.as_deref(), Some("blue"));
}

#[test]
fn cascade_nth_child_even() {
    let doc = parse_html("<html><body><ul><li>1</li><li>2</li><li>3</li><li>4</li></ul></body></html>", "");
    let css = parse_stylesheet("li:nth-child(even) { color: green; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let lis = doc.root.get_elements_by_tag("li");
    // Even = 2, 4 (1-indexed)
    let c2 = cascade::get_styles(&map, &lis[1]).and_then(|s| s.get("color").cloned());
    let c4 = cascade::get_styles(&map, &lis[3]).and_then(|s| s.get("color").cloned());
    assert_eq!(c2.as_deref(), Some("green"));
    assert_eq!(c4.as_deref(), Some("green"));
}

#[test]
fn cascade_nth_child_odd_skips_even() {
    let doc = parse_html("<html><body><ul><li>1</li><li>2</li><li>3</li></ul></body></html>", "");
    let css = parse_stylesheet("li:nth-child(odd) { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let lis = doc.root.get_elements_by_tag("li");
    let c1 = cascade::get_styles(&map, &lis[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &lis[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("red"));
    assert!(c2.is_none(), "even skipnut");
}

#[test]
fn cascade_attribute_selector_exact() {
    let doc = parse_html(r#"<html><body><input type="text"><input type="email"></body></html>"#, "");
    let css = parse_stylesheet(r#"input[type="text"] { padding: 10px; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let inputs = doc.root.get_elements_by_tag("input");
    let p1 = cascade::get_styles(&map, &inputs[0]).and_then(|s| s.get("padding").cloned());
    let p2 = cascade::get_styles(&map, &inputs[1]).and_then(|s| s.get("padding").cloned());
    assert_eq!(p1.as_deref(), Some("10px"));
    assert!(p2.is_none());
}

#[test]
fn cascade_attribute_selector_contains_word() {
    let doc = parse_html(r#"<html><body><div class="alpha beta gamma">x</div><div class="alpha">y</div></body></html>"#, "");
    let css = parse_stylesheet(r#"div[class~="beta"] { color: pink; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let divs = doc.root.get_elements_by_tag("div");
    let c1 = cascade::get_styles(&map, &divs[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &divs[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("pink"));
    assert!(c2.is_none());
}

#[test]
fn cascade_class_combo_with_class() {
    let doc = parse_html(r#"<html><body><div class="a b">x</div><div class="a">y</div></body></html>"#, "");
    let css = parse_stylesheet(".a.b { color: violet; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let divs = doc.root.get_elements_by_tag("div");
    assert_eq!(cascade::get_styles(&map, &divs[0]).and_then(|s| s.get("color").cloned()).as_deref(), Some("violet"));
    assert!(cascade::get_styles(&map, &divs[1]).and_then(|s| s.get("color").cloned()).is_none());
}

#[test]
fn cascade_important_overrides_specificity() {
    let doc = parse_html(r#"<html><body><div id="x" class="y">z</div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        #x { color: red; }
        .y { color: green !important; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let c = cascade::get_styles(&map, &div).and_then(|s| s.get("color").cloned());
    assert_eq!(c.as_deref(), Some("green"));
}

#[test]
fn cascade_later_rule_same_specificity_wins() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet(r#"
        p { color: red; }
        p { color: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let c = cascade::get_styles(&map, &p).and_then(|s| s.get("color").cloned());
    assert_eq!(c.as_deref(), Some("blue"));
}

#[test]
fn cascade_inheritance_does_not_blow_up() {
    // color je technicky inherited per CSS spec, ale cascade muze ukladat
    // je jen na ancestor. Test jen zarucuje ze cascade neprerusi.
    let doc = parse_html("<html><body><div><span>x</span></div></body></html>", "");
    let css = parse_stylesheet("div { color: orange; }");
    let _map = cascade::cascade(&doc.root, &[css]);
    // Neni assert na value - jen test ze nepada
}

#[test]
fn cascade_no_inheritance_for_padding() {
    // padding NENI inherited
    let doc = parse_html("<html><body><div><span>x</span></div></body></html>", "");
    let css = parse_stylesheet("div { padding: 10px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let span = doc.root.find(|n| n.tag_name().as_deref() == Some("span")).unwrap();
    let p = cascade::get_styles(&map, &span).and_then(|s| s.get("padding").cloned());
    assert!(p.is_none());
}

#[test]
fn cascade_pseudo_after_content() {
    // Test ::after pseudo via PseudoStyleMap
    let doc = parse_html("<html><body><p>txt</p></body></html>", "");
    let css = parse_stylesheet(r#"p::after { content: "!"; color: red; }"#);
    let _map = cascade::cascade(&doc.root, &[css]);
    // ::after by mel byt v PseudoStyleMap - ujistime se ze cascade nepada
    // (assert je prazdny - jen kompilace + run)
}

#[test]
fn cascade_var_resolution() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet(r#"
        :root { --main-color: purple; }
        div { color: var(--main-color); }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let c = cascade::get_styles(&map, &div).and_then(|s| s.get("color").cloned());
    assert_eq!(c.as_deref(), Some("purple"));
}

#[test]
fn cascade_var_with_fallback() {
    let doc = parse_html("<html><body><div>x</div></body></html>", "");
    let css = parse_stylesheet(r#"div { color: var(--undefined, teal); }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let c = cascade::get_styles(&map, &div).and_then(|s| s.get("color").cloned());
    assert_eq!(c.as_deref(), Some("teal"));
}

#[test]
fn cascade_calc_simple_addition() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("calc(10px + 5px)", &vars);
    assert_eq!(r, "15px");
}

#[test]
fn cascade_calc_subtraction() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("calc(20px - 5px)", &vars);
    assert_eq!(r, "15px");
}

#[test]
fn cascade_clamp_within_range() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("clamp(0px, 10px, 20px)", &vars);
    assert_eq!(r, "10px");
}

#[test]
fn cascade_clamp_below_min() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("clamp(5px, 1px, 20px)", &vars);
    assert_eq!(r, "5px");
}

#[test]
fn cascade_clamp_above_max() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("clamp(0px, 100px, 50px)", &vars);
    assert_eq!(r, "50px");
}

#[test]
fn cascade_min_function() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("min(10px, 5px, 20px)", &vars);
    assert_eq!(r, "5px");
}

#[test]
fn cascade_max_function() {
    let vars = std::collections::HashMap::new();
    let r = cascade::resolve_value("max(10px, 5px, 20px)", &vars);
    assert_eq!(r, "20px");
}

// ─── Advanced selector tests ────────────────────────────────────────────

#[test]
fn cascade_not_pseudo() {
    let doc = parse_html(r#"<html><body><p>X</p><p class="a">Y</p></body></html>"#, "");
    let css = parse_stylesheet("p:not(.a) { color: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let ps = doc.root.get_elements_by_tag("p");
    let c1 = cascade::get_styles(&map, &ps[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &ps[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("blue"));
    assert!(c2.is_none(), "p.a vyrazeno");
}

#[test]
fn cascade_root_pseudo() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(":root { --primary: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    // :root by mel match html element
    if let Some(html) = doc.root.find(|n| n.tag_name().as_deref() == Some("html")) {
        let _ = cascade::get_styles(&map, &html);
    }
}

#[test]
fn cascade_compound_selector_id_class_tag() {
    let doc = parse_html(r#"<html><body><div id="main" class="box">x</div></body></html>"#, "");
    let css = parse_stylesheet("div#main.box { padding: 20px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let p = cascade::get_styles(&map, &div).and_then(|s| s.get("padding").cloned());
    assert_eq!(p.as_deref(), Some("20px"));
}

#[test]
fn cascade_id_with_dash() {
    let doc = parse_html(r#"<html><body><div id="my-id"></div></body></html>"#, "");
    let css = parse_stylesheet("#my-id { color: green; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert_eq!(cascade::get_styles(&map, &div).and_then(|s| s.get("color").cloned()).as_deref(), Some("green"));
}

#[test]
fn cascade_attribute_starts_with() {
    let doc = parse_html(r#"<html><body><a href="https://x.com">a</a><a href="http://y.com">b</a></body></html>"#, "");
    let css = parse_stylesheet(r#"a[href^="https"] { color: green; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let links = doc.root.get_elements_by_tag("a");
    let c1 = cascade::get_styles(&map, &links[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &links[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("green"));
    assert!(c2.is_none() || c2 != c1);
}

#[test]
fn cascade_attribute_ends_with() {
    let doc = parse_html(r#"<html><body><img src="logo.png"><img src="data.json"></body></html>"#, "");
    let css = parse_stylesheet(r#"img[src$=".png"] { border: 1px solid red; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let imgs = doc.root.get_elements_by_tag("img");
    let b1 = cascade::get_styles(&map, &imgs[0]).and_then(|s| s.get("border").cloned());
    let b2 = cascade::get_styles(&map, &imgs[1]).and_then(|s| s.get("border").cloned());
    assert!(b1.is_some());
    assert!(b2.is_none() || b2 != b1);
}

#[test]
fn cascade_attribute_contains() {
    let doc = parse_html(r#"<html><body><div class="foo-bar-baz"></div><div class="other"></div></body></html>"#, "");
    let css = parse_stylesheet(r#"div[class*="bar"] { color: pink; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let divs = doc.root.get_elements_by_tag("div");
    let c1 = cascade::get_styles(&map, &divs[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &divs[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("pink"));
    assert!(c2.is_none());
}

#[test]
fn cascade_general_sibling_combinator() {
    let doc = parse_html(r#"
        <html><body>
            <h1>title</h1>
            <p>p1</p>
            <span>span</span>
            <p>p2</p>
        </body></html>
    "#, "");
    let css = parse_stylesheet("h1 ~ p { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let ps = doc.root.get_elements_by_tag("p");
    let c1 = cascade::get_styles(&map, &ps[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &ps[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("red"));
    assert_eq!(c2.as_deref(), Some("red"));
}

#[test]
fn cascade_only_child_pseudo() {
    let doc = parse_html(r#"
        <html><body>
            <div><span>only</span></div>
            <div><span>first</span><span>second</span></div>
        </body></html>
    "#, "");
    let css = parse_stylesheet("span:only-child { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let spans = doc.root.get_elements_by_tag("span");
    let c1 = cascade::get_styles(&map, &spans[0]).and_then(|s| s.get("color").cloned());
    let c2 = cascade::get_styles(&map, &spans[1]).and_then(|s| s.get("color").cloned());
    assert_eq!(c1.as_deref(), Some("red"));
    assert!(c2.is_none(), "non only-child");
}

#[test]
fn cascade_disabled_pseudo() {
    let doc = parse_html(r#"<html><body><input disabled><input></body></html>"#, "");
    let css = parse_stylesheet("input:disabled { background: gray; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let inputs = doc.root.get_elements_by_tag("input");
    let b1 = cascade::get_styles(&map, &inputs[0]).and_then(|s| s.get("background").cloned());
    let b2 = cascade::get_styles(&map, &inputs[1]).and_then(|s| s.get("background").cloned());
    assert_eq!(b1.as_deref(), Some("gray"));
    assert!(b2.is_none());
}

// ─── Custom property cascade chain ─────────────────────────────────────

#[test]
fn cascade_custom_property_inherited() {
    let doc = parse_html(r#"<html><body><div><span>x</span></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        :root { --my-color: purple; }
        span { color: var(--my-color); }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let span = doc.root.find(|n| n.tag_name().as_deref() == Some("span")).unwrap();
    let c = cascade::get_styles(&map, &span).and_then(|s| s.get("color").cloned());
    assert_eq!(c.as_deref(), Some("purple"));
}

#[test]
fn cascade_var_chain_resolves() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(r#"
        :root { --base: 10px; --scaled: var(--base); }
        div { padding: var(--scaled); }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let p = cascade::get_styles(&map, &div).and_then(|s| s.get("padding").cloned());
    assert_eq!(p.as_deref(), Some("10px"));
}

#[test]
fn system_color_canvas_parsed() {
    use crate::browser::layout::parse_color;
    // canvas = bily pozadi (light mode default)
    assert_eq!(parse_color("canvas"), Some([255, 255, 255, 255]));
    assert_eq!(parse_color("canvastext"), Some([0, 0, 0, 255]));
    assert_eq!(parse_color("buttonface"), Some([240, 240, 240, 255]));
    assert_eq!(parse_color("highlight"), Some([0, 120, 215, 255]));
    assert_eq!(parse_color("graytext"), Some([109, 109, 109, 255]));
}

#[test]
fn named_color_full_set_parsed() {
    use crate::browser::layout::parse_color;
    assert_eq!(parse_color("rebeccapurple"), Some([102, 51, 153, 255]));
    assert_eq!(parse_color("aqua"), Some([0, 255, 255, 255]));
    assert_eq!(parse_color("fuchsia"), Some([255, 0, 255, 255]));
    assert_eq!(parse_color("lime"), Some([0, 255, 0, 255]));
    assert_eq!(parse_color("silver"), Some([192, 192, 192, 255]));
    assert_eq!(parse_color("navy"), Some([0, 0, 128, 255]));
    assert_eq!(parse_color("maroon"), Some([128, 0, 0, 255]));
    assert_eq!(parse_color("teal"), Some([0, 128, 128, 255]));
    assert_eq!(parse_color("olive"), Some([128, 128, 0, 255]));
}

#[test]
fn unset_removes_property() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    // Prvni pravidlo nastavi barvu, druhe ji unset-uje
    let css = parse_stylesheet("div { color: red; } div { color: unset; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    // Po unset by color nemela byt v computed stylech
    assert!(styles.and_then(|s| s.get("color")).is_none(), "unset odstranil color");
}

#[test]
fn initial_removes_property() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};
    let doc = parse_html(r#"<html><body><p></p></body></html>"#, "");
    let css = parse_stylesheet("p { color: blue; } p { color: initial; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_styles(&map, &p);
    assert!(styles.and_then(|s| s.get("color")).is_none(), "initial resetuje color");
}

#[test]
fn revert_removes_property() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};
    let doc = parse_html(r#"<html><body><span></span></body></html>"#, "");
    let css = parse_stylesheet("span { font-weight: bold; } span { font-weight: revert; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let span = doc.root.find(|n| n.tag_name().as_deref() == Some("span")).unwrap();
    let styles = cascade::get_styles(&map, &span);
    assert!(styles.and_then(|s| s.get("font-weight")).is_none(), "revert resetuje font-weight");
}

#[test]
fn unset_inline_style() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};
    // Inline style "color: unset" smaže barvu nastazenou pravidlem
    let doc = parse_html(r#"<html><body><div style="color: unset;"></div></body></html>"#, "");
    let css = parse_stylesheet("div { color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    assert!(styles.and_then(|s| s.get("color")).is_none(), "inline unset smazel color");
}

#[test]
fn at_property_initial_value_used_when_var_undefined() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(
        "@property --my-bg { syntax: \"<color>\"; inherits: false; initial-value: red; } \
         div { background-color: var(--my-bg); }"
    );
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let bg = styles.and_then(|s| s.get("background-color")).cloned().unwrap_or_default();
    assert_eq!(bg.trim(), "red", "@property initial-value pouzit pri undefined var");
}

#[test]
fn at_property_root_overrides_initial() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(
        "@property --c { syntax: \"<color>\"; inherits: true; initial-value: red; } \
         :root { --c: blue; } \
         div { color: var(--c); }"
    );
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let c = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert_eq!(c.trim(), "blue", ":root prebije initial-value");
}

#[test]
fn pseudo_placeholder_styles_collected() {
    let doc = parse_html(r#"<html><body><input placeholder="hi" /></body></html>"#, "");
    let css = parse_stylesheet("input::placeholder { color: gray; opacity: 0.5; }");
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let input = doc.root.find(|n| n.tag_name().as_deref() == Some("input")).unwrap();
    let styles = cascade::get_pseudo_styles(&pmap, &input, "placeholder");
    assert!(styles.is_some(), "::placeholder pseudo-element styly");
    let s = styles.unwrap();
    assert_eq!(s.get("color").map(String::as_str), Some("gray"));
}

#[test]
fn pseudo_selection_styles_collected() {
    let doc = parse_html(r#"<html><body><p>text</p></body></html>"#, "");
    let css = parse_stylesheet("p::selection { background-color: yellow; }");
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let styles = cascade::get_pseudo_styles(&pmap, &p, "selection");
    assert!(styles.is_some(), "::selection pseudo styly");
}

#[test]
fn pseudo_backdrop_styles_collected() {
    let doc = parse_html(r#"<html><body><dialog open></dialog></body></html>"#, "");
    let css = parse_stylesheet("dialog::backdrop { background: rgba(0,0,0,0.5); }");
    let pmap = cascade::cascade_pseudo(&doc.root, &[css]);
    let dl = doc.root.find(|n| n.tag_name().as_deref() == Some("dialog")).unwrap();
    let styles = cascade::get_pseudo_styles(&pmap, &dl, "backdrop");
    assert!(styles.is_some(), "::backdrop pseudo styly");
}

#[test]
fn container_query_per_ancestor_applies() {
    use std::rc::Rc;
    let doc = parse_html(
        r#"<html><body><section class="card"><div class="inner">x</div></section></body></html>"#, ""
    );
    let css = parse_stylesheet(
        ".card { container-type: inline-size; container-name: card; } \
         @container card (min-width: 400px) { .inner { color: red; } }"
    );
    let card = doc.root.find(|n| {
        n.attr("class").map(|c| c.contains("card")).unwrap_or(false)
    }).unwrap();
    // Manualne nastavime velikost karty na 500px
    let mut sizes = std::collections::HashMap::new();
    sizes.insert(Rc::as_ptr(&card) as usize, (500.0, 300.0));
    let map = cascade::cascade_with_container_sizes(&doc.root, &[css], 1024.0, 768.0, &sizes);
    let inner = doc.root.find(|n| {
        n.attr("class").map(|c| c.contains("inner")).unwrap_or(false)
    }).unwrap();
    let styles = cascade::get_styles(&map, &inner);
    let color = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert_eq!(color.trim(), "red", "@container query matchne ancestor 500px > 400px");
}

#[test]
fn pseudo_user_valid_match() {
    let doc = parse_html(r#"<html><body><input type="text" required value="hello" data-user-valid="true" /></body></html>"#, "");
    let css = parse_stylesheet("input:user-valid { border-color: green; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let inp = doc.root.find(|n| n.tag_name().as_deref() == Some("input")).unwrap();
    let styles = cascade::get_styles(&map, &inp);
    let bc = styles.and_then(|s| s.get("border-color")).cloned().unwrap_or_default();
    assert_eq!(bc.trim(), "green", ":user-valid match s data-user-valid attribute");
}

#[test]
fn pseudo_popover_open_match() {
    let doc = parse_html(r#"<html><body><div popover data-popover-open="true"></div></body></html>"#, "");
    let css = parse_stylesheet("div:popover-open { background: yellow; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let bg = styles.and_then(|s| s.get("background-color")).cloned().unwrap_or_default();
    assert_eq!(bg.trim(), "yellow", ":popover-open match popover otevreny");
}

#[test]
fn pseudo_open_details() {
    let doc = parse_html(r#"<html><body><details open></details></body></html>"#, "");
    let css = parse_stylesheet("details:open { color: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("details")).unwrap();
    let styles = cascade::get_styles(&map, &d);
    let c = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert_eq!(c.trim(), "blue");
}

#[test]
fn pseudo_closed_dialog() {
    let doc = parse_html(r#"<html><body><dialog></dialog></body></html>"#, "");
    let css = parse_stylesheet("dialog:closed { display: none; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("dialog")).unwrap();
    let styles = cascade::get_styles(&map, &d);
    let dis = styles.and_then(|s| s.get("display")).cloned().unwrap_or_default();
    assert_eq!(dis.trim(), "none");
}

#[test]
fn pseudo_modal_dialog_with_attribute() {
    let doc = parse_html(r#"<html><body><dialog open data-modal="true"></dialog></body></html>"#, "");
    let css = parse_stylesheet("dialog:modal { z-index: 9999; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("dialog")).unwrap();
    let styles = cascade::get_styles(&map, &d);
    let z = styles.and_then(|s| s.get("z-index")).cloned().unwrap_or_default();
    assert_eq!(z.trim(), "9999");
}

#[test]
fn at_scope_applies_only_to_descendants() {
    let doc = parse_html(
        r#"<html><body><section class="card"><p>in</p></section><p>out</p></body></html>"#, ""
    );
    let css = parse_stylesheet("@scope (.card) { p { color: red; } }");
    let map = cascade::cascade(&doc.root, &[css]);
    let in_p = doc.root.find(|n| {
        n.tag_name().as_deref() == Some("p")
        && n.parent.borrow().upgrade().and_then(|p| p.attr("class")).map(|c| c.contains("card")).unwrap_or(false)
    }).unwrap();
    let out_p = doc.root.find(|n| {
        n.tag_name().as_deref() == Some("p")
        && !n.parent.borrow().upgrade().and_then(|p| p.attr("class")).map(|c| c.contains("card")).unwrap_or(false)
    }).unwrap();
    let in_color = cascade::get_styles(&map, &in_p).and_then(|s| s.get("color")).cloned().unwrap_or_default();
    let out_color = cascade::get_styles(&map, &out_p).and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert_eq!(in_color.trim(), "red", "p uvnitr .card dostane @scope styl");
    assert!(out_color.trim() != "red", "p mimo .card NEMA @scope styl");
}

#[test]
fn at_scope_with_limit_excludes_subtree() {
    let doc = parse_html(
        r#"<html><body>
            <section class="card">
                <p>before</p>
                <div class="divider"><p>inside-limit</p></div>
            </section>
        </body></html>"#, ""
    );
    let css = parse_stylesheet("@scope (.card) to (.divider) { p { color: red; } }");
    let map = cascade::cascade(&doc.root, &[css]);
    // p pod divider by mel byt mimo scope
    let p_in_divider = doc.root.find(|n| {
        n.tag_name().as_deref() == Some("p")
        && n.parent.borrow().upgrade().and_then(|p| p.attr("class")).map(|c| c.contains("divider")).unwrap_or(false)
    }).unwrap();
    let color = cascade::get_styles(&map, &p_in_divider).and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert!(color.trim() != "red", "p pod limit element NEMA scope styl");
}

#[test]
fn node_in_scope_self_match() {
    use crate::browser::cascade::node_in_scope;
    let doc = parse_html(r#"<html><body><div class="card">x</div></body></html>"#, "");
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    assert!(node_in_scope(&div, ".card", None), "self-match: .card sam je v scope");
}

#[test]
fn at_starting_style_cascade() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("@starting-style { div { opacity: 0; transform: scale(0.5); } }");
    let map = cascade::cascade_starting_style(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    assert!(styles.is_some(), "div ma @starting-style styly");
    let s = styles.unwrap();
    assert_eq!(s.get("opacity").map(String::as_str), Some("0"));
    assert!(s.get("transform").map(|v| v.contains("scale")).unwrap_or(false));
}

#[test]
fn at_starting_style_only_matches_relevant() {
    let doc = parse_html(r#"<html><body><div></div><span></span></body></html>"#, "");
    let css = parse_stylesheet("@starting-style { div { opacity: 0; } }");
    let map = cascade::cascade_starting_style(&doc.root, &[css]);
    let span = doc.root.find(|n| n.tag_name().as_deref() == Some("span")).unwrap();
    let styles = cascade::get_styles(&map, &span);
    // span nedostane opacity:0 protoze ho selektor div nematchne
    assert!(styles.map(|s| s.get("opacity").is_none()).unwrap_or(true));
}

#[test]
fn at_function_basic_call() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(
        "@function --double(x) returns <length> { result: calc(var(--x) * 2); } \
         div { width: --double(50px); }"
    );
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let w = styles.and_then(|s| s.get("width")).cloned().unwrap_or_default();
    // calc(50px * 2) -> 100px
    assert!(w.trim() == "100px" || w.trim() == "100", "@function --double(50px) -> 100px, dostal: {}", w);
}

#[test]
fn at_function_two_args() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(
        "@function --plus(a, b) returns <length> { result: calc(var(--a) + var(--b)); } \
         div { padding: --plus(10px, 20px); }"
    );
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let p = styles.and_then(|s| s.get("padding-top")).cloned()
        .or_else(|| styles.and_then(|s| s.get("padding")).cloned()).unwrap_or_default();
    assert!(p.contains("30"), "10 + 20 = 30, dostal: {}", p);
}

#[test]
fn css_if_function_true_branch() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(":root { --enabled: true; } div { color: if(var(--enabled), red, blue); }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let c = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert_eq!(c.trim(), "red", "if(true, red, blue) -> red");
}

#[test]
fn css_if_function_false_branch() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(":root { --enabled: false; } div { color: if(var(--enabled), red, blue); }");
    let map = cascade::cascade(&doc.root, &[css]);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let styles = cascade::get_styles(&map, &div);
    let c = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert_eq!(c.trim(), "blue", "if(false, red, blue) -> blue");
}

#[test]
fn pseudo_user_invalid_match() {
    let doc = parse_html(r#"<html><body><input type="email" required value="" data-user-invalid="true" /></body></html>"#, "");
    let css = parse_stylesheet("input:user-invalid { border-color: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let inp = doc.root.find(|n| n.tag_name().as_deref() == Some("input")).unwrap();
    let styles = cascade::get_styles(&map, &inp);
    let bc = styles.and_then(|s| s.get("border-color")).cloned().unwrap_or_default();
    assert_eq!(bc.trim(), "red", ":user-invalid match s data-user-invalid attribute");
}

#[test]
fn container_query_too_small_no_match() {
    use std::rc::Rc;
    let doc = parse_html(
        r#"<html><body><section class="card"><div class="inner">x</div></section></body></html>"#, ""
    );
    let css = parse_stylesheet(
        ".card { container-type: inline-size; container-name: card; } \
         @container card (min-width: 400px) { .inner { color: red; } }"
    );
    let card = doc.root.find(|n| {
        n.attr("class").map(|c| c.contains("card")).unwrap_or(false)
    }).unwrap();
    let mut sizes = std::collections::HashMap::new();
    sizes.insert(Rc::as_ptr(&card) as usize, (200.0, 300.0));
    let map = cascade::cascade_with_container_sizes(&doc.root, &[css], 1024.0, 768.0, &sizes);
    let inner = doc.root.find(|n| {
        n.attr("class").map(|c| c.contains("inner")).unwrap_or(false)
    }).unwrap();
    let styles = cascade::get_styles(&map, &inner);
    // 200px < 400px container query NEMATCH - bez viewport fallback contamination.
    let color = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert!(color.trim() != "red",
        "@container nematch (200 < 400) - inner NEMA color: red, dostal {}", color);
}

#[test]
fn container_query_no_container_skipped() {
    // Zadny container ancestor - rules se nemaji aplikovat.
    let doc = parse_html(r#"<html><body><div class="inner">x</div></body></html>"#, "");
    let css = parse_stylesheet(
        "@container (min-width: 100px) { .inner { color: red; } }"
    );
    let sizes = std::collections::HashMap::new(); // zadny container
    let map = cascade::cascade_with_container_sizes(&doc.root, &[css], 1024.0, 768.0, &sizes);
    let inner = doc.root.find(|n| n.attr("class").map(|c| c.contains("inner")).unwrap_or(false)).unwrap();
    let styles = cascade::get_styles(&map, &inner);
    let color = styles.and_then(|s| s.get("color")).cloned().unwrap_or_default();
    assert!(color.trim() != "red", "Bez container ancestor query NEMATCH");
}

// ─── L5 step 2c: cascade_with_viewport_typed smoke test ────────────────

#[test]
fn cascade_typed_dual_write_smoke() {
    use crate::browser::computed_style::PropertyId;
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { color: red; font-size: 14px; bogus-prop: 5px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);

    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let node_id = std::rc::Rc::as_ptr(&p) as usize;

    // style_map (legacy) ma color
    let styles = out.style_map.get(&node_id).expect("style_map entry");
    assert_eq!(styles.get("color").map(|s| s.as_str()), Some("red"));

    // computed je initial (stage 2c stub - bez populace)
    assert!(out.computed.contains_key(&node_id), "computed entry per node");

    // declarations ma 3 props, bogus-prop = Unknown/invalid
    let decls = out.declarations.get(&node_id).expect("declarations entry");
    let color_decl = decls.iter().find(|d| d.property == PropertyId::Color).unwrap();
    assert!(color_decl.valid);
    assert_eq!(color_decl.raw_value, "red");
    let bogus = decls.iter().find(|d| d.raw_value == "5px" && !d.valid);
    assert!(bogus.is_some(), "bogus-prop captured as invalid");
}

// ─── L5 step 3 batch 1: color/opacity/visibility/cursor populace ──────

#[test]
fn cascade_typed_color_populace() {
    use crate::browser::computed_style::Color;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { color: #ff0000; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.color, Color::Rgba { r: 255, g: 0, b: 0, a: 255 });
}

#[test]
fn cascade_typed_opacity_clamp() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    // 1.5 nad max - clamp na 1.0
    let css = parse_stylesheet("div { opacity: 1.5; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert!((cs.opacity - 1.0).abs() < 0.001);
}

#[test]
fn cascade_typed_opacity_invalid_keeps_initial() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { opacity: garbage; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    // Invalid -> initial 1.0 (CSS spec discard)
    assert!((cs.opacity - 1.0).abs() < 0.001);
}

#[test]
fn cascade_typed_visibility() {
    use crate::browser::computed_style::Visibility;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { visibility: hidden; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.visibility, Visibility::Hidden);
}

#[test]
fn cascade_typed_cursor_keyword() {
    use crate::browser::computed_style::Cursor;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { cursor: pointer; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.cursor, Cursor::Pointer);
}

#[test]
fn cascade_typed_cursor_custom_fallback() {
    use crate::browser::computed_style::Cursor;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { cursor: zoom-in; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    // Cursor::Custom("zoom-in") - neznamy keyword zachycen
    match &cs.cursor {
        Cursor::Custom(s) => assert_eq!(s, "zoom-in"),
        other => panic!("expected Custom, got {:?}", other),
    }
}

#[test]
fn cascade_typed_visibility_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { visibility: blah; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    let vis = decls.iter().find(|d| d.raw_value == "blah").unwrap();
    assert!(!vis.valid, "visibility:blah marked invalid pro devtools strikethrough");
}

// ─── L5 step 3 batch 2: display/position/z_index populace ─────────────

#[test]
fn cascade_typed_display() {
    use crate::browser::computed_style::Display as CsDisplay;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { display: flex; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.display, CsDisplay::Flex);
}

#[test]
fn cascade_typed_display_inline_block() {
    use crate::browser::computed_style::Display as CsDisplay;
    let doc = parse_html("<html><body><span></span></body></html>", "");
    let css = parse_stylesheet("span { display: inline-block; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let span = doc.root.find(|n| n.tag_name().as_deref() == Some("span")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&span) as usize)).unwrap();
    assert_eq!(cs.display, CsDisplay::InlineBlock);
}

#[test]
fn cascade_typed_position() {
    use crate::browser::computed_style::PositionKind;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { position: absolute; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.position, PositionKind::Absolute);
}

#[test]
fn cascade_typed_zindex_value() {
    use crate::browser::computed_style::ZIndex;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { z-index: 42; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.z_index, ZIndex::Value(42));
}

#[test]
fn cascade_typed_zindex_auto() {
    use crate::browser::computed_style::ZIndex;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { z-index: auto; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.z_index, ZIndex::Auto);
}

#[test]
fn cascade_typed_position_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { position: floaty; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    let pos = decls.iter().find(|d| d.raw_value == "floaty").unwrap();
    assert!(!pos.valid, "position:floaty marked invalid");
}

// ─── L5 step 3 batch 3: font_size/font_weight/font_style/line_height ───

#[test]
fn cascade_typed_font_size_px() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { font-size: 14px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.font_size, Length::Px(14.0));
}

#[test]
fn cascade_typed_font_size_em() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { font-size: 1.5em; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    // Em zachova jednotku - resolve az pri layout context.
    assert_eq!(cs.font_size, Length::Em(1.5));
}

#[test]
fn cascade_typed_font_weight_numeric() {
    let doc = parse_html("<html><body><strong>x</strong></body></html>", "");
    let css = parse_stylesheet("strong { font-weight: 800; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let s = doc.root.find(|n| n.tag_name().as_deref() == Some("strong")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&s) as usize)).unwrap();
    assert_eq!(cs.font_weight, 800);
}

#[test]
fn cascade_typed_font_weight_bold_keyword() {
    let doc = parse_html("<html><body><b>x</b></body></html>", "");
    let css = parse_stylesheet("b { font-weight: bold; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let b = doc.root.find(|n| n.tag_name().as_deref() == Some("b")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&b) as usize)).unwrap();
    assert_eq!(cs.font_weight, 700);
}

#[test]
fn cascade_typed_font_style_italic() {
    let doc = parse_html("<html><body><i>x</i></body></html>", "");
    let css = parse_stylesheet("i { font-style: italic; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("i")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert!(cs.font_style_italic);
}

#[test]
fn cascade_typed_font_style_oblique() {
    let doc = parse_html("<html><body><i>x</i></body></html>", "");
    let css = parse_stylesheet("i { font-style: oblique 5deg; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("i")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert!(cs.font_style_italic);
}

#[test]
fn cascade_typed_line_height_normal() {
    use crate::browser::computed_style::LineHeight;
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { line-height: normal; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.line_height, LineHeight::Normal);
}

#[test]
fn cascade_typed_line_height_multiplier() {
    use crate::browser::computed_style::LineHeight;
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { line-height: 1.5; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.line_height, LineHeight::Multiplier(1.5));
}

#[test]
fn cascade_typed_line_height_length() {
    use crate::browser::computed_style::{LineHeight, Length};
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { line-height: 24px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.line_height, LineHeight::Length(Length::Px(24.0)));
}

#[test]
fn cascade_typed_font_size_invalid_marked() {
    let doc = parse_html("<html><body><p>x</p></body></html>", "");
    let css = parse_stylesheet("p { font-size: chunky; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    let fs = decls.iter().find(|d| d.raw_value == "chunky").unwrap();
    assert!(!fs.valid, "font-size:chunky marked invalid");
}

// ─── L5 step 3 batch 4: margin-top/right/bottom/left ──────────────────

#[test]
fn cascade_typed_margin_longhands() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { margin-top: 10px; margin-right: 20px; margin-bottom: 30px; margin-left: 40px; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.margin_top, Length::Px(10.0));
    assert_eq!(cs.margin_right, Length::Px(20.0));
    assert_eq!(cs.margin_bottom, Length::Px(30.0));
    assert_eq!(cs.margin_left, Length::Px(40.0));
}

#[test]
fn cascade_typed_margin_shorthand_expand() {
    use crate::browser::computed_style::Length;
    // expand_shorthand v cascade rozdeli `margin: 10px 20px` na 4 longhandy.
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { margin: 10px 20px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.margin_top, Length::Px(10.0));
    assert_eq!(cs.margin_right, Length::Px(20.0));
    assert_eq!(cs.margin_bottom, Length::Px(10.0));
    assert_eq!(cs.margin_left, Length::Px(20.0));
}

#[test]
fn cascade_typed_margin_auto() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { margin-left: auto; margin-right: auto; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.margin_left, Length::Auto);
    assert_eq!(cs.margin_right, Length::Auto);
}

#[test]
fn cascade_typed_margin_percent() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { margin-top: 25%; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.margin_top, Length::Percent(25.0));
}

#[test]
fn cascade_typed_margin_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { margin-top: chunky; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    let m = decls.iter().find(|d| d.raw_value == "chunky").unwrap();
    assert!(!m.valid);
}

// ─── L5 step 3 batch 5: padding-top/right/bottom/left ─────────────────

#[test]
fn cascade_typed_padding_longhands() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { padding-top: 5px; padding-right: 10px; padding-bottom: 15px; padding-left: 20px; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.padding_top, Length::Px(5.0));
    assert_eq!(cs.padding_right, Length::Px(10.0));
    assert_eq!(cs.padding_bottom, Length::Px(15.0));
    assert_eq!(cs.padding_left, Length::Px(20.0));
}

#[test]
fn cascade_typed_padding_shorthand_one() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { padding: 8px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.padding_top, Length::Px(8.0));
    assert_eq!(cs.padding_right, Length::Px(8.0));
    assert_eq!(cs.padding_bottom, Length::Px(8.0));
    assert_eq!(cs.padding_left, Length::Px(8.0));
}

#[test]
fn cascade_typed_padding_percent() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { padding-left: 10%; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.padding_left, Length::Percent(10.0));
}

#[test]
fn cascade_typed_padding_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { padding-top: nope; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    let p = decls.iter().find(|d| d.raw_value == "nope").unwrap();
    assert!(!p.valid);
}

// ─── L5 step 3 batch 6: width/height/min-width/min-height ─────────────

#[test]
fn cascade_typed_width_height() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { width: 300px; height: 200px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.width, Length::Px(300.0));
    assert_eq!(cs.height, Length::Px(200.0));
}

#[test]
fn cascade_typed_width_auto() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { width: auto; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.width, Length::Auto);
}

#[test]
fn cascade_typed_min_dimensions() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { min-width: 100px; min-height: 50%; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.min_width, Length::Px(100.0));
    assert_eq!(cs.min_height, Length::Percent(50.0));
}

#[test]
fn cascade_typed_width_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { width: garbage; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    let w = decls.iter().find(|d| d.raw_value == "garbage").unwrap();
    assert!(!w.valid);
}

// ─── L5 step 3 batch 7: max-width/max-height + top/right ──────────────

#[test]
fn cascade_typed_max_dimensions() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { max-width: 800px; max-height: 600px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.max_width, Length::Px(800.0));
    assert_eq!(cs.max_height, Length::Px(600.0));
}

#[test]
fn cascade_typed_max_none() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { max-width: none; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.max_width, Length::None);
}

#[test]
fn cascade_typed_top_right_offsety() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { top: 10px; right: 25%; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.top, Length::Px(10.0));
    assert_eq!(cs.right, Length::Percent(25.0));
}

#[test]
fn cascade_typed_top_auto() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { top: auto; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.top, Length::Auto);
}

// ─── L5 step 3 batch 8: bottom/left + background_color + font_family ──

#[test]
fn cascade_typed_bottom_left() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { bottom: 5px; left: 0; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.bottom, Length::Px(5.0));
    assert_eq!(cs.left, Length::Px(0.0));
}

#[test]
fn cascade_typed_background_color() {
    use crate::browser::computed_style::Color;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { background-color: blue; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let div = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&div) as usize)).unwrap();
    assert_eq!(cs.background_color, Color::Rgba { r: 0, g: 0, b: 255, a: 255 });
}

#[test]
fn cascade_typed_font_family_named() {
    use crate::browser::computed_style::{FontFamily, GenericFamily};
    let doc = parse_html("<html><body><p></p></body></html>", "");
    let css = parse_stylesheet(r#"p { font-family: "Arial", sans-serif; }"#);
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    // "Arial" je Named (after strip quotes), sans-serif je Generic.
    assert_eq!(cs.font_family.len(), 2);
    match &cs.font_family[0] {
        FontFamily::Named(s) => assert_eq!(s, "Arial"),
        other => panic!("expected Named, got {:?}", other),
    }
    assert_eq!(cs.font_family[1], FontFamily::Generic(GenericFamily::SansSerif));
}

#[test]
fn cascade_typed_font_family_monospace_generic() {
    use crate::browser::computed_style::{FontFamily, GenericFamily};
    let doc = parse_html("<html><body><code></code></body></html>", "");
    let css = parse_stylesheet("code { font-family: monospace; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 1024.0, 768.0);
    let c = doc.root.find(|n| n.tag_name().as_deref() == Some("code")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&c) as usize)).unwrap();
    assert_eq!(cs.font_family, vec![FontFamily::Generic(GenericFamily::Monospace)]);
}

// ─── L5 step 3 batch 9: text-align/white-space/word-break/overflow-wrap

#[test]
fn cascade_typed_text_align() {
    use crate::browser::computed_style::TextAlign as TA;
    let doc = parse_html("<html><body><p></p></body></html>", "");
    let css = parse_stylesheet("p { text-align: center; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.text_align, TA::Center);
}

#[test]
fn cascade_typed_white_space_pre() {
    use crate::browser::computed_style::WhiteSpace;
    let doc = parse_html("<html><body><pre></pre></body></html>", "");
    let css = parse_stylesheet("pre { white-space: pre; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("pre")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.white_space, WhiteSpace::Pre);
}

#[test]
fn cascade_typed_word_break_break_all() {
    use crate::browser::computed_style::WordBreak;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { word-break: break-all; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.word_break, WordBreak::BreakAll);
}

#[test]
fn cascade_typed_overflow_wrap_break_word() {
    use crate::browser::computed_style::OverflowWrap;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { overflow-wrap: break-word; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.overflow_wrap, OverflowWrap::BreakWord);
}

#[test]
fn cascade_typed_overflow_wrap_word_wrap_alias() {
    use crate::browser::computed_style::OverflowWrap;
    // word-wrap je legacy alias pro overflow-wrap. Cascade get_styles
    // returns oboje pod stejnym klicem? Test verifies behavior.
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { word-wrap: anywhere; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.overflow_wrap, OverflowWrap::Anywhere);
}

#[test]
fn cascade_typed_text_align_invalid_marked() {
    let doc = parse_html("<html><body><p></p></body></html>", "");
    let css = parse_stylesheet("p { text-align: middlish; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    let t = decls.iter().find(|d| d.raw_value == "middlish").unwrap();
    assert!(!t.valid);
}

// ─── L5 step 3 batch 10: writing_mode/direction/box_sizing/pointer_events

#[test]
fn cascade_typed_writing_mode() {
    use crate::browser::computed_style::WritingMode;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { writing-mode: vertical-rl; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.writing_mode, WritingMode::VerticalRl);
}

#[test]
fn cascade_typed_direction_rtl() {
    use crate::browser::computed_style::Direction;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { direction: rtl; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.direction, Direction::Rtl);
}

#[test]
fn cascade_typed_box_sizing_border_box() {
    use crate::browser::computed_style::BoxSizing;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { box-sizing: border-box; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.box_sizing, BoxSizing::BorderBox);
}

#[test]
fn cascade_typed_pointer_events_none() {
    use crate::browser::computed_style::PointerEvents;
    let doc = parse_html("<html><body><button></button></body></html>", "");
    let css = parse_stylesheet("button { pointer-events: none; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let b = doc.root.find(|n| n.tag_name().as_deref() == Some("button")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&b) as usize)).unwrap();
    assert_eq!(cs.pointer_events, PointerEvents::None);
}

#[test]
fn cascade_typed_pointer_events_svg_keyword_maps_auto() {
    use crate::browser::computed_style::PointerEvents;
    let doc = parse_html("<html><body><svg></svg></body></html>", "");
    let css = parse_stylesheet("svg { pointer-events: visiblePainted; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let s = doc.root.find(|n| n.tag_name().as_deref() == Some("svg")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&s) as usize)).unwrap();
    // SVG-specific keywords mapped to Auto pro non-SVG layout.
    assert_eq!(cs.pointer_events, PointerEvents::Auto);
}

#[test]
fn cascade_typed_writing_mode_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { writing-mode: sideways-diagonal; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let w = decls.iter().find(|d| d.raw_value == "sideways-diagonal").unwrap();
    assert!(!w.valid);
}

// ─── L5 step 3 batch 11: overflow-x/-y + float + clear ────────────────

#[test]
fn cascade_typed_overflow_xy() {
    use crate::browser::computed_style::Overflow;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { overflow-x: scroll; overflow-y: hidden; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.overflow_x, Overflow::Scroll);
    assert_eq!(cs.overflow_y, Overflow::Hidden);
    assert!(cs.overflow_x.is_scrollable());
    assert!(!cs.overflow_y.is_scrollable());
}

#[test]
fn cascade_typed_overflow_shorthand() {
    use crate::browser::computed_style::Overflow;
    // overflow: auto rozkladame v expand_shorthand drive cascade.
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { overflow: auto; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.overflow_x, Overflow::Auto);
    assert_eq!(cs.overflow_y, Overflow::Auto);
}

#[test]
fn cascade_typed_float_right() {
    use crate::browser::computed_style::Float;
    let doc = parse_html("<html><body><img></body></html>", "");
    let css = parse_stylesheet("img { float: right; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("img")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert_eq!(cs.float, Float::Right);
}

#[test]
fn cascade_typed_clear_both() {
    use crate::browser::computed_style::Clear;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { clear: both; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.clear, Clear::Both);
}

#[test]
fn cascade_typed_overflow_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { overflow-x: floppy; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let o = decls.iter().find(|d| d.raw_value == "floppy").unwrap();
    assert!(!o.valid);
}

// ─── L5 step 3 batch 12: flex_direction/flex_wrap/flex_grow/flex_shrink

#[test]
fn cascade_typed_flex_direction_column() {
    use crate::browser::computed_style::FlexDirection;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-direction: column; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.flex_direction, FlexDirection::Column);
    assert!(!cs.flex_direction.is_row());
}

#[test]
fn cascade_typed_flex_wrap_wrap_reverse() {
    use crate::browser::computed_style::FlexWrap;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-wrap: wrap-reverse; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.flex_wrap, FlexWrap::WrapReverse);
}

#[test]
fn cascade_typed_flex_grow_shrink() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-grow: 2; flex-shrink: 0; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert!((cs.flex_grow - 2.0).abs() < 0.001);
    assert!((cs.flex_shrink - 0.0).abs() < 0.001);
}

#[test]
fn cascade_typed_flex_grow_negative_clamped() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-grow: -1; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    // CSS spec: negative invalid -> clamp na 0 (max(0)).
    assert!((cs.flex_grow - 0.0).abs() < 0.001);
}

#[test]
fn cascade_typed_flex_direction_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-direction: diagonal; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let f = decls.iter().find(|d| d.raw_value == "diagonal").unwrap();
    assert!(!f.valid);
}

// ─── L5 step 3 batch 13: justify-content/align-items/-content/-self ────

#[test]
fn cascade_typed_justify_content_space_between() {
    use crate::browser::computed_style::JustifyContent;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { justify-content: space-between; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.justify_content, JustifyContent::SpaceBetween);
}

#[test]
fn cascade_typed_align_items_baseline() {
    use crate::browser::computed_style::AlignItems;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { align-items: baseline; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.align_items, AlignItems::Baseline);
}

#[test]
fn cascade_typed_align_content_stretch() {
    use crate::browser::computed_style::AlignContent;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { align-content: stretch; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.align_content, AlignContent::Stretch);
}

#[test]
fn cascade_typed_align_self_flex_end() {
    use crate::browser::computed_style::AlignSelf;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { align-self: flex-end; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.align_self, AlignSelf::FlexEnd);
}

#[test]
fn cascade_typed_justify_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { justify-content: spaced-out; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let j = decls.iter().find(|d| d.raw_value == "spaced-out").unwrap();
    assert!(!j.valid);
}

// ─── L5 step 3 batch 14: flex_basis/order/row_gap/column_gap ──────────

#[test]
fn cascade_typed_flex_basis_length() {
    use crate::browser::computed_style::{FlexBasis, Length};
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-basis: 200px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.flex_basis, FlexBasis::Length(Length::Px(200.0)));
}

#[test]
fn cascade_typed_flex_basis_auto() {
    use crate::browser::computed_style::FlexBasis;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-basis: auto; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.flex_basis, FlexBasis::Auto);
}

#[test]
fn cascade_typed_flex_basis_content() {
    use crate::browser::computed_style::FlexBasis;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { flex-basis: content; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.flex_basis, FlexBasis::Content);
}

#[test]
fn cascade_typed_order_negative() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { order: -1; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.order, -1);
}

#[test]
fn cascade_typed_row_column_gap() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { row-gap: 8px; column-gap: 16px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.row_gap, Length::Px(8.0));
    assert_eq!(cs.column_gap, Length::Px(16.0));
}

#[test]
fn cascade_typed_gap_shorthand_expand() {
    use crate::browser::computed_style::Length;
    // gap: <row> <col> -> row-gap + column-gap (existing expand_shorthand).
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { gap: 10px 20px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.row_gap, Length::Px(10.0));
    assert_eq!(cs.column_gap, Length::Px(20.0));
}

// ─── L5 step 3 batch 15: border-*-width ───────────────────────────────

#[test]
fn cascade_typed_border_widths() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { border-top-width: 1px; border-right-width: 2px; \
         border-bottom-width: 3px; border-left-width: 4px; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_width, Length::Px(1.0));
    assert_eq!(cs.border_right_width, Length::Px(2.0));
    assert_eq!(cs.border_bottom_width, Length::Px(3.0));
    assert_eq!(cs.border_left_width, Length::Px(4.0));
}

#[test]
fn cascade_typed_border_width_keywords() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-top-width: thin; border-right-width: medium; border-bottom-width: thick; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_width, Length::Px(1.0));
    assert_eq!(cs.border_right_width, Length::Px(3.0));
    assert_eq!(cs.border_bottom_width, Length::Px(5.0));
}

#[test]
fn cascade_typed_border_width_shorthand() {
    use crate::browser::computed_style::Length;
    // border-width: 5px expand to all 4 sides (uz mame v expand_shorthand).
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-width: 5px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_width, Length::Px(5.0));
    assert_eq!(cs.border_left_width, Length::Px(5.0));
}

#[test]
fn cascade_typed_border_width_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-top-width: chunky; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let b = decls.iter().find(|d| d.raw_value == "chunky").unwrap();
    assert!(!b.valid);
}

// ─── L5 step 3 batch 16: border-*-color ───────────────────────────────

#[test]
fn cascade_typed_border_colors() {
    use crate::browser::computed_style::Color;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { border-top-color: red; border-right-color: blue; \
         border-bottom-color: #00ff00; border-left-color: rgb(128, 128, 128); }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_color, Color::Rgba { r: 255, g: 0, b: 0, a: 255 });
    assert_eq!(cs.border_right_color, Color::Rgba { r: 0, g: 0, b: 255, a: 255 });
    assert_eq!(cs.border_bottom_color, Color::Rgba { r: 0, g: 255, b: 0, a: 255 });
    assert_eq!(cs.border_left_color, Color::Rgba { r: 128, g: 128, b: 128, a: 255 });
}

#[test]
fn cascade_typed_border_color_currentcolor() {
    use crate::browser::computed_style::Color;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-top-color: currentColor; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_color, Color::CurrentColor);
}

#[test]
fn cascade_typed_border_color_shorthand() {
    use crate::browser::computed_style::Color;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-color: red green; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    // 2-value: top/bottom = red, right/left = green.
    assert_eq!(cs.border_top_color, Color::Rgba { r: 255, g: 0, b: 0, a: 255 });
    assert_eq!(cs.border_right_color, Color::Rgba { r: 0, g: 128, b: 0, a: 255 });
    assert_eq!(cs.border_bottom_color, Color::Rgba { r: 255, g: 0, b: 0, a: 255 });
    assert_eq!(cs.border_left_color, Color::Rgba { r: 0, g: 128, b: 0, a: 255 });
}

#[test]
fn cascade_typed_border_color_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-top-color: notacolor; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let b = decls.iter().find(|d| d.raw_value == "notacolor").unwrap();
    assert!(!b.valid);
}

// ─── L5 step 3 batch 17: border styles ────────────────────────────────

#[test]
fn cascade_typed_border_styles() {
    use crate::browser::computed_style::BorderStyle;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { border-top-style: solid; border-right-style: dashed; \
         border-bottom-style: dotted; border-left-style: double; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_style, BorderStyle::Solid);
    assert_eq!(cs.border_right_style, BorderStyle::Dashed);
    assert_eq!(cs.border_bottom_style, BorderStyle::Dotted);
    assert_eq!(cs.border_left_style, BorderStyle::Double);
}

#[test]
fn cascade_typed_border_style_shorthand() {
    use crate::browser::computed_style::BorderStyle;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-style: groove; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_style, BorderStyle::Groove);
    assert_eq!(cs.border_left_style, BorderStyle::Groove);
}

#[test]
fn cascade_typed_border_style_invalid_marked() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-top-style: pretty; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let decls = out.declarations.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    let s = decls.iter().find(|d| d.raw_value == "pretty").unwrap();
    assert!(!s.valid);
}

// ─── L5 step 3 batch 18: border-radius ────────────────────────────────

#[test]
fn cascade_typed_border_radius_corners() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { border-top-left-radius: 2px; border-top-right-radius: 4px; \
         border-bottom-right-radius: 6px; border-bottom-left-radius: 8px; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_left_radius, Length::Px(2.0));
    assert_eq!(cs.border_top_right_radius, Length::Px(4.0));
    assert_eq!(cs.border_bottom_right_radius, Length::Px(6.0));
    assert_eq!(cs.border_bottom_left_radius, Length::Px(8.0));
}

#[test]
fn cascade_typed_border_radius_shorthand_all() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-radius: 10px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_left_radius, Length::Px(10.0));
    assert_eq!(cs.border_top_right_radius, Length::Px(10.0));
    assert_eq!(cs.border_bottom_right_radius, Length::Px(10.0));
    assert_eq!(cs.border_bottom_left_radius, Length::Px(10.0));
}

#[test]
fn cascade_typed_border_radius_shorthand_4values() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { border-radius: 1px 2px 3px 4px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.border_top_left_radius, Length::Px(1.0));
    assert_eq!(cs.border_top_right_radius, Length::Px(2.0));
    assert_eq!(cs.border_bottom_right_radius, Length::Px(3.0));
    assert_eq!(cs.border_bottom_left_radius, Length::Px(4.0));
}

// ─── L5 step 3 batch 19: outline ──────────────────────────────────────

#[test]
fn cascade_typed_outline_props() {
    use crate::browser::computed_style::{BorderStyle, Color, Length};
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { outline-width: 2px; outline-style: solid; \
         outline-color: red; outline-offset: 4px; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.outline_width, Length::Px(2.0));
    assert_eq!(cs.outline_style, BorderStyle::Solid);
    assert_eq!(cs.outline_color, Color::Rgba { r: 255, g: 0, b: 0, a: 255 });
    assert_eq!(cs.outline_offset, Length::Px(4.0));
}

#[test]
fn cascade_typed_outline_width_thick() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { outline-width: thick; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.outline_width, Length::Px(5.0));
}

#[test]
fn cascade_typed_outline_offset_negative() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { outline-offset: -2px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.outline_offset, Length::Px(-2.0));
}

// ─── L5 step 3 batch 20: text-decoration ──────────────────────────────

#[test]
fn cascade_typed_text_decoration_underline() {
    use crate::browser::computed_style::TextDecorationLine;
    let doc = parse_html("<html><body><a></a></body></html>", "");
    let css = parse_stylesheet("a { text-decoration-line: underline; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let a = doc.root.find(|n| n.tag_name().as_deref() == Some("a")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&a) as usize)).unwrap();
    assert_eq!(cs.text_decoration_line, TextDecorationLine::UNDERLINE);
    assert!(cs.text_decoration_line.has_underline());
    assert!(!cs.text_decoration_line.has_line_through());
}

#[test]
fn cascade_typed_text_decoration_combined() {
    use crate::browser::computed_style::TextDecorationLine;
    let doc = parse_html("<html><body><s></s></body></html>", "");
    let css = parse_stylesheet("s { text-decoration-line: underline line-through; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let s = doc.root.find(|n| n.tag_name().as_deref() == Some("s")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&s) as usize)).unwrap();
    assert!(cs.text_decoration_line.has_underline());
    assert!(cs.text_decoration_line.has_line_through());
    assert_eq!(cs.text_decoration_line, TextDecorationLine(5)); // 1|4
}

#[test]
fn cascade_typed_text_decoration_style_color() {
    use crate::browser::computed_style::{TextDecorationStyle, Color};
    let doc = parse_html("<html><body><u></u></body></html>", "");
    let css = parse_stylesheet("u { text-decoration-style: wavy; text-decoration-color: red; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let u = doc.root.find(|n| n.tag_name().as_deref() == Some("u")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&u) as usize)).unwrap();
    assert_eq!(cs.text_decoration_style, TextDecorationStyle::Wavy);
    assert_eq!(cs.text_decoration_color, Color::Rgba { r: 255, g: 0, b: 0, a: 255 });
}

#[test]
fn cascade_typed_text_decoration_thickness() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><u></u></body></html>", "");
    let css = parse_stylesheet("u { text-decoration-thickness: 3px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let u = doc.root.find(|n| n.tag_name().as_deref() == Some("u")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&u) as usize)).unwrap();
    assert_eq!(cs.text_decoration_thickness, Length::Px(3.0));
}

// ─── L5 step 3 batch 21: text-indent/-transform/-overflow + vert-align ──

#[test]
fn cascade_typed_text_indent_em() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><p></p></body></html>", "");
    let css = parse_stylesheet("p { text-indent: 2em; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.text_indent, Length::Em(2.0));
}

#[test]
fn cascade_typed_text_transform_uppercase() {
    use crate::browser::computed_style::TextTransform;
    let doc = parse_html("<html><body><p></p></body></html>", "");
    let css = parse_stylesheet("p { text-transform: uppercase; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("p")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert_eq!(cs.text_transform, TextTransform::Uppercase);
}

#[test]
fn cascade_typed_text_overflow_ellipsis() {
    use crate::browser::computed_style::TextOverflow;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { text-overflow: ellipsis; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.text_overflow, TextOverflow::Ellipsis);
}

#[test]
fn cascade_typed_vertical_align_middle() {
    use crate::browser::computed_style::VerticalAlign;
    let doc = parse_html("<html><body><img></body></html>", "");
    let css = parse_stylesheet("img { vertical-align: middle; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("img")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert_eq!(cs.vertical_align, VerticalAlign::Middle);
}

#[test]
fn cascade_typed_vertical_align_length() {
    use crate::browser::computed_style::{VerticalAlign, Length};
    let doc = parse_html("<html><body><img></body></html>", "");
    let css = parse_stylesheet("img { vertical-align: -4px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("img")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert_eq!(cs.vertical_align, VerticalAlign::Length(Length::Px(-4.0)));
}

// ─── L5 step 3 batch 22: list-style + tab-size ────────────────────────

#[test]
fn cascade_typed_list_style_type() {
    use crate::browser::computed_style::ListStyleType;
    let doc = parse_html("<html><body><ul></ul></body></html>", "");
    let css = parse_stylesheet("ul { list-style-type: square; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let u = doc.root.find(|n| n.tag_name().as_deref() == Some("ul")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&u) as usize)).unwrap();
    assert_eq!(cs.list_style_type, ListStyleType::Square);
}

#[test]
fn cascade_typed_list_style_image_url() {
    use crate::browser::computed_style::ListStyleImage;
    let doc = parse_html("<html><body><ul></ul></body></html>", "");
    let css = parse_stylesheet("ul { list-style-image: url(\"bullet.png\"); }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let u = doc.root.find(|n| n.tag_name().as_deref() == Some("ul")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&u) as usize)).unwrap();
    assert_eq!(cs.list_style_image, ListStyleImage::Url("bullet.png".into()));
}

#[test]
fn cascade_typed_list_style_position_inside() {
    use crate::browser::computed_style::ListStylePosition;
    let doc = parse_html("<html><body><ul></ul></body></html>", "");
    let css = parse_stylesheet("ul { list-style-position: inside; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let u = doc.root.find(|n| n.tag_name().as_deref() == Some("ul")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&u) as usize)).unwrap();
    assert_eq!(cs.list_style_position, ListStylePosition::Inside);
}

#[test]
fn cascade_typed_tab_size() {
    let doc = parse_html("<html><body><pre></pre></body></html>", "");
    let css = parse_stylesheet("pre { tab-size: 4; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let p = doc.root.find(|n| n.tag_name().as_deref() == Some("pre")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&p) as usize)).unwrap();
    assert!((cs.tab_size - 4.0).abs() < 0.001);
}

// ─── L5 step 3 batch 23: table props ──────────────────────────────────

#[test]
fn cascade_typed_border_collapse() {
    use crate::browser::computed_style::BorderCollapse;
    let doc = parse_html("<html><body><table></table></body></html>", "");
    let css = parse_stylesheet("table { border-collapse: collapse; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let t = doc.root.find(|n| n.tag_name().as_deref() == Some("table")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&t) as usize)).unwrap();
    assert_eq!(cs.border_collapse, BorderCollapse::Collapse);
}

#[test]
fn cascade_typed_border_spacing_pair() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><table></table></body></html>", "");
    let css = parse_stylesheet("table { border-spacing: 4px 8px; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let t = doc.root.find(|n| n.tag_name().as_deref() == Some("table")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&t) as usize)).unwrap();
    assert_eq!(cs.border_spacing_h, Length::Px(4.0));
    assert_eq!(cs.border_spacing_v, Length::Px(8.0));
}

#[test]
fn cascade_typed_table_layout_fixed() {
    use crate::browser::computed_style::TableLayout;
    let doc = parse_html("<html><body><table></table></body></html>", "");
    let css = parse_stylesheet("table { table-layout: fixed; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let t = doc.root.find(|n| n.tag_name().as_deref() == Some("table")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&t) as usize)).unwrap();
    assert_eq!(cs.table_layout, TableLayout::Fixed);
}

#[test]
fn cascade_typed_caption_side_bottom() {
    use crate::browser::computed_style::CaptionSide;
    let doc = parse_html("<html><body><table></table></body></html>", "");
    let css = parse_stylesheet("table { caption-side: bottom; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let t = doc.root.find(|n| n.tag_name().as_deref() == Some("table")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&t) as usize)).unwrap();
    assert_eq!(cs.caption_side, CaptionSide::Bottom);
}

// ─── L5 step 3 batch 24: object-fit/-position + aspect-ratio + resize ──

#[test]
fn cascade_typed_object_fit_cover() {
    use crate::browser::computed_style::ObjectFit;
    let doc = parse_html("<html><body><img></body></html>", "");
    let css = parse_stylesheet("img { object-fit: cover; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("img")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert_eq!(cs.object_fit, ObjectFit::Cover);
}

#[test]
fn cascade_typed_object_position_keywords() {
    use crate::browser::computed_style::Length;
    let doc = parse_html("<html><body><img></body></html>", "");
    let css = parse_stylesheet("img { object-position: right top; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("img")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert_eq!(cs.object_position_x, Length::Percent(100.0));
    assert_eq!(cs.object_position_y, Length::Percent(0.0));
}

#[test]
fn cascade_typed_aspect_ratio_pair() {
    let doc = parse_html("<html><body><img></body></html>", "");
    let css = parse_stylesheet("img { aspect-ratio: 16 / 9; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let i = doc.root.find(|n| n.tag_name().as_deref() == Some("img")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&i) as usize)).unwrap();
    assert!((cs.aspect_ratio.unwrap() - 16.0/9.0).abs() < 0.001);
}

#[test]
fn cascade_typed_resize_both() {
    use crate::browser::computed_style::Resize;
    let doc = parse_html("<html><body><textarea></textarea></body></html>", "");
    let css = parse_stylesheet("textarea { resize: both; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let t = doc.root.find(|n| n.tag_name().as_deref() == Some("textarea")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&t) as usize)).unwrap();
    assert_eq!(cs.resize, Resize::Both);
}

// ─── L5 step 3 batch 25: transitions ──────────────────────────────────

#[test]
fn cascade_typed_transition_duration_list() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { transition-duration: 1s, 500ms, 0.25s; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.transition_duration, vec![1.0, 0.5, 0.25]);
}

#[test]
fn cascade_typed_transition_timing_ease_in_out() {
    use crate::browser::computed_style::TimingFunction;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { transition-timing-function: ease-in-out; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.transition_timing_function, vec![TimingFunction::EaseInOut]);
}

#[test]
fn cascade_typed_transition_timing_cubic_bezier() {
    use crate::browser::computed_style::TimingFunction;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { transition-timing-function: cubic-bezier(0.25, 0.1, 0.25, 1.0); }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.transition_timing_function, vec![TimingFunction::CubicBezier(0.25, 0.1, 0.25, 1.0)]);
}

#[test]
fn cascade_typed_transition_delay() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { transition-delay: 200ms; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.transition_delay, vec![0.2]);
}

// ─── L5 step 3 batch 26: animation timing ─────────────────────────────

#[test]
fn cascade_typed_animation_name_list() {
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet("div { animation-name: slideIn, fadeOut; }");
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.animation_name, vec!["slideIn".to_string(), "fadeOut".to_string()]);
}

#[test]
fn cascade_typed_animation_duration_timing_delay() {
    use crate::browser::computed_style::TimingFunction;
    let doc = parse_html("<html><body><div></div></body></html>", "");
    let css = parse_stylesheet(
        "div { animation-duration: 2s; animation-timing-function: linear; \
         animation-delay: 100ms; }"
    );
    let out = cascade::cascade_with_viewport_typed(&doc.root, &[css], 800.0, 600.0);
    let d = doc.root.find(|n| n.tag_name().as_deref() == Some("div")).unwrap();
    let cs = out.computed.get(&(std::rc::Rc::as_ptr(&d) as usize)).unwrap();
    assert_eq!(cs.animation_duration, vec![2.0]);
    assert_eq!(cs.animation_timing_function, vec![TimingFunction::Linear]);
    assert_eq!(cs.animation_delay, vec![0.1]);
}
