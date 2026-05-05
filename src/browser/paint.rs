/// Painting - z LayoutBox tree generuje display list (commands).
/// Display list je sekvence primitiv ktere wgpu rendered pak vykresli.

use super::layout::{LayoutBox, TextAlign, measure_text_width, BgPosition, BgSize};

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
        bold: bool,
        /// font-family - "" pro default
        font_family: String,
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
    /// Image - decoded RGBA bytes + dimensions.
    Image {
        x: f32, y: f32, w: f32, h: f32,
        src: String,
        radius: f32,
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
    /// Rect oriznuty polygonem (CSS clip-path: polygon(...)).
    /// Body jsou absolutni px souradnice. Renderer triangulate via fan
    /// (convex predpoklad). Concave polygon = artefakty.
    ClippedRect {
        color: [u8; 4],
        points: Vec<(f32, f32)>,
    },
}

/// Vrati display list - sekvence primitiv pro renderer.
pub fn build_display_list(root: &LayoutBox) -> Vec<DisplayCommand> {
    let mut commands = Vec::new();
    paint_box(root, &mut commands, None);
    commands
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

/// Emituje SVG shape z child <rect>, <circle>, <ellipse>, <line>.
/// Pri SVG <svg> tagu projde direktni children a emit native shapes.
fn emit_svg_children(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>) {
    let node = match &bx.node { Some(n) => n, None => return };
    for child in node.children.borrow().iter() {
        let tag = match child.tag_name() { Some(t) => t, None => continue };
        let attr_f = |name: &str, default: f32| -> f32 {
            child.attr(name).and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let attr_color = |name: &str, default: [u8;4]| -> [u8;4] {
            child.attr(name).and_then(|v| super::layout::parse_color(&v)).unwrap_or(default)
        };
        match tag.as_str() {
            "rect" => {
                let x = bx.rect.x + attr_f("x", 0.0);
                let y = bx.rect.y + attr_f("y", 0.0);
                let w = attr_f("width", 0.0);
                let h = attr_f("height", 0.0);
                let rx = attr_f("rx", 0.0);
                let fill = attr_color("fill", [0, 0, 0, 255]);
                cmds.push(DisplayCommand::Rect { x, y, w, h, color: fill, radius: rx });
                let stroke_w = attr_f("stroke-width", 0.0);
                if stroke_w > 0.0 {
                    let stroke_c = attr_color("stroke", [0,0,0,255]);
                    cmds.push(DisplayCommand::Border { x, y, w, h, width: stroke_w, color: stroke_c });
                }
            }
            "circle" => {
                let cx = bx.rect.x + attr_f("cx", 0.0);
                let cy = bx.rect.y + attr_f("cy", 0.0);
                let r = attr_f("r", 0.0);
                let fill = attr_color("fill", [0,0,0,255]);
                cmds.push(DisplayCommand::Rect {
                    x: cx - r, y: cy - r, w: 2.0*r, h: 2.0*r,
                    color: fill, radius: r,
                });
            }
            "ellipse" => {
                let cx = bx.rect.x + attr_f("cx", 0.0);
                let cy = bx.rect.y + attr_f("cy", 0.0);
                let rx = attr_f("rx", 0.0);
                let ry = attr_f("ry", 0.0);
                let fill = attr_color("fill", [0,0,0,255]);
                cmds.push(DisplayCommand::Rect {
                    x: cx - rx, y: cy - ry, w: 2.0*rx, h: 2.0*ry,
                    color: fill, radius: rx.min(ry),
                });
            }
            "line" => {
                let x1 = bx.rect.x + attr_f("x1", 0.0);
                let y1 = bx.rect.y + attr_f("y1", 0.0);
                let x2 = bx.rect.x + attr_f("x2", 0.0);
                let y2 = bx.rect.y + attr_f("y2", 0.0);
                let stroke_c = attr_color("stroke", [0,0,0,255]);
                let stroke_w = attr_f("stroke-width", 1.0);
                // Line approx: thin rect od (x1,y1) k (x2,y2) - axis-aligned only.
                // Pro horizontal: stejny y, ruzny x.
                if (y1 - y2).abs() < 0.5 {
                    cmds.push(DisplayCommand::Rect {
                        x: x1.min(x2), y: y1 - stroke_w / 2.0,
                        w: (x1 - x2).abs(), h: stroke_w,
                        color: stroke_c, radius: 0.0,
                    });
                } else if (x1 - x2).abs() < 0.5 {
                    cmds.push(DisplayCommand::Rect {
                        x: x1 - stroke_w / 2.0, y: y1.min(y2),
                        w: stroke_w, h: (y1 - y2).abs(),
                        color: stroke_c, radius: 0.0,
                    });
                }
            }
            "text" => {
                let x = bx.rect.x + attr_f("x", 0.0);
                let y = bx.rect.y + attr_f("y", 0.0);
                let fill = attr_color("fill", [0,0,0,255]);
                let font_size = attr_f("font-size", 14.0);
                let content = child.text_content();
                if !content.trim().is_empty() {
                    cmds.push(DisplayCommand::Text {
                        x, y: y - font_size, content,
                        color: fill, font_size, bold: false,
                        font_family: String::new(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn paint_box(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>, parent_perspective: Option<f32>) {
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

    // Apply opacity multiply + filter chain + clip-path na vsechny barvy
    let alpha_mul = (bx.opacity * 255.0) as u8;
    let filter = bx.filter.clone();
    // Detect blur radius z filter chain (jedna z filterop) - aplikuje multi-tap
    let blur_radius: f32 = filter.iter().filter_map(|op| match op {
        crate::browser::layout::FilterOp::Blur(r) => Some(*r),
        _ => None,
    }).sum();
    // Detect drop-shadow operations
    let drop_shadows: Vec<(f32, f32, f32, [u8; 4])> = filter.iter().filter_map(|op| match op {
        crate::browser::layout::FilterOp::DropShadow { ox, oy, blur, color } => Some((*ox, *oy, *blur, *color)),
        _ => None,
    }).collect();
    let _ = drop_shadows;

    // Backdrop-filter: outer marker - snapshotne scenu, pak element obsah nahoru.
    // Musi byt pred FilterBegin (wraps cely element vcetne filter subtre).
    let backdrop = bx.backdrop_filter.clone();
    let backdrop_blur: f32 = backdrop.iter().filter_map(|op| match op {
        crate::browser::layout::FilterOp::Blur(r) => Some(*r),
        _ => None,
    }).sum();
    let backdrop_matrix = crate::browser::layout::compute_color_matrix(&backdrop);
    let has_backdrop_filter = !backdrop.is_empty();
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
    let color_matrix = crate::browser::layout::compute_color_matrix(&filter);
    let needs_blur = blur_radius >= 0.5;
    let needs_color = !crate::browser::layout::is_identity_matrix(&color_matrix);
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
        if filter.is_empty() || has_subtree_filter {
            after_alpha
        } else {
            crate::browser::layout::apply_filter_chain(after_alpha, &filter)
        }
    };

    // Filter drop-shadow - emit shadow pred bg (per CSS spec)
    for (ox, oy, blur, color) in &drop_shadows {
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

    // Image - emit Image command (img tag - cover boxu)
    if let Some(src) = &bx.image_src {
        cmds.push(DisplayCommand::Image {
            x: bx.rect.x,
            y: bx.rect.y,
            w: bx.rect.width,
            h: bx.rect.height,
            src: src.clone(),
            radius: bx.border_radius,
        });
    }

    // Background layers (Backgrounds L3): renderuj bottom-to-top (reversed).
    // Kazdy layer muze mit: gradient, image url, solid color (jen posledni layer).
    // Pouzivame bx.backgrounds (Vec<BgLayer>) - parser uz rozdelil comma-sep do layeru.
    use crate::browser::layout::BgRepeat;
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

    // bx.bg_gradient a bx.bg_color: legacy cesta pro background shorthand bez backgrounds vec.
    // Pokud uz backgrounds loop zpracoval barvu, preskocime bg_color aby nedoslo k dvojimu vykresleni.
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

    // Border
    if bx.border_width > 0.0 {
        if let Some(bc) = bx.border_color {
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
        let text_w = measure_text_width(text, bx.font_size);
        let inner_w = bx.rect.width - 2.0 * bx.padding;
        let align_offset = match bx.text_align {
            TextAlign::Left | TextAlign::Justify => 0.0,
            TextAlign::Center => ((inner_w - text_w) * 0.5).max(0.0),
            TextAlign::Right  => (inner_w - text_w).max(0.0),
        };
        let text_x = bx.rect.x + bx.padding + align_offset;
        let text_y = bx.rect.y + bx.padding;
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
                font_family: bx.font_family.clone(),
            });
        }
        cmds.push(DisplayCommand::Text {
            x: text_x,
            y: text_y,
            content: text.clone(),
            color: text_color,
            font_size: bx.font_size,
            bold: bx.bold,
            font_family: bx.font_family.clone(),
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

    // Recursivne deti
    for ch in &bx.children {
        paint_box(ch, cmds, child_perspective);
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
        let start = cmds_offset_for_box(bx, cmds);
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
        let start = cmds_offset_for_box(bx, cmds);
        for cmd in &mut cmds[start..] {
            shift_cmd(cmd, tx, ty);
        }
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
        | DisplayCommand::BlurredRect { x, y, w, h, .. }
        | DisplayCommand::FilterBegin { x, y, w, h, .. }
        | DisplayCommand::BackdropFilterBegin { x, y, w, h, .. }
        | DisplayCommand::TransformBegin { x, y, w, h, .. } => {
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
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd => {}
    }
}

fn cmds_offset_for_box(_bx: &LayoutBox, _cmds: &[DisplayCommand]) -> usize {
    // Pro spravnou implementaci by potreboval index z volajiciho.
    // Zatim vraci 0 - znamena translate aplikuje na cely strom (chybne pri vice transformech).
    // Real impl: paint_box vracel range.
    0
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
        | DisplayCommand::BlurredRect { x, y, .. }
        | DisplayCommand::FilterBegin { x, y, .. }
        | DisplayCommand::BackdropFilterBegin { x, y, .. }
        | DisplayCommand::TransformBegin { x, y, .. }
        | DisplayCommand::Text { x, y, .. } => rotate_xy(x, y),
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, py) in points.iter_mut() {
                rotate_xy(px, py);
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd => {}
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
        | DisplayCommand::BlurredRect { x, y, .. }
        | DisplayCommand::FilterBegin { x, y, .. }
        | DisplayCommand::BackdropFilterBegin { x, y, .. }
        | DisplayCommand::TransformBegin { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        DisplayCommand::ClippedRect { points, .. } => {
            for (px, py) in points.iter_mut() {
                *px += dx;
                *py += dy;
            }
        }
        DisplayCommand::FilterEnd | DisplayCommand::BackdropFilterEnd | DisplayCommand::TransformEnd => {}
    }
}
