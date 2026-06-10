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
    /// mix-blend-mode subtree (Overlay/ColorDodge/Hue/Sat/... vyzaduje shader
    /// dst sample). Render inner do offscreen, sample dst snapshot per pixel,
    /// compute blend formula, output do main_rt.
    /// Inspired by Chromium `core/paint/effect_paint_property_node.cc` blend.
    Blend {
        inner: &'a [DisplayCommand],
        x: f32, y: f32, w: f32, h: f32,
        mode: u8,
    },
}

/// Rozdeli display list na Main + Filter + Transform3D segmenty pres
/// FilterBegin/End a TransformBegin/End markery. Pri vnoreni: prvni Begin
/// marker urci typ top-level segmentu, vnorene Begin/End jineho typu
/// jsou soucasti inner cmds (ne novy segment).
/// Symetricnost markeru je predpokladana.
pub fn partition_filter_segments(cmds: &[DisplayCommand]) -> Vec<Seg<'_>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind { Filter, Transform, Backdrop, Mask, Blend }
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
    let mut blend_params: (f32, f32, f32, f32, u8) =
        (0.0, 0.0, 0.0, 0.0, 0);
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
            DisplayCommand::BlendBegin { x, y, w, h, mode } => {
                if depth == 0 {
                    if cursor < i { segments.push(Seg::Main(&cmds[cursor..i])); }
                    seg_start = i + 1;
                    blend_params = (*x, *y, *w, *h, *mode);
                    active_kind = Some(Kind::Blend);
                }
                if active_kind == Some(Kind::Blend) { depth += 1; }
            }
            DisplayCommand::BlendEnd => {
                if active_kind == Some(Kind::Blend) {
                    depth -= 1;
                    if depth == 0 {
                        let (x, y, w, h, mode) = blend_params;
                        segments.push(Seg::Blend {
                            inner: &cmds[seg_start..i],
                            x, y, w, h, mode,
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
        | DisplayCommand::Shadow { y, .. }
        | DisplayCommand::Image { y, .. }
        | DisplayCommand::ImageFit { y, .. }
        | DisplayCommand::BlurredRect { y, .. }
        | DisplayCommand::FilterBegin { y, .. }
        | DisplayCommand::BackdropFilterBegin { y, .. }
        | DisplayCommand::TransformBegin { y, .. }
        | DisplayCommand::MaskBegin { y, .. } => *y += dy,
        // Gradient: krom rect posunout i ABSOLUTNI stred Radial/Conic kindu.
        // Bez tohoto layer-local raster (i scroll shift) nechal stred ve world
        // coords -> shader sampluje vzdalenou (transparent) cast gradientu =
        // "radial/conic boxy cerne" na strankach s vice layery.
        DisplayCommand::Gradient { y, kind, .. } => {
            *y += dy;
            match kind {
                crate::browser::paint::GradientKind::Radial { cy, .. } => *cy += dy,
                crate::browser::paint::GradientKind::Conic { cy, .. } => *cy += dy,
                crate::browser::paint::GradientKind::Linear { .. } => {}
            }
        }
        DisplayCommand::ClippedRect { points, .. } => {
            for (_, py) in points.iter_mut() {
                *py += dy;
            }
        }
        DisplayCommand::ClippedGradient { points, y, .. } => {
            for (_, py) in points.iter_mut() { *py += dy; }
            *y += dy;
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd
        | DisplayCommand::NoScrollShiftBegin | DisplayCommand::NoScrollShiftEnd
        | DisplayCommand::BlendBegin { .. } | DisplayCommand::BlendEnd => {}
    }
}

pub fn shift_command_x(cmd: &mut DisplayCommand, dx: f32) {
    match cmd {
        DisplayCommand::Rect { x, .. }
        | DisplayCommand::Border { x, .. }
        | DisplayCommand::Text { x, .. }
        | DisplayCommand::Shadow { x, .. }
        | DisplayCommand::Image { x, .. }
        | DisplayCommand::ImageFit { x, .. }
        | DisplayCommand::BlurredRect { x, .. }
        | DisplayCommand::FilterBegin { x, .. }
        | DisplayCommand::BackdropFilterBegin { x, .. }
        | DisplayCommand::TransformBegin { x, .. }
        | DisplayCommand::MaskBegin { x, .. } => *x += dx,
        // Gradient: viz shift_command_y - posun i stred Radial/Conic.
        DisplayCommand::Gradient { x, kind, .. } => {
            *x += dx;
            match kind {
                crate::browser::paint::GradientKind::Radial { cx, .. } => *cx += dx,
                crate::browser::paint::GradientKind::Conic { cx, .. } => *cx += dx,
                crate::browser::paint::GradientKind::Linear { .. } => {}
            }
        }
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, _) in points.iter_mut() {
                *px += dx;
            }
        }
        DisplayCommand::ClippedGradient { points, x, .. } => {
            for (px, _) in points.iter_mut() { *px += dx; }
            *x += dx;
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd
        | DisplayCommand::NoScrollShiftBegin | DisplayCommand::NoScrollShiftEnd
        | DisplayCommand::BlendBegin { .. } | DisplayCommand::BlendEnd => {}
    }
}

/// Apply viewport scroll shift na display list. Respektuje
/// NoScrollShiftBegin/End markers (position:fixed elementy zustavaji staticke).
/// Stack-based vnoreni OK.
/// Inspired by Chromium cc/trees/property_tree_builder.cc:CreateScrollNode -
/// fixed elementy maji jiny scroll tree node ne root scroller.
pub fn apply_scroll_shift(cmds: &mut [DisplayCommand], dx: f32, dy: f32) {
    let mut no_shift_depth = 0u32;
    for cmd in cmds.iter_mut() {
        match cmd {
            DisplayCommand::NoScrollShiftBegin => { no_shift_depth += 1; }
            DisplayCommand::NoScrollShiftEnd => { no_shift_depth = no_shift_depth.saturating_sub(1); }
            _ => {
                if no_shift_depth == 0 {
                    shift_command_y(cmd, dy);
                    shift_command_x(cmd, dx);
                }
            }
        }
    }
}
