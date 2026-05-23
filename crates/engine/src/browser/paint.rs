/// Painting - z LayoutBox tree generuje display list (commands).
/// Display list je sekvence primitiv ktere wgpu rendered pak vykresli.

use std::rc::Rc;
use super::layout::{LayoutBox, TextAlign, BgPosition, BgSize};

/// Vypocti final rozmer background image podle bg-size.
/// Pro `cover` / `contain` potrebujeme znat puvodni rozmer - default 100x100 jako placeholder
/// (skutecny rozmer load-time z image cache, ale paint nevidi cache).
pub fn compute_bg_size(size: &BgSize, box_w: f32, box_h: f32) -> (f32, f32) {
    let default = (box_w, box_h);
    match size {
        BgSize::Auto => default,
        BgSize::Cover => default,    // approx: cele box
        BgSize::Contain => default,  // approx: cele box
        BgSize::Length { w, h } => (
            w.unwrap_or(box_w),
            h.unwrap_or(box_h),
        ),
        BgSize::Pct { w, h } => (
            w.map(|p| p * box_w).unwrap_or(box_w),
            h.map(|p| p * box_h).unwrap_or(box_h),
        ),
    }
}

/// Vypocti pozici background image v boxu (top-left).
pub fn compute_bg_position(
    pos: &BgPosition, box_w: f32, box_h: f32,
    img_w: f32, img_h: f32,
    box_x: f32, box_y: f32,
) -> (f32, f32) {
    let (offx, offy) = match pos {
        BgPosition::Px(x, y) => (*x, *y),
        BgPosition::Pct(x, y) => ((box_w - img_w) * x, (box_h - img_h) * y),
        BgPosition::Mixed { x_px, x_pct, y_px, y_pct } => {
            let ox = if let Some(px) = x_px { *px }
                     else if let Some(p)  = x_pct { (box_w - img_w) * p }
                     else { 0.0 };
            let oy = if let Some(px) = y_px { *px }
                     else if let Some(p)  = y_pct { (box_h - img_h) * p }
                     else { 0.0 };
            (ox, oy)
        }
    };
    (box_x + offx, box_y + offy)
}

/// Canvas 2D op - JS API mapping na primitivy.
#[derive(Debug, Clone)]
pub enum CanvasOp {
    /// fillStyle = "<color>"
    FillStyle([u8; 4]),
    /// strokeStyle = "<color>"
    StrokeStyle([u8; 4]),
    /// lineWidth = N
    LineWidth(f32),
    /// font = "<size>px <family>"
    Font { size: f32, family: String },
    /// fillRect(x, y, w, h)
    FillRect { x: f32, y: f32, w: f32, h: f32 },
    /// strokeRect(x, y, w, h)
    StrokeRect { x: f32, y: f32, w: f32, h: f32 },
    /// clearRect(x, y, w, h) - zaplni transparent
    ClearRect { x: f32, y: f32, w: f32, h: f32 },
    /// fillText(text, x, y)
    FillText { text: String, x: f32, y: f32 },
    /// beginPath - reset path
    BeginPath,
    /// moveTo(x, y)
    MoveTo { x: f32, y: f32 },
    /// lineTo(x, y)
    LineTo { x: f32, y: f32 },
    /// arc(cx, cy, r, start_rad, end_rad)
    Arc { cx: f32, cy: f32, r: f32, start: f32, end: f32 },
    /// closePath - close current sub-path
    ClosePath,
    /// stroke - kresli path obrysem
    Stroke,
    /// fill - vyplni path
    Fill,
    /// save() - push state stack
    Save,
    /// restore() - pop state stack
    Restore,
    /// translate(dx, dy)
    Translate { dx: f32, dy: f32 },
    /// rotate(rad)
    Rotate { rad: f32 },
    /// scale(sx, sy)
    Scale { sx: f32, sy: f32 },
    /// setTransform(a, b, c, d, e, f)
    SetTransform { a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 },
    /// transform(a, b, c, d, e, f) - kompozice na soucasnu transform
    Transform { a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 },
    /// resetTransform - identity
    ResetTransform,
    /// globalAlpha (0..1)
    GlobalAlpha(f32),
    /// globalCompositeOperation - source-over | multiply | screen | ...
    GlobalCompositeOperation(String),
    /// quadraticCurveTo(cpx, cpy, x, y)
    QuadraticCurveTo { cpx: f32, cpy: f32, x: f32, y: f32 },
    /// bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y)
    BezierCurveTo { cp1x: f32, cp1y: f32, cp2x: f32, cp2y: f32, x: f32, y: f32 },
    /// rect(x, y, w, h) - prida obdelnik do current path
    PathRect { x: f32, y: f32, w: f32, h: f32 },
    /// roundRect(x, y, w, h, radius)
    RoundRect { x: f32, y: f32, w: f32, h: f32, radius: f32 },
    /// ellipse(cx, cy, rx, ry, rotation, startAngle, endAngle, anticlockwise)
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32, rotation: f32,
              start_angle: f32, end_angle: f32, anticlockwise: bool },
    /// arcTo(x1, y1, x2, y2, radius)
    ArcTo { x1: f32, y1: f32, x2: f32, y2: f32, radius: f32 },
    /// clip() - clip path do region
    Clip,
    /// strokeText(text, x, y)
    StrokeText { text: String, x: f32, y: f32 },
    /// lineCap: butt | round | square
    LineCap(String),
    /// lineJoin: bevel | round | miter
    LineJoin(String),
    /// miterLimit
    MiterLimit(f32),
    /// setLineDash([dash1, dash2, ...])
    LineDash(Vec<f32>),
    /// lineDashOffset
    LineDashOffset(f32),
    /// textAlign: left | right | center | start | end
    TextAlign(String),
    /// textBaseline: top | hanging | middle | alphabetic | ideographic | bottom
    TextBaseline(String),
    /// shadowColor
    ShadowColor([u8; 4]),
    /// shadowBlur (px)
    ShadowBlur(f32),
    /// shadowOffsetX
    ShadowOffsetX(f32),
    /// shadowOffsetY
    ShadowOffsetY(f32),
    /// drawImage(src, dx, dy, dw, dh) - cely obrazek
    DrawImage { src: String, dx: f32, dy: f32, dw: f32, dh: f32 },
    /// drawImage(src, sx, sy, sw, sh, dx, dy, dw, dh) - sub-rect varianta
    DrawImageSrc { src: String, sx: f32, sy: f32, sw: f32, sh: f32,
                   dx: f32, dy: f32, dw: f32, dh: f32 },
    /// fillStyleGradient - linearni gradient (x0, y0, x1, y1) + stops
    FillStyleLinearGradient { x0: f32, y0: f32, x1: f32, y1: f32, stops: Vec<(f32, [u8; 4])> },
    /// fillStyleRadialGradient
    FillStyleRadialGradient { x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32,
                              stops: Vec<(f32, [u8; 4])> },
}

/// Typ gradientu - linear / radial / conic.
#[derive(Debug, Clone)]
pub enum GradientKind {
    /// Linearni gradient. angle_deg: 0=nahoru, 90=doprava, 180=dolu, 270=doleva.
    Linear { angle_deg: f32 },
    /// Radialni gradient od stredu k okraji.
    /// center_pct = (cx, cy) v procentech 0..1, radius = poloomer v px.
    Radial { cx: f32, cy: f32, radius: f32 },
    /// Conic gradient - barva podle uhlu od stredu.
    Conic { cx: f32, cy: f32, start_angle_deg: f32 },
}

#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Solid filled rectangle.
    Rect { x: f32, y: f32, w: f32, h: f32, color: [u8; 4], radius: f32 },
    /// Border (rectangle outline).
    Border { x: f32, y: f32, w: f32, h: f32, width: f32, color: [u8; 4] },
    /// Text rendering.
    Text {
        x: f32, y: f32,
        content: String,
        color: [u8; 4],
        font_size: f32,
        /// CSS font-weight 1..1000. bold field alias pri >= 600.
        font_weight: u32,
        bold: bool,
        /// italic - render pres skew x = 0.2 * y (fake italic).
        italic: bool,
        /// font-family - "" pro default
        font_family: String,
        /// text-decoration line-through (s = 1 line strike).
        strikethrough: bool,
        /// text-decoration underline (u = 1 line under).
        underline: bool,
    },
    /// Linear/radial/conic gradient rect.
    Gradient {
        x: f32, y: f32, w: f32, h: f32,
        kind: GradientKind,
        stops: Vec<(f32, [u8; 4])>,  // (offset 0..1, color)
        radius: f32,
    },
    /// Box shadow rect: smeruje s blur.
    Shadow {
        x: f32, y: f32, w: f32, h: f32,
        offset_x: f32, offset_y: f32,
        blur: f32,
        spread: f32,
        color: [u8; 4],
        radius: f32,
        /// Inset varianta: stin uvnitr boxu (smer fade obraceny).
        inset: bool,
    },
    /// Image - decoded RGBA bytes + dimensions. object_fit + object_position
    /// adjustuji UV/dest mapping pri renderu (cover/contain/fill/none/scale-down).
    Image {
        x: f32, y: f32, w: f32, h: f32,
        src: String,
        radius: f32,
    },
    /// Image s object-fit/object-position (CSS Images L3). Renderer prepocita
    /// inner dst rect dle aspect ratio + position string ("center" / "left top" / "50% 50%").
    ImageFit {
        x: f32, y: f32, w: f32, h: f32,
        src: String,
        radius: f32,
        object_fit: String,    // fill | contain | cover | none | scale-down
        object_position: String,
    },
    /// Blurred solid rect - shader mode 8. Smoothstep edge blur radius.
    BlurredRect { x: f32, y: f32, w: f32, h: f32, color: [u8; 4], radius: f32, blur: f32 },
    /// Marker zacatku filter subtree. Renderer chytne nasledujici commands
    /// (vc nested) az do FilterEnd a vykresli je do offscreen RT s gauss blur
    /// + color matrix transform.
    /// (x, y, w, h) je bbox subtree pro composit + scissor.
    /// blur_radius = 0 znamena bez gauss blur. color_matrix - 4x5 row-major
    /// (identita = no-op color transform).
    FilterBegin {
        x: f32, y: f32, w: f32, h: f32,
        blur_radius: f32,
        color_matrix: [f32; 20],
    },
    /// Konec filter subtree. Parovan s FilterBegin (LIFO stack).
    FilterEnd,
    /// Marker pro backdrop-filter. Renderer snapshotne scenu za elementem,
    /// aplikuje filter (blur + color matrix), composit jako podklad,
    /// pak vykresli inner obsah elementu nahoru.
    BackdropFilterBegin {
        x: f32, y: f32, w: f32, h: f32,
        blur_radius: f32,
        color_matrix: [f32; 20],
    },
    /// Konec backdrop-filter subtree.
    BackdropFilterEnd,
    /// Marker zacatku 3D transform subtree. Renderer chytne nasledujici
    /// commands az do TransformEnd, vykresli je do offscreen RT a slozi
    /// transformovany quad pres compose pipeline s 4x4 matrix.
    /// (x, y, w, h) = bbox (untransformed local rect element).
    /// matrix = 4x4 row-major (vc perspective ancestor).
    TransformBegin {
        x: f32, y: f32, w: f32, h: f32,
        matrix: [f32; 16],
    },
    /// Konec 3D transform subtree.
    TransformEnd,
    /// Marker zacatku mask-image subtree. Renderer vykresli inner obsah do offscreen RT,
    /// pak aplikuje masku (gradient/image) jako alpha multiply, composit zpet.
    /// mask_src: "linear-gradient(...)" nebo "url(...)"
    MaskBegin {
        x: f32, y: f32, w: f32, h: f32,
        mask_src: String,
    },
    /// Konec mask-image subtree.
    MaskEnd,
    /// Rect oriznuty polygonem (CSS clip-path: polygon(...)).
    /// Body jsou absolutni px souradnice. Renderer triangulate via fan
    /// (convex predpoklad). Concave polygon = artefakty.
    ClippedRect {
        color: [u8; 4],
        points: Vec<(f32, f32)>,
    },
    /// Marker: nasledujici commands NEMA byt posunuty pri viewport scroll
    /// shift (position: fixed elementy zustavaji vuci viewportu staticke).
    /// Stack-based - paruje s `NoScrollShiftEnd`. Vnoreni OK (nested fixed).
    /// Inspired by Chromium cc/trees/property_tree_builder.cc - fixed elements
    /// maji jiny scroll tree node nez root scroller.
    NoScrollShiftBegin,
    NoScrollShiftEnd,
    /// mix-blend-mode subtree wrapper. Render subtree do offscreen RT, pak
    /// composite pres shader s blend formula (Multiply/Screen/Overlay/atd).
    /// Inspired by Chromium `core/paint/effect_paint_property_node.cc` blend.
    BlendBegin {
        x: f32, y: f32, w: f32, h: f32,
        mode: u8, // BlendMode enum tag (Normal=0..)
    },
    BlendEnd,
}

/// Vrati display list - sekvence primitiv pro renderer.
pub fn build_display_list(root: &LayoutBox) -> Vec<DisplayCommand> {
    // PERF: pre-alloc 1024 capacity (typicke pages 500-2000 commands).
    let mut commands = Vec::with_capacity(1024);
    paint_box(root, &mut commands, None);
    commands
}

/// Emit main viewport scrollbar overlay. Vola se po build_display_list pri
/// host (WebView) - drive bylo inline v render_via, ted public/testable.
/// Vykresli vertikalni a horizontalni scrollbar Rect pres viewport okraje
/// pokud layout content presahuje viewport. Barvy auto-pick dle body bg
/// (svetla/tmava schema).
pub fn emit_main_scrollbar_overlay(
    layout_root: &LayoutBox,
    display_list: &mut Vec<DisplayCommand>,
    viewport_w: f32, viewport_h: f32,
    scroll_x: f32, scroll_y: f32,
) {
    // Body bg detect pro track/thumb barvy.
    let body_bg_dark = layout_root.children.first().and_then(|html_box| {
        html_box.children.iter().find(|c| c.tag.as_deref() == Some("body"))
    }).and_then(|body| body.bg_color)
        .map(|c| (c[0] as u32 + c[1] as u32 + c[2] as u32) < 384)
        .unwrap_or(false);
    let (track_col, thumb_col) = if body_bg_dark {
        ([55, 55, 65, 220], [140, 140, 150, 255])
    } else {
        ([210, 210, 215, 220], [120, 120, 130, 255])
    };
    let total_h = layout_root.rect.height;
    if total_h > viewport_h {
        let bar_w = 12.0_f32;
        let bar_x = viewport_w - bar_w;
        display_list.push(DisplayCommand::Rect {
            x: bar_x, y: 0.0, w: bar_w, h: viewport_h,
            color: track_col, radius: 0.0,
        });
        let thumb_h = (viewport_h * viewport_h / total_h).max(40.0);
        let max_scroll = (total_h - viewport_h).max(1.0);
        let thumb_y = (scroll_y / max_scroll) * (viewport_h - thumb_h);
        display_list.push(DisplayCommand::Rect {
            x: bar_x + 2.0, y: thumb_y + 2.0,
            w: bar_w - 4.0, h: thumb_h - 4.0,
            color: thumb_col, radius: (bar_w - 4.0) * 0.5,
        });
    }
    let total_w = layout_root.rect.width;
    if total_w > viewport_w {
        let bar_h = 12.0_f32;
        let bar_y = viewport_h - bar_h;
        display_list.push(DisplayCommand::Rect {
            x: 0.0, y: bar_y, w: viewport_w, h: bar_h,
            color: track_col, radius: 0.0,
        });
        let thumb_w = (viewport_w * viewport_w / total_w).max(40.0);
        let max_scroll_x = (total_w - viewport_w).max(1.0);
        let thumb_x = (scroll_x / max_scroll_x) * (viewport_w - thumb_w);
        display_list.push(DisplayCommand::Rect {
            x: thumb_x + 2.0, y: bar_y + 2.0,
            w: thumb_w - 4.0, h: bar_h - 4.0,
            color: thumb_col, radius: (bar_h - 4.0) * 0.5,
        });
    }
    // Inner scrollbar overlay - kazdy nested element s overflow:auto/scroll +
    // content > rect. Devtools panels (.dom-tree, .styles-body, .computed-list)
    // pouzivaji nested overflow - bez tohoto se content cut off bez visual hint
    // ze je scrollovatelne.
    emit_inner_scrollbars(layout_root, display_list, track_col, thumb_col, scroll_x, scroll_y);
}

fn emit_inner_scrollbars(
    bx: &LayoutBox,
    display_list: &mut Vec<DisplayCommand>,
    track_col: [u8; 4],
    thumb_col: [u8; 4],
    scroll_x: f32, scroll_y: f32,
) {
    let _ = scroll_x;
    let _ = scroll_y;
    use crate::browser::scroll::Scrollable;
    let needs_y = bx.needs_scrollbar_y();
    let needs_x = bx.needs_scrollbar_x();
    if needs_y {
        let bar_w = bx.scrollbar_size.max(8.0).min(14.0);
        let bar_x = bx.rect.x + bx.rect.width - bar_w;
        let bar_y = bx.rect.y;
        let bar_h = bx.rect.height;
        display_list.push(DisplayCommand::Rect {
            x: bar_x, y: bar_y, w: bar_w, h: bar_h,
            color: track_col, radius: 0.0,
        });
        if let Some((thumb_off, thumb_h)) = bx.thumb_y(bar_h) {
            display_list.push(DisplayCommand::Rect {
                x: bar_x + 2.0, y: bar_y + thumb_off + 2.0,
                w: bar_w - 4.0, h: (thumb_h - 4.0).max(8.0),
                color: thumb_col, radius: (bar_w - 4.0) * 0.5,
            });
        }
    }
    if needs_x {
        let bar_h = bx.scrollbar_size.max(8.0).min(14.0);
        let bar_x = bx.rect.x;
        let bar_y = bx.rect.y + bx.rect.height - bar_h;
        let bar_w = bx.rect.width - if needs_y { 12.0 } else { 0.0 };
        display_list.push(DisplayCommand::Rect {
            x: bar_x, y: bar_y, w: bar_w, h: bar_h,
            color: track_col, radius: 0.0,
        });
        if let Some((thumb_off, thumb_w)) = bx.thumb_x(bar_w) {
            display_list.push(DisplayCommand::Rect {
                x: bar_x + thumb_off + 2.0, y: bar_y + 2.0,
                w: (thumb_w - 4.0).max(8.0), h: bar_h - 4.0,
                color: thumb_col, radius: (bar_h - 4.0) * 0.5,
            });
        }
    }
    for ch in &bx.children {
        emit_inner_scrollbars(ch, display_list, track_col, thumb_col, scroll_x, scroll_y);
    }
}

// Thread-local viewport pro paint culling. Pred build_display_list_culled
// se nastavi, paint_box pak preskoci elementy mimo.
thread_local! {
    static VIEWPORT_CULL: std::cell::Cell<Option<(f32, f32)>> = const { std::cell::Cell::new(None) };
}

/// Vrati display list s viewport culling.
/// Boxy mimo (scroll_y - 200, scroll_y + viewport_h + 200) se preskocej.
/// Sticky/Fixed/Absolute pozice elementy nikdy nepreskoceny (mohou byt jinde).
pub fn build_display_list_culled(root: &LayoutBox, scroll_y: f32, viewport_h: f32) -> Vec<DisplayCommand> {
    let mut commands = Vec::with_capacity(1024);
    build_display_list_culled_into(root, scroll_y, viewport_h, &mut commands);
    commands
}

/// Reuse buffer variant - clear + fill misto alocace.
pub fn build_display_list_culled_into(root: &LayoutBox, scroll_y: f32, viewport_h: f32, commands: &mut Vec<DisplayCommand>) {
    VIEWPORT_CULL.with(|c| c.set(Some((scroll_y, scroll_y + viewport_h))));
    commands.clear();
    paint_box(root, commands, None);
    VIEWPORT_CULL.with(|c| c.set(None));
}

/// Public access pro per-layer paint - set viewport cull range pred kreseni.
pub fn set_viewport_cull(top: f32, bottom: f32) {
    VIEWPORT_CULL.with(|c| c.set(Some((top, bottom))));
}
pub fn clear_viewport_cull() {
    VIEWPORT_CULL.with(|c| c.set(None));
}

/// Vraci LayoutBox ktery vlastni layer s `layer_id`.
pub fn find_box_by_node_id(root: &LayoutBox, target_id: usize) -> Option<&LayoutBox> {
    if let Some(n) = &root.node {
        if std::rc::Rc::as_ptr(n) as usize == target_id { return Some(root); }
    }
    for ch in &root.children {
        if let Some(f) = find_box_by_node_id(ch, target_id) { return Some(f); }
    }
    None
}

thread_local! {
    /// Aktualne aktivni layer root node_id. Pri paint_box recurse:
    /// pokud child je layer boundary AND child.node_id != LAYER_SCOPE -> skip
    /// (paint ji ve vlastni layer pass). U bezneho monolithic paint je
    /// LAYER_SCOPE = None = walk vsechno (legacy behavior).
    /// Inspired by WebRender Picture/Tile boundary model.
    pub(crate) static LAYER_SCOPE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Paint commands jen pro subtree dane layer (= picture). Nested layer
/// boundaries skip (paintuji separatne ve vlastni pass).
/// Inspired by WebRender Picture build pass.
pub fn paint_layer_into(
    layer_root_box: &LayoutBox,
    cmds: &mut Vec<DisplayCommand>,
) {
    let layer_id = layer_root_box.node.as_ref()
        .map(|n| std::rc::Rc::as_ptr(n) as usize).unwrap_or(0);
    LAYER_SCOPE.with(|c| c.set(Some(layer_id)));
    paint_box(layer_root_box, cmds, None);
    LAYER_SCOPE.with(|c| c.set(None));
}

fn culled_out(bx: &LayoutBox) -> bool {
    let bounds = VIEWPORT_CULL.with(|c| c.get());
    let (vt, vb) = match bounds { Some(b) => b, None => return false };
    let always_visible = matches!(bx.position,
        super::layout::Position::Fixed | super::layout::Position::Sticky
        | super::layout::Position::Absolute);
    if always_visible { return false; }
    if !bx.transforms.is_empty() { return false; } // transforms muzou bbox menit
    let buffer = 200.0;
    let bx_top = bx.rect.y;
    let bx_bot = bx.rect.y + bx.rect.height;
    bx_bot < vt - buffer || bx_top > vb + buffer
}

/// Vypocita clip-path adjusted rect pro element bg/image.
/// Vrati (x, y, w, h, radius) - radius vetsi nez box.border_radius pri circle/ellipse.
fn compute_clip_rect(bx: &LayoutBox) -> (f32, f32, f32, f32, f32) {
    use crate::browser::layout::ClipPath;
    let default = (bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height, bx.border_radius);
    match &bx.clip_path {
        Some(ClipPath::Inset { top, right, bottom, left, radius }) => (
            bx.rect.x + left,
            bx.rect.y + top,
            (bx.rect.width - left - right).max(0.0),
            (bx.rect.height - top - bottom).max(0.0),
            radius.max(bx.border_radius),
        ),
        Some(ClipPath::Circle { cx_pct, cy_pct, radius_pct }) => {
            let cx = bx.rect.x + bx.rect.width  * cx_pct;
            let cy = bx.rect.y + bx.rect.height * cy_pct;
            let half_diag = ((bx.rect.width / 2.0).powi(2) + (bx.rect.height / 2.0).powi(2)).sqrt();
            let r = half_diag * radius_pct;
            (cx - r, cy - r, 2.0 * r, 2.0 * r, r)
        }
        Some(ClipPath::Ellipse { cx_pct, cy_pct, rx_pct, ry_pct }) => {
            let cx = bx.rect.x + bx.rect.width  * cx_pct;
            let cy = bx.rect.y + bx.rect.height * cy_pct;
            let rx = bx.rect.width  * rx_pct;
            let ry = bx.rect.height * ry_pct;
            (cx - rx, cy - ry, 2.0 * rx, 2.0 * ry, rx.min(ry))
        }
        Some(ClipPath::Polygon(_)) => default,  // Polygon vyzaduje shader/stencil
        None => default,
    }
}

/// Capitalize: prvni pismeno kazdeho slova upper.
fn capitalize_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut new_word = true;
    for c in s.chars() {
        if c.is_whitespace() {
            new_word = true;
            out.push(c);
        } else if new_word {
            out.extend(c.to_uppercase());
            new_word = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Serializace LayoutBox SVG subtree na SVG markup string. Pres resvg parse.
fn serialize_svg(bx: &LayoutBox) -> Option<String> {
    let node = bx.node.as_ref()?;
    if node.tag_name().as_deref() != Some("svg") { return None; }
    // Resolve currentColor pres bx.text_color (cascade).
    let cc = bx.text_color.unwrap_or([0, 0, 0, 255]);
    let cc_str = format!("rgb({},{},{})", cc[0], cc[1], cc[2]);
    fn walk(node: &std::rc::Rc<crate::browser::dom::Node>, out: &mut String, current_color: &str) {
        let tag = match node.tag_name() { Some(t) => t, None => return };
        out.push('<');
        out.push_str(&tag);
        for (k, v) in node.attributes.borrow().iter() {
            let v_resolved = if v.eq_ignore_ascii_case("currentcolor") {
                current_color.to_string()
            } else { v.clone() };
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            // Escape minimum
            for c in v_resolved.chars() {
                match c {
                    '"' => out.push_str("&quot;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '&' => out.push_str("&amp;"),
                    _ => out.push(c),
                }
            }
            out.push('"');
        }
        let children = node.children.borrow();
        if children.is_empty() {
            out.push_str("/>");
        } else {
            out.push('>');
            for child in children.iter() {
                walk(child, out, current_color);
            }
            out.push_str("</");
            out.push_str(&tag);
            out.push('>');
        }
    }
    let mut buf = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    // Pridat root attrs (kdyz uz buf opening tag, attr na inner walk - simplify
    // emit svg root manually + recurse children).
    for (k, v) in node.attributes.borrow().iter() {
        if k == "xmlns" { continue; }
        let v_resolved = if v.eq_ignore_ascii_case("currentcolor") {
            cc_str.clone()
        } else { v.clone() };
        buf.push(' ');
        buf.push_str(k);
        buf.push_str("=\"");
        for c in v_resolved.chars() {
            match c {
                '"' => buf.push_str("&quot;"),
                '<' => buf.push_str("&lt;"),
                '>' => buf.push_str("&gt;"),
                '&' => buf.push_str("&amp;"),
                _ => buf.push(c),
            }
        }
        buf.push('"');
    }
    buf.push('>');
    for child in node.children.borrow().iter() {
        walk(child, &mut buf, &cc_str);
    }
    buf.push_str("</svg>");
    Some(buf)
}

/// Render SVG via resvg + tiny-skia (= Chrome/FF-quality vector raster s
/// analytical coverage AA). Cached as RGBA bitmap v thread_local map pres
/// (svg_string_hash, target_w, target_h). Returns image bytes RGBA.
fn rasterize_svg_resvg(svg: &str, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).ok()?;
    let svg_size = tree.size();
    let sx = target_w as f32 / svg_size.width();
    let sy = target_h as f32 / svg_size.height();
    let mut pixmap = tiny_skia::Pixmap::new(target_w, target_h)?;
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.take())
}

/// Inline SVG bitmap cache - klic = svg+dims hash, hodnota = (RGBA bytes, w, h).
/// Render phase volá `flush_inline_svg_cache(&mut image_atlas)` pred build_vertices
/// pres upload do GPU atlas.
thread_local! {
    pub static INLINE_SVG_CACHE: std::cell::RefCell<
        std::collections::HashMap<String, (Vec<u8>, u32, u32)>
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Emituje SVG shape z child <rect>, <circle>, <ellipse>, <line>, <polygon>,
/// <polyline>, <path>, <text>, <g>. Podporuje fill, stroke, transform attribute,
/// viewBox a preserveAspectRatio na root <svg>.
///
/// Strategie: serialize SVG subtree -> resvg parse + tiny-skia rasterize pri
/// target_w/target_h pres physical px (= Chrome/FF analytical coverage AA).
/// Result cached v INLINE_SVG_CACHE pres flush -> image_atlas pred render.
fn emit_svg_children(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>) {
    let zoom = crate::browser::render::current_zoom().unwrap_or(1.0);
    let target_w = (bx.rect.width * zoom).round().max(1.0) as u32;
    let target_h = (bx.rect.height * zoom).round().max(1.0) as u32;
    if target_w > 0 && target_h > 0 && target_w <= 4096 && target_h <= 4096 {
        if let Some(svg_str) = serialize_svg(bx) {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            svg_str.hash(&mut h);
            target_w.hash(&mut h);
            target_h.hash(&mut h);
            let key = format!("__inline_svg_{:x}", h.finish());
            let has_cache = INLINE_SVG_CACHE.with(|c| c.borrow().contains_key(&key));
            if !has_cache {
                if let Some(rgba) = rasterize_svg_resvg(&svg_str, target_w, target_h) {
                    INLINE_SVG_CACHE.with(|c| {
                        c.borrow_mut().insert(key.clone(), (rgba, target_w, target_h));
                    });
                }
            }
            if INLINE_SVG_CACHE.with(|c| c.borrow().contains_key(&key)) {
                cmds.push(DisplayCommand::Image {
                    x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
                    src: key,
                    radius: 0.0,
                });
                return;
            }
        }
    }
    // Fallback custom polygon path.
    emit_svg_children_fallback(bx, cmds);
}

#[allow(dead_code)]
fn emit_svg_children_fallback(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>) {
    // ViewBox parse: "min-x min-y width height".
    let mut xform = [1.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    if let Some(node) = &bx.node {
        if let Some(vb_str) = node.attr("viewBox") {
            let nums: Vec<f32> = vb_str.split(|c: char| c == ',' || c.is_whitespace())
                .filter(|p| !p.is_empty())
                .filter_map(|p| p.parse::<f32>().ok())
                .collect();
            if nums.len() >= 4 && nums[2] > 0.0 && nums[3] > 0.0 {
                let (vx, vy, vw, vh) = (nums[0], nums[1], nums[2], nums[3]);
                let sx = bx.rect.width / vw;
                let sy = bx.rect.height / vh;
                // preserveAspectRatio: default "xMidYMid meet" - uniform scale + center.
                let par = node.attr("preserveAspectRatio").unwrap_or_else(|| "xMidYMid meet".to_string());
                let par = par.trim().to_lowercase();
                let is_none = par.starts_with("none");
                if is_none {
                    // Stretch (non-uniform).
                    xform = [sx, 0.0, 0.0, sy, -vx * sx, -vy * sy];
                } else {
                    let slice = par.contains("slice");
                    let s = if slice { sx.max(sy) } else { sx.min(sy) };
                    // Center alignment (xMidYMid). xMin/xMax + yMin/yMax variants.
                    let aligned_w = vw * s;
                    let aligned_h = vh * s;
                    let mut tx = -vx * s;
                    let mut ty = -vy * s;
                    if par.contains("xmin") {} else if par.contains("xmax") {
                        tx += bx.rect.width - aligned_w;
                    } else { // xmid (default)
                        tx += (bx.rect.width - aligned_w) * 0.5;
                    }
                    if par.contains("ymin") {} else if par.contains("ymax") {
                        ty += bx.rect.height - aligned_h;
                    } else { // ymid
                        ty += (bx.rect.height - aligned_h) * 0.5;
                    }
                    xform = [s, 0.0, 0.0, s, tx, ty];
                }
            }
        }
    }
    emit_svg_children_xform(bx, &xform, cmds);
}

fn emit_svg_children_xform(bx: &LayoutBox, parent_xform: &[f32; 6], cmds: &mut Vec<DisplayCommand>) {
    let node = match &bx.node { Some(n) => n, None => return };
    let origin = (bx.rect.x, bx.rect.y);
    // SVG attribute inheritance per SVG spec: child elementy bez own attr dedi
    // z parent SVG root. Lucide icons: `<svg stroke="currentColor" fill="none">`
    // s `<path>` uvnitr BEZ attrs - dedi stroke+fill ze svg.
    let inherited_stroke = node.attr("stroke");
    let inherited_fill = node.attr("fill");
    let inherited_stroke_w = node.attr("stroke-width").and_then(|v| v.parse::<f32>().ok());
    for child in node.children.borrow().iter() {
        let tag = match child.tag_name() { Some(t) => t, None => continue };
        let attr_f = |name: &str, default: f32| -> f32 {
            child.attr(name).and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let _attr_color = |name: &str, default: [u8;4]| -> [u8;4] {
            child.attr(name).and_then(|v| super::layout::parse_color(&v)).unwrap_or(default)
        };
        // Local transform from "transform" attr.
        let local_xform = child.attr("transform").map(|s| parse_svg_transform(&s)).unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let xform = compose_svg_transform(parent_xform, &local_xform);
        // Transform helper: local SVG-space point -> render-space px.
        let xf = |x: f32, y: f32| {
            let (tx, ty) = apply_svg_transform(&xform, (x, y));
            (origin.0 + tx, origin.1 + ty)
        };
        // "none" znamena nezadno (ne barva). Default fill = black, stroke = none.
        // SVG currentColor keyword -> cascade color (= bx.text_color resolved via
        // CSS color property na SVG element). Bez tohoto lucide-style icons s
        // stroke="currentColor" by render cerne (= parse_color None fallback).
        let current_color = bx.text_color.unwrap_or([0, 0, 0, 255]);
        let resolve_color = |v: &str, default: [u8; 4]| -> [u8; 4] {
            if v.trim().eq_ignore_ascii_case("currentcolor") {
                return current_color;
            }
            super::layout::parse_color(v).unwrap_or(default)
        };
        // Resolve fill: child attr > inherited z svg root > default black.
        let fill_attr = child.attr("fill").or_else(|| inherited_fill.clone());
        let fill_none = fill_attr.as_deref().map(|v| v.trim() == "none").unwrap_or(false);
        let fill = if fill_none { [0; 4] } else {
            fill_attr.as_deref().map(|v| resolve_color(v, [0, 0, 0, 255])).unwrap_or([0, 0, 0, 255])
        };
        // Resolve stroke: child attr > inherited z svg root > default none.
        let stroke_attr = child.attr("stroke").or_else(|| inherited_stroke.clone());
        let stroke_none = stroke_attr.as_deref().map(|v| v.trim() == "none").unwrap_or(true); // default stroke=none
        let stroke_c = stroke_attr.as_deref().map(|v| resolve_color(v, [0, 0, 0, 255])).unwrap_or([0, 0, 0, 255]);
        let stroke_w_raw: f32 = child.attr("stroke-width").and_then(|v| v.parse().ok())
            .or(inherited_stroke_w)
            .unwrap_or(1.0);
        // Per SVG spec: stroke-width is in user-space (= viewBox units). xform
        // scaluje from viewBox -> render. Pres scale-up viewBox (24) -> small
        // SVG box (14), real stroke = stroke_w * scale. Bez tohoto lucide
        // 2px stroke v 24-viewBox renderuje 2px pres render = TLUSTY.
        // Uniform scale assume - average sx/sy z xform matrix.
        let scale_x = (xform[0] * xform[0] + xform[1] * xform[1]).sqrt();
        let scale_y = (xform[2] * xform[2] + xform[3] * xform[3]).sqrt();
        let xf_scale = (scale_x + scale_y) * 0.5;
        let stroke_w = if xf_scale > 0.0 { stroke_w_raw * xf_scale } else { stroke_w_raw };
        match tag.as_str() {
            "rect" => {
                let x = attr_f("x", 0.0);
                let y = attr_f("y", 0.0);
                let w = attr_f("width", 0.0);
                let h = attr_f("height", 0.0);
                let rx = attr_f("rx", 0.0);
                // Rectangle pri identity xform: emit native Rect; jinak polygon.
                let is_identity = (xform[0] - 1.0).abs() < 0.001 && xform[1].abs() < 0.001
                    && xform[2].abs() < 0.001 && (xform[3] - 1.0).abs() < 0.001;
                if is_identity {
                    let (ax, ay) = xf(x, y);
                    if !fill_none {
                        cmds.push(DisplayCommand::Rect { x: ax, y: ay, w, h, color: fill, radius: rx });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        cmds.push(DisplayCommand::Border { x: ax, y: ay, w, h, width: stroke_w, color: stroke_c });
                    }
                } else {
                    let pts = vec![xf(x, y), xf(x+w, y), xf(x+w, y+h), xf(x, y+h)];
                    if !fill_none {
                        cmds.push(DisplayCommand::ClippedRect { color: fill, points: pts.clone() });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        emit_stroked_polyline(&pts, stroke_w, stroke_c, true, cmds);
                    }
                }
            }
            "circle" => {
                let cx = attr_f("cx", 0.0);
                let cy = attr_f("cy", 0.0);
                let r = attr_f("r", 0.0);
                let is_identity = (xform[0] - 1.0).abs() < 0.001 && xform[1].abs() < 0.001
                    && xform[2].abs() < 0.001 && (xform[3] - 1.0).abs() < 0.001;
                if is_identity {
                    let (acx, acy) = xf(cx, cy);
                    if !fill_none {
                        cmds.push(DisplayCommand::Rect {
                            x: acx - r, y: acy - r, w: 2.0*r, h: 2.0*r,
                            color: fill, radius: r,
                        });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        // Border na rounded rect, taky aproximace.
                        cmds.push(DisplayCommand::Border {
                            x: acx - r, y: acy - r, w: 2.0*r, h: 2.0*r,
                            width: stroke_w, color: stroke_c,
                        });
                    }
                } else {
                    // Rotated/scaled circle -> aproximace polygon 32 vertices.
                    let mut pts = Vec::with_capacity(32);
                    for i in 0..32 {
                        let a = i as f32 / 32.0 * std::f32::consts::TAU;
                        pts.push(xf(cx + r * a.cos(), cy + r * a.sin()));
                    }
                    if !fill_none {
                        cmds.push(DisplayCommand::ClippedRect { color: fill, points: pts.clone() });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        emit_stroked_polyline(&pts, stroke_w, stroke_c, true, cmds);
                    }
                }
            }
            "ellipse" => {
                let cx = attr_f("cx", 0.0);
                let cy = attr_f("cy", 0.0);
                let rx = attr_f("rx", 0.0);
                let ry = attr_f("ry", 0.0);
                let is_identity = (xform[0] - 1.0).abs() < 0.001 && xform[1].abs() < 0.001
                    && xform[2].abs() < 0.001 && (xform[3] - 1.0).abs() < 0.001;
                if is_identity {
                    // Identity transform - emit jako Rect s rounded corners
                    // (SDF gives crisp AA edges). border_radius = min(rx, ry)
                    // ne uplne presne pri rx != ry, ale visualne lepsi nez
                    // 32-vertex polygon bez AA. Pri rx == ry je perfect circle.
                    let (acx, acy) = xf(cx, cy);
                    let radius = rx.min(ry);
                    if !fill_none {
                        cmds.push(DisplayCommand::Rect {
                            x: acx - rx, y: acy - ry, w: 2.0*rx, h: 2.0*ry,
                            color: fill, radius,
                        });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        cmds.push(DisplayCommand::Border {
                            x: acx - rx, y: acy - ry, w: 2.0*rx, h: 2.0*ry,
                            width: stroke_w, color: stroke_c,
                        });
                    }
                } else {
                    // Tessellate ellipse jako polygon (transformed).
                    let mut pts = Vec::with_capacity(32);
                    for i in 0..32 {
                        let a = i as f32 / 32.0 * std::f32::consts::TAU;
                        pts.push(xf(cx + rx * a.cos(), cy + ry * a.sin()));
                    }
                    if !fill_none {
                        cmds.push(DisplayCommand::ClippedRect { color: fill, points: pts.clone() });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        emit_stroked_polyline(&pts, stroke_w, stroke_c, true, cmds);
                    }
                }
            }
            "line" => {
                let x1 = attr_f("x1", 0.0);
                let y1 = attr_f("y1", 0.0);
                let x2 = attr_f("x2", 0.0);
                let y2 = attr_f("y2", 0.0);
                let p1 = xf(x1, y1);
                let p2 = xf(x2, y2);
                // Line ma stroke default cerny i bez stroke attr (SVG spec).
                let line_stroke = if stroke_none && stroke_attr.is_none() { false } else { !stroke_none };
                if line_stroke && stroke_w > 0.0 {
                    emit_stroked_segment(p1, p2, stroke_w, stroke_c, cmds);
                }
            }
            "text" => {
                // Note: text transform pres render je out of scope - identity xform OK,
                // jinak placement OK ale glyf neotacime.
                let x = attr_f("x", 0.0);
                let y = attr_f("y", 0.0);
                let (ax, ay) = xf(x, y);
                let font_size = attr_f("font-size", 14.0);
                let content = child.text_content();
                if !content.trim().is_empty() {
                    cmds.push(DisplayCommand::Text {
                        x: ax, y: ay - font_size, content,
                        color: fill, font_size, bold: false, font_weight: 400,
                        italic: false,
                        font_family: String::new(),
                        strikethrough: false, underline: false,
                    });
                }
            }
            "polygon" => {
                let points_str = child.attr("points").unwrap_or_default();
                let raw = parse_svg_points(&points_str);
                if raw.len() >= 3 {
                    let pts: Vec<(f32, f32)> = raw.iter().map(|(x, y)| xf(*x, *y)).collect();
                    if !fill_none {
                        cmds.push(DisplayCommand::ClippedRect { color: fill, points: pts.clone() });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        emit_stroked_polyline(&pts, stroke_w, stroke_c, true, cmds);
                    }
                }
            }
            "polyline" => {
                let points_str = child.attr("points").unwrap_or_default();
                let raw = parse_svg_points(&points_str);
                let pts: Vec<(f32, f32)> = raw.iter().map(|(x, y)| xf(*x, *y)).collect();
                // Polyline default fill je black (nepovolovany ale spec). Pokud chce uzivatel ne,
                // zada fill=none. Tady kdyz fill_none nebo neni explicitne -> jen stroke.
                if !fill_none && fill_attr.is_some() && pts.len() >= 3 {
                    cmds.push(DisplayCommand::ClippedRect { color: fill, points: pts.clone() });
                }
                let line_stroke = if stroke_none && stroke_attr.is_none() { false } else { !stroke_none };
                if line_stroke && stroke_w > 0.0 {
                    emit_stroked_polyline(&pts, stroke_w, stroke_c, false, cmds);
                }
            }
            "path" => {
                let d = child.attr("d").unwrap_or_default();
                let raw = parse_svg_path(&d);
                if !raw.is_empty() {
                    let pts: Vec<(f32, f32)> = raw.iter().map(|(x, y)| xf(*x, *y)).collect();
                    if !fill_none && pts.len() >= 3 {
                        cmds.push(DisplayCommand::ClippedRect { color: fill, points: pts.clone() });
                    }
                    if !stroke_none && stroke_w > 0.0 {
                        emit_stroked_polyline(&pts, stroke_w, stroke_c, false, cmds);
                    }
                }
            }
            "g" => {
                // Group - rekurzivne emit children s vlastnim transform. Atributy fill/stroke
                // by se mely inheritnout, ale to vyzaduje samostatny inheritance walk - skip.
                let mut virt = LayoutBox::new();
                virt.rect = bx.rect;
                virt.node = Some(Rc::clone(child));
                emit_svg_children_xform(&virt, &xform, cmds);
            }
            _ => {}
        }
    }
}

/// Stroke segment (p1,p2) jako rotated quad (4-point ClippedRect).
/// Pro polyline: zavolej per segment + push do cmds. Negarantuje join continuity
/// (mitre/round joins out of scope).
fn emit_stroked_segment(p1: (f32, f32), p2: (f32, f32), width: f32, color: [u8; 4], cmds: &mut Vec<DisplayCommand>) {
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let len = (dx*dx + dy*dy).sqrt();
    if len < 0.001 { return; }
    let half = width * 0.5;
    // Perpendicular normalized.
    let px = -dy / len * half;
    let py =  dx / len * half;
    let pts = vec![
        (p1.0 + px, p1.1 + py),
        (p2.0 + px, p2.1 + py),
        (p2.0 - px, p2.1 - py),
        (p1.0 - px, p1.1 - py),
    ];
    cmds.push(DisplayCommand::ClippedRect { color, points: pts });
}

/// Stroke uzavrene/otevrene polyline. closed=true append (last->first).
fn emit_stroked_polyline(pts: &[(f32, f32)], width: f32, color: [u8; 4], closed: bool, cmds: &mut Vec<DisplayCommand>) {
    if pts.len() < 2 { return; }
    for w in pts.windows(2) {
        emit_stroked_segment(w[0], w[1], width, color, cmds);
    }
    if closed && pts.len() >= 3 {
        emit_stroked_segment(*pts.last().unwrap(), pts[0], width, color, cmds);
    }
}

/// Parsuje "transform" SVG attribute (translate/rotate/scale/matrix). Vrati
/// 2D affine matice [a b c d e f] (row-major: x' = a*x + c*y + e, y' = b*x + d*y + f).
fn parse_svg_transform(s: &str) -> [f32; 6] {
    let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; // identity
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        // Skip whitespace + commas
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b',' || bytes[i] == b'\t') { i += 1; }
        if i >= bytes.len() { break; }
        // Read function name
        let name_start = i;
        while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() { i += 1; }
        let name = &s[name_start..i];
        if name.is_empty() { break; }
        // Skip to '('
        while i < bytes.len() && bytes[i] != b'(' { i += 1; }
        if i >= bytes.len() { break; }
        i += 1;
        // Read args
        let args_start = i;
        while i < bytes.len() && bytes[i] != b')' { i += 1; }
        let args_str = &s[args_start..i];
        let nums: Vec<f32> = args_str.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse::<f32>().ok())
            .collect();
        if i < bytes.len() { i += 1; } // skip ')'
        // Compose s prev m.
        let local = match name {
            "translate" => {
                let tx = *nums.first().unwrap_or(&0.0);
                let ty = *nums.get(1).unwrap_or(&0.0);
                [1.0, 0.0, 0.0, 1.0, tx, ty]
            }
            "scale" => {
                let sx = *nums.first().unwrap_or(&1.0);
                let sy = *nums.get(1).unwrap_or(&sx);
                [sx, 0.0, 0.0, sy, 0.0, 0.0]
            }
            "rotate" => {
                let ang = nums.first().copied().unwrap_or(0.0).to_radians();
                let (s, c) = ang.sin_cos();
                if let (Some(&cx), Some(&cy)) = (nums.get(1), nums.get(2)) {
                    // rotate(angle, cx, cy) = T(cx,cy) * R(angle) * T(-cx,-cy)
                    // Pre-compose into single matrix.
                    let tx = cx - c*cx + s*cy;
                    let ty = cy - s*cx - c*cy;
                    [c, s, -s, c, tx, ty]
                } else {
                    [c, s, -s, c, 0.0, 0.0]
                }
            }
            "skewX" => {
                let ang = nums.first().copied().unwrap_or(0.0).to_radians();
                [1.0, 0.0, ang.tan(), 1.0, 0.0, 0.0]
            }
            "skewY" => {
                let ang = nums.first().copied().unwrap_or(0.0).to_radians();
                [1.0, ang.tan(), 0.0, 1.0, 0.0, 0.0]
            }
            "matrix" => {
                if nums.len() >= 6 { [nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]] }
                else { [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] }
            }
            _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        };
        // m = m * local
        let a = m[0]*local[0] + m[2]*local[1];
        let b = m[1]*local[0] + m[3]*local[1];
        let c = m[0]*local[2] + m[2]*local[3];
        let d = m[1]*local[2] + m[3]*local[3];
        let e = m[0]*local[4] + m[2]*local[5] + m[4];
        let f = m[1]*local[4] + m[3]*local[5] + m[5];
        m = [a, b, c, d, e, f];
    }
    m
}

/// Aplikuje 2D affine matrix na bod.
fn apply_svg_transform(m: &[f32; 6], p: (f32, f32)) -> (f32, f32) {
    (m[0]*p.0 + m[2]*p.1 + m[4], m[1]*p.0 + m[3]*p.1 + m[5])
}

/// Komponuj dve 2D affine matice: a * b.
fn compose_svg_transform(a: &[f32; 6], b: &[f32; 6]) -> [f32; 6] {
    [
        a[0]*b[0] + a[2]*b[1],
        a[1]*b[0] + a[3]*b[1],
        a[0]*b[2] + a[2]*b[3],
        a[1]*b[2] + a[3]*b[3],
        a[0]*b[4] + a[2]*b[5] + a[4],
        a[1]*b[4] + a[3]*b[5] + a[5],
    ]
}

/// Parsuje SVG points attribute: "x1,y1 x2,y2 x3 y3" -> Vec<(f32, f32)>.
fn parse_svg_points(s: &str) -> Vec<(f32, f32)> {
    let nums: Vec<f32> = s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<f32>().ok())
        .collect();
    nums.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect()
}

/// Parsuje SVG path `d` attribut a tesseluje krivky (Bezier, arc) na polyline.
/// Podporovane prikazy:
/// - `M`/`m` move-to (absolute/relative)
/// - `L`/`l` line-to
/// - `H`/`h` horizontal line
/// - `V`/`v` vertical line
/// - `Z`/`z` close path
/// - `C`/`c` cubic Bezier (3 control points)
/// - `S`/`s` smooth cubic (control1 reflection)
/// - `Q`/`q` quadratic Bezier (1 control point)
/// - `T`/`t` smooth quadratic
/// - `A`/`a` elliptical arc (rx ry x-rot large-arc sweep x y)
pub fn parse_svg_path(d: &str) -> Vec<(f32, f32)> {
    /// Subdivide cubic bezier - 16 segmenty (linearne casovane).
    fn cubic_tessellate(p0: (f32,f32), p1: (f32,f32), p2: (f32,f32), p3: (f32,f32), out: &mut Vec<(f32,f32)>) {
        const N: u32 = 16;
        for i in 1..=N {
            let t = i as f32 / N as f32;
            let mt = 1.0 - t;
            let x = mt*mt*mt*p0.0 + 3.0*mt*mt*t*p1.0 + 3.0*mt*t*t*p2.0 + t*t*t*p3.0;
            let y = mt*mt*mt*p0.1 + 3.0*mt*mt*t*p1.1 + 3.0*mt*t*t*p2.1 + t*t*t*p3.1;
            out.push((x, y));
        }
    }
    /// Subdivide quadratic bezier - 12 segmenty.
    fn quad_tessellate(p0: (f32,f32), p1: (f32,f32), p2: (f32,f32), out: &mut Vec<(f32,f32)>) {
        const N: u32 = 12;
        for i in 1..=N {
            let t = i as f32 / N as f32;
            let mt = 1.0 - t;
            let x = mt*mt*p0.0 + 2.0*mt*t*p1.0 + t*t*p2.0;
            let y = mt*mt*p0.1 + 2.0*mt*t*p1.1 + t*t*p2.1;
            out.push((x, y));
        }
    }
    /// Tessellate elliptic arc per SVG implementation notes (W3C SVG 1.1 F.6).
    fn arc_tessellate(p0: (f32,f32), rx: f32, ry: f32, x_rot_deg: f32, large_arc: bool, sweep: bool, p1: (f32,f32), out: &mut Vec<(f32,f32)>) {
        let rx = rx.abs();
        let ry = ry.abs();
        if rx == 0.0 || ry == 0.0 || (p0.0 == p1.0 && p0.1 == p1.1) {
            out.push(p1);
            return;
        }
        let phi = x_rot_deg.to_radians();
        let cos_p = phi.cos();
        let sin_p = phi.sin();
        // Step 1: compute (x1', y1')
        let dx2 = (p0.0 - p1.0) / 2.0;
        let dy2 = (p0.1 - p1.1) / 2.0;
        let x1p =  cos_p * dx2 + sin_p * dy2;
        let y1p = -sin_p * dx2 + cos_p * dy2;
        // Correction of out-of-range radii
        let mut rx = rx;
        let mut ry = ry;
        let lambda = (x1p*x1p) / (rx*rx) + (y1p*y1p) / (ry*ry);
        if lambda > 1.0 {
            let s = lambda.sqrt();
            rx *= s;
            ry *= s;
        }
        // Step 2: compute (cx', cy')
        let sign = if large_arc == sweep { -1.0 } else { 1.0 };
        let sq = ((rx*rx*ry*ry - rx*rx*y1p*y1p - ry*ry*x1p*x1p) / (rx*rx*y1p*y1p + ry*ry*x1p*x1p)).max(0.0);
        let coef = sign * sq.sqrt();
        let cxp = coef * (rx * y1p / ry);
        let cyp = coef * -(ry * x1p / rx);
        // Step 3: compute (cx, cy)
        let cx = cos_p * cxp - sin_p * cyp + (p0.0 + p1.0) / 2.0;
        let cy = sin_p * cxp + cos_p * cyp + (p0.1 + p1.1) / 2.0;
        // Step 4: compute angles
        let ang = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
            let dot = ux*vx + uy*vy;
            let len = (ux*ux+uy*uy).sqrt() * (vx*vx+vy*vy).sqrt();
            let mut a = (dot / len).clamp(-1.0, 1.0).acos();
            if ux*vy - uy*vx < 0.0 { a = -a; }
            a
        };
        let theta1 = ang(1.0, 0.0, (x1p - cxp)/rx, (y1p - cyp)/ry);
        let mut delta_theta = ang((x1p - cxp)/rx, (y1p - cyp)/ry, (-x1p - cxp)/rx, (-y1p - cyp)/ry);
        if !sweep && delta_theta > 0.0 { delta_theta -= 2.0 * std::f32::consts::PI; }
        if sweep && delta_theta < 0.0 { delta_theta += 2.0 * std::f32::consts::PI; }
        // Tessellate - 24 segmenty po 360 stupnich.
        let n = ((delta_theta.abs() / (std::f32::consts::PI / 12.0)).ceil() as u32).max(1);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let theta = theta1 + delta_theta * t;
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            let x = cos_p * (rx * cos_t) - sin_p * (ry * sin_t) + cx;
            let y = sin_p * (rx * cos_t) + cos_p * (ry * sin_t) + cy;
            out.push((x, y));
        }
    }

    let mut pts: Vec<(f32, f32)> = Vec::new();
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut start = (0.0_f32, 0.0_f32);
    // Last control point pro smooth bezier (S, T).
    let mut last_cubic_ctrl: Option<(f32, f32)> = None;
    let mut last_quad_ctrl: Option<(f32, f32)> = None;
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if !c.is_ascii_alphabetic() { i += 1; continue; }
        i += 1;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b',') { i += 1; }
        let mut nums: Vec<f32> = Vec::new();
        while i < bytes.len() && !(bytes[i] as char).is_ascii_alphabetic() {
            let start_idx = i;
            if bytes[i] == b'-' || bytes[i] == b'+' { i += 1; }
            while i < bytes.len() && ((bytes[i] as char).is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b'e' || bytes[i] == b'E'
                                       || ((bytes[i] == b'-' || bytes[i] == b'+') && i > start_idx && (bytes[i-1] == b'e' || bytes[i-1] == b'E'))) {
                i += 1;
            }
            if start_idx < i {
                if let Ok(n) = d[start_idx..i].parse::<f32>() {
                    nums.push(n);
                }
            }
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b',') { i += 1; }
            if i >= bytes.len() || (bytes[i] as char).is_ascii_alphabetic() { break; }
        }
        match c {
            'M' => {
                let mut k = 0;
                if nums.len() >= 2 { x = nums[0]; y = nums[1]; pts.push((x, y)); start = (x, y); k = 2; }
                while k + 1 < nums.len() { x = nums[k]; y = nums[k+1]; pts.push((x, y)); k += 2; }
                last_cubic_ctrl = None; last_quad_ctrl = None;
            }
            'm' => {
                let mut k = 0;
                if nums.len() >= 2 { x += nums[0]; y += nums[1]; pts.push((x, y)); start = (x, y); k = 2; }
                while k + 1 < nums.len() { x += nums[k]; y += nums[k+1]; pts.push((x, y)); k += 2; }
                last_cubic_ctrl = None; last_quad_ctrl = None;
            }
            'L' => {
                let mut k = 0;
                while k + 1 < nums.len() { x = nums[k]; y = nums[k+1]; pts.push((x, y)); k += 2; }
                last_cubic_ctrl = None; last_quad_ctrl = None;
            }
            'l' => {
                let mut k = 0;
                while k + 1 < nums.len() { x += nums[k]; y += nums[k+1]; pts.push((x, y)); k += 2; }
                last_cubic_ctrl = None; last_quad_ctrl = None;
            }
            'H' => { for n in nums { x = n; pts.push((x, y)); } last_cubic_ctrl = None; last_quad_ctrl = None; }
            'h' => { for n in nums { x += n; pts.push((x, y)); } last_cubic_ctrl = None; last_quad_ctrl = None; }
            'V' => { for n in nums { y = n; pts.push((x, y)); } last_cubic_ctrl = None; last_quad_ctrl = None; }
            'v' => { for n in nums { y += n; pts.push((x, y)); } last_cubic_ctrl = None; last_quad_ctrl = None; }
            'Z' | 'z' => { pts.push(start); x = start.0; y = start.1; }
            'C' | 'c' => {
                let mut k = 0;
                while k + 5 < nums.len() {
                    let p0 = (x, y);
                    let (c1, c2, p3) = if c == 'C' {
                        ((nums[k], nums[k+1]), (nums[k+2], nums[k+3]), (nums[k+4], nums[k+5]))
                    } else {
                        ((x+nums[k], y+nums[k+1]), (x+nums[k+2], y+nums[k+3]), (x+nums[k+4], y+nums[k+5]))
                    };
                    cubic_tessellate(p0, c1, c2, p3, &mut pts);
                    x = p3.0; y = p3.1;
                    last_cubic_ctrl = Some(c2);
                    last_quad_ctrl = None;
                    k += 6;
                }
            }
            'S' | 's' => {
                // Smooth cubic - control1 = reflection of last cubic ctrl through current point.
                let mut k = 0;
                while k + 3 < nums.len() {
                    let p0 = (x, y);
                    let c1 = match last_cubic_ctrl {
                        Some(prev) => (2.0*x - prev.0, 2.0*y - prev.1),
                        None => p0,
                    };
                    let (c2, p3) = if c == 'S' {
                        ((nums[k], nums[k+1]), (nums[k+2], nums[k+3]))
                    } else {
                        ((x+nums[k], y+nums[k+1]), (x+nums[k+2], y+nums[k+3]))
                    };
                    cubic_tessellate(p0, c1, c2, p3, &mut pts);
                    x = p3.0; y = p3.1;
                    last_cubic_ctrl = Some(c2);
                    last_quad_ctrl = None;
                    k += 4;
                }
            }
            'Q' | 'q' => {
                let mut k = 0;
                while k + 3 < nums.len() {
                    let p0 = (x, y);
                    let (c1, p2) = if c == 'Q' {
                        ((nums[k], nums[k+1]), (nums[k+2], nums[k+3]))
                    } else {
                        ((x+nums[k], y+nums[k+1]), (x+nums[k+2], y+nums[k+3]))
                    };
                    quad_tessellate(p0, c1, p2, &mut pts);
                    x = p2.0; y = p2.1;
                    last_quad_ctrl = Some(c1);
                    last_cubic_ctrl = None;
                    k += 4;
                }
            }
            'T' | 't' => {
                // Smooth quadratic - control = reflection of last quad ctrl.
                let mut k = 0;
                while k + 1 < nums.len() {
                    let p0 = (x, y);
                    let c1 = match last_quad_ctrl {
                        Some(prev) => (2.0*x - prev.0, 2.0*y - prev.1),
                        None => p0,
                    };
                    let p2 = if c == 'T' {
                        (nums[k], nums[k+1])
                    } else {
                        (x+nums[k], y+nums[k+1])
                    };
                    quad_tessellate(p0, c1, p2, &mut pts);
                    x = p2.0; y = p2.1;
                    last_quad_ctrl = Some(c1);
                    last_cubic_ctrl = None;
                    k += 2;
                }
            }
            'A' | 'a' => {
                // Elliptic arc: rx ry x-axis-rotation large-arc-flag sweep-flag x y
                let mut k = 0;
                while k + 6 < nums.len() {
                    let rx = nums[k];
                    let ry = nums[k+1];
                    let xrot = nums[k+2];
                    let large = nums[k+3] != 0.0;
                    let sweep = nums[k+4] != 0.0;
                    let p0 = (x, y);
                    let p1 = if c == 'A' {
                        (nums[k+5], nums[k+6])
                    } else {
                        (x + nums[k+5], y + nums[k+6])
                    };
                    arc_tessellate(p0, rx, ry, xrot, large, sweep, p1, &mut pts);
                    x = p1.0; y = p1.1;
                    last_cubic_ctrl = None; last_quad_ctrl = None;
                    k += 7;
                }
            }
            _ => {}
        }
    }
    pts
}

fn paint_box(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>, parent_perspective: Option<f32>) {
    // Viewport culling - skip cely subtree mimo viewport (+ buffer).
    if culled_out(bx) { return; }
    // Layer scope check: pri per-layer paint mode (LAYER_SCOPE = Some)
    // nested layer boundary skip - paintuji ve vlastni layer pass.
    // Inspired by WebRender Picture::build descent stop pri Tile boundary.
    let cur_scope = LAYER_SCOPE.with(|c| c.get());
    if let Some(scope_id) = cur_scope {
        if super::compositor::is_layer_boundary(bx) {
            let bx_id = bx.node.as_ref()
                .map(|n| std::rc::Rc::as_ptr(n) as usize).unwrap_or(0);
            if bx_id != scope_id {
                return; // skip nested layer - own pass
            }
        }
    }
    // Debug breakpoint hook: BP_TAG/BP_ID/BP_CLASS env vars + IDE breakpoint
    // na `breakpoint_paint` v src/debug_bp.rs.
    if crate::debug_bp::bp_enabled() {
        let tag = bx.tag.as_deref().unwrap_or("");
        let id = bx.node.as_ref().and_then(|n| n.attr("id")).unwrap_or_default();
        let class = bx.node.as_ref().and_then(|n| n.attr("class")).unwrap_or_default();
        if crate::debug_bp::bp_match(tag, &id, &class) {
            crate::debug_bp::breakpoint_paint();
        }
    }
    // Index PRED jakymkoliv emit pro tento box - transform 2D apply pres
    // cmds[box_start..] (vse co tento box vyemituje vc. children).
    let box_start = cmds.len();
    // Position: fixed elementy zustavaji staticke vuci viewportu - obal subtree
    // do NoScrollShiftBegin/End markers, pak viewport scroll shift respektuje.
    let is_fixed = matches!(bx.position, super::layout::Position::Fixed);
    if is_fixed {
        cmds.push(DisplayCommand::NoScrollShiftBegin);
    }
    // mix-blend-mode subtree wrap. Pres non-Normal mode renderer composite
    // pres shader s blend formula.
    use super::computed_style::BlendMode;
    let blend_active = !matches!(bx.mix_blend_mode, BlendMode::Normal);
    if blend_active {
        let mode_tag: u8 = match bx.mix_blend_mode {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
            BlendMode::Darken => 4,
            BlendMode::Lighten => 5,
            BlendMode::ColorDodge => 6,
            BlendMode::ColorBurn => 7,
            BlendMode::HardLight => 8,
            BlendMode::SoftLight => 9,
            BlendMode::Difference => 10,
            BlendMode::Exclusion => 11,
            BlendMode::Hue => 12,
            BlendMode::Saturation => 13,
            BlendMode::Color => 14,
            BlendMode::Luminosity => 15,
        };
        cmds.push(DisplayCommand::BlendBegin {
            x: bx.rect.x, y: bx.rect.y,
            w: bx.rect.width, h: bx.rect.height,
            mode: mode_tag,
        });
    }
    // Detekce 3D transformu - pokud ano, obal cely emit do TransformBegin/End
    // a vynech CPU post-process transformaci (renderer aplikuje matrix shader-side).
    let needs_3d = crate::browser::layout::needs_3d_pipeline(&bx.transforms, parent_perspective);
    if needs_3d {
        let m = crate::browser::layout::compute_transform_matrix(&bx.transforms, parent_perspective);
        cmds.push(DisplayCommand::TransformBegin {
            x: bx.rect.x,
            y: bx.rect.y,
            w: bx.rect.width,
            h: bx.rect.height,
            matrix: m,
        });
    }
    // Predame own perspective do children (s fallbackem na parent)
    let child_perspective = bx.perspective.or(parent_perspective);

    // PERF: vetsina boxu (99%+) nema filter ani backdrop_filter. Fast-path
    // bail bez iter/sum kdyz oba empty.
    let alpha_mul = (bx.opacity * 255.0) as u8;
    let filter_empty = bx.filter.is_empty();
    let backdrop_empty = bx.backdrop_filter.is_empty();
    let blur_radius: f32 = if filter_empty { 0.0 } else {
        bx.filter.iter().filter_map(|op| match op {
            crate::browser::layout::FilterOp::Blur(r) => Some(*r),
            _ => None,
        }).sum()
    };

    // Backdrop-filter: outer marker - snapshotne scenu, pak element obsah nahoru.
    let backdrop_blur: f32 = if backdrop_empty { 0.0 } else {
        bx.backdrop_filter.iter().filter_map(|op| match op {
            crate::browser::layout::FilterOp::Blur(r) => Some(*r),
            _ => None,
        }).sum()
    };
    let backdrop_matrix = if backdrop_empty {
        // Identity bez compose loop.
        [1.0, 0.0, 0.0, 0.0, 0.0,
         0.0, 1.0, 0.0, 0.0, 0.0,
         0.0, 0.0, 1.0, 0.0, 0.0,
         0.0, 0.0, 0.0, 1.0, 0.0]
    } else {
        crate::browser::layout::compute_color_matrix(&bx.backdrop_filter)
    };
    let has_backdrop_filter = !backdrop_empty;
    if has_backdrop_filter {
        let pad = 2.0 * backdrop_blur;
        cmds.push(DisplayCommand::BackdropFilterBegin {
            x: bx.rect.x - pad,
            y: bx.rect.y - pad,
            w: bx.rect.width  + 2.0 * pad,
            h: bx.rect.height + 2.0 * pad,
            blur_radius: if backdrop_blur >= 0.5 { backdrop_blur } else { 0.0 },
            color_matrix: backdrop_matrix,
        });
    }

    // Filter subtree: emit FilterBegin marker pokud chain obsahuje neco
    // co RT pipeline umi - blur (run_blur_passes) NEBO non-identity color
    // matrix (compose shader). Bbox se rozsiri o 2*blur_radius.
    // PERF: fast-path - skip compute pri prazdnem chainu (vetsina elementu).
    let (color_matrix, needs_color);
    if filter_empty {
        color_matrix = [1.0, 0.0, 0.0, 0.0, 0.0,
                        0.0, 1.0, 0.0, 0.0, 0.0,
                        0.0, 0.0, 1.0, 0.0, 0.0,
                        0.0, 0.0, 0.0, 1.0, 0.0];
        needs_color = false;
    } else {
        color_matrix = crate::browser::layout::compute_color_matrix(&bx.filter);
        needs_color = !crate::browser::layout::is_identity_matrix(&color_matrix);
    }
    let needs_blur = blur_radius >= 0.5;
    let has_subtree_filter = needs_blur || needs_color;
    if has_subtree_filter {
        let pad = 2.0 * blur_radius;
        cmds.push(DisplayCommand::FilterBegin {
            x: bx.rect.x - pad,
            y: bx.rect.y - pad,
            w: bx.rect.width  + 2.0 * pad,
            h: bx.rect.height + 2.0 * pad,
            blur_radius: if needs_blur { blur_radius } else { 0.0 },
            color_matrix,
        });
    }

    // mask-image: obal cely emit element obsahu do MaskBegin/MaskEnd
    let has_mask = bx.mask_image.is_some();
    if has_mask {
        let mask_src = bx.mask_image.as_deref().unwrap_or("").to_string();
        cmds.push(DisplayCommand::MaskBegin {
            x: bx.rect.x, y: bx.rect.y,
            w: bx.rect.width, h: bx.rect.height,
            mask_src,
        });
    }

    // Clip-path: vypocita modifikaci box rectu pro emit Rect/Image.
    // Single element clip (CPU side) - inset zmensi rect, circle/ellipse pridaji
    // radius. Polygon zatim no-op.
    let (clip_x, clip_y, clip_w, clip_h, clip_radius) = compute_clip_rect(bx);

    let with_alpha = |c: [u8; 4]| -> [u8; 4] {
        let a = ((c[3] as u16 * alpha_mul as u16) / 255) as u8;
        let after_alpha = [c[0], c[1], c[2], a];
        // Subtree filtry resi RT pipeline + compose shader -> CPU chain skip
        // (jinak by se aplikoval dvakrat). Pro elementy bez subtree filteru
        // (napr. pouze drop-shadow) ponechame CPU chain.
        if bx.filter.is_empty() || has_subtree_filter {
            after_alpha
        } else {
            crate::browser::layout::apply_filter_chain(after_alpha, &bx.filter)
        }
    };

    // Filter drop-shadow - emit shadow pred bg (per CSS spec).
    // PERF: iterate &bx.filter primo bez Vec collect.
    for op in &bx.filter {
        if let crate::browser::layout::FilterOp::DropShadow { ox, oy, blur, color } = op {
            cmds.push(DisplayCommand::Shadow {
                x: bx.rect.x + ox,
                y: bx.rect.y + oy,
                w: bx.rect.width,
                h: bx.rect.height,
                offset_x: *ox, offset_y: *oy,
                blur: *blur, spread: 0.0,
                color: *color,
                radius: bx.border_radius,
                inset: false,
            });
        }
    }
    // Box shadow - emit pred bg.
    // Inset: shadow uvnitr boxu, ne vne. Bbox = box, ne expanded.
    if let Some((ox, oy, blur, spread, color, inset)) = bx.box_shadow {
        let (sx, sy, sw, sh) = if inset {
            (bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height)
        } else {
            (bx.rect.x + ox - spread, bx.rect.y + oy - spread,
             bx.rect.width + 2.0 * spread, bx.rect.height + 2.0 * spread)
        };
        cmds.push(DisplayCommand::Shadow {
            x: sx, y: sy, w: sw, h: sh,
            offset_x: ox,
            offset_y: oy,
            blur,
            spread,
            color: with_alpha(color),
            radius: bx.border_radius,
            inset,
        });
    }

    // Image - emit Image command (img tag - cover boxu). Object-fit/position
    // emit ImageFit pokud nastaveny (renderer prepocita src/dest mapping).
    //
    // HTML `loading=lazy`: skip emit pri boxu mimo viewport + lazy margin.
    // Spec: https://html.spec.whatwg.org/multipage/urls-and-fetching.html#lazy-loading-attributes
    // Bez emit -> render skip load_image_as fetch -> data setrime.
    // Cache cur viewport-cull range; pri lazy nasobime margin (~1250px = Chrome default).
    let lazy_skip = if bx.loading_lazy {
        VIEWPORT_CULL.with(|c| match c.get() {
            Some((top, bottom)) => {
                const LAZY_MARGIN: f32 = 1250.0;
                let b_top = bx.rect.y;
                let b_bot = bx.rect.y + bx.rect.height;
                b_bot < top - LAZY_MARGIN || b_top > bottom + LAZY_MARGIN
            }
            None => false,
        })
    } else { false };
    if let Some(src) = &bx.image_src {
        if !lazy_skip {
            let needs_fit = !matches!(bx.object_fit, crate::browser::layout::ObjectFit::Fill);
            if needs_fit {
                cmds.push(DisplayCommand::ImageFit {
                    x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
                    src: src.clone(),
                    radius: bx.border_radius,
                    object_fit: bx.object_fit.to_string(),
                    object_position: bx.object_position.trim().to_string(),
                });
            } else {
                cmds.push(DisplayCommand::Image {
                    x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
                    src: src.clone(),
                    radius: bx.border_radius,
                });
            }
        }
    }

    // <video> placeholder pri chybejicim posteru: tmavy box + play triangle.
    if bx.tag.as_deref() == Some("video") && bx.image_src.is_none() {
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
            color: [16, 16, 20, 255], radius: bx.border_radius,
        });
        // Play triangle uprostred - 3-bod polygon.
        let cx = bx.rect.x + bx.rect.width * 0.5;
        let cy = bx.rect.y + bx.rect.height * 0.5;
        let s = bx.rect.width.min(bx.rect.height) * 0.15;
        let pts = vec![
            (cx - s * 0.5, cy - s),
            (cx + s, cy),
            (cx - s * 0.5, cy + s),
        ];
        cmds.push(DisplayCommand::ClippedRect {
            color: [255, 255, 255, 200], points: pts,
        });
    }
    // <select> dropdown: rounded box + selected text uvnitr + chevron arrow vpravo.
    if bx.tag.as_deref() == Some("select") {
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
            color: [255, 255, 255, 255], radius: 4.0,
        });
        cmds.push(DisplayCommand::Border {
            x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
            width: 1.0, color: [160, 160, 170, 255],
        });
        // Chevron triangle vpravo.
        let cx = bx.rect.x + bx.rect.width - 12.0;
        let cy = bx.rect.y + bx.rect.height * 0.5;
        let s = 4.0;
        let pts = vec![
            (cx - s, cy - s * 0.5),
            (cx + s, cy - s * 0.5),
            (cx, cy + s * 0.7),
        ];
        cmds.push(DisplayCommand::ClippedRect {
            color: [80, 80, 90, 255], points: pts,
        });
    }
    // <progress>: pozadi + fill dle value/max attrs.
    if bx.tag.as_deref() == Some("progress") {
        let value = bx.node.as_ref().and_then(|n| n.attr("value")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let max = bx.node.as_ref().and_then(|n| n.attr("max")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0).max(0.0001);
        let frac = (value / max).clamp(0.0, 1.0);
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
            color: [220, 220, 225, 255], radius: bx.rect.height * 0.3,
        });
        if frac > 0.0 {
            cmds.push(DisplayCommand::Rect {
                x: bx.rect.x, y: bx.rect.y, w: bx.rect.width * frac, h: bx.rect.height,
                color: [80, 130, 240, 255], radius: bx.rect.height * 0.3,
            });
        }
    }
    // <meter>: pozadi + fill dle value/min/max. Color zalezi na low/high/optimum.
    if bx.tag.as_deref() == Some("meter") {
        let value = bx.node.as_ref().and_then(|n| n.attr("value")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let min_v = bx.node.as_ref().and_then(|n| n.attr("min")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let max_v = bx.node.as_ref().and_then(|n| n.attr("max")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        let low = bx.node.as_ref().and_then(|n| n.attr("low")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(min_v);
        let high = bx.node.as_ref().and_then(|n| n.attr("high")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(max_v);
        let range = (max_v - min_v).max(0.0001);
        let frac = ((value - min_v) / range).clamp(0.0, 1.0);
        // Barva: cervena pri value < low nebo > high, jinak zelena.
        let fill_color = if value < low || value > high {
            [240, 80, 80, 255]
        } else {
            [80, 200, 100, 255]
        };
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
            color: [220, 220, 225, 255], radius: bx.rect.height * 0.3,
        });
        if frac > 0.0 {
            cmds.push(DisplayCommand::Rect {
                x: bx.rect.x, y: bx.rect.y, w: bx.rect.width * frac, h: bx.rect.height,
                color: fill_color, radius: bx.rect.height * 0.3,
            });
        }
    }
    // <audio> placeholder controls bar.
    if bx.tag.as_deref() == Some("audio") {
        // Bar pozadi.
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
            color: [240, 240, 245, 255], radius: bx.rect.height * 0.5,
        });
        // Play icon (kruh) na levem konci.
        let icon_size = bx.rect.height * 0.7;
        let icon_x = bx.rect.x + bx.rect.height * 0.15;
        let icon_y = bx.rect.y + (bx.rect.height - icon_size) * 0.5;
        cmds.push(DisplayCommand::Rect {
            x: icon_x, y: icon_y, w: icon_size, h: icon_size,
            color: [80, 80, 90, 255], radius: icon_size * 0.5,
        });
        // Play triangle uvnitr kruhu.
        let cx = icon_x + icon_size * 0.5;
        let cy = icon_y + icon_size * 0.5;
        let s = icon_size * 0.25;
        let pts = vec![
            (cx - s * 0.4, cy - s),
            (cx + s * 0.7, cy),
            (cx - s * 0.4, cy + s),
        ];
        cmds.push(DisplayCommand::ClippedRect {
            color: [255, 255, 255, 255], points: pts,
        });
        // Progress track.
        let track_x = icon_x + icon_size + bx.rect.height * 0.3;
        let track_y = bx.rect.y + bx.rect.height * 0.45;
        let track_w = (bx.rect.x + bx.rect.width) - track_x - bx.rect.height * 0.3;
        let track_h = bx.rect.height * 0.1;
        if track_w > 0.0 {
            cmds.push(DisplayCommand::Rect {
                x: track_x, y: track_y, w: track_w, h: track_h,
                color: [200, 200, 210, 255], radius: track_h * 0.5,
            });
        }
    }

    // Background-clip: text - skip box bg paint, text se sam renderuje s bg color/gradient.
    // (Plna implementace by vyzadovala SDF text mask compose; zatim aspon override text color z bg.)
    let any_bg_clip_text = bx.backgrounds.iter().any(|l| matches!(l.clip, crate::browser::layout::BgBox::Text));
    if any_bg_clip_text {
        // Nasledujici background paint preskocit, ale aplikuj barvu na text override (pokud bg layer ma color)
        // Text rendering happens later v paint_inline_or_text - color je v bx.color.
        // Override: pokud existuje bg layer s color, pouzij ji jako text fill.
        if let Some(c) = bx.backgrounds.iter().rev().find_map(|l| l.color) {
            // Side-effect skrz bx neni pristupny zde (immutable). Pouzijeme alternativni mechanismus -
            // text rendering uvidi bx.background_clip_v == "text" a precte bg color jeste z layers.
            let _ = c;
        }
    }

    // Background layers (Backgrounds L3): renderuj bottom-to-top (reversed).
    // Kazdy layer muze mit: gradient, image url, solid color (jen posledni layer).
    // Pouzivame bx.backgrounds (Vec<BgLayer>) - parser uz rozdelil comma-sep do layeru.
    use crate::browser::layout::BgRepeat;
    if any_bg_clip_text {
        // skip vsechny bg layery
    } else {
    for layer in bx.backgrounds.iter().rev() {
        // Solid color pozadi (jen na poslednim/spodnim layeru, parser to zajistuje).
        // Pouziva clip_x/y/w/h/radius stejne jako stara bg_color cesta (circle/ellipse/inset).
        if let Some(bg) = layer.color {
            if let Some(crate::browser::layout::ClipPath::Polygon(pct_pts)) = &bx.clip_path {
                let abs_pts: Vec<(f32, f32)> = pct_pts.iter().map(|(xp, yp)| {
                    (bx.rect.x + bx.rect.width * xp, bx.rect.y + bx.rect.height * yp)
                }).collect();
                if abs_pts.len() >= 3 {
                    cmds.push(DisplayCommand::ClippedRect { color: with_alpha(bg), points: abs_pts });
                }
            } else {
                cmds.push(DisplayCommand::Rect {
                    x: clip_x, y: clip_y, w: clip_w, h: clip_h,
                    color: with_alpha(bg), radius: clip_radius,
                });
            }
        }
        // Gradient layer
        if let Some(g) = &layer.gradient {
            use crate::browser::layout::BgGradientKind;
            let kind = match g.kind {
                BgGradientKind::Linear { angle_deg } => GradientKind::Linear { angle_deg },
                BgGradientKind::Radial { cx_pct, cy_pct, radius_pct } => {
                    let cx = bx.rect.x + bx.rect.width  * cx_pct;
                    let cy = bx.rect.y + bx.rect.height * cy_pct;
                    let half_diag = ((bx.rect.width / 2.0).powi(2) + (bx.rect.height / 2.0).powi(2)).sqrt();
                    GradientKind::Radial { cx, cy, radius: half_diag * radius_pct }
                }
                BgGradientKind::Conic { cx_pct, cy_pct, start_angle_deg } => {
                    let cx = bx.rect.x + bx.rect.width  * cx_pct;
                    let cy = bx.rect.y + bx.rect.height * cy_pct;
                    GradientKind::Conic { cx, cy, start_angle_deg }
                }
            };
            cmds.push(DisplayCommand::Gradient {
                x: bx.rect.x, y: bx.rect.y, w: bx.rect.width, h: bx.rect.height,
                kind,
                stops: g.stops.iter().map(|(o, c)| (*o, with_alpha(*c))).collect(),
                radius: bx.border_radius,
            });
        }
        // Image url layer
        if let Some(src) = &layer.image_src {
            let (img_w, img_h) = compute_bg_size(&layer.size, bx.rect.width, bx.rect.height);
            let (img_x, img_y) = compute_bg_position(&layer.position, bx.rect.width, bx.rect.height,
                                                     img_w, img_h, bx.rect.x, bx.rect.y);
            let (rep_x, rep_y) = match layer.repeat {
                BgRepeat::NoRepeat => (1, 1),
                BgRepeat::RepeatX => ((bx.rect.width / img_w).ceil() as i32 + 1, 1),
                BgRepeat::RepeatY => (1, (bx.rect.height / img_h).ceil() as i32 + 1),
                _ => (
                    (bx.rect.width / img_w).ceil() as i32 + 1,
                    (bx.rect.height / img_h).ceil() as i32 + 1,
                ),
            };
            for ix in 0..rep_x {
                for iy in 0..rep_y {
                    let tx = img_x + (ix as f32) * img_w;
                    let ty = img_y + (iy as f32) * img_h;
                    if tx + img_w < bx.rect.x || tx > bx.rect.x + bx.rect.width
                        || ty + img_h < bx.rect.y || ty > bx.rect.y + bx.rect.height {
                        continue;
                    }
                    cmds.push(DisplayCommand::Image {
                        x: tx, y: ty, w: img_w, h: img_h,
                        src: src.clone(),
                        radius: bx.border_radius,
                    });
                }
            }
        }
    }
    } // any_bg_clip_text else

    // bx.bg_gradient + bx.bg_color: shorthand bez backgrounds vec.
    // Skip bg_color pokud backgrounds loop uz barvu vykreslil.
    let bg_color_handled_by_layers = bx.backgrounds.iter().any(|l| l.color.is_some());

    // Background gradient ma prioritu pred solid color
    if let Some(g) = &bx.bg_gradient {
        use crate::browser::layout::BgGradientKind;
        let kind = match g.kind {
            BgGradientKind::Linear { angle_deg } => GradientKind::Linear { angle_deg },
            BgGradientKind::Radial { cx_pct, cy_pct, radius_pct } => {
                let cx = bx.rect.x + bx.rect.width  * cx_pct;
                let cy = bx.rect.y + bx.rect.height * cy_pct;
                // Polomer = farthest-corner * radius_pct
                let half_diag = ((bx.rect.width / 2.0).powi(2) + (bx.rect.height / 2.0).powi(2)).sqrt();
                let radius = half_diag * radius_pct;
                GradientKind::Radial { cx, cy, radius }
            }
            BgGradientKind::Conic { cx_pct, cy_pct, start_angle_deg } => {
                let cx = bx.rect.x + bx.rect.width  * cx_pct;
                let cy = bx.rect.y + bx.rect.height * cy_pct;
                GradientKind::Conic { cx, cy, start_angle_deg }
            }
        };
        cmds.push(DisplayCommand::Gradient {
            x: bx.rect.x,
            y: bx.rect.y,
            w: bx.rect.width,
            h: bx.rect.height,
            kind,
            stops: g.stops.iter().map(|(o, c)| (*o, with_alpha(*c))).collect(),
            radius: bx.border_radius,
        });
    } else if let Some(bg) = bx.bg_color.filter(|_| !bg_color_handled_by_layers) {
        // Polygon clip-path: emit ClippedRect misto Rect.
        // Renderer aplikuje fan triangulation (convex polygon assumption).
        if let Some(crate::browser::layout::ClipPath::Polygon(pct_pts)) = &bx.clip_path {
            let abs_pts: Vec<(f32, f32)> = pct_pts.iter().map(|(xp, yp)| {
                (bx.rect.x + bx.rect.width * xp, bx.rect.y + bx.rect.height * yp)
            }).collect();
            if abs_pts.len() >= 3 {
                cmds.push(DisplayCommand::ClippedRect {
                    color: with_alpha(bg),
                    points: abs_pts,
                });
            }
        } else {
            // Pokud je has_blur_subtree, RT pipeline blur aplikuje na cely subtree
            // -> emitujem normalni Rect. BlurredRect (mode 8) je legacy fallback,
            // pouzity jen kdyz neni RT pipeline (napr. pri error).
            cmds.push(DisplayCommand::Rect {
                x: clip_x, y: clip_y, w: clip_w, h: clip_h,
                color: with_alpha(bg), radius: clip_radius,
            });
        }
    }

    // Border - jen pri specifikovanem border-style != "none". CSS UA default
    // border-style: none -> bez ohledu na width/color border se NEKRESLI.
    // border shorthand "1px solid red" parsuje style v cascade::expand_shorthand.
    let border_visible = !bx.border_style.is_empty()
        && bx.border_style != "none"
        && bx.border_style != "hidden";
    if bx.border_width > 0.0 && border_visible {
        if let Some(bc) = bx.border_color {
            // Pres border-radius > 0: emit rounded border ring via 2 rounded
            // rects (outer = border color, inner inset = transparent cut).
            // Pres border-radius = 0: 4 sharp strips (= original Border path).
            // Bez tohoto pres rounded box border rendered sharp = corner clip.
            if bx.border_radius > 0.5 {
                let bw = bx.border_width;
                // Emit outer rounded rect s border color - ON TOP of bg.
                // Bg uz emitnut s radius == border_radius. Outer kresli pres
                // bg = bg overwriten v ring area. Then INNER inset rounded
                // rect bg overlay restores fill inside.
                cmds.push(DisplayCommand::Rect {
                    x: bx.rect.x, y: bx.rect.y,
                    w: bx.rect.width, h: bx.rect.height,
                    color: with_alpha(bc),
                    radius: bx.border_radius,
                });
                // Inner bg rect (inset by border_width, reduced radius).
                if let Some(bg) = bx.bg_color {
                    let inner_radius = (bx.border_radius - bw).max(0.0);
                    cmds.push(DisplayCommand::Rect {
                        x: bx.rect.x + bw, y: bx.rect.y + bw,
                        w: (bx.rect.width - 2.0 * bw).max(0.0),
                        h: (bx.rect.height - 2.0 * bw).max(0.0),
                        color: with_alpha(bg),
                        radius: inner_radius,
                    });
                }
            } else {
                cmds.push(DisplayCommand::Border {
                    x: bx.rect.x,
                    y: bx.rect.y,
                    w: bx.rect.width,
                    h: bx.rect.height,
                    width: bx.border_width,
                    color: with_alpha(bc),
                });
            }
        }
    }

    // Outline (mimo border, posunuto o offset, neovlivnuje layout)
    if bx.outline_width > 0.0 && bx.outline_style != "none" && !bx.outline_style.is_empty() {
        if let Some(oc) = bx.outline_color {
            let off = bx.outline_offset;
            cmds.push(DisplayCommand::Border {
                x: bx.rect.x - bx.outline_width - off,
                y: bx.rect.y - bx.outline_width - off,
                w: bx.rect.width + 2.0 * (bx.outline_width + off),
                h: bx.rect.height + 2.0 * (bx.outline_width + off),
                width: bx.outline_width,
                color: with_alpha(oc),
            });
        }
    }

    // Text - aplikuj text_align: x posun podle align
    if let Some(text) = &bx.text {
        // text-transform aplikace pred mereni
        let text_owned: String;
        let text: &str = match bx.text_transform {
            crate::browser::layout::TextTransform::None => text.as_str(),
            crate::browser::layout::TextTransform::Uppercase => {
                text_owned = text.to_uppercase();
                &text_owned
            }
            crate::browser::layout::TextTransform::Lowercase => {
                text_owned = text.to_lowercase();
                &text_owned
            }
            crate::browser::layout::TextTransform::Capitalize => {
                text_owned = capitalize_words(text);
                &text_owned
            }
        };
        let text = text.to_string();
        let text = &text;
        // Family + bold + italic aware - bez tohoto text-align center bold
        // text pres measure_text_width (Times Regular metrics) misaligned vs
        // render Ubuntu Bold (sirsi glyfy) -> centering offset spatne.
        let text_w = super::layout::measure_text_width_full(text, bx.font_size, bx.effective_weight(), bx.italic, &bx.font_family, bx.letter_spacing);
        // Asymmetric padding: padding-left wins pro text x-pozici, padding-top
        // pro text y-pozici. Pri "padding: 2px 8px" potrebujeme 8 horizontalni
        // a 2 vertikalni.
        let pad_l = bx.padding_left.unwrap_or(bx.padding);
        let pad_r = bx.padding_right.unwrap_or(bx.padding);
        let pad_t = bx.padding_top.unwrap_or(bx.padding);
        let pad_b = bx.padding_bottom.unwrap_or(bx.padding);
        // inner_w pres asymmetric padding (drive 2 * bx.padding shorthand chyba
        // pri asymmetric -> text-align center off).
        let inner_w = bx.rect.width - pad_l - pad_r;
        let align_offset = match bx.text_align {
            TextAlign::Left | TextAlign::Justify => 0.0,
            TextAlign::Center => ((inner_w - text_w) * 0.5).max(0.0),
            TextAlign::Right  => (inner_w - text_w).max(0.0),
        };
        let border = bx.border_width.max(0.0);
        // text_x = vnejsi rect + border + padding_left (CSS box-model).
        // Bez border posunu by text byl ulozeny na border edge = posunuty
        // o border_width doleva od skutecne content area.
        let text_x = bx.rect.x + border + pad_l + align_offset;
        // Vertical centering: CSS line-box leading split. Pri natural
        // line_height ~= 1.2 * font_size (default leading 0.2*font_size)
        // glyph je natural top - render pen_y/bearing_y resi baseline.
        // Pri explicit "line-height: 60px" >> natural je extra leading
        // ktery half-spli nad/pod glyf -> text vertikalne centered v
        // line boxu. Pak pri inner_h matching line-height = single line
        // vertically centered v boxu (CSS technique).
        // Vertical centering: text v inner box (rect.height - pad_t - pad_b).
        // Driv pouzival line_height_px; ale pri inline elementu s padding (badge
        // .anim-box, button) je rect.height vypoctena z parent line_height_default,
        // ne z elementu vlastniho line_height -> mismatch -> text nahore.
        // Spravne: pen_y na inner_center adjustovany o ascender (~0.8*font_size).
        // Pro spravne center: visible glyph extent ~0.9*fs (cap_height 0.7 +
        // descender 0.2). Pro center text v inner_h:
        //   pen_y = inner_top + (inner_h - 0.9*fs)/2 + cap_height
        // Render dela pen_y = text_y + fs (text_y = inner_top - shift):
        //   shift = fs - cap_height - (inner_h - 0.9*fs)/2 = 0.3*fs - (inner_h - 0.9*fs)/2
        //   v_offset = -shift = (inner_h - 0.9*fs)/2 - 0.3*fs = (inner_h - 1.5*fs)/2
        // BEZ clampu na 0 - pri inner_h < 1.5*fs negative shift posune text
        // do pad_t area (badge/highlight maly inner = mensi clam). Akceptace.
        let inner_h = bx.rect.height - pad_t - pad_b - 2.0 * border;
        // Vertical centering: jen kdyz inner_h substantialne > advance_h (=
        // box vyssi nez line, tj. button/badge/td s extra space). Pri
        // single-line text v normalnim flow (rect.height ≈ advance_h) nebo
        // multi-line wrap = top-align (leading na vrchu line boxu, browser
        // default).
        let advance_h = bx.font_size * 1.2;
        let is_multiline = text.contains('\n') || inner_h > advance_h * 1.5;
        let needs_center = !is_multiline && inner_h > advance_h + 4.0;
        let v_offset = if needs_center {
            (inner_h - bx.font_size * 1.5) * 0.5
        } else {
            0.0
        };
        let text_y = bx.rect.y + border + pad_t + v_offset;
        let text_color = with_alpha(bx.text_color.unwrap_or([0, 0, 0, 255]));
        // Text shadow - emit pred main text aby byl v pozadi
        if let Some((ox, oy, _blur, color)) = bx.text_shadow {
            cmds.push(DisplayCommand::Text {
                x: text_x + ox,
                y: text_y + oy,
                content: text.clone(),
                color: with_alpha(color),
                font_size: bx.font_size,
                bold: bx.bold,
                font_weight: bx.effective_weight(),
                italic: bx.italic,
                font_family: bx.font_family.clone(),
                strikethrough: false, underline: false,
            });
        }
        // Strike-through pri <s>/<del>/<strike> tagu (line-through default).
        let is_strike_tag = matches!(bx.tag.as_deref(),
            Some("s") | Some("strike") | Some("del"));
        cmds.push(DisplayCommand::Text {
            x: text_x,
            y: text_y,
            content: text.clone(),
            color: text_color,
            font_size: bx.font_size,
            bold: bx.bold,
            font_weight: bx.font_weight,
            italic: bx.italic,
            font_family: bx.font_family.clone(),
            strikethrough: is_strike_tag,
            // Underline render handluje style-aware emit nize (dotted/dashed/
            // wavy/double/solid). Pri pasovani true sem render kresli druhe
            // solid line. Style block uz emituje spravnou variantu.
            underline: false,
        });
        // Underline / strikethrough s ruznymi styly (solid/double/dotted/dashed/wavy)
        if bx.text_underline {
            let dec_color = bx.text_decoration_color
                .map(with_alpha).unwrap_or(text_color);
            let thickness = bx.text_decoration_thickness.max(1.0);
            let offset = bx.text_underline_offset;
            let base_y = text_y + bx.font_size + 1.0 + offset;
            let style = bx.text_decoration_style.as_str();
            match style {
                "double" => {
                    cmds.push(DisplayCommand::Rect {
                        x: text_x, y: base_y,
                        w: text_w, h: thickness,
                        color: dec_color, radius: 0.0,
                    });
                    cmds.push(DisplayCommand::Rect {
                        x: text_x, y: base_y + thickness + 2.0,
                        w: text_w, h: thickness,
                        color: dec_color, radius: 0.0,
                    });
                }
                "dotted" => {
                    let mut x = text_x;
                    while x < text_x + text_w {
                        cmds.push(DisplayCommand::Rect {
                            x, y: base_y, w: thickness, h: thickness,
                            color: dec_color, radius: thickness * 0.5,
                        });
                        x += thickness * 2.0;
                    }
                }
                "dashed" => {
                    let mut x = text_x;
                    let dash = 4.0;
                    while x < text_x + text_w {
                        cmds.push(DisplayCommand::Rect {
                            x, y: base_y, w: dash, h: thickness,
                            color: dec_color, radius: 0.0,
                        });
                        x += dash * 2.0;
                    }
                }
                "wavy" => {
                    // Approx: zigzag s krokem ~6px
                    let step = 4.0;
                    let amp = 2.0;
                    let mut x = text_x;
                    let mut up = true;
                    while x < text_x + text_w {
                        let y = if up { base_y } else { base_y + amp };
                        cmds.push(DisplayCommand::Rect {
                            x, y, w: step, h: thickness,
                            color: dec_color, radius: 0.0,
                        });
                        x += step;
                        up = !up;
                    }
                }
                _ /* solid */ => {
                    cmds.push(DisplayCommand::Rect {
                        x: text_x, y: base_y,
                        w: text_w, h: thickness,
                        color: dec_color, radius: 0.0,
                    });
                }
            }
        }
        if bx.text_strikethrough {
            cmds.push(DisplayCommand::Rect {
                x: text_x,
                y: text_y + bx.font_size * 0.55,
                w: text_w,
                h: 1.0,
                color: text_color,
                radius: 0.0,
            });
        }
    }

    // SVG shapes - emituj pred normal children rekursi (svg children jsou shapes ne LayoutBoxes)
    if bx.tag.as_deref() == Some("svg") {
        emit_svg_children(bx, cmds);
    }

    // Children paint order per CSS 2.1 Appendix E §3 (stacking contexts).
    // 7-stage spec, my pouzivame 4-bucket aproximaci:
    //   bucket 0: negative z-index positioned descendants
    //   bucket 1: in-flow non-positioned (block + inline, DOM order)
    //   bucket 2: positioned z-index auto/0 (DOM order)
    //   bucket 3: positive z-index positioned descendants (z order)
    //
    // Inspired by Chromium core/paint/paint_layer_painter.cc::PaintChildren -
    // jejich kompletni 7-stage pipeline pro stacking context boundaries
    // (negativeZOrderList + posZOrderList + normalFlowList). Tato implementace
    // separuje BUCKETS jen v ramci jednoho parenta - plne spec by vyzadovalo
    // walk pres descendants stacking contextu (audit #7 mention).
    let has_positioned = bx.children.iter().any(|c|
        !matches!(c.position, super::layout::Position::Static));
    let children_order: Vec<usize> = if has_positioned {
        let mut indices: Vec<usize> = (0..bx.children.len()).collect();
        indices.sort_by_key(|&i| {
            let ch = &bx.children[i];
            let is_positioned = !matches!(ch.position, super::layout::Position::Static);
            let z = ch.z_index;
            let bucket: i32 = if !is_positioned {
                1  // in-flow non-positioned
            } else if let Some(zv) = z {
                if zv < 0 { 0 }
                else if zv > 0 { 3 }
                else { 2 }  // z=0 explicit
            } else {
                2  // positioned, z-index auto
            };
            // Secondary key: z hodnota (relevantni jen pro bucket 0 a 3).
            // Tertiary: DOM order (stable - pri stejne bucket+z drzi puvod).
            (bucket, z.unwrap_or(0), i as i32)
        });
        indices
    } else {
        (0..bx.children.len()).collect()
    };

    let children_start = cmds.len();
    // SVG shape children (rect/circle/text/...) jsou nakresleny pres emit_svg_children
    // s spravnymi SVG-space coordinates. Layout boxes pro tyto childeren existuji
    // jen pro devtools highlight + hit-test, jejich rect.x/y nesedi s render
    // pozicemi (set v build-time pred layout positioning). Skipnout rekurzi
    // aby se text node "SVG text" nevkreslil pres regular paint_inline_or_text
    // path na chybnem absolute (= build-time) x.
    let is_svg_root = bx.tag.as_deref() == Some("svg");
    if !is_svg_root {
        // Recursivne deti - auto-grow stack pro deep DOMs.
        for &ci in &children_order {
            let ch = &bx.children[ci];
            stacker::maybe_grow(32 * 1024, 8 * 1024 * 1024, || {
                paint_box(ch, cmds, child_perspective);
            });
        }
    }

    // Overflow:hidden / clip: filter out commands jejichz bbox je MIMO bx.rect.
    // CPU-side bbox cull - nepatrny precision (commands kompletne outside dropuju,
    // partial overlap necham byt). Resi prelevani content carouselu mimo
    // overflow:hidden container.
    let oh_x = bx.overflow_x.clips();
    let oh_y = bx.overflow_y.clips();
    if (oh_x || oh_y) && cmds.len() > children_start {
        let bx_x0 = bx.rect.x;
        let bx_y0 = bx.rect.y;
        let bx_x1 = bx.rect.x + bx.rect.width;
        let bx_y1 = bx.rect.y + bx.rect.height;
        let cull_x_min = if oh_x { bx_x0 } else { f32::NEG_INFINITY };
        let cull_x_max = if oh_x { bx_x1 } else { f32::INFINITY };
        let cull_y_min = if oh_y { bx_y0 } else { f32::NEG_INFINITY };
        let cull_y_max = if oh_y { bx_y1 } else { f32::INFINITY };
        let cmd_outside = |c: &DisplayCommand| -> bool {
            use DisplayCommand::*;
            let (x, y, w, h) = match c {
                Rect { x, y, w, h, .. } => (*x, *y, *w, *h),
                Text { x, y, font_size, content, .. } => {
                    let est_w = content.chars().count() as f32 * font_size * 0.6;
                    (*x, *y, est_w, *font_size * 1.4)
                }
                Image { x, y, w, h, .. } => (*x, *y, *w, *h),
                Gradient { x, y, w, h, .. } => (*x, *y, *w, *h),
                Shadow { x, y, w, h, .. } => (*x, *y, *w, *h),
                Border { x, y, w, h, .. } => (*x, *y, *w, *h),
                _ => return false,  // markers, polygons, transform begin/end - nedotvkat
            };
            x + w < cull_x_min || x > cull_x_max
                || y + h < cull_y_min || y > cull_y_max
        };
        cmds[children_start..].iter().enumerate()
            .filter(|(_, c)| cmd_outside(c))
            .map(|(i, _)| children_start + i)
            .collect::<Vec<_>>()
            .into_iter().rev().for_each(|i| { cmds.remove(i); });
    }

    // CSS Multi-column Layout L1 - column-rule mezi sloupci. Vykresleno PO
    // children aby separator byl viditelny pres pozadi sloupcu.
    if bx.column_count > 1 && bx.column_rule_width > 0.0 && bx.column_rule_style != "none" {
        let pad_l = bx.padding_left.unwrap_or(bx.padding);
        let pad_r = bx.padding_right.unwrap_or(bx.padding);
        let pad_t = bx.padding_top.unwrap_or(bx.padding);
        let pad_b = bx.padding_bottom.unwrap_or(bx.padding);
        let inner_x = bx.rect.x + pad_l + bx.border_width;
        let inner_y = bx.rect.y + pad_t + bx.border_width;
        let inner_w = bx.rect.width - pad_l - pad_r - 2.0 * bx.border_width;
        let inner_h = bx.rect.height - pad_t - pad_b - 2.0 * bx.border_width;
        let n_cols = bx.column_count as f32;
        let gap = bx.column_gap_multicol;
        let col_w = ((inner_w - gap * (n_cols - 1.0)) / n_cols).max(1.0);
        let mut color = bx.column_rule_color;
        color[3] = ((color[3] as u32 * alpha_mul as u32) / 255).min(255) as u8;
        for i in 1..bx.column_count {
            // Rule cara mezi sloupcem (i-1) a i. Stred gap.
            let rx = inner_x + (i as f32) * col_w + (i as f32 - 0.5) * gap - bx.column_rule_width * 0.5;
            cmds.push(DisplayCommand::Rect {
                x: rx, y: inner_y, w: bx.column_rule_width, h: inner_h,
                color, radius: 0.0,
            });
        }
    }

    // Inner scrollbar overlay - pro elementy s overflow_y/x: auto/scroll
    // kdyz inner content presahuje rect bounds. Stage 1: pouze visual indicator
    // (no scroll position tracking). Track + thumb rect pres self bounds.
    // Real interaktivni scroll dela WV (Stage 2 TODO).
    if bx.overflow_y.scrollable() && bx.inner_content_h > bx.rect.height + 1.0
        && bx.rect.width > 12.0 && bx.rect.height > 30.0 {
        let bar_w = 10.0_f32;
        let bar_x = bx.rect.x + bx.rect.width - bar_w;
        let bar_y = bx.rect.y;
        let bar_h = bx.rect.height;
        // Track pozadi (semi-transparent)
        cmds.push(DisplayCommand::Rect {
            x: bar_x, y: bar_y, w: bar_w, h: bar_h,
            color: [60, 60, 70, 100], radius: 0.0,
        });
        // Thumb - proporcionalni k content_h
        let ratio = bx.rect.height / bx.inner_content_h;
        let thumb_h = (bar_h * ratio).max(20.0).min(bar_h);
        // Stage 1: thumb position 0 (no scroll yet). Stage 2 vlozi scroll_y / max * (bar_h - thumb_h)
        cmds.push(DisplayCommand::Rect {
            x: bar_x + 1.0, y: bar_y + 1.0,
            w: bar_w - 2.0, h: thumb_h - 2.0,
            color: [140, 140, 150, 220], radius: (bar_w - 2.0) * 0.5,
        });
    }
    if bx.overflow_x.scrollable() && bx.inner_content_w > bx.rect.width + 1.0
        && bx.rect.height > 12.0 && bx.rect.width > 30.0 {
        let bar_h = 10.0_f32;
        let bar_x = bx.rect.x;
        let bar_y = bx.rect.y + bx.rect.height - bar_h;
        let bar_w = bx.rect.width;
        cmds.push(DisplayCommand::Rect {
            x: bar_x, y: bar_y, w: bar_w, h: bar_h,
            color: [60, 60, 70, 100], radius: 0.0,
        });
        let ratio = bx.rect.width / bx.inner_content_w;
        let thumb_w = (bar_w * ratio).max(20.0).min(bar_w);
        cmds.push(DisplayCommand::Rect {
            x: bar_x + 1.0, y: bar_y + 1.0,
            w: thumb_w - 2.0, h: bar_h - 2.0,
            color: [140, 140, 150, 220], radius: (bar_h - 2.0) * 0.5,
        });
    }

    // mask-image end marker
    if has_mask {
        cmds.push(DisplayCommand::MaskEnd);
    }

    // Filter subtree end marker - paruje s FilterBegin (LIFO)
    if has_subtree_filter {
        cmds.push(DisplayCommand::FilterEnd);
    }
    // Backdrop-filter end marker - paruje s BackdropFilterBegin
    if has_backdrop_filter {
        cmds.push(DisplayCommand::BackdropFilterEnd);
    }

    // 3D transform: skip CPU post-process - vse resi shader matrix.
    if needs_3d {
        cmds.push(DisplayCommand::TransformEnd);
    }

    // Transform aplikovan na vsechny prave vlozene commands tohoto boxu (post-process)
    // Translate / Translate3D - aplikuje shift; rotate/scale 2D pres centroid;
    // matrix3d/perspective - aplikuje matrix multiply na rohy.
    // Skip kdyz needs_3d - shader pipeline aplikuje cely 4x4 matrix.
    use super::layout::TransformOp;
    if !bx.transforms.is_empty() && !needs_3d {
        let start = box_start;
        // Vypocet centroid box-u pro rotate/scale relative-origin
        let cx = bx.rect.x + bx.rect.width  * 0.5;
        let cy = bx.rect.y + bx.rect.height * 0.5;
        for op in &bx.transforms {
            match op {
                TransformOp::Translate(tx, ty) => {
                    for cmd in &mut cmds[start..] { shift_cmd(cmd, *tx, *ty); }
                }
                TransformOp::Translate3D { x, y, .. } => {
                    for cmd in &mut cmds[start..] { shift_cmd(cmd, *x, *y); }
                }
                TransformOp::Scale(sx, sy) => {
                    for cmd in &mut cmds[start..] { scale_cmd(cmd, *sx, *sy, cx, cy); }
                }
                TransformOp::Scale3D { x, y, .. } => {
                    for cmd in &mut cmds[start..] { scale_cmd(cmd, *x, *y, cx, cy); }
                }
                TransformOp::Matrix3D(m) => {
                    // Aplikuje 4x4 matrix na pos rect rohy: translate slot
                    // (m[12], m[13]) + scale (m[0], m[5]) jako approximation 2D
                    let tx = m[12]; let ty = m[13];
                    let sx = m[0]; let sy = m[5];
                    if sx != 1.0 || sy != 1.0 {
                        for cmd in &mut cmds[start..] { scale_cmd(cmd, sx, sy, cx, cy); }
                    }
                    for cmd in &mut cmds[start..] { shift_cmd(cmd, tx, ty); }
                }
                TransformOp::Rotate(rad) => {
                    // 2D rotace kolem centroid - aplikuje na rect rohy + text pos
                    // Pri rotate sirka/vyska zustavaji - jen pozice se posuva (approx).
                    // Real impl by potrebovala shader matrix uniform.
                    let cos = rad.cos();
                    let sin = rad.sin();
                    for cmd in &mut cmds[start..] { rotate_cmd(cmd, cos, sin, cx, cy); }
                }
                TransformOp::Rotate3D { x: ax, y: ay, z: az, angle_rad: rad } => {
                    // Aproximace: kdyz osa ~ Z (0, 0, 1), pouzij 2D rotate.
                    // Jinak skip (vyzaduje shader matrix).
                    if az.abs() > 0.5 && ax.abs() < 0.1 && ay.abs() < 0.1 {
                        let cos = rad.cos();
                        let sin = rad.sin();
                        for cmd in &mut cmds[start..] { rotate_cmd(cmd, cos, sin, cx, cy); }
                    }
                    // X/Y axis rotation: 2D approximace = squeeze sirky/vysky
                    // pri 90 deg axis -> 0 visible. Pro start: jen scale dle cos(angle).
                    else if ax.abs() > 0.5 {
                        // Y-axis -> stlaceni vysky
                        let scale_y = rad.cos().abs();
                        for cmd in &mut cmds[start..] { scale_cmd(cmd, 1.0, scale_y, cx, cy); }
                    } else if ay.abs() > 0.5 {
                        let scale_x = rad.cos().abs();
                        for cmd in &mut cmds[start..] { scale_cmd(cmd, scale_x, 1.0, cx, cy); }
                    }
                }
                TransformOp::Perspective(_) | TransformOp::None => {} // No-op
            }
        }
    } else if let Some(TransformOp::Translate(tx, ty)) = bx.transform {
        let start = box_start;
        for cmd in &mut cmds[start..] {
            shift_cmd(cmd, tx, ty);
        }
    }
    if blend_active {
        cmds.push(DisplayCommand::BlendEnd);
    }
    // Close NoScrollShift bracket - po vsech subtree emit + post-process (transform).
    if is_fixed {
        cmds.push(DisplayCommand::NoScrollShiftEnd);
    }
}

fn scale_cmd(cmd: &mut DisplayCommand, sx: f32, sy: f32, cx: f32, cy: f32) {
    let scale_xy = |x: &mut f32, y: &mut f32| {
        *x = cx + (*x - cx) * sx;
        *y = cy + (*y - cy) * sy;
    };
    let scale_wh = |w: &mut f32, h: &mut f32| {
        *w *= sx; *h *= sy;
    };
    match cmd {
        DisplayCommand::Rect { x, y, w, h, .. }
        | DisplayCommand::Border { x, y, w, h, .. }
        | DisplayCommand::Gradient { x, y, w, h, .. }
        | DisplayCommand::Shadow { x, y, w, h, .. }
        | DisplayCommand::Image { x, y, w, h, .. }
        | DisplayCommand::ImageFit { x, y, w, h, .. }
        | DisplayCommand::BlurredRect { x, y, w, h, .. }
        | DisplayCommand::FilterBegin { x, y, w, h, .. }
        | DisplayCommand::BackdropFilterBegin { x, y, w, h, .. }
        | DisplayCommand::TransformBegin { x, y, w, h, .. }
        | DisplayCommand::MaskBegin { x, y, w, h, .. } => {
            scale_xy(x, y); scale_wh(w, h);
        }
        DisplayCommand::Text { x, y, font_size, .. } => {
            scale_xy(x, y);
            *font_size *= sy.abs();
        }
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, py) in points.iter_mut() {
                scale_xy(px, py);
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd
        | DisplayCommand::NoScrollShiftBegin | DisplayCommand::NoScrollShiftEnd
        | DisplayCommand::BlendBegin { .. } | DisplayCommand::BlendEnd => {}
    }
}

/// Rotace pozice kolem centroid (cx, cy). Sirka/vyska zustavaji - jen pos rotuje.
/// Pro real OBB rotation by se musely vrcholy rotovat zvlast (slozitejsi).
fn rotate_cmd(cmd: &mut DisplayCommand, cos: f32, sin: f32, cx: f32, cy: f32) {
    let rotate_xy = |x: &mut f32, y: &mut f32| {
        let rx = *x - cx;
        let ry = *y - cy;
        *x = cx + rx * cos - ry * sin;
        *y = cy + rx * sin + ry * cos;
    };
    match cmd {
        DisplayCommand::Rect { x, y, .. }
        | DisplayCommand::Border { x, y, .. }
        | DisplayCommand::Gradient { x, y, .. }
        | DisplayCommand::Shadow { x, y, .. }
        | DisplayCommand::Image { x, y, .. }
        | DisplayCommand::ImageFit { x, y, .. }
        | DisplayCommand::BlurredRect { x, y, .. }
        | DisplayCommand::FilterBegin { x, y, .. }
        | DisplayCommand::BackdropFilterBegin { x, y, .. }
        | DisplayCommand::TransformBegin { x, y, .. }
        | DisplayCommand::MaskBegin { x, y, .. }
        | DisplayCommand::Text { x, y, .. } => rotate_xy(x, y),
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, py) in points.iter_mut() {
                rotate_xy(px, py);
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd
        | DisplayCommand::NoScrollShiftBegin | DisplayCommand::NoScrollShiftEnd
        | DisplayCommand::BlendBegin { .. } | DisplayCommand::BlendEnd => {}
    }
}

fn shift_cmd(cmd: &mut DisplayCommand, dx: f32, dy: f32) {
    match cmd {
        DisplayCommand::Rect { x, y, .. }
        | DisplayCommand::Border { x, y, .. }
        | DisplayCommand::Text { x, y, .. }
        | DisplayCommand::Gradient { x, y, .. }
        | DisplayCommand::Shadow { x, y, .. }
        | DisplayCommand::Image { x, y, .. }
        | DisplayCommand::ImageFit { x, y, .. }
        | DisplayCommand::BlurredRect { x, y, .. }
        | DisplayCommand::FilterBegin { x, y, .. }
        | DisplayCommand::BackdropFilterBegin { x, y, .. }
        | DisplayCommand::TransformBegin { x, y, .. }
        | DisplayCommand::MaskBegin { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, py) in points.iter_mut() {
                *px += dx;
                *py += dy;
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd | DisplayCommand::MaskEnd
        | DisplayCommand::NoScrollShiftBegin | DisplayCommand::NoScrollShiftEnd
        | DisplayCommand::BlendBegin { .. } | DisplayCommand::BlendEnd => {}
    }
}
