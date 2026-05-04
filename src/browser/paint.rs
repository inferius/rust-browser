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
}

/// Vrati display list - sekvence primitiv pro renderer.
pub fn build_display_list(root: &LayoutBox) -> Vec<DisplayCommand> {
    let mut commands = Vec::new();
    paint_box(root, &mut commands);
    commands
}

fn paint_box(bx: &LayoutBox, cmds: &mut Vec<DisplayCommand>) {
    // Background
    if let Some(bg) = bx.bg_color {
        cmds.push(DisplayCommand::Rect {
            x: bx.rect.x,
            y: bx.rect.y,
            w: bx.rect.width,
            h: bx.rect.height,
            color: bg,
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
                color: bc,
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
        cmds.push(DisplayCommand::Text {
            x: bx.rect.x + bx.padding + align_offset,
            y: bx.rect.y + bx.padding,
            content: text.clone(),
            color: bx.text_color.unwrap_or([0, 0, 0, 255]),
            font_size: bx.font_size,
            bold: bx.bold,
        });
    }

    // Recursivne deti
    for ch in &bx.children {
        paint_box(ch, cmds);
    }
}
