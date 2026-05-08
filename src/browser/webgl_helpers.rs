//! WebGL helper functions extracted z render.rs.
//!
//! Obsahuje: serializace uniformy, attrib format konverze, draw extraction,
//! count helpers.

/// Serialize uniformy z WebGLState dle program uniform layout do bytes.
/// Buffer alokovan na uniform_buffer_size, prazdne sloty zustavaji 0.
pub fn webgl_serialize_uniforms(
    layout: &[crate::interpreter::UniformSlot],
    values: &std::collections::HashMap<String, crate::interpreter::WebGLUniformValue>,
    buffer_size: u64,
) -> Vec<u8> {
    use crate::interpreter::{UniformSlotKind, WebGLUniformValue};
    let mut out = vec![0u8; buffer_size as usize];
    for slot in layout {
        let val = match values.get(&slot.name) {
            Some(v) => v,
            None => continue,
        };
        let off = slot.offset as usize;
        if off + slot.size as usize > out.len() { continue; }
        match (slot.kind, val) {
            (UniformSlotKind::Float, WebGLUniformValue::Float(v)) => {
                if let Some(&x) = v.first() {
                    out[off..off+4].copy_from_slice(&x.to_le_bytes());
                }
            }
            (UniformSlotKind::Vec2, WebGLUniformValue::Float(v)) => {
                if v.len() >= 2 {
                    out[off..off+4].copy_from_slice(&v[0].to_le_bytes());
                    out[off+4..off+8].copy_from_slice(&v[1].to_le_bytes());
                }
            }
            (UniformSlotKind::Vec3, WebGLUniformValue::Float(v)) => {
                if v.len() >= 3 {
                    out[off..off+4].copy_from_slice(&v[0].to_le_bytes());
                    out[off+4..off+8].copy_from_slice(&v[1].to_le_bytes());
                    out[off+8..off+12].copy_from_slice(&v[2].to_le_bytes());
                    // Vec3 v WGSL std140 ma 4-component padding (16 byte size).
                }
            }
            (UniformSlotKind::Vec4, WebGLUniformValue::Float(v)) => {
                for i in 0..4.min(v.len()) {
                    out[off+i*4..off+i*4+4].copy_from_slice(&v[i].to_le_bytes());
                }
            }
            (UniformSlotKind::Int, WebGLUniformValue::Int(v)) => {
                if let Some(&x) = v.first() {
                    out[off..off+4].copy_from_slice(&x.to_le_bytes());
                }
            }
            (UniformSlotKind::Mat2, WebGLUniformValue::Mat(v)) => {
                for i in 0..4.min(v.len()) {
                    out[off+i*4..off+i*4+4].copy_from_slice(&v[i].to_le_bytes());
                }
            }
            (UniformSlotKind::Mat3, WebGLUniformValue::Mat(v)) => {
                // mat3x3 v WGSL std140: 3 vec3 s padding (3 * 16 = 48 bytes)
                for col in 0..3 {
                    for row in 0..3 {
                        let src_idx = col * 3 + row;
                        if src_idx < v.len() {
                            let dst = off + col * 16 + row * 4;
                            if dst + 4 <= out.len() {
                                out[dst..dst+4].copy_from_slice(&v[src_idx].to_le_bytes());
                            }
                        }
                    }
                }
            }
            (UniformSlotKind::Mat4, WebGLUniformValue::Mat(v)) => {
                for i in 0..16.min(v.len()) {
                    out[off+i*4..off+i*4+4].copy_from_slice(&v[i].to_le_bytes());
                }
            }
            _ => {}
        }
    }
    out
}

/// WebGL component type -> velikost v bytech.
fn webgl_type_size(ctype: u32) -> u32 {
    match ctype {
        0x1400 => 1,  // BYTE
        0x1401 => 1,  // UNSIGNED_BYTE
        0x1402 => 2,  // SHORT
        0x1403 => 2,  // UNSIGNED_SHORT
        0x1404 => 4,  // INT
        0x1405 => 4,  // UNSIGNED_INT
        0x1406 => 4,  // FLOAT
        _ => 4,
    }
}

/// Mapuje WebGL (size, type) na wgpu::VertexFormat.
/// Vraci None pri nepodporovanem formatu.
pub fn webgl_attrib_to_vertex_format(size: i32, ctype: u32) -> Option<wgpu::VertexFormat> {
    use wgpu::VertexFormat as VF;
    match (size, ctype) {
        (1, 0x1406) => Some(VF::Float32),       // FLOAT
        (2, 0x1406) => Some(VF::Float32x2),
        (3, 0x1406) => Some(VF::Float32x3),
        (4, 0x1406) => Some(VF::Float32x4),
        (2, 0x1404) => Some(VF::Sint32x2),      // INT
        (4, 0x1404) => Some(VF::Sint32x4),
        (2, 0x1405) => Some(VF::Uint32x2),      // UNSIGNED_INT
        (4, 0x1405) => Some(VF::Uint32x4),
        _ => None,
    }
}

/// Snapshot z WebGLState extrahovany pro processing.
/// Pure data - nepotrebuje wgpu Device, lze testovat unit.
pub struct WebGLPendingFrame {
    pub commands: Vec<crate::interpreter::WebGLDrawCmd>,
    pub buffers: std::collections::HashMap<u32, Vec<u8>>,
    /// program_id -> (vertex_wgsl, fragment_wgsl)
    pub programs: std::collections::HashMap<u32, (Option<String>, Option<String>)>,
    pub default_clear: [f32; 4],
}

/// Drain queue + clone buffers + extract WGSL strings z linked programs.
/// Po volani je state.draw_queue prazdne. Buffers a programs zustavaji
/// nezmeneny (jen clone).
pub fn webgl_extract_pending(state: &mut crate::interpreter::WebGLState) -> WebGLPendingFrame {
    let commands: Vec<_> = state.draw_queue.drain(..).collect();
    let buffers = state.buffers.clone();
    let programs: std::collections::HashMap<u32, (Option<String>, Option<String>)> = state.programs.iter()
        .map(|(k, p)| (*k, (p.vertex_wgsl.clone(), p.fragment_wgsl.clone())))
        .collect();
    let default_clear = state.clear_color;
    WebGLPendingFrame { commands, buffers, programs, default_clear }
}

/// Vypocita efektivni clear color z command sequence.
/// Vraci Some(color) pokud queue obsahuje Clear s COLOR_BUFFER_BIT (0x4000).
/// Color = posledni ClearColor pred Clear, nebo default pri zadnym ClearColor.
/// None pokud Clear bit chybi.
pub fn webgl_effective_clear(commands: &[crate::interpreter::WebGLDrawCmd], default: [f32; 4]) -> Option<[f32; 4]> {
    use crate::interpreter::WebGLDrawCmd;
    let mut last_cc: Option<[f32; 4]> = None;
    let mut had_clear = false;
    for cmd in commands {
        match cmd {
            WebGLDrawCmd::ClearColor(c) => last_cc = Some(*c),
            WebGLDrawCmd::Clear(mask) => {
                if mask & 0x4000 != 0 { had_clear = true; }
            }
            _ => {}
        }
    }
    if had_clear { Some(last_cc.unwrap_or(default)) } else { None }
}

/// Pocet draw calls (DrawArrays + DrawElements) v command sequence.
pub fn webgl_count_draws(commands: &[crate::interpreter::WebGLDrawCmd]) -> usize {
    use crate::interpreter::WebGLDrawCmd;
    commands.iter().filter(|c| matches!(c,
        WebGLDrawCmd::DrawArrays { .. } | WebGLDrawCmd::DrawElements { .. }
    )).count()
}

/// Pocet clear calls v sequence (jen Clear, ne ClearColor).
pub fn webgl_count_clears(commands: &[crate::interpreter::WebGLDrawCmd]) -> usize {
    use crate::interpreter::WebGLDrawCmd;
    commands.iter().filter(|c| matches!(c, WebGLDrawCmd::Clear(_))).count()
}

/// Vraci IDs vsech linkovanych programu (s vertex + fragment WGSL).
pub fn webgl_linked_program_ids(state: &crate::interpreter::WebGLState) -> Vec<u32> {
    state.programs.iter()
        .filter(|(_, p)| p.linked && p.vertex_wgsl.is_some() && p.fragment_wgsl.is_some())
        .map(|(k, _)| *k)
        .collect()
}

/// True pokud layout tree obsahuje canvas tag s WebGL state pres webgl_states map.
/// Walk celym tree, vraci pri prvni hit.
pub fn webgl_layout_has_canvas(
    bx: &super::layout::LayoutBox,
    webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
) -> bool {
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if webgl_states.contains_key(&ptr) {
                return true;
            }
        }
    }
    bx.children.iter().any(|ch| webgl_layout_has_canvas(ch, webgl_states))
}

/// Spocita pocet WebGL canvases v layout tree.
pub fn webgl_canvas_count(
    bx: &super::layout::LayoutBox,
    webgl_states: &std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<crate::interpreter::WebGLState>>>,
) -> usize {
    let mut count = 0;
    if bx.tag.as_deref() == Some("canvas") {
        if let Some(node) = &bx.node {
            let ptr = std::rc::Rc::as_ptr(node) as usize;
            if webgl_states.contains_key(&ptr) {
                count += 1;
            }
        }
    }
    for ch in &bx.children {
        count += webgl_canvas_count(ch, webgl_states);
    }
    count
}

/// Spocita stride pro vertex layout pokud slot.stride == 0 (tightly packed).
pub fn webgl_compute_stride(attribs: &[(u32, crate::interpreter::WebGLAttribSlot)]) -> u64 {
    if let Some((_, slot)) = attribs.first() {
        if slot.stride > 0 {
            return slot.stride as u64;
        }
    }
    // Tightly packed: suma sizes * type_size
    attribs.iter().map(|(_, s)| {
        (s.size as u32 * webgl_type_size(s.component_type)) as u64
    }).sum()
}
