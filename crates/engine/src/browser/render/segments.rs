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
        | DisplayCommand::RectRounded { y, .. }
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
        | DisplayCommand::RectRounded { x, .. }
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

/// Konzervativni (spis vetsi) AABB bbox display commandu v jeho souradnicovem
/// prostoru (= layer-local pri tile rasteru). None = marker bez geometrie
/// (End markery, NoScrollShift) - filtr je vzdy nechava projit.
///
/// Effect-extenty (shadow blur/spread, filter/blurred blur) jsou ZAPOCITANY do
/// bboxu (rozsireny), aby filtr na tile rect neorezal stin/blur presahujici box.
/// Scoped Begin markery vraci bbox subtree (x,y,w,h z markeru); Transform ma
/// velky margin (transformovany obsah muze presahnout untransformed rect).
pub(crate) fn cmd_bbox(cmd: &DisplayCommand) -> Option<(f32, f32, f32, f32)> {
    use crate::browser::paint::DisplayCommand as D;
    // Maly univerzalni pad (sub-pixel snap + AA feather).
    const PAD: f32 = 2.0;
    let pad = |x: f32, y: f32, w: f32, h: f32, e: f32| {
        Some((x - e - PAD, y - e - PAD, w + 2.0 * (e + PAD), h + 2.0 * (e + PAD)))
    };
    match cmd {
        D::Rect { x, y, w, h, .. }
        | D::RectRounded { x, y, w, h, .. }
        | D::Border { x, y, w, h, .. }
        | D::Gradient { x, y, w, h, .. }
        | D::Image { x, y, w, h, .. }
        | D::ImageFit { x, y, w, h, .. } => pad(*x, *y, *w, *h, 0.0),
        D::BlurredRect { x, y, w, h, blur, .. } => pad(*x, *y, *w, *h, blur.abs()),
        D::Shadow { x, y, w, h, offset_x, offset_y, blur, spread, .. } => {
            // Stin se posune o offset + rozteka o blur+spread na obe strany.
            let e = blur.abs() + spread.abs();
            pad(*x + offset_x.min(0.0), *y + offset_y.min(0.0),
                *w + offset_x.abs(), *h + offset_y.abs(), e)
        }
        D::Text { x, y, content, font_size, .. } => {
            // Konzervativni odhad (radsi vetsi): char width <= font_size, line
            // height ~1.6*fs. Multi-line pres '\n'. y muze byt top i baseline ->
            // pad o font_size nahoru.
            let max_chars = content.split('\n')
                .map(|l| l.chars().count()).max().unwrap_or(0);
            let n_lines = content.split('\n').count().max(1) as f32;
            let w = (max_chars as f32) * font_size + font_size;
            let h = n_lines * font_size * 1.6;
            Some((*x - PAD, *y - *font_size, w + 2.0 * PAD, h + 2.0 * font_size))
        }
        D::ClippedRect { points, .. } | D::ClippedGradient { points, .. } => {
            if points.is_empty() { return None; }
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (px, py) in points {
                x0 = x0.min(*px); y0 = y0.min(*py); x1 = x1.max(*px); y1 = y1.max(*py);
            }
            pad(x0, y0, x1 - x0, y1 - y0, 0.0)
        }
        D::FilterBegin { x, y, w, h, blur_radius, .. }
        | D::BackdropFilterBegin { x, y, w, h, blur_radius, .. } =>
            pad(*x, *y, *w, *h, blur_radius.abs()),
        D::MaskBegin { x, y, w, h, .. }
        | D::BlendBegin { x, y, w, h, .. } => pad(*x, *y, *w, *h, 0.0),
        D::TransformBegin { x, y, w, h, .. } => {
            // Transformovany obsah (rotace/scale/translate) muze presahnout
            // untransformed rect -> velky margin (radsi zahrnout do vic tiles
            // nez orezat). Vzacne v tiled layeru (transformed el = vlastni layer).
            let m = (w.max(*h)) * 0.5 + 64.0;
            pad(*x, *y, *w, *h, m)
        }
        // End markery, NoScrollShift = bez geometrie -> filtr je nechava projit.
        _ => None,
    }
}

/// Vrati (type_id, is_begin) pro scoped segment marker (Filter/Backdrop/Transform/
/// Mask/Blend), jinak None. NoScrollShift NENI partition scope (ignorovan).
fn scope_marker_kind(cmd: &DisplayCommand) -> Option<(u8, bool)> {
    use crate::browser::paint::DisplayCommand as D;
    match cmd {
        D::FilterBegin { .. } => Some((0, true)),
        D::FilterEnd => Some((0, false)),
        D::BackdropFilterBegin { .. } => Some((1, true)),
        D::BackdropFilterEnd => Some((1, false)),
        D::TransformBegin { .. } => Some((2, true)),
        D::TransformEnd => Some((2, false)),
        D::MaskBegin { .. } => Some((3, true)),
        D::MaskEnd => Some((3, false)),
        D::BlendBegin { .. } => Some((4, true)),
        D::BlendEnd => Some((4, false)),
        _ => None,
    }
}

/// Najde index End markeru parovaneho s Begin na `start` (stejny typ, depth-aware,
/// match logiky partition_filter_segments). Pri malformed vraci posledni index.
fn scope_end_index(cmds: &[DisplayCommand], start: usize) -> usize {
    let kind = match scope_marker_kind(&cmds[start]) { Some((k, _)) => k, None => return start };
    let mut depth = 0i32;
    for (j, cmd) in cmds.iter().enumerate().skip(start) {
        if let Some((k, is_begin)) = scope_marker_kind(cmd) {
            if k == kind {
                if is_begin { depth += 1; } else { depth -= 1; if depth == 0 { return j; } }
            }
        }
    }
    cmds.len().saturating_sub(1)
}

#[inline]
fn aabb_intersect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

/// Filtr display listu na `tile` rect (x,y,w,h v layer-local coords): vrati jen
/// commands ktere protinaji tile. Scoped segmenty (Filter/Transform/Mask/Blend/
/// Backdrop) se berou ATOMICKY - cely Begin..End se zahrne nebo zahodi dle bboxu
/// scope (zachova Begin/End parovani pro partition_filter_segments). Markery bez
/// geometrie (NoScrollShift) projdou vzdy.
///
/// Win: render_into_tile pak buildi/kresli jen ~obsah tile misto cele (velke)
/// vrstvy per tile -> N dirty tiles uz neni N x full-layer vertex build.
pub(crate) fn filter_cmds_to_tile(
    cmds: &[DisplayCommand],
    tile: (f32, f32, f32, f32),
) -> Vec<DisplayCommand> {
    let mut out: Vec<DisplayCommand> = Vec::new();
    let mut i = 0;
    while i < cmds.len() {
        if scope_marker_kind(&cmds[i]).map(|(_, b)| b) == Some(true) {
            // Top-level scope Begin - vezmi atomicky cely scope dle bboxu.
            let end = scope_end_index(cmds, i);
            let keep = cmd_bbox(&cmds[i]).map(|bb| aabb_intersect(bb, tile)).unwrap_or(true);
            if keep {
                out.extend_from_slice(&cmds[i..=end]);
            }
            i = end + 1;
        } else {
            // Bezny cmd (nebo End/NoScrollShift = bbox None -> projde).
            let keep = cmd_bbox(&cmds[i]).map(|bb| aabb_intersect(bb, tile)).unwrap_or(true);
            if keep { out.push(cmds[i].clone()); }
            i += 1;
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::paint::DisplayCommand as D;

    fn rect(x: f32, y: f32) -> D {
        D::Rect { x, y, w: 50.0, h: 50.0, color: [0, 0, 0, 255], radius: 0.0 }
    }
    fn filt_begin(x: f32, y: f32) -> D {
        D::FilterBegin { x, y, w: 100.0, h: 100.0, blur_radius: 0.0, color_matrix: [0.0; 20] }
    }

    #[test]
    fn filter_keeps_intersecting_drops_outside() {
        let cmds = vec![rect(10.0, 10.0), rect(5000.0, 5000.0)];
        let out = filter_cmds_to_tile(&cmds, (0.0, 0.0, 1024.0, 1024.0));
        assert_eq!(out.len(), 1, "jen protinajici Rect zustane");
        match &out[0] { D::Rect { x, .. } => assert!((*x - 10.0).abs() < 0.1), _ => panic!() }
    }

    #[test]
    fn filter_scope_atomic_balanced() {
        // Scope uvnitr tile -> cely (Begin + inner + End) zahrnut.
        let inside = vec![filt_begin(10.0, 10.0), rect(20.0, 20.0), D::FilterEnd];
        let out = filter_cmds_to_tile(&inside, (0.0, 0.0, 1024.0, 1024.0));
        assert_eq!(out.len(), 3, "cely scope zahrnut (Begin+inner+End)");
        // Scope mimo tile -> cely zahozen vc. Begin i End (balance zachovan).
        let outside = vec![filt_begin(5000.0, 5000.0), rect(5020.0, 5020.0), D::FilterEnd];
        let out2 = filter_cmds_to_tile(&outside, (0.0, 0.0, 1024.0, 1024.0));
        assert_eq!(out2.len(), 0, "cely scope zahozen, zadny osamoceny End");
    }

    #[test]
    fn filter_text_bbox_conservative() {
        // Text bbox je konzervativni (spis vetsi) - kratky text na (10,10) musi
        // protnout tile u originu.
        let txt = D::Text {
            x: 10.0, y: 10.0, content: "ahoj".to_string(), color: [255; 4],
            font_size: 16.0, font_weight: 400, bold: false, italic: false,
            font_family: String::new(), strikethrough: false, underline: false,
        };
        let out = filter_cmds_to_tile(std::slice::from_ref(&txt), (0.0, 0.0, 1024.0, 1024.0));
        assert_eq!(out.len(), 1);
        // Daleko mimo tile -> zahozen.
        let out2 = filter_cmds_to_tile(std::slice::from_ref(&txt), (5000.0, 5000.0, 1024.0, 1024.0));
        assert_eq!(out2.len(), 0);
    }
}
