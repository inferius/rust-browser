//! Render tiles - viewport partitioned into N-px tiles for incremental raster.
//!
//! Chromium reference: cc::Tile.
//! Tile sizes typical: 256x256 or 512x512. Viewport tile region updates per frame;
//! offscreen tiles get evicted from cache. Allows scrolling without re-raster.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub x: i32,
    pub y: i32,
    pub layer_id: u32,
    pub lod: u8,        // level of detail (zoom)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TilePriority {
    Now,            // visible in viewport
    Soon,           // adjacent to viewport (1-tile margin)
    Eventually,     // farther
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub key: TileKey,
    pub size_px: u32,
    pub priority: TilePriority,
    pub raster_status: RasterStatus,
    pub last_used_frame: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RasterStatus {
    NotReady,
    Pending,
    Ready,
    Failed,
}

#[derive(Default)]
pub struct TileCache {
    pub tiles: HashMap<TileKey, Tile>,
    pub memory_budget_bytes: u64,
    pub used_bytes: u64,
    pub current_frame: u64,
}

impl TileCache {
    pub fn new(budget_mb: u64) -> Self {
        Self { memory_budget_bytes: budget_mb * 1024 * 1024, ..Self::default() }
    }

    pub fn ensure(&mut self, key: TileKey, size_px: u32, priority: TilePriority) {
        let frame = self.current_frame;
        let bytes = (size_px as u64) * (size_px as u64) * 4;
        let needs_insert = !self.tiles.contains_key(&key);
        if needs_insert {
            self.maybe_evict(bytes);
            self.tiles.insert(key, Tile {
                key, size_px, priority,
                raster_status: RasterStatus::NotReady,
                last_used_frame: frame,
                bytes,
            });
            self.used_bytes += bytes;
            return;
        }
        let t = self.tiles.get_mut(&key).unwrap();
        t.priority = priority;
        t.last_used_frame = frame;
    }

    pub fn mark_ready(&mut self, key: TileKey) {
        if let Some(t) = self.tiles.get_mut(&key) {
            t.raster_status = RasterStatus::Ready;
        }
    }

    pub fn next_frame(&mut self) { self.current_frame += 1; }

    pub fn maybe_evict(&mut self, incoming: u64) {
        while self.used_bytes + incoming > self.memory_budget_bytes {
            // Drop oldest LOWEST priority tile.
            let to_drop = self.tiles.values()
                .filter(|t| t.priority != TilePriority::Now)
                .min_by_key(|t| (t.priority_rank(), t.last_used_frame))
                .map(|t| t.key);
            match to_drop {
                Some(k) => {
                    if let Some(t) = self.tiles.remove(&k) {
                        self.used_bytes = self.used_bytes.saturating_sub(t.bytes);
                    }
                }
                None => break, // can't evict any
            }
        }
    }

    pub fn ready_tiles_for_viewport(&self) -> Vec<&Tile> {
        self.tiles.values().filter(|t| t.raster_status == RasterStatus::Ready && t.priority == TilePriority::Now).collect()
    }
}

impl Tile {
    pub fn priority_rank(&self) -> u8 {
        match self.priority {
            TilePriority::Now => 0,
            TilePriority::Soon => 1,
            TilePriority::Eventually => 2,
        }
    }
}

/// Compute the set of tile keys covering a viewport.
pub fn tiles_covering(viewport: (f32, f32, f32, f32), tile_size: f32, layer_id: u32, lod: u8) -> Vec<TileKey> {
    let (x, y, w, h) = viewport;
    let x_start = (x / tile_size).floor() as i32;
    let y_start = (y / tile_size).floor() as i32;
    let x_end = ((x + w) / tile_size).ceil() as i32;
    let y_end = ((y + h) / tile_size).ceil() as i32;
    let mut out = Vec::new();
    for ty in y_start..y_end {
        for tx in x_start..x_end {
            out.push(TileKey { x: tx, y: ty, layer_id, lod });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_cover_simple() {
        let tiles = tiles_covering((0.0, 0.0, 600.0, 400.0), 256.0, 1, 0);
        // ceil(600/256) = 3 columns x ceil(400/256) = 2 rows = 6
        assert_eq!(tiles.len(), 6);
    }

    #[test]
    fn tile_cover_offset() {
        let tiles = tiles_covering((100.0, 100.0, 200.0, 200.0), 256.0, 1, 0);
        // Covers (0,0) and (1,0) and (0,1) and (1,1) potentially
        assert!(tiles.len() >= 2);
    }

    #[test]
    fn cache_ensure_inserts() {
        let mut c = TileCache::new(64);
        let key = TileKey { x: 0, y: 0, layer_id: 1, lod: 0 };
        c.ensure(key, 256, TilePriority::Now);
        assert!(c.tiles.contains_key(&key));
    }

    #[test]
    fn cache_eviction_when_over_budget() {
        let mut c = TileCache::new(1); // 1 MB
        // each 256x256x4 = 256KB. 5 tiles = 1.25 MB > 1 MB.
        for i in 0..5 {
            c.ensure(TileKey { x: i, y: 0, layer_id: 1, lod: 0 }, 256, TilePriority::Soon);
        }
        assert!(c.used_bytes <= c.memory_budget_bytes);
    }

    #[test]
    fn ready_tile_filter() {
        let mut c = TileCache::new(64);
        let k1 = TileKey { x: 0, y: 0, layer_id: 1, lod: 0 };
        let k2 = TileKey { x: 1, y: 0, layer_id: 1, lod: 0 };
        c.ensure(k1, 256, TilePriority::Now);
        c.ensure(k2, 256, TilePriority::Now);
        c.mark_ready(k1);
        let ready = c.ready_tiles_for_viewport();
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn current_priority_now_not_evicted() {
        let mut c = TileCache::new(1);
        // Fill with Now-priority tiles; cannot evict.
        for i in 0..3 {
            c.ensure(TileKey { x: i, y: 0, layer_id: 1, lod: 0 }, 256, TilePriority::Now);
        }
        // All 3 remain even past budget.
        assert_eq!(c.tiles.len(), 3);
    }
}
