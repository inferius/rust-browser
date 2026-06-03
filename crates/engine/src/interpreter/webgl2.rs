//! WebGL2 stub - rozšírenie WebGL1 s 3D textures, instanced rendering, transform
//! feedback, uniform buffer objects, sampler objects, etc.
//!
//! Spec: https://www.khronos.org/registry/webgl/specs/latest/2.0/
//! Foundation: API surface + state tracking. Real GPU = WebGL pipeline integration.

use std::collections::HashMap;

/// WebGL2 specific state nad WebGL1.
#[derive(Debug, Default)]
pub struct WebGL2State {
    pub vertex_array_objects: HashMap<u32, VertexArrayObject>,
    pub uniform_buffers: HashMap<u32, UniformBuffer>,
    pub transform_feedback_buffers: HashMap<u32, TransformFeedback>,
    pub sampler_objects: HashMap<u32, Sampler>,
    pub texture_3d: HashMap<u32, Texture3D>,
    pub next_id: u32,
}

#[derive(Debug, Clone)]
pub struct VertexArrayObject {
    pub id: u32,
    pub attributes_bound: HashMap<u32, u32>, // attrib_index -> buffer_id
}

#[derive(Debug, Clone)]
pub struct UniformBuffer {
    pub id: u32,
    pub binding: u32,
    pub size: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TransformFeedback {
    pub id: u32,
    pub buffer_id: u32,
    pub varyings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Sampler {
    pub id: u32,
    pub mag_filter: u32,
    pub min_filter: u32,
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub wrap_r: u32,
}

#[derive(Debug, Clone)]
pub struct Texture3D {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub internal_format: u32,
}

impl WebGL2State {
    pub fn new() -> Self { Self::default() }

    pub fn create_vao(&mut self) -> u32 {
        let id = self.next();
        self.vertex_array_objects.insert(id, VertexArrayObject {
            id, attributes_bound: HashMap::new(),
        });
        id
    }

    pub fn create_uniform_buffer(&mut self, binding: u32, size: usize) -> u32 {
        let id = self.next();
        self.uniform_buffers.insert(id, UniformBuffer {
            id, binding, size, data: vec![0u8; size],
        });
        id
    }

    pub fn create_sampler(&mut self) -> u32 {
        let id = self.next();
        self.sampler_objects.insert(id, Sampler {
            id, mag_filter: 0x2601, // LINEAR
            min_filter: 0x2601,
            wrap_s: 0x2901,    // REPEAT
            wrap_t: 0x2901,
            wrap_r: 0x2901,
        });
        id
    }

    pub fn create_texture_3d(&mut self, w: u32, h: u32, d: u32, format: u32) -> u32 {
        let id = self.next();
        self.texture_3d.insert(id, Texture3D {
            id, width: w, height: h, depth: d, internal_format: format,
        });
        id
    }

    fn next(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_resources() {
        let mut s = WebGL2State::new();
        let vao = s.create_vao();
        let ubo = s.create_uniform_buffer(0, 256);
        let smp = s.create_sampler();
        let tex = s.create_texture_3d(64, 64, 16, 0x8058);
        assert!(vao > 0);
        assert_ne!(vao, ubo);
        assert_ne!(ubo, smp);
        assert_ne!(smp, tex);
    }

    #[test]
    fn ubo_zero_initialized() {
        let mut s = WebGL2State::new();
        let id = s.create_uniform_buffer(0, 64);
        let ubo = s.uniform_buffers.get(&id).unwrap();
        assert_eq!(ubo.data.len(), 64);
        assert!(ubo.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn sampler_defaults() {
        let mut s = WebGL2State::new();
        let id = s.create_sampler();
        let smp = s.sampler_objects.get(&id).unwrap();
        assert_eq!(smp.mag_filter, 0x2601); // LINEAR
    }
}
