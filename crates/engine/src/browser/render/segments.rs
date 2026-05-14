//! Display list segmentace + shift helpers.
//!
//! Filter/Transform/Mask segmenty se renderuje pres offscreen RT.
//! Main segmenty jdou primo na swap chain. Shift_command_y/x posune
//! souradnice v X/Y (pro scroll posun).

use crate::browser::paint::DisplayCommand;

/// Segment displej listu pro renderer: Main = normal, Filter = RT-mediated.
pub enum Seg<'a> {
    Main(&'a [DisplayCommand]),
    Filter {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        radius: f32,
        color_matrix: [f32; 20],
    },
    Transform3D {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        matrix: [f32; 16],
    },
    /// Backdrop-filter: snapshotne main_rt (scenu za elementem), aplikuje
    /// blur + color matrix, composit jako podklad, pak inner obsah nahoru.
    BackdropFilter {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        radius: f32,
        color_matrix: [f32; 20],
    },
    /// mask-image subtree: render content do RT, aplikuj alpha mask, composit.
    Mask {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        mask_src: String,
    },
}

/// Rozdeli display list na Main + Filter + Transform3D segmenty pres
/// FilterBegin/End a TransformBegin/End markery. Pri vnoreni: prvni Begin
/// marker urci typ top-level segmentu, vnorene Begin/End jineho typu
/// jsou soucasti inner cmds (ne novy segment).
/// Symetricnost markeru je predpokladana.
pub fn partition_filter_segments(cmds: &[DisplayCommand]) -> Vec<Seg<'_>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind { Filter, Transform, Backdrop, Mask }
    let mut segments: Vec<Seg> = Vec::new();
    let mut depth: i32 = 0;
    let mut active_kind: Option<Kind> = None;
    let mut cursor: usize = 0;
    let mut seg_start: usize = 0;
    let mut filter_params: (f32, f32, f32, f32, f32, [f32; 20]) =
        (0.0, 0.0, 0.0, 0.0, 0.0, [0.0; 20]);
    let mut tx_params: (f32, f32, f32, f32, [f32; 16]) =
        (0.0, 0.0, 0.0, 0.0, [0.0; 16]);
    let mut backdrop_params: (f32, f32, f32, f32, f32, [f32; 20]) =
        (0.0, 0.0, 0.0, 0.0, 0.0, [0.0; 20]);
    let mut mask_params: (f32, f32, f32, f32, String) =
        (0.0, 0.0, 0.0, 0.0, String::new());
    for i in 0..cmds.len() {
        match &cmds[i] {
            DisplayCommand::FilterBegin { x, y, w, h, blur_radius, color_matrix } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    filter_params = (*x, *y, *w, *h, *blur_radius, *color_matrix);
                    active_kind = Some(Kind::Filter);
                }
                if active_kind == Some(Kind::Filter) { depth += 1; }
            }
            DisplayCommand::FilterEnd => {
                if active_kind == Some(Kind::Filter) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, r, m) = filter_params;
                        segments.push(Seg::Filter {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, radius: r, color_matrix: m,
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            DisplayCommand::TransformBegin { x, y, w, h, matrix } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    tx_params = (*x, *y, *w, *h, *matrix);
                    active_kind = Some(Kind::Transform);
                }
                if active_kind == Some(Kind::Transform) { depth += 1; }
            }
            DisplayCommand::TransformEnd => {
                if active_kind == Some(Kind::Transform) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, m) = tx_params;
                        segments.push(Seg::Transform3D {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, matrix: m,
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            DisplayCommand::BackdropFilterBegin { x, y, w, h, blur_radius, color_matrix } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    backdrop_params = (*x, *y, *w, *h, *blur_radius, *color_matrix);
                    active_kind = Some(Kind::Backdrop);
                }
                if active_kind == Some(Kind::Backdrop) { depth += 1; }
            }
            DisplayCommand::BackdropFilterEnd => {
                if active_kind == Some(Kind::Backdrop) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, r, m) = backdrop_params;
                        segments.push(Seg::BackdropFilter {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, radius: r, color_matrix: m,
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            DisplayCommand::MaskBegin { x, y, w, h, mask_src } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    mask_params = (*x, *y, *w, *h, mask_src.clone());
                    active_kind = Some(Kind::Mask);
                }
                if active_kind == Some(Kind::Mask) { depth += 1; }
            }
            DisplayCommand::MaskEnd => {
                if active_kind == Some(Kind::Mask) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, ref src) = mask_params;
                        segments.push(Seg::Mask {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, mask_src: src.clone(),
                        });
                        cursor = i + 1;
                        active_kind = None;
                    }
                }
            }
            _ => {}
        }
    }
    if cursor < cmds.len() {
        segments.push(Seg::Main(&cmds[cursor..]));
    }
    segments
}

/// Posune Y souradnice display command (pro scroll).
pub fn shift_command_y(cmd: &mut DisplayCommand, dy: f32) {
    match cmd {
        DisplayCommand::Rect { y, .. }
        | DisplayCommand::Border { y, .. }
        | DisplayCommand::Text { y, .. }
        | DisplayCommand::Gradient { y, .. }
        | DisplayCommand::Shadow { y, .. }
        | DisplayCommand::Image { y, .. }
        | DisplayCommand::ImageFit { y, .. }
        | DisplayCommand::BlurredRect { y, .. }
        | DisplayCommand::FilterBegin { y, .. }
        | DisplayCommand::BackdropFilterBegin { y, .. }
        | DisplayCommand::TransformBegin { y, .. }
        | DisplayCommand::MaskBegin { y, .. } => *y += dy,
        DisplayCommand::ClippedRect { points, .. } => {
            for (_, py) in points.iter_mut() {
                *py += dy;
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd => {}
    }
}

pub fn shift_command_x(cmd: &mut DisplayCommand, dx: f32) {
    match cmd {
        DisplayCommand::Rect { x, .. }
        | DisplayCommand::Border { x, .. }
        | DisplayCommand::Text { x, .. }
        | DisplayCommand::Gradient { x, .. }
        | DisplayCommand::Shadow { x, .. }
        | DisplayCommand::Image { x, .. }
        | DisplayCommand::ImageFit { x, .. }
        | DisplayCommand::BlurredRect { x, .. }
        | DisplayCommand::FilterBegin { x, .. }
        | DisplayCommand::BackdropFilterBegin { x, .. }
        | DisplayCommand::TransformBegin { x, .. }
        | DisplayCommand::MaskBegin { x, .. } => *x += dx,
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, _) in points.iter_mut() {
                *px += dx;
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd => {}
    }
}
