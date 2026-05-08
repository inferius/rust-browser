//! Dirty rect tracking pro inkrementalni render.

// ─── Dirty rect tracking ────────────────────────────────────────────────

/// Sleduje obdelnikovou oblast ktera potrebuje prekresleni.
/// `None` = vse ciste (zadna zmena). `Some([x,y,w,h])` = dirty oblast.
/// Slucovani: unionem s novou dirty oblast.
#[derive(Debug, Clone, Default)]
pub struct DirtyRegion {
    pub rect: Option<[f32; 4]>,
}

impl DirtyRegion {
    pub fn new() -> Self { DirtyRegion { rect: None } }

    /// Oznaci oblast jako dirty. Slucuje s existujici dirty oblasti (union).
    pub fn mark(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rect = Some(match self.rect {
            None => [x, y, w, h],
            Some([ox, oy, ow, oh]) => {
                let nx = ox.min(x);
                let ny = oy.min(y);
                let nw = (ox + ow).max(x + w) - nx;
                let nh = (oy + oh).max(y + h) - ny;
                [nx, ny, nw, nh]
            }
        });
    }

    /// Vymaze dirty stav. Vraci oblast ktera byla dirty (pro render).
    pub fn take(&mut self) -> Option<[f32; 4]> {
        self.rect.take()
    }

    pub fn is_dirty(&self) -> bool { self.rect.is_some() }

    /// Nastavi dirty na cele viewport.
    pub fn mark_all(&mut self, w: f32, h: f32) {
        self.mark(0.0, 0.0, w, h);
    }
}
