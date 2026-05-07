//! Variable font parser - parsuje fvar tabulku z TTF/OTF.
//!
//! fvar (font variations) layout:
//! - VariationAxisRecord (20B kazdy): tag, minValue, defaultValue, maxValue, flags, axisNameID
//! - InstanceRecord variabilni: subfamilyNameID, flags, [coordinates: numAxes * 4B], [postScriptNameID]
//!
//! Pri praci s variable fontem si CSS muze nastavit font-variation-settings
//! (e.g. "wght 400, wdth 100"). Real glyph outline interpolation pres gvar
//! (TrueType) nebo CFF2 (OpenType) je velky kus prace - zatim parsujeme jen
//! axis metadata.

#[derive(Debug, Clone)]
pub struct VariableAxis {
    /// Tag jako 4 ASCII znaky, npr "wght", "wdth", "ital", "slnt", "opsz".
    pub tag: String,
    pub min_value: f32,
    pub default_value: f32,
    pub max_value: f32,
}

/// Parsuje fvar tabulku z font bytes. Vrati Vec<VariableAxis> nebo prazdny pri non-variable.
pub fn parse_axes(font_data: &[u8]) -> Vec<VariableAxis> {
    let table_data = match find_table(font_data, b"fvar") {
        Some(d) => d,
        None => return Vec::new(),
    };
    if table_data.len() < 16 { return Vec::new(); }

    // fvar header (16B):
    // uint16 majorVersion, uint16 minorVersion
    // uint16 axesArrayOffset (relative to table start)
    // uint16 reserved
    // uint16 axisCount
    // uint16 axisSize (= 20)
    // uint16 instanceCount
    // uint16 instanceSize
    let _major = read_u16(table_data, 0);
    let _minor = read_u16(table_data, 2);
    let axes_offset = read_u16(table_data, 4) as usize;
    let axis_count = read_u16(table_data, 8) as usize;
    let axis_size = read_u16(table_data, 10) as usize;
    if axis_size != 20 { return Vec::new(); }
    if axes_offset + axis_count * 20 > table_data.len() { return Vec::new(); }

    let mut axes = Vec::with_capacity(axis_count);
    for i in 0..axis_count {
        let off = axes_offset + i * axis_size;
        let tag_bytes = [table_data[off], table_data[off+1], table_data[off+2], table_data[off+3]];
        let tag = String::from_utf8_lossy(&tag_bytes).to_string();
        let min_value = read_f2dot14_or_fixed(table_data, off + 4);
        let default_value = read_f2dot14_or_fixed(table_data, off + 8);
        let max_value = read_f2dot14_or_fixed(table_data, off + 12);
        axes.push(VariableAxis { tag, min_value, default_value, max_value });
    }
    axes
}

/// Pomocna: najdi TTF table podle tagu.
fn find_table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    if font.len() < 12 { return None; }
    let num_tables = read_u16(font, 4) as usize;
    if 12 + num_tables * 16 > font.len() { return None; }
    for i in 0..num_tables {
        let off = 12 + i * 16;
        if &font[off..off+4] == tag {
            let table_off = read_u32(font, off + 8) as usize;
            let table_len = read_u32(font, off + 12) as usize;
            if table_off + table_len <= font.len() {
                return Some(&font[table_off..table_off + table_len]);
            }
        }
    }
    None
}

fn read_u16(data: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([data[off], data[off+1]])
}
fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([data[off], data[off+1], data[off+2], data[off+3]])
}
/// fvar pouziva Fixed (16.16) format pro min/default/max values.
fn read_f2dot14_or_fixed(data: &[u8], off: usize) -> f32 {
    if off + 4 > data.len() { return 0.0; }
    let raw = i32::from_be_bytes([data[off], data[off+1], data[off+2], data[off+3]]);
    raw as f32 / 65536.0
}

#[cfg(test)]
mod tests {
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
}
