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
fn parse_length_units() {
    assert_eq!(layout::parse_length("16px"), 16.0);
    assert_eq!(layout::parse_length("2em"),  32.0);
    assert_eq!(layout::parse_length("1rem"), 16.0);
}

#[test]
fn measure_text_width_estimate() {
    // 5 chars * 16px * 0.55 = 44.0
    let w = layout::measure_text_width("hello", 16.0);
    assert!((w - 44.0).abs() < 0.1);
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
