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
