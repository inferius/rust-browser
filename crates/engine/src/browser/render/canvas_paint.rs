//! Canvas2D ops -> DisplayCommand emit (paint phase).

use crate::browser::layout::LayoutBox;
use crate::browser::paint::{CanvasOp, DisplayCommand};

/// Emituje DisplayCommands pro canvas tag z canvas_ops storage.
pub fn paint_canvas_ops(
    bx: &LayoutBox,
    ops_storage: &std::collections::HashMap<usize, Vec<CanvasOp>>,
    cmds: &mut Vec<DisplayCommand>,
) {
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if let Some(ops) = ops_storage.get(&ptr) {
                let mut current_fill: [u8; 4] = [0, 0, 0, 255];
                let mut current_stroke: [u8; 4] = [0, 0, 0, 255];
                let mut current_lw: f32 = 1.0;
                let mut current_font_size: f32 = 14.0;
                // Path state
                let mut path_points: Vec<(f32, f32)> = Vec::new();
                let mut path_arcs: Vec<(f32, f32, f32)> = Vec::new(); // (cx, cy, r)
                let ox = bx.rect.x;
                let oy = bx.rect.y;
                for op in ops {
                    match op {
                        CanvasOp::FillStyle(c) => current_fill = *c,
                        CanvasOp::StrokeStyle(c) => current_stroke = *c,
                        CanvasOp::LineWidth(w) => current_lw = *w,
                        CanvasOp::Font { size, .. } => current_font_size = *size,
                        CanvasOp::BeginPath => {
                            path_points.clear();
                            path_arcs.clear();
                        }
                        CanvasOp::MoveTo { x, y } | CanvasOp::LineTo { x, y } => {
                            path_points.push((ox + *x, oy + *y));
                        }
                        CanvasOp::Arc { cx, cy, r, .. } => {
                            path_arcs.push((ox + *cx, oy + *cy, *r));
                        }
                        CanvasOp::ClosePath => {
                            if let (Some(first), Some(last)) = (path_points.first().copied(), path_points.last().copied()) {
                                if first != last { path_points.push(first); }
                            }
                        }
                        CanvasOp::Stroke => {
                            // Pro path_points: kresli ax-aligned line segmenty (zjednoduseni)
                            for w in path_points.windows(2) {
                                let (x1, y1) = w[0];
                                let (x2, y2) = w[1];
                                if (y1 - y2).abs() < 0.5 {
                                    cmds.push(DisplayCommand::Rect {
                                        x: x1.min(x2), y: y1 - current_lw / 2.0,
                                        w: (x1 - x2).abs(), h: current_lw,
                                        color: current_stroke, radius: 0.0,
                                    });
                                } else if (x1 - x2).abs() < 0.5 {
                                    cmds.push(DisplayCommand::Rect {
                                        x: x1 - current_lw / 2.0, y: y1.min(y2),
                                        w: current_lw, h: (y1 - y2).abs(),
                                        color: current_stroke, radius: 0.0,
                                    });
                                } else {
                                    // Diagonal - aproximace pres axis-aligned mensich segmentu
                                    let dx = x2 - x1; let dy = y2 - y1;
                                    let steps = ((dx.abs() + dy.abs()) / 2.0).max(1.0) as i32;
                                    for i in 0..steps {
                                        let t = i as f32 / steps as f32;
                                        let x = x1 + dx * t;
                                        let y = y1 + dy * t;
                                        cmds.push(DisplayCommand::Rect {
                                            x: x - current_lw / 2.0, y: y - current_lw / 2.0,
                                            w: current_lw, h: current_lw,
                                            color: current_stroke, radius: 0.0,
                                        });
                                    }
                                }
                            }
                            // Arcs jako rounded rect outline aproximace
                            for (cx, cy, r) in &path_arcs {
                                cmds.push(DisplayCommand::Border {
                                    x: cx - r, y: cy - r,
                                    w: 2.0 * r, h: 2.0 * r,
                                    width: current_lw, color: current_stroke,
                                });
                            }
                        }
                        CanvasOp::Fill => {
                            // Fill: pro arc - emit rect s plnym radius
                            for (cx, cy, r) in &path_arcs {
                                cmds.push(DisplayCommand::Rect {
                                    x: cx - r, y: cy - r,
                                    w: 2.0 * r, h: 2.0 * r,
                                    color: current_fill, radius: *r,
                                });
                            }
                            // Polygon fill: bounding box approx
                            if path_points.len() >= 3 {
                                let xs: Vec<f32> = path_points.iter().map(|p| p.0).collect();
                                let ys: Vec<f32> = path_points.iter().map(|p| p.1).collect();
                                let xmin = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                                let xmax = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                                let ymin = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                                let ymax = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                                cmds.push(DisplayCommand::Rect {
                                    x: xmin, y: ymin,
                                    w: xmax - xmin, h: ymax - ymin,
                                    color: current_fill, radius: 0.0,
                                });
                            }
                        }
                        CanvasOp::FillRect { x, y, w, h } => {
                            cmds.push(DisplayCommand::Rect {
                                x: ox + *x, y: oy + *y, w: *w, h: *h,
                                color: current_fill, radius: 0.0,
                            });
                        }
                        CanvasOp::StrokeRect { x, y, w, h } => {
                            cmds.push(DisplayCommand::Border {
                                x: ox + *x, y: oy + *y, w: *w, h: *h,
                                width: current_lw, color: current_stroke,
                            });
                        }
                        CanvasOp::ClearRect { x, y, w, h } => {
                            // Clear = bg cerny (canvas default)
                            cmds.push(DisplayCommand::Rect {
                                x: ox + *x, y: oy + *y, w: *w, h: *h,
                                color: [0, 0, 0, 255], radius: 0.0,
                            });
                        }
                        CanvasOp::FillText { text, x, y } => {
                            cmds.push(DisplayCommand::Text {
                                x: ox + *x, y: oy + *y - current_font_size,
                                content: text.clone(),
                                color: current_fill,
                                font_size: current_font_size,
                                bold: false, font_weight: 400,
                                italic: false,
                                font_family: String::new(),
                                strikethrough: false, underline: false,
                            });
                        }
                        CanvasOp::DrawImage { src, dx, dy, dw, dh } => {
                            cmds.push(DisplayCommand::Image {
                                x: bx.rect.x + dx, y: bx.rect.y + dy,
                                w: *dw, h: *dh,
                                src: src.clone(),
                                radius: 0.0,
                            });
                        }
                        CanvasOp::DrawImageSrc { src, dx, dy, dw, dh, .. } => {
                            // src crop neimplementovan: vykresli cely image do dest rect.
                            cmds.push(DisplayCommand::Image {
                                x: bx.rect.x + dx, y: bx.rect.y + dy,
                                w: *dw, h: *dh,
                                src: src.clone(),
                                radius: 0.0,
                            });
                        }
                        CanvasOp::PathRect { x, y, w, h } => {
                            // Pridame 4 body do path (alternativa k MoveTo/LineTo)
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                        }
                        CanvasOp::RoundRect { x, y, w, h, radius: _ } => {
                            // Aproximace: ostre rohy (radius ignorovany).
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y));
                            path_points.push((bx.rect.x + x + w, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y + h));
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                        }
                        CanvasOp::Ellipse { cx, cy, rx, ry, .. } => {
                            // 16-bodova polygon aproximace.
                            for i in 0..=16 {
                                let t = (i as f32) * std::f32::consts::TAU / 16.0;
                                let px = bx.rect.x + cx + rx * t.cos();
                                let py = bx.rect.y + cy + ry * t.sin();
                                path_points.push((px, py));
                            }
                        }
                        CanvasOp::QuadraticCurveTo { x, y, .. }
                        | CanvasOp::BezierCurveTo { x, y, .. }
                        | CanvasOp::ArcTo { x2: x, y2: y, .. } => {
                            // Aproximace: jen endpoint (zadna krivka interpolace).
                            path_points.push((bx.rect.x + x, bx.rect.y + y));
                        }
                        CanvasOp::StrokeText { text, x, y } => {
                            cmds.push(DisplayCommand::Text {
                                x: bx.rect.x + x,
                                y: bx.rect.y + y,
                                content: text.clone(),
                                color: current_stroke,
                                font_size: current_font_size,
                                bold: false, font_weight: 400,
                                italic: false,
                                font_family: String::new(),
                                strikethrough: false, underline: false,
                            });
                        }
                        // State / transform / styling ops - render je no-op,
                        // plna impl by drzela state stack per-op.
                        CanvasOp::Save | CanvasOp::Restore
                        | CanvasOp::Translate { .. } | CanvasOp::Rotate { .. }
                        | CanvasOp::Scale { .. } | CanvasOp::SetTransform { .. }
                        | CanvasOp::Transform { .. } | CanvasOp::ResetTransform
                        | CanvasOp::GlobalAlpha(_) | CanvasOp::GlobalCompositeOperation(_)
                        | CanvasOp::Clip
                        | CanvasOp::LineCap(_) | CanvasOp::LineJoin(_)
                        | CanvasOp::MiterLimit(_) | CanvasOp::LineDash(_)
                        | CanvasOp::LineDashOffset(_)
                        | CanvasOp::TextAlign(_) | CanvasOp::TextBaseline(_)
                        | CanvasOp::ShadowColor(_) | CanvasOp::ShadowBlur(_)
                        | CanvasOp::ShadowOffsetX(_) | CanvasOp::ShadowOffsetY(_)
                        | CanvasOp::FillStyleLinearGradient { .. }
                        | CanvasOp::FillStyleRadialGradient { .. } => {}
                    }
                }
            }
        }
    }
    for child in &bx.children {
        paint_canvas_ops(child, ops_storage, cmds);
    }
}
