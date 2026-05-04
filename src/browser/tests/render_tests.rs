/// Testy pro display list segmentaci a render helpers.
/// Plne wgpu render se neda unit testit (potrebuje GPU device), ale partition
/// filteru a paint segmentaci ano.

use crate::browser::paint::DisplayCommand;
use crate::browser::render::{
    partition_filter_segments, Seg, polygon_signed_area, triangulate_polygon,
    paint_webgl_canvases, webgl_attrib_to_vertex_format, webgl_compute_stride,
    webgl_extract_pending, webgl_effective_clear, webgl_count_draws, webgl_count_clears,
    webgl_linked_program_ids, webgl_layout_has_canvas, webgl_canvas_count,
};

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

// ─── WebGL phase 3b - paint_webgl_canvases ──────────────────────────────

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::interpreter::{WebGLState, WebGLDrawCmd, WebGLAttribSlot, WebGLProgram};
use crate::browser::dom::NodeData;
use crate::browser::layout::{LayoutBox, Rect};

fn make_canvas_layout_box(node_ptr: Rc<NodeData>) -> LayoutBox {
    let mut bx = LayoutBox::new();
    bx.tag = Some("canvas".to_string());
    bx.rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 150.0 };
    bx.node = Some(node_ptr);
    bx
}

#[test]
fn paint_webgl_no_state_no_emit() {
    let node = NodeData::new_element("canvas", HashMap::new());
    let bx = make_canvas_layout_box(node);
    let states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    assert_eq!(cmds.len(), 0);
}

#[test]
fn paint_webgl_clear_emits_rect() {
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);

    assert_eq!(cmds.len(), 1);
    if let DisplayCommand::Rect { color, w, h, .. } = &cmds[0] {
        assert_eq!(*color, [255, 0, 0, 255], "red clear color");
        assert!((*w - 300.0).abs() < 1e-3);
        assert!((*h - 150.0).abs() < 1e-3);
    } else {
        panic!("expected Rect");
    }
}

#[test]
fn paint_webgl_clear_drains_queue() {
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([0.5, 0.5, 0.5, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let state_rc = Rc::new(RefCell::new(state));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::clone(&state_rc));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    // Po paint by mela byt queue prazdna (drained)
    assert_eq!(state_rc.borrow().draw_queue.len(), 0);
}

#[test]
fn paint_webgl_no_clear_color_no_emit() {
    // ClearColor bez Clear -> nic
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0]));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    assert_eq!(cmds.len(), 0, "ClearColor sam nestaci, treba Clear bit");
}

#[test]
fn paint_webgl_clear_without_color_uses_state_default() {
    // Clear bez ClearColor -> pouzije se default state.clear_color (0,0,0,1).
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    assert_eq!(cmds.len(), 1);
    if let DisplayCommand::Rect { color, .. } = &cmds[0] {
        assert_eq!(*color, [0, 0, 0, 255], "default clear = black");
    }
}

#[test]
fn paint_webgl_last_clear_color_wins() {
    // Vice ClearColor + Clear -> pouzije se posledni ClearColor.
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::ClearColor([0.0, 1.0, 0.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    if let DisplayCommand::Rect { color, .. } = &cmds[0] {
        assert_eq!(*color, [0, 255, 0, 255], "green wins");
    }
}

#[test]
fn paint_webgl_skips_non_canvas_tag() {
    let node = NodeData::new_element("div", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let mut bx = LayoutBox::new();
    bx.tag = Some("div".into());
    bx.rect = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
    bx.node = Some(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    assert_eq!(cmds.len(), 0, "non-canvas tag se preskakuje");
}

#[test]
fn paint_webgl_draw_arrays_emits_stripe_overlay() {
    // DrawArrays bez clear -> emit indikator stripes (placeholder phase 3c).
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::DrawArrays {
        program_id: Some(1), mode: 4, first: 0, count: 3,
        attribs: Vec::new(),
        uniforms: HashMap::new(),
        viewport: [0, 0, 300, 150],
    });
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    assert!(cmds.len() >= 1, "DrawArrays emit aspon 1 stripe overlay rect");
}

#[test]
fn paint_webgl_clear_plus_draw_combines() {
    // Clear + DrawArrays -> bg rect + stripe overlay
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(Rc::clone(&node));
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([0.5, 0.0, 0.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    state.draw_queue.push(WebGLDrawCmd::DrawArrays {
        program_id: Some(1), mode: 4, first: 0, count: 3,
        attribs: Vec::new(),
        uniforms: HashMap::new(),
        viewport: [0, 0, 300, 150],
    });
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&bx, &states, &mut cmds);
    // Min 2 cmds: bg + overlay
    assert!(cmds.len() >= 2);
    // Prvni je clear color
    if let DisplayCommand::Rect { color, .. } = &cmds[0] {
        assert_eq!(color[0], 127, "red bg ~ 0.5*255");
    }
}

// ─── WebGL phase 3c5 pure-logic helpers ────────────────────────────────

fn make_drawarrays(count: i32) -> WebGLDrawCmd {
    WebGLDrawCmd::DrawArrays {
        program_id: Some(1), mode: 4, first: 0, count,
        attribs: Vec::new(),
        uniforms: HashMap::new(),
        viewport: [0, 0, 100, 100],
    }
}

fn make_drawelements() -> WebGLDrawCmd {
    WebGLDrawCmd::DrawElements {
        program_id: Some(1), mode: 4, count: 6,
        index_type: 0x1403, offset: 0,
        index_buffer_id: Some(1),
        attribs: Vec::new(),
        uniforms: HashMap::new(),
        viewport: [0, 0, 100, 100],
    }
}

#[test]
fn extract_pending_drains_queue() {
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let frame = webgl_extract_pending(&mut state);
    assert_eq!(frame.commands.len(), 2);
    assert_eq!(state.draw_queue.len(), 0, "queue drained");
}

#[test]
fn extract_pending_empty_returns_empty() {
    let mut state = WebGLState::new();
    let frame = webgl_extract_pending(&mut state);
    assert_eq!(frame.commands.len(), 0);
    assert_eq!(frame.buffers.len(), 0);
    assert_eq!(frame.programs.len(), 0);
}

#[test]
fn extract_pending_clones_buffers() {
    let mut state = WebGLState::new();
    state.buffers.insert(1, vec![1, 2, 3, 4]);
    state.buffers.insert(2, vec![5, 6]);
    let frame = webgl_extract_pending(&mut state);
    assert_eq!(frame.buffers.len(), 2);
    assert_eq!(frame.buffers.get(&1), Some(&vec![1, 2, 3, 4]));
    assert_eq!(state.buffers.len(), 2, "state buffers preserved");
}

#[test]
fn extract_pending_clones_programs_wgsl() {
    let mut state = WebGLState::new();
    state.programs.insert(1, WebGLProgram {
        vertex_shader: Some(2), fragment_shader: Some(3), linked: true,
        info_log: String::new(),
        vertex_wgsl: Some("@vertex fn main() {}".into()),
        fragment_wgsl: Some("@fragment fn main() {}".into()),
    });
    let frame = webgl_extract_pending(&mut state);
    assert_eq!(frame.programs.len(), 1);
    let (vs, fs) = frame.programs.get(&1).unwrap();
    assert!(vs.as_ref().unwrap().contains("@vertex"));
    assert!(fs.as_ref().unwrap().contains("@fragment"));
}

#[test]
fn extract_pending_default_clear() {
    let mut state = WebGLState::new();
    state.clear_color = [0.5, 0.25, 0.75, 1.0];
    let frame = webgl_extract_pending(&mut state);
    assert_eq!(frame.default_clear, [0.5, 0.25, 0.75, 1.0]);
}

#[test]
fn effective_clear_empty_none() {
    let cmds: Vec<WebGLDrawCmd> = Vec::new();
    assert!(webgl_effective_clear(&cmds, [0.0, 0.0, 0.0, 1.0]).is_none());
}

#[test]
fn effective_clear_clear_only_uses_default() {
    let cmds = vec![WebGLDrawCmd::Clear(0x4000)];
    let r = webgl_effective_clear(&cmds, [0.1, 0.2, 0.3, 1.0]);
    assert_eq!(r, Some([0.1, 0.2, 0.3, 1.0]));
}

#[test]
fn effective_clear_color_then_clear_uses_color() {
    let cmds = vec![
        WebGLDrawCmd::ClearColor([0.7, 0.8, 0.9, 1.0]),
        WebGLDrawCmd::Clear(0x4000),
    ];
    let r = webgl_effective_clear(&cmds, [0.0; 4]);
    assert_eq!(r, Some([0.7, 0.8, 0.9, 1.0]));
}

#[test]
fn effective_clear_last_color_wins() {
    let cmds = vec![
        WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0]),
        WebGLDrawCmd::ClearColor([0.0, 1.0, 0.0, 1.0]),
        WebGLDrawCmd::Clear(0x4000),
    ];
    let r = webgl_effective_clear(&cmds, [0.0; 4]);
    assert_eq!(r, Some([0.0, 1.0, 0.0, 1.0]));
}

#[test]
fn effective_clear_color_without_clear_returns_none() {
    let cmds = vec![WebGLDrawCmd::ClearColor([1.0, 0.0, 0.0, 1.0])];
    assert!(webgl_effective_clear(&cmds, [0.0; 4]).is_none());
}

#[test]
fn effective_clear_depth_bit_only_no_color() {
    // Clear s jen DEPTH_BUFFER_BIT (0x100) bez COLOR (0x4000) -> None
    let cmds = vec![WebGLDrawCmd::Clear(0x100)];
    assert!(webgl_effective_clear(&cmds, [0.0; 4]).is_none());
}

#[test]
fn effective_clear_combined_bits() {
    // COLOR | DEPTH = 0x4100
    let cmds = vec![WebGLDrawCmd::Clear(0x4100)];
    let r = webgl_effective_clear(&cmds, [0.5, 0.5, 0.5, 1.0]);
    assert_eq!(r, Some([0.5, 0.5, 0.5, 1.0]));
}

#[test]
fn count_draws_only_drawarrays() {
    let cmds = vec![make_drawarrays(3), make_drawarrays(6), make_drawarrays(9)];
    assert_eq!(webgl_count_draws(&cmds), 3);
}

#[test]
fn count_draws_mixed_with_clears() {
    let cmds = vec![
        WebGLDrawCmd::ClearColor([0.0; 4]),
        WebGLDrawCmd::Clear(0x4000),
        make_drawarrays(3),
        WebGLDrawCmd::Clear(0x4000),
        make_drawelements(),
    ];
    assert_eq!(webgl_count_draws(&cmds), 2);
    assert_eq!(webgl_count_clears(&cmds), 2);
}

#[test]
fn count_draws_empty() {
    assert_eq!(webgl_count_draws(&[]), 0);
    assert_eq!(webgl_count_clears(&[]), 0);
}

#[test]
fn count_clears_ignores_clear_color() {
    let cmds = vec![
        WebGLDrawCmd::ClearColor([0.0; 4]),
        WebGLDrawCmd::ClearColor([0.0; 4]),
        WebGLDrawCmd::Clear(0x4000),
    ];
    assert_eq!(webgl_count_clears(&cmds), 1, "ClearColor neni Clear");
}

#[test]
fn linked_program_ids_only_linked() {
    let mut state = WebGLState::new();
    state.programs.insert(1, WebGLProgram {
        vertex_shader: Some(10), fragment_shader: Some(11),
        linked: true,
        info_log: String::new(),
        vertex_wgsl: Some("v".into()), fragment_wgsl: Some("f".into()),
    });
    state.programs.insert(2, WebGLProgram {
        vertex_shader: None, fragment_shader: None,
        linked: false,
        info_log: String::new(),
        vertex_wgsl: None, fragment_wgsl: None,
    });
    let ids = webgl_linked_program_ids(&state);
    assert_eq!(ids, vec![1]);
}

#[test]
fn linked_program_ids_skips_missing_wgsl() {
    let mut state = WebGLState::new();
    state.programs.insert(1, WebGLProgram {
        vertex_shader: Some(1), fragment_shader: Some(2),
        linked: true,  // marked linked, ale chybi WGSL
        info_log: String::new(),
        vertex_wgsl: None, fragment_wgsl: None,
    });
    let ids = webgl_linked_program_ids(&state);
    assert_eq!(ids.len(), 0, "linked bez WGSL strings se preskakuje");
}

#[test]
fn linked_program_ids_empty_state() {
    let state = WebGLState::new();
    assert_eq!(webgl_linked_program_ids(&state).len(), 0);
}

#[test]
fn layout_has_canvas_finds_top_level() {
    let node = NodeData::new_element("canvas", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let bx = make_canvas_layout_box(node);
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(WebGLState::new())));
    assert!(webgl_layout_has_canvas(&bx, &states));
}

#[test]
fn layout_has_canvas_finds_in_children() {
    let parent_node = NodeData::new_element("div", HashMap::new());
    let canvas_node = NodeData::new_element("canvas", HashMap::new());
    let canvas_ptr = Rc::as_ptr(&canvas_node) as usize;
    let mut parent = LayoutBox::new();
    parent.tag = Some("div".into());
    parent.node = Some(parent_node);
    parent.children.push(make_canvas_layout_box(canvas_node));

    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(canvas_ptr, Rc::new(RefCell::new(WebGLState::new())));
    assert!(webgl_layout_has_canvas(&parent, &states));
}

#[test]
fn layout_has_canvas_no_state_returns_false() {
    let node = NodeData::new_element("canvas", HashMap::new());
    let bx = make_canvas_layout_box(node);
    let states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    assert!(!webgl_layout_has_canvas(&bx, &states));
}

#[test]
fn layout_has_canvas_non_canvas_tag_false() {
    let node = NodeData::new_element("div", HashMap::new());
    let ptr = Rc::as_ptr(&node) as usize;
    let mut bx = LayoutBox::new();
    bx.tag = Some("div".into());
    bx.node = Some(node);
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(ptr, Rc::new(RefCell::new(WebGLState::new())));
    assert!(!webgl_layout_has_canvas(&bx, &states), "div neni canvas");
}

#[test]
fn canvas_count_zero_when_no_state() {
    let node = NodeData::new_element("canvas", HashMap::new());
    let bx = make_canvas_layout_box(node);
    let states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    assert_eq!(webgl_canvas_count(&bx, &states), 0);
}

#[test]
fn canvas_count_multiple_canvases() {
    let parent_node = NodeData::new_element("div", HashMap::new());
    let c1_node = NodeData::new_element("canvas", HashMap::new());
    let c2_node = NodeData::new_element("canvas", HashMap::new());
    let c1_ptr = Rc::as_ptr(&c1_node) as usize;
    let c2_ptr = Rc::as_ptr(&c2_node) as usize;
    let mut parent = LayoutBox::new();
    parent.tag = Some("div".into());
    parent.node = Some(parent_node);
    parent.children.push(make_canvas_layout_box(c1_node));
    parent.children.push(make_canvas_layout_box(c2_node));

    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(c1_ptr, Rc::new(RefCell::new(WebGLState::new())));
    states.insert(c2_ptr, Rc::new(RefCell::new(WebGLState::new())));
    assert_eq!(webgl_canvas_count(&parent, &states), 2);
}

#[test]
fn canvas_count_partial_states() {
    // 2 canvas v tree, jen 1 ma WebGL state.
    let c1_node = NodeData::new_element("canvas", HashMap::new());
    let c2_node = NodeData::new_element("canvas", HashMap::new());
    let c1_ptr = Rc::as_ptr(&c1_node) as usize;
    let parent_node = NodeData::new_element("div", HashMap::new());
    let mut parent = LayoutBox::new();
    parent.tag = Some("div".into());
    parent.node = Some(parent_node);
    parent.children.push(make_canvas_layout_box(c1_node));
    parent.children.push(make_canvas_layout_box(c2_node));

    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(c1_ptr, Rc::new(RefCell::new(WebGLState::new())));
    assert_eq!(webgl_canvas_count(&parent, &states), 1);
}

// ─── WebGL phase 3c2 - vertex format + stride helpers ─────────────────

#[test]
fn webgl_attrib_format_float_2() {
    let f = webgl_attrib_to_vertex_format(2, 0x1406);  // FLOAT
    assert_eq!(f, Some(wgpu::VertexFormat::Float32x2));
}

#[test]
fn webgl_attrib_format_float_3() {
    let f = webgl_attrib_to_vertex_format(3, 0x1406);
    assert_eq!(f, Some(wgpu::VertexFormat::Float32x3));
}

#[test]
fn webgl_attrib_format_float_4() {
    let f = webgl_attrib_to_vertex_format(4, 0x1406);
    assert_eq!(f, Some(wgpu::VertexFormat::Float32x4));
}

#[test]
fn webgl_attrib_format_uint_2() {
    let f = webgl_attrib_to_vertex_format(2, 0x1405);  // UNSIGNED_INT
    assert_eq!(f, Some(wgpu::VertexFormat::Uint32x2));
}

#[test]
fn webgl_attrib_format_unsupported_byte() {
    let f = webgl_attrib_to_vertex_format(1, 0x1401);  // UNSIGNED_BYTE
    assert!(f.is_none(), "byte size 1 ne support");
}

#[test]
fn webgl_stride_explicit() {
    let attribs = vec![
        (0u32, WebGLAttribSlot {
            buffer_id: 1, size: 2, component_type: 0x1406,
            normalized: false, stride: 32, offset: 0, enabled: true,
        }),
    ];
    assert_eq!(webgl_compute_stride(&attribs), 32);
}

#[test]
fn webgl_stride_tightly_packed_float2() {
    let attribs = vec![
        (0u32, WebGLAttribSlot {
            buffer_id: 1, size: 2, component_type: 0x1406,
            normalized: false, stride: 0, offset: 0, enabled: true,
        }),
    ];
    // 2 * 4 (FLOAT) = 8
    assert_eq!(webgl_compute_stride(&attribs), 8);
}

#[test]
fn webgl_stride_tightly_packed_multi_attrib() {
    let attribs = vec![
        (0u32, WebGLAttribSlot {
            buffer_id: 1, size: 3, component_type: 0x1406,  // pos vec3
            normalized: false, stride: 0, offset: 0, enabled: true,
        }),
        (1u32, WebGLAttribSlot {
            buffer_id: 1, size: 2, component_type: 0x1406,  // uv vec2
            normalized: false, stride: 0, offset: 12, enabled: true,
        }),
    ];
    // 3*4 + 2*4 = 20
    assert_eq!(webgl_compute_stride(&attribs), 20);
}

#[test]
fn webgl_stride_empty_returns_zero() {
    assert_eq!(webgl_compute_stride(&[]), 0);
}

// ─── Phase 3c5 dual path consistency ───────────────────────────────────

#[test]
fn webgl_state_drain_idempotent() {
    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    state.draw_queue.push(make_drawarrays(3));

    // First drain
    let f1 = webgl_extract_pending(&mut state);
    assert_eq!(f1.commands.len(), 2);

    // Second drain - musi byt prazdne (already drained)
    let f2 = webgl_extract_pending(&mut state);
    assert_eq!(f2.commands.len(), 0);
}

#[test]
fn webgl_state_buffers_persist_across_drains() {
    // Pri kazdem extract_pending - buffers se NEVYMAZAVAJI (clone).
    let mut state = WebGLState::new();
    state.buffers.insert(1, vec![1, 2, 3, 4]);
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));

    let f1 = webgl_extract_pending(&mut state);
    assert_eq!(f1.buffers.len(), 1);

    // State stale ma buffer
    assert_eq!(state.buffers.len(), 1);

    // Drain znovu
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let f2 = webgl_extract_pending(&mut state);
    assert_eq!(f2.buffers.len(), 1, "buffers v state pri kazdem extract");
}

#[test]
fn webgl_extract_default_clear_color_preserved() {
    let mut state = WebGLState::new();
    state.clear_color = [0.7, 0.3, 0.1, 1.0];
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let frame = webgl_extract_pending(&mut state);
    assert_eq!(frame.default_clear, [0.7, 0.3, 0.1, 1.0]);
    // State clear_color preserved (mutace pres ClearColor cmd, ne extract)
    assert_eq!(state.clear_color, [0.7, 0.3, 0.1, 1.0]);
}

#[test]
fn webgl_effective_clear_with_only_explicit_clear_color_in_command() {
    // ClearColor v command + Clear bit - posledni vyhraje
    let cmds = vec![
        WebGLDrawCmd::ClearColor([0.1, 0.2, 0.3, 0.5]),
        WebGLDrawCmd::Clear(0x4000),
    ];
    let r = webgl_effective_clear(&cmds, [0.9, 0.9, 0.9, 1.0]);
    assert_eq!(r, Some([0.1, 0.2, 0.3, 0.5]), "command-level ClearColor wins");
}

#[test]
fn webgl_count_draws_drawelements_counted() {
    let cmds = vec![make_drawelements(), make_drawelements()];
    assert_eq!(webgl_count_draws(&cmds), 2);
    assert_eq!(webgl_count_clears(&cmds), 0);
}

// ─── partition_filter_segments dual path ───────────────────────────────

#[test]
fn partition_main_filter_transform_main() {
    // Sequence: Main, Filter, Main, Transform, Main
    let cmds = vec![
        rect(0.0, 0.0),
        filter_begin(2.0),
        rect(10.0, 10.0),
        DisplayCommand::FilterEnd,
        rect(20.0, 20.0),
        transform_begin(1.0),
        rect(30.0, 30.0),
        DisplayCommand::TransformEnd,
        rect(40.0, 40.0),
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 5, "Main+Filter+Main+Transform+Main = 5 segs");
    matches!(&segs[0], Seg::Main(_));
    matches!(&segs[1], Seg::Filter { .. });
    matches!(&segs[2], Seg::Main(_));
    matches!(&segs[3], Seg::Transform3D { .. });
    matches!(&segs[4], Seg::Main(_));
}

#[test]
fn partition_no_main_between_filter_and_transform() {
    let cmds = vec![
        filter_begin(2.0),
        rect(0.0, 0.0),
        DisplayCommand::FilterEnd,
        transform_begin(1.0),
        rect(10.0, 10.0),
        DisplayCommand::TransformEnd,
    ];
    let segs = partition_filter_segments(&cmds);
    assert_eq!(segs.len(), 2, "Filter, Transform - bez Main mezi");
}

#[test]
fn partition_unbalanced_transform_no_seg() {
    let cmds = vec![
        rect(0.0, 0.0),
        transform_begin(1.0),
        rect(10.0, 10.0),
        // chybi TransformEnd
    ];
    let segs = partition_filter_segments(&cmds);
    // Mel by mit aspon Main pred Transform
    assert!(segs.len() <= 2);
}

// ─── webgl_layout_walkers edge cases ───────────────────────────────────

#[test]
fn webgl_canvas_count_recursive_through_div() {
    let parent_node = NodeData::new_element("div", HashMap::new());
    let inner_div = NodeData::new_element("div", HashMap::new());
    let canvas_node = NodeData::new_element("canvas", HashMap::new());
    let canvas_ptr = Rc::as_ptr(&canvas_node) as usize;

    let mut deep_canvas = make_canvas_layout_box(canvas_node);
    deep_canvas.tag = Some("canvas".into());

    let mut middle = LayoutBox::new();
    middle.tag = Some("div".into());
    middle.node = Some(inner_div);
    middle.children.push(deep_canvas);

    let mut outer = LayoutBox::new();
    outer.tag = Some("div".into());
    outer.node = Some(parent_node);
    outer.children.push(middle);

    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(canvas_ptr, Rc::new(RefCell::new(WebGLState::new())));
    assert_eq!(webgl_canvas_count(&outer, &states), 1, "deep canvas finded");
    assert!(webgl_layout_has_canvas(&outer, &states));
}

#[test]
fn webgl_layout_has_canvas_short_circuits() {
    // Ne walkuje vic nez musi - test s prazdnymi states.
    let canvas_node = NodeData::new_element("canvas", HashMap::new());
    let bx = make_canvas_layout_box(canvas_node);
    let states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    assert!(!webgl_layout_has_canvas(&bx, &states));
}

#[test]
fn linked_program_ids_filters_unlinked() {
    let mut state = WebGLState::new();
    state.programs.insert(1, WebGLProgram {
        vertex_shader: Some(2), fragment_shader: Some(3),
        linked: false,  // not linked
        info_log: String::new(),
        vertex_wgsl: Some("v".into()), fragment_wgsl: Some("f".into()),
    });
    state.programs.insert(2, WebGLProgram {
        vertex_shader: Some(4), fragment_shader: Some(5),
        linked: true,
        info_log: String::new(),
        vertex_wgsl: Some("v".into()), fragment_wgsl: Some("f".into()),
    });
    let ids = webgl_linked_program_ids(&state);
    assert_eq!(ids, vec![2]);
}

#[test]
fn webgl_count_draws_combines_arrays_elements() {
    let cmds = vec![
        make_drawarrays(3),
        make_drawelements(),
        WebGLDrawCmd::Clear(0x4000),
        make_drawarrays(6),
    ];
    assert_eq!(webgl_count_draws(&cmds), 3);
    assert_eq!(webgl_count_clears(&cmds), 1);
}

#[test]
fn polygon_signed_area_reverse_winding_negative() {
    // CCW polygon (in screen y-down) -> negative area.
    let pts = [(0.0_f32, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0)];
    let area = polygon_signed_area(&pts);
    assert!(area < 0.0, "CCW screen polygon -> negative");
}

#[test]
fn triangulate_triangle_with_reverse_winding() {
    // CCW triangle - jeden ear, jeden triangle.
    let pts = vec![(0.0_f32, 0.0), (5.0, 10.0), (10.0, 0.0)];
    let tris = triangulate_polygon(&pts);
    assert_eq!(tris.len(), 1);
}

#[test]
fn triangulate_returns_correct_total_area() {
    // Convex pentagon - sum trojuhelniku rovny polygon area
    let pts = vec![
        (50.0_f32, 0.0),
        (100.0, 38.0),
        (82.0, 100.0),
        (18.0, 100.0),
        (0.0, 38.0),
    ];
    let tris = triangulate_polygon(&pts);
    let total: f32 = tris.iter().map(|(a, b, c)| {
        ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs() * 0.5
    }).sum();
    let poly = polygon_signed_area(&pts).abs();
    assert!((total - poly).abs() < 5.0, "triangulace area = polygon area");
}

#[test]
fn webgl_attrib_format_negative_size_returns_none() {
    assert!(webgl_attrib_to_vertex_format(0, 0x1406).is_none());
    assert!(webgl_attrib_to_vertex_format(5, 0x1406).is_none());
}

#[test]
fn webgl_compute_stride_zero_attribs_zero() {
    assert_eq!(webgl_compute_stride(&[]), 0);
}

#[test]
fn webgl_compute_stride_explicit_takes_precedence() {
    let attribs = vec![
        (0u32, WebGLAttribSlot {
            buffer_id: 1, size: 4, component_type: 0x1406,
            normalized: false, stride: 64, offset: 0, enabled: true,
        }),
    ];
    // Explicit 64 stride > tightly packed 16
    assert_eq!(webgl_compute_stride(&attribs), 64);
}

#[test]
fn paint_webgl_recurses_to_children() {
    let parent_node = NodeData::new_element("div", HashMap::new());
    let canvas_node = NodeData::new_element("canvas", HashMap::new());
    let canvas_ptr = Rc::as_ptr(&canvas_node) as usize;

    let mut parent = LayoutBox::new();
    parent.tag = Some("div".into());
    parent.rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    parent.node = Some(parent_node);
    parent.children.push(make_canvas_layout_box(canvas_node));

    let mut state = WebGLState::new();
    state.draw_queue.push(WebGLDrawCmd::ClearColor([0.0, 0.0, 1.0, 1.0]));
    state.draw_queue.push(WebGLDrawCmd::Clear(0x4000));
    let mut states: HashMap<usize, Rc<RefCell<WebGLState>>> = HashMap::new();
    states.insert(canvas_ptr, Rc::new(RefCell::new(state)));

    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_webgl_canvases(&parent, &states, &mut cmds);
    assert_eq!(cmds.len(), 1);
    if let DisplayCommand::Rect { color, .. } = &cmds[0] {
        assert_eq!(*color, [0, 0, 255, 255], "blue z child canvas");
    }
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
