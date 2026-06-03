//! WebGPU JS API foundation - Adapter, Device, Buffer, Texture, Pipeline.
//!
//! Spec: https://www.w3.org/TR/webgpu/
//! Foundation pres existing wgpu integration. JS API surface = thin wrapper.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerPreference {
    LowPower,
    HighPerformance,
}

#[derive(Debug, Clone)]
pub struct GpuAdapter {
    pub id: u32,
    pub name: String,
    pub features: Vec<String>,
    pub limits: GpuLimits,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GpuLimits {
    pub max_texture_dimension_2d: u32,
    pub max_buffer_size: u64,
    pub max_compute_workgroups_per_dimension: u32,
}

impl GpuLimits {
    pub fn defaults() -> Self {
        Self {
            max_texture_dimension_2d: 8192,
            max_buffer_size: 268435456, // 256 MB
            max_compute_workgroups_per_dimension: 65535,
        }
    }
}

#[derive(Debug)]
pub struct GpuDevice {
    pub id: u32,
    pub adapter_id: u32,
    pub buffers: HashMap<u32, GpuBuffer>,
    pub textures: HashMap<u32, GpuTexture>,
    pub pipelines: HashMap<u32, GpuPipeline>,
    pub next_id: u32,
}

#[derive(Debug)]
pub struct GpuBuffer {
    pub id: u32,
    pub size: u64,
    pub usage: u32,
    pub mapped: bool,
}

#[derive(Debug)]
pub struct GpuTexture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub format: String,
}

#[derive(Debug, Clone)]
pub enum GpuPipeline {
    Render(RenderPipelineDescriptor),
    Compute(ComputePipelineDescriptor),
}

#[derive(Debug, Clone)]
pub struct RenderPipelineDescriptor {
    pub vertex_shader: String,
    pub fragment_shader: String,
}

#[derive(Debug, Clone)]
pub struct ComputePipelineDescriptor {
    pub compute_shader: String,
    pub entry_point: String,
}

impl GpuDevice {
    pub fn new(id: u32, adapter_id: u32) -> Self {
        Self {
            id, adapter_id,
            buffers: HashMap::new(),
            textures: HashMap::new(),
            pipelines: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn create_buffer(&mut self, size: u64, usage: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.buffers.insert(id, GpuBuffer { id, size, usage, mapped: false });
        id
    }

    pub fn create_texture(&mut self, w: u32, h: u32, d: u32, format: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.textures.insert(id, GpuTexture {
            id, width: w, height: h, depth: d,
            format: format.into(),
        });
        id
    }

    pub fn create_render_pipeline(&mut self, vs: &str, fs: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.pipelines.insert(id, GpuPipeline::Render(RenderPipelineDescriptor {
            vertex_shader: vs.into(),
            fragment_shader: fs.into(),
        }));
        id
    }

    pub fn create_compute_pipeline(&mut self, cs: &str, entry: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.pipelines.insert(id, GpuPipeline::Compute(ComputePipelineDescriptor {
            compute_shader: cs.into(),
            entry_point: entry.into(),
        }));
        id
    }
}

#[derive(Default)]
pub struct WebGpuRegistry {
    pub adapters: HashMap<u32, GpuAdapter>,
    pub devices: HashMap<u32, GpuDevice>,
    pub next_id: u32,
}

impl WebGpuRegistry {
    pub fn new() -> Self {
        let mut r = Self::default();
        // Default adapter.
        r.next_id += 1;
        r.adapters.insert(r.next_id, GpuAdapter {
            id: r.next_id,
            name: "Default Adapter".into(),
            features: vec!["timestamp-query".into(), "texture-compression-bc".into()],
            limits: GpuLimits::defaults(),
        });
        r
    }

    pub fn request_adapter(&self, _power: PowerPreference) -> Option<&GpuAdapter> {
        self.adapters.values().next()
    }

    pub fn request_device(&mut self, adapter_id: u32) -> Option<u32> {
        if !self.adapters.contains_key(&adapter_id) { return None; }
        self.next_id += 1;
        let id = self.next_id;
        self.devices.insert(id, GpuDevice::new(id, adapter_id));
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_default_adapter() {
        let r = WebGpuRegistry::new();
        assert!(r.request_adapter(PowerPreference::HighPerformance).is_some());
    }

    #[test]
    fn request_device_creates() {
        let mut r = WebGpuRegistry::new();
        let adapter = r.request_adapter(PowerPreference::HighPerformance).unwrap().id;
        let device = r.request_device(adapter).unwrap();
        assert!(r.devices.contains_key(&device));
    }

    #[test]
    fn create_buffer_and_texture() {
        let mut d = GpuDevice::new(1, 1);
        let b = d.create_buffer(1024, 0x1);
        let t = d.create_texture(256, 256, 1, "rgba8unorm");
        assert!(d.buffers.contains_key(&b));
        assert!(d.textures.contains_key(&t));
    }

    #[test]
    fn pipeline_creation() {
        let mut d = GpuDevice::new(1, 1);
        let r = d.create_render_pipeline("vs main", "fs main");
        let c = d.create_compute_pipeline("cs main", "main");
        assert!(matches!(d.pipelines.get(&r), Some(GpuPipeline::Render(_))));
        assert!(matches!(d.pipelines.get(&c), Some(GpuPipeline::Compute(_))));
    }
}
