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

/// COLR v0 rasterizer: pro base glyph_id najde jeho layers, rasterizuje
/// kazdy pres swash scaler indexed, tintuje palette barvou + composit
/// alpha-over do RGBA bufferu.
///
/// Vraci (width, height, x_offset, y_offset, RGBA bytes).
/// Pri base glyph nema layers v COLR vrati None - caller nech rasterize as alpha.
pub fn rasterize_color_glyph(
    face: &super::render::SwashFontFace,
    base_glyph_id: u16,
    px: f32,
    colr: &ColrData,
    foreground: [u8; 4],
) -> Option<(usize, usize, i32, i32, Vec<u8>)> {
    use swash::scale::{ScaleContext, Render, Source};
    use swash::zeno::Format;

    let layers = colr.base_to_layers.get(&base_glyph_id)?;
    if layers.is_empty() { return None; }

    let font_ref = face.as_ref();
    let mut ctx = ScaleContext::new();
    let mut scaler = ctx.builder(font_ref)
        .size(px)
        .hint(true)
        .build();

    // Compute bounding box across all layers (max width/height).
    // Swash placement: left = xmin, top = ymax (baseline-relative top, positive up).
    // V fontdue m.ymin = baseline-relative bottom (positive up). Pri swash:
    // bottom = top - height. Pro porovnani s fontdue semantikou used puvodne
    // (ymin/height) prepocet: ymin = top - height.
    let mut max_w = 0i32;
    let mut max_h = 0i32;
    let mut min_xmin = i32::MAX;
    let mut min_ymin = i32::MAX;
    struct LayerImage {
        xmin: i32,
        ymin: i32,
        width: usize,
        height: usize,
        alpha: Vec<u8>,
        color: [u8; 4],
    }
    let mut layer_data: Vec<LayerImage> = Vec::with_capacity(layers.len());
    for layer in layers {
        let image = match Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .render(&mut scaler, layer.glyph_id)
        {
            Some(i) => i,
            None => continue,
        };
        let color = if layer.palette_idx == 0xFFFF {
            foreground
        } else {
            colr.palette.get(layer.palette_idx as usize).copied().unwrap_or([0, 0, 0, 255])
        };
        let w = image.placement.width as usize;
        let h = image.placement.height as usize;
        let xmin = image.placement.left;
        let ymin = image.placement.top - image.placement.height as i32;
        // Bbox track v "advance space" - xmin/ymin negative-ok.
        if xmin < min_xmin { min_xmin = xmin; }
        if ymin < min_ymin { min_ymin = ymin; }
        let right = xmin + w as i32;
        let top = ymin + h as i32;
        if right > max_w { max_w = right; }
        if top > max_h { max_h = top; }
        layer_data.push(LayerImage { xmin, ymin, width: w, height: h, alpha: image.data, color });
    }
    if min_xmin == i32::MAX { return None; }

    let out_w = (max_w - min_xmin).max(1) as usize;
    let out_h = (max_h - min_ymin).max(1) as usize;
    let mut rgba = vec![0u8; out_w * out_h * 4];

    // Composite each layer alpha-over.
    for li in &layer_data {
        let dx = (li.xmin - min_xmin) as usize;
        let dy_top = (max_h - (li.ymin + li.height as i32)) as usize;
        for ly in 0..li.height {
            for lx in 0..li.width {
                let a_idx = ly * li.width + lx;
                let layer_a = li.alpha[a_idx];
                if layer_a == 0 { continue; }
                let ox = dx + lx;
                let oy = dy_top + ly;
                if ox >= out_w || oy >= out_h { continue; }
                let dst = (oy * out_w + ox) * 4;
                // Source RGBA premultiplied by layer_a.
                let s_a = (layer_a as u16 * li.color[3] as u16 / 255) as u8;
                let s_r = (li.color[0] as u16 * s_a as u16 / 255) as u8;
                let s_g = (li.color[1] as u16 * s_a as u16 / 255) as u8;
                let s_b = (li.color[2] as u16 * s_a as u16 / 255) as u8;
                // dst already premultiplied (pri 0 init OK).
                let d_r = rgba[dst];
                let d_g = rgba[dst + 1];
                let d_b = rgba[dst + 2];
                let d_a = rgba[dst + 3];
                let inv_s_a = 255 - s_a as u16;
                rgba[dst]     = (s_r as u16 + d_r as u16 * inv_s_a / 255) as u8;
                rgba[dst + 1] = (s_g as u16 + d_g as u16 * inv_s_a / 255) as u8;
                rgba[dst + 2] = (s_b as u16 + d_b as u16 * inv_s_a / 255) as u8;
                rgba[dst + 3] = (s_a as u16 + d_a as u16 * inv_s_a / 255) as u8;
            }
        }
    }
    // Vrat (width, height, x_offset, y_offset, rgba).
    Some((out_w, out_h, min_xmin, min_ymin, rgba))
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
#[path = "tests/emoji_fonts_tests.rs"]
mod tests;
