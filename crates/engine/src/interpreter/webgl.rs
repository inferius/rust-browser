//! WebGL JS API + GLSL preprocessing + naga -> WGSL compile.
//! Extrahovano z mod.rs (Iter 267 refactor).

use std::rc::Rc;
use std::cell::RefCell;
use super::{JsValue, JsObject};
use super::helpers::native;

/// Pre-processing GLSL ES 1.0 (WebGL 1.0) -> GLSL ES 3.0 (WebGL 2.0)
/// pro lepsi naga compatibility. WebGL 1.0 bez `#version` line implicitne
/// 1.0 ES s `attribute`/`varying`/`gl_FragColor`. Naga GLSL frontend lepe
/// zvlada 300 es se `in`/`out`.
pub(crate) fn preprocess_glsl_es1_to_es3(source: &str, shader_type: u32) -> String {
    let trimmed = source.trim_start();
    if trimmed.starts_with("#version") {
        return source.to_string();  // user explicit verzi - ne sahat
    }
    let is_vertex = shader_type == 0x8B31;
    let is_fragment = shader_type == 0x8B30;
    let mut out = String::with_capacity(source.len() + 200);
    // Naga GLSL frontend nepodporuje "300 es". Pouzijeme desktop 330
    // (in/out keywords + bezne types). Precision qualifiers se ignoruji.
    out.push_str("#version 450 core\n");
    if is_fragment {
        out.push_str("out vec4 _gl_FragColor;\n");
    }
    // Replace deprecated keywords. Naivni - nedotyka se v komentari/string,
    // ale bez full GLSL preprocesoru je toto rozumne kompromis.
    let mut transformed = source.to_string();
    if is_vertex {
        transformed = transformed.replace("attribute ", "in ");
        transformed = transformed.replace("varying ", "out ");
    } else if is_fragment {
        transformed = transformed.replace("varying ", "in ");
        transformed = transformed.replace("gl_FragColor", "_gl_FragColor");
        transformed = transformed.replace("texture2D(", "texture(");
    }
    out.push_str(&transformed);
    out
}

/// Compile GLSL source pres naga -> Module + validovany.
/// Vraci (Module, info_log) pri uspechu, nebo (None, error_log) pri fail.
/// Aplikuje GLSL ES 1.0 -> 3.0 pre-processing pri chybejicim #version.
pub(crate) fn compile_glsl_to_naga(source: &str, shader_type: u32) -> (Option<naga::Module>, String) {
    use naga::front::glsl;
    use naga::ShaderStage;
    let stage = match shader_type {
        0x8B31 => ShaderStage::Vertex,    // VERTEX_SHADER
        0x8B30 => ShaderStage::Fragment,  // FRAGMENT_SHADER
        _ => return (None, format!("nepodporovany shader type 0x{:X}", shader_type)),
    };
    let processed = preprocess_glsl_es1_to_es3(source, shader_type);
    let mut frontend = glsl::Frontend::default();
    let options = glsl::Options { stage, defines: Default::default() };
    match frontend.parse(&options, &processed) {
        Ok(module) => (Some(module), String::new()),
        Err(errors) => {
            let log = errors.errors.iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            (None, log)
        }
    }
}

/// Konvertuj naga Module na WGSL string.
pub(crate) fn naga_module_to_wgsl(module: &naga::Module) -> Result<String, String> {
    use naga::back::wgsl;
    use naga::valid::{Validator, ValidationFlags, Capabilities};
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(module)
        .map_err(|e| format!("validace: {e:?}"))?;
    wgsl::write_string(module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| format!("wgsl write: {e:?}"))
}

/// Vyrobi WebGL handle objekt (buffer/texture/shader/program) s __webgl_id__.
pub(crate) fn make_webgl_handle(id: u32, kind: &str) -> JsValue {
    let obj = Rc::new(RefCell::new(JsObject::new()));
    obj.borrow_mut().set("__webgl_id__".into(), JsValue::Number(id as f64));
    obj.borrow_mut().set("__webgl_kind__".into(), JsValue::Str(kind.into()));
    JsValue::Object(obj)
}

/// Vyextrahuje __webgl_id__ z handle objektu.
pub(crate) fn webgl_id_from(value: &JsValue) -> Option<u32> {
    if let JsValue::Object(o) = value {
        if let Some(JsValue::Number(n)) = o.borrow().props.get("__webgl_id__") {
            return Some(*n as u32);
        }
    }
    None
}

pub(crate) fn create_webgl_context(state: Rc<RefCell<WebGLState>>) -> JsValue {
    let obj_rc = Rc::new(RefCell::new(JsObject::new()));
    // Klicove WebGL constants (alespon tie nejcastejsi)
    let constants = [
        ("VERTEX_SHADER", 0x8B31), ("FRAGMENT_SHADER", 0x8B30),
        ("ARRAY_BUFFER", 0x8892), ("ELEMENT_ARRAY_BUFFER", 0x8893),
        ("STATIC_DRAW", 0x88E4), ("DYNAMIC_DRAW", 0x88E8),
        ("FLOAT", 0x1406), ("UNSIGNED_INT", 0x1405), ("UNSIGNED_SHORT", 0x1403),
        ("TRIANGLES", 0x0004), ("TRIANGLE_STRIP", 0x0005), ("LINES", 0x0001),
        ("COLOR_BUFFER_BIT", 0x4000), ("DEPTH_BUFFER_BIT", 0x0100),
        ("DEPTH_TEST", 0x0B71), ("BLEND", 0x0BE2),
        ("TEXTURE_2D", 0x0DE1), ("TEXTURE0", 0x84C0),
        ("RGBA", 0x1908), ("RGB", 0x1907),
        ("UNSIGNED_BYTE", 0x1401),
        ("LINEAR", 0x2601), ("NEAREST", 0x2600),
        ("CLAMP_TO_EDGE", 0x812F), ("REPEAT", 0x2901),
        ("COMPILE_STATUS", 0x8B81), ("LINK_STATUS", 0x8B82),
        ("NO_ERROR", 0x0000),
        ("SRC_ALPHA", 0x0302), ("ONE_MINUS_SRC_ALPHA", 0x0303), ("ONE", 0x0001), ("ZERO", 0x0000),
        ("LESS", 0x0201), ("LEQUAL", 0x0203), ("ALWAYS", 0x0207),
        ("CCW", 0x0901), ("CW", 0x0900),
        ("BACK", 0x0405), ("FRONT", 0x0404),
        ("TEXTURE_MIN_FILTER", 0x2801), ("TEXTURE_MAG_FILTER", 0x2800),
        ("TEXTURE_WRAP_S", 0x2802), ("TEXTURE_WRAP_T", 0x2803),
    ];
    for (name, val) in &constants {
        obj_rc.borrow_mut().set(name.to_string(), JsValue::Number(*val as f64));
    }
    // WebGL2 constants - vsechny additions oproti WebGL1.
    let webgl2_constants: &[(&str, u32)] = &[
        // Buffer types
        ("UNIFORM_BUFFER", 0x8A11), ("COPY_READ_BUFFER", 0x8F36),
        ("COPY_WRITE_BUFFER", 0x8F37), ("TRANSFORM_FEEDBACK_BUFFER", 0x8C8E),
        ("PIXEL_PACK_BUFFER", 0x88EB), ("PIXEL_UNPACK_BUFFER", 0x88EC),
        // Texture
        ("TEXTURE_2D_ARRAY", 0x8C1A), ("TEXTURE_3D", 0x806F),
        ("TEXTURE_BINDING_2D_ARRAY", 0x8C1D),
        ("RGBA32F", 0x8814), ("RGB32F", 0x8815),
        ("RGBA16F", 0x881A), ("RGB16F", 0x881B),
        ("R8", 0x8229), ("RG8", 0x822B), ("RGBA8", 0x8058),
        ("R32F", 0x822E), ("RG32F", 0x8230),
        ("DEPTH_COMPONENT24", 0x81A6), ("DEPTH_COMPONENT32F", 0x8CAC),
        ("DEPTH24_STENCIL8", 0x88F0), ("RED", 0x1903), ("RG", 0x8227),
        ("HALF_FLOAT", 0x140B), ("FLOAT_VEC2", 0x8B50),
        ("UNSIGNED_INT_2_10_10_10_REV", 0x8368),
        ("UNSIGNED_INT_24_8", 0x84FA), ("FLOAT_32_UNSIGNED_INT_24_8_REV", 0x8DAD),
        // Color attachments (MRT)
        ("COLOR_ATTACHMENT0", 0x8CE0), ("COLOR_ATTACHMENT1", 0x8CE1),
        ("COLOR_ATTACHMENT2", 0x8CE2), ("COLOR_ATTACHMENT3", 0x8CE3),
        ("COLOR_ATTACHMENT4", 0x8CE4), ("COLOR_ATTACHMENT5", 0x8CE5),
        ("COLOR_ATTACHMENT6", 0x8CE6), ("COLOR_ATTACHMENT7", 0x8CE7),
        ("DEPTH_ATTACHMENT", 0x8D00), ("STENCIL_ATTACHMENT", 0x8D20),
        ("READ_FRAMEBUFFER", 0x8CA8), ("DRAW_FRAMEBUFFER", 0x8CA9),
        // Sampler
        ("SAMPLER_BINDING", 0x8919), ("SAMPLER_2D_ARRAY", 0x8DC1),
        ("SAMPLER_3D", 0x8B5F), ("SAMPLER_2D_SHADOW", 0x8B62),
        // Transform feedback
        ("TRANSFORM_FEEDBACK", 0x8E22), ("INTERLEAVED_ATTRIBS", 0x8C8C),
        ("SEPARATE_ATTRIBS", 0x8C8D),
        // Sync
        ("SYNC_GPU_COMMANDS_COMPLETE", 0x9117),
        ("SIGNALED", 0x9119), ("UNSIGNALED", 0x9118),
        // Misc
        ("MAX_3D_TEXTURE_SIZE", 0x8073), ("MAX_ARRAY_TEXTURE_LAYERS", 0x88FF),
        ("MAX_VERTEX_UNIFORM_BLOCKS", 0x8A2B),
        ("MAX_FRAGMENT_UNIFORM_BLOCKS", 0x8A2D),
        ("UNIFORM_BUFFER_OFFSET_ALIGNMENT", 0x8A34),
    ];
    for (name, val) in webgl2_constants {
        obj_rc.borrow_mut().set(name.to_string(), JsValue::Number(*val as f64));
    }
    // canvas property (minimal stub)
    obj_rc.borrow_mut().set("drawingBufferWidth".into(), JsValue::Number(300.0));
    obj_rc.borrow_mut().set("drawingBufferHeight".into(), JsValue::Number(150.0));

    // ─── WebGL2 method stubs ─────────────────────────────────────────
    // VAO (Vertex Array Object) - drzi vertex attrib state.
    let vao_counter = Rc::new(RefCell::new(0u32));
    {
        let counter = Rc::clone(&vao_counter);
        obj_rc.borrow_mut().set("createVertexArray".into(),
            native("createVertexArray", move |_| {
                let mut c = counter.borrow_mut();
                *c += 1;
                let mut o = JsObject::new();
                o.set("__vao__".into(), JsValue::Number(*c as f64));
                Ok(JsValue::Object(Rc::new(RefCell::new(o))))
            }));
    }
    obj_rc.borrow_mut().set("bindVertexArray".into(),
        native("bindVertexArray", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("deleteVertexArray".into(),
        native("deleteVertexArray", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("isVertexArray".into(),
        native("isVertexArray", |_| Ok(JsValue::Bool(true))));
    // Instancing
    obj_rc.borrow_mut().set("drawArraysInstanced".into(),
        native("drawArraysInstanced", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("drawElementsInstanced".into(),
        native("drawElementsInstanced", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("vertexAttribDivisor".into(),
        native("vertexAttribDivisor", |_| Ok(JsValue::Undefined)));
    // UBO (Uniform Buffer Objects)
    obj_rc.borrow_mut().set("getUniformBlockIndex".into(),
        native("getUniformBlockIndex", |_| Ok(JsValue::Number(0.0))));
    obj_rc.borrow_mut().set("uniformBlockBinding".into(),
        native("uniformBlockBinding", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("bindBufferBase".into(),
        native("bindBufferBase", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("bindBufferRange".into(),
        native("bindBufferRange", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("getActiveUniformBlockParameter".into(),
        native("getActiveUniformBlockParameter", |_| Ok(JsValue::Number(0.0))));
    obj_rc.borrow_mut().set("getActiveUniformBlockName".into(),
        native("getActiveUniformBlockName", |_| Ok(JsValue::Str(String::new()))));
    // Sampler objects
    let sampler_counter = Rc::new(RefCell::new(0u32));
    {
        let counter = Rc::clone(&sampler_counter);
        obj_rc.borrow_mut().set("createSampler".into(),
            native("createSampler", move |_| {
                let mut c = counter.borrow_mut();
                *c += 1;
                let mut o = JsObject::new();
                o.set("__sampler__".into(), JsValue::Number(*c as f64));
                Ok(JsValue::Object(Rc::new(RefCell::new(o))))
            }));
    }
    obj_rc.borrow_mut().set("bindSampler".into(),
        native("bindSampler", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("samplerParameteri".into(),
        native("samplerParameteri", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("samplerParameterf".into(),
        native("samplerParameterf", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("deleteSampler".into(),
        native("deleteSampler", |_| Ok(JsValue::Undefined)));
    // Transform feedback
    obj_rc.borrow_mut().set("createTransformFeedback".into(),
        native("createTransformFeedback", |_| {
            let mut o = JsObject::new();
            o.set("__tf__".into(), JsValue::Bool(true));
            Ok(JsValue::Object(Rc::new(RefCell::new(o))))
        }));
    obj_rc.borrow_mut().set("bindTransformFeedback".into(),
        native("bindTransformFeedback", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("beginTransformFeedback".into(),
        native("beginTransformFeedback", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("endTransformFeedback".into(),
        native("endTransformFeedback", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("transformFeedbackVaryings".into(),
        native("transformFeedbackVaryings", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("getTransformFeedbackVarying".into(),
        native("getTransformFeedbackVarying", |_| Ok(JsValue::Null)));
    // MRT (Multi Render Target)
    obj_rc.borrow_mut().set("drawBuffers".into(),
        native("drawBuffers", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("readBuffer".into(),
        native("readBuffer", |_| Ok(JsValue::Undefined)));
    // Texture storage
    obj_rc.borrow_mut().set("texStorage2D".into(),
        native("texStorage2D", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("texStorage3D".into(),
        native("texStorage3D", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("texImage3D".into(),
        native("texImage3D", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("texSubImage3D".into(),
        native("texSubImage3D", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("copyTexSubImage3D".into(),
        native("copyTexSubImage3D", |_| Ok(JsValue::Undefined)));
    // Framebuffer
    obj_rc.borrow_mut().set("framebufferTextureLayer".into(),
        native("framebufferTextureLayer", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("blitFramebuffer".into(),
        native("blitFramebuffer", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("invalidateFramebuffer".into(),
        native("invalidateFramebuffer", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("invalidateSubFramebuffer".into(),
        native("invalidateSubFramebuffer", |_| Ok(JsValue::Undefined)));
    // Uniforms (vetsi range)
    for m in &["uniform1ui", "uniform2ui", "uniform3ui", "uniform4ui",
               "uniform1uiv", "uniform2uiv", "uniform3uiv", "uniform4uiv",
               "uniformMatrix2x3fv", "uniformMatrix3x2fv",
               "uniformMatrix2x4fv", "uniformMatrix4x2fv",
               "uniformMatrix3x4fv", "uniformMatrix4x3fv",
               "vertexAttribI4i", "vertexAttribI4iv",
               "vertexAttribI4ui", "vertexAttribI4uiv",
               "vertexAttribIPointer"] {
        obj_rc.borrow_mut().set(m.to_string(),
            native(m, |_| Ok(JsValue::Undefined)));
    }
    // Sync objects
    obj_rc.borrow_mut().set("fenceSync".into(), native("fenceSync", |_| {
        let mut o = JsObject::new();
        o.set("__sync__".into(), JsValue::Bool(true));
        Ok(JsValue::Object(Rc::new(RefCell::new(o))))
    }));
    obj_rc.borrow_mut().set("clientWaitSync".into(),
        native("clientWaitSync", |_| Ok(JsValue::Number(0.0))));
    obj_rc.borrow_mut().set("waitSync".into(),
        native("waitSync", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("deleteSync".into(),
        native("deleteSync", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("isSync".into(),
        native("isSync", |_| Ok(JsValue::Bool(true))));
    // Query
    obj_rc.borrow_mut().set("createQuery".into(), native("createQuery", |_| {
        let mut o = JsObject::new();
        o.set("__query__".into(), JsValue::Bool(true));
        Ok(JsValue::Object(Rc::new(RefCell::new(o))))
    }));
    obj_rc.borrow_mut().set("beginQuery".into(),
        native("beginQuery", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("endQuery".into(),
        native("endQuery", |_| Ok(JsValue::Undefined)));
    obj_rc.borrow_mut().set("getQueryParameter".into(),
        native("getQueryParameter", |_| Ok(JsValue::Number(0.0))));
    obj_rc.borrow_mut().set("deleteQuery".into(),
        native("deleteQuery", |_| Ok(JsValue::Undefined)));

    // ─── State setters ──────────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("clearColor".into(), native("gl.clearColor", move |args| {
            let mut it = args.into_iter();
            let r = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let g = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let b = it.next().map(|v| v.to_number()).unwrap_or(0.0) as f32;
            let a = it.next().map(|v| v.to_number()).unwrap_or(1.0) as f32;
            let mut s = st.borrow_mut();
            s.clear_color = [r, g, b, a];
            s.draw_queue.push(WebGLDrawCmd::ClearColor([r, g, b, a]));
            if std::env::var("RWE_WEBGL_DBG").is_ok() {
                eprintln!("[webgl] clearColor({},{},{},{}) - queue.len={}",
                    r, g, b, a, s.draw_queue.len());
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("clear".into(), native("gl.clear", move |args| {
            let mask = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let mut s = st.borrow_mut();
            s.draw_call_count += 1;
            s.draw_queue.push(WebGLDrawCmd::Clear(mask));
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("viewport".into(), native("gl.viewport", move |args| {
            let mut it = args.into_iter();
            let x = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let y = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            st.borrow_mut().viewport_xywh = [x, y, w, h];
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("enable".into(), native("gl.enable", move |args| {
            let cap = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            match cap {
                0x0BE2 => st.borrow_mut().blend_enabled = true,
                0x0B71 => st.borrow_mut().depth_test_enabled = true,
                _ => {}
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("disable".into(), native("gl.disable", move |args| {
            let cap = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            match cap {
                0x0BE2 => st.borrow_mut().blend_enabled = false,
                0x0B71 => st.borrow_mut().depth_test_enabled = false,
                _ => {}
            }
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Shader management ───────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("createShader".into(), native("gl.createShader", move |args| {
            let stype = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let mut s = st.borrow_mut();
            let id = s.alloc_id();
            s.shaders.insert(id, WebGLShader {
                shader_type: stype, source: String::new(), compiled: false,
                info_log: String::new(), naga_module: None,
            });
            Ok(make_webgl_handle(id, "shader"))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("shaderSource".into(), native("gl.shaderSource", move |args| {
            let mut it = args.into_iter();
            let id = it.next().and_then(|v| webgl_id_from(&v));
            let src = it.next().map(|v| v.to_string()).unwrap_or_default();
            if let Some(id) = id {
                if let Some(sh) = st.borrow_mut().shaders.get_mut(&id) {
                    sh.source = src;
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("compileShader".into(), native("gl.compileShader", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                let mut s = st.borrow_mut();
                if let Some(sh) = s.shaders.get_mut(&id) {
                    if sh.source.is_empty() {
                        sh.compiled = false;
                        sh.info_log = "shader source je prazdny".into();
                    } else {
                        // Real GLSL parse pres naga.
                        let (module, log) = compile_glsl_to_naga(&sh.source, sh.shader_type);
                        if let Some(m) = module {
                            sh.naga_module = Some(m);
                            sh.compiled = true;
                            sh.info_log = String::new();
                        } else {
                            sh.compiled = false;
                            sh.info_log = log;
                        }
                    }
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getShaderParameter".into(), native("gl.getShaderParameter", move |args| {
            let mut it = args.into_iter();
            let id = it.next().and_then(|v| webgl_id_from(&v));
            let pname = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            if pname == 0x8B81 { // COMPILE_STATUS
                if let Some(id) = id {
                    if let Some(sh) = st.borrow().shaders.get(&id) {
                        return Ok(JsValue::Bool(sh.compiled));
                    }
                }
            }
            Ok(JsValue::Bool(false))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getShaderInfoLog".into(), native("gl.getShaderInfoLog", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(sh) = st.borrow().shaders.get(&id) {
                    return Ok(JsValue::Str(sh.info_log.clone()));
                }
            }
            Ok(JsValue::Str(String::new()))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("deleteShader".into(), native("gl.deleteShader", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id { st.borrow_mut().shaders.remove(&id); }
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Program management ──────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("createProgram".into(), native("gl.createProgram", move |_| {
            let mut s = st.borrow_mut();
            let id = s.alloc_id();
            s.programs.insert(id, WebGLProgram {
                vertex_shader: None, fragment_shader: None, linked: false,
                info_log: String::new(), vertex_wgsl: None, fragment_wgsl: None,
                uniform_layout: Vec::new(), uniform_buffer_size: 0,
                sampler_count: 0, texture_count: 0,
                uniform_binding: None,
                texture_bindings: Vec::new(),
                sampler_bindings: Vec::new(),
            });
            Ok(make_webgl_handle(id, "program"))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("attachShader".into(), native("gl.attachShader", move |args| {
            let mut it = args.into_iter();
            let prog_id = it.next().and_then(|v| webgl_id_from(&v));
            let sh_id = it.next().and_then(|v| webgl_id_from(&v));
            if let (Some(pid), Some(sid)) = (prog_id, sh_id) {
                let stype = st.borrow().shaders.get(&sid).map(|s| s.shader_type);
                if let Some(prog) = st.borrow_mut().programs.get_mut(&pid) {
                    match stype {
                        Some(0x8B31) => prog.vertex_shader = Some(sid), // VERTEX_SHADER
                        Some(0x8B30) => prog.fragment_shader = Some(sid), // FRAGMENT_SHADER
                        _ => {}
                    }
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("linkProgram".into(), native("gl.linkProgram", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                let mut s = st.borrow_mut();
                // Pre-fetch shader WGSL pred mut borrow programs
                let (vs_wgsl, fs_wgsl, error_log, layout, layout_size, sampler_cnt, texture_cnt,
                     uniform_b, texture_b, sampler_b) = {
                    let prog = match s.programs.get(&id) { Some(p) => p, None => return Ok(JsValue::Undefined) };
                    let vs_id = prog.vertex_shader;
                    let fs_id = prog.fragment_shader;
                    let mut log = String::new();
                    let mut vw: Option<String> = None;
                    let mut fw: Option<String> = None;
                    let mut combined_layout: Vec<UniformSlot> = Vec::new();
                    let mut combined_size: u64 = 0;
                    let mut samplers = 0u32;
                    let mut textures = 0u32;
                    let mut uni_b: Option<u32> = None;
                    let mut tex_b: Vec<(String, u32)> = Vec::new();
                    let mut samp_b: Vec<(String, u32)> = Vec::new();
                    if let (Some(vid), Some(fid)) = (vs_id, fs_id) {
                        if let Some(vsh) = s.shaders.get(&vid) {
                            if !vsh.compiled {
                                log.push_str("vertex shader nezkompilovan\n");
                            } else if let Some(m) = &vsh.naga_module {
                                match naga_module_to_wgsl(m) {
                                    Ok(w) => vw = Some(w),
                                    Err(e) => log.push_str(&format!("vertex WGSL: {e}\n")),
                                }
                                let (slots, sz) = extract_uniform_layout(m);
                                for slot in slots {
                                    if !combined_layout.iter().any(|s: &UniformSlot| s.name == slot.name) {
                                        combined_layout.push(slot);
                                    }
                                }
                                if sz > combined_size { combined_size = sz; }
                                let (vs_samp, vs_tex) = extract_texture_sampler_counts(m);
                                samplers = samplers.max(vs_samp);
                                textures = textures.max(vs_tex);
                                let (ub, tb, sb) = extract_resource_bindings(m);
                                if uni_b.is_none() { uni_b = ub; }
                                for entry in tb { if !tex_b.iter().any(|(n, _)| n == &entry.0) { tex_b.push(entry); } }
                                for entry in sb { if !samp_b.iter().any(|(n, _)| n == &entry.0) { samp_b.push(entry); } }
                            }
                        }
                        if let Some(fsh) = s.shaders.get(&fid) {
                            if !fsh.compiled {
                                log.push_str("fragment shader nezkompilovan\n");
                            } else if let Some(m) = &fsh.naga_module {
                                match naga_module_to_wgsl(m) {
                                    Ok(w) => fw = Some(w),
                                    Err(e) => log.push_str(&format!("fragment WGSL: {e}\n")),
                                }
                                let (slots, sz) = extract_uniform_layout(m);
                                for slot in slots {
                                    if !combined_layout.iter().any(|s: &UniformSlot| s.name == slot.name) {
                                        combined_layout.push(slot);
                                    }
                                }
                                if sz > combined_size { combined_size = sz; }
                                let (fs_samp, fs_tex) = extract_texture_sampler_counts(m);
                                samplers = samplers.max(fs_samp);
                                textures = textures.max(fs_tex);
                                let (ub, tb, sb) = extract_resource_bindings(m);
                                if uni_b.is_none() { uni_b = ub; }
                                for entry in tb { if !tex_b.iter().any(|(n, _)| n == &entry.0) { tex_b.push(entry); } }
                                for entry in sb { if !samp_b.iter().any(|(n, _)| n == &entry.0) { samp_b.push(entry); } }
                            }
                        }
                    } else {
                        log.push_str("program postrada vertex nebo fragment shader\n");
                    }
                    (vw, fw, log, combined_layout, combined_size, samplers, textures, uni_b, tex_b, samp_b)
                };
                // Apply
                if let Some(prog) = s.programs.get_mut(&id) {
                    prog.linked = vs_wgsl.is_some() && fs_wgsl.is_some() && error_log.is_empty();
                    prog.vertex_wgsl = vs_wgsl;
                    prog.fragment_wgsl = fs_wgsl;
                    prog.info_log = error_log;
                    prog.uniform_layout = layout;
                    prog.uniform_buffer_size = layout_size;
                    prog.sampler_count = sampler_cnt;
                    prog.texture_count = texture_cnt;
                    prog.uniform_binding = uniform_b;
                    prog.texture_bindings = texture_b;
                    prog.sampler_bindings = sampler_b;
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("useProgram".into(), native("gl.useProgram", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            st.borrow_mut().current_program = id;
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getProgramParameter".into(), native("gl.getProgramParameter", move |args| {
            let mut it = args.into_iter();
            let id = it.next().and_then(|v| webgl_id_from(&v));
            let pname = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            if pname == 0x8B82 { // LINK_STATUS
                if let Some(id) = id {
                    if let Some(prog) = st.borrow().programs.get(&id) {
                        return Ok(JsValue::Bool(prog.linked));
                    }
                }
            }
            Ok(JsValue::Bool(false))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getProgramInfoLog".into(), native("gl.getProgramInfoLog", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(JsValue::Str(prog.info_log.clone()));
                }
            }
            Ok(JsValue::Str(String::new()))
        }));
    }
    // Diagnostic: vrati WGSL transpilaci po link (pro testy + debug)
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__program_vertex_wgsl__".into(), native("gl.__program_vertex_wgsl__", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(prog.vertex_wgsl.clone().map(JsValue::Str).unwrap_or(JsValue::Null));
                }
            }
            Ok(JsValue::Null)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__program_fragment_wgsl__".into(), native("gl.__program_fragment_wgsl__", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(prog.fragment_wgsl.clone().map(JsValue::Str).unwrap_or(JsValue::Null));
                }
            }
            Ok(JsValue::Null)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__program_uniform_count__".into(), native("gl.__program_uniform_count__", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(JsValue::Number(prog.uniform_layout.len() as f64));
                }
            }
            Ok(JsValue::Number(0.0))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__program_uniform_buffer_size__".into(), native("gl.__program_uniform_buffer_size__", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(JsValue::Number(prog.uniform_buffer_size as f64));
                }
            }
            Ok(JsValue::Number(0.0))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__program_sampler_count__".into(), native("gl.__program_sampler_count__", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(JsValue::Number(prog.sampler_count as f64));
                }
            }
            Ok(JsValue::Number(0.0))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__program_texture_count__".into(), native("gl.__program_texture_count__", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id {
                if let Some(prog) = st.borrow().programs.get(&id) {
                    return Ok(JsValue::Number(prog.texture_count as f64));
                }
            }
            Ok(JsValue::Number(0.0))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("deleteProgram".into(), native("gl.deleteProgram", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id { st.borrow_mut().programs.remove(&id); }
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Buffer management ───────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("createBuffer".into(), native("gl.createBuffer", move |_| {
            let mut s = st.borrow_mut();
            let id = s.alloc_id();
            s.buffers.insert(id, Vec::new());
            Ok(make_webgl_handle(id, "buffer"))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("bindBuffer".into(), native("gl.bindBuffer", move |args| {
            let mut it = args.into_iter();
            let target = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let id = it.next().and_then(|v| webgl_id_from(&v));
            let mut s = st.borrow_mut();
            match target {
                0x8892 => s.bound_array_buffer = id, // ARRAY_BUFFER
                0x8893 => s.bound_element_buffer = id, // ELEMENT_ARRAY_BUFFER
                _ => {}
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("bufferData".into(), native("gl.bufferData", move |args| {
            let mut it = args.into_iter();
            let target = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let data = it.next().unwrap_or(JsValue::Undefined);
            let mut s = st.borrow_mut();
            let bound_id = match target {
                0x8892 => s.bound_array_buffer,
                0x8893 => s.bound_element_buffer,
                _ => None,
            };
            if let Some(id) = bound_id {
                // data muze byt: number (size), Array (typed array values), nebo cele Float32Array
                let bytes: Vec<u8> = match &data {
                    JsValue::Array(arr) => {
                        let arr = arr.borrow();
                        let mut buf = Vec::with_capacity(arr.len() * 4);
                        for v in arr.iter() {
                            let f = v.to_number() as f32;
                            buf.extend_from_slice(&f.to_le_bytes());
                        }
                        buf
                    }
                    JsValue::Number(n) => vec![0u8; *n as usize],
                    _ => Vec::new(),
                };
                if let Some(b) = s.buffers.get_mut(&id) { *b = bytes; }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("deleteBuffer".into(), native("gl.deleteBuffer", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id { st.borrow_mut().buffers.remove(&id); }
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Texture management ──────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("createTexture".into(), native("gl.createTexture", move |_| {
            let mut s = st.borrow_mut();
            let id = s.alloc_id();
            s.textures.insert(id, WebGLTexture { width: 0, height: 0, format: 0x1908, data: Vec::new() });
            Ok(make_webgl_handle(id, "texture"))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("bindTexture".into(), native("gl.bindTexture", move |args| {
            let mut it = args.into_iter();
            let target = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let id = it.next().and_then(|v| webgl_id_from(&v));
            if target == 0x0DE1 { // TEXTURE_2D
                let mut s = st.borrow_mut();
                s.bound_texture_2d = id;
                // Take aktualni active unit -> texture_units mapping
                let unit = s.active_texture_unit;
                if let Some(tex_id) = id {
                    s.texture_units.insert(unit, tex_id);
                } else {
                    s.texture_units.remove(&unit);
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("activeTexture".into(), native("gl.activeTexture", move |args| {
            let target = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            // GL_TEXTURE0 = 0x84C0, TEXTUREn = 0x84C0 + n
            if target >= 0x84C0 {
                let unit = target - 0x84C0;
                st.borrow_mut().active_texture_unit = unit;
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        // texImage2D - dva overloady (8-arg s width/height/data + 6-arg s ImageElement).
        // Phase 3c8: ukladame width, height, format, data do bound texture v WebGLState.
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("texImage2D".into(), native("gl.texImage2D", move |args| {
            // Args (8-arg variant): target, level, internalformat, width, height, border, format, type, pixels
            // Args (6-arg variant): target, level, internalformat, format, type, source(ImageData|HTMLImageElement)
            let n = args.len();
            let (width, height, format, data) = if n >= 9 {
                let mut it = args.into_iter();
                let _target = it.next();
                let _level = it.next();
                let _internalformat = it.next();
                let w = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
                let h = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
                let _border = it.next();
                let fmt = it.next().map(|v| v.to_number()).unwrap_or(6408.0) as u32;
                let _ty = it.next();
                let pixels = it.next().unwrap_or(JsValue::Null);
                let bytes: Vec<u8> = match pixels {
                    JsValue::Array(arr) => arr.borrow().iter()
                        .map(|v| (v.to_number() as i64).clamp(0, 255) as u8)
                        .collect(),
                    _ => Vec::new(),
                };
                (w, h, fmt, bytes)
            } else {
                (0u32, 0u32, 0x1908u32, Vec::new())
            };
            let mut s = st.borrow_mut();
            if let Some(tex_id) = s.bound_texture_2d {
                if let Some(tex) = s.textures.get_mut(&tex_id) {
                    if width > 0 && height > 0 {
                        tex.width = width;
                        tex.height = height;
                        tex.format = format;
                        tex.data = data;
                    }
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    obj_rc.borrow_mut().set("texParameteri".into(), native("gl.texParameteri", |_| Ok(JsValue::Undefined)));
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("deleteTexture".into(), native("gl.deleteTexture", move |args| {
            let id = args.into_iter().next().and_then(|v| webgl_id_from(&v));
            if let Some(id) = id { st.borrow_mut().textures.remove(&id); }
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Locations + uniforms ────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getAttribLocation".into(), native("gl.getAttribLocation", move |args| {
            let mut it = args.into_iter();
            let _prog = it.next();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            // Stable ID podle name hash (positive, ne -1)
            let id = name.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32)) % 16;
            st.borrow_mut().attrib_locations.insert(id, name);
            Ok(JsValue::Number(id as f64))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getUniformLocation".into(), native("gl.getUniformLocation", move |args| {
            let mut it = args.into_iter();
            let _prog = it.next();
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let id = name.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32)) & 0xFFFF;
            st.borrow_mut().uniform_locations.insert(id, name.clone());
            let obj = Rc::new(RefCell::new(JsObject::new()));
            obj.borrow_mut().set("__webgl_uniform_id__".into(), JsValue::Number(id as f64));
            obj.borrow_mut().set("__webgl_uniform_name__".into(), JsValue::Str(name));
            Ok(JsValue::Object(obj))
        }));
    }

    // ─── Vertex attribs ──────────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("vertexAttribPointer".into(), native("gl.vertexAttribPointer", move |args| {
            let mut it = args.into_iter();
            let index = it.next().map(|v| v.to_number()).unwrap_or(0.0) as usize;
            let size = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let component_type = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let normalized = it.next().map(|v| matches!(v, JsValue::Bool(true))).unwrap_or(false);
            let stride = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let mut s = st.borrow_mut();
            let buf_id = s.bound_array_buffer.unwrap_or(0);
            if index < s.vertex_attribs.len() {
                let prev_enabled = s.vertex_attribs[index].as_ref().map(|a| a.enabled).unwrap_or(false);
                s.vertex_attribs[index] = Some(WebGLAttribSlot {
                    buffer_id: buf_id, size, component_type, normalized, stride, offset,
                    enabled: prev_enabled,
                });
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("enableVertexAttribArray".into(), native("gl.enableVertexAttribArray", move |args| {
            let index = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as usize;
            let mut s = st.borrow_mut();
            if index < s.vertex_attribs.len() {
                if let Some(slot) = s.vertex_attribs[index].as_mut() {
                    slot.enabled = true;
                } else {
                    // Vytvori placeholder slot
                    s.vertex_attribs[index] = Some(WebGLAttribSlot {
                        buffer_id: 0, size: 0, component_type: 0, normalized: false,
                        stride: 0, offset: 0, enabled: true,
                    });
                }
            }
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("disableVertexAttribArray".into(), native("gl.disableVertexAttribArray", move |args| {
            let index = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as usize;
            let mut s = st.borrow_mut();
            if index < s.vertex_attribs.len() {
                if let Some(slot) = s.vertex_attribs[index].as_mut() {
                    slot.enabled = false;
                }
            }
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Uniforms - real recording ───────────────────────────────────
    let make_uniform_setter = |kind: &'static str, st: Rc<RefCell<WebGLState>>| {
        native(&format!("gl.{kind}"), move |args| {
            let mut it = args.into_iter();
            let loc = it.next();
            let name = match &loc {
                Some(JsValue::Object(o)) => o.borrow().props.get("__webgl_uniform_name__")
                    .map(|v| v.to_string()).unwrap_or_default(),
                _ => return Ok(JsValue::Undefined),
            };
            let value: WebGLUniformValue = match kind {
                "uniform1f" | "uniform2f" | "uniform3f" | "uniform4f" => {
                    let nums: Vec<f32> = it.map(|v| v.to_number() as f32).collect();
                    WebGLUniformValue::Float(nums)
                }
                "uniform1fv" | "uniform2fv" | "uniform3fv" | "uniform4fv" => {
                    let arg = it.next().unwrap_or(JsValue::Undefined);
                    let nums: Vec<f32> = if let JsValue::Array(a) = arg {
                        a.borrow().iter().map(|v| v.to_number() as f32).collect()
                    } else { vec![arg.to_number() as f32] };
                    WebGLUniformValue::Float(nums)
                }
                "uniform1i" | "uniform2i" | "uniform3i" | "uniform4i" => {
                    let nums: Vec<i32> = it.map(|v| v.to_number() as i32).collect();
                    WebGLUniformValue::Int(nums)
                }
                "uniformMatrix2fv" | "uniformMatrix3fv" | "uniformMatrix4fv" => {
                    let _transpose = it.next();  // ignore
                    let arg = it.next().unwrap_or(JsValue::Undefined);
                    let nums: Vec<f32> = if let JsValue::Array(a) = arg {
                        a.borrow().iter().map(|v| v.to_number() as f32).collect()
                    } else { Vec::new() };
                    WebGLUniformValue::Mat(nums)
                }
                _ => WebGLUniformValue::Float(Vec::new()),
            };
            st.borrow_mut().uniforms.insert(name, value);
            Ok(JsValue::Undefined)
        })
    };
    for m in &["uniform1f", "uniform2f", "uniform3f", "uniform4f",
               "uniform1i", "uniform2i", "uniform3i", "uniform4i",
               "uniform1fv", "uniform2fv", "uniform3fv", "uniform4fv",
               "uniformMatrix2fv", "uniformMatrix3fv", "uniformMatrix4fv"] {
        let name = m.to_string();
        obj_rc.borrow_mut().set(name.clone(), make_uniform_setter(m, Rc::clone(&state)));
    }

    // ─── No-op-zatim metody ──────────────────────────────────────────
    for m in &["blendFunc", "blendFuncSeparate", "depthFunc", "cullFace", "frontFace",
               "pixelStorei", "flush", "finish",
               "scissor", "stencilFunc", "stencilOp", "stencilMask",
               "lineWidth", "polygonOffset", "depthMask", "colorMask"] {
        let name = m.to_string();
        obj_rc.borrow_mut().set(name.clone(), native(&name, |_| Ok(JsValue::Undefined)));
    }

    // ─── Draw calls - real recording ────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("drawArrays".into(), native("gl.drawArrays", move |args| {
            let mut it = args.into_iter();
            let mode = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let first = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let count = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let mut s = st.borrow_mut();
            s.draw_call_count += 1;
            // Snapshot enabled attribs
            let attribs: Vec<(u32, WebGLAttribSlot)> = s.vertex_attribs.iter().enumerate()
                .filter_map(|(i, opt)| opt.as_ref().filter(|a| a.enabled).map(|a| (i as u32, a.clone())))
                .collect();
            let cmd = WebGLDrawCmd::DrawArrays {
                program_id: s.current_program,
                mode, first, count,
                attribs,
                uniforms: s.uniforms.clone(),
                viewport: s.viewport_xywh,
            };
            s.draw_queue.push(cmd);
            Ok(JsValue::Undefined)
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("drawElements".into(), native("gl.drawElements", move |args| {
            let mut it = args.into_iter();
            let mode = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let count = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let index_type = it.next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let offset = it.next().map(|v| v.to_number()).unwrap_or(0.0) as i32;
            let mut s = st.borrow_mut();
            s.draw_call_count += 1;
            let attribs: Vec<(u32, WebGLAttribSlot)> = s.vertex_attribs.iter().enumerate()
                .filter_map(|(i, opt)| opt.as_ref().filter(|a| a.enabled).map(|a| (i as u32, a.clone())))
                .collect();
            let cmd = WebGLDrawCmd::DrawElements {
                program_id: s.current_program,
                mode, count, index_type, offset,
                index_buffer_id: s.bound_element_buffer,
                attribs,
                uniforms: s.uniforms.clone(),
                viewport: s.viewport_xywh,
            };
            s.draw_queue.push(cmd);
            Ok(JsValue::Undefined)
        }));
    }

    // ─── Diagnostic ──────────────────────────────────────────────────
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("getError".into(), native("gl.getError", move |_| {
            let err = st.borrow().last_error;
            st.borrow_mut().last_error = 0;
            Ok(JsValue::Number(err as f64))
        }));
    }
    obj_rc.borrow_mut().set("getParameter".into(), native("gl.getParameter", |args| {
        let pname = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
        // Klicove parametry pro lib detection
        match pname {
            0x1F00 => Ok(JsValue::Str("Mozilla".into())),  // VENDOR
            0x1F01 => Ok(JsValue::Str("RustWebEngine WebGL".into())),  // RENDERER
            0x1F02 => Ok(JsValue::Str("WebGL 1.0 (RustWebEngine)".into())),  // VERSION
            0x8B8C => Ok(JsValue::Str("WebGL GLSL ES 1.0".into())),  // SHADING_LANGUAGE_VERSION
            _ => Ok(JsValue::Number(0.0)),
        }
    }));
    obj_rc.borrow_mut().set("getSupportedExtensions".into(), native("gl.getSupportedExtensions", |_| {
        Ok(JsValue::Array(Rc::new(RefCell::new(Vec::new()))))
    }));
    obj_rc.borrow_mut().set("getExtension".into(), native("gl.getExtension", |_| Ok(JsValue::Null)));
    obj_rc.borrow_mut().set("isContextLost".into(), native("gl.isContextLost", |_| Ok(JsValue::Bool(false))));

    // Diagnostic accessor pro tests - drawCallCount
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__draw_call_count__".into(), native("gl.__draw_call_count__", move |_| {
            Ok(JsValue::Number(st.borrow().draw_call_count as f64))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__draw_queue_size__".into(), native("gl.__draw_queue_size__", move |_| {
            Ok(JsValue::Number(st.borrow().draw_queue.len() as f64))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__active_texture_unit__".into(), native("gl.__active_texture_unit__", move |_| {
            Ok(JsValue::Number(st.borrow().active_texture_unit as f64))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__texture_unit_binding__".into(), native("gl.__texture_unit_binding__", move |args| {
            let unit = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as u32;
            let s = st.borrow();
            match s.texture_units.get(&unit) {
                Some(tex_id) => Ok(JsValue::Number(*tex_id as f64)),
                None => Ok(JsValue::Null),
            }
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__attrib_enabled__".into(), native("gl.__attrib_enabled__", move |args| {
            let idx = args.into_iter().next().map(|v| v.to_number()).unwrap_or(0.0) as usize;
            let s = st.borrow();
            let enabled = s.vertex_attribs.get(idx).and_then(|a| a.as_ref()).map(|a| a.enabled).unwrap_or(false);
            Ok(JsValue::Bool(enabled))
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__uniform_value__".into(), native("gl.__uniform_value__", move |args| {
            let name = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
            let s = st.borrow();
            match s.uniforms.get(&name) {
                Some(WebGLUniformValue::Float(v)) => {
                    let arr: Vec<JsValue> = v.iter().map(|x| JsValue::Number(*x as f64)).collect();
                    Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
                }
                Some(WebGLUniformValue::Int(v)) => {
                    let arr: Vec<JsValue> = v.iter().map(|x| JsValue::Number(*x as f64)).collect();
                    Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
                }
                Some(WebGLUniformValue::Mat(v)) => {
                    let arr: Vec<JsValue> = v.iter().map(|x| JsValue::Number(*x as f64)).collect();
                    Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
                }
                None => Ok(JsValue::Null),
            }
        }));
    }
    {
        let st = Rc::clone(&state);
        obj_rc.borrow_mut().set("__clear_color__".into(), native("gl.__clear_color__", move |_| {
            let cc = st.borrow().clear_color;
            Ok(JsValue::Array(Rc::new(RefCell::new(vec![
                JsValue::Number(cc[0] as f64),
                JsValue::Number(cc[1] as f64),
                JsValue::Number(cc[2] as f64),
                JsValue::Number(cc[3] as f64),
            ]))))
        }));
    }

    JsValue::Object(obj_rc)
}
/// WebGL context stub - vsechny methods no-op.
/// WebGL state objekt - sdileny pres Rc<RefCell<>> mezi metodami.
/// Phase 1: handle counters + state tracking. Phase 2: shader compile.
/// Phase 3: real draw call emission.
pub struct WebGLState {
    pub next_id: u32,
    /// Shaders: id -> (type, source, compiled_ok)
    pub shaders: std::collections::HashMap<u32, WebGLShader>,
    /// Programs: id -> (vertex_shader_id, fragment_shader_id, linked)
    pub programs: std::collections::HashMap<u32, WebGLProgram>,
    /// Buffers: id -> raw bytes
    pub buffers: std::collections::HashMap<u32, Vec<u8>>,
    /// Textures: id -> (width, height, rgba_bytes)
    pub textures: std::collections::HashMap<u32, WebGLTexture>,
    /// Currently bound state (per WebGL spec)
    pub current_program: Option<u32>,
    pub bound_array_buffer: Option<u32>,
    pub bound_element_buffer: Option<u32>,
    pub bound_texture_2d: Option<u32>,
    pub clear_color: [f32; 4],
    pub viewport_xywh: [i32; 4],
    pub blend_enabled: bool,
    pub depth_test_enabled: bool,
    /// Aktualni active texture unit index (z gl.activeTexture(TEXTUREN)).
    pub active_texture_unit: u32,
    /// Mapping unit_idx -> texture_id (z gl.bindTexture pri current unit).
    pub texture_units: std::collections::HashMap<u32, u32>,
    /// Draw call count (pro testovani + diagnostiku)
    pub draw_call_count: u32,
    pub last_error: u32,
    /// Vertex attribute slots - index -> slot. Sparse Vec, pevny size 16
    /// (typical max attributes pro WebGL 1.0 = 8, WebGL 2.0 = 16).
    pub vertex_attribs: Vec<Option<WebGLAttribSlot>>,
    /// Uniformy nastavene pro current program (key = uniform name).
    pub uniforms: std::collections::HashMap<String, WebGLUniformValue>,
    /// Map z uniform location ID na uniform name (z getUniformLocation).
    pub uniform_locations: std::collections::HashMap<u32, String>,
    /// Map z attrib location ID na attrib name (pro debug).
    pub attrib_locations: std::collections::HashMap<u32, String>,
    /// Recorded draw commands queue (phase 3b: renderer drain + real wgpu emit).
    pub draw_queue: Vec<WebGLDrawCmd>,
    /// Sticky clear state - once JS volal clear(COLOR_BUFFER_BIT), canvas
    /// drzi posledni clear barvu pri kazdem repaint dokud nedo dalsi clear/draw.
    /// Bez tohoto pri prvnim paint canvas modry, dalsi paint queue prazdny -> blank.
    pub sticky_cleared: bool,
    pub sticky_clear_color: [f32; 4],
}

pub struct WebGLShader {
    pub shader_type: u32,
    pub source: String,
    pub compiled: bool,
    /// Compile log (chyby pri parse / fail reasons).
    pub info_log: String,
    /// Naga IR module - cache po uspesnem GLSL parse. Phase 3 pouzije.
    pub naga_module: Option<naga::Module>,
}

pub struct WebGLProgram {
    pub vertex_shader: Option<u32>,
    pub fragment_shader: Option<u32>,
    pub linked: bool,
    /// Link log.
    pub info_log: String,
    /// Vertex + fragment WGSL transpilace (link-time output).
    pub vertex_wgsl: Option<String>,
    pub fragment_wgsl: Option<String>,
    /// Uniform layout - jak serializovat uniformy do GPU buffer.
    /// Name -> (offset_bytes, size_bytes, ty).
    pub uniform_layout: Vec<UniformSlot>,
    /// Total uniform buffer size (zaokrouhleno na 16-byte multiple pro WGSL).
    pub uniform_buffer_size: u64,
    /// Pocet sampler2D / texture2D bindings v shaderech.
    pub sampler_count: u32,
    /// Pocet texture image bindings (separate od samplers).
    pub texture_count: u32,
    /// Resource binding indexy z naga IR. (name, binding_idx) pro kazdy.
    pub uniform_binding: Option<u32>,
    pub texture_bindings: Vec<(String, u32)>,
    pub sampler_bindings: Vec<(String, u32)>,
}

/// Layout slot pro 1 uniform v GPU buffer.
#[derive(Clone, Debug)]
pub struct UniformSlot {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub kind: UniformSlotKind,
}

/// Typ uniformu pro serializaci. Match naga TypeInner.
#[derive(Clone, Copy, Debug)]
pub enum UniformSlotKind {
    Float,        // f32
    Vec2,         // vec2<f32>
    Vec3,         // vec3<f32>
    Vec4,         // vec4<f32>
    Int,          // i32
    Mat2,         // mat2x2
    Mat3,         // mat3x3
    Mat4,         // mat4x4
    Other,        // unknown - skip serialize
}

/// Extract uniform layout z naga Module - prochazi global variables s
/// Uniform address space, mapuje na UniformSlot list. Vraci take total
/// size buffer (16-byte align).
pub(crate) fn extract_uniform_layout(module: &naga::Module) -> (Vec<UniformSlot>, u64) {
    use naga::{AddressSpace, TypeInner, ScalarKind};
    let mut slots: Vec<UniformSlot> = Vec::new();
    let mut max_end: u32 = 0;

    let kind_of = |inner: &TypeInner| -> (UniformSlotKind, u32) {
        match inner {
            TypeInner::Scalar(s) => match s.kind {
                ScalarKind::Float => (UniformSlotKind::Float, 4),
                ScalarKind::Sint | ScalarKind::Uint => (UniformSlotKind::Int, 4),
                _ => (UniformSlotKind::Other, 4),
            },
            TypeInner::Vector { size, scalar } if matches!(scalar.kind, ScalarKind::Float) => {
                match size {
                    naga::VectorSize::Bi => (UniformSlotKind::Vec2, 8),
                    naga::VectorSize::Tri => (UniformSlotKind::Vec3, 16), // padded
                    naga::VectorSize::Quad => (UniformSlotKind::Vec4, 16),
                }
            }
            TypeInner::Matrix { columns, rows, .. } => {
                let bytes = match (columns, rows) {
                    (naga::VectorSize::Bi, naga::VectorSize::Bi) => 16,
                    (naga::VectorSize::Tri, naga::VectorSize::Tri) => 48,
                    (naga::VectorSize::Quad, naga::VectorSize::Quad) => 64,
                    _ => 16,
                };
                let kind = match (columns, rows) {
                    (naga::VectorSize::Bi, naga::VectorSize::Bi) => UniformSlotKind::Mat2,
                    (naga::VectorSize::Tri, naga::VectorSize::Tri) => UniformSlotKind::Mat3,
                    (naga::VectorSize::Quad, naga::VectorSize::Quad) => UniformSlotKind::Mat4,
                    _ => UniformSlotKind::Other,
                };
                (kind, bytes)
            }
            _ => (UniformSlotKind::Other, 16),
        }
    };

    for (_, gv) in module.global_variables.iter() {
        if !matches!(gv.space, AddressSpace::Uniform) { continue; }
        let ty = &module.types[gv.ty];
        match &ty.inner {
            TypeInner::Struct { members, span } => {
                for m in members {
                    let name = m.name.clone().unwrap_or_default();
                    if name.is_empty() { continue; }
                    let mty = &module.types[m.ty];
                    let (kind, size) = kind_of(&mty.inner);
                    slots.push(UniformSlot {
                        name,
                        offset: m.offset,
                        size,
                        kind,
                    });
                    let end = m.offset + size;
                    if end > max_end { max_end = end; }
                }
                if *span > max_end { max_end = *span; }
            }
            other => {
                // Top-level uniform (rare po naga wrap)
                let name = gv.name.clone().unwrap_or_default();
                if !name.is_empty() {
                    let (kind, size) = kind_of(other);
                    slots.push(UniformSlot {
                        name, offset: 0, size, kind,
                    });
                    if size > max_end { max_end = size; }
                }
            }
        }
    }

    // Round up na 16 bytes (WGSL std140-like requirement). Pri zadnych slotech vraci 0.
    if slots.is_empty() {
        (slots, 0)
    } else {
        let total = ((max_end as u64 + 15) / 16) * 16;
        (slots, total.max(16))
    }
}

/// Spocita pocet sampler + texture bindings v naga Module.
/// Vraci (sampler_count, texture_count). WebGL sampler2D = 1 sampler + 1 texture.
pub(crate) fn extract_texture_sampler_counts(module: &naga::Module) -> (u32, u32) {
    use naga::TypeInner;
    let mut samplers = 0u32;
    let mut textures = 0u32;
    for (_, gv) in module.global_variables.iter() {
        let ty = &module.types[gv.ty];
        match &ty.inner {
            TypeInner::Sampler { .. } => samplers += 1,
            TypeInner::Image { .. } => textures += 1,
            _ => {}
        }
    }
    (samplers, textures)
}

/// Vyextrahuje konkretni binding indexy + nazve pro uniform/texture/sampler
/// resources. Vraci (uniform_binding, texture_bindings, sampler_bindings).
/// Filtruje group=0 (jen jedna group v WebGL).
pub(crate) fn extract_resource_bindings(module: &naga::Module) -> (
    Option<u32>,
    Vec<(String, u32)>,
    Vec<(String, u32)>,
) {
    use naga::{TypeInner, AddressSpace};
    let mut uniform_binding: Option<u32> = None;
    let mut textures: Vec<(String, u32)> = Vec::new();
    let mut samplers: Vec<(String, u32)> = Vec::new();
    for (_, gv) in module.global_variables.iter() {
        let ty = &module.types[gv.ty];
        let binding = match &gv.binding {
            Some(b) if b.group == 0 => b.binding,
            _ => continue,
        };
        let name = gv.name.clone().unwrap_or_default();
        match &ty.inner {
            TypeInner::Sampler { .. } => samplers.push((name, binding)),
            TypeInner::Image { .. } => textures.push((name, binding)),
            _ => {
                if matches!(gv.space, AddressSpace::Uniform) {
                    uniform_binding = Some(binding);
                }
            }
        }
    }
    (uniform_binding, textures, samplers)
}

pub struct WebGLTexture {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub data: Vec<u8>,
}

/// Vertex attribute slot - state nastaveny pres vertexAttribPointer/enableVertexAttribArray.
#[derive(Clone, Debug)]
pub struct WebGLAttribSlot {
    pub buffer_id: u32,
    pub size: i32,         // 1..4 components
    pub component_type: u32,  // FLOAT/UNSIGNED_BYTE/...
    pub normalized: bool,
    pub stride: i32,
    pub offset: i32,
    pub enabled: bool,
}

/// Uniform value (float/int/matrix variants).
#[derive(Clone, Debug)]
pub enum WebGLUniformValue {
    Float(Vec<f32>),    // 1f, 2f, 3f, 4f, 1fv, 2fv, 3fv, 4fv
    Int(Vec<i32>),      // 1i, 2i, 3i, 4i, 1iv, ...
    Mat(Vec<f32>),      // matrix2/3/4 fv
}

/// Recorded draw command - queue pro renderer (phase 3b real emission).
#[derive(Clone, Debug)]
pub enum WebGLDrawCmd {
    ClearColor([f32; 4]),
    Clear(u32),  // mask
    DrawArrays {
        program_id: Option<u32>,
        mode: u32,
        first: i32,
        count: i32,
        attribs: Vec<(u32, WebGLAttribSlot)>,  // (location, slot)
        uniforms: std::collections::HashMap<String, WebGLUniformValue>,
        viewport: [i32; 4],
    },
    DrawElements {
        program_id: Option<u32>,
        mode: u32,
        count: i32,
        index_type: u32,
        offset: i32,
        index_buffer_id: Option<u32>,
        attribs: Vec<(u32, WebGLAttribSlot)>,
        uniforms: std::collections::HashMap<String, WebGLUniformValue>,
        viewport: [i32; 4],
    },
}

impl WebGLState {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            shaders: std::collections::HashMap::new(),
            programs: std::collections::HashMap::new(),
            buffers: std::collections::HashMap::new(),
            textures: std::collections::HashMap::new(),
            current_program: None,
            bound_array_buffer: None,
            bound_element_buffer: None,
            bound_texture_2d: None,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            viewport_xywh: [0, 0, 300, 150],
            blend_enabled: false,
            depth_test_enabled: false,
            active_texture_unit: 0,
            texture_units: std::collections::HashMap::new(),
            draw_call_count: 0,
            last_error: 0,
            vertex_attribs: vec![None; 16],
            uniforms: std::collections::HashMap::new(),
            uniform_locations: std::collections::HashMap::new(),
            attrib_locations: std::collections::HashMap::new(),
            draw_queue: Vec::new(),
            sticky_cleared: false,
            sticky_clear_color: [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}
