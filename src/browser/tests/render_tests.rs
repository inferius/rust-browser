/// Testy pro display list segmentaci a render helpers.
/// Plne wgpu render se neda unit testit (potrebuje GPU device), ale partition
/// filteru a paint segmentaci ano.

use crate::browser::paint::DisplayCommand;
use crate::browser::render::{partition_filter_segments, Seg};

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
        }
    }
    // 5 ne-marker cmds (3 mimo + 2 uvnitr filtru); markery se neztraceji v inner
    assert_eq!(total, 5);
}
