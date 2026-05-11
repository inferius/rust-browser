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
    /// @font-face loaded fonty: family name -> Vec<Font>. Vec drzi vsechny
    /// subsety daneho family (Google Fonts dela per unicode-range chunks - 30+
    /// woff2 souboru per family). Pri rasterize lookup itera vsechny subsety
    /// dokud najde font ktery ma glyph pro dany char. Bez Vec posledni
    /// register override predchozi -> Czech latin-ext subset pretazeny latin
    /// subsetem = diakritika fallback na default Times Roman.
    pub(super) extra_fonts: std::collections::HashMap<String, Vec<fontdue::Font>>,
    /// Atlas pixely (shedy: 0=transparent, 255=opaque)
    pub(super) pixels: Vec<u8>,
    /// Primary cache: (family_hash u64, char, font_size) -> GlyphInfo.
    /// Family stored hashed (FxHash by predef) - vyhne se String::to_string()
    /// na hot path text render (drive ~10k alocs/frame).
    pub(super) cache: std::collections::HashMap<(u64, char, u32), GlyphInfo, ahash::RandomState>,
    /// Family hash -> original String (pro rasterize, kde potrebujem font lookup).
    /// Inserted at first add(); read-only po vlozeni.
    pub(super) family_names: std::collections::HashMap<u64, String, ahash::RandomState>,
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
                    extra_fonts.entry(family.to_string()).or_insert_with(Vec::new).push(f.clone());
                    // Register tez pro layout measure_text_width_full - bez
                    // tohoto Inter/CamingoMono measure pouzival system sans/serif
                    // (jine metrics) -> rect width neshodi render glyph width.
                    crate::browser::layout::register_measure_font(family, f.clone());
                    // Pro <family>-Bold / <family>-Italic - take pridat styled key
                    // aby `__bold__:Inter` lookup uspesny (Inter-Bold = bold variant).
                    if family.ends_with("-Bold") {
                        let base = &family[..family.len() - 5];
                        crate::browser::layout::register_measure_font(
                            &format!("{}__bold__", base), f);
                    } else if family.ends_with("-Italic") {
                        let base = &family[..family.len() - 7];
                        crate::browser::layout::register_measure_font(
                            &format!("{}__italic__", base), f);
                    }
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
            cache: std::collections::HashMap::with_hasher(ahash::RandomState::new()),
            family_names: std::collections::HashMap::with_hasher(ahash::RandomState::new()),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    /// True pokud font umi rasterizovat dany char (= ma glyph index != 0).
    /// Pouzite pro fallback chain pri diakritice (Times Roman nema CP > U+00FF).
    #[inline]
    pub(super) fn has_glyph(font: &fontdue::Font, ch: char) -> bool {
        font.lookup_glyph_index(ch) != 0
    }

    /// Helper - prvni font z Vec (pokud existuje neprazdny). Pouzite v
    /// font_for() pro "give me primary font for this family" semantics.
    #[inline]
    fn first_font<'a>(vec: &'a Vec<fontdue::Font>) -> Option<&'a fontdue::Font> {
        vec.first()
    }

    /// Vrati font ktery umi rasterizovat dany char. Postup:
    /// 1) Itera vsechny subsety primary family (Google Fonts: 30+ woff2 per family,
    ///    kazdy subset jiny unicode-range).
    /// 2) Pri commaseparated CSS list (`"Roboto", "Arial", sans-serif`) zkousi
    ///    kazdy alternative.
    /// 3) Pak vsechny ostatni extra_fonts + system fonts.
    /// 4) Last resort default font (i kdyz neumi - prazdny glyph lepsi crash).
    pub(super) fn font_for_char(&self, family: &str, ch: char) -> &fontdue::Font {
        // ASCII space / control: vse umi - vrat primary.
        if (ch as u32) < 0x20 {
            return self.font_for(family);
        }
        // Detect style: __bi__ / __italic__ / __bold__ prefix.
        let (style_suffix, raw_family) = if let Some(rest) = family.strip_prefix("__bi__:") {
            ("__bi__", rest)
        } else if let Some(rest) = family.strip_prefix("__italic__:") {
            ("__italic__", rest)
        } else if let Some(rest) = family.strip_prefix("__bold__:") {
            ("__bold__", rest)
        } else {
            ("", family)
        };
        // 1) Style-specific subsety primary family (Ubuntu__bold__ pred Ubuntu).
        if !raw_family.is_empty() && !style_suffix.is_empty() {
            let styled_key = format!("{}{}", raw_family, style_suffix);
            if let Some(vec) = self.extra_fonts.get(&styled_key) {
                for f in vec {
                    if Self::has_glyph(f, ch) { return f; }
                }
            }
        }
        // 2) Primary family - vsechny subsety regular.
        if !raw_family.is_empty() {
            // Direct family match (font-family: 'Roboto').
            if let Some(vec) = self.extra_fonts.get(raw_family) {
                for f in vec {
                    if Self::has_glyph(f, ch) { return f; }
                }
            }
            // CSS comma list (font-family: "Roboto", "Arial", sans-serif).
            for alt in raw_family.split(',') {
                let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
                if trimmed.is_empty() || trimmed == raw_family { continue; }
                // Take style-specific pres comma list.
                if !style_suffix.is_empty() {
                    let sk = format!("{}{}", trimmed, style_suffix);
                    if let Some(vec) = self.extra_fonts.get(&sk) {
                        for f in vec {
                            if Self::has_glyph(f, ch) { return f; }
                        }
                    }
                }
                if let Some(vec) = self.extra_fonts.get(trimmed) {
                    for f in vec {
                        if Self::has_glyph(f, ch) { return f; }
                    }
                }
            }
        }
        // 2) Vsechny ostatni extra_fonts (jine families - last-resort
        //    Czech znaky z jineho fontu, kdyz primary subset nepokryva).
        for vec in self.extra_fonts.values() {
            for f in vec {
                if Self::has_glyph(f, ch) { return f; }
            }
        }
        // 3) System fonts.
        if let Some(b) = &self.font_bold { if Self::has_glyph(b, ch) { return b; } }
        if let Some(i) = &self.font_italic { if Self::has_glyph(i, ch) { return i; } }
        if let Some(bi) = &self.font_bold_italic { if Self::has_glyph(bi, ch) { return bi; } }
        // 4) Default font (primary fallback bez glyph check - cluster glyph 0).
        &self.font
    }

    /// Vrati referenci na "primary" font dle family (= prvni subset z Vec
    /// pokud @font-face, jinak system font). Pouziti pro initial metrics,
    /// pre-rasterize pass. Pro per-char glyph rasterize pouzij font_for_char.
    /// "" nebo neznamy -> default. "__bold__:" / "__italic__:" / "__bi__:"
    /// prefixy preferuji styled variant.
    pub(super) fn font_for(&self, family: &str) -> &fontdue::Font {
        if let Some(rest) = family.strip_prefix("__bi__:") {
            // Hledat <family>__bi__ v extra_fonts (registrace per @font-face
            // weight + italic key). Fallback chain pres bold-only, italic-only,
            // regular family, system bold-italic.
            let bi_key = format!("{}__bi__", rest);
            if let Some(f) = self.extra_fonts.get(&bi_key).and_then(Self::first_font) { return f; }
            let bold_key = format!("{}__bold__", rest);
            if let Some(f) = self.extra_fonts.get(&bold_key).and_then(Self::first_font) { return f; }
            let italic_key = format!("{}__italic__", rest);
            if let Some(f) = self.extra_fonts.get(&italic_key).and_then(Self::first_font) { return f; }
            if let Some(f) = self.extra_fonts.get(rest).and_then(Self::first_font) { return f; }
            if let Some(f) = &self.font_bold_italic { return f; }
            if let Some(f) = &self.font_italic { return f; }
            if let Some(f) = &self.font_bold { return f; }
            return self.font_for(rest);
        }
        if let Some(rest) = family.strip_prefix("__italic__:") {
            let italic_key = format!("{}__italic__", rest);
            if let Some(f) = self.extra_fonts.get(&italic_key).and_then(Self::first_font) { return f; }
            if let Some(f) = self.extra_fonts.get(rest).and_then(Self::first_font) { return f; }
            if let Some(f) = &self.font_italic { return f; }
            return self.font_for(rest);
        }
        if let Some(rest) = family.strip_prefix("__bold__:") {
            let bold_key = format!("{}__bold__", rest);
            if let Some(f) = self.extra_fonts.get(&bold_key).and_then(Self::first_font) { return f; }
            // CSS comma list (font-family: "Ubuntu", "Roboto", sans-serif).
            for alt in rest.split(',') {
                let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
                let bk = format!("{}__bold__", trimmed);
                if let Some(f) = self.extra_fonts.get(&bk).and_then(Self::first_font) { return f; }
            }
            if let Some(f) = self.extra_fonts.get(rest).and_then(Self::first_font) { return f; }
            if let Some(b) = &self.font_bold { return b; }
            return self.font_for(rest);
        }
        if family.is_empty() { return &self.font; }
        if let Some(f) = self.extra_fonts.get(family).and_then(Self::first_font) { return f; }
        // CSS font-family seznam: "Roboto", "Arial", sans-serif - try each.
        for alt in family.split(',') {
            let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
            if let Some(f) = self.extra_fonts.get(trimmed).and_then(Self::first_font) { return f; }
        }
        &self.font
    }

    /// Hash family name pres ahash. Pouziva se jako stable lookup key bez
    /// String allokace. Caller muze cache predcomputed hash + lookup pres
    /// `get_hashed` pri opakovanym lookup s same family v hot loopu.
    #[inline]
    pub(super) fn hash_family(family: &str) -> u64 {
        use std::hash::{BuildHasher, Hash, Hasher};
        let s = ahash::RandomState::with_seeds(0xdead_beef_5555_aaaa, 0xfeed_face_cafe_d00d,
                                                0x1234_5678_9abc_def0, 0xface_b00c_0000_1111);
        let mut h = s.build_hasher();
        family.hash(&mut h);
        h.finish()
    }

    pub(super) fn get(&self, family: &str, ch: char, size: u32) -> Option<&GlyphInfo> {
        self.cache.get(&(Self::hash_family(family), ch, size))
    }

    /// Lookup s pre-hashovany family. Caller spocte hash JEDNOU pred char loopem.
    #[inline]
    pub(super) fn get_hashed(&self, family_hash: u64, ch: char, size: u32) -> Option<&GlyphInfo> {
        self.cache.get(&(family_hash, ch, size))
    }

    /// Rasterize glyph and add to atlas.
    /// Pri size < LCD_THRESHOLD pouzij fontdue rasterize_subpixel = 3x sirka
    /// swizzled RGB. Render shader 3-tap sample = LCD subpixel rendering
    /// (ClearType-style, sharp text na maly fonty).
    pub(super) fn add(&mut self, family: &str, ch: char, size: u32) {
        const LCD_THRESHOLD: u32 = 24;
        let family_hash = Self::hash_family(family);
        let key = (family_hash, ch, size);
        if self.cache.contains_key(&key) { return; }
        // Lazy populate family_names pri prvni vlozeni (rare, jen unique families).
        if !self.family_names.contains_key(&family_hash) {
            self.family_names.insert(family_hash, family.to_string());
        }
        // font_for_char dela fallback chain pri primary fontu chybejicim
        // glyph (Times Roman nema CP > U+00FF -> diakritika fallne na default).
        let font = self.font_for_char(family, ch);
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
