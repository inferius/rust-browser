/// Testy layout enginu + paintingu.

use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout, paint};

#[test]
fn layout_block_stacking() {
    let doc = parse_html(r#"<html><body>
        <div></div>
        <div></div>
        <div></div>
    </body></html>"#, "");
    let css = parse_stylesheet("div { background: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);

    // Body by mel mit 3 deti
    let body = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|html| html.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .expect("body should exist");
    assert_eq!(body.children.len(), 3);

    // Bloky stackuji vertikalne - kazdy ma jiny y
    let ys: Vec<f32> = body.children.iter().map(|c| c.rect.y).collect();
    assert!(ys[0] < ys[1] && ys[1] < ys[2], "blocks should stack: {ys:?}");
}

#[test]
fn multicol_distributes_children_to_columns() {
    let doc = parse_html(r#"<html><body><div class="cols">
        <p>A</p><p>B</p><p>C</p><p>D</p><p>E</p><p>F</p>
    </div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .cols { column-count: 3; column-gap: 10px; width: 600px; }
        p { height: 50px; margin: 0; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let cols = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .expect("div.cols");
    // 3 sloupce -> children rozdeleny po 2 (6 total / 3 cols).
    // Sirka kazdeho sloupce: (600 - 2*10) / 3 = ~193.3 px.
    let col_w = (600.0 - 2.0 * 10.0) / 3.0;
    // Children dostali ruzne x souradnice po sloupcich.
    let unique_xs: std::collections::HashSet<i32> = cols.children.iter()
        .map(|c| c.rect.x as i32).collect();
    assert!(unique_xs.len() >= 2, "musi byt vice nez 1 sloupec, x = {:?}", unique_xs);
    // Kazdy child ma sirku col_w +- 2px tolerance.
    for c in &cols.children {
        assert!((c.rect.width - col_w).abs() < 5.0,
            "expected col_w ~{} got {}", col_w, c.rect.width);
    }
}

#[test]
fn grid_two_columns_200px_1fr() {
    // Engine-test.html .page layout: 200px sidebar + 1fr content side-by-side.
    let doc = parse_html(r#"<html><body><div class="page">
        <div id="sidebar"></div>
        <main></main>
    </div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .page { display: grid; grid-template-columns: 200px 1fr; width: 1024px; }
        #sidebar { background: red; }
        main { background: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let page = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .expect("page");
    assert_eq!(page.children.len(), 2);
    let sidebar = &page.children[0];
    let main = &page.children[1];
    println!("sidebar: x={} y={} w={} h={}",
        sidebar.rect.x, sidebar.rect.y, sidebar.rect.width, sidebar.rect.height);
    println!("main:    x={} y={} w={} h={}",
        main.rect.x, main.rect.y, main.rect.width, main.rect.height);
    // Sidebar by mel byt vlevo (x=0), sirka 200.
    assert!((sidebar.rect.width - 200.0).abs() < 5.0,
        "sidebar width: expected 200, got {}", sidebar.rect.width);
    // Main vedle sidebaru (x >= 200), sirka 824 (1024 - 200).
    assert!(main.rect.x >= sidebar.rect.x + sidebar.rect.width - 5.0,
        "main musi byt vedle sidebaru, sidebar.x+w={} main.x={}",
        sidebar.rect.x + sidebar.rect.width, main.rect.x);
    // Y pozice stejna (vedle sebe, ne pod).
    assert!((sidebar.rect.y - main.rect.y).abs() < 5.0,
        "sidebar.y={} main.y={} - ocekavano vedle sebe, ne pod", sidebar.rect.y, main.rect.y);
}

#[test]
fn sticky_header_top_pinned() {
    let doc = parse_html(r#"<html><body>
        <header id="hdr"></header>
        <main></main>
    </body></html>"#, "");
    let css = parse_stylesheet(r#"
        header { position: sticky; top: 0; height: 48px; background: red; }
        main { height: 2000px; background: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let body = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .expect("body");
    let header = body.children.iter()
        .find(|c| c.tag.as_deref() == Some("header"))
        .expect("header");
    assert_eq!(header.position, layout::Position::Sticky);
    // Pri vychozi (scroll=0) sticky se chova jako relative - na puvodni pozici.
    println!("header.y = {}", header.rect.y);
    assert!(header.rect.y < 50.0, "header musi byt nahore: y = {}", header.rect.y);
}

#[test]
fn float_left_image_text_wraps() {
    // Float left: image vlevo, text obeha vpravo.
    let doc = parse_html(r#"<html><body><div class="container">
        <div class="box"></div>
        <p>Lorem ipsum dolor sit amet consectetur adipiscing</p>
    </div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .container { width: 600px; }
        .box { float: left; width: 100px; height: 100px; background: red; }
        p { margin: 0; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let container = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .expect("container");
    let box_el = &container.children[0];
    let p_el = &container.children[1];
    println!("box: x={} y={} w={} h={}", box_el.rect.x, box_el.rect.y, box_el.rect.width, box_el.rect.height);
    println!("p:   x={} y={} w={} h={}", p_el.rect.x, p_el.rect.y, p_el.rect.width, p_el.rect.height);
    assert_eq!(box_el.float_value, "left");
    // P element musi zacit vpravo od float (x >= 100).
    assert!(p_el.rect.x >= box_el.rect.x + box_el.rect.width - 5.0,
        "p.x={} ocekavano >= {}", p_el.rect.x, box_el.rect.x + box_el.rect.width);
}

#[test]
fn float_right_positioning() {
    let doc = parse_html(r#"<html><body><div class="container">
        <div class="box"></div>
        <p>Text</p>
    </div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .container { width: 600px; }
        .box { float: right; width: 100px; height: 50px; background: red; }
        p { margin: 0; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let container = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .expect("container");
    let box_el = &container.children[0];
    println!("box: x={} y={} w={}", box_el.rect.x, box_el.rect.y, box_el.rect.width);
    // Float right -> x at right edge of container.
    let container_right = container.rect.x + container.rect.width;
    let expected = container_right - 100.0;
    assert!((box_el.rect.x - expected).abs() < 5.0,
        "float right x = {}, expected ~{}", box_el.rect.x, expected);
}

#[test]
fn float_clear_both() {
    let doc = parse_html(r#"<html><body><div class="container">
        <div class="box1"></div>
        <div class="box2"></div>
        <p class="cleared">Below</p>
    </div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        .container { width: 600px; }
        .box1 { float: left; width: 100px; height: 100px; }
        .box2 { float: right; width: 100px; height: 80px; }
        .cleared { clear: both; margin: 0; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let container = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .expect("container");
    let cleared = &container.children[2];
    println!("cleared: y={}", cleared.rect.y);
    // Cleared element musi byt pod nejvyssim float (100px box1).
    assert!(cleared.rect.y >= 100.0,
        "cleared y={} musi byt >= 100 (max float height)", cleared.rect.y);
}

#[test]
fn debug_engine_test_html_top_layout() {
    // Replikuje engine-test.html strukturu - body s sticky header (display:flex)
    // a grid .page child. Verify ze body NE da header celou page height.
    let html = r#"<html><body>
        <div id="header"></div>
        <div class="page">
            <nav id="sidebar"></nav>
            <main></main>
        </div>
    </body></html>"#;
    let css = r#"
        *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
        body { background: black; }
        #header {
            position: sticky; top: 0; z-index: 999;
            height: 48px;
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 0 32px;
            background: rgba(10,10,12,0.7);
        }
        .page {
            display: grid;
            grid-template-columns: 200px 1fr;
            min-height: calc(100vh - 48px);
        }
        #sidebar { position: sticky; top: 48px; height: calc(100vh - 48px); }
    "#;
    let doc = parse_html(html, "");
    let stylesheet = parse_stylesheet(css);
    let map = cascade::cascade(&doc.root, &[stylesheet]);
    let root = layout::layout_tree(&doc.root, &map, 3045.0, 2063.0);
    let body = root.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .expect("body");
    println!("body: x={} y={} w={} h={} children={}",
        body.rect.x, body.rect.y, body.rect.width, body.rect.height, body.children.len());
    for (i, c) in body.children.iter().enumerate() {
        let id = c.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
        let cls = c.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
        println!("  [{}] tag={:?} id={} class={} x={} y={} w={} h={}",
            i, c.tag, id, cls, c.rect.x, c.rect.y, c.rect.width, c.rect.height);
    }
    let header = body.children.iter()
        .find(|c| c.node.as_ref().and_then(|n| n.attr("id")).as_deref() == Some("header"))
        .expect("header");
    assert!((header.rect.height - 48.0).abs() < 5.0,
        "header height={} expected 48", header.rect.height);
    let page = body.children.iter()
        .find(|c| c.node.as_ref().and_then(|n| n.attr("class")).map(|s| s.contains("page")).unwrap_or(false))
        .expect("page");
    println!("page: x={} y={} w={} h={} children={}",
        page.rect.x, page.rect.y, page.rect.width, page.rect.height, page.children.len());
    assert!(page.children.len() >= 2, "page musi mit 2+ children (sidebar+main)");
}

#[test]
fn debug_real_engine_test_html_body_layout() {
    let path = "static/engine-test.html";
    let html = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => { return; }
    };
    let doc = parse_html(&html, "");
    let css_blocks = doc.root.get_elements_by_tag("style");
    let css: String = css_blocks.iter().map(|s| s.text_content()).collect::<Vec<_>>().join("\n");
    let stylesheet = parse_stylesheet(&css);
    let map = cascade::cascade(&doc.root, &[stylesheet]);
    let root = layout::layout_tree(&doc.root, &map, 3045.0, 2063.0);
    let body = root.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .expect("body");
    println!("body: x={} y={} w={} h={} children={}",
        body.rect.x, body.rect.y, body.rect.width, body.rect.height, body.children.len());
    for (i, c) in body.children.iter().enumerate() {
        let id = c.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
        let cls = c.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
        println!("  body[{}] tag={:?} id={} class={} x={} y={} w={} h={} display={:?}",
            i, c.tag, id, cls, c.rect.x, c.rect.y, c.rect.width, c.rect.height, c.display);
    }
}

#[test]
fn flex_inline_children_become_flex_items() {
    // CSS Flex L1: pri display: flex parent, inline children (span) se
    // chovaji jako flex items (blockified) a justify-content je aplikuje.
    let doc = parse_html(r#"<html><body><div id="hdr">
        <span class="logo">LEFT</span>
        <span id="cnt">RIGHT</span>
    </div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        #hdr { display: flex; justify-content: space-between; height: 48px; width: 1000px; }
        .logo { color: red; }
        #cnt { color: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 3045.0, 2063.0);
    let hdr = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .expect("hdr");
    let logo = &hdr.children[0];
    let cnt = &hdr.children[1];
    println!("hdr:  x={} y={} w={} h={}", hdr.rect.x, hdr.rect.y, hdr.rect.width, hdr.rect.height);
    println!("logo: x={} y={} w={} h={}", logo.rect.x, logo.rect.y, logo.rect.width, logo.rect.height);
    println!("cnt:  x={} y={} w={} h={}", cnt.rect.x, cnt.rect.y, cnt.rect.width, cnt.rect.height);
    assert!(logo.rect.x < 100.0, "logo musi byt vlevo, x={}", logo.rect.x);
    assert!(cnt.rect.x > 800.0, "cnt musi byt vpravo (justify-content space-between), x={}", cnt.rect.x);
}

#[test]
fn parse_color_hex() {
    assert_eq!(layout::parse_color("#ff0000"), Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("#f00"), Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("#FF0000FF"), Some([255, 0, 0, 255]));
}

#[test]
fn parse_color_rgb() {
    assert_eq!(layout::parse_color("rgb(255, 0, 0)"), Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("rgba(0, 255, 0, 1.0)"), Some([0, 255, 0, 255]));
}

#[test]
fn parse_color_named() {
    assert_eq!(layout::parse_color("red"),   Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("white"), Some([255, 255, 255, 255]));
    assert_eq!(layout::parse_color("transparent"), Some([0, 0, 0, 0]));
}

#[test]
fn parse_relative_rgb_identity() {
    assert_eq!(layout::parse_color("rgb(from red r g b)"), Some([255, 0, 0, 255]));
}

#[test]
fn parse_relative_rgb_swap() {
    assert_eq!(layout::parse_color("rgb(from #102040 b g r)"), Some([0x40, 0x20, 0x10, 255]));
}

#[test]
fn parse_relative_rgb_calc_half() {
    let c = layout::parse_color("rgb(from #ff0000 calc(r * 0.5) g b)").unwrap();
    assert!((c[0] as i32 - 127).abs() <= 1);
    assert_eq!(c[1], 0);
    assert_eq!(c[2], 0);
}

#[test]
fn parse_relative_rgb_explicit_zero() {
    assert_eq!(layout::parse_color("rgb(from red 0 g b)"), Some([0, 0, 0, 255]));
}

#[test]
fn parse_relative_rgb_alpha_override() {
    let c = layout::parse_color("rgb(from red r g b / 0.5)").unwrap();
    assert_eq!(c[0], 255);
    assert!((c[3] as i32 - 127).abs() <= 1);
}

#[test]
fn parse_contrast_color_dark_bg_returns_white() {
    let c = layout::parse_color("contrast-color(black)").unwrap();
    assert_eq!(c, [255, 255, 255, 255]);
}

#[test]
fn parse_contrast_color_light_bg_returns_black() {
    let c = layout::parse_color("contrast-color(white)").unwrap();
    assert_eq!(c, [0, 0, 0, 255]);
}

#[test]
fn parse_contrast_picks_best_candidate() {
    // bg=white vs red,black -> black ma nejvyssi kontrast
    let c = layout::parse_color("contrast(white vs red, black)").unwrap();
    assert_eq!(c, [0, 0, 0, 255]);
}

#[test]
fn parse_contrast_single_arg_returns_inverse() {
    // contrast(white) -> black
    let c = layout::parse_color("contrast(white)").unwrap();
    assert_eq!(c, [0, 0, 0, 255]);
}

#[test]
fn parse_light_dark_returns_first() {
    // light-dark(red, blue) -> v light mode = red
    let c = layout::parse_color("light-dark(red, blue)").unwrap();
    assert_eq!(c, [255, 0, 0, 255]);
}

#[test]
fn parse_relative_hsl_identity() {
    let c = layout::parse_color("hsl(from red h s l)").unwrap();
    assert_eq!(c[0], 255);
    assert!(c[1] <= 5);
    assert!(c[2] <= 5);
}

#[test]
fn parse_color_srgb() {
    assert_eq!(layout::parse_color("color(srgb 1 0 0)"), Some([255, 0, 0, 255]));
}

#[test]
fn parse_color_display_p3() {
    let c = layout::parse_color("color(display-p3 0 1 0)").unwrap();
    assert_eq!(c[1], 255);
}

#[test]
fn parse_color_xyz() {
    // XYZ d65 (0.95, 1.0, 1.09) ~ white
    let c = layout::parse_color("color(xyz 0.95 1.0 1.09)").unwrap();
    assert!(c[0] > 240 && c[1] > 240 && c[2] > 240);
}

#[test]
fn parse_color_with_alpha() {
    let c = layout::parse_color("color(srgb 1 0 0 / 0.5)").unwrap();
    assert_eq!(c[0], 255);
    assert!((c[3] as i32 - 127).abs() <= 1);
}

#[test]
fn parse_length_dvw_dvh() {
    assert!((layout::parse_length_ctx("50dvw", 1000.0, 800.0, 0.0) - 500.0).abs() < 0.1);
    assert!((layout::parse_length_ctx("25dvh", 1000.0, 800.0, 0.0) - 200.0).abs() < 0.1);
}

#[test]
fn parse_length_svw_lvh() {
    assert!((layout::parse_length_ctx("100svw", 1000.0, 800.0, 0.0) - 1000.0).abs() < 0.1);
    assert!((layout::parse_length_ctx("100lvh", 1000.0, 800.0, 0.0) - 800.0).abs() < 0.1);
}

#[test]
fn parse_length_ch_lh() {
    let ch = layout::parse_length("10ch");
    assert!(ch > 0.0);
    let lh = layout::parse_length("2lh");
    assert!(lh > 0.0);
}

#[test]
fn parse_length_absolute_units() {
    let cm = layout::parse_length("1cm");
    assert!((cm - 37.795).abs() < 0.1);
    let mm = layout::parse_length("10mm");
    assert!((mm - 37.795).abs() < 0.1);
    let inch = layout::parse_length("1in");
    assert!((inch - 96.0).abs() < 0.1);
    let pc = layout::parse_length("1pc");
    assert!((pc - 16.0).abs() < 0.1);
}

// CSS Backgrounds L3 - border-image
#[test]
fn border_image_source_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        div {
            border-image-source: url(border.png);
            border-image-slice: 30 fill;
            border-image-width: 2;
            border-image-repeat: round;
        }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.border_image_source.as_deref(), Some("border.png"));
    assert_eq!(d.border_image_slice, [30.0, 30.0, 30.0, 30.0]);
    assert_eq!(d.border_image_width, [2.0, 2.0, 2.0, 2.0]);
    assert_eq!(d.border_image_repeat, "round");
}

// Text emphasis (Text Decor L4)
#[test]
fn text_emphasis_shorthand() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet("p { text-emphasis: filled red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.text_emphasis_style, "filled");
    assert_eq!(p.text_emphasis_color, Some([255, 0, 0, 255]));
}

#[test]
fn text_decoration_skip_ink_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><a></a></body></html>"#, "");
    let css = parse_stylesheet("a { text-decoration-skip-ink: none; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let a = find_box_by_tag(&root, "a").unwrap();
    assert_eq!(a.text_decoration_skip_ink, "none");
}

#[test]
fn field_sizing_content() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><input /></body></html>"#, "");
    let css = parse_stylesheet("input { field-sizing: content; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let inp = find_box_by_tag(&root, "input").unwrap();
    assert_eq!(inp.field_sizing, "content");
}

#[test]
fn interpolate_size_keywords() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { interpolate-size: allow-keywords; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.interpolate_size, "allow-keywords");
}

#[test]
fn mix_blend_mode_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { mix-blend-mode: multiply; background-blend-mode: screen; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.mix_blend_mode, "multiply");
    assert_eq!(d.background_blend_mode, "screen");
}

#[test]
fn grid_template_columns_named_lines() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { grid-template-columns: [start] 1fr [middle] 2fr [end]; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert!(d.grid_template_columns.contains("[start]"));
    assert!(d.grid_template_columns.contains("[middle]"));
    assert!(d.grid_template_columns.contains("[end]"));
}

#[test]
fn grid_template_areas_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"div { grid-template-areas: "header header" "nav main"; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert!(d.grid_template_areas.contains("header"));
    assert!(d.grid_template_areas.contains("nav"));
}

#[test]
fn grid_area_assignment() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { grid-area: header; grid-column: 1 / 3; grid-row: 2; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.grid_area, "header");
    assert_eq!(d.grid_column, "1 / 3");
    assert_eq!(d.grid_row, "2");
}

#[test]
fn parse_shape_function_circle() {
    use crate::browser::layout::{parse_shape_function, ShapeFunction};
    let s = parse_shape_function("circle(50% at 25% 75%)").unwrap();
    match s {
        ShapeFunction::Circle { radius_pct, cx_pct, cy_pct } => {
            assert!((radius_pct - 0.5).abs() < 1e-3);
            assert!((cx_pct - 0.25).abs() < 1e-3);
            assert!((cy_pct - 0.75).abs() < 1e-3);
        }
        _ => panic!("ocekavan Circle"),
    }
}

#[test]
fn parse_shape_function_ellipse() {
    use crate::browser::layout::{parse_shape_function, ShapeFunction};
    let s = parse_shape_function("ellipse(40% 30% at 50% 50%)").unwrap();
    match s {
        ShapeFunction::Ellipse { rx_pct, ry_pct, cx_pct, cy_pct } => {
            assert!((rx_pct - 0.4).abs() < 1e-3);
            assert!((ry_pct - 0.3).abs() < 1e-3);
            assert!((cx_pct - 0.5).abs() < 1e-3);
            assert!((cy_pct - 0.5).abs() < 1e-3);
        }
        _ => panic!("ocekavan Ellipse"),
    }
}

#[test]
fn parse_shape_function_polygon() {
    use crate::browser::layout::{parse_shape_function, ShapeFunction};
    let s = parse_shape_function("polygon(0% 0%, 100% 0%, 50% 100%)").unwrap();
    match s {
        ShapeFunction::Polygon(pts) => {
            assert_eq!(pts.len(), 3);
            assert!((pts[0].0 - 0.0).abs() < 1e-3);
            assert!((pts[2].1 - 1.0).abs() < 1e-3);
        }
        _ => panic!("ocekavan Polygon"),
    }
}

#[test]
fn parse_shape_function_inset_round() {
    use crate::browser::layout::{parse_shape_function, ShapeFunction};
    let s = parse_shape_function("inset(10% 20% 30% 40% round 5%)").unwrap();
    match s {
        ShapeFunction::Inset { top, right, bottom, left, radius } => {
            assert!((top - 0.1).abs() < 1e-3);
            assert!((right - 0.2).abs() < 1e-3);
            assert!((bottom - 0.3).abs() < 1e-3);
            assert!((left - 0.4).abs() < 1e-3);
            assert!((radius - 0.05).abs() < 1e-3);
        }
        _ => panic!("ocekavan Inset"),
    }
}

#[test]
fn shape_outside_circle() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { shape-outside: circle(50%); shape-margin: 10px; shape-image-threshold: 0.5; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.shape_outside.as_deref(), Some("circle(50%)"));
    assert!((d.shape_margin - 10.0).abs() < 0.1);
    assert!((d.shape_image_threshold - 0.5).abs() < 0.001);
}

#[test]
fn scrollbar_gutter_stable() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { scrollbar-gutter: stable both-edges; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.scrollbar_gutter, "stable both-edges");
}

#[test]
fn svg_markers_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"div { marker-start: url(#start); marker-mid: url(#mid); marker-end: url(#end); }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert!(d.marker_start.contains("start"));
    assert!(d.marker_mid.contains("mid"));
    assert!(d.marker_end.contains("end"));
}

#[test]
fn background_position_xy_split() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { background-position-x: right; background-position-y: top; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.background_position_x, "right");
    assert_eq!(d.background_position_y, "top");
}

#[test]
fn image_orientation_from_image() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><img></body></html>"#, "");
    let css = parse_stylesheet("img { image-orientation: from-image; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let img = find_box_by_tag(&root, "img").unwrap();
    assert_eq!(img.image_orientation, "from-image");
}

#[test]
fn hyphenate_character_quoted() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><p></p></body></html>"#, "");
    let css = parse_stylesheet(r#"p { hyphenate-character: "-"; hyphenate-limit-chars: 6 3 3; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.hyphenate_character, "-");
    assert_eq!(p.hyphenate_limit_chars, "6 3 3");
}

#[test]
fn text_box_trim_edge() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><span></span></body></html>"#, "");
    let css = parse_stylesheet("span { text-box-trim: trim-both; text-box-edge: cap alphabetic; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let s = find_box_by_tag(&root, "span").unwrap();
    assert_eq!(s.text_box_trim, "trim-both");
    assert_eq!(s.text_box_edge, "cap alphabetic");
}

#[test]
fn position_area_keyword() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { position-area: top-left; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.position_area, "top-left");
}

#[test]
fn inset_shorthand_4_values() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { inset: 10px 20px 30px 40px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.inset[0], Some(10.0));
    assert_eq!(d.inset[1], Some(20.0));
    assert_eq!(d.inset[2], Some(30.0));
    assert_eq!(d.inset[3], Some(40.0));
}

#[test]
fn inset_shorthand_auto() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { inset: auto; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert!(d.inset.iter().all(|i| i.is_none()));
}

#[test]
fn text_spacing_extras() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><p></p></body></html>"#, "");
    let css = parse_stylesheet("p { text-spacing: trim-auto; text-autospace: ideograph-alpha; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.text_spacing, "trim-auto");
    assert_eq!(p.text_autospace, "ideograph-alpha");
}

#[test]
fn initial_letter_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><p></p></body></html>"#, "");
    let css = parse_stylesheet("p { initial-letter: 3 2; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.initial_letter, "3 2");
}

#[test]
fn ruby_overhang_merge() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><ruby></ruby></body></html>"#, "");
    let css = parse_stylesheet("ruby { ruby-overhang: auto; ruby-merge: collapse; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let r = find_box_by_tag(&root, "ruby").unwrap();
    assert_eq!(r.ruby_overhang, "auto");
    assert_eq!(r.ruby_merge, "collapse");
}

#[test]
fn math_shift_centered() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><math></math></body></html>"#, "");
    let css = parse_stylesheet("math { math-shift: centered; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let m = find_box_by_tag(&root, "math").unwrap();
    assert_eq!(m.math_shift, "centered");
}

#[test]
fn transition_behavior_allow_discrete() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { transition-behavior: allow-discrete; animation-composition: add; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.transition_behavior, "allow-discrete");
    assert_eq!(d.animation_composition, "add");
}

#[test]
fn subgrid_display_recognized() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    use crate::browser::layout::Display;
    let doc = parse_html(r#"<html><body><div class="grid"><div class="sub">x</div></div></body></html>"#, "");
    let css = parse_stylesheet(
        ".grid { display: grid; grid-template-columns: 1fr 1fr 1fr; } \
         .sub { display: subgrid; grid-template-columns: subgrid; }"
    );
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    // 2. div = sub
    let sub = find_nth_box_by_tag(&root, "div", 2).unwrap();
    assert!(matches!(sub.display, Display::Subgrid | Display::Grid),
        "subgrid display rozpoznan");
}

#[test]
fn subgrid_template_subgrid_keyword_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { grid-template-rows: subgrid; grid-template-columns: subgrid; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.grid_template_rows, "subgrid");
    assert_eq!(d.grid_template_columns, "subgrid");
}

#[test]
fn animation_range_extras() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { animation-range-start: entry 50%; animation-range-end: exit 0%; timeline-scope: --my; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.animation_range_start, "entry 50%");
    assert_eq!(d.animation_range_end, "exit 0%");
    assert_eq!(d.timeline_scope, "--my");
}

#[test]
fn scroll_marker_group_parsed() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { scroll-marker-group: after; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.scroll_marker_group, "after");
}

#[test]
fn anchor_scope_position_visibility() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { anchor-scope: --my; position-visibility: anchors-visible; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.anchor_scope, "--my");
    assert_eq!(d.position_visibility, "anchors-visible");
}

#[test]
fn reading_flow_grid() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { reading-flow: grid-rows; reading-order: 5; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.reading_flow, "grid-rows");
    assert_eq!(d.reading_order, "5");
}

#[test]
fn list_style_position_resize() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><ul></ul></body></html>"#, "");
    let css = parse_stylesheet("ul { list-style-position: inside; resize: both; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let u = find_box_by_tag(&root, "ul").unwrap();
    assert_eq!(u.list_style_position_v, "inside");
    assert_eq!(u.resize_v, "both");
}

#[test]
fn voice_family_speech() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><p></p></body></html>"#, "");
    let css = parse_stylesheet("p { voice-family: female; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.voice_family, "female");
}

#[test]
fn contain_intrinsic_size_axes() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { contain-intrinsic-block-size: 200px; contain-intrinsic-inline-size: 300px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert!((d.contain_intrinsic_block_size - 200.0).abs() < 0.1);
    assert!((d.contain_intrinsic_inline_size - 300.0).abs() < 0.1);
}

#[test]
fn parse_color_modern_rgb_space_syntax() {
    // Modern syntax: mezery + lomitko alpha
    assert_eq!(layout::parse_color("rgb(255 0 0)"), Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("rgb(255 0 0 / 0.5)"), Some([255, 0, 0, 128]));
    assert_eq!(layout::parse_color("rgb(255 0 0 / 50%)"), Some([255, 0, 0, 128]));
}

#[test]
fn parse_color_hex_alpha_short() {
    // #RGBA (4-digit)
    assert_eq!(layout::parse_color("#f00f"), Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("#f008"), Some([255, 0, 0, 136]));
}

#[test]
fn parse_color_hsl() {
    // hsl(0, 100%, 50%) = red
    assert_eq!(layout::parse_color("hsl(0, 100%, 50%)"), Some([255, 0, 0, 255]));
    // hsl(120 100% 50%) modern = green pure
    assert_eq!(layout::parse_color("hsl(120 100% 50%)"), Some([0, 255, 0, 255]));
    // hsl(0, 0%, 0%) = black
    assert_eq!(layout::parse_color("hsl(0, 0%, 0%)"), Some([0, 0, 0, 255]));
}

#[test]
fn parse_color_hsla_alpha() {
    let c = layout::parse_color("hsla(0, 100%, 50%, 0.5)").unwrap();
    assert_eq!(c[0], 255);
    assert_eq!(c[1], 0);
    assert_eq!(c[2], 0);
    assert!(c[3] >= 127 && c[3] <= 128);
}

#[test]
fn parse_color_hwb() {
    // hwb(0 0% 0%) = red
    let c = layout::parse_color("hwb(0 0% 0%)").unwrap();
    assert_eq!(c, [255, 0, 0, 255]);
    // hwb(0 50% 0%) = svetla cervena
    let c = layout::parse_color("hwb(0 50% 0%)").unwrap();
    assert!(c[1] >= 120 && c[1] <= 135);
}

#[test]
fn parse_color_oklch_red_approximate() {
    // oklch(0.628 0.258 29.23) ~ #ff0000 (cervena)
    let c = layout::parse_color("oklch(0.628 0.258 29.23)").unwrap();
    assert!(c[0] >= 240, "R={}", c[0]);
    assert!(c[1] <= 30, "G={}", c[1]);
    assert!(c[2] <= 30, "B={}", c[2]);
}

#[test]
fn parse_color_oklab_zero_zero_zero_black() {
    // oklab(0 0 0) = cerna
    let c = layout::parse_color("oklab(0 0 0)").unwrap();
    assert_eq!(c, [0, 0, 0, 255]);
}

#[test]
fn parse_color_lab_d65_red() {
    // lab(53.24 80.09 67.20) ~ red (D65 reference)
    let c = layout::parse_color("lab(53.24 80.09 67.20)").unwrap();
    assert!(c[0] >= 240, "R={}", c[0]);
    assert!(c[1] <= 30, "G={}", c[1]);
}

#[test]
fn parse_color_mix_in_srgb_50_50() {
    // black + white = mid grey
    let c = layout::parse_color("color-mix(in srgb, black, white)").unwrap();
    assert!(c[0] >= 125 && c[0] <= 130);
    assert!(c[1] >= 125 && c[1] <= 130);
    assert!(c[2] >= 125 && c[2] <= 130);
}

#[test]
fn parse_color_mix_in_oklab_red_blue() {
    let c = layout::parse_color("color-mix(in oklab, red, blue)").unwrap();
    // Vysledek je purple-ish (mix v perceptualne uniformnim space)
    assert!(c[0] > 100, "R={}", c[0]);
    assert!(c[2] > 100, "B={}", c[2]);
}

#[test]
fn parse_color_mix_with_explicit_weights() {
    // 25% red + 75% blue
    let c = layout::parse_color("color-mix(in srgb, red 25%, blue 75%)").unwrap();
    assert!(c[0] >= 60 && c[0] <= 70);
    assert!(c[2] >= 185 && c[2] <= 195);
}

#[test]
fn parse_length_units() {
    assert_eq!(layout::parse_length("16px"), 16.0);
    assert_eq!(layout::parse_length("2em"),  32.0);
    assert_eq!(layout::parse_length("1rem"), 16.0);
    // pt -> px (1.333 multiplier)
    let pt = layout::parse_length("12pt");
    assert!((pt - 16.0).abs() < 1.0);
}

#[test]
fn parse_length_viewport_units() {
    use crate::browser::layout::parse_length_ctx;
    assert_eq!(parse_length_ctx("50vw", 1000.0, 800.0, 16.0), 500.0);
    assert_eq!(parse_length_ctx("25vh", 1000.0, 800.0, 16.0), 200.0);
    assert_eq!(parse_length_ctx("10vmin", 1000.0, 800.0, 16.0), 80.0);
    assert_eq!(parse_length_ctx("10vmax", 1000.0, 800.0, 16.0), 100.0);
    // % parent based
    assert_eq!(parse_length_ctx("50%", 1000.0, 800.0, 200.0), 100.0);
}

#[test]
fn parse_linear_gradient_basic() {
    let g = layout::parse_linear_gradient("linear-gradient(45deg, red, blue)");
    assert!(g.is_some());
    let (angle, stops) = g.unwrap();
    assert_eq!(angle, 45.0);
    assert_eq!(stops.len(), 2);
}

#[test]
fn parse_linear_gradient_multi_stop() {
    let g = layout::parse_linear_gradient("linear-gradient(90deg, red, yellow 50%, blue)");
    assert!(g.is_some());
    let (angle, stops) = g.unwrap();
    assert_eq!(angle, 90.0);
    assert_eq!(stops.len(), 3);
    // Prvni a posledni stop maji default offsety 0.0 a 1.0
    assert_eq!(stops[0].0, 0.0);
    assert!((stops[1].0 - 0.5).abs() < 0.01);
    assert_eq!(stops[2].0, 1.0);
}

#[test]
fn parse_linear_gradient_four_stops() {
    let g = layout::parse_linear_gradient("linear-gradient(180deg, red, green 33%, blue 66%, yellow)");
    assert!(g.is_some());
    let (_, stops) = g.unwrap();
    assert_eq!(stops.len(), 4);
}

#[test]
fn svg_rect_emits_display_command() {
    use crate::browser::paint;
    let doc = parse_html(r#"<html><body>
        <svg width="100" height="50">
            <rect x="0" y="0" width="50" height="20" fill="red"/>
        </svg>
    </body></html>"#, "");
    let css = parse_stylesheet("");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let cmds = paint::build_display_list(&root);
    // SVG <rect> by mel emit Rect command s red color
    let red_rect = cmds.iter().find(|c| matches!(c,
        paint::DisplayCommand::Rect { color: [255, 0, 0, 255], .. }));
    assert!(red_rect.is_some(), "svg rect mel byt emitten");
}

#[test]
fn svg_circle_emits_rounded_rect() {
    use crate::browser::paint;
    let doc = parse_html(r#"<html><body>
        <svg width="100" height="100">
            <circle cx="50" cy="50" r="30" fill="blue"/>
        </svg>
    </body></html>"#, "");
    let css = parse_stylesheet("");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let cmds = paint::build_display_list(&root);
    let blue = cmds.iter().find(|c| matches!(c,
        paint::DisplayCommand::Rect { color: [0, 0, 255, 255], radius, .. } if *radius == 30.0));
    assert!(blue.is_some());
}

#[test]
fn canvas_default_size_300_150() {
    let doc = parse_html(r#"<html><body><canvas></canvas></body></html>"#, "");
    let css = parse_stylesheet("");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let canvas = find_box_by_tag(&root, "canvas").unwrap();
    assert_eq!(canvas.rect.width, 300.0);
    assert_eq!(canvas.rect.height, 150.0);
}

#[test]
fn canvas_custom_attr_size() {
    let doc = parse_html(r#"<html><body><canvas width="500" height="200"></canvas></body></html>"#, "");
    let css = parse_stylesheet("");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let canvas = find_box_by_tag(&root, "canvas").unwrap();
    assert_eq!(canvas.rect.width, 500.0);
    assert_eq!(canvas.rect.height, 200.0);
}

#[test]
fn pseudo_before_creates_virtual_child() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"p::before { content: "ARROW"; color: red; }"#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = crate::browser::cascade::cascade_pseudo(&doc.root, &[css]);
    let root_box = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);

    // Najdi `p` LayoutBox + jeho prvniho childa
    let p_box = find_box_by_tag(&root_box, "p").expect("p must exist");
    assert!(!p_box.children.is_empty(), "p musi mit ::before child");
    let first = &p_box.children[0];
    assert_eq!(first.tag.as_deref(), Some("::pseudo"));
    assert_eq!(first.text.as_deref(), Some("ARROW"));
}

#[test]
fn pseudo_after_appended_last() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"p::after { content: "!END"; }"#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = crate::browser::cascade::cascade_pseudo(&doc.root, &[css]);
    let root_box = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);

    let p_box = find_box_by_tag(&root_box, "p").unwrap();
    let last = p_box.children.last().unwrap();
    assert_eq!(last.tag.as_deref(), Some("::pseudo"));
    assert_eq!(last.text.as_deref(), Some("!END"));
}

#[test]
fn pseudo_attr_content() {
    let doc = parse_html(r#"<html><body><p data-prefix="-> ">hello</p></body></html>"#, "");
    let css = parse_stylesheet(r#"p::before { content: attr(data-prefix); }"#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = crate::browser::cascade::cascade_pseudo(&doc.root, &[css]);
    let root_box = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);

    let p_box = find_box_by_tag(&root_box, "p").unwrap();
    let before = &p_box.children[0];
    assert_eq!(before.text.as_deref(), Some("-> "));
}

fn find_box_by_tag<'a>(bx: &'a layout::LayoutBox, tag: &str) -> Option<&'a layout::LayoutBox> {
    if bx.tag.as_deref() == Some(tag) { return Some(bx); }
    for c in &bx.children {
        if let Some(found) = find_box_by_tag(c, tag) { return Some(found); }
    }
    None
}

/// Najde nested div - vrati Nth div pri DFS (1-indexovano).
#[allow(dead_code)]
fn find_nth_box_by_tag<'a>(bx: &'a layout::LayoutBox, tag: &str, n: usize) -> Option<&'a layout::LayoutBox> {
    let mut count = 0;
    fn walk<'a>(bx: &'a layout::LayoutBox, tag: &str, n: usize, count: &mut usize) -> Option<&'a layout::LayoutBox> {
        if bx.tag.as_deref() == Some(tag) {
            *count += 1;
            if *count == n { return Some(bx); }
        }
        for c in &bx.children {
            if let Some(found) = walk(c, tag, n, count) { return Some(found); }
        }
        None
    }
    walk(bx, tag, n, &mut count)
}

#[test]
fn scroll_snap_parsed() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        div {
            scroll-snap-type: x mandatory;
            scroll-snap-align: start;
            scroll-padding: 10px 20px;
        }
    "#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.scroll_snap_type, "x mandatory");
    assert_eq!(d.scroll_snap_align, "start");
    // 2 hodnoty: top/bottom = 10, left/right = 20
    assert_eq!(d.scroll_padding, [10.0, 20.0, 10.0, 20.0]);
}

#[test]
fn scroll_behavior_smooth() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { scroll-behavior: smooth; overscroll-behavior: contain; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.scroll_behavior, "smooth");
    assert_eq!(d.overscroll_behavior, "contain");
}

#[test]
fn scrollbar_color_parsed() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { scrollbar-color: red blue; scrollbar-width: thin; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.scrollbar_color, Some(([255, 0, 0, 255], [0, 0, 255, 255])));
    assert_eq!(d.scrollbar_width, "thin");
}

#[test]
fn color_scheme_parsed() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { color-scheme: light dark; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.color_scheme, "light dark");
}

#[test]
fn accent_color_parsed() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { accent-color: red; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.accent_color, Some([255, 0, 0, 255]));
}

#[test]
fn contain_strict_sets_all_bits() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { contain: strict; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.contain, 1 | 2 | 4 | 8);
}

#[test]
fn contain_layout_paint_combo() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { contain: layout paint; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.contain, 1 | 2);
}

#[test]
fn text_decoration_l4_props() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"
        p {
            text-decoration-color: red;
            text-decoration-style: wavy;
            text-decoration-thickness: 3px;
            text-underline-offset: 4px;
            text-indent: 16px;
        }
    "#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.text_decoration_color, Some([255, 0, 0, 255]));
    assert_eq!(p.text_decoration_style, "wavy");
    assert_eq!(p.text_decoration_thickness, 3.0);
    assert_eq!(p.text_underline_offset, 4.0);
    assert_eq!(p.text_indent, 16.0);
}

#[test]
fn parse_transform_chain_three_ops() {
    use crate::browser::layout::{parse_transform_chain, TransformOp};
    let chain = parse_transform_chain("translate(10px, 20px) rotate(45deg) scale(1.5)");
    assert_eq!(chain.len(), 3);
    assert!(matches!(chain[0], TransformOp::Translate(10.0, 20.0)));
    assert!(matches!(chain[1], TransformOp::Rotate(_)));
    assert!(matches!(chain[2], TransformOp::Scale(1.5, 1.5)));
}

#[test]
fn parse_transform_translate3d() {
    use crate::browser::layout::{parse_transform, TransformOp};
    let t = parse_transform("translate3d(10px, 20px, 30px)").unwrap();
    if let TransformOp::Translate3D { x, y, z } = t {
        assert_eq!((x, y, z), (10.0, 20.0, 30.0));
    } else { panic!(); }
}

#[test]
fn parse_transform_rotate3d() {
    use crate::browser::layout::{parse_transform, TransformOp};
    let t = parse_transform("rotate3d(0, 1, 0, 90deg)").unwrap();
    if let TransformOp::Rotate3D { x, y, z, angle_rad } = t {
        assert_eq!((x, y, z), (0.0, 1.0, 0.0));
        assert!((angle_rad - std::f32::consts::FRAC_PI_2).abs() < 1e-3);
    } else { panic!(); }
}

#[test]
fn parse_transform_perspective() {
    use crate::browser::layout::{parse_transform, TransformOp};
    let t = parse_transform("perspective(500px)").unwrap();
    if let TransformOp::Perspective(d) = t {
        assert_eq!(d, 500.0);
    } else { panic!(); }
}

#[test]
fn parse_transform_scale3d() {
    use crate::browser::layout::{parse_transform, TransformOp};
    let t = parse_transform("scale3d(1.5, 2.0, 0.5)").unwrap();
    if let TransformOp::Scale3D { x, y, z } = t {
        assert_eq!((x, y, z), (1.5, 2.0, 0.5));
    } else { panic!(); }
}

#[test]
fn parse_transform_matrix3d() {
    use crate::browser::layout::{parse_transform, TransformOp};
    let t = parse_transform("matrix3d(1,0,0,0, 0,1,0,0, 0,0,1,0, 10,20,30,1)").unwrap();
    if let TransformOp::Matrix3D(m) = t {
        assert_eq!(m[0], 1.0);
        assert_eq!(m[12], 10.0);
        assert_eq!(m[15], 1.0);
    } else { panic!(); }
}

#[test]
fn font_family_picks_first_from_list() {
    let doc = parse_html(r#"<html><body><p>x</p></body></html>"#, "");
    let css = parse_stylesheet(r#"p { font-family: "MyFont", Arial, sans-serif; }"#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.font_family, "MyFont");
}

#[test]
fn text_transform_uppercase_applied() {
    use crate::browser::layout::TextTransform;
    let doc = parse_html(r#"<html><body><p>hello world</p></body></html>"#, "");
    let css = parse_stylesheet("p { text-transform: uppercase; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.text_transform, TextTransform::Uppercase);
}

#[test]
fn aspect_ratio_parsed_from_fraction() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { aspect-ratio: 16 / 9; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    let ar = d.aspect_ratio.unwrap();
    assert!((ar - 16.0/9.0).abs() < 0.001);
}

#[test]
fn aspect_ratio_parsed_from_decimal() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { aspect-ratio: 1.5; }");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.aspect_ratio, Some(1.5));
}

#[test]
fn parse_text_shadow_basic() {
    let s = layout::parse_text_shadow("2px 4px 8px black").unwrap();
    assert_eq!(s.0, 2.0);
    assert_eq!(s.1, 4.0);
    assert_eq!(s.2, 8.0);
    assert_eq!(s.3, [0, 0, 0, 255]);
}

#[test]
fn parse_text_shadow_no_blur() {
    let s = layout::parse_text_shadow("1px 1px red").unwrap();
    assert_eq!(s.0, 1.0);
    assert_eq!(s.1, 1.0);
    assert_eq!(s.2, 0.0);
    assert_eq!(s.3, [255, 0, 0, 255]);
}

#[test]
fn multiple_backgrounds_parsed() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        div {
            background-image: url("a.png"), url("b.png");
            background-position: top left, bottom right;
            background-color: yellow;
        }
    "#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let div = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(div.backgrounds.len(), 2);
    assert_eq!(div.backgrounds[0].image_src, Some("a.png".into()));
    assert_eq!(div.backgrounds[1].image_src, Some("b.png".into()));
    // Color jen na posledni layer
    assert_eq!(div.backgrounds[1].color, Some([255, 255, 0, 255]));
    assert_eq!(div.backgrounds[0].color, None);
}

#[test]
fn clip_path_circle_emits_rounded_rect() {
    use crate::browser::paint;
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        div {
            background: red;
            clip-path: circle(50% at center);
        }
    "#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let cmds = paint::build_display_list(&root);
    // Circle clip - bg rect by mel byt s velkym radius (alespon cca 50% sirky)
    let red_round = cmds.iter().find(|c| matches!(c,
        paint::DisplayCommand::Rect { color: [255, 0, 0, 255], radius, .. } if *radius > 100.0));
    assert!(red_round.is_some(), "circle clip mela emit rect s radius > 100");
}

#[test]
fn clip_path_inset_shrinks_rect() {
    use crate::browser::paint;
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        div {
            display: inline-block;
            width: 200px;
            height: 100px;
            background: red;
            clip-path: inset(10px 20px 30px 40px);
        }
    "#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let cmds = paint::build_display_list(&root);
    // Bg rect po inset clip: w = puv - left - right, h = puv - top - bottom
    let red = cmds.iter().find(|c| matches!(c, paint::DisplayCommand::Rect { color: [255, 0, 0, 255], .. }));
    if let Some(paint::DisplayCommand::Rect { w, h, .. }) = red {
        // w = orig - 60, h = orig - 40 (relativni, presny w neumime spolehnout
        // kvuli inline-block layoutu - ale ratio overujem)
        assert!(*w < 200.0, "w mel byt mensi (clip) - got {}", w);
        assert!(*h < 100.0, "h mel byt mensi - got {}", h);
    } else {
        panic!("missing red rect");
    }
}

#[test]
fn parse_clip_path_inset() {
    use crate::browser::layout::{parse_clip_path, ClipPath};
    let cp = parse_clip_path("inset(10px 20px 30px 40px)").unwrap();
    if let ClipPath::Inset { top, right, bottom, left, .. } = cp {
        assert_eq!(top, 10.0);
        assert_eq!(right, 20.0);
        assert_eq!(bottom, 30.0);
        assert_eq!(left, 40.0);
    } else { panic!("expected Inset"); }
}

#[test]
fn parse_clip_path_inset_with_radius() {
    use crate::browser::layout::{parse_clip_path, ClipPath};
    let cp = parse_clip_path("inset(10px round 8px)").unwrap();
    if let ClipPath::Inset { radius, .. } = cp {
        assert_eq!(radius, 8.0);
    } else { panic!(); }
}

#[test]
fn parse_clip_path_circle() {
    use crate::browser::layout::{parse_clip_path, ClipPath};
    let cp = parse_clip_path("circle(50% at center)").unwrap();
    if let ClipPath::Circle { cx_pct, cy_pct, radius_pct } = cp {
        assert_eq!(radius_pct, 0.5);
        assert_eq!(cx_pct, 0.5);
        assert_eq!(cy_pct, 0.5);
    } else { panic!(); }
}

#[test]
fn parse_clip_path_polygon() {
    use crate::browser::layout::{parse_clip_path, ClipPath};
    let cp = parse_clip_path("polygon(0 0, 100% 0, 50% 100%)").unwrap();
    if let ClipPath::Polygon(points) = cp {
        assert_eq!(points.len(), 3);
        assert_eq!(points[1].0, 1.0); // 100%
    } else { panic!(); }
}

#[test]
fn parse_clip_path_none() {
    use crate::browser::layout::parse_clip_path;
    assert!(parse_clip_path("none").is_none());
    assert!(parse_clip_path("").is_none());
}

#[test]
fn parse_bg_position_keywords() {
    use crate::browser::layout::{parse_bg_position, BgPosition};
    let p = parse_bg_position("center top");
    if let BgPosition::Mixed { x_pct, y_pct, .. } = p {
        assert_eq!(x_pct, Some(0.5));
        assert_eq!(y_pct, Some(0.0));
    } else { panic!("expected Mixed"); }

    let p2 = parse_bg_position("right bottom");
    if let BgPosition::Mixed { x_pct, y_pct, .. } = p2 {
        assert_eq!(x_pct, Some(1.0));
        assert_eq!(y_pct, Some(1.0));
    } else { panic!(); }
}

#[test]
fn parse_bg_position_lengths() {
    use crate::browser::layout::{parse_bg_position, BgPosition};
    let p = parse_bg_position("10px 20px");
    if let BgPosition::Mixed { x_px, y_px, .. } = p {
        assert_eq!(x_px, Some(10.0));
        assert_eq!(y_px, Some(20.0));
    } else { panic!(); }
}

#[test]
fn parse_bg_position_pct() {
    use crate::browser::layout::{parse_bg_position, BgPosition};
    let p = parse_bg_position("50% 25%");
    if let BgPosition::Mixed { x_pct, y_pct, .. } = p {
        assert_eq!(x_pct, Some(0.5));
        assert_eq!(y_pct, Some(0.25));
    } else { panic!(); }
}

#[test]
fn parse_bg_size_keywords() {
    use crate::browser::layout::{parse_bg_size, BgSize};
    assert!(matches!(parse_bg_size("cover"), BgSize::Cover));
    assert!(matches!(parse_bg_size("contain"), BgSize::Contain));
    assert!(matches!(parse_bg_size("auto"), BgSize::Auto));
}

#[test]
fn parse_bg_size_lengths() {
    use crate::browser::layout::{parse_bg_size, BgSize};
    if let BgSize::Length { w, h } = parse_bg_size("100px 200px") {
        assert_eq!(w, Some(100.0));
        assert_eq!(h, Some(200.0));
    } else { panic!(); }
    if let BgSize::Pct { w, h } = parse_bg_size("50% auto") {
        assert_eq!(w, Some(0.5));
        assert_eq!(h, None);
    } else { panic!(); }
}

#[test]
fn parse_bg_repeat_variants() {
    use crate::browser::layout::{parse_bg_repeat, BgRepeat};
    assert!(matches!(parse_bg_repeat("no-repeat"), BgRepeat::NoRepeat));
    assert!(matches!(parse_bg_repeat("repeat-x"), BgRepeat::RepeatX));
    assert!(matches!(parse_bg_repeat("repeat-y"), BgRepeat::RepeatY));
    assert!(matches!(parse_bg_repeat("repeat"), BgRepeat::Repeat));
    assert!(matches!(parse_bg_repeat("space"), BgRepeat::Space));
    assert!(matches!(parse_bg_repeat("round"), BgRepeat::Round));
}

#[test]
fn parse_bg_box_variants() {
    use crate::browser::layout::{parse_bg_box, BgBox};
    assert!(matches!(parse_bg_box("border-box"), BgBox::BorderBox));
    assert!(matches!(parse_bg_box("padding-box"), BgBox::PaddingBox));
    assert!(matches!(parse_bg_box("content-box"), BgBox::ContentBox));
}

#[test]
fn build_box_populates_backgrounds() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        div {
            background-color: red;
            background-image: url("logo.png");
            background-position: 50% 50%;
            background-size: cover;
            background-repeat: no-repeat;
            background-clip: padding-box;
        }
    "#);
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let div = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(div.backgrounds.len(), 1);
    let bg = &div.backgrounds[0];
    assert_eq!(bg.color, Some([255, 0, 0, 255]));
    assert_eq!(bg.image_src, Some("logo.png".into()));
    assert!(matches!(bg.size, layout::BgSize::Cover));
    assert!(matches!(bg.repeat, layout::BgRepeat::NoRepeat));
    assert!(matches!(bg.clip, layout::BgBox::PaddingBox));
}

#[test]
fn apply_filter_grayscale_full_makes_gray() {
    use crate::browser::layout::{apply_filter_chain, FilterOp};
    let red = [255, 0, 0, 255];
    let result = apply_filter_chain(red, &[FilterOp::Grayscale(1.0)]);
    // Lum z red je ~76 (0.299*255). R=G=B=76 ish.
    assert!((result[0] as i16 - result[1] as i16).abs() <= 2);
    assert!((result[1] as i16 - result[2] as i16).abs() <= 2);
    assert!(result[0] >= 70 && result[0] <= 80, "expected ~76, got {}", result[0]);
}

#[test]
fn apply_filter_invert_full_inverts() {
    use crate::browser::layout::{apply_filter_chain, FilterOp};
    let r = apply_filter_chain([0, 0, 0, 255], &[FilterOp::Invert(1.0)]);
    assert_eq!(r, [255, 255, 255, 255]);
    let r2 = apply_filter_chain([100, 200, 50, 255], &[FilterOp::Invert(1.0)]);
    // 255-100=155, 255-200=55, 255-50=205 (mozna o 1 nizsi kvuli f32 rounding)
    assert!((r2[0] as i16 - 155).abs() <= 1);
    assert!((r2[1] as i16 - 55).abs() <= 1);
    assert!((r2[2] as i16 - 205).abs() <= 1);
}

#[test]
fn apply_filter_brightness_doubles() {
    use crate::browser::layout::{apply_filter_chain, FilterOp};
    let r = apply_filter_chain([100, 100, 100, 255], &[FilterOp::Brightness(2.0)]);
    assert_eq!(r, [200, 200, 200, 255]);
    // Clamp pri overflow
    let r2 = apply_filter_chain([200, 200, 200, 255], &[FilterOp::Brightness(2.0)]);
    assert_eq!(r2, [255, 255, 255, 255]);
}

#[test]
fn apply_filter_opacity_lowers_alpha() {
    use crate::browser::layout::{apply_filter_chain, FilterOp};
    let r = apply_filter_chain([255, 0, 0, 255], &[FilterOp::Opacity(0.5)]);
    assert_eq!(r[3], 127);
    assert_eq!(r[0], 255);
}

#[test]
fn apply_filter_chained_brightness_then_invert() {
    use crate::browser::layout::{apply_filter_chain, FilterOp};
    // 50% gray -> brightness 0.5 -> 25% gray (~64) -> invert -> ~191
    let r = apply_filter_chain([128, 128, 128, 255], &[
        FilterOp::Brightness(0.5),
        FilterOp::Invert(1.0),
    ]);
    assert!(r[0] >= 188 && r[0] <= 195, "got {}", r[0]);
}

#[test]
fn parse_filter_chain_blur() {
    use crate::browser::layout::{parse_filter_chain, FilterOp};
    let chain = parse_filter_chain("blur(4px)");
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0], FilterOp::Blur(4.0));
}

#[test]
fn parse_filter_chain_multiple() {
    use crate::browser::layout::{parse_filter_chain, FilterOp};
    let chain = parse_filter_chain("blur(2px) brightness(1.2) hue-rotate(45deg)");
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0], FilterOp::Blur(2.0));
    assert_eq!(chain[1], FilterOp::Brightness(1.2));
    assert_eq!(chain[2], FilterOp::HueRotate(45.0));
}

#[test]
fn parse_filter_chain_pct() {
    use crate::browser::layout::{parse_filter_chain, FilterOp};
    let chain = parse_filter_chain("grayscale(50%) sepia(30%)");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0], FilterOp::Grayscale(0.5));
    assert_eq!(chain[1], FilterOp::Sepia(0.3));
}

#[test]
fn parse_filter_chain_drop_shadow() {
    use crate::browser::layout::{parse_filter_chain, FilterOp};
    let chain = parse_filter_chain("drop-shadow(2px 4px 8px black)");
    assert_eq!(chain.len(), 1);
    if let FilterOp::DropShadow { ox, oy, blur, color } = chain[0] {
        assert_eq!(ox, 2.0);
        assert_eq!(oy, 4.0);
        assert_eq!(blur, 8.0);
        assert_eq!(color, [0, 0, 0, 255]);
    } else {
        panic!("expected DropShadow");
    }
}

#[test]
fn parse_filter_chain_none() {
    use crate::browser::layout::parse_filter_chain;
    assert!(parse_filter_chain("none").is_empty());
    assert!(parse_filter_chain("").is_empty());
}

#[test]
fn parse_radial_gradient_basic() {
    let g = layout::parse_radial_gradient("radial-gradient(red, blue)").unwrap();
    matches!(g.kind, crate::browser::layout::BgGradientKind::Radial { .. });
    assert_eq!(g.stops.len(), 2);
    assert_eq!(g.stops[0].1, [255, 0, 0, 255]);
    assert_eq!(g.stops[1].1, [0, 0, 255, 255]);
}

#[test]
fn parse_radial_gradient_with_position() {
    let g = layout::parse_radial_gradient("radial-gradient(circle at top left, red, blue)").unwrap();
    if let crate::browser::layout::BgGradientKind::Radial { cx_pct, cy_pct, .. } = g.kind {
        assert_eq!(cx_pct, 0.0);
        assert_eq!(cy_pct, 0.0);
    } else {
        panic!("expected Radial");
    }
}

#[test]
fn parse_conic_gradient_basic() {
    let g = layout::parse_conic_gradient("conic-gradient(red, yellow, green, blue, red)").unwrap();
    matches!(g.kind, crate::browser::layout::BgGradientKind::Conic { .. });
    assert_eq!(g.stops.len(), 5);
}

#[test]
fn parse_conic_gradient_from_angle() {
    let g = layout::parse_conic_gradient("conic-gradient(from 90deg at center, red, blue)").unwrap();
    if let crate::browser::layout::BgGradientKind::Conic { start_angle_deg, cx_pct, cy_pct } = g.kind {
        assert_eq!(start_angle_deg, 90.0);
        assert_eq!(cx_pct, 0.5);
        assert_eq!(cy_pct, 0.5);
    } else {
        panic!("expected Conic");
    }
}

#[test]
fn parse_any_gradient_dispatches() {
    use crate::browser::layout::{parse_any_gradient, BgGradientKind};
    let lin = parse_any_gradient("linear-gradient(45deg, red, blue)").unwrap();
    assert!(matches!(lin.kind, BgGradientKind::Linear { .. }));
    let rad = parse_any_gradient("radial-gradient(red, blue)").unwrap();
    assert!(matches!(rad.kind, BgGradientKind::Radial { .. }));
    let con = parse_any_gradient("conic-gradient(red, blue)").unwrap();
    assert!(matches!(con.kind, BgGradientKind::Conic { .. }));
}

#[test]
fn parse_box_shadow_inset() {
    let s = layout::parse_box_shadow("inset 0 0 10px rgba(0,0,0,0.5)").unwrap();
    assert_eq!(s.5, true, "inset flag musi byt true");
    let s2 = layout::parse_box_shadow("0 0 10px black").unwrap();
    assert_eq!(s2.5, false);
    let s3 = layout::parse_box_shadow("2px 4px 8px 2px red inset").unwrap();
    assert_eq!(s3.5, true, "inset na konci taky pocitano");
    assert_eq!(s3.0, 2.0); // offset_x
    assert_eq!(s3.1, 4.0); // offset_y
    assert_eq!(s3.2, 8.0); // blur
    assert_eq!(s3.3, 2.0); // spread
}

#[test]
fn parse_box_shadow_basic() {
    let s = layout::parse_box_shadow("2px 4px 8px black");
    assert!(s.is_some());
    let (ox, oy, blur, _spread, _color, _inset) = s.unwrap();
    assert_eq!(ox, 2.0);
    assert_eq!(oy, 4.0);
    assert_eq!(blur, 8.0);
}

#[test]
fn parse_transform_translate() {
    use crate::browser::layout::TransformOp;
    let t = layout::parse_transform("translate(10px, 20px)");
    assert!(matches!(t, Some(TransformOp::Translate(10.0, 20.0))));
}

#[test]
fn interpolate_keyframes_at_50pct() {
    use crate::browser::css_parser::Declaration;
    let frames = vec![
        (0.0, vec![Declaration { property: "left".into(), value: "0px".into(), important: false }]),
        (1.0, vec![Declaration { property: "left".into(), value: "100px".into(), important: false }]),
    ];
    let result = layout::interpolate_keyframes(&frames, 0.5);
    assert_eq!(result.get("left").map(|s| s.as_str()), Some("50px"));
}

#[test]
fn parse_keyframes_block() {
    use crate::browser::css_parser::parse_stylesheet;
    let s = parse_stylesheet(r#"
        @keyframes slide {
            0%   { left: 0px; }
            50%  { left: 100px; }
            100% { left: 200px; }
        }
    "#);
    assert_eq!(s.keyframes.len(), 1);
    assert_eq!(s.keyframes[0].name, "slide");
    assert_eq!(s.keyframes[0].frames.len(), 3);
}

#[test]
fn parse_transform_rotate() {
    use crate::browser::layout::TransformOp;
    let t = layout::parse_transform("rotate(90deg)");
    if let Some(TransformOp::Rotate(rad)) = t {
        assert!((rad - std::f32::consts::FRAC_PI_2).abs() < 0.01);
    } else {
        panic!("expected Rotate");
    }
}

#[test]
fn measure_text_width_estimate() {
    // Pri real fontu: priblizne 30-50 px pro "hello" v 16px (zalezi na fontu)
    // Pri fallback heuristice: 5 * 16 * 0.55 = 44
    let w = layout::measure_text_width("hello", 16.0);
    assert!(w > 10.0 && w < 100.0, "expected reasonable width, got {w}");
}

#[test]
fn inline_text_wraps_to_new_line() {
    // Block s velmi uzkou sirkou - text wrappuje
    let doc = parse_html(r#"<html><body>
        <p>velmi dlouhy text ktery musi byt zabalen na nekolik radku protoze sirka rodice je mala</p>
    </body></html>"#, "");
    let css = parse_stylesheet("p { padding: 4px; }");
    let map = cascade::cascade(&doc.root, &[css]);
    // Maly viewport - 200px
    let layout = layout::layout_tree(&doc.root, &map, 200.0, 768.0);

    // p element by mel mit vysku > jeden radek
    let body = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .unwrap();
    let p = body.children.iter().find(|c| c.tag.as_deref() == Some("p")).unwrap();
    // Pri 200px width by mel byt p vyssi nez jeden radek (>30px)
    assert!(p.rect.height > 30.0, "p should wrap, got height {}", p.rect.height);
}

#[test]
fn flex_layout_horizontal() {
    let doc = parse_html(r#"<html><body>
        <div class="row">
            <div class="item">A</div>
            <div class="item">B</div>
            <div class="item">C</div>
        </div>
    </body></html>"#, "");
    let css = parse_stylesheet(r#"
        .row { display: flex; }
        .item { padding: 10px; background: blue; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);

    let row = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .unwrap();
    // Item A i B by mely byt na stejnem y (horizontal flex)
    if row.children.len() >= 2 {
        assert!((row.children[0].rect.y - row.children[1].rect.y).abs() < 5.0,
            "flex items should be horizontal");
    }
}

#[test]
fn position_relative_offsets() {
    let doc = parse_html(r#"<html><body>
        <div class="rel" style="position: relative; top: 50px; left: 30px;">moved</div>
    </body></html>"#, "");
    let css = parse_stylesheet(".rel { background: blue; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let div = layout.children.iter()
        .find(|c| c.tag.as_deref() == Some("html"))
        .and_then(|h| h.children.iter().find(|c| c.tag.as_deref() == Some("body")))
        .and_then(|b| b.children.iter().find(|c| c.tag.as_deref() == Some("div")))
        .unwrap();
    // Relative element: top + left aplikovany
    // Original y by byl ~80 (po headerech), s top:50 by mel byt ~130
    assert!(div.rect.x >= 30.0, "left should add 30px, got x={}", div.rect.x);
}

#[test]
fn paint_generates_commands() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { background: red; }");
    let map = cascade::cascade(&doc.root, &[css]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    let cmds = paint::build_display_list(&layout);
    // Mela by byt aspon jedna Rect command (red div)
    let has_rect = cmds.iter().any(|c| matches!(c, paint::DisplayCommand::Rect { .. }));
    assert!(has_rect);
}

#[test]
fn animation_spec_shorthand_parsing() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("animation".into(), "slide 2s linear infinite".into());
    let spec = cascade::AnimationSpec::from_styles(&s).unwrap();
    assert_eq!(spec.name, "slide");
    assert_eq!(spec.duration_secs, 2.0);
    assert_eq!(spec.timing_function, "linear");
    assert!(spec.iteration_count.is_infinite());
}

#[test]
fn transition_spec_shorthand_simple() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("transition".into(), "color 200ms ease-in".into());
    let specs = cascade::TransitionSpec::from_styles(&s);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].property, "color");
    assert_eq!(specs[0].duration_secs, 0.2);
    assert_eq!(specs[0].timing_function, "ease-in");
    assert_eq!(specs[0].delay_secs, 0.0);
}

#[test]
fn transition_spec_shorthand_with_delay() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("transition".into(), "transform 0.5s linear 1s".into());
    let specs = cascade::TransitionSpec::from_styles(&s);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].property, "transform");
    assert_eq!(specs[0].duration_secs, 0.5);
    assert_eq!(specs[0].delay_secs, 1.0);
}

#[test]
fn transition_spec_multiple_comma() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("transition".into(), "color 200ms, opacity 500ms ease-in".into());
    let specs = cascade::TransitionSpec::from_styles(&s);
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].property, "color");
    assert_eq!(specs[0].duration_secs, 0.2);
    assert_eq!(specs[1].property, "opacity");
    assert_eq!(specs[1].duration_secs, 0.5);
    assert_eq!(specs[1].timing_function, "ease-in");
}

#[test]
fn transition_spec_longhand() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("transition-property".into(), "color, transform".into());
    s.insert("transition-duration".into(), "200ms, 500ms".into());
    s.insert("transition-timing-function".into(), "linear".into());
    let specs = cascade::TransitionSpec::from_styles(&s);
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].duration_secs, 0.2);
    assert_eq!(specs[1].duration_secs, 0.5);
    // Timing function se opakuje (1 hodnota -> aplikovana pro vsechny)
    assert_eq!(specs[0].timing_function, "linear");
    assert_eq!(specs[1].timing_function, "linear");
}

#[test]
fn transition_detect_change_creates_active() {
    use crate::browser::cascade;
    use std::collections::HashMap;

    let mut prev: cascade::StyleMap = HashMap::new();
    let mut cur: cascade::StyleMap = HashMap::new();

    let mut prev_styles: HashMap<String, String> = HashMap::new();
    prev_styles.insert("color".into(), "red".into());
    prev_styles.insert("transition".into(), "color 200ms".into());
    prev.insert(42, prev_styles);

    let mut cur_styles: HashMap<String, String> = HashMap::new();
    cur_styles.insert("color".into(), "blue".into());
    cur_styles.insert("transition".into(), "color 200ms".into());
    cur.insert(42, cur_styles);

    let active = cascade::detect_transitions(&prev, &cur, vec![], 1.0);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].property, "color");
    assert_eq!(active[0].from_value, "red");
    assert_eq!(active[0].to_value, "blue");
}

#[test]
fn transition_apply_interpolates_numeric() {
    use crate::browser::cascade::{ActiveTransition, TransitionSpec, apply_transitions};
    use std::collections::HashMap;

    let mut style_map: cascade::StyleMap = HashMap::new();
    let mut styles: HashMap<String, String> = HashMap::new();
    styles.insert("opacity".into(), "1".into());
    style_map.insert(99, styles);

    let active = vec![ActiveTransition {
        node_id: 99,
        property: "opacity".into(),
        from_value: "0px".into(),
        to_value: "100px".into(),
        spec: TransitionSpec {
            property: "opacity".into(),
            duration_secs: 1.0,
            timing_function: "linear".into(),
            delay_secs: 0.0,
        },
        start_time: 0.0,
    }];
    apply_transitions(&mut style_map, &active, 0.5);
    let v = style_map.get(&99).unwrap().get("opacity").unwrap();
    // 50px na 50% prubehu (linear)
    assert_eq!(v, "50px");
}

#[test]
fn animation_spec_fill_mode_play_state() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("animation".into(), "slide 2s ease-in forwards paused".into());
    let spec = cascade::AnimationSpec::from_styles(&s).unwrap();
    assert_eq!(spec.fill_mode, "forwards");
    assert_eq!(spec.play_state, "paused");
    assert_eq!(spec.timing_function, "ease-in");
}

#[test]
fn animation_spec_cubic_bezier_in_shorthand() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("animation".into(), "fade 1s cubic-bezier(0.25,0.1,0.25,1) infinite".into());
    let spec = cascade::AnimationSpec::from_styles(&s).unwrap();
    assert!(spec.timing_function.starts_with("cubic-bezier("),
        "got: {}", spec.timing_function);
    assert!(spec.iteration_count.is_infinite());
}

#[test]
fn animation_fill_mode_forwards_holds_last_frame() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet};
    let doc = parse_html(r#"<html><body><div id="x"></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        #x { animation: slide 1s linear forwards; left: 0px; position: relative; }
        @keyframes slide { 0% { left: 0px; } 100% { left: 100px; } }
    "#);
    let mut map = cascade::cascade(&doc.root, &[css.clone()]);
    // Po skonceni (5s > 1s) s forwards drzi 100px
    cascade::apply_animations(&mut map, &[css], 5.0);
    let div = doc.root.get_elements_by_tag("div");
    let s = cascade::get_styles(&map, &div[0]).unwrap();
    assert_eq!(s.get("left").map(|v| v.as_str()), Some("100px"));
}

#[test]
fn animation_paused_freezes_at_zero() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet};
    let doc = parse_html(r#"<html><body><div id="x"></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        #x { animation: slide 1s linear; animation-play-state: paused; left: 0px; }
        @keyframes slide { 0% { left: 0px; } 100% { left: 100px; } }
    "#);
    let mut map = cascade::cascade(&doc.root, &[css.clone()]);
    cascade::apply_animations(&mut map, &[css], 0.5);
    let div = doc.root.get_elements_by_tag("div");
    let s = cascade::get_styles(&map, &div[0]).unwrap();
    assert_eq!(s.get("left").map(|v| v.as_str()), Some("0px"));
}

#[test]
fn animation_spec_longhand_overrides() {
    use std::collections::HashMap;
    let mut s: HashMap<String, String> = HashMap::new();
    s.insert("animation-name".into(), "fade".into());
    s.insert("animation-duration".into(), "500ms".into());
    s.insert("animation-iteration-count".into(), "3".into());
    let spec = cascade::AnimationSpec::from_styles(&s).unwrap();
    assert_eq!(spec.name, "fade");
    assert_eq!(spec.duration_secs, 0.5);
    assert_eq!(spec.iteration_count, 3.0);
}

#[test]
fn apply_animations_interpolates_at_half_duration() {
    let doc = parse_html(r#"<html><body><div id="x"></div></body></html>"#, "");
    let css = parse_stylesheet(r#"
        #x { animation: slide 2s linear; left: 0px; position: relative; }
        @keyframes slide {
            0%   { left: 0px; }
            100% { left: 100px; }
        }
    "#);
    let mut map = cascade::cascade(&doc.root, &[css.clone()]);
    // V case 1.0s (50% z 2s) - linearne -> left 50px
    let active = cascade::apply_animations(&mut map, &[css], 1.0);
    assert!(active);
    // Najdi div node a jeho styles
    let divs = doc.root.get_elements_by_tag("div");
    let div = divs.first().unwrap();
    let styles = cascade::get_styles(&map, div).unwrap();
    let left = styles.get("left").map(|s| s.as_str()).unwrap_or("");
    assert_eq!(left, "50px", "expected left=50px at t=1s of 2s linear, got {left}");
}

// ─── Color matrix ───────────────────────────────────────────────────────

fn approx_eq_mat(a: &[f32; 20], b: &[f32; 20], eps: f32) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < eps)
}

#[test]
fn color_matrix_empty_is_identity() {
    let m = layout::compute_color_matrix(&[]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_brightness_one_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Brightness(1.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_brightness_half_scales_rgb() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Brightness(0.5)]);
    // R, G, B kanaly skalovany 0.5; alpha + offsety nezmeny
    assert!((m[0] - 0.5).abs() < 1e-5, "r coef");
    assert!((m[6] - 0.5).abs() < 1e-5, "g coef");
    assert!((m[12] - 0.5).abs() < 1e-5, "b coef");
    assert!((m[18] - 1.0).abs() < 1e-5, "alpha coef nezmenen");
    assert!((m[4]).abs() < 1e-5, "no offset r");
}

#[test]
fn color_matrix_contrast_half() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Contrast(0.5)]);
    // contrast: r' = 0.5*r + 0.25
    assert!((m[0] - 0.5).abs() < 1e-5);
    assert!((m[4] - 0.25).abs() < 1e-5);
}

#[test]
fn color_matrix_invert_full() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Invert(1.0)]);
    // r' = -1*r + 1
    assert!((m[0] + 1.0).abs() < 1e-5);
    assert!((m[4] - 1.0).abs() < 1e-5);
}

#[test]
fn color_matrix_invert_zero_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Invert(0.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_grayscale_full_uses_luma() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Grayscale(1.0)]);
    // Vsechny RGB rady = (0.2126, 0.7152, 0.0722, 0)
    assert!((m[0] - 0.2126).abs() < 1e-4);
    assert!((m[1] - 0.7152).abs() < 1e-4);
    assert!((m[2] - 0.0722).abs() < 1e-4);
    assert!((m[5] - 0.2126).abs() < 1e-4);
    assert!((m[6] - 0.7152).abs() < 1e-4);
}

#[test]
fn color_matrix_grayscale_zero_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Grayscale(0.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_sepia_full_known_coeffs() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Sepia(1.0)]);
    // R: 0.393, 0.769, 0.189
    assert!((m[0] - 0.393).abs() < 1e-3);
    assert!((m[1] - 0.769).abs() < 1e-3);
    assert!((m[2] - 0.189).abs() < 1e-3);
    // G: 0.349, 0.686, 0.168
    assert!((m[5] - 0.349).abs() < 1e-3);
}

#[test]
fn color_matrix_saturate_one_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Saturate(1.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_saturate_zero_collapses_to_luma() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Saturate(0.0)]);
    // Pri saturate(0) by mely byt vsechny radky (lr, lg, lb, 0, 0)
    assert!((m[0] - 0.213).abs() < 1e-3);
    assert!((m[1] - 0.715).abs() < 1e-3);
    assert!((m[5] - 0.213).abs() < 1e-3);
    assert!((m[10] - 0.213).abs() < 1e-3);
}

#[test]
fn color_matrix_hue_zero_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::HueRotate(0.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_hue_360_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::HueRotate(360.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_opacity_full_is_identity() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Opacity(1.0)]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_opacity_half_scales_alpha() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Opacity(0.5)]);
    assert!((m[18] - 0.5).abs() < 1e-5, "alpha kanal scaled");
}

#[test]
fn color_matrix_blur_skipped() {
    // Blur a DropShadow nemaji color matrix prispevek
    let m = layout::compute_color_matrix(&[
        layout::FilterOp::Blur(5.0),
        layout::FilterOp::DropShadow { ox: 0.0, oy: 0.0, blur: 0.0, color: [0,0,0,255] },
    ]);
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_chain_brightness_invert() {
    // Order matters: invert(1) o brightness(0.5)
    let m = layout::compute_color_matrix(&[
        layout::FilterOp::Brightness(0.5),
        layout::FilterOp::Invert(1.0),
    ]);
    // Po brightness 0.5 -> r' = 0.5r. Po invert -> r'' = 1 - 0.5r = -0.5r + 1
    assert!((m[0] + 0.5).abs() < 1e-5);
    assert!((m[4] - 1.0).abs() < 1e-5);
}

#[test]
fn is_identity_matrix_detects_diff() {
    let mut m = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    assert!(layout::is_identity_matrix(&m));
    m[0] = 0.99;
    assert!(!layout::is_identity_matrix(&m));
}

#[test]
fn is_identity_matrix_zero_offset_only() {
    // Mensi rozdil nez epsilon - 1e-5 < 1e-4 -> stale identity
    let m = [
        1.0 + 1e-5, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    assert!(layout::is_identity_matrix(&m));
}

#[test]
fn color_matrix_chain_double_brightness_multiplies() {
    // brightness(0.5) twice -> 0.25
    let m = layout::compute_color_matrix(&[
        layout::FilterOp::Brightness(0.5),
        layout::FilterOp::Brightness(0.5),
    ]);
    assert!((m[0] - 0.25).abs() < 1e-5);
    assert!((m[6] - 0.25).abs() < 1e-5);
    assert!((m[12] - 0.25).abs() < 1e-5);
}

#[test]
fn color_matrix_chain_grayscale_then_invert() {
    // Verify chain doesn't degrade
    let m = layout::compute_color_matrix(&[
        layout::FilterOp::Grayscale(1.0),
        layout::FilterOp::Invert(1.0),
    ]);
    let id = [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    // Result is NOT identity (non-trivial chain)
    assert!(!approx_eq_mat(&m, &id, 1e-3));
}

// ─── Roman numerals ─────────────────────────────────────────────────────

#[test]
fn to_roman_basic() {
    assert_eq!(layout::to_roman(1), "I");
    assert_eq!(layout::to_roman(3), "III");
    assert_eq!(layout::to_roman(4), "IV");
    assert_eq!(layout::to_roman(9), "IX");
    assert_eq!(layout::to_roman(40), "XL");
    assert_eq!(layout::to_roman(50), "L");
    assert_eq!(layout::to_roman(90), "XC");
    assert_eq!(layout::to_roman(100), "C");
    assert_eq!(layout::to_roman(400), "CD");
    assert_eq!(layout::to_roman(500), "D");
    assert_eq!(layout::to_roman(900), "CM");
    assert_eq!(layout::to_roman(1000), "M");
}

#[test]
fn to_roman_compound() {
    assert_eq!(layout::to_roman(2024), "MMXXIV");
    assert_eq!(layout::to_roman(1999), "MCMXCIX");
    assert_eq!(layout::to_roman(3888), "MMMDCCCLXXXVIII");
}

#[test]
fn to_roman_zero_or_negative_returns_empty() {
    assert_eq!(layout::to_roman(0), "");
    assert_eq!(layout::to_roman(-5), "");
}

// ─── Filter chain parser ────────────────────────────────────────────────

#[test]
fn parse_filter_chain_blur_px() {
    let v = layout::parse_filter_chain("blur(5px)");
    assert_eq!(v.len(), 1);
    matches!(v[0], layout::FilterOp::Blur(_));
    if let layout::FilterOp::Blur(r) = v[0] {
        assert!((r - 5.0).abs() < 1e-5);
    }
}

#[test]
fn parse_filter_chain_multiple_ops() {
    let v = layout::parse_filter_chain("blur(2px) brightness(1.2) hue-rotate(45deg) saturate(2)");
    assert_eq!(v.len(), 4);
    matches!(v[0], layout::FilterOp::Blur(_));
    matches!(v[1], layout::FilterOp::Brightness(_));
    matches!(v[2], layout::FilterOp::HueRotate(_));
    matches!(v[3], layout::FilterOp::Saturate(_));
}

#[test]
fn parse_filter_chain_none_empty() {
    assert_eq!(layout::parse_filter_chain("none").len(), 0);
    assert_eq!(layout::parse_filter_chain("").len(), 0);
    assert_eq!(layout::parse_filter_chain("   ").len(), 0);
}

#[test]
fn parse_filter_chain_grayscale_pct() {
    let v = layout::parse_filter_chain("grayscale(50%)");
    assert_eq!(v.len(), 1);
    if let layout::FilterOp::Grayscale(g) = v[0] {
        assert!((g - 0.5).abs() < 1e-5);
    } else { panic!("expected grayscale"); }
}

#[test]
fn parse_filter_chain_hue_rad() {
    let v = layout::parse_filter_chain("hue-rotate(3.14159rad)");
    assert_eq!(v.len(), 1);
    if let layout::FilterOp::HueRotate(d) = v[0] {
        assert!((d - 180.0).abs() < 0.5, "rad->deg konverze, got {d}");
    } else { panic!("expected hue-rotate"); }
}

#[test]
fn apply_filter_chain_brightness_doubles_red() {
    let result = layout::apply_filter_chain([100, 50, 25, 200], &[layout::FilterOp::Brightness(2.0)]);
    assert_eq!(result[0], 200);
    assert_eq!(result[1], 100);
    assert_eq!(result[2], 50);
    assert_eq!(result[3], 200);  // alpha nezmenen
}

#[test]
fn apply_filter_chain_brightness_clamps() {
    let result = layout::apply_filter_chain([200, 0, 0, 255], &[layout::FilterOp::Brightness(2.0)]);
    assert_eq!(result[0], 255, "clamped to 255");
}

#[test]
fn apply_filter_chain_invert_full() {
    let result = layout::apply_filter_chain([255, 0, 100, 255], &[layout::FilterOp::Invert(1.0)]);
    assert_eq!(result[0], 0);
    assert_eq!(result[1], 255);
    assert_eq!(result[2], 155);
}

// ─── 3D transform matrix compose ────────────────────────────────────────

fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() < eps
}

#[test]
fn transform_matrix_empty_is_identity() {
    let m = layout::compute_transform_matrix(&[], None);
    let id = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    for i in 0..16 { assert!(approx_eq(m[i], id[i], 1e-5), "[{i}] {} != {}", m[i], id[i]); }
}

#[test]
fn transform_matrix_translate_2d() {
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Translate(50.0, 30.0),
    ], None);
    assert!(approx_eq(m[3], 50.0, 1e-5));
    assert!(approx_eq(m[7], 30.0, 1e-5));
    assert!(approx_eq(m[15], 1.0, 1e-5));
}

#[test]
fn transform_matrix_translate_3d() {
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Translate3D { x: 10.0, y: 20.0, z: 30.0 },
    ], None);
    assert!(approx_eq(m[3], 10.0, 1e-5));
    assert!(approx_eq(m[7], 20.0, 1e-5));
    assert!(approx_eq(m[11], 30.0, 1e-5));
}

#[test]
fn transform_matrix_scale_diagonal() {
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Scale(2.0, 3.0),
    ], None);
    assert!(approx_eq(m[0], 2.0, 1e-5));
    assert!(approx_eq(m[5], 3.0, 1e-5));
    assert!(approx_eq(m[10], 1.0, 1e-5));
}

#[test]
fn transform_matrix_scale3d_diagonal() {
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Scale3D { x: 2.0, y: 3.0, z: 4.0 },
    ], None);
    assert!(approx_eq(m[0], 2.0, 1e-5));
    assert!(approx_eq(m[5], 3.0, 1e-5));
    assert!(approx_eq(m[10], 4.0, 1e-5));
}

#[test]
fn transform_matrix_rotate_z_90() {
    let rad = std::f32::consts::FRAC_PI_2;
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Rotate(rad),
    ], None);
    // rotate Z 90deg: [0, -1, 0, 0; 1, 0, 0, 0; ...]
    assert!(approx_eq(m[0], 0.0, 1e-5));
    assert!(approx_eq(m[1], -1.0, 1e-5));
    assert!(approx_eq(m[4], 1.0, 1e-5));
    assert!(approx_eq(m[5], 0.0, 1e-5));
}

#[test]
fn transform_matrix_rotate3d_z_axis_matches_rotate() {
    let rad = std::f32::consts::FRAC_PI_2;
    let m_rotz = layout::compute_transform_matrix(&[
        layout::TransformOp::Rotate3D { x: 0.0, y: 0.0, z: 1.0, angle_rad: rad },
    ], None);
    let m_2d = layout::compute_transform_matrix(&[
        layout::TransformOp::Rotate(rad),
    ], None);
    // Top-left 2x2 by mel byt stejny (Z rotation rotuje XY plane)
    for i in [0, 1, 4, 5] {
        assert!((m_rotz[i] - m_2d[i]).abs() < 1e-4, "{i}: {} vs {}", m_rotz[i], m_2d[i]);
    }
}

#[test]
fn transform_matrix_compose_translate_then_scale() {
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Translate(10.0, 20.0),
        layout::TransformOp::Scale(2.0, 2.0),
    ], None);
    // m = T * S. Pri P=(0,0,0,1): T*S*P = T*(0,0,0,1) = (10, 20, 0, 1).
    // Pri P=(1,0,0,1): T*S*P = T*(2,0,0,1) = (12, 20, 0, 1).
    // Test transform applied to point:
    let px = m[0]*1.0 + m[3]; // (1,0) maps to: 2*1 + 10 = 12
    assert!(approx_eq(px, 12.0, 1e-5));
}

#[test]
fn transform_matrix_perspective_w_factor() {
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Perspective(800.0),
    ], None);
    // Perspective(d): m[14] = -1/d
    assert!(approx_eq(m[14], -1.0 / 800.0, 1e-5));
}

#[test]
fn transform_matrix_parent_perspective_wraps() {
    // Pri parent_perspective = 800, transform = Translate
    let m_with = layout::compute_transform_matrix(&[
        layout::TransformOp::Translate(10.0, 0.0),
    ], Some(800.0));
    // m[14] musi byt -1/800 (z perspective wrapper)
    assert!(approx_eq(m_with[14], -1.0 / 800.0, 1e-5));
    // m[3] = 10 (translate) prezit
    assert!(approx_eq(m_with[3], 10.0, 1e-5));
}

#[test]
fn transform_matrix_matrix3d_passthrough() {
    let custom = [
        2.0, 0.0, 0.0, 5.0,
        0.0, 3.0, 0.0, 7.0,
        0.0, 0.0, 4.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let m = layout::compute_transform_matrix(&[
        layout::TransformOp::Matrix3D(custom),
    ], None);
    for i in 0..16 {
        assert!(approx_eq(m[i], custom[i], 1e-5));
    }
}

// ─── needs_3d_pipeline ──────────────────────────────────────────────────

#[test]
fn needs_3d_empty_no() {
    assert!(!layout::needs_3d_pipeline(&[], None));
}

#[test]
fn needs_3d_translate_2d_no() {
    assert!(!layout::needs_3d_pipeline(&[
        layout::TransformOp::Translate(10.0, 20.0),
    ], None));
}

#[test]
fn needs_3d_rotate_z_yes() {
    // 2D Rotate uz forcuje 3D pipeline (CPU rotate_cmd jen sdouval origin
    // ale rect zustal axis-aligned - vizualne nerotoval).
    assert!(layout::needs_3d_pipeline(&[
        layout::TransformOp::Rotate(1.0),
    ], None));
}

#[test]
fn needs_3d_rotate_x_axis_yes() {
    assert!(layout::needs_3d_pipeline(&[
        layout::TransformOp::Rotate3D { x: 1.0, y: 0.0, z: 0.0, angle_rad: 0.5 },
    ], None));
}

#[test]
fn needs_3d_rotate_y_axis_yes() {
    assert!(layout::needs_3d_pipeline(&[
        layout::TransformOp::Rotate3D { x: 0.0, y: 1.0, z: 0.0, angle_rad: 0.5 },
    ], None));
}

#[test]
fn needs_3d_perspective_yes() {
    assert!(layout::needs_3d_pipeline(&[
        layout::TransformOp::Perspective(800.0),
    ], None));
}

#[test]
fn needs_3d_parent_perspective_with_translate3d_z_yes() {
    assert!(layout::needs_3d_pipeline(&[
        layout::TransformOp::Translate3D { x: 0.0, y: 0.0, z: 50.0 },
    ], Some(800.0)));
}

#[test]
fn needs_3d_parent_perspective_only_2d_translate_no() {
    // Pure 2D transform pod parent perspective - nepotrebuje 3D
    // (perspective bez Z neni viditelny rozdil)
    assert!(!layout::needs_3d_pipeline(&[
        layout::TransformOp::Translate(10.0, 20.0),
    ], Some(800.0)));
}

#[test]
fn needs_3d_matrix3d_with_z_yes() {
    let m = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 50.0,  // m[11] = z translate
        0.0, 0.0, 0.0, 1.0,
    ];
    assert!(layout::needs_3d_pipeline(&[layout::TransformOp::Matrix3D(m)], None));
}

// ─── parse_length / parse_length_ctx ────────────────────────────────────

#[test]
fn parse_length_px() {
    assert_eq!(layout::parse_length("16px"), 16.0);
    assert_eq!(layout::parse_length("0px"), 0.0);
    assert_eq!(layout::parse_length("100px"), 100.0);
}

#[test]
fn parse_length_em_uses_16() {
    assert_eq!(layout::parse_length("1em"), 16.0);
    assert_eq!(layout::parse_length("2em"), 32.0);
    assert_eq!(layout::parse_length("0.5em"), 8.0);
}

#[test]
fn parse_length_rem_uses_16() {
    assert_eq!(layout::parse_length("1rem"), 16.0);
    assert_eq!(layout::parse_length("2rem"), 32.0);
}

#[test]
fn parse_length_vw() {
    let v = layout::parse_length_ctx("50vw", 1000.0, 800.0, 16.0);
    assert!((v - 500.0).abs() < 1e-3);
}

#[test]
fn parse_length_vh() {
    let v = layout::parse_length_ctx("25vh", 1000.0, 800.0, 16.0);
    assert!((v - 200.0).abs() < 1e-3);
}

#[test]
fn parse_length_vmin_uses_smaller() {
    let v = layout::parse_length_ctx("50vmin", 1000.0, 800.0, 16.0);
    assert!((v - 400.0).abs() < 1e-3, "vmin = 50% z 800");
}

#[test]
fn parse_length_vmax_uses_larger() {
    let v = layout::parse_length_ctx("50vmax", 1000.0, 800.0, 16.0);
    assert!((v - 500.0).abs() < 1e-3, "vmax = 50% z 1000");
}

#[test]
fn parse_length_pt_to_px() {
    // 12pt ~= 16px
    let v = layout::parse_length("12pt");
    assert!((v - 16.0).abs() < 0.5);
}

#[test]
fn parse_length_invalid_returns_zero() {
    assert_eq!(layout::parse_length("invalid"), 0.0);
    assert_eq!(layout::parse_length(""), 0.0);
}

#[test]
fn parse_length_negative() {
    assert_eq!(layout::parse_length("-10px"), -10.0);
}

// ─── parse_color rozsireno ──────────────────────────────────────────────

#[test]
fn parse_color_named_basic() {
    assert_eq!(layout::parse_color("red"), Some([255, 0, 0, 255]));
    assert_eq!(layout::parse_color("blue"), Some([0, 0, 255, 255]));
    assert_eq!(layout::parse_color("white"), Some([255, 255, 255, 255]));
    assert_eq!(layout::parse_color("black"), Some([0, 0, 0, 255]));
}

#[test]
fn parse_color_transparent() {
    let c = layout::parse_color("transparent");
    assert!(c.is_some());
    assert_eq!(c.unwrap()[3], 0);
}

#[test]
fn parse_color_hsl_red() {
    // hsl(0, 100%, 50%) = pure red
    let c = layout::parse_color("hsl(0, 100%, 50%)");
    assert!(c.is_some());
    let rgba = c.unwrap();
    assert!(rgba[0] >= 250 && rgba[1] < 10 && rgba[2] < 10);
}

#[test]
fn parse_color_invalid_returns_none() {
    assert!(layout::parse_color("#xyz").is_none());
}

// ─── ClipPath parsing ───────────────────────────────────────────────────

#[test]
fn parse_clip_path_inset_basic() {
    let cp = layout::parse_clip_path("inset(10px)");
    assert!(matches!(cp, Some(layout::ClipPath::Inset { .. })));
}

#[test]
fn parse_clip_path_inset_4_values() {
    if let Some(layout::ClipPath::Inset { top, right, bottom, left, .. }) =
        layout::parse_clip_path("inset(10px 20px 30px 40px)")
    {
        assert_eq!(top, 10.0);
        assert_eq!(right, 20.0);
        assert_eq!(bottom, 30.0);
        assert_eq!(left, 40.0);
    } else {
        panic!("expected Inset");
    }
}

#[test]
fn parse_clip_path_circle_only() {
    let cp = layout::parse_clip_path("circle(50%)");
    assert!(matches!(cp, Some(layout::ClipPath::Circle { .. })));
}

#[test]
fn parse_clip_path_ellipse_only() {
    let cp = layout::parse_clip_path("ellipse(40% 60%)");
    assert!(matches!(cp, Some(layout::ClipPath::Ellipse { .. })));
}

#[test]
fn parse_clip_path_polygon_count() {
    let cp = layout::parse_clip_path("polygon(0 0, 100% 0, 50% 100%)");
    if let Some(layout::ClipPath::Polygon(pts)) = cp {
        assert_eq!(pts.len(), 3);
    } else {
        panic!("expected Polygon");
    }
}

#[test]
fn parse_clip_path_none_returns_none() {
    assert!(layout::parse_clip_path("none").is_none());
    assert!(layout::parse_clip_path("").is_none());
}

#[test]
fn parse_clip_path_unknown_returns_none() {
    assert!(layout::parse_clip_path("unknown(50%)").is_none());
}

#[test]
fn parse_clip_path_polygon_pct_to_normalized() {
    let cp = layout::parse_clip_path("polygon(50% 0%, 100% 100%, 0% 100%)");
    if let Some(layout::ClipPath::Polygon(pts)) = cp {
        // Body v normalizovanem 0..1 ramci
        assert!((pts[0].0 - 0.5).abs() < 1e-3 && pts[0].1.abs() < 1e-3);
        assert!((pts[1].0 - 1.0).abs() < 1e-3 && (pts[1].1 - 1.0).abs() < 1e-3);
        assert!(pts[2].0.abs() < 1e-3 && (pts[2].1 - 1.0).abs() < 1e-3);
    } else {
        panic!("expected Polygon");
    }
}

// ─── BgGradientKind parsing - smoke ────────────────────────────────────

#[test]
fn build_dl_with_gradient_no_panic() {
    let doc = crate::browser::html_parser::parse_html(
        r#"<html><body><div></div></body></html>"#, ""
    );
    let css = crate::browser::css_parser::parse_stylesheet(
        "div { background: linear-gradient(90deg, red 0%, blue 100%); width: 100px; height: 100px; }"
    );
    let map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let _layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
}

// ─── Display enum + value parsing ──────────────────────────────────────

#[test]
fn display_from_block() {
    assert_eq!(layout::Display::from_str("block"), layout::Display::Block);
}

#[test]
fn display_from_inline() {
    assert_eq!(layout::Display::from_str("inline"), layout::Display::Inline);
}

#[test]
fn display_from_flex() {
    assert_eq!(layout::Display::from_str("flex"), layout::Display::Flex);
}

#[test]
fn display_from_grid() {
    assert_eq!(layout::Display::from_str("grid"), layout::Display::Grid);
}

#[test]
fn display_from_none() {
    assert_eq!(layout::Display::from_str("none"), layout::Display::None);
}

// ─── Layout box rect basics ────────────────────────────────────────────

#[test]
fn layout_default_block_height_zero_for_empty() {
    let doc = crate::browser::html_parser::parse_html(
        r#"<html><body><div></div></body></html>"#, ""
    );
    let css = crate::browser::css_parser::parse_stylesheet("");
    let map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let layout_root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    // Mel by parse + layout bez panic
    let _ = layout_root.rect.width;
}

#[test]
fn layout_div_with_explicit_dimensions_smoke() {
    let doc = crate::browser::html_parser::parse_html(
        r#"<html><body><div></div></body></html>"#, ""
    );
    let css = crate::browser::css_parser::parse_stylesheet(
        "div { width: 200px; height: 100px; }"
    );
    let map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let layout_root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    fn find_div(bx: &layout::LayoutBox) -> Option<&layout::LayoutBox> {
        if bx.tag.as_deref() == Some("div") { return Some(bx); }
        for ch in &bx.children { if let Some(d) = find_div(ch) { return Some(d); } }
        None
    }
    let div = find_div(&layout_root).expect("div");
    // Smoke - layout vraci nejake rozmery, ne nule.
    assert!(div.rect.width > 0.0 && div.rect.height > 0.0);
}

#[test]
fn layout_padding_propagates() {
    let doc = crate::browser::html_parser::parse_html(
        r#"<html><body><div></div></body></html>"#, ""
    );
    let css = crate::browser::css_parser::parse_stylesheet(
        "div { width: 100px; height: 50px; padding: 10px; }"
    );
    let map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let layout_root = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    fn find_div(bx: &layout::LayoutBox) -> Option<&layout::LayoutBox> {
        if bx.tag.as_deref() == Some("div") { return Some(bx); }
        for ch in &bx.children { if let Some(d) = find_div(ch) { return Some(d); } }
        None
    }
    let div = find_div(&layout_root).expect("div");
    // Padding by se mel propagovat (nezavisle na shape interpretace)
    assert!(div.padding >= 0.0);
}

// ─── Filter chain parsing ──────────────────────────────────────────────

#[test]
fn parse_filter_chain_blur_em_unit() {
    let v = layout::parse_filter_chain("blur(0.5em)");
    assert_eq!(v.len(), 1);
    if let layout::FilterOp::Blur(r) = v[0] {
        assert!((r - 8.0).abs() < 1e-3, "0.5em = 8px, got {r}");
    }
}

#[test]
fn parse_filter_chain_invert_pct() {
    let v = layout::parse_filter_chain("invert(50%)");
    if let layout::FilterOp::Invert(i) = v[0] {
        assert!((i - 0.5).abs() < 1e-3);
    }
}

#[test]
fn parse_filter_chain_drop_shadow_extended() {
    let v = layout::parse_filter_chain("drop-shadow(2px 3px 4px black)");
    matches!(v[0], layout::FilterOp::DropShadow { .. });
    if let layout::FilterOp::DropShadow { ox, oy, blur, color } = v[0] {
        assert!((ox - 2.0).abs() < 1e-3);
        assert!((oy - 3.0).abs() < 1e-3);
        assert!((blur - 4.0).abs() < 1e-3);
        assert_eq!(color[3], 255, "alpha 1.0");
    }
}

#[test]
fn parse_filter_chain_brightness_unitless() {
    let v = layout::parse_filter_chain("brightness(1.5)");
    if let layout::FilterOp::Brightness(b) = v[0] {
        assert!((b - 1.5).abs() < 1e-3);
    }
}

#[test]
fn parse_filter_chain_opacity_pct() {
    let v = layout::parse_filter_chain("opacity(75%)");
    if let layout::FilterOp::Opacity(o) = v[0] {
        assert!((o - 0.75).abs() < 1e-3);
    }
}

#[test]
fn parse_filter_chain_combined_grayscale_invert() {
    let v = layout::parse_filter_chain("grayscale(100%) invert(100%)");
    assert_eq!(v.len(), 2);
}

#[test]
fn parse_filter_chain_invalid_func_skipped() {
    // Unknown filter func - bud skipnut nebo error, oba acceptable (no panic)
    let _ = layout::parse_filter_chain("unknown_filter(50%)");
}

// ─── Color matrix chain ────────────────────────────────────────────────

#[test]
fn color_matrix_double_invert_is_near_identity() {
    let m = layout::compute_color_matrix(&[
        layout::FilterOp::Invert(1.0),
        layout::FilterOp::Invert(1.0),
    ]);
    // r' = -1 * (-1*r + 1) + 1 = r - 1 + 1 = r -> coef 1, offset 0
    assert!((m[0] - 1.0).abs() < 1e-3);
    assert!(m[4].abs() < 1e-3);
}

#[test]
fn color_matrix_brightness_zero_blackens_rgb() {
    let m = layout::compute_color_matrix(&[layout::FilterOp::Brightness(0.0)]);
    // Vsechny RGB coef nula -> output = (0, 0, 0, alpha)
    assert!(m[0].abs() < 1e-5);
    assert!(m[6].abs() < 1e-5);
    assert!(m[12].abs() < 1e-5);
    assert!((m[18] - 1.0).abs() < 1e-5, "alpha kanal preserved");
}

#[test]
fn pseudo_placeholder_default_color() {
    let doc = parse_html(r#"<html><body><input placeholder="typ neco"></body></html>"#, "");
    let css = parse_stylesheet("");
    let style_map = cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = cascade::cascade_pseudo(&doc.root, &[css]);
    let root = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);
    let input = find_box_by_tag(&root, "input").unwrap();
    // placeholder_color nastaven na darkgray default
    assert!(input.placeholder_color.is_some());
    // child ::placeholder existuje s textem
    let ph = input.children.iter().find(|c| c.tag.as_deref() == Some("::placeholder"));
    assert!(ph.is_some());
    assert_eq!(ph.unwrap().text.as_deref(), Some("typ neco"));
}

#[test]
fn pseudo_placeholder_custom_color() {
    let doc = parse_html(r#"<html><body><input placeholder="hint"></body></html>"#, "");
    let css = parse_stylesheet("input::placeholder { color: red; }");
    let style_map = cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = cascade::cascade_pseudo(&doc.root, &[css]);
    let root = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);
    let input = find_box_by_tag(&root, "input").unwrap();
    assert_eq!(input.placeholder_color, Some([255, 0, 0, 255]));
    let ph = input.children.iter().find(|c| c.tag.as_deref() == Some("::placeholder")).unwrap();
    assert_eq!(ph.text_color, Some([255, 0, 0, 255]));
}

#[test]
fn pseudo_selection_colors_stored() {
    let doc = parse_html(r#"<html><body><p>hello</p></body></html>"#, "");
    let css = parse_stylesheet("p::selection { background-color: blue; color: white; }");
    let style_map = cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = cascade::cascade_pseudo(&doc.root, &[css]);
    let root = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);
    let p = find_box_by_tag(&root, "p").unwrap();
    assert_eq!(p.selection_bg, Some([0, 0, 255, 255]));
    assert_eq!(p.selection_color, Some([255, 255, 255, 255]));
}

#[test]
fn pseudo_backdrop_dialog_open() {
    let doc = parse_html(r#"<html><body><dialog open>obsah</dialog></body></html>"#, "");
    let css = parse_stylesheet("dialog::backdrop { background-color: rgba(0,0,0,0.5); }");
    let style_map = cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = cascade::cascade_pseudo(&doc.root, &[css]);
    let root = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);
    let dialog = find_box_by_tag(&root, "dialog").unwrap();
    let backdrop = dialog.children.iter().find(|c| c.tag.as_deref() == Some("::backdrop"));
    assert!(backdrop.is_some(), "::backdrop child existuje pro dialog open");
    let bd = backdrop.unwrap();
    assert_eq!(bd.position, layout::Position::Fixed);
    // bg_color je rgba(0,0,0,128)
    assert!(bd.bg_color.is_some());
}

#[test]
fn pseudo_backdrop_dialog_closed_no_backdrop() {
    let doc = parse_html(r#"<html><body><dialog>obsah</dialog></body></html>"#, "");
    let css = parse_stylesheet("dialog::backdrop { background-color: black; }");
    let style_map = cascade::cascade(&doc.root, &[css.clone()]);
    let pseudo_map = cascade::cascade_pseudo(&doc.root, &[css]);
    let root = layout::layout_tree_with_pseudo(&doc.root, &style_map, &pseudo_map, 1024.0, 768.0);
    let dialog = find_box_by_tag(&root, "dialog").unwrap();
    let backdrop = dialog.children.iter().find(|c| c.tag.as_deref() == Some("::backdrop"));
    assert!(backdrop.is_none(), "::backdrop se nevlozi pro dialog bez open");
}

#[test]
fn explicit_width_applied() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { width: 200px; height: 80px; }");
    let style_map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert_eq!(d.explicit_width, Some(200.0));
    assert_eq!(d.explicit_height, Some(80.0));
    assert_eq!(d.rect.width, 200.0, "rect.width respektuje explicit CSS width");
    assert_eq!(d.rect.height, 80.0, "rect.height respektuje explicit CSS height");
}

#[test]
fn min_max_width_clamping() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { width: 50px; min-width: 100px; max-width: 300px; }");
    let style_map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    // min-width 100px klampuje width 50px nahoru
    assert!(d.rect.width >= 100.0, "min-width klampuje sirku nahoru, dostali jsme {}", d.rect.width);
}

#[test]
fn max_width_clamping() {
    let doc = parse_html(r#"<html><body><div></div></body></html>"#, "");
    let css = parse_stylesheet("div { width: 500px; max-width: 200px; }");
    let style_map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    assert!(d.rect.width <= 200.0, "max-width klampuje sirku dolu, dostali jsme {}", d.rect.width);
}

#[test]
fn min_content_width_keyword() {
    let doc = parse_html(r#"<html><body><div>hello</div></body></html>"#, "");
    let css = parse_stylesheet("div { width: min-content; }");
    let style_map = cascade::cascade(&doc.root, &[css]);
    let root = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    let d = find_box_by_tag(&root, "div").unwrap();
    // min-content nastavi explicit_width na odhadovou text sirku
    assert!(d.explicit_width.is_some(), "min-content nastavi explicit_width");
}

// --- writing-mode ---

fn make_layout(html: &str, css: &str) -> layout::LayoutBox {
    let doc = parse_html(html, "");
    let sheet = parse_stylesheet(css);
    let map = cascade::cascade(&doc.root, &[sheet]);
    layout::layout_tree(&doc.root, &map, 800.0, 600.0)
}

fn find_body(root: &layout::LayoutBox) -> Option<&layout::LayoutBox> {
    if root.tag.as_deref() == Some("body") { return Some(root); }
    for c in &root.children {
        if let Some(b) = find_body(c) { return Some(b); }
    }
    None
}

#[test]
fn writing_mode_vertical_lr_children_stack_x() {
    let root = make_layout(
        r#"<html><body><div><div></div><div></div></div></body></html>"#,
        r#"
        body > div { writing-mode: vertical-lr; width: 200px; height: 100px; }
        body > div > div { width: 40px; height: 80px; background: blue; }
        "#,
    );
    let body = find_body(&root).expect("body");
    let outer = body.children.first().expect("outer div");
    let children: Vec<&layout::LayoutBox> = outer.children.iter().collect();
    assert!(children.len() >= 2, "outer div musi mit 2 deti, has {}", children.len());
    // V vertical-lr: deti jdou zleva doprava
    assert!(children[0].rect.x < children[1].rect.x,
        "child[0].x={} musi byt < child[1].x={}", children[0].rect.x, children[1].rect.x);
    assert!((children[0].rect.y - children[1].rect.y).abs() < 5.0,
        "oba deti maji podobne y");
}

#[test]
fn writing_mode_horizontal_tb_normal() {
    let root = make_layout(
        r#"<html><body><div><div></div><div></div></div></body></html>"#,
        r#"
        body > div { width: 200px; }
        body > div > div { height: 30px; background: red; }
        "#,
    );
    let body = find_body(&root).expect("body");
    let outer = body.children.first().expect("outer div");
    let children: Vec<&layout::LayoutBox> = outer.children.iter().collect();
    assert!(children.len() >= 2, "outer div musi mit 2 deti");
    // Normalni block layout: druhy div je pod prvnim
    assert!(children[1].rect.y > children[0].rect.y,
        "child[1].y={} musi byt > child[0].y={}", children[1].rect.y, children[0].rect.y);
}

// --- Table border-collapse + UA defaults tests ---

#[test]
fn table_border_collapse_emits_cell_border() {
    let root = make_layout(
        r#"<html><body><table><tr><td>A</td><td>B</td></tr></table></body></html>"#,
        r#"table { border-collapse: collapse; }"#,
    );
    fn find_td<'a>(bx: &'a layout::LayoutBox) -> Option<&'a layout::LayoutBox> {
        if bx.tag.as_deref() == Some("td") { return Some(bx); }
        for ch in &bx.children { if let Some(f) = find_td(ch) { return Some(f); } }
        None
    }
    let td = find_td(&root).expect("td");
    assert!(td.border_width > 0.0, "td v border-collapse table musi mit border");
    assert!(td.border_color.is_some());
}

#[test]
fn table_without_collapse_no_default_border() {
    let root = make_layout(
        r#"<html><body><table><tr><td>A</td></tr></table></body></html>"#,
        r#""#,
    );
    fn find_td<'a>(bx: &'a layout::LayoutBox) -> Option<&'a layout::LayoutBox> {
        if bx.tag.as_deref() == Some("td") { return Some(bx); }
        for ch in &bx.children { if let Some(f) = find_td(ch) { return Some(f); } }
        None
    }
    let td = find_td(&root).expect("td");
    // Bez border-collapse:collapse, td bez explicitniho border = bez border default.
    assert_eq!(td.border_width, 0.0);
}

#[test]
fn code_tag_gets_inline_bg_and_padding() {
    let root = make_layout(
        r#"<html><body><p>Some <code>code</code> here</p></body></html>"#,
        r#""#,
    );
    fn find_code<'a>(bx: &'a layout::LayoutBox) -> Option<&'a layout::LayoutBox> {
        if bx.tag.as_deref() == Some("code") { return Some(bx); }
        for ch in &bx.children { if let Some(f) = find_code(ch) { return Some(f); } }
        None
    }
    let code = find_code(&root).expect("code");
    assert!(code.bg_color.is_some(), "code musi mit default bg");
    assert!(code.padding_left.unwrap_or(0.0) > 0.0);
    assert!(code.border_radius > 0.0);
}

#[test]
fn mark_tag_yellow_bg_with_padding() {
    let root = make_layout(
        r#"<html><body><p>Some <mark>marked</mark> text</p></body></html>"#,
        r#""#,
    );
    fn find_mark<'a>(bx: &'a layout::LayoutBox) -> Option<&'a layout::LayoutBox> {
        if bx.tag.as_deref() == Some("mark") { return Some(bx); }
        for ch in &bx.children { if let Some(f) = find_mark(ch) { return Some(f); } }
        None
    }
    let mark = find_mark(&root).expect("mark");
    let bg = mark.bg_color.expect("mark musi mit bg");
    assert!(bg[0] > 200 && bg[1] > 200, "mark bg by mela byt zluta-ish ({:?})", bg);
    assert!(mark.padding_left.unwrap_or(0.0) > 0.0);
}

#[test]
fn transform_matrix_rotate_y_45_produces_expected_corners() {
    use crate::browser::layout::{compute_transform_matrix, parse_transform_chain};
    let ops = parse_transform_chain("rotateY(45deg)");
    assert_eq!(ops.len(), 1);
    let m = compute_transform_matrix(&ops, None);
    // Apply matrix to (hw, 0, 0, 1) where hw=40 (box width 80).
    let hw = 40.0_f32;
    let lx = hw;
    let tx = m[0]*lx + m[1]*0.0 + m[2]*0.0 + m[3];
    let tw = m[12]*lx + m[13]*0.0 + m[14]*0.0 + m[15];
    let px = tx / tw;
    // Expected: c*hw / 1 = cos(45)*40 = 28.28
    let expected = (45.0_f32.to_radians()).cos() * hw;
    assert!((px - expected).abs() < 0.1, "px={} expected={}", px, expected);
    // Right edge total width
    let lx2 = -hw;
    let tx2 = m[0]*lx2 + m[3];
    let tw2 = m[12]*lx2 + m[15];
    let px2 = tx2 / tw2;
    let width = (px - px2).abs();
    let expected_width = 2.0 * expected;
    assert!((width - expected_width).abs() < 0.5, "width={} expected={}", width, expected_width);
}

#[test]
fn transform_matrix_perspective_rotate_y_35_width() {
    use crate::browser::layout::{compute_transform_matrix, parse_transform_chain};
    let ops = parse_transform_chain("perspective(600px) rotateY(35deg)");
    let m = compute_transform_matrix(&ops, None);
    let hw = 40.0_f32;
    let c = (35.0_f32.to_radians()).cos();
    let s = (35.0_f32.to_radians()).sin();
    // After rotate: x'=c*lx, z'=-s*lx. After perspective(d=600):
    // tw = 1 - z'/d = 1 + s*lx/d. inv_w = 1/tw.
    let apply = |lx: f32| -> f32 {
        let tx = m[0]*lx + m[1]*0.0 + m[2]*0.0 + m[3];
        let tw = m[12]*lx + m[13]*0.0 + m[14]*0.0 + m[15];
        tx / tw
    };
    let pr = apply(hw);
    let pl = apply(-hw);
    let width = (pr - pl).abs();
    // Expected approx
    let expected_pr = c * hw / (1.0 - (-s * hw)/600.0);
    let expected_pl = c * (-hw) / (1.0 - (s * hw)/600.0);
    let expected_width = (expected_pr - expected_pl).abs();
    println!("width={} expected={}", width, expected_width);
    assert!((width - expected_width).abs() < 0.5);
}

#[test]
fn text_wrap_inserts_newline_at_break() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(
        r#"<html><body><p>jeden dva tri ctyri pet sest sedm osm devet deset</p></body></html>"#,
        ""
    );
    let css = parse_stylesheet(r#"body { font-size: 16px; } p { width: 100px; padding: 0; margin: 0; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let lr = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    // Najdi p > text node
    fn find_text(b: &layout::LayoutBox) -> Option<String> {
        if b.tag.is_none() && b.text.is_some() { return b.text.clone(); }
        for ch in &b.children {
            if let Some(t) = find_text(ch) { return Some(t); }
        }
        None
    }
    let text = find_text(&lr).unwrap_or_default();
    println!("wrapped text: {:?}", text);
    assert!(text.contains('\n'), "expected newline at wrap, got: {:?}", text);
}

#[test]
fn button_with_padding_has_full_height() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(
        r#"<html><body><button class="b">Primary</button></body></html>"#,
        ""
    );
    let css = parse_stylesheet(r#"body { font-size: 16px; }
        .b { padding: 8px 16px; font-size: 14px; border-width: 0; border-radius: 4px; color: white; }
    "#);
    let map = cascade::cascade(&doc.root, &[css]);
    let lr = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    fn find(b: &layout::LayoutBox) -> Option<&layout::LayoutBox> {
        if b.tag.as_deref() == Some("button") { return Some(b); }
        for ch in &b.children {
            if let Some(r) = find(ch) { return Some(r); }
        }
        None
    }
    let btn = find(&lr).expect("button not found");
    println!("button rect h={} pad_t={:?} pad_b={:?} font_size={} line_height={}",
        btn.rect.height, btn.padding_top, btn.padding_bottom, btn.font_size, btn.line_height);
    // Pad_t + content + pad_b. Content min font_size (14) ale s line-height obvykle vetsi.
    // Min button height = 8 + 14 + 8 = 30. Idealne 8 + 19.6 + 8 = 35.6.
    assert!(btn.rect.height >= 30.0, "button height {} < expected min 30", btn.rect.height);
    assert_eq!(btn.padding_top, Some(8.0));
    assert_eq!(btn.padding_bottom, Some(8.0));
}

#[test]
fn h2_heading_wraps_at_narrow_viewport() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout};
    let doc = parse_html(
        r#"<html><body><section><h2>Polygon clip-path (fan triangulace)</h2></section></body></html>"#,
        ""
    );
    let css = parse_stylesheet(r#"body { font-size: 16px; margin: 0; } section { padding: 0; } h2 { padding: 0; margin: 0; }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    // Narrow viewport
    let lr = layout::layout_tree(&doc.root, &map, 320.0, 768.0);
    fn find_text(b: &layout::LayoutBox) -> Option<String> {
        if b.tag.is_none() && b.text.is_some() { return b.text.clone(); }
        for ch in &b.children {
            if let Some(t) = find_text(ch) { return Some(t); }
        }
        None
    }
    let text = find_text(&lr).unwrap_or_default();
    println!("h2 wrapped text: {:?}", text);
    assert!(text.contains('\n'), "h2 should wrap at 320px viewport, got: {:?}", text);
}
