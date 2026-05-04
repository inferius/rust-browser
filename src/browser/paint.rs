/// Painting - z LayoutBox tree generuje display list (commands).
/// Display list je sekvence primitiv ktere wgpu rendered pak vykresli.

use super::layout::LayoutBox;

#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Solid filled rectangle.
    Rect { x: f32, y: f32, w: f32, h: f32, color: [u8; 4] },
    /// Border (rectangle outline).
    Border { x: f32, y: f32, w: f32, h: f32, width: f32, color: [u8; 4] },
    /// Text rendering.
    Text { x: f32, y: f32, content: String, color: [u8; 4], font_size: f32 },
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

    // Text
    if let Some(text) = &bx.text {
        cmds.push(DisplayCommand::Text {
            x: bx.rect.x + bx.padding,
            y: bx.rect.y + bx.padding,
            content: text.clone(),
            color: bx.text_color.unwrap_or([0, 0, 0, 255]),
            font_size: bx.font_size,
        });
    }

    // Recursivne deti
    for ch in &bx.children {
        paint_box(ch, cmds);
    }
}
