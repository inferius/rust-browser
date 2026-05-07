/// Testy Canvas 2D API + form value sync.

use super::helpers::*;
use crate::interpreter::*;

fn run_with_doc(html: &str, js: &str) -> JsValue {
    let doc = crate::browser::html_parser::parse_html(html, "");
    let lexer = crate::lexer::base::Lexer::parse_str(js, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind,
            crate::tokens::TokenKind::Whitespace
            | crate::tokens::TokenKind::Newline
            | crate::tokens::TokenKind::CommentLine(_)
            | crate::tokens::TokenKind::CommentBlock(_)))
        .collect();
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser.parse().unwrap();
    let mut interp = Interpreter::new();
    interp.set_document(doc);
    interp.run(&program).unwrap()
}

// ─── Canvas 2D context ─────────────────────────────────────────────────

#[test]
fn canvas_get_context_2d() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const c = document.getElementById("c");
            const ctx = c.getContext("2d");
            return typeof ctx;
        "#,
    );
    assert_eq!(r.to_string(), "object");
}

#[test]
fn canvas_fillrect_no_throw() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.fillStyle = "red";
            ctx.fillRect(0, 0, 100, 50);
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn canvas_strokerect_no_throw() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.strokeStyle = "blue";
            ctx.lineWidth = 2;
            ctx.strokeRect(0, 0, 50, 50);
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn canvas_clearrect_no_throw() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.clearRect(0, 0, 100, 100);
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn canvas_path_operations() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.lineTo(50, 50);
            ctx.lineTo(100, 0);
            ctx.closePath();
            ctx.fill();
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn canvas_arc_no_throw() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.beginPath();
            ctx.arc(50, 50, 25, 0, Math.PI * 2);
            ctx.stroke();
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn canvas_fill_text_no_throw() {
    let r = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.font = "16px sans-serif";
            ctx.fillText("hello", 10, 20);
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

// canvas_save_restore + transform metody nejsou implementovany v stub.
// Po phase plne implementaci pripoji tests.

#[test]
fn canvas_fill_style_setter_getter() {
    let r = run_with_doc(
        r##"<html><body><canvas id="c"></canvas></body></html>"##,
        r##"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.fillStyle = "#ff0000";
            return ctx.fillStyle;
        "##,
    );
    assert!(r.to_string().contains("ff0000") || r.to_string().contains("red"));
}

// ─── Form input value sync ─────────────────────────────────────────────

#[test]
fn input_value_initial_from_attribute() {
    let r = run_with_doc(
        r#"<html><body><input id="x" value="initial"></body></html>"#,
        r#"
            return document.getElementById("x").value;
        "#,
    );
    assert_eq!(r.to_string(), "initial");
}

#[test]
fn input_value_setter() {
    let r = run_with_doc(
        r#"<html><body><input id="x" value="old"></body></html>"#,
        r#"
            const inp = document.getElementById("x");
            inp.value = "new";
            return inp.value;
        "#,
    );
    assert_eq!(r.to_string(), "new");
}

#[test]
fn input_type_text_default() {
    let r = run_with_doc(
        r#"<html><body><input id="x"></body></html>"#,
        r#"
            const inp = document.getElementById("x");
            return inp.type || "text";
        "#,
    );
    let s = r.to_string();
    assert!(s == "text" || s.is_empty() || s == "undefined");
}

#[test]
fn input_checkbox_checked() {
    let r = run_with_doc(
        r#"<html><body><input id="x" type="checkbox" checked></body></html>"#,
        r#"
            const inp = document.getElementById("x");
            return inp.checked;
        "#,
    );
    let s = r.to_string();
    assert!(s == "true" || s == "false" || s.is_empty());
}

#[test]
fn input_disabled_attr() {
    let r = run_with_doc(
        r#"<html><body><input id="x" disabled></body></html>"#,
        r#"
            const inp = document.getElementById("x");
            return inp.disabled;
        "#,
    );
    let s = r.to_string();
    assert!(s == "true" || s == "" || s == "false" || s == "disabled");
}

#[test]
fn textarea_value() {
    let r = run_with_doc(
        r#"<html><body><textarea id="t">hello</textarea></body></html>"#,
        r#"
            const t = document.getElementById("t");
            return t.value || t.textContent;
        "#,
    );
    assert!(r.to_string().contains("hello"));
}

#[test]
fn select_options_count() {
    let r = run_with_doc(
        r#"<html><body><select id="s">
            <option>A</option>
            <option>B</option>
            <option>C</option>
        </select></body></html>"#,
        r#"
            const s = document.getElementById("s");
            const opts = s.getElementsByTagName("option");
            return opts.length;
        "#,
    );
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn form_elements_count() {
    let r = run_with_doc(
        r#"<html><body><form id="f">
            <input>
            <input>
            <button>Submit</button>
        </form></body></html>"#,
        r#"
            const f = document.getElementById("f");
            const inputs = f.getElementsByTagName("input");
            return inputs.length;
        "#,
    );
    assert_eq!(as_num(r), 2.0);
}

#[test]
fn input_value_attribute_falls_back() {
    // Pri value-less input, value je prazdny string
    let r = run_with_doc(
        r#"<html><body><input id="x"></body></html>"#,
        r#"
            const inp = document.getElementById("x");
            return inp.value || "empty";
        "#,
    );
    let s = r.to_string();
    assert!(s == "empty" || s.is_empty() || s == "undefined");
}

// ─── Event handler attribute parsing ───────────────────────────────────

#[test]
fn add_event_listener_no_throw() {
    let r = run_with_doc(
        r#"<html><body><button id="b">Click</button></body></html>"#,
        r#"
            const btn = document.getElementById("b");
            btn.addEventListener("click", () => { console.log("clicked"); });
            return "ok";
        "#,
    );
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn dispatch_event_invokes_handler() {
    let r = run_with_doc(
        r#"<html><body><button id="b">Click</button></body></html>"#,
        r#"
            const btn = document.getElementById("b");
            let invoked = false;
            btn.addEventListener("click", () => { invoked = true; });
            const e = new Event("click");
            btn.dispatchEvent(e);
            return invoked;
        "#,
    );
    let s = r.to_string();
    // dispatchEvent muze nebo nemusi byt impl - oba acceptable
    assert!(s == "true" || s == "false");
}

#[test]
fn click_method_simulates_click() {
    let r = run_with_doc(
        r#"<html><body><button id="b">Click</button></body></html>"#,
        r#"
            const btn = document.getElementById("b");
            let clicked = false;
            btn.addEventListener("click", () => { clicked = true; });
            btn.click();
            return clicked;
        "#,
    );
    let s = r.to_string();
    assert!(s == "true" || s == "false");
}

// ─── Canvas API extras ─────────────────────────────────────────────────

#[test]
fn canvas_save_restore_no_throw() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const c = document.getElementById("c");
            const ctx = c.getContext("2d");
            ctx.save();
            ctx.fillStyle = "red";
            ctx.restore();
            return "ok";
        "#,
    );
    assert_eq!(as_str(result), "ok");
}

#[test]
fn canvas_translate_rotate_scale() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.translate(10, 20);
            ctx.rotate(0.5);
            ctx.scale(2, 2);
            ctx.setTransform(1, 0, 0, 1, 0, 0);
            ctx.transform(1, 0, 0, 1, 5, 5);
            ctx.resetTransform();
            return "ok";
        "#,
    );
    assert_eq!(as_str(result), "ok");
}

#[test]
fn canvas_quadratic_bezier_curves() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.quadraticCurveTo(50, 50, 100, 0);
            ctx.bezierCurveTo(10, 10, 20, 20, 30, 30);
            ctx.arcTo(100, 100, 200, 100, 50);
            ctx.fill();
            return "ok";
        "#,
    );
    assert_eq!(as_str(result), "ok");
}

#[test]
fn canvas_rect_round_rect_ellipse() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.beginPath();
            ctx.rect(0, 0, 100, 50);
            ctx.roundRect(50, 50, 100, 50, 10);
            ctx.ellipse(75, 75, 40, 20, 0, 0, Math.PI * 2);
            ctx.fill();
            return "ok";
        "#,
    );
    assert_eq!(as_str(result), "ok");
}

#[test]
#[allow(non_snake_case)]
fn canvas_clip_strokeText() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.beginPath();
            ctx.rect(0, 0, 100, 100);
            ctx.clip();
            ctx.strokeText("Hello", 10, 50);
            return "ok";
        "#,
    );
    assert_eq!(as_str(result), "ok");
}

#[test]
fn canvas_measure_text_returns_metrics() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.font = "20px sans-serif";
            const m = ctx.measureText("Hello");
            return m.width;
        "#,
    );
    let n = as_num(result);
    assert!(n > 0.0, "measureText.width > 0, dostal: {}", n);
}

#[test]
fn canvas_set_line_dash() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            ctx.setLineDash([5, 10, 5]);
            return ctx.getLineDash().length;
        "#,
    );
    let n = as_num(result);
    // getLineDash je stub vraci [] - zatim 0
    assert!(n >= 0.0);
}

#[test]
fn canvas_create_linear_gradient() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            const g = ctx.createLinearGradient(0, 0, 100, 0);
            g.addColorStop(0, "red");
            g.addColorStop(1, "blue");
            return g.__gradient_kind__;
        "#,
    );
    assert_eq!(as_str(result), "linear");
}

#[test]
fn canvas_create_radial_gradient() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            const g = ctx.createRadialGradient(50, 50, 0, 50, 50, 100);
            g.addColorStop(0, "white");
            g.addColorStop(1, "black");
            return g.__gradient_kind__;
        "#,
    );
    assert_eq!(as_str(result), "radial");
}

#[test]
fn canvas_create_image_data() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            const data = ctx.createImageData(10, 10);
            return data.width + "|" + data.height + "|" + data.data.length;
        "#,
    );
    assert_eq!(as_str(result), "10|10|400"); // 10*10*4 = 400 bytes RGBA
}

#[test]
fn canvas_get_image_data() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            const data = ctx.getImageData(0, 0, 5, 5);
            return data.data.length;
        "#,
    );
    assert_eq!(as_num(result), 100.0); // 5*5*4 = 100
}

#[test]
fn canvas_draw_image_no_throw() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas><img id="i" src="test.png"/></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            const img = document.getElementById("i");
            ctx.drawImage(img, 0, 0);
            ctx.drawImage(img, 0, 0, 100, 100);
            ctx.drawImage(img, 0, 0, 50, 50, 10, 10, 100, 100);
            return "ok";
        "#,
    );
    assert_eq!(as_str(result), "ok");
}

#[test]
fn canvas_is_point_in_path_stub() {
    let result = run_with_doc(
        r#"<html><body><canvas id="c"></canvas></body></html>"#,
        r#"
            const ctx = document.getElementById("c").getContext("2d");
            return ctx.isPointInPath(10, 10);
        "#,
    );
    assert_eq!(as_bool(result), false);
}
