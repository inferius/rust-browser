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
