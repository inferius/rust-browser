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
