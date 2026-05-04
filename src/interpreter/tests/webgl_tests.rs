/// Testy pro WebGL phase 1 - state machine + handle objects + clear color.

use super::helpers::*;
use crate::interpreter::JsValue;

const SETUP: &str = r#"
const c = document.createElement("canvas");
const gl = c.getContext("webgl");
"#;

#[test]
fn webgl_context_constants() {
    let r = run(&format!("{SETUP}return gl.VERTEX_SHADER;"));
    assert_eq!(r.to_string(), "35633"); // 0x8B31
}

#[test]
fn webgl_fragment_shader_constant() {
    let r = run(&format!("{SETUP}return gl.FRAGMENT_SHADER;"));
    assert_eq!(r.to_string(), "35632"); // 0x8B30
}

#[test]
fn webgl_array_buffer_constant() {
    let r = run(&format!("{SETUP}return gl.ARRAY_BUFFER;"));
    assert_eq!(r.to_string(), "34962"); // 0x8892
}

#[test]
fn webgl_clear_color_stored() {
    let r = run(&format!(r#"{SETUP}
        gl.clearColor(0.5, 0.25, 0.75, 1.0);
        const cc = gl.__clear_color__();
        return cc[0] + "," + cc[1] + "," + cc[2] + "," + cc[3];
    "#));
    assert_eq!(r.to_string(), "0.5,0.25,0.75,1");
}

#[test]
fn webgl_create_shader_returns_handle() {
    let r = run(&format!(r#"{SETUP}
        const sh = gl.createShader(gl.VERTEX_SHADER);
        return typeof sh.__webgl_id__;
    "#));
    assert_eq!(r.to_string(), "number");
}

#[test]
fn webgl_create_shader_unique_ids() {
    let r = run(&format!(r#"{SETUP}
        const a = gl.createShader(gl.VERTEX_SHADER);
        const b = gl.createShader(gl.FRAGMENT_SHADER);
        return a.__webgl_id__ !== b.__webgl_id__;
    "#));
    assert_eq!(r.to_string(), "true");
}

#[test]
fn webgl_shader_compile_with_source() {
    let r = run(&format!(r#"{SETUP}
        const sh = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(sh, "void main() {{}}");
        gl.compileShader(sh);
        return gl.getShaderParameter(sh, gl.COMPILE_STATUS);
    "#));
    assert_eq!(r.to_string(), "true");
}

#[test]
fn webgl_shader_compile_empty_source_fails() {
    let r = run(&format!(r#"{SETUP}
        const sh = gl.createShader(gl.VERTEX_SHADER);
        gl.compileShader(sh);
        return gl.getShaderParameter(sh, gl.COMPILE_STATUS);
    "#));
    assert_eq!(r.to_string(), "false");
}

#[test]
fn webgl_create_program_link_with_shaders() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{}}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{}}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v);
        gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.getProgramParameter(p, gl.LINK_STATUS);
    "#));
    assert_eq!(r.to_string(), "true");
}

#[test]
fn webgl_link_without_shaders_fails() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        gl.linkProgram(p);
        return gl.getProgramParameter(p, gl.LINK_STATUS);
    "#));
    assert_eq!(r.to_string(), "false");
}

#[test]
fn webgl_create_buffer_returns_handle() {
    let r = run(&format!(r#"{SETUP}
        const b = gl.createBuffer();
        return typeof b.__webgl_id__;
    "#));
    assert_eq!(r.to_string(), "number");
}

#[test]
fn webgl_buffer_data_via_array() {
    let r = run(&format!(r#"{SETUP}
        const b = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, b);
        gl.bufferData(gl.ARRAY_BUFFER, [1.0, 2.0, 3.0, 4.0], gl.STATIC_DRAW);
        return typeof b.__webgl_id__;
    "#));
    assert_eq!(r.to_string(), "number");
}

#[test]
fn webgl_create_texture_returns_handle() {
    let r = run(&format!(r#"{SETUP}
        const t = gl.createTexture();
        return t.__webgl_kind__;
    "#));
    assert_eq!(r.to_string(), "texture");
}

#[test]
fn webgl_use_program_no_throw() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        gl.useProgram(p);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn webgl_draw_arrays_increments_counter() {
    let r = run(&format!(r#"{SETUP}
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.drawArrays(gl.TRIANGLES, 0, 6);
        gl.drawArrays(gl.TRIANGLES, 0, 9);
        return gl.__draw_call_count__();
    "#));
    assert_eq!(r.to_string(), "3");
}

#[test]
fn webgl_draw_elements_increments_counter() {
    let r = run(&format!(r#"{SETUP}
        gl.drawElements(gl.TRIANGLES, 3, gl.UNSIGNED_SHORT, 0);
        gl.drawElements(gl.TRIANGLES, 6, gl.UNSIGNED_SHORT, 0);
        return gl.__draw_call_count__();
    "#));
    assert_eq!(r.to_string(), "2");
}

#[test]
fn webgl_get_attrib_location_returns_number() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getAttribLocation(p, "aPosition");
        return typeof loc;
    "#));
    assert_eq!(r.to_string(), "number");
}

#[test]
fn webgl_get_uniform_location_returns_object() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getUniformLocation(p, "uColor");
        return typeof loc;
    "#));
    assert_eq!(r.to_string(), "object");
}

#[test]
fn webgl_get_parameter_renderer_string() {
    // GL_RENDERER = 0x1F01
    let r = run(&format!(r#"{SETUP}
        return gl.getParameter(0x1F01);
    "#));
    assert!(r.to_string().contains("RustWebEngine"));
}

#[test]
fn webgl_no_error_after_init() {
    let r = run(&format!(r#"{SETUP}
        return gl.getError();
    "#));
    assert_eq!(r.to_string(), "0"); // NO_ERROR
}

#[test]
fn webgl_enable_blend_no_throw() {
    let r = run(&format!(r#"{SETUP}
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn webgl_viewport_no_throw() {
    let r = run(&format!(r#"{SETUP}
        gl.viewport(0, 0, 800, 600);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn webgl_full_init_sequence() {
    // Realne typicke WebGL init - 30+ volani
    let r = run(&format!(r#"{SETUP}
        gl.clearColor(0.1, 0.1, 0.1, 1.0);
        gl.viewport(0, 0, 300, 150);
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

        const vSrc = "attribute vec2 aPos; void main() {{ gl_Position = vec4(aPos, 0, 1); }}";
        const fSrc = "void main() {{ gl_FragColor = vec4(1, 0, 0, 1); }}";

        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, vSrc); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, fSrc); gl.compileShader(f);

        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        gl.useProgram(p);

        const buf = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, buf);
        gl.bufferData(gl.ARRAY_BUFFER, [-1, -1, 1, -1, 0, 1], gl.STATIC_DRAW);

        const aPos = gl.getAttribLocation(p, "aPos");
        gl.enableVertexAttribArray(aPos);
        gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

        gl.clear(gl.COLOR_BUFFER_BIT);
        gl.drawArrays(gl.TRIANGLES, 0, 3);

        return gl.getError();
    "#));
    assert_eq!(r.to_string(), "0", "no errors at end of typical init");
}

#[test]
fn webgl_canvas_drawing_buffer_size() {
    let r = run(&format!(r#"{SETUP}
        return gl.drawingBufferWidth + "x" + gl.drawingBufferHeight;
    "#));
    assert_eq!(r.to_string(), "300x150");
}

#[test]
fn webgl_get_extension_returns_null() {
    let r = run(&format!(r#"{SETUP}
        return gl.getExtension("OES_texture_float");
    "#));
    let s = r.to_string();
    assert!(s == "null" || s == "undefined");
}

#[test]
fn webgl_is_context_lost_false() {
    let r = run(&format!(r#"{SETUP}
        return gl.isContextLost();
    "#));
    assert_eq!(r.to_string(), "false");
}
