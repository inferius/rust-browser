//! Repro testy pro gradienty ze static/engine-test.html sekce 07
//! (radial circle/ellipse, conic quadrant, repeating-radial = cerne boxy).

use super::parse_any_gradient;

#[test]
fn radial_circle_at_pos_with_transparent() {
    // .g-radial-1
    let g = parse_any_gradient("radial-gradient(circle at 30% 30%, #e8ff6a, transparent)")
        .expect("radial circle at parse");
    assert!(g.stops.len() >= 2, "stops: {:?}", g.stops);
    assert_eq!(g.stops[0].1, [232, 255, 106, 255]);
    assert_eq!(g.stops[1].1[3], 0, "transparent stop ma alpha 0: {:?}", g.stops[1]);
}

#[test]
fn radial_ellipse_at_center_pct_stops() {
    // .g-radial-2
    let g = parse_any_gradient("radial-gradient(ellipse at center, #a06aff 0%, transparent 70%)")
        .expect("radial ellipse parse");
    assert!(g.stops.len() >= 2, "stops: {:?}", g.stops);
    assert_eq!(g.stops[0].1, [160, 106, 255, 255]);
}

#[test]
fn conic_quadrant_deg_double_position() {
    // .g-conic-2 (po var() substituci)
    let g = parse_any_gradient("conic-gradient(#e8ff6a 90deg, #1a1a1f 90deg 180deg, #6affdb 180deg 270deg, #1a1a1f 270deg)")
        .expect("conic quadrant parse");
    assert!(g.stops.len() >= 4, "stops: {:?}", g.stops);
}

#[test]
fn conic_quadrant_paint_emits_multi_stop_gradient() {
    // Cely pipeline: HTML+CSS -> cascade -> layout -> paint. Conic quadrant
    // musi emitnout Gradient command s Conic kind + >= 4 stops (vsechny deg
    // pozice), ne 2-stop degradaci.
    use crate::browser::{html_parser::parse_html, css_parser::parse_stylesheet, cascade, layout, paint};
    let doc = parse_html(
        r#"<html><body><div class="q">x</div></body></html>"#, "");
    let css = parse_stylesheet(
        r#"body { margin: 0; }
           .q { width: 148px; height: 80px;
                background: conic-gradient(#e8ff6a 90deg, #1a1a1f 90deg 180deg, #6affdb 180deg 270deg, #1a1a1f 270deg); }"#);
    let map = cascade::cascade(&doc.root, &[css]);
    let lr = layout::layout_tree(&doc.root, &map, 400.0, 300.0);
    let cmds = paint::build_display_list(&lr);
    let grads: Vec<_> = cmds.iter().filter_map(|c| match c {
        paint::DisplayCommand::Gradient { kind, stops, .. } => Some((kind.clone(), stops.clone())),
        _ => None,
    }).collect();
    println!("gradient cmds: {:?}", grads);
    assert_eq!(grads.len(), 1, "presne 1 Gradient command, ne {}", grads.len());
    let (kind, stops) = &grads[0];
    assert!(matches!(kind, paint::GradientKind::Conic { .. }), "kind: {:?}", kind);
    assert!(stops.len() >= 4, "vsech 6 deg stops (4 unikatni barvy), ma {}: {:?}", stops.len(), stops);
}

#[test]
fn repeating_radial_px_stops() {
    // .g-repeating-r (po var() substituci)
    let g = parse_any_gradient("repeating-radial-gradient(circle, rgba(160,106,255,0.15) 0px, rgba(160,106,255,0.15) 4px, transparent 4px, transparent 12px)")
        .expect("repeating radial px parse");
    assert!(!g.stops.is_empty(), "stops: {:?}", g.stops);
}
