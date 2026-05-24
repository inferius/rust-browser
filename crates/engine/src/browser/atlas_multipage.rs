//! Multi-page glyph/image atlas - misto fixed 4096x4096 dynamic seznam pages.
//!
//! Currently atlas hardcoded 4096x4096 single texture. Pri prekroceni = fail.
//! Multi-page: allocate novou tex pri full + LRU eviction nejstarsich.
//!
//! Foundation: page allocator + slot tracking. Real wire do GlyphAtlas/ImageAtlas
//! vyzaduje refactor existing atlas.rs.
//!
//! Inspired by Chromium `gpu/command_buffer/service/gles2_cmd_decoder.cc`
//! texture pool + Firefox `gfx/wr/wr_glyph_rasterizer/`.

#[derive(Debug, Clone)]
pub struct AtlasPage {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// Posledni frame ID s use (LRU eviction).
    pub last_used_frame: u64,
    /// Free regions - shelf-pack: per-shelf height + x cursor.
    pub shelves: Vec<Shelf>,
}

#[derive(Debug, Clone)]
pub struct Shelf {
    pub y: u32,
    pub height: u32,
    pub x_cursor: u32,
}

impl AtlasPage {
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        Self {
            id, width, height,
            last_used_frame: 0,
            shelves: Vec::new(),
        }
    }

    /// Pokusi se alokovat slot (w, h). Vraci (x, y) nebo None pri no fit.
    pub fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.width || h > self.height { return None; }
        // Find shelf s vyskou >= h s mistem.
        for shelf in self.shelves.iter_mut() {
            if shelf.height >= h && self.width - shelf.x_cursor >= w {
                let pos = (shelf.x_cursor, shelf.y);
                shelf.x_cursor += w;
                return Some(pos);
            }
        }
        // Novy shelf - top y = posledni shelf y + height.
        let y = self.shelves.last().map(|s| s.y + s.height).unwrap_or(0);
        if y + h > self.height { return None; }
        let shelf = Shelf { y, height: h, x_cursor: w };
        self.shelves.push(shelf);
        Some((0, y))
    }

    pub fn touch(&mut self, frame: u64) {
        self.last_used_frame = frame;
    }
}

#[derive(Default)]
pub struct MultiPageAtlas {
    pub pages: Vec<AtlasPage>,
    pub next_page_id: u32,
    pub page_size: u32, // typicky 4096
    /// Max pages pred eviction.
    pub max_pages: u32,
    /// Current frame counter pro LRU.
    pub frame: u64,
}

impl MultiPageAtlas {
    pub fn new(page_size: u32, max_pages: u32) -> Self {
        Self {
            pages: Vec::new(),
            next_page_id: 0,
            page_size,
            max_pages,
            frame: 0,
        }
    }

    /// Allocate slot (w, h) - try existing pages pak alloc novou.
    /// Vraci (page_id, x, y).
    pub fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32, u32)> {
        for page in self.pages.iter_mut() {
            if let Some((x, y)) = page.allocate(w, h) {
                page.touch(self.frame);
                return Some((page.id, x, y));
            }
        }
        // No fit - alloc new page if room.
        if self.pages.len() as u32 >= self.max_pages {
            self.evict_lru();
        }
        let id = self.next_page_id;
        self.next_page_id += 1;
        let mut page = AtlasPage::new(id, self.page_size, self.page_size);
        let pos = page.allocate(w, h)?;
        page.touch(self.frame);
        let result = (page.id, pos.0, pos.1);
        self.pages.push(page);
        Some(result)
    }

    fn evict_lru(&mut self) {
        // Drop oldest used page.
        if let Some((idx, _)) = self.pages.iter().enumerate()
            .min_by_key(|(_, p)| p.last_used_frame)
        {
            self.pages.remove(idx);
        }
    }

    pub fn next_frame(&mut self) {
        self.frame += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_single_slot() {
        let mut a = MultiPageAtlas::new(256, 4);
        let (page, x, y) = a.allocate(32, 32).unwrap();
        assert_eq!(page, 0);
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn allocate_packs_shelf() {
        let mut a = MultiPageAtlas::new(256, 4);
        a.allocate(32, 32).unwrap();
        let (_, x, y) = a.allocate(32, 32).unwrap();
        assert_eq!((x, y), (32, 0));
    }

    #[test]
    fn allocates_new_page_when_full() {
        let mut a = MultiPageAtlas::new(64, 4);
        for _ in 0..4 { a.allocate(32, 32).unwrap(); }
        // 64x64 = 4 sloty 32x32. Dalsi alloc = novy page.
        let r = a.allocate(32, 32).unwrap();
        assert!(r.0 >= 1);
    }

    #[test]
    fn lru_eviction() {
        let mut a = MultiPageAtlas::new(64, 2);
        a.allocate(64, 64).unwrap(); // page 0
        a.next_frame();
        a.allocate(64, 64).unwrap(); // page 1
        a.next_frame();
        // page 0 ma starsi last_used - bude evicted.
        a.allocate(64, 64).unwrap(); // alloc -> evict page 0
        assert_eq!(a.pages.len(), 2);
        // page 0 mel by byt pryc.
        assert!(!a.pages.iter().any(|p| p.id == 0));
    }
}
