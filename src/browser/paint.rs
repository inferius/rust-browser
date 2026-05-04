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
    Text { x: f32, y: f32, content: String, color: [u8; 4], font_size: f32, bold: bool },
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
}

/// Vrati display list - sekvence primitiv pro renderer.
pub fn build_display_list(root: &LayoutBox) -> Vec<DisplayCommand> {
    let mut commands = Vec::new();
    paint_box(root, &mut commands);
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
                    });
                }
            }
            _ => {}
        }
    }
}

fn paint_box(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>) {
    // Apply opacity multiply + filter chain + clip-path na vsechny barvy
    let alpha_mul = (bx.opacity * 255.0) as u8;
    let filter = bx.filter.clone();

    // Clip-path: vypocita modifikaci box rectu pro emit Rect/Image.
    // Single element clip (CPU side) - inset zmensi rect, circle/ellipse pridaji
    // radius. Polygon zatim no-op.
    let (clip_x, clip_y, clip_w, clip_h, clip_radius) = compute_clip_rect(bx);

    let with_alpha = |c: [u8; 4]| -> [u8; 4] {
        let a = ((c[3] as u16 * alpha_mul as u16) / 255) as u8;
        let after_alpha = [c[0], c[1], c[2], a];
        if filter.is_empty() {
            after_alpha
        } else {
            crate::browser::layout::apply_filter_chain(after_alpha, &filter)
        }
    };

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

    // Background-image: aplikuj BgLayer position/size/repeat (single layer aktualne)
    if let Some(layer) = bx.backgrounds.first() {
        if let Some(src) = &layer.image_src {
            // Vypocti vychozi rozmer image podle background-size
            let (img_w, img_h) = compute_bg_size(&layer.size, bx.rect.width, bx.rect.height);
            // Pozice
            let (img_x, img_y) = compute_bg_position(&layer.position, bx.rect.width, bx.rect.height,
                                                     img_w, img_h, bx.rect.x, bx.rect.y);
            // Repeat - pri repeat-x emituje vice tilu vodorovne, repeat-y vertikalne, repeat oboji
            // (no-repeat default - 1 tile)
            use crate::browser::layout::BgRepeat;
            let (rep_x, rep_y) = match layer.repeat {
                BgRepeat::NoRepeat => (1, 1),
                BgRepeat::RepeatX => ((bx.rect.width / img_w).ceil() as i32 + 1, 1),
                BgRepeat::RepeatY => (1, (bx.rect.height / img_h).ceil() as i32 + 1),
                _ /* repeat / space / round */ => (
                    (bx.rect.width / img_w).ceil() as i32 + 1,
                    (bx.rect.height / img_h).ceil() as i32 + 1,
                ),
            };
            // Pri >1 tile musime emitvyat vice Image commandu vedle sebe (clip na box)
            for ix in 0..rep_x {
                for iy in 0..rep_y {
                    let tx = img_x + (ix as f32) * img_w;
                    let ty = img_y + (iy as f32) * img_h;
                    // Skip kdyz tile mimo box
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
    } else if let Some(bg) = bx.bg_color {
        cmds.push(DisplayCommand::Rect {
            x: clip_x,
            y: clip_y,
            w: clip_w,
            h: clip_h,
            color: with_alpha(bg),
            radius: clip_radius,
        });
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
            });
        }
        cmds.push(DisplayCommand::Text {
            x: text_x,
            y: text_y,
            content: text.clone(),
            color: text_color,
            font_size: bx.font_size,
            bold: bx.bold,
        });
        // Underline / strikethrough
        if bx.text_underline {
            cmds.push(DisplayCommand::Rect {
                x: text_x,
                y: text_y + bx.font_size + 1.0,
                w: text_w,
                h: 1.0,
                color: text_color,
                radius: 0.0,
            });
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
        paint_box(ch, cmds);
    }

    // Transform aplikovan na vsechny prave vlozene commands tohoto boxu (post-process)
    // Aktualne transform aplikuje jen translate (rotate/scale potrebuji shader matrix)
    if let Some(super::layout::TransformOp::Translate(tx, ty)) = bx.transform {
        let start = cmds_offset_for_box(bx, cmds);
        for cmd in &mut cmds[start..] {
            shift_cmd(cmd, tx, ty);
        }
    }
}

fn cmds_offset_for_box(_bx: &LayoutBox, _cmds: &[DisplayCommand]) -> usize {
    // Pro spravnou implementaci by potreboval index z volajiciho.
    // Zatim vraci 0 - znamena translate aplikuje na cely strom (chybne pri vice transformech).
    // Real impl: paint_box vracel range.
    0
}

fn shift_cmd(cmd: &mut DisplayCommand, dx: f32, dy: f32) {
    match cmd {
        DisplayCommand::Rect { x, y, .. }
        | DisplayCommand::Border { x, y, .. }
        | DisplayCommand::Text { x, y, .. }
        | DisplayCommand::Gradient { x, y, .. }
        | DisplayCommand::Shadow { x, y, .. }
        | DisplayCommand::Image { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
    }
}
