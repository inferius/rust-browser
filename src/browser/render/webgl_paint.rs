//! WebGL canvas paint stub - drainuje WebGLDrawCmd queue, emituje placeholder.

use crate::browser::layout::LayoutBox;
use crate::browser::paint::DisplayCommand;
use crate::interpreter::{WebGLState, WebGLDrawCmd};

/// Walk layout tree + pro kazdy canvas tag, pokud existuje WebGLState,
/// drainuje queue a emituje display commands. Phase 3b: jen Clear color
/// jako solid Rect bg + DrawArrays stripe overlay placeholder.
/// Pro real GPU draw integration phase 3c5+ vyzaduje refactor (dual path
/// konflict s run_webgl_frame).
pub fn paint_webgl_canvases(
    bx: &LayoutBox,
    webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<WebGLState>>>,
    cmds: &mut Vec<DisplayCommand>,
) {
    
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if let Some(state_rc) = webgl_states.get(&ptr) {
                let mut state = state_rc.borrow_mut();
                // Drain queue. Effektivni clear color = posledni ClearColor command.
                // Pri Clear command s COLOR_BUFFER_BIT (0x4000), vyplnime canvas barvou.
                let mut last_clear_color: Option<[f32; 4]> = None;
                let mut had_clear = false;
                let mut draw_commands_count: usize = 0;
                for cmd in state.draw_queue.drain(..) {
                    match cmd {
                        WebGLDrawCmd::ClearColor(c) => last_clear_color = Some(c),
                        WebGLDrawCmd::Clear(mask) => {
                            if mask & 0x4000 != 0 {
                                had_clear = true;
                            }
                        }
                        WebGLDrawCmd::DrawArrays { .. } | WebGLDrawCmd::DrawElements { .. } => {
                            draw_commands_count += 1;
                        }
                    }
                }
                // Aplikace: pokud bylo Clear + last_clear_color, fill canvas.
                let bg_color = if had_clear {
                    last_clear_color.or(Some(state.clear_color))
                } else {
                    None
                };
                if let Some(c) = bg_color {
                    let rgba: [u8; 4] = [
                        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                    ];
                    cmds.push(DisplayCommand::Rect {
                        x: bx.rect.x, y: bx.rect.y,
                        w: bx.rect.width, h: bx.rect.height,
                        color: rgba, radius: 0.0,
                    });
                }
                // Phase 3c placeholder: pri DrawArrays/DrawElements, emitujem
                // overlay rect (semi-transparent stripes) jako vizualni indikator
                // ze JS volal draw call. Real wgpu pipeline v dalsi fazi.
                if draw_commands_count > 0 {
                    let stripe_count = (draw_commands_count.min(8)) as i32;
                    let stripe_h = bx.rect.height / stripe_count.max(1) as f32;
                    for i in 0..stripe_count {
                        let alpha = ((i as f32 + 1.0) / stripe_count as f32 * 80.0) as u8;
                        cmds.push(DisplayCommand::Rect {
                            x: bx.rect.x,
                            y: bx.rect.y + (i as f32) * stripe_h,
                            w: bx.rect.width,
                            h: stripe_h * 0.5,
                            color: [255, 255, 255, alpha],
                            radius: 0.0,
                        });
                    }
                }
                // Diagnostic - draw_commands_count se uchova ve state pro test access.
                state.draw_call_count = state.draw_call_count.saturating_add(draw_commands_count as u32);
            }
        }
    }
    for child in &bx.children {
        paint_webgl_canvases(child, webgl_states, cmds);
    }
}
