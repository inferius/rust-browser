use super::*;

#[test]
fn parse_axes_empty_on_non_variable() {
    let data = vec![0u8; 200];
    let axes = parse_axes(&data);
    assert!(axes.is_empty());
}

/// Sestavi minimalisticky TTF s fvar tabulkou se 2 axis: wght + wdth.
fn make_minimal_variable_font() -> Vec<u8> {
    // fvar table data
    let mut fvar = Vec::new();
    // Header
    fvar.extend_from_slice(&1u16.to_be_bytes()); // major
    fvar.extend_from_slice(&0u16.to_be_bytes()); // minor
    fvar.extend_from_slice(&16u16.to_be_bytes()); // axesArrayOffset
    fvar.extend_from_slice(&2u16.to_be_bytes()); // reserved
    fvar.extend_from_slice(&2u16.to_be_bytes()); // axisCount
    fvar.extend_from_slice(&20u16.to_be_bytes()); // axisSize
    fvar.extend_from_slice(&0u16.to_be_bytes()); // instanceCount
    fvar.extend_from_slice(&0u16.to_be_bytes()); // instanceSize

    // Axis 1: wght 100..900, default 400
    fvar.extend_from_slice(b"wght");
    fvar.extend_from_slice(&((100i32 << 16) as u32).to_be_bytes()); // min
    fvar.extend_from_slice(&((400i32 << 16) as u32).to_be_bytes()); // default
    fvar.extend_from_slice(&((900i32 << 16) as u32).to_be_bytes()); // max
    fvar.extend_from_slice(&0u16.to_be_bytes()); // flags
    fvar.extend_from_slice(&256u16.to_be_bytes()); // axisNameID

    // Axis 2: wdth 75..125, default 100
    fvar.extend_from_slice(b"wdth");
    fvar.extend_from_slice(&((75i32 << 16) as u32).to_be_bytes());
    fvar.extend_from_slice(&((100i32 << 16) as u32).to_be_bytes());
    fvar.extend_from_slice(&((125i32 << 16) as u32).to_be_bytes());
    fvar.extend_from_slice(&0u16.to_be_bytes());
    fvar.extend_from_slice(&257u16.to_be_bytes());

    // TTF header + 1 table directory entry pro fvar
    let mut ttf = Vec::new();
    ttf.extend_from_slice(&0x00010000u32.to_be_bytes()); // version
    ttf.extend_from_slice(&1u16.to_be_bytes()); // numTables
    ttf.extend_from_slice(&16u16.to_be_bytes()); // searchRange
    ttf.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
    ttf.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

    // Table directory entry (16B): tag, checksum, offset, length
    let table_offset = 12 + 16; // = 28
    ttf.extend_from_slice(b"fvar");
    ttf.extend_from_slice(&0u32.to_be_bytes()); // checksum
    ttf.extend_from_slice(&(table_offset as u32).to_be_bytes());
    ttf.extend_from_slice(&(fvar.len() as u32).to_be_bytes());

    // Table data
    ttf.extend_from_slice(&fvar);
    ttf
}

#[test]
fn parse_axes_extracts_wght_and_wdth() {
    let font = make_minimal_variable_font();
    let axes = parse_axes(&font);
    assert_eq!(axes.len(), 2);
    assert_eq!(axes[0].tag, "wght");
    assert_eq!(axes[0].min_value, 100.0);
    assert_eq!(axes[0].default_value, 400.0);
    assert_eq!(axes[0].max_value, 900.0);
    assert_eq!(axes[1].tag, "wdth");
    assert_eq!(axes[1].min_value, 75.0);
    assert_eq!(axes[1].default_value, 100.0);
    assert_eq!(axes[1].max_value, 125.0);
}
