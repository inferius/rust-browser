/// Testy pro display list segmentaci a render helpers.
/// Plne wgpu render se neda unit testit (potrebuje GPU device), ale partition
/// filteru a paint segmentaci ano.

use crate::browser::paint::DisplayCommand;
use crate::browser::render::{partition_filter_segments, Seg, polygon_signed_area, triangulate_polygon};

fn rect(x: f32, y: f32) -> DisplayCommand {
    DisplayCommand::Rect { x, y, w: 10.0, h: 10.0, color: [255,0,0,255], radius: 0.0 }
}

fn filter_begin(blur: f32) -> DisplayCommand {
    DisplayCommand::FilterBegin {
        x: 0.0, y: 0.0, w: 100.0, h: 100.0,
        blur_radius: blur,
        color_matrix: [
            1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ],
    }
}

#[test]
fn partition_empty_returns_no_segments() {
    let cmds: Vec<DisplayCommand> = vec![];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 0);
}

#[test]
fn partition_only_main_returns_single_main() {
    let cmds = vec![rect(0.0, 0.0), rect(10.0, 10.0)];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Main(s) => assert_eq!(s.len(), 2),
        _ => panic!("expected Main"),
    }
}

#[test]
fn partition_only_filter_returns_single_filter() {
    let cmds = vec![filter_begin(5.0), rect(0.0, 0.0), DisplayCommand::FilterEnd];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Filter { inner, radius, .. } => {
            assert_eq!(inner.len(), 1);
            assert!((*radius - 5.0).abs() < 1e-5);
        }
        _ => panic!("expected Filter"),
    }
}

#[test]
fn partition_main_filter_main() {
    let cmds = vec![
        rect(0.0, 0.0),
        rect(10.0, 10.0),
        filter_begin(3.0),
        rect(20.0, 20.0),
        DisplayCommand::FilterEnd,
        rect(30.0, 30.0),
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 3);
    match &segs[0] {
        Seg::Main(s) => assert_eq!(s.len(), 2),
        _ => panic!("expected Main"),
    }
    match &segs[1] {
        Seg::Filter { inner, .. } => assert_eq!(inner.len(), 1),
        _ => panic!("expected Filter"),
    }
    match &segs[2] {
        Seg::Main(s) => assert_eq!(s.len(), 1),
        _ => panic!("expected Main"),
    }
}

#[test]
fn partition_nested_filter_treated_as_inner_cmds() {
    // Vnoreny FilterBegin v ramci top-level filter spans nezpracovava se -
    // jeho inner cmds se renderuji bez extra blur (protoze parent uz je RT-mediated).
    let cmds = vec![
        filter_begin(5.0),
        rect(0.0, 0.0),
        filter_begin(2.0),
        rect(10.0, 10.0),
        DisplayCommand::FilterEnd,
        rect(20.0, 20.0),
        DisplayCommand::FilterEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1, "vnoreny filter ma byt soucasti outer span, ne novy segment");
    match &segs[0] {
        Seg::Filter { inner, radius, .. } => {
            // inner = 5 cmds (rect, filterBegin, rect, filterEnd, rect)
            assert_eq!(inner.len(), 5);
            assert!((*radius - 5.0).abs() < 1e-5);
        }
        _ => panic!("expected Filter"),
    }
}

#[test]
fn partition_two_consecutive_filters() {
    let cmds = vec![
        filter_begin(2.0),
        rect(0.0, 0.0),
        DisplayCommand::FilterEnd,
        filter_begin(4.0),
        rect(10.0, 10.0),
        DisplayCommand::FilterEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 2);
    let mut radii = Vec::new();
    for s in &segs {
        if let Seg::Filter { radius, .. } = s {
            radii.push(*radius);
        }
    }
    assert_eq!(radii.len(), 2);
    assert!((radii[0] - 2.0).abs() < 1e-5);
    assert!((radii[1] - 4.0).abs() < 1e-5);
}

#[test]
fn partition_main_filter_back_to_back_no_main_between() {
    let cmds = vec![
        rect(0.0, 0.0),
        filter_begin(2.0),
        rect(10.0, 10.0),
        DisplayCommand::FilterEnd,
        filter_begin(3.0),
        rect(20.0, 20.0),
        DisplayCommand::FilterEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    // Main, Filter, Filter (zadny Main mezi 2 filtry)
    assert_eq!(segs.len(), 3);
}

#[test]
fn partition_unbalanced_only_begin_no_panic() {
    // Defensive: kdyz neni FilterEnd parovan, nepanikari.
    let cmds = vec![
        rect(0.0, 0.0),
        filter_begin(2.0),
        rect(10.0, 10.0),
    ];
    let segs = partition_filter_segments(&cmds);
    // FilterBegin oddeli main, ale bez FilterEnd Filter segment se neemituje
    // -> jen 1 main segment pred. Tail neni catched (cursor zustal pred).
    assert!(segs.len() <= 2);
}

#[test]
fn partition_color_matrix_passes_through() {
    let mut cmd = filter_begin(0.0);
    if let DisplayCommand::FilterBegin { color_matrix, .. } = &mut cmd {
        color_matrix[0] = 0.5;  // non-identity
    }
    let cmds = vec![cmd, rect(0.0, 0.0), DisplayCommand::FilterEnd];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Filter { color_matrix, .. } => {
            assert!((color_matrix[0] - 0.5).abs() < 1e-5);
        }
        _ => panic!("expected Filter"),
    }
}

#[test]
fn partition_filter_with_zero_blur_still_emits() {
    // Color-matrix-only filter (bez blur) - mel by emit Filter segment
    let cmds = vec![filter_begin(0.0), rect(0.0, 0.0), DisplayCommand::FilterEnd];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Filter { radius, .. } => assert_eq!(*radius, 0.0),
        _ => panic!("expected Filter"),
    }
}

fn transform_begin(matrix_marker: f32) -> DisplayCommand {
    let mut m = [0.0_f32; 16];
    // Identity + marker pro identifikaci
    m[0] = 1.0; m[5] = 1.0; m[10] = 1.0; m[15] = 1.0;
    m[3] = matrix_marker;  // tx jako marker
    DisplayCommand::TransformBegin {
        x: 0.0, y: 0.0, w: 100.0, h: 100.0,
        matrix: m,
    }
}

#[test]
fn partition_transform_only_returns_transform3d_seg() {
    let cmds = vec![transform_begin(7.0), rect(0.0, 0.0), DisplayCommand::TransformEnd];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Transform3D { inner, matrix, .. } => {
            assert_eq!(inner.len(), 1);
            assert!((matrix[3] - 7.0).abs() < 1e-5);
        }
        _ => panic!("expected Transform3D"),
    }
}

#[test]
fn partition_main_transform_main() {
    let cmds = vec![
        rect(0.0, 0.0),
        transform_begin(1.0),
        rect(10.0, 10.0),
        DisplayCommand::TransformEnd,
        rect(20.0, 20.0),
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 3);
    matches!(&segs[0], Seg::Main(_));
    matches!(&segs[1], Seg::Transform3D { .. });
    matches!(&segs[2], Seg::Main(_));
}

#[test]
fn partition_filter_inside_transform_treated_as_inner() {
    // Top-level Transform pohlti Filter inside (kvuli first-cut implementaci).
    let cmds = vec![
        transform_begin(2.0),
        filter_begin(5.0),
        rect(0.0, 0.0),
        DisplayCommand::FilterEnd,
        DisplayCommand::TransformEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Transform3D { inner, .. } => {
            // inner obsahuje FilterBegin/Rect/FilterEnd
            assert_eq!(inner.len(), 3);
        }
        _ => panic!("expected Transform3D"),
    }
}

#[test]
fn partition_transform_inside_filter_treated_as_inner() {
    let cmds = vec![
        filter_begin(5.0),
        transform_begin(2.0),
        rect(0.0, 0.0),
        DisplayCommand::TransformEnd,
        DisplayCommand::FilterEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 1);
    match &segs[0] {
        Seg::Filter { inner, .. } => {
            assert_eq!(inner.len(), 3);
        }
        _ => panic!("expected Filter"),
    }
}

#[test]
fn partition_two_consecutive_transforms() {
    let cmds = vec![
        transform_begin(1.0),
        rect(0.0, 0.0),
        DisplayCommand::TransformEnd,
        transform_begin(2.0),
        rect(10.0, 10.0),
        DisplayCommand::TransformEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 2);
    let mut markers = Vec::new();
    for s in &segs {
        if let Seg::Transform3D { matrix, .. } = s {
            markers.push(matrix[3]);
        }
    }
    assert_eq!(markers.len(), 2);
    assert!((markers[0] - 1.0).abs() < 1e-5);
    assert!((markers[1] - 2.0).abs() < 1e-5);
}

// ─── Polygon triangulation ──────────────────────────────────────────────

#[test]
fn polygon_signed_area_square_cw_screen() {
    // Square (y down): (0,0), (10,0), (10,10), (0,10) - CW screen orientation
    let pts = [(0.0_f32, 0.0_f32), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let area = polygon_signed_area(&pts);
    // Algoritmus pro screen-space (y down): CW -> kladne
    assert!(area > 0.0, "CW square has positive screen area, got {area}");
    assert!((area.abs() - 100.0).abs() < 1.0, "area magnitude ~100");
}

#[test]
fn polygon_signed_area_triangle() {
    let pts = [(0.0_f32, 0.0_f32), (10.0, 0.0), (10.0, 10.0)];
    let area = polygon_signed_area(&pts);
    assert!((area.abs() - 50.0).abs() < 1.0);
}

#[test]
fn triangulate_triangle_returns_one() {
    let pts = vec![(0.0_f32, 0.0_f32), (10.0, 0.0), (5.0, 10.0)];
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 1);
}

#[test]
fn triangulate_quad_returns_two() {
    let pts = vec![(0.0_f32, 0.0_f32), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 2, "quad -> 2 triangles");
}

#[test]
fn triangulate_pentagon_returns_three() {
    let pts = vec![
        (50.0_f32, 0.0_f32),
        (100.0, 38.0),
        (82.0, 100.0),
        (18.0, 100.0),
        (0.0, 38.0),
    ];
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 3, "pentagon -> n-2 = 3 triangles");
}

#[test]
fn triangulate_concave_arrow_correct_count() {
    // Arrow shape: 7 bodu, mel by emit 5 trojuhelniku
    let pts = vec![
        (0.0_f32, 25.0_f32),
        (60.0, 25.0),
        (60.0, 0.0),
        (100.0, 50.0),
        (60.0, 100.0),
        (60.0, 75.0),
        (0.0, 75.0),
    ];
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 5, "arrow (7 vertices) -> 5 triangles");
}

#[test]
fn triangulate_concave_l_shape() {
    // L-shape: 6 bodu, mel by emit 4 trojuhelniky
    let pts = vec![
        (0.0_f32, 0.0_f32),
        (10.0, 0.0),
        (10.0, 5.0),
        (5.0, 5.0),
        (5.0, 10.0),
        (0.0, 10.0),
    ];
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 4, "L-shape -> n-2 = 4 triangles");
}

#[test]
fn triangulate_empty_returns_empty() {
    let tris = triangulate_polygon(&[]);
    assert_eq!(tris.len(), 0);
}

#[test]
fn triangulate_two_points_returns_empty() {
    let tris = triangulate_polygon(&[(0.0, 0.0), (10.0, 0.0)]);
    assert_eq!(tris.len(), 0);
}

#[test]
fn triangulate_concave_no_overlap() {
    // Arrow polygon: trojuhelniky se nesmeji prekryvat ani vyletavat ven.
    // Test: vsechny trojuhelniky maji pozitivni area + total area = polygon area.
    let pts = vec![
        (0.0_f32, 25.0_f32),
        (60.0, 25.0),
        (60.0, 0.0),
        (100.0, 50.0),
        (60.0, 100.0),
        (60.0, 75.0),
        (0.0, 75.0),
    ];
    let tris = triangulate_polygon(&pts);
    let total_tri_area: f32 = tris.iter().map(|(a, b, c)| {
        let area = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs() * 0.5;
        area
    }).sum();
    let poly_area = polygon_signed_area(&pts).abs();
    assert!((total_tri_area - poly_area).abs() < 1.0,
        "sum trojuhelniku ({total_tri_area}) ~ polygon area ({poly_area})");
}

#[test]
fn triangulate_star_concave_count() {
    // 5-cipa hvezda = 10 bodu, n-2 = 8 trojuhelniku
    let pts: Vec<(f32, f32)> = (0..10).map(|i| {
        let angle = (i as f32) * std::f32::consts::PI / 5.0;
        let r = if i % 2 == 0 { 50.0 } else { 20.0 };
        (50.0 + r * angle.cos(), 50.0 + r * angle.sin())
    }).collect();
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 8, "star (10 vertices) -> n-2 = 8 triangles");
}

#[test]
fn partition_preserves_command_count() {
    let cmds = vec![
        rect(0.0, 0.0),
        filter_begin(2.0),
        rect(10.0, 10.0),
        rect(20.0, 20.0),
        DisplayCommand::FilterEnd,
        rect(30.0, 30.0),
        rect(40.0, 40.0),
    ];
    let segs = partition_filter_segments(&cmds);
    let mut total: usize = 0;
    for s in &segs {
        match s {
            Seg::Main(s) => total += s.len(),
            Seg::Filter { inner, .. } => total += inner.len(),
            Seg::Transform3D { inner, .. } => total += inner.len(),
        }
    }
    // 5 ne-marker cmds (3 mimo + 2 uvnitr filtru); markery se neztraceji v inner
    assert_eq!(total, 5);
}
