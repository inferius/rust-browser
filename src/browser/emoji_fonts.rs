//! Color emoji font detection + COLR/CPAL parser.
//!
//! Color glyph tables v OpenType:
//! - **COLR**: vector layered (each glyph = stack of monochrome glyphs + palette colors).
//!   COLR v0 = simple layers, COLR v1 = paint graph (gradients, transforms, ...)
//! - **CPAL**: color palette s seznamem RGBA color records (referencovany z COLR).
//! - **CBDT/CBLC**: bitmap data + locations (PNG/JPEG embedded).
//! - **sbix**: Apple bitmap (PNG strikes per glyph).
//! - **SVG**: SVG glyphs.
//!
//! Aktualne tato modul jen DETEKUJE tabulky a parsuje COLR v0 base/layer info
//! pro forward compat. Real rendering vyzaduje RGBA atlas + per-glyph rasterizer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorFormat {
    None,
    ColrV0,
    ColrV1,
    Cbdt,
    Sbix,
    Svg,
}

#[derive(Debug, Clone)]
pub struct ColorFontInfo {
    pub format: ColorFormat,
    /// Pocet base glyfu (jen pro COLR).
    pub colr_base_count: u32,
    /// Pocet vrstev (COLR layers).
    pub colr_layer_count: u32,
    /// Pocet palettes v CPAL (jen pri COLR + CPAL).
    pub cpal_palette_count: u16,
}

/// Jeden COLR layer: glyph_id + palette index pro tint.
/// 0xFFFF palette_idx = "foreground color" (text color, ne palette).
#[derive(Debug, Clone, Copy)]
pub struct ColrLayer {
    pub glyph_id: u16,
    pub palette_idx: u16,
}

/// Plne parsed COLR + CPAL data pro rasterization.
#[derive(Debug, Clone)]
pub struct ColrData {
    /// Mapping base glyph_id -> Vec<ColrLayer>.
    pub base_to_layers: std::collections::HashMap<u16, Vec<ColrLayer>>,
    /// CPAL palette[0]: kazdy entry RGBA [u8; 4].
    pub palette: Vec<[u8; 4]>,
}

/// Plne parse COLR v0 + CPAL pro rasterization. Vrati None pri non-COLR fonts.
pub fn parse_colr_full(font_data: &[u8]) -> Option<ColrData> {
    let colr = find_table(font_data, b"COLR")?;
    if colr.len() < 14 { return None; }
    let version = u16::from_be_bytes([colr[0], colr[1]]);
    if version != 0 { return None; } // v1 ma jiny format
    let num_base = u16::from_be_bytes([colr[2], colr[3]]) as usize;
    let base_off = u32::from_be_bytes([colr[4], colr[5], colr[6], colr[7]]) as usize;
    let layer_off = u32::from_be_bytes([colr[8], colr[9], colr[10], colr[11]]) as usize;
    let num_layers = u16::from_be_bytes([colr[12], colr[13]]) as usize;
    if base_off + num_base * 6 > colr.len() { return None; }
    if layer_off + num_layers * 4 > colr.len() { return None; }

    // Parse layer records (each 4B: glyph_id u16 + palette_idx u16).
    let mut all_layers: Vec<ColrLayer> = Vec::with_capacity(num_layers);
    for i in 0..num_layers {
        let off = layer_off + i * 4;
        let gid = u16::from_be_bytes([colr[off], colr[off+1]]);
        let pidx = u16::from_be_bytes([colr[off+2], colr[off+3]]);
        all_layers.push(ColrLayer { glyph_id: gid, palette_idx: pidx });
    }

    // Parse base glyph records (each 6B: glyph_id u16, first_layer u16, num_layers u16).
    let mut base_to_layers: std::collections::HashMap<u16, Vec<ColrLayer>> =
        std::collections::HashMap::with_capacity(num_base);
    for i in 0..num_base {
        let off = base_off + i * 6;
        let gid = u16::from_be_bytes([colr[off], colr[off+1]]);
        let first = u16::from_be_bytes([colr[off+2], colr[off+3]]) as usize;
        let count = u16::from_be_bytes([colr[off+4], colr[off+5]]) as usize;
        if first + count > all_layers.len() { continue; }
        let layers: Vec<ColrLayer> = all_layers[first..first + count].to_vec();
        base_to_layers.insert(gid, layers);
    }

    // Parse CPAL.
    let cpal = find_table(font_data, b"CPAL")?;
    if cpal.len() < 12 { return None; }
    // Header (v0): version u16, numPaletteEntries u16, numPalettes u16,
    //              numColorRecords u16, colorRecordsArrayOffset u32, ...
    let num_palette_entries = u16::from_be_bytes([cpal[2], cpal[3]]) as usize;
    let _num_palettes = u16::from_be_bytes([cpal[4], cpal[5]]);
    let _num_color_records = u16::from_be_bytes([cpal[6], cpal[7]]);
    let color_records_off = u32::from_be_bytes([cpal[8], cpal[9], cpal[10], cpal[11]]) as usize;
    // Palette[0] starts at color_records_off, each color = BGRA u8[4].
    let mut palette: Vec<[u8; 4]> = Vec::with_capacity(num_palette_entries);
    for i in 0..num_palette_entries {
        let off = color_records_off + i * 4;
        if off + 4 > cpal.len() { break; }
        // Spec: BGRA order in CPAL.
        let b = cpal[off];
        let g = cpal[off + 1];
        let r = cpal[off + 2];
        let a = cpal[off + 3];
        palette.push([r, g, b, a]);
    }

    Some(ColrData { base_to_layers, palette })
}

pub fn detect_color_format(font_data: &[u8]) -> ColorFontInfo {
    let mut info = ColorFontInfo {
        format: ColorFormat::None,
        colr_base_count: 0,
        colr_layer_count: 0,
        cpal_palette_count: 0,
    };
    // sbix priority - Apple emoji
    if has_table(font_data, b"sbix") {
        info.format = ColorFormat::Sbix;
        return info;
    }
    if has_table(font_data, b"CBDT") && has_table(font_data, b"CBLC") {
        info.format = ColorFormat::Cbdt;
        return info;
    }
    if has_table(font_data, b"SVG ") {
        info.format = ColorFormat::Svg;
        return info;
    }
    if let Some(colr) = find_table(font_data, b"COLR") {
        if colr.len() >= 14 {
            let version = u16::from_be_bytes([colr[0], colr[1]]);
            let base_count = u16::from_be_bytes([colr[2], colr[3]]) as u32;
            // baseGlyphRecordsOffset(4)+layerRecordsOffset(4)+numLayerRecords(2)
            let layer_count = u16::from_be_bytes([colr[12], colr[13]]) as u32;
            info.colr_base_count = base_count;
            info.colr_layer_count = layer_count;
            info.format = if version == 0 { ColorFormat::ColrV0 } else { ColorFormat::ColrV1 };
            if let Some(cpal) = find_table(font_data, b"CPAL") {
                if cpal.len() >= 12 {
                    info.cpal_palette_count = u16::from_be_bytes([cpal[6], cpal[7]]);
                }
            }
        }
    }
    info
}

fn has_table(font: &[u8], tag: &[u8; 4]) -> bool {
    find_table(font, tag).is_some()
}

fn find_table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    if font.len() < 12 { return None; }
    let num_tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    if 12 + num_tables * 16 > font.len() { return None; }
    for i in 0..num_tables {
        let off = 12 + i * 16;
        if &font[off..off+4] == tag {
            let table_off = u32::from_be_bytes([font[off+8], font[off+9], font[off+10], font[off+11]]) as usize;
            let table_len = u32::from_be_bytes([font[off+12], font[off+13], font[off+14], font[off+15]]) as usize;
            if table_off + table_len <= font.len() {
                return Some(&font[table_off..table_off + table_len]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_font_with_table(tag: &[u8; 4], table_data: &[u8]) -> Vec<u8> {
        let mut ttf = Vec::new();
        ttf.extend_from_slice(&0x00010000u32.to_be_bytes());
        ttf.extend_from_slice(&1u16.to_be_bytes()); // numTables
        ttf.extend_from_slice(&16u16.to_be_bytes()); // searchRange
        ttf.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
        ttf.extend_from_slice(&0u16.to_be_bytes()); // rangeShift
        // table directory entry
        let table_offset = 12 + 16; // 28
        ttf.extend_from_slice(tag);
        ttf.extend_from_slice(&0u32.to_be_bytes()); // checksum
        ttf.extend_from_slice(&(table_offset as u32).to_be_bytes());
        ttf.extend_from_slice(&(table_data.len() as u32).to_be_bytes());
        ttf.extend_from_slice(table_data);
        ttf
    }

    #[test]
    fn detect_no_color_in_plain_font() {
        let data = make_font_with_table(b"name", &[0u8; 32]);
        let info = detect_color_format(&data);
        assert_eq!(info.format, ColorFormat::None);
    }

    #[test]
    fn detect_sbix_format() {
        let data = make_font_with_table(b"sbix", &[0u8; 16]);
        let info = detect_color_format(&data);
        assert_eq!(info.format, ColorFormat::Sbix);
    }

    #[test]
    fn parse_colr_full_returns_layers() {
        // Build minimal COLR v0 with 1 base glyph + 2 layers.
        let mut colr = Vec::new();
        colr.extend_from_slice(&0u16.to_be_bytes()); // version 0
        colr.extend_from_slice(&1u16.to_be_bytes()); // numBaseGlyphRecords = 1
        colr.extend_from_slice(&14u32.to_be_bytes()); // baseGlyphRecordsOffset = 14 (after header)
        colr.extend_from_slice(&20u32.to_be_bytes()); // layerRecordsOffset = 14 + 6 = 20
        colr.extend_from_slice(&2u16.to_be_bytes()); // numLayerRecords = 2
        // Base record (6B): glyph_id=100, firstLayer=0, numLayers=2
        colr.extend_from_slice(&100u16.to_be_bytes());
        colr.extend_from_slice(&0u16.to_be_bytes());
        colr.extend_from_slice(&2u16.to_be_bytes());
        // Layer 0: glyph_id=200, palette_idx=0
        colr.extend_from_slice(&200u16.to_be_bytes());
        colr.extend_from_slice(&0u16.to_be_bytes());
        // Layer 1: glyph_id=201, palette_idx=1
        colr.extend_from_slice(&201u16.to_be_bytes());
        colr.extend_from_slice(&1u16.to_be_bytes());

        // Build minimal CPAL v0 with 2 palette entries.
        let mut cpal = Vec::new();
        cpal.extend_from_slice(&0u16.to_be_bytes()); // version 0
        cpal.extend_from_slice(&2u16.to_be_bytes()); // numPaletteEntries = 2
        cpal.extend_from_slice(&1u16.to_be_bytes()); // numPalettes = 1
        cpal.extend_from_slice(&2u16.to_be_bytes()); // numColorRecords = 2
        cpal.extend_from_slice(&12u32.to_be_bytes()); // colorRecordsArrayOffset = 12
        // Color record 0: BGRA = blue
        cpal.extend_from_slice(&[0xFF, 0x00, 0x00, 0xFF]); // B G R A
        // Color record 1: BGRA = green
        cpal.extend_from_slice(&[0x00, 0xFF, 0x00, 0xFF]);

        // Build TTF s 2 tables (COLR + CPAL).
        let mut ttf = Vec::new();
        ttf.extend_from_slice(&0x00010000u32.to_be_bytes());
        ttf.extend_from_slice(&2u16.to_be_bytes()); // numTables = 2
        ttf.extend_from_slice(&32u16.to_be_bytes()); // searchRange
        ttf.extend_from_slice(&1u16.to_be_bytes()); // entrySelector
        ttf.extend_from_slice(&0u16.to_be_bytes()); // rangeShift
        // Sorted by tag: COLR < CPAL alphabetic.
        let colr_off = 12 + 32; // 12 hdr + 2 dir entries (32B)
        let cpal_off = colr_off + colr.len();
        // dir entry COLR
        ttf.extend_from_slice(b"COLR");
        ttf.extend_from_slice(&0u32.to_be_bytes()); // checksum
        ttf.extend_from_slice(&(colr_off as u32).to_be_bytes());
        ttf.extend_from_slice(&(colr.len() as u32).to_be_bytes());
        // dir entry CPAL
        ttf.extend_from_slice(b"CPAL");
        ttf.extend_from_slice(&0u32.to_be_bytes());
        ttf.extend_from_slice(&(cpal_off as u32).to_be_bytes());
        ttf.extend_from_slice(&(cpal.len() as u32).to_be_bytes());
        ttf.extend_from_slice(&colr);
        ttf.extend_from_slice(&cpal);

        let result = parse_colr_full(&ttf).expect("parse should succeed");
        assert_eq!(result.base_to_layers.len(), 1);
        let layers = result.base_to_layers.get(&100).unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].glyph_id, 200);
        assert_eq!(layers[0].palette_idx, 0);
        assert_eq!(layers[1].glyph_id, 201);
        assert_eq!(layers[1].palette_idx, 1);
        assert_eq!(result.palette.len(), 2);
        assert_eq!(result.palette[0], [0x00, 0x00, 0xFF, 0xFF]); // RGBA = blue
        assert_eq!(result.palette[1], [0x00, 0xFF, 0x00, 0xFF]); // RGBA = green
    }

    #[test]
    fn detect_colr_v0_with_counts() {
        // COLR v0 header (14B):
        // u16 version=0, u16 numBaseGlyphRecords=5,
        // u32 baseOffset, u32 layerOffset, u16 numLayerRecords=12
        let mut colr = Vec::new();
        colr.extend_from_slice(&0u16.to_be_bytes());
        colr.extend_from_slice(&5u16.to_be_bytes());
        colr.extend_from_slice(&14u32.to_be_bytes());
        colr.extend_from_slice(&44u32.to_be_bytes());
        colr.extend_from_slice(&12u16.to_be_bytes());
        let data = make_font_with_table(b"COLR", &colr);
        let info = detect_color_format(&data);
        assert_eq!(info.format, ColorFormat::ColrV0);
        assert_eq!(info.colr_base_count, 5);
        assert_eq!(info.colr_layer_count, 12);
    }
}
