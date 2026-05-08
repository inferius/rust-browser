//! Performance panel: ring-buffer frame timings + counters.

const FRAME_HISTORY: usize = 240;

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameSample {
    pub frame_index: u64,
    /// Total frame time = build display list + GPU submit + present (ms).
    pub total_ms: f32,
    pub layout_ms: f32,
    pub paint_build_ms: f32,
    pub gpu_submit_ms: f32,
    pub display_list_size: u32,
    pub vertex_count: u32,
}

#[derive(Debug, Clone)]
pub struct PerformanceState {
    pub samples: Vec<FrameSample>,
    pub head: usize,
    pub layout_cache_hits: u64,
    pub layout_cache_misses: u64,
    pub glyph_atlas_used: u32,
    pub image_atlas_used: u32,
}

impl Default for PerformanceState {
    fn default() -> Self {
        PerformanceState {
            samples: vec![FrameSample::default(); FRAME_HISTORY],
            head: 0,
            layout_cache_hits: 0,
            layout_cache_misses: 0,
            glyph_atlas_used: 0,
            image_atlas_used: 0,
        }
    }
}

impl PerformanceState {
    pub fn push(&mut self, s: FrameSample) {
        self.samples[self.head] = s;
        self.head = (self.head + 1) % FRAME_HISTORY;
    }

    /// Vraci samples v chronologickem poradi (oldest -> newest).
    pub fn ordered(&self) -> Vec<FrameSample> {
        let mut out = Vec::with_capacity(FRAME_HISTORY);
        for i in 0..FRAME_HISTORY {
            let idx = (self.head + i) % FRAME_HISTORY;
            out.push(self.samples[idx]);
        }
        out
    }

    pub fn avg_total_ms(&self) -> f32 {
        let valid: Vec<f32> = self.samples.iter().filter_map(|s| if s.total_ms > 0.0 { Some(s.total_ms) } else { None }).collect();
        if valid.is_empty() { return 0.0; }
        valid.iter().sum::<f32>() / valid.len() as f32
    }
}
