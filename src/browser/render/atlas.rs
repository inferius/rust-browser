//! Glyph atlas + Image atlas - RGBA8 packed textures pro text + img rendering.
//!
//! GlyphAtlas: shelf-pack font glyphs, klic = (family, char, size). LCD subpixel
//!   pri size<24 (3x sirka swizzled R/G/B). Multi-font: default/bold/italic/bold-italic + extra.
//! ImageAtlas: shelf-pack RGBA images, klic = src URL. Re-raster pri zoom zmene.

/// Pokusi se najit a nacist systemovy font (None pri selhani - pro layout fallback).
pub fn try_load_default_font() -> Option<Vec<u8>> {
    if let Ok(path) = std::env::var("RUST_WEB_ENGINE_FONT_PATH") {
        if let Ok(data) = std::fs::read(&path) { return Some(data); }
    }
    // Match Chrome default UA font: pri unstyled body Chrome pouziva Times New
    // Roman (serif). Pri explicit font-family: sans-serif spadne na Arial/Segoe UI.
    // Defaultne tedy Times New Roman pokud existuje, jinak fallback Segoe UI/Arial.
    let candidates: &[&str] = &[
        "C:\\Windows\\Fonts\\times.ttf",     // Windows Times New Roman (Chrome default)
        "C:\\Windows\\Fonts\\segoeui.ttf",   // Windows fallback
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\verdana.ttf",
        "/System/Library/Fonts/Times.ttc",   // macOS Times
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

pub(super) fn load_default_font() -> Vec<u8> {
    try_load_default_font()
        .expect("Nelze najit system font. Set RUST_WEB_ENGINE_FONT_PATH na cestu k TTF souboru.")
}

// ─── Glyph atlas ────────────────────────────────────────────────────────

pub(super) const ATLAS_SIZE: u32 = 4096;

pub(super) struct GlyphInfo {
    /// UV coords v atlasu (0..1)
    pub(super) uv0: [f32; 2],
    pub(super) uv1: [f32; 2],
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) bearing_x: f32,
    pub(super) bearing_y: f32,
    pub(super) advance: f32,
    /// LCD subpixel: pri size < threshold rasterujeme pres fontdue
    /// rasterize_subpixel = 3x sirka swizzled RGB. Render shader pak sample
    /// 3 horizontal texely per fragment pro R/G/B sub-pixely.
    pub(super) lcd: bool,
}

pub(super) struct GlyphAtlas {
    /// Default font (fallback pri family lookup miss)
    pub(super) font: fontdue::Font,
    /// Default bold variant (Segoe UI Bold etc.). Pouzity pri bx.bold=true
    /// pokud k dispozici. Jinak fake bold pres double-draw smear.
    pub(super) font_bold: Option<fontdue::Font>,
    /// Italic variant (Times Italic etc.). Pri bx.italic=true. Jinak fake
    /// skew transform (predchozi default).
    pub(super) font_italic: Option<fontdue::Font>,
    /// Bold + italic kombinace (timesbi.ttf etc.).
    pub(super) font_bold_italic: Option<fontdue::Font>,
    /// @font-face loaded fonty: family name -> Font
    pub(super) extra_fonts: std::collections::HashMap<String, fontdue::Font>,
    /// Atlas pixely (shedy: 0=transparent, 255=opaque)
    pub(super) pixels: Vec<u8>,
    /// (family, char, font_size) -> glyph info. Family "" = default.
    /// Pri bold se pouzije family "__bold__" jako klic.
    pub(super) cache: std::collections::HashMap<(String, char, u32), GlyphInfo>,
    /// Fast-path lookup pres precomputed hash. Driv `family.to_string()`
    /// alokoval String pri kazdem add() i pri cache hit, coz na strankach
    /// s ~20k chars per frame stalo ~20 ms per frame. Hash check skip
    /// alokaci pri pre-cached glyph. Hash kolize fallback na slow path.
    pub(super) cache_hashes: std::collections::HashSet<u64>,
    /// Volna pozice pro dalsi glyph
    pub(super) cursor_x: u32,
    pub(super) cursor_y: u32,
    /// Vyska aktualniho radku
    pub(super) row_height: u32,
}

impl GlyphAtlas {
    pub(super) fn new() -> Self {
        let font_data = load_default_font();
        let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
            .expect("font parse failed");
        // Try Times New Roman Bold / Segoe UI Bold / Arial Bold pro real bold
        // rendering. Fake bold (double-draw smear) je fallback pri None.
        let bold_candidates: &[&str] = &[
            "C:\\Windows\\Fonts\\timesbd.ttf",   // Times New Roman Bold (Chrome default)
            "C:\\Windows\\Fonts\\segoeuib.ttf",
            "C:\\Windows\\Fonts\\arialbd.ttf",
            "/System/Library/Fonts/SFNSBold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        ];
        let font_bold = bold_candidates.iter().find_map(|p| {
            std::fs::read(p).ok()
                .and_then(|d| fontdue::Font::from_bytes(d, fontdue::FontSettings::default()).ok())
        });
        // Italic variant (Times Italic / Arial Italic / etc.).
        let italic_candidates: &[&str] = &[
            "C:\\Windows\\Fonts\\timesi.ttf",     // Times New Roman Italic
            "C:\\Windows\\Fonts\\segoeuii.ttf",
            "C:\\Windows\\Fonts\\ariali.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Italic.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Italic.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Italic.ttf",
        ];
        let font_italic = italic_candidates.iter().find_map(|p| {
            std::fs::read(p).ok()
                .and_then(|d| fontdue::Font::from_bytes(d, fontdue::FontSettings::default()).ok())
        });
        // Bold italic.
        let bi_candidates: &[&str] = &[
            "C:\\Windows\\Fonts\\timesbi.ttf",
            "C:\\Windows\\Fonts\\segoeuiz.ttf",
            "C:\\Windows\\Fonts\\arialbi.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-BoldItalic.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-BoldItalic.ttf",
        ];
        let font_bold_italic = bi_candidates.iter().find_map(|p| {
            std::fs::read(p).ok()
                .and_then(|d| fontdue::Font::from_bytes(d, fontdue::FontSettings::default()).ok())
        });
        // Pre-load custom fonts pro DevTools UI. Resolution chain:
        // 1) primy path (relative k cwd)
        // 2) project root (parent exe dir / lookup CARGO_MANIFEST_DIR)
        // 3) embedded bytes (include_bytes!) - guaranteed fallback
        let mut extra_fonts = std::collections::HashMap::new();
        let try_load = |path: &str| -> Option<Vec<u8>> {
            if let Ok(d) = std::fs::read(path) { return Some(d); }
            // exe parent / static/fonts/...
            if let Ok(exe) = std::env::current_exe() {
                if let Some(p) = exe.parent() {
                    let alt = p.join(path);
                    if let Ok(d) = std::fs::read(&alt) { return Some(d); }
                    // Cargo deep target/debug/exe -> ../../<path>
                    if let Some(p2) = p.parent().and_then(|x| x.parent()) {
                        let alt2 = p2.join(path);
                        if let Ok(d) = std::fs::read(&alt2) { return Some(d); }
                    }
                }
            }
            None
        };
        // Embedded fallback - guarantees Inter prosli pri kazdem buildu.
        let embedded: &[(&str, &[u8])] = &[
            ("Inter", include_bytes!("../../../static/fonts/Inter-Regular.ttf")),
            ("Inter-Bold", include_bytes!("../../../static/fonts/Inter-Bold.ttf")),
            ("Inter-Italic", include_bytes!("../../../static/fonts/Inter-Italic.ttf")),
        ];
        for (family, path) in &[
            ("CamingoMono", "static/fonts/CamingoMono-Light.ttf"),
            ("CamingoMono-Bold", "static/fonts/CamingoMono-Bold.ttf"),
            ("CamingoMono-Italic", "static/fonts/CamingoMono-LightItalic.ttf"),
            ("MaterialSymbolsOutlined", "static/fonts/MaterialSymbolsOutlined.ttf"),
            ("Inter", "static/fonts/Inter-Regular.ttf"),
            ("Inter-Bold", "static/fonts/Inter-Bold.ttf"),
            ("Inter-Italic", "static/fonts/Inter-Italic.ttf"),
        ] {
            let data = try_load(path).or_else(|| {
                embedded.iter().find(|(f, _)| f == family).map(|(_, b)| b.to_vec())
            });
            if let Some(data) = data {
                if let Ok(f) = fontdue::Font::from_bytes(data, fontdue::FontSettings::default()) {
                    extra_fonts.insert(family.to_string(), f);
                } else {
                    eprintln!("[fonts] {} parse failed", family);
                }
            } else {
                eprintln!("[fonts] {} nenalezen ({})", family, path);
            }
        }
        GlyphAtlas {
            font,
            font_bold,
            font_italic,
            font_bold_italic,
            extra_fonts,
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            cache: std::collections::HashMap::new(),
            cache_hashes: std::collections::HashSet::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    /// Vrati referenci na font dle family. "" nebo neznamy -> default.
    /// Family s prefixem "__bold__:" -> bold variant pokud k dispozici.
    /// Pri comma-separated seznamu (CSS font-family fallback) iteruje
    /// kazdy alternative a vraci prvni nalezeny @font-face entry.
    pub(super) fn font_for(&self, family: &str) -> &fontdue::Font {
        // Combinace bold+italic: __bi__: prefix.
        // CRITICAL: extra_fonts (explicit family name jako "Inter-Bold") MUSI
        // mit prioritu pred system bold variant. Drive: __bold__:Inter-Bold
        // by se skoncilo na font_bold (Times Bold) a ignorovat Inter-Bold.
        if let Some(rest) = family.strip_prefix("__bi__:") {
            // Prvne explicit family v extra_fonts (napr. "Inter-Bold").
            if let Some(f) = self.extra_fonts.get(rest) { return f; }
            // Fallback na system bold-italic > italic > bold > regular.
            if let Some(f) = &self.font_bold_italic { return f; }
            if let Some(f) = &self.font_italic { return f; }
            if let Some(f) = &self.font_bold { return f; }
            return self.font_for(rest);
        }
        if let Some(rest) = family.strip_prefix("__italic__:") {
            if let Some(f) = self.extra_fonts.get(rest) { return f; }
            if let Some(f) = &self.font_italic { return f; }
            return self.font_for(rest);
        }
        if let Some(rest) = family.strip_prefix("__bold__:") {
            if let Some(f) = self.extra_fonts.get(rest) { return f; }
            if let Some(b) = &self.font_bold { return b; }
            return self.font_for(rest);
        }
        if family.is_empty() { return &self.font; }
        if let Some(f) = self.extra_fonts.get(family) { return f; }
        // CSS font-family seznam: "Roboto", "Arial", sans-serif - try each.
        for alt in family.split(',') {
            let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
            if let Some(f) = self.extra_fonts.get(trimmed) { return f; }
        }
        &self.font
    }

    pub(super) fn get(&self, family: &str, ch: char, size: u32) -> Option<&GlyphInfo> {
        self.cache.get(&(family.to_string(), ch, size))
    }

    /// Rasterize glyph and add to atlas.
    /// Pri size < LCD_THRESHOLD pouzij fontdue rasterize_subpixel = 3x sirka
    /// swizzled RGB. Render shader 3-tap sample = LCD subpixel rendering
    /// (ClearType-style, sharp text na maly fonty).
    pub(super) fn add(&mut self, family: &str, ch: char, size: u32) {
        const LCD_THRESHOLD: u32 = 24;
        // Fast path - precomputed hash lookup bez String alokace.
        // Hash kolize fallback na slow path (vzacne, prijatelne).
        let hash_key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            family.hash(&mut h);
            ch.hash(&mut h);
            size.hash(&mut h);
            h.finish()
        };
        if self.cache_hashes.contains(&hash_key) { return; }
        let key = (family.to_string(), ch, size);
        if self.cache.contains_key(&key) {
            self.cache_hashes.insert(hash_key);
            return;
        }
        let font = self.font_for(family);
        let lcd = size < LCD_THRESHOLD;
        let (metrics, bitmap) = if lcd {
            font.rasterize_subpixel(ch, size as f32)
        } else {
            font.rasterize(ch, size as f32)
        };
        let w = metrics.width as u32;
        let h = metrics.height as u32;

        // Pri LCD je bitmap w*3 sirka swizzled RGB, atlas ma store 3x cols.
        let atlas_w = if lcd { w * 3 } else { w };
        // Najdi misto v atlasu
        if self.cursor_x + atlas_w > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h > ATLAS_SIZE {
            return; // atlas full
        }
        // Copy bitmap do atlasu (bitmap.len() = atlas_w * h)
        for row in 0..h {
            for col in 0..atlas_w {
                let src = (row * atlas_w + col) as usize;
                let dst = ((self.cursor_y + row) * ATLAS_SIZE + (self.cursor_x + col)) as usize;
                if let Some(p) = bitmap.get(src) {
                    self.pixels[dst] = *p;
                }
            }
        }
        let info = GlyphInfo {
            uv0: [self.cursor_x as f32 / ATLAS_SIZE as f32,
                  self.cursor_y as f32 / ATLAS_SIZE as f32],
            uv1: [(self.cursor_x + atlas_w) as f32 / ATLAS_SIZE as f32,
                  (self.cursor_y + h) as f32 / ATLAS_SIZE as f32],
            width: w as f32,
            height: h as f32,
            bearing_x: metrics.xmin as f32,
            bearing_y: metrics.ymin as f32 + h as f32,
            advance: metrics.advance_width,
            lcd,
        };
        self.cache.insert(key, info);
        self.cache_hashes.insert(hash_key);
        self.cursor_x += atlas_w + 1;
        self.row_height = self.row_height.max(h);
    }
}

// ─── Image atlas (RGBA8 packed) ─────────────────────────────────────────

/// Velikost RGBA atlasu - 2048x2048 = 16 MB. Dost pro typickou stranku.
pub(super) const IMAGE_ATLAS_SIZE: u32 = 2048;

#[derive(Clone, Copy)]
pub struct ImageInfo {
    /// UV coords v atlasu (0..1)
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub width: f32,
    pub height: f32,
}

pub struct ImageAtlas {
    /// RGBA pixely (4 byte per pixel)
    pub(super)pixels: Vec<u8>,
    /// src URL/path -> ImageInfo
    pub(super) cache: std::collections::HashMap<String, ImageInfo>,
    /// Shelf packing kurzor
    pub(super) cursor_x: u32,
    pub(super) cursor_y: u32,
    pub(super) row_height: u32,
    /// Dirty flag - byly pridany nove obrazky -> potreba upload
    pub(super)dirty: bool,
}

impl ImageAtlas {
    pub fn new() -> Self {
        ImageAtlas {
            pixels: vec![0u8; (IMAGE_ATLAS_SIZE * IMAGE_ATLAS_SIZE * 4) as usize],
            cache: std::collections::HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    pub fn get(&self, src: &str) -> Option<&ImageInfo> {
        self.cache.get(src)
    }

    /// Test helper: count cached images.
    pub fn cache_size(&self) -> usize { self.cache.len() }

    /// Test helper: get UV bounds for src - (uv0, uv1) v 0..1 atlas range.
    pub fn uv_bounds(&self, src: &str) -> Option<([f32; 2], [f32; 2])> {
        self.cache.get(src).map(|i| (i.uv0, i.uv1))
    }

    /// Test helper: check if src is in cache.
    pub fn contains(&self, src: &str) -> bool { self.cache.contains_key(src) }

    /// Vlozi RGBA bitmap do atlasu. Pri overflow vrati false.
    pub fn add(&mut self, src: &str, w: u32, h: u32, rgba: &[u8]) -> bool {
        if self.cache.contains_key(src) { return true; }
        if w == 0 || h == 0 { return false; }
        // Obrazek vetsi nez cely atlas - nelze
        if w > IMAGE_ATLAS_SIZE || h > IMAGE_ATLAS_SIZE { return false; }

        // Shelf packing: novy radek pri preteceni X
        if self.cursor_x + w > IMAGE_ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h > IMAGE_ATLAS_SIZE {
            return false; // atlas full
        }

        // Copy RGBA bytes do atlasu
        for row in 0..h {
            let src_off = (row * w * 4) as usize;
            let dst_off = (((self.cursor_y + row) * IMAGE_ATLAS_SIZE + self.cursor_x) * 4) as usize;
            let len = (w * 4) as usize;
            if src_off + len <= rgba.len() && dst_off + len <= self.pixels.len() {
                self.pixels[dst_off..dst_off + len].copy_from_slice(&rgba[src_off..src_off + len]);
            }
        }

        let info = ImageInfo {
            uv0: [self.cursor_x as f32 / IMAGE_ATLAS_SIZE as f32,
                  self.cursor_y as f32 / IMAGE_ATLAS_SIZE as f32],
            uv1: [(self.cursor_x + w) as f32 / IMAGE_ATLAS_SIZE as f32,
                  (self.cursor_y + h) as f32 / IMAGE_ATLAS_SIZE as f32],
            width: w as f32,
            height: h as f32,
        };
        self.cache.insert(src.to_string(), info);
        self.cursor_x += w + 1;
        self.row_height = self.row_height.max(h);
        self.dirty = true;
        true
    }
}
