/// Painting - z LayoutBox tree generuje display list (commands).
/// Display list je sekvence primitiv ktere wgpu rendered pak vykresli.

use super::layout::{LayoutBox, TextAlign, measure_text_width};

#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Solid filled rectangle.
    Rect { x: f32, y: f32, w: f32, h: f32, color: [u8; 4], radius: f32 },
    /// Border (rectangle outline).
    Border { x: f32, y: f32, w: f32, h: f32, width: f32, color: [u8; 4] },
    /// Text rendering.
    Text { x: f32, y: f32, content: String, color: [u8; 4], font_size: f32, bold: bool },
    /// Linear gradient rect: barva interpolovana podle smeru.
    /// angle_deg: 0 = nahoru (z dola), 90 = doprava, 180 = dolu, 270 = doleva
    Gradient {
        x: f32, y: f32, w: f32, h: f32,
        angle_deg: f32,
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

fn paint_box(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>) {
    // Apply opacity multiply na vsechny barvy
    let alpha_mul = (bx.opacity * 255.0) as u8;
    let with_alpha = |c: [u8; 4]| -> [u8; 4] {
        let a = ((c[3] as u16 * alpha_mul as u16) / 255) as u8;
        [c[0], c[1], c[2], a]
    };

    // Box shadow - emit pred bg
    if let Some((ox, oy, blur, spread, color)) = bx.box_shadow {
        cmds.push(DisplayCommand::Shadow {
            x: bx.rect.x + ox - spread,
            y: bx.rect.y + oy - spread,
            w: bx.rect.width + 2.0 * spread,
            h: bx.rect.height + 2.0 * spread,
            offset_x: ox,
            offset_y: oy,
            blur,
            spread,
            color: with_alpha(color),
            radius: bx.border_radius,
        });
    }

    // Image - emit Image command (s priorita pres bg)
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

    // Background gradient ma prioritu pred solid color
    if let Some((angle, ref stops)) = bx.bg_gradient {
        cmds.push(DisplayCommand::Gradient {
            x: bx.rect.x,
            y: bx.rect.y,
            w: bx.rect.width,
            h: bx.rect.height,
            angle_deg: angle,
            stops: stops.iter().map(|(o, c)| (*o, with_alpha(*c))).collect(),
            radius: bx.border_radius,
        });
    } else if let Some(bg) = bx.bg_color {
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x,
            y: bx.rect.y,
            w: bx.rect.width,
            h: bx.rect.height,
            color: with_alpha(bg),
            radius: bx.border_radius,
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
