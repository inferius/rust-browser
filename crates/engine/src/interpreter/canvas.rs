//! Canvas 2D context JS API + gradients + font parser.
//! Extrahovano z mod.rs (Iter 267 refactor).

use std::rc::Rc;
use std::cell::RefCell;
use super::{JsValue, JsObject};
use super::helpers::native;

/// Precte fill/stroke styl z props + aplikuje globalAlpha na alpha kanal.
/// Drive se globalAlpha (`ctx.globalAlpha = 0.5`) ignoroval -> particle fade
/// (globalAlpha = p.life) se neaplikoval = plne opaque misto mizejici.
fn style_color_with_alpha(obj: &Rc<RefCell<JsObject>>, prop: &str) -> [u8; 4] {
    let b = obj.borrow();
    let style_str = b.props.get(prop).map(|v| v.to_string())
        .unwrap_or_else(|| "black".into());
    let mut color = crate::browser::layout::parse_color(&style_str)
        .unwrap_or([0, 0, 0, 255]);
    let ga = b.props.get("globalAlpha").map(|v| v.to_number()).unwrap_or(1.0)
        .clamp(0.0, 1.0);
    color[3] = (color[3] as f64 * ga).round() as u8;
    color
}

pub(crate) fn create_canvas_2d_context(
    canvas_ptr: usize,
    ops_storage: Rc<RefCell<std::collections::HashMap<usize, Vec<crate::browser::paint::CanvasOp>>>>,
) -> JsValue {
    use crate::browser::paint::CanvasOp;

    let obj_rc: Rc<RefCell<JsObject>> = Rc::new(RefCell::new(JsObject::new()));
    {
        let mut o = obj_rc.borrow_mut();
        o.set("__canvas_ptr__".into(), JsValue::Number(canvas_ptr as f64));
        o.set("fillStyle".into(), JsValue::Str("#000000".into()));
        o.set("strokeStyle".into(), JsValue::Str("#000000".into()));
        o.set("lineWidth".into(), JsValue::Number(1.0));
        o.set("font".into(), JsValue::Str("14px sans-serif".into()));
    }

    let push_op = {
        let storage = Rc::clone(&ops_storage);
        move |op: CanvasOp| {
            storage.borrow_mut().entry(canvas_ptr).or_default().push(op);
        }
    };

    // fillRect
    {
        let push = push_op.clone();
        let obj_clone = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("fillRect".into(), native("fillRect", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let color = style_color_with_alpha(&obj_clone, "fillStyle");
            push(CanvasOp::FillStyle(color));
            push(CanvasOp::FillRect { x, y, w, h });
            Ok(JsValue::Undefined)
        }));
    }
    // strokeRect
    {
        let push = push_op.clone();
        let obj_clone = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("strokeRect".into(), native("strokeRect", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let color = style_color_with_alpha(&obj_clone, "strokeStyle");
            let lw = obj_clone.borrow().props.get("lineWidth")
                .map(|v| v.to_number()).unwrap_or(1.0) as f32;
            push(CanvasOp::StrokeStyle(color));
            push(CanvasOp::LineWidth(lw));
            push(CanvasOp::StrokeRect { x, y, w, h });
            Ok(JsValue::Undefined)
        }));
    }
    // clearRect
    {
        let push = push_op.clone();
        let storage = Rc::clone(&ops_storage);
        obj_rc.borrow_mut().set("clearRect".into(), native("clearRect", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            // Full-canvas clear (origin) = RESET op historie. Bez tohoto buffer
            // rostl do nekonecna (kazdy RAF frame pushne desitky ops) -> paint
            // replayuje celou historii = leak + progresivni zpomaleni. Po clearu
            // se pushou jen ops daneho framu (clear -> bg fill -> particles).
            if x <= 0.0 && y <= 0.0 {
                storage.borrow_mut().entry(canvas_ptr).or_default().clear();
            } else {
                push(CanvasOp::ClearRect { x, y, w, h });
            }
            Ok(JsValue::Undefined)
        }));
    }
    // fillText
    {
        let push = push_op.clone();
        let obj_clone = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("fillText".into(), native("fillText", move |args| {
            let mut it = args.into_iter();
            let text = it.next().map(|v| v.to_string()).unwrap_or_default();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let color = style_color_with_alpha(&obj_clone, "fillStyle");
            let font_str = obj_clone.borrow().props.get("font")
                .map(|v| v.to_string()).unwrap_or_else(|| "14px sans-serif".into());
            let (size, family) = parse_canvas_font(&font_str);
            push(CanvasOp::FillStyle(color));
            push(CanvasOp::Font { size, family });
            push(CanvasOp::FillText { text, x, y });
            Ok(JsValue::Undefined)
        }));
    }
    // Path methods
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("beginPath".into(), native("beginPath", move |_| {
            push(CanvasOp::BeginPath);
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("moveTo".into(), native("moveTo", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::MoveTo { x, y });
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("lineTo".into(), native("lineTo", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::LineTo { x, y });
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("arc".into(), native("arc", move |args| {
            let mut it = args.into_iter();
            let cx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let cy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let r = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let start = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let end = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::Arc { cx, cy, r, start, end });
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("closePath".into(), native("closePath", move |_| {
            push(CanvasOp::ClosePath);
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        let obj_clone = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("stroke".into(), native("stroke", move |_| {
            let color = style_color_with_alpha(&obj_clone, "strokeStyle");
            let lw = obj_clone.borrow().props.get("lineWidth")
                .map(|v| v.to_number()).unwrap_or(1.0) as f32;
            push(CanvasOp::StrokeStyle(color));
            push(CanvasOp::LineWidth(lw));
            push(CanvasOp::Stroke);
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        let obj_clone = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("fill".into(), native("fill", move |_| {
            let color = style_color_with_alpha(&obj_clone, "fillStyle");
            push(CanvasOp::FillStyle(color));
            push(CanvasOp::Fill);
            Ok(JsValue::Undefined)
        }));
    }
    // save / restore - state stack
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("save".into(), native("save", move |_| {
            push(CanvasOp::Save); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("restore".into(), native("restore", move |_| {
            push(CanvasOp::Restore); Ok(JsValue::Undefined)
        }));
    }
    // translate / rotate / scale
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("translate".into(), native("translate", move |args| {
            let mut it = args.into_iter();
            let dx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let dy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::Translate { dx, dy }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("rotate".into(), native("rotate", move |args| {
            let rad = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::Rotate { rad }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("scale".into(), native("scale", move |args| {
            let mut it = args.into_iter();
            let sx = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            let sy = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            push(CanvasOp::Scale { sx, sy }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("setTransform".into(), native("setTransform", move |args| {
            let mut it = args.into_iter();
            let a = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            let b = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let c = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let d = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            let e = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let f = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::SetTransform { a, b, c, d, e, f }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("transform".into(), native("transform", move |args| {
            let mut it = args.into_iter();
            let a = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            let b = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let c = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let d = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            let e = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let f = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::Transform { a, b, c, d, e, f }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("resetTransform".into(), native("resetTransform", move |_| {
            push(CanvasOp::ResetTransform); Ok(JsValue::Undefined)
        }));
    }
    // quadraticCurveTo / bezierCurveTo / arcTo
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("quadraticCurveTo".into(), native("quadraticCurveTo", move |args| {
            let mut it = args.into_iter();
            let cpx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let cpy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::QuadraticCurveTo { cpx, cpy, x, y }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("bezierCurveTo".into(), native("bezierCurveTo", move |args| {
            let mut it = args.into_iter();
            let cp1x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let cp1y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let cp2x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let cp2y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::BezierCurveTo { cp1x, cp1y, cp2x, cp2y, x, y }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("arcTo".into(), native("arcTo", move |args| {
            let mut it = args.into_iter();
            let x1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let x2 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y2 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let radius = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::ArcTo { x1, y1, x2, y2, radius }); Ok(JsValue::Undefined)
        }));
    }
    // rect / roundRect / ellipse
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("rect".into(), native("rect", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::PathRect { x, y, w, h }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("roundRect".into(), native("roundRect", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let radius = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::RoundRect { x, y, w, h, radius }); Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("ellipse".into(), native("ellipse", move |args| {
            let mut it = args.into_iter();
            let cx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let cy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let rx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let ry = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let rotation = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let start_angle = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let end_angle = it.next().map(|v| v.to_number()).unwrap_or(std::f64::consts::TAU as f64) as f32;
            let anticlockwise = it.next().map(|v| v.is_truthy()).unwrap_or(false);
            push(CanvasOp::Ellipse { cx, cy, rx, ry, rotation, start_angle, end_angle, anticlockwise });
            Ok(JsValue::Undefined)
        }));
    }
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("clip".into(), native("clip", move |_| {
            push(CanvasOp::Clip); Ok(JsValue::Undefined)
        }));
    }
    // strokeText
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("strokeText".into(), native("strokeText", move |args| {
            let mut it = args.into_iter();
            let text = it.next().map(|v| v.to_string()).unwrap_or_default();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            push(CanvasOp::StrokeText { text, x, y }); Ok(JsValue::Undefined)
        }));
    }
    // measureText - vraci objekt s width
    {
        let obj_clone = Rc::clone(&obj_rc);
        obj_rc.borrow_mut().set("measureText".into(), native("measureText", move |args| {
            let text = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let font_str = obj_clone.borrow().props.get("font")
                .map(|v| v.to_string()).unwrap_or_else(|| "14px sans-serif".into());
            let (size, _) = parse_canvas_font(&font_str);
            // Approximace: 0.5 * size * char count
            let width = (text.chars().count() as f32) * size * 0.5;
            let mut metrics = JsObject::new();
            metrics.set("width".into(), JsValue::Number(width as f64));
            metrics.set("actualBoundingBoxAscent".into(), JsValue::Number((size * 0.8) as f64));
            metrics.set("actualBoundingBoxDescent".into(), JsValue::Number((size * 0.2) as f64));
            metrics.set("fontBoundingBoxAscent".into(), JsValue::Number((size * 0.8) as f64));
            metrics.set("fontBoundingBoxDescent".into(), JsValue::Number((size * 0.2) as f64));
            metrics.set("emHeightAscent".into(), JsValue::Number((size * 0.8) as f64));
            metrics.set("emHeightDescent".into(), JsValue::Number((size * 0.2) as f64));
            Ok(JsValue::Object(Rc::new(RefCell::new(metrics))))
        }));
    }
    // setLineDash / getLineDash
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("setLineDash".into(), native("setLineDash", move |args| {
            let arr = args.into_iter().next().unwrap_or(JsValue::Undefined);
            let dashes: Vec<f32> = if let JsValue::Array(a) = arr {
                a.borrow().iter().map(|v| v.to_number() as f32).collect()
            } else { Vec::new() };
            push(CanvasOp::LineDash(dashes)); Ok(JsValue::Undefined)
        }));
    }
    obj_rc.borrow_mut().set("getLineDash".into(),
        native("getLineDash", |_| Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))));
    // drawImage
    {
        let push = push_op.clone();
        obj_rc.borrow_mut().set("drawImage".into(), native("drawImage", move |args| {
            let arg_count = args.len();
            let mut it = args.into_iter();
            let img = it.next().unwrap_or(JsValue::Undefined);
            let src = if let JsValue::DomNode(n) = &img {
                n.attr("src").unwrap_or_default()
            } else if let JsValue::Object(o) = &img {
                o.borrow().props.get("src").map(|v| v.to_string()).unwrap_or_default()
            } else { String::new() };
            match arg_count {
                3 => {
                    let dx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    push(CanvasOp::DrawImage { src, dx, dy, dw: 0.0, dh: 0.0 });
                }
                5 => {
                    let dx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dw = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dh = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    push(CanvasOp::DrawImage { src, dx, dy, dw, dh });
                }
                9 => {
                    let sx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let sy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let sw = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let sh = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dx = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dy = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dw = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    let dh = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
                    push(CanvasOp::DrawImageSrc { src, sx, sy, sw, sh, dx, dy, dw, dh });
                }
                _ => {}
            }
            Ok(JsValue::Undefined)
        }));
    }
    // createLinearGradient / createRadialGradient - vraci CanvasGradient s addColorStop
    obj_rc.borrow_mut().set("createLinearGradient".into(),
        native("createLinearGradient", move |args| {
            let mut it = args.into_iter();
            let x0 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y0 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let x1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            Ok(make_canvas_gradient_linear(x0, y0, x1, y1))
        }));
    obj_rc.borrow_mut().set("createRadialGradient".into(),
        native("createRadialGradient", move |args| {
            let mut it = args.into_iter();
            let x0 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y0 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let r0 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let x1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let y1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let r1 = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            Ok(make_canvas_gradient_radial(x0, y0, r0, x1, y1, r1))
        }));
    // ImageData / createImageData / getImageData / putImageData
    obj_rc.borrow_mut().set("createImageData".into(),
        native("createImageData", |args| {
            let mut it = args.into_iter();
            let w = it.next().map(|v| v.to_number()).unwrap_or(1.0);
            let h = it.next().map(|v| v.to_number()).unwrap_or(1.0);
            let len = (w * h * 4.0) as usize;
            let mut data = JsObject::new();
            data.set("width".into(), JsValue::Number(w));
            data.set("height".into(), JsValue::Number(h));
            data.set("data".into(), JsValue::Array(Rc::new(RefCell::new(
                vec![JsValue::Number(0.0); len]
            ))));
            Ok(JsValue::Object(Rc::new(RefCell::new(data))))
        }));
    obj_rc.borrow_mut().set("getImageData".into(),
        native("getImageData", |args| {
            let mut it = args.into_iter().skip(2);
            let w = it.next().map(|v| v.to_number()).unwrap_or(1.0);
            let h = it.next().map(|v| v.to_number()).unwrap_or(1.0);
            let len = (w * h * 4.0) as usize;
            let mut data = JsObject::new();
            data.set("width".into(), JsValue::Number(w));
            data.set("height".into(), JsValue::Number(h));
            data.set("data".into(), JsValue::Array(Rc::new(RefCell::new(
                vec![JsValue::Number(0.0); len]
            ))));
            Ok(JsValue::Object(Rc::new(RefCell::new(data))))
        }));
    obj_rc.borrow_mut().set("putImageData".into(),
        native("putImageData", |_| Ok(JsValue::Undefined)));
    // isPointInPath / isPointInStroke - stub
    obj_rc.borrow_mut().set("isPointInPath".into(),
        native("isPointInPath", |_| Ok(JsValue::Bool(false))));
    obj_rc.borrow_mut().set("isPointInStroke".into(),
        native("isPointInStroke", |_| Ok(JsValue::Bool(false))));

    JsValue::Object(obj_rc)
}

/// Vyrobi CanvasGradient object pro createLinearGradient.
pub(crate) fn make_canvas_gradient_linear(x0: f32, y0: f32, x1: f32, y1: f32) -> JsValue {
    let stops: Rc<RefCell<Vec<(f32, [u8; 4])>>> = Rc::new(RefCell::new(Vec::new()));
    let obj = Rc::new(RefCell::new(JsObject::new()));
    obj.borrow_mut().set("__gradient_kind__".into(), JsValue::Str("linear".into()));
    obj.borrow_mut().set("__x0__".into(), JsValue::Number(x0 as f64));
    obj.borrow_mut().set("__y0__".into(), JsValue::Number(y0 as f64));
    obj.borrow_mut().set("__x1__".into(), JsValue::Number(x1 as f64));
    obj.borrow_mut().set("__y1__".into(), JsValue::Number(y1 as f64));
    let stops_clone = Rc::clone(&stops);
    obj.borrow_mut().set("addColorStop".into(), native("addColorStop", move |args| {
        use crate::browser::layout::parse_color;
        let mut it = args.into_iter();
        let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
        let color_str = it.next().map(|v| v.to_string()).unwrap_or_default();
        let color = parse_color(&color_str).unwrap_or([0, 0, 0, 255]);
        stops_clone.borrow_mut().push((offset, color));
        Ok(JsValue::Undefined)
    }));
    JsValue::Object(obj)
}

/// Vyrobi CanvasGradient object pro createRadialGradient.
pub(crate) fn make_canvas_gradient_radial(x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32) -> JsValue {
    let stops: Rc<RefCell<Vec<(f32, [u8; 4])>>> = Rc::new(RefCell::new(Vec::new()));
    let obj = Rc::new(RefCell::new(JsObject::new()));
    obj.borrow_mut().set("__gradient_kind__".into(), JsValue::Str("radial".into()));
    obj.borrow_mut().set("__x0__".into(), JsValue::Number(x0 as f64));
    obj.borrow_mut().set("__y0__".into(), JsValue::Number(y0 as f64));
    obj.borrow_mut().set("__r0__".into(), JsValue::Number(r0 as f64));
    obj.borrow_mut().set("__x1__".into(), JsValue::Number(x1 as f64));
    obj.borrow_mut().set("__y1__".into(), JsValue::Number(y1 as f64));
    obj.borrow_mut().set("__r1__".into(), JsValue::Number(r1 as f64));
    let stops_clone = Rc::clone(&stops);
    obj.borrow_mut().set("addColorStop".into(), native("addColorStop", move |args| {
        use crate::browser::layout::parse_color;
        let mut it = args.into_iter();
        let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
        let color_str = it.next().map(|v| v.to_string()).unwrap_or_default();
        let color = parse_color(&color_str).unwrap_or([0, 0, 0, 255]);
        stops_clone.borrow_mut().push((offset, color));
        Ok(JsValue::Undefined)
    }));
    JsValue::Object(obj)
}

pub(crate) fn parse_canvas_font(s: &str) -> (f32, String) {
    let parts: Vec<&str> = s.split_whitespace().collect();
    let mut size = 14.0;
    let mut family = String::from("sans-serif");
    for (i, p) in parts.iter().enumerate() {
        if let Some(num) = p.strip_suffix("px") {
            if let Ok(n) = num.parse::<f32>() {
                size = n;
                if i + 1 < parts.len() {
                    family = parts[i+1..].join(" ").trim_matches('"').trim_matches('\'').to_string();
                }
                break;
            }
        }
    }
    (size, family)
}

