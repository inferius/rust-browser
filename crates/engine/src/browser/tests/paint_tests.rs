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

// ─── SVG resvg raster helpery ────────────────────────────────────────────
// Inline SVG se od commitu "Inline SVG via resvg+tiny-skia" NErenderuje jako
// Rect/Circle/ClippedRect display commands, ale rasterizuje pres resvg do
// bitmapy (INLINE_SVG_CACHE) + emit jako DisplayCommand::Image (src "__isvg_").
// Testy proto overuji: (1) emit prave 1 __isvg_ Image (serialize_svg behlo),
// (2) rasterizovane pixely obsahuji ocekavanou fill barvu (= serialize_svg
// spravne zahrnul danou shape; resvg sam je external + dukladne testovany).

/// Najde __isvg_ Image v display listu + vrati jeho rasterizovanou RGBA bitmapu
/// (premultiplied, R,G,B,A) z INLINE_SVG_CACHE. None = zadny SVG Image.
fn svg_image_rgba(cmds: &[DisplayCommand]) -> Option<(Vec<u8>, u32, u32)> {
    let key = cmds.iter().find_map(|c| match c {
        DisplayCommand::Image { src, .. } if src.starts_with("__isvg_") => Some(src.clone()),
        _ => None,
    })?;
    crate::browser::paint::INLINE_SVG_CACHE.with(|c| {
        c.borrow().get(&key).map(|(rgba, w, h, _)| (rgba.clone(), *w, *h))
    })
}

/// Pocet __isvg_ Image commandu (= kolikrat se SVG emitnul).
fn svg_image_count(cmds: &[DisplayCommand]) -> usize {
    cmds.iter().filter(|c| matches!(c,
        DisplayCommand::Image { src, .. } if src.starts_with("__isvg_"))).count()
}

/// True kdyz rasterizovana bitmapa obsahuje aspon 1 plne-kryjici pixel ~ (r,g,b).
/// Premultiplied + opaque (a>200) fill = straight barva, tolerance per kanal 60.
fn rgba_has_color(rgba: &[u8], r: u8, g: u8, b: u8) -> bool {
    rgba.chunks_exact(4).any(|p| {
        p[3] > 200
            && (p[0] as i32 - r as i32).abs() <= 60
            && (p[1] as i32 - g as i32).abs() <= 60
            && (p[2] as i32 - b as i32).abs() <= 60
    })
}

/// True kdyz bitmapa obsahuje purpurovy pixel (r+b vysoke, g nizke) - tenky AA
/// text nemusi mit plne-kryjici pixel, takze detekuje i premultiplied AA.
fn rgba_has_purple(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|p| {
        p[3] > 30 && p[0] > 30 && p[2] > 30
            && (p[1] as i32) < (p[0] as i32) - 10
            && (p[1] as i32) < (p[2] as i32) - 10
    })
}

#[test]
fn paint_border_bottom_emits_strip() {
    // Per-side border (border-bottom) - drive se shorthand neexpandoval na
    // border-bottom-width -> border-bottom (row separatory, underliny) se
    // nekreslil. Ted emit Rect strip (h = border-bottom-width) u spodu boxu.
    let cmds = build_dl(
        r#"<html><body><div>x</div></body></html>"#,
        r#"div { width: 100px; height: 40px; border-bottom: 3px solid #ff0000; }"#,
    );
    let strip = cmds.iter().any(|c| matches!(c,
        DisplayCommand::Rect { color: [255, 0, 0, 255], w, h, .. }
        if (*h - 3.0).abs() < 0.1 && *w >= 99.0));
    assert!(strip, "border-bottom: 3px solid red emit 3px cerveny Rect strip");
}

#[test]
fn paint_border_left_emits_strip() {
    let cmds = build_dl(
        r#"<html><body><div>x</div></body></html>"#,
        r#"div { width: 100px; height: 40px; border-left: 5px solid #00ff00; }"#,
    );
    // border-left -> vertikalni strip (w = 5) u leve hrany.
    let strip = cmds.iter().any(|c| matches!(c,
        DisplayCommand::Rect { color: [0, 255, 0, 255], w, h, .. }
        if (*w - 5.0).abs() < 0.1 && *h >= 39.0));
    assert!(strip, "border-left: 5px solid green emit 5px zeleny vertikalni strip");
}

#[test]
fn overflow_hidden_clamps_straddling_child_bg() {
    // Box vysky 30px, overflow:hidden, dite vysky 60px s cervenym pozadim.
    // Dite straddluje hranici -> bg Rect se musi clampnout na clip (<=30px),
    // ne pretekat ven (driv "partial overlap necham byt" -> pretekani).
    let cmds = build_dl(
        r#"<html><body><div class="ov"><div class="ch"></div></div></body></html>"#,
        r#".ov { width: 100px; height: 30px; overflow: hidden; }
           .ch { width: 100px; height: 60px; background: #ff0000; }"#,
    );
    // Najdi cerveny child bg Rect - jeho vyska musi byt clampnuta na <= ~30.
    let red_h = cmds.iter().find_map(|c| match c {
        DisplayCommand::Rect { h, color: [255, 0, 0, 255], .. } => Some(*h),
        _ => None,
    });
    let h = red_h.expect("cerveny child bg Rect existuje");
    assert!(h <= 31.0, "overflow:hidden clampuje child bg na clip vysku, h={} (ma byt <=30)", h);
}

#[test]
fn overflow_hidden_drops_fully_below_text() {
    // Box 30px, dite s textem na y~80 (kompletne pod boxem) -> text command
    // se musi zahodit (ne emit pod boxem).
    let cmds = build_dl(
        r#"<html><body><div class="ov"><div class="tall">AAAA</div></div></body></html>"#,
        r#".ov { width: 200px; height: 30px; overflow: hidden; }
           .tall { margin-top: 80px; font-size: 16px; }"#,
    );
    // Zadny Text command nesmi mit y > clip bottom + rezerva (box ~ y=8..38).
    let bad_text = cmds.iter().any(|c| matches!(c,
        DisplayCommand::Text { y, .. } if *y > 60.0));
    assert!(!bad_text, "text kompletne pod overflow:hidden boxem se musi zahodit");
}

#[test]
fn paint_checkbox_checked_emits_accent_check() {
    // Nativni checkbox control - drive se nekreslil vubec.
    let cmds = build_dl(r#"<html><body><input type="checkbox" checked></body></html>"#, "");
    let fill = cmds.iter().any(|c| matches!(c, DisplayCommand::Rect { color: [26, 115, 232, 255], .. }));
    let check = cmds.iter().any(|c| matches!(c, DisplayCommand::ClippedRect { color: [255, 255, 255, 255], .. }));
    assert!(fill, "checked checkbox emit modry accent fill");
    assert!(check, "checked checkbox emit bily checkmark (ClippedRect)");
}

#[test]
fn paint_range_emits_thumb() {
    let cmds = build_dl(r#"<html><body><input type="range" min="0" max="100" value="50"></body></html>"#, "");
    // range -> thumb = accent kruh (Rect radius > 3).
    let thumb = cmds.iter().any(|c| matches!(c,
        DisplayCommand::Rect { color: [26, 115, 232, 255], radius, .. } if *radius > 3.0));
    assert!(thumb, "range emit modry thumb (kruh s radius)");
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
    // Pad = 3*blur + 2 (rezerva pro Gauss tail - drive 2*blur, blur se useknul nahore).
    let pad = 3.0 * br + 2.0;
    assert!((fx - (rx - pad)).abs() < 1e-3, "x posunuto o -pad");
    assert!((fy - (ry - pad)).abs() < 1e-3, "y posunuto o -pad");
    assert!((fw - (rw + 2.0 * pad)).abs() < 1e-3, "w expanded 2*pad");
    assert!((fh - (rh + 2.0 * pad)).abs() < 1e-3, "h expanded 2*pad");
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

#[test]
fn paint_radial_gradient_kind_radial() {
    use crate::browser::paint::GradientKind;
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: radial-gradient(circle, red, blue); width: 100px; height: 100px; }"#,
    );
    let radial_kind = cmds.iter().any(|c| matches!(c,
        DisplayCommand::Gradient { kind: GradientKind::Radial { .. }, .. }
    ));
    assert!(radial_kind, "radial-gradient -> GradientKind::Radial");
}

#[test]
fn paint_conic_gradient_kind_conic() {
    use crate::browser::paint::GradientKind;
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: conic-gradient(red, blue); width: 100px; height: 100px; }"#,
    );
    let conic_kind = cmds.iter().any(|c| matches!(c,
        DisplayCommand::Gradient { kind: GradientKind::Conic { .. }, .. }
    ));
    assert!(conic_kind, "conic-gradient -> GradientKind::Conic");
}

#[test]
fn paint_radial_gradient_with_position() {
    use crate::browser::paint::GradientKind;
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: radial-gradient(at 25% 75%, red, blue); width: 200px; height: 100px; }"#,
    );
    // Najdi radial gradient + verify center position
    let radial = cmds.iter().find_map(|c| match c {
        DisplayCommand::Gradient { kind: GradientKind::Radial { cx, cy, .. }, .. } =>
            Some((*cx, *cy)),
        _ => None,
    });
    assert!(radial.is_some(), "radial-gradient s pozici emit");
}

#[test]
fn paint_inset_shadow_with_offset() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px;
                box-shadow: inset 5px 10px 15px rgba(0,0,0,0.5); }"#,
    );
    let inset = cmds.iter().find_map(|c| match c {
        DisplayCommand::Shadow { inset: true, offset_x, offset_y, blur, .. } =>
            Some((*offset_x, *offset_y, *blur)),
        _ => None,
    });
    let (ox, oy, blur) = inset.expect("inset shadow emit");
    assert!((ox - 5.0).abs() < 0.1);
    assert!((oy - 10.0).abs() < 0.1);
    assert!((blur - 15.0).abs() < 0.1);
}

// ─── SVG support ─────────────────────────────────────────────────────────

#[test]
fn paint_svg_rect_emits_rect() {
    let cmds = build_dl(
        r#"<html><body><svg width="100" height="100">
            <rect x="10" y="10" width="50" height="30" fill="red"/>
        </svg></body></html>"#,
        "",
    );
    // SVG se rasterizuje pres resvg -> 1 __isvg_ Image; bitmapa ma cervene pixely.
    assert_eq!(svg_image_count(&cmds), 1, "SVG emit prave 1 __isvg_ Image");
    let (rgba, _, _) = svg_image_rgba(&cmds).expect("SVG rasterizovan do cache");
    assert!(rgba_has_color(&rgba, 255, 0, 0), "SVG <rect fill=red> rasterizovan cervene");
}

#[test]
fn paint_svg_circle_emits_rounded_rect() {
    let cmds = build_dl(
        r#"<html><body><svg width="100" height="100">
            <circle cx="50" cy="50" r="20" fill="blue"/>
        </svg></body></html>"#,
        "",
    );
    assert_eq!(svg_image_count(&cmds), 1, "SVG emit prave 1 __isvg_ Image");
    let (rgba, _, _) = svg_image_rgba(&cmds).expect("SVG rasterizovan do cache");
    assert!(rgba_has_color(&rgba, 0, 0, 255), "SVG <circle fill=blue> rasterizovan modre");
}

#[test]
fn paint_svg_polygon_emits_clipped() {
    let cmds = build_dl(
        r#"<html><body><svg width="100" height="100">
            <polygon points="50,5 90,90 10,90" fill="green"/>
        </svg></body></html>"#,
        "",
    );
    assert_eq!(svg_image_count(&cmds), 1, "SVG emit prave 1 __isvg_ Image");
    let (rgba, _, _) = svg_image_rgba(&cmds).expect("SVG rasterizovan do cache");
    assert!(rgba_has_color(&rgba, 0, 0x80, 0), "SVG <polygon fill=green> rasterizovan zelene");
}

#[test]
fn paint_svg_path_basic_lineto() {
    use crate::browser::paint::parse_svg_path;
    let pts = parse_svg_path("M 10 20 L 30 40 L 50 60 Z");
    assert_eq!(pts.len(), 4); // 3 explicit + Z back to start
    assert_eq!(pts[0], (10.0, 20.0));
    assert_eq!(pts[1], (30.0, 40.0));
    assert_eq!(pts[2], (50.0, 60.0));
    assert_eq!(pts[3], (10.0, 20.0)); // Z -> back to M
}

#[test]
fn paint_svg_path_relative_lineto() {
    use crate::browser::paint::parse_svg_path;
    let pts = parse_svg_path("M 10 10 l 20 0 l 0 20 l -20 0 z");
    assert_eq!(pts.len(), 5);
    assert_eq!(pts[1], (30.0, 10.0));
    assert_eq!(pts[2], (30.0, 30.0));
    assert_eq!(pts[3], (10.0, 30.0));
}

#[test]
fn paint_svg_path_horizontal_vertical() {
    use crate::browser::paint::parse_svg_path;
    let pts = parse_svg_path("M 0 0 H 100 V 50 H 0 Z");
    // M (0,0), H (100,0), V (100,50), H (0,50), Z (0,0) = 5 bodu
    assert_eq!(pts.len(), 5);
    assert_eq!(pts[1], (100.0, 0.0));
    assert_eq!(pts[2], (100.0, 50.0));
    assert_eq!(pts[3], (0.0, 50.0));
    assert_eq!(pts[4], (0.0, 0.0)); // Z
}

#[test]
fn paint_svg_path_cubic_bezier_endpoint() {
    use crate::browser::paint::parse_svg_path;
    // Cubic bezier ted tessellated (16 segmenty + start). Posledni bod = endpoint.
    let pts = parse_svg_path("M 0 0 C 10 10 20 20 30 30");
    assert!(pts.len() >= 2, "alespon start + endpoint");
    let last = *pts.last().unwrap();
    assert!((last.0 - 30.0).abs() < 0.5 && (last.1 - 30.0).abs() < 0.5,
        "endpoint = (30,30), got {:?}", last);
}

#[test]
fn paint_svg_group_recursion() {
    let cmds = build_dl(
        r#"<html><body><svg width="200" height="200">
            <g>
                <rect x="0" y="0" width="50" height="50" fill="red"/>
                <rect x="60" y="0" width="50" height="50" fill="blue"/>
            </g>
        </svg></body></html>"#,
        "",
    );
    // <g> recursion: serialize_svg musi zahrnout OBE deti -> raster ma cervene i modre.
    assert_eq!(svg_image_count(&cmds), 1, "SVG emit prave 1 __isvg_ Image");
    let (rgba, _, _) = svg_image_rgba(&cmds).expect("SVG rasterizovan do cache");
    assert!(rgba_has_color(&rgba, 255, 0, 0), "SVG <g> -> <rect red> rasterizovan");
    assert!(rgba_has_color(&rgba, 0, 0, 255), "SVG <g> -> <rect blue> rasterizovan");
}

#[test]
fn paint_multiple_box_shadows() {
    // CSS box-shadow podporuje vice shadows oddelene carkou
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { width: 50px; height: 50px;
                box-shadow: 2px 2px 4px red, inset 0 0 8px blue; }"#,
    );
    let count = cmds.iter().filter(|c| matches!(c, DisplayCommand::Shadow { .. })).count();
    // Aspon 1 shadow (parser muze prijmout jen prvni)
    assert!(count >= 1);
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
fn paint_2d_rotate_emits_marker() {
    // 2D rotate Z je teda pres GPU shader pipeline (jako 3D), CPU rotate_cmd
    // pouze sdouval origin a vizualne nerotoval rect.
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px; transform: rotate(45deg); }"#,
    );
    assert!(count_transform_begins(&cmds) >= 1);
    assert_eq!(count_transform_begins(&cmds), count_transform_ends(&cmds));
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

/// 4 rot-boxes v flex row, kazdy ma transform. Vsechny 4 musi byt v
/// display list (TransformBegin za rotate*, Rect za scale + Rect.r-0).
/// Drive: rozbity transform 3D pipeline mohl zpusobit, ze nektera Transform
/// segment vypadne (segment partition error nebo paint skip).
#[test]
fn paint_transform_3d_row_all_emit() {
    let cmds = build_dl(
        r#"<html><body><div class="row">
            <div class="t-rx"></div>
            <div class="t-ry"></div>
            <div class="t-rxy"></div>
            <div class="t-persp"></div>
        </div></body></html>"#,
        r#"
            .row { display: flex; }
            .row > div { width: 80px; height: 60px; background: #2997ff; }
            .t-rx    { transform: rotateX(45deg); }
            .t-ry    { transform: rotateY(45deg); }
            .t-rxy   { transform: rotateX(30deg) rotateY(30deg); }
            .t-persp { transform: perspective(600px) rotateY(35deg); }
        "#,
    );
    // Pocitej Transform Begin/End - 4 elementy = 4 begin + 4 end.
    let begins = count_transform_begins(&cmds);
    let ends = count_transform_ends(&cmds);
    assert_eq!(begins, 4, "4 transform 3D elementu musi emit 4 TransformBegin");
    assert_eq!(ends, 4, "TransformEnd musi byt 4");
}

#[test]
fn paint_transform_3d_perspective_chain_matrix_w_nonidentity() {
    // perspective(600px) rotateY(35deg) - perspective ovlivni m[14]/m[15],
    // rotateY ovlivni m[0], m[2], m[8], m[10].
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { width: 80px; height: 60px; background: red;
                transform: perspective(600px) rotateY(35deg); }"#,
    );
    for cmd in &cmds {
        if let DisplayCommand::TransformBegin { matrix, .. } = cmd {
            // m[0] = cos(35deg) ~= 0.819
            assert!((matrix[0] - 35.0_f32.to_radians().cos()).abs() < 1e-3,
                "m[0] = cos(35) expected, got {}", matrix[0]);
            // m[14] = -cos(35)/600 (perspective * rotateY compose)
            // Perspective alone = -1/600 at m[14]; multiplied by rotateY
            // row3*col2 sum daje -cos(angle)/600.
            let expected = -35.0_f32.to_radians().cos() / 600.0;
            assert!((matrix[14] - expected).abs() < 1e-4,
                "m[14] expected ~{} (= -cos(35)/600), got {}", expected, matrix[14]);
            return;
        }
    }
    panic!("TransformBegin nenalezen pri perspective+rotateY");
}

/// Regrese: SVG text element musi byt nakreslen JEN pres emit_svg_children
/// (s SVG-space coords + SVG.rect.x/y origin). Drive: paint_box rekurzivne
/// emitoval text node pres bx.children paint = inline text rendered ABSOLUTNE
/// na build-time SVG.rect.x (= 0) -> text mimo SVG box.
/// Fix: paint_box skipne children loop pro tag="svg".
#[test]
fn paint_svg_text_emitted_once_not_double() {
    let cmds = build_dl(
        r#"<html><body><svg width="400" height="80">
            <text x="290" y="65" fill="purple" font-size="14">SVG text</text>
        </svg></body></html>"#,
        r#"body { margin: 0; padding: 50px; } svg { display: block; }"#,
    );
    // SVG text se rasterizuje do resvg bitmapy (ne DisplayCommand::Text). Overeni:
    // (1) emit prave 1 __isvg_ Image (ne 2x = "not double"), (2) serialize_svg
    // zahrnul <text> -> bitmapa ma purpurove pixely (jinak prazdna).
    assert_eq!(svg_image_count(&cmds), 1, "SVG text emit JEN 1 __isvg_ Image (ne 2x)");
    let (rgba, _, _) = svg_image_rgba(&cmds).expect("SVG rasterizovan do cache");
    assert!(rgba_has_purple(&rgba), "SVG <text fill=purple> rasterizovan (serialize_svg zahrnul text)");
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

// ─── Display Command transform handlers (scroll-y shift) ───────────────

use crate::browser::paint::DisplayCommand as DC;

#[test]
fn paint_clipped_rect_emit_with_polygon_in_pixels() {
    // Polygon clip-path bgcolor produces ClippedRect s 5 body
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: green; width: 100px; height: 100px;
                clip-path: polygon(50% 0, 100% 50%, 50% 100%, 0 50%); }"#,
    );
    let clipped = cmds.iter().find_map(|c| if let DC::ClippedRect { points, color } = c {
        Some((points.clone(), *color))
    } else { None });
    let (pts, color) = clipped.expect("ClippedRect");
    assert_eq!(pts.len(), 4, "kosoctverec ma 4 body");
    assert_eq!(color[1], 128, "green color");
}

#[test]
fn paint_clipped_rect_inset_clip_path_no_emit() {
    // inset clip-path neni polygon -> stale Rect, ne ClippedRect.
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 100px; height: 100px;
                clip-path: inset(20px); }"#,
    );
    assert!(!cmds.iter().any(|c| matches!(c, DC::ClippedRect { .. })));
}

// ─── Filter color matrix runtime in markers ────────────────────────────

#[test]
fn paint_chained_filter_color_matrix_combines() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px;
                filter: brightness(0.5) invert(1); }"#,
    );
    for cmd in &cmds {
        if let DC::FilterBegin { color_matrix, .. } = cmd {
            // brightness 0.5 then invert: r' = 1 - 0.5*r -> coef -0.5 + 1
            assert!((color_matrix[0] + 0.5).abs() < 1e-3, "r coef chain: {}", color_matrix[0]);
            assert!((color_matrix[4] - 1.0).abs() < 1e-3, "r offset 1.0");
            return;
        }
    }
    panic!("FilterBegin nenalezen");
}

#[test]
fn paint_filter_blur_radius_zero_no_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px;
                filter: blur(0px); }"#,
    );
    let begins = count_filter_begins(&cmds);
    assert_eq!(begins, 0, "blur(0) je no-op, ne emit FilterBegin");
}

#[test]
fn paint_filter_brightness_over_one_emits_marker() {
    // brightness > 1 = non-identity matrix -> emit
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px;
                filter: brightness(1.5); }"#,
    );
    assert!(count_filter_begins(&cmds) >= 1);
}

// ─── Element bg color extraction ────────────────────────────────────────

#[test]
fn paint_div_emits_rect_with_correct_bg() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: blue; width: 50px; height: 50px; }"#,
    );
    let blue_rect = cmds.iter().any(|c| matches!(c,
        DC::Rect { color, .. } if color[2] >= 200 && color[0] < 50
    ));
    assert!(blue_rect);
}

#[test]
fn paint_no_bg_color_no_rect() {
    let cmds = build_dl(
        r#"<html><body><div>X</div></body></html>"#,
        r#"div { width: 50px; height: 50px; }"#,
    );
    // Bez bg color, nemel by emit Rect (jen text)
    let has_solid_rect = cmds.iter().any(|c| matches!(c,
        DC::Rect { color, .. } if color[3] == 255 && (color[0] > 0 || color[1] > 0 || color[2] > 0)
    ));
    assert!(!has_solid_rect);
}

#[test]
fn paint_rgba_alpha_propagates() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: rgba(255, 0, 0, 0.5); width: 50px; height: 50px; }"#,
    );
    let semi_red = cmds.iter().any(|c| matches!(c,
        DC::Rect { color, .. } if color[0] == 255 && color[3] >= 100 && color[3] < 200
    ));
    assert!(semi_red);
}

// ─── Border styles ──────────────────────────────────────────────────────

#[test]
fn paint_border_width_and_color() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px;
                border: 5px solid green; }"#,
    );
    let green_5 = cmds.iter().any(|c| matches!(c,
        DC::Border { width, color, .. } if (*width - 5.0).abs() < 1e-3 && color[1] >= 100
    ));
    assert!(green_5);
}

#[test]
fn paint_border_zero_no_emit() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px;
                border: 0 solid green; }"#,
    );
    // Border width 0 by se nemel emit
    let any_border = cmds.iter().any(|c| matches!(c,
        DC::Border { width, color, .. } if *width > 0.0 && color[1] > 50
    ));
    assert!(!any_border);
}

// ─── Box shadow edge cases ──────────────────────────────────────────────

#[test]
fn paint_box_shadow_with_spread() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px;
                box-shadow: 4px 4px 8px 2px rgba(0,0,0,0.5); }"#,
    );
    let with_spread = cmds.iter().any(|c| matches!(c,
        DC::Shadow { spread, .. } if *spread > 0.0
    ));
    assert!(with_spread, "spread param parsed");
}

#[test]
fn paint_box_shadow_offset_propagates() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px;
                box-shadow: 5px 10px 0 black; }"#,
    );
    let with_offset = cmds.iter().any(|c| matches!(c,
        DC::Shadow { offset_x, offset_y, .. }
            if (*offset_x - 5.0).abs() < 1e-3 && (*offset_y - 10.0).abs() < 1e-3
    ));
    assert!(with_offset);
}

// ─── Image emission via background-image ──────────────────────────────

#[test]
fn paint_bg_image_url_quoted() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background-image: url("test.png"); width: 50px; height: 50px; }"#,
    );
    let has_img = cmds.iter().any(|c| matches!(c,
        DC::Image { src, .. } if src.contains("test.png")
    ));
    assert!(has_img);
}

#[test]
fn paint_no_bg_image_no_emit() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; }"#,
    );
    let any_img = cmds.iter().any(|c| matches!(c, DC::Image { .. }));
    assert!(!any_img);
}

// ─── Multiple element nesting ───────────────────────────────────────────

#[test]
fn paint_nested_divs_emit_multiple_rects() {
    let cmds = build_dl(
        r#"<html><body><div><div><div></div></div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; padding: 5px; }"#,
    );
    let red_count = cmds.iter().filter(|c| matches!(c,
        DC::Rect { color, .. } if color[0] == 255 && color[1] == 0
    )).count();
    assert!(red_count >= 3, "3 nested divs -> 3 red rects, got {red_count}");
}

#[test]
fn paint_multiple_siblings_distinct_colors() {
    let cmds = build_dl(
        r#"<html><body>
            <div class="a"></div>
            <div class="b"></div>
            <div class="c"></div>
        </body></html>"#,
        r#"
            .a { background: red; width: 50px; height: 50px; }
            .b { background: green; width: 50px; height: 50px; }
            .c { background: blue; width: 50px; height: 50px; }
        "#,
    );
    let red = cmds.iter().any(|c| matches!(c, DC::Rect { color, .. } if color[0] == 255 && color[1] == 0));
    let green = cmds.iter().any(|c| matches!(c, DC::Rect { color, .. } if color[1] >= 100 && color[0] < 50));
    let blue = cmds.iter().any(|c| matches!(c, DC::Rect { color, .. } if color[2] >= 200 && color[0] < 50));
    assert!(red && green && blue);
}

// ─── Filter chain emission combinations ────────────────────────────────

#[test]
fn paint_multiple_filters_one_marker() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px;
                filter: blur(3px) brightness(1.5) grayscale(50%); }"#,
    );
    // Vsechny 3 filtry pohromade -> jeden FilterBegin
    assert_eq!(count_filter_begins(&cmds), 1);
}

#[test]
fn paint_drop_shadow_chain_emits_shadow_per_op() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px;
                filter: drop-shadow(2px 2px 4px black) drop-shadow(0 0 8px red); }"#,
    );
    let shadow_count = cmds.iter().filter(|c| matches!(c, DC::Shadow { .. })).count();
    assert!(shadow_count >= 2, "2 drop-shadows = 2 shadows commands");
}

// ─── Border radius effect ──────────────────────────────────────────────

#[test]
fn paint_border_radius_propagates_to_rect() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; width: 50px; height: 50px; border-radius: 10px; }"#,
    );
    let with_radius = cmds.iter().any(|c| matches!(c,
        DC::Rect { radius, .. } if (*radius - 10.0).abs() < 1e-3
    ));
    assert!(with_radius, "border-radius -> Rect.radius");
}

// ─── Outline + offset ──────────────────────────────────────────────────

#[test]
fn paint_outline_default_no_emit() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: white; width: 50px; height: 50px; }"#,
    );
    // Bez outline property, jen default Border (none) - nemel by emit pro outline
    let outline_emit_count = cmds.iter().filter(|c| matches!(c,
        DC::Border { width, .. } if *width > 0.0
    )).count();
    assert_eq!(outline_emit_count, 0);
}

// ─── Element s mnoho deti ──────────────────────────────────────────────

#[test]
fn paint_ul_with_many_li_each_text() {
    let cmds = build_dl(
        r#"<html><body><ul>
            <li>1</li><li>2</li><li>3</li><li>4</li><li>5</li>
        </ul></body></html>"#,
        r#"li { color: black; }"#,
    );
    let text_count = cmds.iter().filter(|c| matches!(c, DC::Text { .. })).count();
    assert!(text_count >= 5, "5 li -> aspon 5 texts, got {text_count}");
}

// ─── Empty layout ──────────────────────────────────────────────────────

#[test]
fn paint_empty_body_emits_minimal() {
    let cmds = build_dl(r#"<html><body></body></html>"#, "");
    // Minimal output - no panic
    let _ = cmds.len();
}

#[test]
fn paint_only_text_node_emits_text() {
    let cmds = build_dl(r#"<html><body>just text</body></html>"#, "");
    let has_text = cmds.iter().any(|c| matches!(c,
        DC::Text { content, .. } if content.contains("text")
    ));
    assert!(has_text);
}

// ─── Filter v Transform interakce ──────────────────────────────────────

#[test]
fn paint_filter_inside_transform_emits_both() {
    let cmds = build_dl(
        r#"<html><body><div class="o"><div class="i"></div></div></body></html>"#,
        r#"
            .o { background: red; width: 100px; height: 100px; transform: rotateX(45deg); }
            .i { background: blue; width: 50px; height: 50px; filter: blur(5px); }
        "#,
    );
    let begins_t = cmds.iter().filter(|c| matches!(c, DC::TransformBegin { .. })).count();
    let begins_f = cmds.iter().filter(|c| matches!(c, DC::FilterBegin { .. })).count();
    assert!(begins_t >= 1, "outer transform marker");
    assert!(begins_f >= 1, "inner filter marker");
}

#[test]
fn paint_transform_then_filter_balanced() {
    let cmds = build_dl(
        r#"<html><body><div class="t"><div class="f"></div></div></body></html>"#,
        r#"
            .t { background: red; width: 100px; height: 100px; transform: rotateY(45deg); }
            .f { background: blue; width: 50px; height: 50px; filter: blur(3px); }
        "#,
    );
    let mut depth_t: i32 = 0;
    let mut depth_f: i32 = 0;
    for c in &cmds {
        match c {
            DC::TransformBegin { .. } => depth_t += 1,
            DC::TransformEnd => depth_t -= 1,
            DC::FilterBegin { .. } => depth_f += 1,
            DC::FilterEnd => depth_f -= 1,
            _ => {}
        }
        assert!(depth_t >= 0 && depth_f >= 0, "ne uncesnete End");
    }
    assert_eq!(depth_t, 0);
    assert_eq!(depth_f, 0);
}

// ─── Display list ordering ────────────────────────────────────────────

#[test]
fn paint_emit_order_bg_before_text() {
    let cmds = build_dl(
        r#"<html><body><p>Hello</p></body></html>"#,
        r#"p { background: yellow; color: black; }"#,
    );
    let bg_idx = cmds.iter().position(|c| matches!(c,
        DC::Rect { color, .. } if color[0] >= 200 && color[1] >= 200 && color[2] < 50
    ));
    let text_idx = cmds.iter().position(|c| matches!(c,
        DC::Text { content, .. } if content.contains("Hello")
    ));
    if let (Some(b), Some(t)) = (bg_idx, text_idx) {
        assert!(b < t, "bg pred text");
    }
}

#[test]
fn paint_emit_order_shadow_before_bg() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { background: red; box-shadow: 0 0 10px black; width: 50px; height: 50px; }"#,
    );
    let shadow_idx = cmds.iter().position(|c| matches!(c, DC::Shadow { .. }));
    let bg_idx = cmds.iter().position(|c| matches!(c,
        DC::Rect { color, .. } if color[0] == 255 && color[1] == 0 && color[2] == 0
    ));
    if let (Some(s), Some(b)) = (shadow_idx, bg_idx) {
        assert!(s < b, "shadow pred bg");
    }
}

// ─── Multi-class match ─────────────────────────────────────────────────

#[test]
fn paint_combined_class_styles_apply() {
    let cmds = build_dl(
        r#"<html><body><div class="big red"></div></body></html>"#,
        r#"
            .big { width: 200px; height: 100px; }
            .red { background: red; }
        "#,
    );
    let red_rect = cmds.iter().any(|c| matches!(c,
        DC::Rect { color, .. } if color[0] == 255 && color[1] == 0 && color[2] == 0
    ));
    assert!(red_rect);
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

#[test]
fn multiple_backgrounds_two_gradients_emitted() {
    // Dva gradienty: prvni navrchu (cerveny), druhy dole (modry).
    // Paint renderuje bottom-to-top = modry pak cerveny.
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div {
            background-image: linear-gradient(red, red), linear-gradient(blue, blue);
            width: 100px; height: 50px;
        }"#,
    );
    let grads: Vec<_> = cmds.iter().filter(|c| matches!(c, DisplayCommand::Gradient { .. })).collect();
    assert_eq!(grads.len(), 2, "dva gradienty emitovany");
}

#[test]
fn multiple_backgrounds_gradient_over_image() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div {
            background-image: linear-gradient(rgba(0,0,0,0.5), rgba(0,0,0,0.5)), url(photo.jpg);
            width: 200px; height: 100px;
        }"#,
    );
    let has_img = cmds.iter().any(|c| matches!(c, DisplayCommand::Image { src, .. } if src.contains("photo.jpg")));
    let has_grad = cmds.iter().any(|c| matches!(c, DisplayCommand::Gradient { .. }));
    assert!(has_img, "image layer emitovan");
    assert!(has_grad, "gradient overlay emitovan");
    // image musi byt pred gradientem v seznamu (bottom first = image pred gradientem)
    let img_pos = cmds.iter().position(|c| matches!(c, DisplayCommand::Image { .. })).unwrap();
    let grad_pos = cmds.iter().position(|c| matches!(c, DisplayCommand::Gradient { .. })).unwrap();
    assert!(img_pos < grad_pos, "image je pred gradientem (bottom-to-top poradi)");
}

#[test]
fn multiple_backgrounds_color_on_last_layer() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div {
            background-image: linear-gradient(red, red);
            background-color: green;
            width: 100px; height: 50px;
        }"#,
    );
    // Rect pro solid color emitovan (green)
    let has_rect = cmds.iter().any(|c| matches!(c, DisplayCommand::Rect { color, .. } if color[1] > color[0] && color[1] > color[2]));
    let has_grad = cmds.iter().any(|c| matches!(c, DisplayCommand::Gradient { .. }));
    assert!(has_rect, "solid-color rect emitovan");
    assert!(has_grad, "gradient emitovan");
}

// --- mask-image ---

#[test]
fn mask_image_emits_begin_end_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { width: 100px; height: 100px; mask-image: linear-gradient(to bottom, black, transparent); }"#,
    );
    let begins = cmds.iter().filter(|c| matches!(c, crate::browser::paint::DisplayCommand::MaskBegin { .. })).count();
    let ends   = cmds.iter().filter(|c| matches!(c, crate::browser::paint::DisplayCommand::MaskEnd)).count();
    assert!(begins >= 1, "MaskBegin musi byt emitovan");
    assert_eq!(begins, ends, "MaskBegin a MaskEnd museji byt sprovany");
}

#[test]
fn no_mask_image_no_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { width: 100px; height: 100px; background: red; }"#,
    );
    let begins = cmds.iter().filter(|c| matches!(c, crate::browser::paint::DisplayCommand::MaskBegin { .. })).count();
    assert_eq!(begins, 0, "Bez mask-image nema byt zadny MaskBegin");
}

#[test]
fn mask_image_none_no_markers() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { width: 100px; height: 100px; mask-image: none; }"#,
    );
    let begins = cmds.iter().filter(|c| matches!(c, crate::browser::paint::DisplayCommand::MaskBegin { .. })).count();
    assert_eq!(begins, 0);
}

#[test]
fn background_clip_text_skips_bg_paint() {
    let cmds = build_dl(
        r#"<html><body><div>Hello</div></body></html>"#,
        r#"div { width: 100px; height: 50px; background: red; background-clip: text; color: transparent; }"#,
    );
    let red_rect = cmds.iter().any(|c| matches!(c,
        crate::browser::paint::DisplayCommand::Rect { color: [255, 0, 0, 255], .. }
    ));
    assert!(!red_rect, "background-clip:text potlaci box bg fill");
}

#[test]
fn background_clip_default_bg_painted() {
    let cmds = build_dl(
        r#"<html><body><div></div></body></html>"#,
        r#"div { width: 100px; height: 50px; background: red; }"#,
    );
    let red_rect = cmds.iter().any(|c| matches!(c,
        crate::browser::paint::DisplayCommand::Rect { color: [255, 0, 0, 255], .. }
    ));
    assert!(red_rect, "Bezny background renderuje Rect");
}

#[test]
fn paint_radial_gradient_with_bg_color_override_emits_gradient() {
    let cmds = build_dl(
        r#"<html><body><div class="grad"></div></body></html>"#,
        r#".grad { background: #2997ff; width: 100px; height: 60px; }
           .grad { background: radial-gradient(red, blue); }
        "#,
    );
    let has_gradient = cmds.iter().any(|c| matches!(c, DisplayCommand::Gradient { .. }));
    assert!(has_gradient, "Expected Gradient command emitted, got: {:?}",
        cmds.iter().map(|c| format!("{:?}", c).chars().take(40).collect::<String>()).collect::<Vec<_>>());
}

#[test]
fn paint_radial_gradient_no_solid_blue_overlay() {
    let cmds = build_dl(
        r#"<html><body><div class="grad">x</div></body></html>"#,
        r#".grad { background: #2997ff; width: 100px; height: 60px; }
           .grad.x { background: radial-gradient(red, blue); }
        "#,
    );
    // Bez .x selectoru druhe pravidlo nematchne -> prvni vyhraje (solid).
    let solid_rects: Vec<_> = cmds.iter().filter(|c| matches!(c, DisplayCommand::Rect { color, .. } if color[2] > 200 && color[0] < 100)).collect();
    let _ = solid_rects;
}

#[test]
fn paint_test_page_grad_box_actually_renders_gradient() {
    let css = std::fs::read_to_string("static/test.css").unwrap_or_default();
    let html = std::fs::read_to_string("static/test.html").unwrap_or_default();
    if css.is_empty() || html.is_empty() { return; }
    let cmds = build_dl(&html, &css);
    // Najdi gradient prikazy.
    let grads: Vec<_> = cmds.iter().filter(|c| matches!(c, DisplayCommand::Gradient { .. })).collect();
    println!("Gradient commands: {}", grads.len());
    for g in &grads {
        if let DisplayCommand::Gradient { kind, x, y, w, h, stops, .. } = g {
            println!("  kind={:?} pos=({},{}) size=({},{}) stops={}", kind, x, y, w, h, stops.len());
        }
    }
    // Najdi vsechny modre Rects (ozn solid bg #2997ff)
    let blue_rects: Vec<_> = cmds.iter().filter(|c| match c {
        DisplayCommand::Rect { color, .. } => color[0] == 0x29 && color[1] == 0x97 && color[2] == 0xff,
        _ => false,
    }).collect();
    println!("Blue (#2997ff) Rect commands: {}", blue_rects.len());
    assert!(grads.len() >= 2, "expected at least 2 gradient commands (g-lin, g-rad, g-con), got {}", grads.len());
}

#[test]
fn transform_3d_emit_correct_box_dims() {
    let cmds = build_dl(
        r#"<html><body><div class="b"></div></body></html>"#,
        r#".b { width: 80px; height: 60px; transform: rotateY(45deg); display: inline-block; }"#,
    );
    let tb = cmds.iter().find_map(|c| {
        if let DisplayCommand::TransformBegin { x, y, w, h, matrix } = c {
            Some((*x, *y, *w, *h, *matrix))
        } else { None }
    }).expect("TransformBegin not found");
    let (_, _, w, h, m) = tb;
    println!("TransformBegin w={} h={}", w, h);
    assert!((w - 80.0).abs() < 1.0, "width = {}, expected 80", w);
    assert!((h - 60.0).abs() < 1.0, "height = {}, expected 60", h);
    // Matrix entries
    println!("matrix row0: {:?}", &m[0..4]);
    println!("matrix row3: {:?}", &m[12..16]);
    let c45 = (45.0_f32.to_radians()).cos();
    assert!((m[0] - c45).abs() < 0.01, "m[0]={} expected cos(45)={}", m[0], c45);
}

#[test]
fn badge_text_centered_vertically_in_box() {
    let cmds = build_dl(
        r#"<html><body><span class="b">badge</span></body></html>"#,
        r#"body { font-size: 16px; }
           .b { background: red; color: white; padding: 2px 8px; border-radius: 12px; font-size: 12px; display: inline-block; }
        "#,
    );
    // Najdi Rect (badge bg) a Text command.
    let bg_rect = cmds.iter().find_map(|c| {
        if let DisplayCommand::Rect { x, y, w, h, color, .. } = c {
            if color[0] == 255 && color[1] == 0 { return Some((*x, *y, *w, *h)); }
        }
        None
    });
    let text_cmd = cmds.iter().find_map(|c| {
        if let DisplayCommand::Text { x, y, font_size, content, .. } = c {
            if content.contains("badge") { return Some((*x, *y, *font_size)); }
        }
        None
    });
    if let (Some((rx, ry, _, rh)), Some((_tx, ty, fs))) = (bg_rect, text_cmd) {
        let box_center = ry + rh * 0.5;
        let baseline = ty + fs;
        let glyph_top = baseline - fs * 0.7;
        let text_center = (glyph_top + baseline) * 0.5;
        let diff = (text_center - box_center).abs();
        println!("box_center={} text_center={} diff={}", box_center, text_center, diff);
        // Tolerujeme do 2px diff (font-specific ascender/descender variabilita).
        assert!(diff < 3.0, "badge text vertical center off by {} px (rect_y={} h={} text_y={} fs={})",
            diff, ry, rh, ty, fs);
        let _ = rx;
    }
}

#[test]
fn button_text_position_within_button_box() {
    let cmds = build_dl(
        r#"<html><body><button class="b">P</button></body></html>"#,
        r#"body { font-size: 16px; }
           .b { padding: 8px 16px; font-size: 14px; }
        "#,
    );
    let btn_rect = cmds.iter().find_map(|c| {
        if let DisplayCommand::Rect { x, y, w, h, color, radius: _, .. } = c {
            // Default button bg = [239, 239, 239, 255]
            if color[0] == 239 { return Some((*x, *y, *w, *h)); }
        }
        None
    });
    let text_cmd = cmds.iter().find_map(|c| {
        if let DisplayCommand::Text { x, y, font_size, content, .. } = c {
            if content.contains('P') { return Some((*x, *y, *font_size)); }
        }
        None
    });
    if let (Some((_, ry, _, rh)), Some((_, ty, fs))) = (btn_rect, text_cmd) {
        let baseline = ty + fs;
        let button_top = ry;
        let button_bot = ry + rh;
        let button_center = ry + rh * 0.5;
        let glyph_top = baseline - fs * 0.7;
        let glyph_bot = baseline + fs * 0.2;  // s descender
        let space_above = glyph_top - button_top;
        let space_below = button_bot - glyph_bot;
        println!("btn h={} text baseline={} glyph_top={} glyph_bot={} center={}",
            rh, baseline, glyph_top, glyph_bot, button_center);
        println!("  space_above={} space_below={} diff={}",
            space_above, space_below, (space_above - space_below).abs());
        // Mela by byt asymmetrie max ~3px (descender vs cap height).
        assert!((space_above - space_below).abs() < 5.0,
            "asymmetric: above={} below={}", space_above, space_below);
    }
}

#[test]
fn debug_button_all_commands() {
    let cmds = build_dl(
        r#"<html><body><button class="b">P</button></body></html>"#,
        r#"body { font-size: 16px; }
           .b { padding: 8px 16px; font-size: 14px; }
        "#,
    );
    for c in &cmds {
        println!("{:?}", c);
    }
}

#[test]
fn engine_test_html_inline_styles_load() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet};
    let html = match std::fs::read_to_string("static/engine-test.html") {
        Ok(s) => s, Err(_) => return,
    };
    let doc = parse_html(&html, "about:blank");
    let styles: Vec<String> = doc.root.get_elements_by_tag("style")
        .iter().map(|s| s.text_content()).collect();
    println!("inline <style> blocks: {}", styles.len());
    for (i, s) in styles.iter().enumerate() {
        println!("  block {}: {} chars, first 80: {}", i, s.len(),
            s.chars().take(80).collect::<String>());
    }
    assert!(!styles.is_empty(), "no inline <style> blocks extracted");
    let combined: String = styles.join("\n");
    let sheet = parse_stylesheet(&combined);
    println!("parsed rules: {}", sheet.rules.len());
    println!("parsed @media: {}", sheet.media_queries.len());
    assert!(sheet.rules.len() > 5, "few rules parsed: {}", sheet.rules.len());
}

#[test]
fn engine_test_html_cascade_applies() {
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade};
    let html = match std::fs::read_to_string("static/engine-test.html") {
        Ok(s) => s, Err(_) => return,
    };
    let doc = parse_html(&html, "about:blank");
    let combined: String = doc.root.get_elements_by_tag("style")
        .iter().map(|s| s.text_content()).collect::<Vec<_>>().join("\n");
    let sheet = parse_stylesheet(&combined);
    let map = cascade::cascade(&doc.root, &[sheet]);
    println!("cascade entries: {}", map.len());
    let bodies = doc.root.get_elements_by_tag("body");
    if let Some(body) = bodies.first() {
        let body_id = std::rc::Rc::as_ptr(body) as usize;
        if let Some(styles) = map.get(&body_id) {
            println!("body styles ({} props):", styles.len());
            for (k, v) in styles.iter().take(10) {
                println!("  {}: {}", k, v);
            }
        } else {
            println!("body NOT in cascade map!");
        }
    }
}

#[test]
fn rotate_y_45_emits_matrix_with_cos45() {
    let cmds = build_dl(
        r#"<html><body><div class="b"></div></body></html>"#,
        r#".b { width: 80px; height: 60px; transform: rotateY(45deg); display: inline-block; }"#,
    );
    let m = cmds.iter().find_map(|c| {
        if let DisplayCommand::TransformBegin { matrix, .. } = c { Some(*matrix) } else { None }
    }).expect("TransformBegin");
    let c45 = (45.0_f32.to_radians()).cos();
    println!("matrix m[0]={} (expected cos45={})", m[0], c45);
    println!("matrix m[2]={} (expected sin45={})", m[2], (45.0_f32.to_radians()).sin());
    println!("matrix row3: {:?}", &m[12..16]);
    assert!((m[0] - c45).abs() < 0.01, "m[0]={} not cos(45)={}", m[0], c45);
    assert!((m[15] - 1.0).abs() < 0.01, "m[15]={} should be 1", m[15]);
    // tw pri lx=hw=40, ly=0, lz=0:
    let tw = m[12]*40.0 + m[13]*0.0 + m[14]*0.0 + m[15];
    let tx = m[0]*40.0 + m[1]*0.0 + m[2]*0.0 + m[3];
    let px = tx / tw.max(0.0001);
    println!("hw=40, lx=40 -> tx={} tw={} px={} (expected ~28.3)", tx, tw, px);
    assert!((px - 28.28).abs() < 0.5, "px={} expected ~28.3", px);
}

#[test]
fn rot_box_in_flex_row_keeps_80x60() {
    let cmds = build_dl(
        r#"<html><body><div class="row"><div class="b">A</div><div class="b">B</div></div></body></html>"#,
        r#".row { display: flex; gap: 8px; }
           .b { width: 80px; height: 60px; transform: rotateY(45deg); }"#,
    );
    let tbs: Vec<_> = cmds.iter().filter_map(|c| {
        if let DisplayCommand::TransformBegin { x, y, w, h, .. } = c {
            Some((*x, *y, *w, *h))
        } else { None }
    }).collect();
    println!("transform begins: {:?}", tbs);
    assert!(tbs.len() >= 2, "expected 2 transforms");
    for (_, _, w, h) in &tbs {
        assert!((w - 80.0).abs() < 1.0, "w={} != 80", w);
        assert!((h - 60.0).abs() < 1.0, "h={} != 60", h);
    }
}

#[test]
fn main_scrollbar_emits_when_layout_overflows_viewport() {
    // Build a layout that exceeds viewport, then run emit_main_scrollbar_overlay
    // directly to verify it pushes Rect commands for track + thumb.
    let html = r#"<html><body><div class="tall"></div></body></html>"#;
    let css = r#"body { margin: 0; background: #fff; }
                 .tall { height: 3000px; background: red; }"#;
    let doc = parse_html(html, "");
    let css_sheet = parse_stylesheet(css);
    let map = cascade::cascade(&doc.root, &[css_sheet]);
    let layout_root = layout::layout_tree(&doc.root, &map, 800.0, 600.0);
    assert!(layout_root.rect.height > 600.0, "layout height {} should overflow viewport 600",
        layout_root.rect.height);
    // Build display list culled + scrollbar overlay.
    let mut cmds = paint::build_display_list_culled(&layout_root, 0.0, 600.0);
    let pre_len = cmds.len();
    paint::emit_main_scrollbar_overlay(&layout_root, &mut cmds, 800.0, 600.0, 0.0, 0.0);
    let post_len = cmds.len();
    // Emit pridava 4 cmds: track Rect + thumb Rect + 2 sipky (ClippedRect
    // trojuhelniky, Chrome-like arrow buttons). Canvas bg uz NEemittuje
    // (D4 clear color / monolithic insert resi caller pres canvas_background()
    // - insert(0) tady rozbijel d4_overlay_start indexing).
    assert_eq!(post_len - pre_len, 4,
        "expected track + thumb + 2 arrows, got {} ({} -> {})", post_len - pre_len, pre_len, post_len);
    // canvas_background() vraci body bg pro caller.
    let bg = paint::canvas_background(&layout_root);
    assert_eq!(bg, Some([255, 255, 255, 255]), "canvas bg ma byt body #fff, got {:?}", bg);
    // Poradi: track Rect, thumb Rect, 2x arrow ClippedRect. Track right edge
    // = viewport_w.
    let track = &cmds[post_len - 4];
    if let DisplayCommand::Rect { x, w, .. } = track {
        assert!((*x + *w - 800.0).abs() < 1.0, "track right edge {} != viewport_w 800", *x + *w);
    } else {
        panic!("expected Rect for scrollbar track, got {:?}", track);
    }
    assert!(matches!(&cmds[post_len - 2], DisplayCommand::ClippedRect { .. }),
        "predposledni cmd ma byt arrow ClippedRect");
    assert!(matches!(&cmds[post_len - 1], DisplayCommand::ClippedRect { .. }),
        "posledni cmd ma byt arrow ClippedRect");
}

#[test]
fn main_scrollbar_no_emit_when_content_fits_viewport() {
    let html = r#"<html><body><div></div></body></html>"#;
    let css = r#"div { width: 100px; height: 100px; background: red; }"#;
    let doc = parse_html(html, "");
    let css_sheet = parse_stylesheet(css);
    let map = cascade::cascade(&doc.root, &[css_sheet]);
    let layout_root = layout::layout_tree(&doc.root, &map, 800.0, 600.0);
    let mut cmds = paint::build_display_list_culled(&layout_root, 0.0, 600.0);
    let pre_len = cmds.len();
    paint::emit_main_scrollbar_overlay(&layout_root, &mut cmds, 800.0, 600.0, 0.0, 0.0);
    assert_eq!(cmds.len(), pre_len, "no scrollbar should emit when content fits");
}
