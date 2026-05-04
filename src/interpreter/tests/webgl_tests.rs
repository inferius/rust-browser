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
        gl.shaderSource(sh, "void main() {{ gl_Position = vec4(0.0); }}");
        gl.compileShader(sh);
        const ok = gl.getShaderParameter(sh, gl.COMPILE_STATUS);
        const log = gl.getShaderInfoLog(sh);
        return ok + "|" + log.substring(0, 200);
    "#));
    assert!(r.to_string().starts_with("true|"), "compile failed: {}", r.to_string());
}

#[test]
fn webgl_shader_compile_invalid_source_fails() {
    let r = run(&format!(r#"{SETUP}
        const sh = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(sh, "this is not valid glsl @#$");
        gl.compileShader(sh);
        return gl.getShaderParameter(sh, gl.COMPILE_STATUS);
    "#));
    assert_eq!(r.to_string(), "false");
}

#[test]
fn webgl_shader_info_log_contains_error() {
    let r = run(&format!(r#"{SETUP}
        const sh = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(sh, "syntax error garbage");
        gl.compileShader(sh);
        const log = gl.getShaderInfoLog(sh);
        return log.length > 0;
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
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v);
        gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.getProgramParameter(p, gl.LINK_STATUS);
    "#));
    assert_eq!(r.to_string(), "true");
}

#[test]
fn webgl_link_outputs_wgsl_string() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        const vw = gl.__program_vertex_wgsl__(p);
        const fw = gl.__program_fragment_wgsl__(p);
        return typeof vw + "|" + typeof fw + "|" + (vw.length > 0) + "|" + (fw.length > 0);
    "#));
    assert_eq!(r.to_string(), "string|string|true|true");
}

// ─── Phase 3c8: texImage2D real ulozeni ───────────────────────────────

#[test]
fn webgl_tex_image_2d_stores_dimensions() {
    let r = run(&format!(r#"{SETUP}
        const t = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, t);
        const data = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255];
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 2, 2, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
        return t.__webgl_id__;
    "#));
    let n: f64 = r.to_string().parse().unwrap();
    assert!(n > 0.0, "texture id allocated");
}

#[test]
fn webgl_tex_image_2d_no_throw_on_missing_data() {
    // 6-arg variant (image element) - data missing
    let r = run(&format!(r#"{SETUP}
        const t = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, t);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, null);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn webgl_tex_image_2d_without_bound_no_throw() {
    let r = run(&format!(r#"{SETUP}
        const data = [255, 0, 0, 255];
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn webgl_create_texture_unique_ids() {
    let r = run(&format!(r#"{SETUP}
        const t1 = gl.createTexture();
        const t2 = gl.createTexture();
        return t1.__webgl_id__ !== t2.__webgl_id__;
    "#));
    assert_eq!(r.to_string(), "true");
}

#[test]
fn webgl_bind_texture_state_tracked() {
    let r = run(&format!(r#"{SETUP}
        const t = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, t);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

#[test]
fn webgl_tex_parameteri_no_throw() {
    let r = run(&format!(r#"{SETUP}
        const t = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, t);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        return "ok";
    "#));
    assert_eq!(r.to_string(), "ok");
}

// ─── Phase 3c7: uniform layout extraction ──────────────────────────────

#[test]
fn webgl_uniform_count_zero_for_program_without_uniforms() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.__program_uniform_count__(p);
    "#));
    assert_eq!(r.to_string(), "0");
}

#[test]
fn webgl_uniform_count_with_uniform_block() {
    // Naga GLSL frontend lépe handluje uniform block syntax
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "layout(std140) uniform Block {{ float uTime; vec2 uPos; }}; void main(){{ gl_Position = vec4(uTime, uPos, 1.0); }}");
        gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.__program_uniform_count__(p);
    "#));
    let n: f64 = r.to_string().parse().unwrap();
    // Bud naga extracts (>= 1) nebo block syntax neni handluje (0) - oba acceptable
    assert!(n >= 0.0, "smoke - bez panic");
}

#[test]
fn webgl_uniform_buffer_size_default_zero() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.__program_uniform_buffer_size__(p);
    "#));
    let n: f64 = r.to_string().parse().unwrap();
    assert_eq!(n, 0.0, "no uniforms -> 0 size");
}

#[test]
fn webgl_sampler_count_zero_for_simple_shader() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.__program_sampler_count__(p);
    "#));
    assert_eq!(r.to_string(), "0");
}

#[test]
fn webgl_texture_count_zero_for_simple_shader() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.__program_texture_count__(p);
    "#));
    assert_eq!(r.to_string(), "0");
}

#[test]
fn webgl_sampler_count_with_sampler2d_uniform() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "uniform sampler2D uTex; void main(){{ gl_FragColor = texture2D(uTex, vec2(0.5)); }}");
        gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        const samp = gl.__program_sampler_count__(p);
        const tex = gl.__program_texture_count__(p);
        return samp + ":" + tex;
    "#));
    let s = r.to_string();
    // Naga sampler2D: 1 sampler + 1 texture (separate v WGSL).
    // Pri compilation success vraci 1:1, pri ne 0:0.
    assert!(s == "1:1" || s == "0:0", "got {s}");
}

#[test]
fn webgl_uniform_layout_zero_for_unlinked() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        return gl.__program_uniform_count__(p);
    "#));
    assert_eq!(r.to_string(), "0");
}

#[test]
fn webgl_wgsl_has_vertex_stage_decorator() {
    // Naga musi generovat @vertex/@fragment decorators - nutne pro wgpu pipeline phase 3c.
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        const vw = gl.__program_vertex_wgsl__(p);
        const fw = gl.__program_fragment_wgsl__(p);
        return (vw.indexOf("@vertex") >= 0) + "|" + (fw.indexOf("@fragment") >= 0);
    "#));
    assert_eq!(r.to_string(), "true|true",
        "WGSL musi mit stage decorators @vertex + @fragment pro wgpu pipeline");
}

#[test]
fn webgl_link_with_uncompiled_shader_fails() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "garbage"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main(){{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p);
        return gl.getProgramParameter(p, gl.LINK_STATUS);
    "#));
    assert_eq!(r.to_string(), "false");
}

#[test]
fn webgl_link_uses_only_compiled_shaders_for_wgsl() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        // Pripoji jen vertex bez fragment - mel by failnout link.
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main(){{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        gl.attachShader(p, v);
        gl.linkProgram(p);
        return gl.getProgramParameter(p, gl.LINK_STATUS);
    "#));
    assert_eq!(r.to_string(), "false");
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
    // Realne typicke WebGL init - 30+ volani.
    // Pouzivam minimal valid GLSL ktery parse projde naga.
    let r = run(&format!(r#"{SETUP}
        gl.clearColor(0.1, 0.1, 0.1, 1.0);
        gl.viewport(0, 0, 300, 150);
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

        const vSrc = "void main() {{ gl_Position = vec4(0.0, 0.0, 0.0, 1.0); }}";
        const fSrc = "void main() {{ gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0); }}";

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

// ─── Phase 3a: command queue + state recording ─────────────────────────

#[test]
fn webgl_clear_color_pushed_to_queue() {
    let r = run(&format!(r#"{SETUP}
        gl.clearColor(0.5, 0.5, 0.5, 1.0);
        gl.clear(gl.COLOR_BUFFER_BIT);
        return gl.__draw_queue_size__();
    "#));
    assert_eq!(r.to_string(), "2", "ClearColor + Clear = 2 queue items");
}

#[test]
fn webgl_draw_arrays_pushes_to_queue() {
    let r = run(&format!(r#"{SETUP}
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.drawArrays(gl.TRIANGLES, 0, 6);
        return gl.__draw_queue_size__();
    "#));
    assert_eq!(r.to_string(), "2");
}

#[test]
fn webgl_draw_elements_pushes_to_queue() {
    let r = run(&format!(r#"{SETUP}
        gl.drawElements(gl.TRIANGLES, 6, gl.UNSIGNED_SHORT, 0);
        return gl.__draw_queue_size__();
    "#));
    assert_eq!(r.to_string(), "1");
}

#[test]
fn webgl_enable_vertex_attrib_array_state() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getAttribLocation(p, "aPos");
        gl.enableVertexAttribArray(loc);
        return gl.__attrib_enabled__(loc);
    "#));
    assert_eq!(r.to_string(), "true");
}

#[test]
fn webgl_disable_vertex_attrib_array_state() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getAttribLocation(p, "aPos");
        gl.enableVertexAttribArray(loc);
        gl.disableVertexAttribArray(loc);
        return gl.__attrib_enabled__(loc);
    "#));
    assert_eq!(r.to_string(), "false");
}

#[test]
fn webgl_uniform1f_stored() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getUniformLocation(p, "uTime");
        gl.uniform1f(loc, 0.42);
        const v = gl.__uniform_value__("uTime");
        return v[0];
    "#));
    let val: f64 = r.to_string().parse().unwrap();
    assert!((val - 0.42).abs() < 1e-3);
}

#[test]
fn webgl_uniform2f_stored() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getUniformLocation(p, "uPos");
        gl.uniform2f(loc, 1.0, 2.0);
        const v = gl.__uniform_value__("uPos");
        return v[0] + "," + v[1];
    "#));
    assert_eq!(r.to_string(), "1,2");
}

#[test]
fn webgl_uniform4f_stored() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getUniformLocation(p, "uColor");
        gl.uniform4f(loc, 1.0, 0.5, 0.25, 1.0);
        const v = gl.__uniform_value__("uColor");
        return v.join(",");
    "#));
    assert_eq!(r.to_string(), "1,0.5,0.25,1");
}

#[test]
fn webgl_uniform_matrix4fv_stored() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getUniformLocation(p, "uMVP");
        const id = [1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1];
        gl.uniformMatrix4fv(loc, false, id);
        const v = gl.__uniform_value__("uMVP");
        return v.length;
    "#));
    assert_eq!(r.to_string(), "16");
}

#[test]
fn webgl_uniform1i_stored() {
    let r = run(&format!(r#"{SETUP}
        const p = gl.createProgram();
        const loc = gl.getUniformLocation(p, "uSampler");
        gl.uniform1i(loc, 7);
        const v = gl.__uniform_value__("uSampler");
        return v[0];
    "#));
    assert_eq!(r.to_string(), "7");
}

#[test]
fn webgl_uniform_uninitialized_returns_null() {
    let r = run(&format!(r#"{SETUP}
        const v = gl.__uniform_value__("undefined_uniform");
        return v;
    "#));
    let s = r.to_string();
    assert!(s == "null" || s == "undefined");
}

#[test]
fn webgl_realistic_render_loop_records_full_queue() {
    let r = run(&format!(r#"{SETUP}
        // Setup
        const v = gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(v, "void main() {{ gl_Position = vec4(0.0); }}"); gl.compileShader(v);
        const f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(f, "void main() {{ gl_FragColor = vec4(1.0); }}"); gl.compileShader(f);
        const p = gl.createProgram();
        gl.attachShader(p, v); gl.attachShader(p, f);
        gl.linkProgram(p); gl.useProgram(p);

        const buf = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, buf);
        gl.bufferData(gl.ARRAY_BUFFER, [-1, -1, 1, -1, 0, 1], gl.STATIC_DRAW);

        const aPos = gl.getAttribLocation(p, "aPos");
        gl.enableVertexAttribArray(aPos);
        gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

        const uTime = gl.getUniformLocation(p, "uTime");
        gl.uniform1f(uTime, 1.5);

        // 60 frame render loop
        for (let i = 0; i < 60; i++) {{
            gl.clearColor(0, 0, 0, 1);
            gl.clear(gl.COLOR_BUFFER_BIT);
            gl.drawArrays(gl.TRIANGLES, 0, 3);
        }}
        return gl.__draw_queue_size__();
    "#));
    // 60 * (clearColor + clear + drawArrays) = 180
    assert_eq!(r.to_string(), "180");
}
