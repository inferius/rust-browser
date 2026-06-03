//! `SwashFontFace` - owned wrapper okolo `swash::FontRef`.
//!
//! swash::FontRef ma `data: &[u8]` borrowed reference - nemozne primo store
//! v HashMap<String, T>. SwashFontFace drzi Arc<Vec<u8>> + offset + CacheKey,
//! reconstructs FontRef on demand (cheap - 3 fields copy, no parse).
//!
//! Drive nas engine pouzival `fontdue::Font` (owned + parse-once). Swash je
//! state-of-the-art pure Rust rasterizer s TT hinting interpreter (=
//! Chrome/FF quality na 11-12px UI text). fontdue NEMA hinting = blurry small
//! text.
//!
//! Pres SwashFontFace.glyph_metrics().scale() per font+size cache + .advance()
//! per char lookup = hot path measure_text_width_full.

use swash::{CacheKey, FontRef};

/// Owned wrapper okolo `swash::FontRef` - drzi Arc<Vec<u8>> + offset + key.
#[derive(Clone)]
pub struct SwashFontFace {
    data: std::sync::Arc<Vec<u8>>,
    offset: u32,
    key: CacheKey,
}

impl SwashFontFace {
    /// Parse TTF/OTF bytes -> SwashFontFace. None pri invalid font.
    pub fn from_bytes(data: Vec<u8>) -> Option<Self> {
        let fr = FontRef::from_index(&data, 0)?;
        let (offset, key) = (fr.offset, fr.key);
        Some(Self {
            data: std::sync::Arc::new(data),
            offset,
            key,
        })
    }

    /// Reconstructs FontRef pres data borrow. Cheap - 3 fields struct.
    pub fn as_ref(&self) -> FontRef<'_> {
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }

    /// True pokud font ma glyph pro `ch` (charmap lookup != 0 = .notdef).
    pub fn has_glyph(&self, ch: char) -> bool {
        self.as_ref().charmap().map(ch) != 0
    }

    /// Glyph ID pres `ch`. 0 = .notdef (no glyph).
    pub fn glyph_id(&self, ch: char) -> u16 {
        self.as_ref().charmap().map(ch)
    }

    /// Advance width pres `ch` pri `size` px. Pro hot path measure_text -
    /// pres caller cachuje GlyphMetrics::scale per (font, size) tuple.
    pub fn advance_for(&self, ch: char, size: f32) -> f32 {
        let fr = self.as_ref();
        let gid = fr.charmap().map(ch);
        if gid == 0 {
            return 0.0;
        }
        fr.glyph_metrics(&[]).scale(size).advance_width(gid)
    }
}
