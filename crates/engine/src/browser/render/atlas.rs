//! Glyph atlas + Image atlas - RGBA8 packed textures pro text + img rendering.
//!
//! GlyphAtlas: shelf-pack font glyphs, klic = (family, char, size). LCD subpixel
//!   pri size<24 (3x sirka swizzled R/G/B). Multi-font: default/bold/italic/bold-italic + extra.
//! ImageAtlas: shelf-pack RGBA images, klic = src URL. Re-raster pri zoom zmene.

use super::SwashFontFace;

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
    /// LCD subpixel: pri size < threshold rasterujeme pres swash Subpixel
    /// format = RGBA 4 byte per pixel. Atlas storage 3x sirka swizzled RGB
    /// (R/G/B z 0/1/2 byte). Render shader pak sample 3 horizontal texely per
    /// fragment pro R/G/B sub-pixely (mode 9 unchanged).
    pub(super) lcd: bool,
}

pub struct GlyphAtlas {
    /// Default font (fallback pri family lookup miss) - serif (Times Roman).
    pub(super) font: SwashFontFace,
    /// Default bold variant (Times Bold etc.). Pouzity pri bx.bold=true
    /// pokud k dispozici. Jinak fake bold pres double-draw smear.
    pub(super) font_bold: Option<SwashFontFace>,
    /// Italic variant (Times Italic etc.). Pri bx.italic=true. Jinak fake
    /// skew transform (predchozi default).
    pub(super) font_italic: Option<SwashFontFace>,
    /// Bold + italic kombinace (timesbi.ttf etc.).
    pub(super) font_bold_italic: Option<SwashFontFace>,
    /// NOTE: sans-serif / monospace fonts loaded pres `extra_fonts` map
    /// keys "Segoe UI"/"Arial"/"sans-serif"/"Consolas"/"monospace" (viz
    /// system_aliases v new()). font_for_char pres classify keyword routing.
    /// @font-face loaded fonty: family name -> Vec<Font>. Vec drzi vsechny
    /// subsety daneho family (Google Fonts dela per unicode-range chunks - 30+
    /// woff2 souboru per family). Pri rasterize lookup itera vsechny subsety
    /// dokud najde font ktery ma glyph pro dany char. Bez Vec posledni
    /// register override predchozi -> Czech latin-ext subset pretazeny latin
    /// subsetem = diakritika fallback na default Times Roman.
    pub(super) extra_fonts: std::collections::HashMap<String, Vec<SwashFontFace>>,
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
    /// Dirty flag - byly pridany nove glyphs po posledni upload.
    /// upload_atlas dela 16MB GPU upload - skip kdyz no new glyphs (perf).
    pub(super) dirty: bool,
}

impl GlyphAtlas {
    pub(super) fn new() -> Self {
        let font_data = load_default_font();
        let font = SwashFontFace::from_bytes(font_data)
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
            std::fs::read(p).ok().and_then(SwashFontFace::from_bytes)
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
            std::fs::read(p).ok().and_then(SwashFontFace::from_bytes)
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
            std::fs::read(p).ok().and_then(SwashFontFace::from_bytes)
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
            ("Inter", include_bytes!("../../../../../static/fonts/Inter-Regular.ttf")),
            ("Inter-Bold", include_bytes!("../../../../../static/fonts/Inter-Bold.ttf")),
            ("Inter-Italic", include_bytes!("../../../../../static/fonts/Inter-Italic.ttf")),
        ];
        // Registrace system fonts s family aliases. Bez tohoto atlas font_for
        // pri "Segoe UI" / "Arial" / "sans-serif" / "Courier" spadne na Times
        // (self.font). shape_text vs atlas: SHAPE pouziva sans_opt (Segoe UI),
        // atlas fallback na Times -> glyph metrics neshoduje -> render overlap.
        // Pres explicit register kazdou family name + variant atlas resolve
        // same font jako shape_text_advances.
        let system_aliases: &[(&str, &str)] = &[
            // Sans-serif system family aliases - all point na Segoe UI on Win.
            ("Segoe UI", "C:\\Windows\\Fonts\\segoeui.ttf"),
            ("Arial", "C:\\Windows\\Fonts\\arial.ttf"),
            ("sans-serif", "C:\\Windows\\Fonts\\segoeui.ttf"),
            ("Verdana", "C:\\Windows\\Fonts\\verdana.ttf"),
            // Monospace.
            ("Courier New", "C:\\Windows\\Fonts\\cour.ttf"),
            ("Consolas", "C:\\Windows\\Fonts\\consola.ttf"),
            ("monospace", "C:\\Windows\\Fonts\\cour.ttf"),
        ];
        for (family, path) in system_aliases {
            if let Ok(data) = std::fs::read(path) {
                if let Some(f) = SwashFontFace::from_bytes(data) {
                    extra_fonts.entry(family.to_string()).or_insert_with(Vec::new).push(f.clone());
                    crate::browser::layout::register_measure_font(family, f);
                }
            }
        }
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
                if let Some(f) = SwashFontFace::from_bytes(data) {
                    extra_fonts.entry(family.to_string()).or_insert_with(Vec::new).push(f.clone());
                    // Register tez pro layout measure_text_width_full - bez
                    // tohoto Inter/CamingoMono measure pouzival system sans/serif
                    // (jine metrics) -> rect width neshodi render glyph width.
                    crate::browser::layout::register_measure_font(family, f);
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
            dirty: false,
        }
    }

    /// True pokud font umi rasterizovat dany char (= ma glyph index != 0).
    /// Pouzite pro fallback chain pri diakritice (Times Roman nema CP > U+00FF).
    #[inline]
    pub(super) fn has_glyph(face: &SwashFontFace, ch: char) -> bool {
        face.has_glyph(ch)
    }

    /// Helper - prvni font z Vec (pokud existuje neprazdny). Pouzite v
    /// font_for() pro "give me primary font for this family" semantics.
    #[inline]
    fn first_font(vec: &Vec<SwashFontFace>) -> Option<&SwashFontFace> {
        vec.first()
    }

    /// Vrati font ktery umi rasterizovat dany char. Postup:
    /// 1) Itera vsechny subsety primary family (Google Fonts: 30+ woff2 per family,
    ///    kazdy subset jiny unicode-range).
    /// 2) Pri commaseparated CSS list (`"Roboto", "Arial", sans-serif`) zkousi
    ///    kazdy alternative.
    /// 3) Pak vsechny ostatni extra_fonts + system fonts.
    /// 4) Last resort default font (i kdyz neumi - prazdny glyph lepsi crash).
    pub(super) fn font_for_char(&self, family: &str, ch: char) -> &SwashFontFace {
        // ASCII space / control: vse umi - vrat primary.
        if (ch as u32) < 0x20 {
            return self.font_for(family);
        }
        // Compact weight prefix `__wN_<I>__:family` (new format) - delegate na
        // font_for_weight, ktery dela CSS Fonts L4 nearest-match. Pri ne-match
        // exact weight + has_glyph, font_for_weight uz fallback chainuje.
        if let Some(rest) = family.strip_prefix("__w") {
            if let Some(sep_idx) = rest.find("__:") {
                let head = &rest[..sep_idx];
                let raw_family = &rest[sep_idx + 3..];
                if let Some(underscore) = head.rfind('_') {
                    let weight_str = &head[..underscore];
                    let italic_str = &head[underscore + 1..];
                    if let Ok(weight) = weight_str.parse::<u32>() {
                        let italic = italic_str == "1";
                        // Empty raw_family = caller bez explicit font-family.
                        // Skip extra_fonts walk (Inter / @font-face by ji shadowed
                        // system default Times) a vrat system font - shoduje s
                        // measure_text_width_weight default fallback.
                        if raw_family.is_empty() {
                            if italic && weight >= 600 {
                                if let Some(f) = &self.font_bold_italic { if Self::has_glyph(f, ch) { return f; } }
                            }
                            if italic {
                                if let Some(f) = &self.font_italic { if Self::has_glyph(f, ch) { return f; } }
                            }
                            if weight >= 600 {
                                if let Some(f) = &self.font_bold { if Self::has_glyph(f, ch) { return f; } }
                            }
                            if Self::has_glyph(&self.font, ch) { return &self.font; }
                            // Char ne v system Times -> fallback extra_fonts walk.
                            for vec in self.extra_fonts.values() {
                                for f in vec {
                                    if Self::has_glyph(f, ch) { return f; }
                                }
                            }
                            return &self.font;
                        }
                        // Iter VSECH subsets requested weight (Google Fonts splits
                        // family do unicode-range subsets - cyrillic-ext, latin,
                        // latin-ext, ...). Prvni s glyph wins. Bez tohoto Vec.first
                        // mohl byt cyrillic-ext bez ASCII 'a' -> fallback heavy
                        // weight chain.
                        let suffix = if italic { "__i__" } else { "__" };
                        let opp_suffix = if italic { "__" } else { "__i__" };
                        let order: Vec<u32> = if weight < 400 {
                            let mut v: Vec<u32> = (100..=weight).rev().collect();
                            v.extend([400, 500, 600, 700, 800, 900].iter().copied());
                            v
                        } else if weight <= 500 {
                            let mut v = vec![weight];
                            if weight != 500 { v.push(500); }
                            if weight != 400 { v.push(400); }
                            v.extend([300, 200, 100, 600, 700, 800, 900].iter().copied());
                            v
                        } else {
                            let mut v = vec![weight];
                            for w in [600, 700, 800, 900] { if w != weight { v.push(w); } }
                            v.extend([500, 400, 300, 200, 100].iter().copied());
                            v
                        };
                        // Try requested italic - vsech weight buckets, vsech subsets.
                        for w in &order {
                            let key = format!("{}__w{}{}", raw_family, w, suffix);
                            if let Some(vec) = self.extra_fonts.get(&key) {
                                for f in vec {
                                    if Self::has_glyph(f, ch) { return f; }
                                }
                            }
                        }
                        // Try opposite italic.
                        for w in &order {
                            let key = format!("{}__w{}{}", raw_family, w, opp_suffix);
                            if let Some(vec) = self.extra_fonts.get(&key) {
                                for f in vec {
                                    if Self::has_glyph(f, ch) { return f; }
                                }
                            }
                        }
                        // Try base family bez weight key.
                        if let Some(vec) = self.extra_fonts.get(raw_family) {
                            for f in vec {
                                if Self::has_glyph(f, ch) { return f; }
                            }
                        }
                        // CSS comma list - try each alt s same weight chain.
                        for alt in raw_family.split(',') {
                            let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
                            if trimmed.is_empty() || trimmed == raw_family { continue; }
                            for w in &order {
                                let key = format!("{}__w{}{}", trimmed, w, suffix);
                                if let Some(vec) = self.extra_fonts.get(&key) {
                                    for f in vec {
                                        if Self::has_glyph(f, ch) { return f; }
                                    }
                                }
                            }
                            // Plain alt key bez weight - system_aliases registruje
                            // pres "Segoe UI" / "Arial" / "sans-serif" / "monospace".
                            // Bez tohoto fallback by atlas spadl na "all extra_fonts
                            // walk" = arbitrary HashMap order = mismatch shape_text
                            // deterministic pick = chars rozsekane.
                            if let Some(vec) = self.extra_fonts.get(trimmed) {
                                for f in vec {
                                    if Self::has_glyph(f, ch) { return f; }
                                }
                            }
                        }
                        // Pred fallback all-extra walk: shape_text classify (is_sans/
                        // is_mono) musi matchovat. Direct call do classify selektoru
                        // + system aliases lookup. Tohle drzi shape_text vs atlas font
                        // deterministicky stejny.
                        let lower = raw_family.to_lowercase();
                        let is_mono = lower.contains("monospace")
                            || lower.contains("courier")
                            || lower.contains("consolas")
                            || lower.contains("monaco")
                            || lower.contains("menlo");
                        let is_sans = !is_mono && (
                            lower.contains("sans-serif")
                            || lower.contains("arial")
                            || lower.contains("helvetica")
                            || lower.contains("segoe")
                            || lower.contains("verdana")
                            || lower.contains("inter")
                            || lower.contains("roboto")
                            || lower.contains("system-ui"));
                        if is_mono {
                            if let Some(vec) = self.extra_fonts.get("monospace") {
                                for f in vec { if Self::has_glyph(f, ch) { return f; } }
                            }
                            if let Some(vec) = self.extra_fonts.get("Consolas") {
                                for f in vec { if Self::has_glyph(f, ch) { return f; } }
                            }
                        }
                        if is_sans {
                            if let Some(vec) = self.extra_fonts.get("sans-serif") {
                                for f in vec { if Self::has_glyph(f, ch) { return f; } }
                            }
                            if let Some(vec) = self.extra_fonts.get("Segoe UI") {
                                for f in vec { if Self::has_glyph(f, ch) { return f; } }
                            }
                        }
                        // Fallback char-coverage chain pres ostatni extra_fonts.
                        for vec in self.extra_fonts.values() {
                            for f in vec {
                                if Self::has_glyph(f, ch) { return f; }
                            }
                        }
                        if let Some(b) = &self.font_bold { if Self::has_glyph(b, ch) { return b; } }
                        if let Some(i) = &self.font_italic { if Self::has_glyph(i, ch) { return i; } }
                        if Self::has_glyph(&self.font, ch) { return &self.font; }
                        return self.font_for_weight(raw_family, weight, italic);
                    }
                }
            }
        }
        // Plain family lookup (callsite bez weight/italic context, napr.
        // intrinsic ASCII metrics). Pro styled lookup vola se font_for_char
        // pres __w<N>_<I>__: compact prefix (vyse).
        if !family.is_empty() {
            if let Some(vec) = self.extra_fonts.get(family) {
                for f in vec {
                    if Self::has_glyph(f, ch) { return f; }
                }
            }
            // CSS comma list (font-family: "Roboto", "Arial", sans-serif).
            for alt in family.split(',') {
                let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
                if trimmed.is_empty() || trimmed == family { continue; }
                if let Some(vec) = self.extra_fonts.get(trimmed) {
                    for f in vec {
                        if Self::has_glyph(f, ch) { return f; }
                    }
                }
            }
        }
        // Last-resort fallback: vsechny ostatni extra_fonts (jine families -
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

    /// CSS Fonts L4 nearest-match algorithm pres @font-face weight variants.
    /// Pri exact `<family>__w<weight>__[i__]` key v extra_fonts use. Jinak
    /// fallback dle CSS spec:
    /// - weight < 400: prefer weight, klesat (250->200->150->100), pak 400+.
    /// - weight == 400/500: prefer requested, then 500/400, then klesat.
    /// - weight >= 500 (light path) - prefer up to 500.
    /// - weight >= 600: prefer requested, then higher, then lower.
    pub(super) fn font_for_weight(&self, family: &str, weight: u32, italic: bool) -> &SwashFontFace {
        let suffix = if italic { "__i__" } else { "__" };
        // Search order per CSS Fonts L4 spec:
        // Build search list of weight buckets to try.
        let order: Vec<u32> = if weight < 400 {
            // Lower than 400: prefer requested, descending, then ascending from 400+.
            let mut v: Vec<u32> = (100..=weight).rev().collect();
            v.extend([400, 500, 600, 700, 800, 900].iter().copied());
            v
        } else if weight <= 500 {
            // 400 or 500: prefer 400/500 chain, then descending light, then ascending heavy.
            let mut v = vec![weight];
            if weight != 500 { v.push(500); }
            if weight != 400 { v.push(400); }
            v.extend([300, 200, 100, 600, 700, 800, 900].iter().copied());
            v
        } else {
            // >= 600 (bold path): prefer requested, then heavier, then lighter.
            let mut v = vec![weight];
            for w in [600, 700, 800, 900] {
                if w != weight { v.push(w); }
            }
            v.extend([500, 400, 300, 200, 100].iter().copied());
            v
        };
        // Try styled variant per requested italic.
        for w in &order {
            let key = format!("{}__w{}{}", family, w, suffix);
            if let Some(f) = self.extra_fonts.get(&key).and_then(Self::first_font) { return f; }
        }
        // Try opposite italic (italic 400 -> regular 400) v ramci weight nearest.
        let opp_suffix = if italic { "__" } else { "__i__" };
        for w in &order {
            let key = format!("{}__w{}{}", family, w, opp_suffix);
            if let Some(f) = self.extra_fonts.get(&key).and_then(Self::first_font) {
                return f;
            }
        }
        // Try regular family bez weight suffix (legacy / unknown weights).
        if let Some(f) = self.extra_fonts.get(family).and_then(Self::first_font) { return f; }
        // CSS comma list.
        for alt in family.split(',') {
            let trimmed = alt.trim().trim_matches('"').trim_matches('\'');
            for w in &order {
                let key = format!("{}__w{}{}", trimmed, w, suffix);
                if let Some(f) = self.extra_fonts.get(&key).and_then(Self::first_font) {
                    return f;
                }
            }
            if let Some(f) = self.extra_fonts.get(trimmed).and_then(Self::first_font) { return f; }
        }
        // System fallback.
        if weight >= 600 && italic {
            if let Some(f) = &self.font_bold_italic { return f; }
        }
        if italic {
            if let Some(f) = &self.font_italic { return f; }
        }
        if weight >= 600 {
            if let Some(f) = &self.font_bold { return f; }
        }
        &self.font
    }

    /// Vrati referenci na "primary" font dle family (= prvni subset z Vec
    /// pokud @font-face, jinak system font). Pouziti pro initial metrics,
    /// pre-rasterize pass. Pro per-char glyph rasterize pouzij font_for_char.
    ///
    /// Pripousti compact prefix `__w<N>_<I>__:family` (legacy encoding -
    /// callsite radsi vola font_for_weight primo). "" nebo neznamy -> default.
    pub(super) fn font_for(&self, family: &str) -> &SwashFontFace {
        // Compact weight prefix: `__wN_<I>__:family` kde N = weight 1..1000,
        // I = 0/1 (italic). Pri match call do CSS Fonts L4 nearest-match.
        if let Some(rest) = family.strip_prefix("__w") {
            if let Some(sep_idx) = rest.find("__:") {
                let head = &rest[..sep_idx];
                let raw_family = &rest[sep_idx + 3..];
                if let Some(underscore) = head.rfind('_') {
                    let weight_str = &head[..underscore];
                    let italic_str = &head[underscore + 1..];
                    if let Ok(weight) = weight_str.parse::<u32>() {
                        let italic = italic_str == "1";
                        return self.font_for_weight(raw_family, weight, italic);
                    }
                }
            }
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

    /// Encode (family, weight, italic) na compact prefix string pro cache
    /// keys + font_for lookup. Format: `__w<N>_<I>__:family` kde N=weight,
    /// I=italic (0/1). Caller pak passuje string do get/add - cache klic je
    /// hash(string), neni allocace pri lookup pres hash_family.
    pub(super) fn compose_styled(family: &str, weight: u32, italic: bool) -> String {
        let italic_i = if italic { 1u8 } else { 0u8 };
        format!("__w{}_{}__:{}", weight, italic_i, family)
    }

    /// Styled glyph lookup - typed wrapper nad get(compose_styled(...), ch, size).
    #[inline]
    pub(super) fn get_styled(&self, family: &str, weight: u32, italic: bool, ch: char, size: u32) -> Option<&GlyphInfo> {
        let key = Self::compose_styled(family, weight, italic);
        self.get(&key, ch, size)
    }

    /// Styled glyph rasterize + cache. Typed wrapper nad add(compose_styled(...), ch, size).
    #[inline]
    pub(super) fn add_styled(&mut self, family: &str, weight: u32, italic: bool, ch: char, size: u32) {
        let key = Self::compose_styled(family, weight, italic);
        self.add(&key, ch, size);
    }

    /// Rasterize glyph and add to atlas.
    /// Pri size < LCD_THRESHOLD pouzij swash Subpixel format (RGBA 4 byte/pixel),
    /// rozbal R/G/B do 3x atlas cols swizzled jako pres fontdue puvodne. Render
    /// shader mode 9 sample 3 horizontal texely per fragment pro RGB sub-pixely.
    /// Pres velke fs (>=24) Alpha format = 1 byte/pixel = bilinear sample.
    pub(super) fn add(&mut self, family: &str, ch: char, size: u32) {
        use swash::scale::{ScaleContext, Render, Source};
        use swash::zeno::Format;

        thread_local! {
            static SCALE_CTX: std::cell::RefCell<ScaleContext> =
                std::cell::RefCell::new(ScaleContext::new());
        }

        // LCD subpixel raster jen pres male fs. Pres velke fs (>24) regular
        // raster (= bilinear sample). LCD 3-tap pri velkem bitmap = 3 taps within
        // 1 display pixel = grayscale = WORSE nez bilinear.
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
        // Borrow face -> compute image + advance, pak drop borrow pred pixel write.
        let lcd = size < LCD_THRESHOLD;
        let (image, advance) = {
            let face = self.font_for_char(family, ch);
            let font_ref = face.as_ref();
            let gid = font_ref.charmap().map(ch);
            if gid == 0 { return; }
            let image_opt = SCALE_CTX.with(|ctx| {
                let mut ctx = ctx.borrow_mut();
                let mut scaler = ctx.builder(font_ref)
                    .size(size as f32)
                    .hint(true)
                    .build();
                let mut render = Render::new(&[Source::Outline]);
                if lcd {
                    render.format(Format::Subpixel);
                } else {
                    render.format(Format::Alpha);
                }
                render.render(&mut scaler, gid)
            });
            let image = match image_opt { Some(i) => i, None => return };
            // Advance via glyph_metrics(&[]).scale(size).advance_width(gid).
            let advance = font_ref.glyph_metrics(&[]).scale(size as f32).advance_width(gid);
            // DIAG: debug swash output dimensions vs expected bytes-per-pixel.
            if std::env::var("RWE_SWASH_DEBUG").is_ok() {
                let w_ = image.placement.width;
                let h_ = image.placement.height;
                let data_len = image.data.len();
                let bpp_calc = if w_ > 0 && h_ > 0 { data_len as f32 / (w_ * h_) as f32 } else { 0.0 };
                eprintln!("[SWASH] ch={:?} size={} lcd={} placement={}x{} adv={:.2} bear=({},{}) data_len={} bpp={:.2}",
                    ch, size, lcd, w_, h_, advance, image.placement.left, image.placement.top, data_len, bpp_calc);
            }
            (image, advance)
        };

        let w = image.placement.width;
        let h = image.placement.height;

        // Swash Subpixel format = 4 bytes per pixel (R,G,B,A). Pro legacy atlas
        // layout (3x sirka R-channel) rozbalit RGB do 3 horizontal texelu.
        // Alpha format = 1 byte per pixel.
        let atlas_w = if lcd { w * 3 } else { w };

        if self.cursor_x + atlas_w > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h > ATLAS_SIZE { return; }

        if lcd {
            // Swash Format::Subpixel gives 3 bytes/pixel (RGB triplet, NO alpha).
            // Pack R, G, B as 3 horizontal texels v atlas (legacy shader mode 9
            // 3x R-channel layout). Drive `* 4` predpoklad RGBA byl SPATNY - src
            // index pres ven byte array -> skip copy -> atlas glyph oriznute na
            // ~25% sirky -> chars overlap pres LCD render.
            let bytes_per_px = if image.data.len() >= (w * h * 4) as usize { 4 } else { 3 };
            for row in 0..h {
                for col in 0..w {
                    let src_idx = (row * w + col) as usize * bytes_per_px;
                    let dst_base = ((self.cursor_y + row) * ATLAS_SIZE + self.cursor_x + col * 3) as usize;
                    if src_idx + 2 < image.data.len() && dst_base + 2 < self.pixels.len() {
                        self.pixels[dst_base]     = image.data[src_idx];     // R
                        self.pixels[dst_base + 1] = image.data[src_idx + 1]; // G
                        self.pixels[dst_base + 2] = image.data[src_idx + 2]; // B
                    }
                }
            }
        } else {
            for row in 0..h {
                let src_row = (row * w) as usize;
                let dst_row = ((self.cursor_y + row) * ATLAS_SIZE + self.cursor_x) as usize;
                let len = w as usize;
                if src_row + len <= image.data.len() && dst_row + len <= self.pixels.len() {
                    self.pixels[dst_row..dst_row + len]
                        .copy_from_slice(&image.data[src_row..src_row + len]);
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
            // fontdue bearing_x = xmin, bearing_y = ymin + height (= top from baseline).
            // swash placement.left = xmin, placement.top = baseline-relative top.
            bearing_x: image.placement.left as f32,
            bearing_y: image.placement.top as f32,
            advance,
            lcd,
        };
        self.cache.insert(key, info);
        self.cursor_x += atlas_w + 1;
        if h > self.row_height { self.row_height = h; }
        self.dirty = true;
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

    /// Vlozi NEBO prepise existujici entry. Pro animovany obsah (inline SVG/canvas
    /// co se meni kazdy frame) - pri shode dims OVERWRITE pixely na existujicim
    /// slotu (UV se nemeni, zadny novy slot = zadny atlas rust). Pri zmene dims
    /// stary slot zahozen (vzacne - jen resize/zoom) + novy slot. Bez `replace`
    /// kazdy frame = novy slot = atlas se zaplni za sekundy + neomezeny RAM/GPU.
    pub fn replace(&mut self, src: &str, w: u32, h: u32, rgba: &[u8]) -> bool {
        if w == 0 || h == 0 { return false; }
        if let Some(info) = self.cache.get(src).copied() {
            if info.width as u32 == w && info.height as u32 == h {
                let sx = (info.uv0[0] * IMAGE_ATLAS_SIZE as f32).round() as u32;
                let sy = (info.uv0[1] * IMAGE_ATLAS_SIZE as f32).round() as u32;
                for row in 0..h {
                    let src_off = (row * w * 4) as usize;
                    let dst_off = (((sy + row) * IMAGE_ATLAS_SIZE + sx) * 4) as usize;
                    let len = (w * 4) as usize;
                    if src_off + len <= rgba.len() && dst_off + len <= self.pixels.len() {
                        self.pixels[dst_off..dst_off + len].copy_from_slice(&rgba[src_off..src_off + len]);
                    }
                }
                self.dirty = true;
                return true;
            }
            // Dims se zmenily -> stary slot leakne (bounded, vzacne), novy slot.
            self.cache.remove(src);
        }
        self.add(src, w, h, rgba)
    }
}
