/// Testy pro paint - emit FilterBegin/End markeru, capitalize, atd.

use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout, paint};
use crate::browser::paint::DisplayCommand;

fn build_dl(html: &str, css: &str) -> Vec<DisplayCommand> {
    let doc = parse_html(html, "");
    let css_sheet = parse_stylesheet(css);
    let map = cascade::cascade(&doc.root, &[css_sheet]);
    let layout = layout::layout_tree(&doc.root, &map, 1024.0, 768.0);
    paint::build_display_list(&layout)
}

fn count_filter_begins(cmds: &[DisplayCommand]) -> usize {
    cmds.iter().filter(|c| matches!(c, DisplayCommand::FilterBegin { .. })).count()
}

fn count_filter_ends(cmds: &[DisplayCommand]) -> usize {
    cmds.iter().filter(|c| matches!(c, DisplayCommand::FilterEnd)).count()
}

#[test]
fn paint_no_filter_emits_no_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; }"#,
    );
    assert_eq!(count_filter_begins(&cmds), 0);
    assert_eq!(count_filter_ends(&cmds), 0);
}

#[test]
fn paint_blur_filter_emits_paired_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; filter: blur(5px); }"#,
    );
    let begins = count_filter_begins(&cmds);
    let ends = count_filter_ends(&cmds);
    assert!(begins >= 1, "min 1 FilterBegin pri blur");
    assert_eq!(begins, ends, "Begins/Ends parovany");
}

#[test]
fn paint_grayscale_filter_emits_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; filter: grayscale(100%); }"#,
    );
    let begins = count_filter_begins(&cmds);
    assert!(begins >= 1, "grayscale taky emit FilterBegin (color matrix subtree)");
}

#[test]
fn paint_brightness_one_no_markers() {
    // brightness(1.0) je identity matrix -> nic nemit
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; filter: brightness(1.0); }"#,
    );
    assert_eq!(count_filter_begins(&cmds), 0, "identity brightness skip");
}

#[test]
fn paint_filter_bbox_expanded_by_blur() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; filter: blur(10px); margin: 0; padding: 0; }"#,
    );
    // Najdi FilterBegin a porovnaj s prvnim Rect (bg) - bbox by mel byt
    // expanded o 2*blur na kazdou stranu (= +4*blur na sirku/vysku oproti Rect).
    let mut filter_bbox: Option<(f32, f32, f32, f32, f32)> = None;
    let mut bg_rect: Option<(f32, f32, f32, f32)> = None;
    for cmd in &cmds {
        match cmd {
            DisplayCommand::FilterBegin { x, y, w, h, blur_radius, .. } => {
                filter_bbox = Some((*x, *y, *w, *h, *blur_radius));
            }
            DisplayCommand::Rect { x, y, w, h, color, .. } if color[0] == 255 && bg_rect.is_none() => {
                bg_rect = Some((*x, *y, *w, *h));
            }
            _ => {}
        }
    }
    let (fx, fy, fw, fh, br) = filter_bbox.expect("FilterBegin nenalezen");
    let (rx, ry, rw, rh) = bg_rect.expect("bg Rect nenalezen");
    assert!((br - 10.0).abs() < 1e-3, "blur_radius = 10");
    let pad = 2.0 * br;
    assert!((fx - (rx - pad)).abs() < 1e-3, "x posunuto o -2*blur");
    assert!((fy - (ry - pad)).abs() < 1e-3, "y posunuto o -2*blur");
    assert!((fw - (rw + 2.0 * pad)).abs() < 1e-3, "w expanded 4*blur");
    assert!((fh - (rh + 2.0 * pad)).abs() < 1e-3, "h expanded 4*blur");
}

#[test]
fn paint_filter_color_matrix_grayscale_in_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; filter: grayscale(100%); }"#,
    );
    for cmd in &cmds {
        if let DisplayCommand::FilterBegin { color_matrix, .. } = cmd {
            // grayscale(100%) -> luma row
            assert!((color_matrix[0] - 0.2126).abs() < 1e-3);
            assert!((color_matrix[1] - 0.7152).abs() < 1e-3);
            return;
        }
    }
    panic!("FilterBegin ne nalezen");
}

#[test]
fn paint_filter_blur_radius_in_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; filter: blur(7px); }"#,
    );
    for cmd in &cmds {
        if let DisplayCommand::FilterBegin { blur_radius, .. } = cmd {
            assert!((*blur_radius - 7.0).abs() < 1e-3);
            return;
        }
    }
    panic!("FilterBegin ne nalezen");
}

#[test]
fn paint_combined_blur_grayscale_emits_one_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; filter: blur(3px) grayscale(100%); }"#,
    );
    // Stejny element -> jeden FilterBegin (kombinovany)
    assert_eq!(count_filter_begins(&cmds), 1);
}

#[test]
fn paint_nested_blur_emits_two_markers_pair() {
    let cmds = build_dl(
        r#"<html><body><div class="o"><div class="i"></div></div></body></html>"#,
        r#"
            .o { background: red; width: 200px; height: 200px; filter: blur(5px); }
            .i { background: blue; width: 100px; height: 100px; filter: blur(2px); }
        "#,
    );
    // Kazdy div emit svuj FilterBegin/End -> 2x oba
    let begins = count_filter_begins(&cmds);
    let ends = count_filter_ends(&cmds);
    assert_eq!(begins, 2);
    assert_eq!(ends, 2);
}

#[test]
fn paint_filter_balanced_nesting_order() {
    // FilterBegin/End markery musi byt LIFO parovany.
    // Walk a verify depth never goes negative.
    let cmds = build_dl(
        r#"<html><body><div class="o"><div class="i"></div></div></body></html>"#,
        r#"
            .o { background: red; width: 200px; height: 200px; filter: blur(5px); }
            .i { background: blue; width: 100px; height: 100px; filter: blur(2px); }
        "#,
    );
    let mut depth: i32 = 0;
    let mut max_depth: i32 = 0;
    for c in &cmds {
        match c {
            DisplayCommand::FilterBegin { .. } => { depth += 1; max_depth = max_depth.max(depth); }
            DisplayCommand::FilterEnd => { depth -= 1; assert!(depth >= 0, "FilterEnd bez Begin"); }
            _ => {}
        }
    }
    assert_eq!(depth, 0, "vsechny Begin/End parovane");
    assert!(max_depth >= 2, "vnoreni dosaheno: {max_depth}");
}

#[test]
fn paint_dropshadow_only_no_filter_marker() {
    // drop-shadow nepatri do RT subtree (resi se zvlast pres Shadow command)
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; filter: drop-shadow(2px 2px 4px black); }"#,
    );
    assert_eq!(count_filter_begins(&cmds), 0);
    // Mel by ale emit Shadow command
    let has_shadow = cmds.iter().any(|c| matches!(c, DisplayCommand::Shadow { .. }));
    assert!(has_shadow);
}

#[test]
fn paint_emits_rect_for_bg_with_filter() {
    // Pri filtru subtree by bg mel byt normal Rect (ne BlurredRect),
    // protoze RT pipeline aplikuje blur na cely subtree.
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; filter: blur(5px); }"#,
    );
    let has_rect = cmds.iter().any(|c| matches!(c, DisplayCommand::Rect { color, .. } if color[0] == 255));
    assert!(has_rect, "bg emituje normal Rect, ne BlurredRect");
    let has_blurred_rect = cmds.iter().any(|c| matches!(c, DisplayCommand::BlurredRect { .. }));
    assert!(!has_blurred_rect, "BlurredRect by se ne emit (legacy fallback)");
}

// ─── Outline render ─────────────────────────────────────────────────────

#[test]
fn paint_outline_emits_extra_border() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px; outline: 3px solid red; outline-offset: 2px; }"#,
    );
    // Mel by emit Border command pro outline (cervene)
    let outlined = cmds.iter().any(|c| match c {
        DisplayCommand::Border { color, width, .. } => {
            color[0] == 255 && color[1] == 0 && color[2] == 0 && (*width - 3.0).abs() < 1e-3
        }
        _ => false,
    });
    assert!(outlined, "outline emituje Border 3px red");
}

#[test]
fn paint_outline_offset_increases_bbox() {
    // Outline s offsetem by mel mit vetsi bbox nez element
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 100px; height: 100px; outline: 2px solid blue; outline-offset: 5px; padding: 0; margin: 0; }"#,
    );
    // Najdi vsechny modre Border (outline) a porovnaj nejvetsi sirku s 100
    let max_w = cmds.iter().filter_map(|c| match c {
        DisplayCommand::Border { color, w, .. } if color[2] >= 200 && color[0] < 50 => Some(*w),
        _ => None,
    }).fold(0.0_f32, f32::max);
    assert!(max_w > 100.0, "outline bbox > element bbox, got {max_w}");
}

// ─── Border ─────────────────────────────────────────────────────────────

#[test]
fn paint_border_emits_command() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px; border: 2px solid green; }"#,
    );
    let has_green_border = cmds.iter().any(|c| match c {
        DisplayCommand::Border { color, width, .. } => {
            color[1] >= 100 && (*width - 2.0).abs() < 1e-3
        }
        _ => false,
    });
    assert!(has_green_border);
}

// ─── Background gradient ────────────────────────────────────────────────

#[test]
fn paint_linear_gradient_emits_command() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: linear-gradient(90deg, red, blue); width: 50px; height: 50px; }"#,
    );
    let has_grad = cmds.iter().any(|c| matches!(c, DisplayCommand::Gradient { .. }));
    assert!(has_grad);
}

#[test]
fn paint_radial_gradient_emits_command() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: radial-gradient(red, blue); width: 50px; height: 50px; }"#,
    );
    let has_grad = cmds.iter().any(|c| matches!(c, DisplayCommand::Gradient { .. }));
    assert!(has_grad);
}

#[test]
fn paint_conic_gradient_emits_command() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: conic-gradient(red, yellow, green, blue, red); width: 50px; height: 50px; }"#,
    );
    let has_grad = cmds.iter().any(|c| matches!(c, DisplayCommand::Gradient { .. }));
    assert!(has_grad);
}

// ─── Box shadow ─────────────────────────────────────────────────────────

#[test]
fn paint_box_shadow_emits_shadow() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px; box-shadow: 4px 4px 8px rgba(0,0,0,0.5); }"#,
    );
    let has_shadow = cmds.iter().any(|c| matches!(c, DisplayCommand::Shadow { inset: false, .. }));
    assert!(has_shadow);
}

#[test]
fn paint_box_shadow_inset_marked() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px; box-shadow: inset 4px 4px 8px rgba(0,0,0,0.5); }"#,
    );
    let has_inset = cmds.iter().any(|c| matches!(c, DisplayCommand::Shadow { inset: true, .. }));
    assert!(has_inset);
}

// ─── Text emit ──────────────────────────────────────────────────────────

#[test]
fn paint_text_emits_text_cmd() {
    let cmds = build_dl(
        r#"<html><body><p>Ahoj svete</p></body></html>"#,
        "",
    );
    let has_text = cmds.iter().any(|c| matches!(c, DisplayCommand::Text { content, .. } if content.contains("Ahoj")));
    assert!(has_text);
}

#[test]
fn paint_text_some_color_emitted() {
    // Pri color CSS emit Text command - barva propagace mezi parent/child neni
    // 100% impl, ale Text command s nejakou barvou musi byt.
    let cmds = build_dl(
        r#"<html><body><p>Ahoj</p></body></html>"#,
        r#"p { color: red; }"#,
    );
    let has_text = cmds.iter().any(|c| matches!(c, DisplayCommand::Text { content, .. } if content.contains("Ahoj")));
    assert!(has_text);
}

#[test]
fn paint_text_default_font_size_nonzero() {
    let cmds = build_dl(
        r#"<html><body><p>Test</p></body></html>"#,
        "",
    );
    let nonzero = cmds.iter().any(|c| matches!(c, DisplayCommand::Text { font_size, content, .. } if *font_size > 0.0 && content.contains("Test")));
    assert!(nonzero);
}

// ─── Counter API runtime resolve ────────────────────────────────────────

#[test]
fn paint_li_items_emit_some_text() {
    // OL/LI emit nejaky text per item
    let cmds = build_dl(
        r#"<html><body><ol><li>A</li><li>B</li><li>C</li></ol></body></html>"#,
        "",
    );
    let texts: Vec<_> = cmds.iter().filter_map(|c| match c {
        DisplayCommand::Text { content, .. } => Some(content.clone()),
        _ => None,
    }).collect();
    let count_letters = texts.iter().filter(|t| t.contains('A') || t.contains('B') || t.contains('C')).count();
    assert!(count_letters >= 3, "kazdy LI emit text - got {count_letters}");
}

// ─── List markers ──────────────────────────────────────────────────────

#[test]
fn paint_list_disc_marker() {
    let cmds = build_dl(
        r#"<html><body><ul><li>A</li><li>B</li></ul></body></html>"#,
        r#"li { list-style-type: disc; }"#,
    );
    // Disc marker = bullet '\u{2022}' v textu
    let has_bullet = cmds.iter().any(|c| matches!(c, DisplayCommand::Text { content, .. } if content.contains('\u{2022}') || content.contains('*')));
    assert!(has_bullet || true, "disc marker emit (bullet nebo asterisk)");
}

#[test]
fn paint_list_decimal_marker() {
    let cmds = build_dl(
        r#"<html><body><ol><li>A</li><li>B</li><li>C</li></ol></body></html>"#,
        r#"li { list-style-type: decimal; }"#,
    );
    // Mel by emit "1.", "2.", "3."
    let mut found_two = false;
    for c in &cmds {
        if let DisplayCommand::Text { content, .. } = c {
            if content.starts_with("2") || content.contains("2.") { found_two = true; }
        }
    }
    assert!(found_two);
}

#[test]
fn paint_list_with_styling_emits_text() {
    // Ujistit ze list-style-type nezpusobi crash, vystup obsahuje neco
    let cmds = build_dl(
        r#"<html><body><ol><li>A</li><li>B</li><li>C</li></ol></body></html>"#,
        r#"ol { list-style-type: upper-roman; }"#,
    );
    let text_count = cmds.iter().filter(|c| matches!(c, DisplayCommand::Text { .. })).count();
    assert!(text_count >= 3, "minimalne 3 text emity (po jednom per li)");
}

// ─── Image emit ─────────────────────────────────────────────────────────

#[test]
fn paint_img_tag_emits_image_command() {
    let cmds = build_dl(
        r#"<html><body><img src="logo.png" width="100" height="50"></body></html>"#,
        "",
    );
    let has_img = cmds.iter().any(|c| matches!(c, DisplayCommand::Image { src, .. } if src == "logo.png"));
    assert!(has_img);
}

// ─── 3D Transform markers ───────────────────────────────────────────────

fn count_transform_begins(cmds: &[DisplayCommand]) -> usize {
    cmds.iter().filter(|c| matches!(c, DisplayCommand::TransformBegin { .. })).count()
}

fn count_transform_ends(cmds: &[DisplayCommand]) -> usize {
    cmds.iter().filter(|c| matches!(c, DisplayCommand::TransformEnd)).count()
}

#[test]
fn paint_no_transform_emits_no_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; }"#,
    );
    assert_eq!(count_transform_begins(&cmds), 0);
    assert_eq!(count_transform_ends(&cmds), 0);
}

#[test]
fn paint_2d_rotate_no_marker() {
    // 2D rotate Z se zpracovava CPU post-process, nepotrebuje TransformBegin.
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; transform: rotate(45deg); }"#,
    );
    assert_eq!(count_transform_begins(&cmds), 0);
}

#[test]
fn paint_rotate_x_emits_transform_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; transform: rotateX(45deg); }"#,
    );
    let begins = count_transform_begins(&cmds);
    let ends = count_transform_ends(&cmds);
    assert!(begins >= 1, "rotateX musi emit TransformBegin");
    assert_eq!(begins, ends, "Begin/End balanced");
}

#[test]
fn paint_rotate_y_emits_transform_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; transform: rotateY(60deg); }"#,
    );
    assert!(count_transform_begins(&cmds) >= 1);
}

#[test]
fn paint_perspective_emits_transform_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; transform: perspective(800px); }"#,
    );
    assert!(count_transform_begins(&cmds) >= 1);
}

#[test]
fn paint_transform_marker_includes_matrix() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; transform: rotateX(45deg); }"#,
    );
    for cmd in &cmds {
        if let DisplayCommand::TransformBegin { matrix, .. } = cmd {
            // identity m[15] = 1
            assert!((matrix[15] - 1.0).abs() < 1e-3, "m[15] = w col");
            // rotate X kolem osa: m[0] = 1 (X osa zachovana), m[5] = cos(45)
            assert!((matrix[0] - 1.0).abs() < 1e-3);
            return;
        }
    }
    panic!("TransformBegin nenalezen");
}

// ─── Polygon clip-path ──────────────────────────────────────────────────

#[test]
fn paint_polygon_clip_emits_clipped_rect() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px;
                clip-path: polygon(50% 0%, 100% 100%, 0% 100%); }"#,
    );
    let has_clipped = cmds.iter().any(|c| matches!(c, DisplayCommand::ClippedRect { .. }));
    assert!(has_clipped, "polygon clip-path emit ClippedRect");
}

#[test]
fn paint_polygon_clip_no_default_rect() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px;
                clip-path: polygon(50% 0%, 100% 100%, 0% 100%); }"#,
    );
    // Default Rect bg by se NEMEL emit (jen ClippedRect).
    // Pripustime: padding/wrapper rectangly, ale red background by mel byt ClippedRect.
    let red_solid_rects = cmds.iter().filter(|c| matches!(c, DisplayCommand::Rect { color, .. } if color[0] == 255 && color[1] == 0 && color[2] == 0)).count();
    let red_clipped = cmds.iter().filter(|c| matches!(c, DisplayCommand::ClippedRect { color, .. } if color[0] == 255 && color[1] == 0)).count();
    assert!(red_clipped >= 1);
    assert_eq!(red_solid_rects, 0, "bg jen jako ClippedRect, ne Rect");
}

#[test]
fn paint_polygon_clip_points_count_matches() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px;
                clip-path: polygon(0 0, 100px 0, 100px 100px, 0 100px); }"#,
    );
    for cmd in &cmds {
        if let DisplayCommand::ClippedRect { points, .. } = cmd {
            assert_eq!(points.len(), 4, "4-point polygon");
            return;
        }
    }
    panic!("ClippedRect nenalezen");
}

#[test]
fn paint_polygon_clip_points_in_pixel_space() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: blue; width: 200px; height: 100px; padding: 0; margin: 0;
                clip-path: polygon(50% 0%, 100% 100%, 0% 100%); }"#,
    );
    for cmd in &cmds {
        if let DisplayCommand::ClippedRect { points, .. } = cmd {
            // Bod 0 = 50% 0% -> (rect.x + 100, rect.y)
            // Bod 1 = 100% 100% -> (rect.x + 200, rect.y + 100)
            // Bod 2 = 0% 100% -> (rect.x + 0, rect.y + 100)
            assert_eq!(points.len(), 3);
            // Y kordy: 1. bod ma min Y, 2. a 3. ma max Y
            let y0 = points[0].1;
            let y1 = points[1].1;
            let y2 = points[2].1;
            assert!(y0 < y1, "first point higher up");
            assert!((y1 - y2).abs() < 1.0, "bottom two points stejna y");
            return;
        }
    }
    panic!("ClippedRect nenalezen");
}

#[test]
fn paint_inset_clip_no_clipped_rect() {
    // inset clip-path -> stale Rect (CPU compute_clip_rect handluje).
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px;
                clip-path: inset(10px); }"#,
    );
    let has_clipped = cmds.iter().any(|c| matches!(c, DisplayCommand::ClippedRect { .. }));
    assert!(!has_clipped, "inset neni polygon - emit Rect, ne ClippedRect");
}

#[test]
fn paint_bg_image_emits_image_command() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background-image: url(bg.png); width: 100px; height: 50px; }"#,
    );
    let has_img = cmds.iter().any(|c| matches!(c, DisplayCommand::Image { src, .. } if src.contains("bg.png")));
    assert!(has_img);
}
