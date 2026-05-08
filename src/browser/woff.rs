//! WOFF 1.0 dekompresor - prevod WOFF souboru na TTF/OTF bytes.
//!
//! WOFF format:
//! - Header (44B): signature, flavor, length, numTables, ...
//! - Table directory (numTables * 20B): tag, offset, compLength, origLength, origChecksum
//! - Table data: zlib komprimovane (compLength != origLength) nebo raw (==).
//!
//! Output je standardni sfnt:
//! - Offset table (12B): version, numTables, searchRange/entrySelector/rangeShift
//! - Table directory (numTables * 16B): tag, checksum, offset, length
//! - Table data, zarovnane na 4 byty.
//!
//! WOFF2 vyzaduje brotli + glyf transform - zatim TODO.

use std::io::Read;
use flate2::read::ZlibDecoder;

/// Vrati TTF/OTF bytes z WOFF (or vrati input pri non-WOFF).
pub fn maybe_decode_woff(data: &[u8]) -> Vec<u8> {
    if data.len() >= 4 && &data[0..4] == b"wOFF" {
        match decode_woff1(data) {
            Ok(out) => return out,
            Err(_) => return data.to_vec(),
        }
    }
    if data.len() >= 4 && &data[0..4] == b"wOF2" {
        match decode_woff2(data) {
            Ok(out) => return out,
            Err(e) => {
                eprintln!("[woff2] decode failed: {:?} - font wont load", e);
                return data.to_vec();
            }
        }
    }
    data.to_vec()
}

fn read_u32(data: &[u8], off: usize) -> Option<u32> {
    if off + 4 > data.len() { return None; }
    Some(u32::from_be_bytes([data[off], data[off+1], data[off+2], data[off+3]]))
}
fn read_u16(data: &[u8], off: usize) -> Option<u16> {
    if off + 2 > data.len() { return None; }
    Some(u16::from_be_bytes([data[off], data[off+1]]))
}

#[derive(Debug)]
pub enum WoffError {
    BadSignature,
    BadHeader,
    Decompress,
    OutOfBounds,
    /// WOFF2 vyzaduje glyf transform reversal pro non-default tables.
    /// Aktualne implementovan jen brotli dekomprese - tabulky jsou raw.
    TransformNotImplemented,
}

pub fn decode_woff1(data: &[u8]) -> Result<Vec<u8>, WoffError> {
    if data.len() < 44 || &data[0..4] != b"wOFF" {
        return Err(WoffError::BadSignature);
    }
    let flavor = read_u32(data, 4).ok_or(WoffError::BadHeader)?;
    let _length = read_u32(data, 8).ok_or(WoffError::BadHeader)?;
    let num_tables = read_u16(data, 12).ok_or(WoffError::BadHeader)? as usize;
    // 14: reserved, 16: totalSfntSize, ...

    if num_tables == 0 || num_tables > 1024 {
        return Err(WoffError::BadHeader);
    }
    // Parse table directory entries.
    let mut entries: Vec<(u32, u32, u32, u32, u32)> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let off = 44 + i * 20;
        if off + 20 > data.len() { return Err(WoffError::BadHeader); }
        let tag = read_u32(data, off).ok_or(WoffError::BadHeader)?;
        let table_off = read_u32(data, off + 4).ok_or(WoffError::BadHeader)?;
        let comp_len = read_u32(data, off + 8).ok_or(WoffError::BadHeader)?;
        let orig_len = read_u32(data, off + 12).ok_or(WoffError::BadHeader)?;
        let checksum = read_u32(data, off + 16).ok_or(WoffError::BadHeader)?;
        entries.push((tag, table_off, comp_len, orig_len, checksum));
    }

    // Output sfnt: header + table directory + table data (4-byte aligned).
    let largest_pow2 = {
        let mut p = 1;
        while p * 2 <= num_tables { p *= 2; }
        p
    };
    let entry_selector = (largest_pow2 as f32).log2() as u16;
    let search_range = (largest_pow2 * 16) as u16;
    let range_shift = (num_tables * 16 - search_range as usize) as u16;

    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // Reservuj prostor pro table directory.
    let dir_start = out.len();
    out.resize(dir_start + num_tables * 16, 0);

    // Decompress kazdou tabulku + emit do out, padding 4 bytes.
    let mut current_offset = out.len(); // od konce dir
    let mut directory: Vec<(u32, u32, u32, u32)> = Vec::new(); // tag, checksum, offset, length

    for (tag, table_off, comp_len, orig_len, checksum) in &entries {
        if *table_off as usize + *comp_len as usize > data.len() {
            return Err(WoffError::OutOfBounds);
        }
        let raw = &data[*table_off as usize..*table_off as usize + *comp_len as usize];
        let decompressed: Vec<u8> = if *comp_len < *orig_len {
            let mut z = ZlibDecoder::new(raw);
            let mut buf = Vec::with_capacity(*orig_len as usize);
            z.read_to_end(&mut buf).map_err(|_| WoffError::Decompress)?;
            if buf.len() != *orig_len as usize {
                return Err(WoffError::Decompress);
            }
            buf
        } else {
            raw.to_vec()
        };
        let table_offset = current_offset;
        out.extend_from_slice(&decompressed);
        let len = decompressed.len();
        // Padding na 4 byte boundary.
        let pad = (4 - (len % 4)) % 4;
        for _ in 0..pad { out.push(0); }
        current_offset += len + pad;
        directory.push((*tag, *checksum, table_offset as u32, len as u32));
    }

    // Sort directory by tag (sfnt requirement).
    directory.sort_by_key(|(tag, _, _, _)| *tag);

    // Vlozit directory.
    for (i, (tag, checksum, offset, length)) in directory.iter().enumerate() {
        let dir_off = dir_start + i * 16;
        out[dir_off..dir_off + 4].copy_from_slice(&tag.to_be_bytes());
        out[dir_off + 4..dir_off + 8].copy_from_slice(&checksum.to_be_bytes());
        out[dir_off + 8..dir_off + 12].copy_from_slice(&offset.to_be_bytes());
        out[dir_off + 12..dir_off + 16].copy_from_slice(&length.to_be_bytes());
    }

    Ok(out)
}

/// WOFF2 dekompresor - parsuje header, table directory s preset tags +
/// 255UInt16 lengths, decompressuje brotli stream, splaci tabulky do sfnt.
///
/// LIMITS: glyf table transform (form 1) NEN reversed - pri vstupu s glyf
/// transform vrati TransformNotImplemented. To je vetsina realnych WOFF2
/// fontu.
pub fn decode_woff2(data: &[u8]) -> Result<Vec<u8>, WoffError> {
    if data.len() < 48 || &data[0..4] != b"wOF2" {
        return Err(WoffError::BadSignature);
    }
    let flavor = read_u32(data, 4).ok_or(WoffError::BadHeader)?;
    let _length = read_u32(data, 8).ok_or(WoffError::BadHeader)?;
    let num_tables = read_u16(data, 12).ok_or(WoffError::BadHeader)? as usize;
    // 14: reserved
    let _total_sfnt_size = read_u32(data, 16).ok_or(WoffError::BadHeader)?;
    let total_compressed_size = read_u32(data, 20).ok_or(WoffError::BadHeader)? as usize;
    // 24-25: majorVersion, 26-27: minorVersion
    // 28..47: meta + priv (5 * uint32)

    if num_tables == 0 || num_tables > 1024 {
        return Err(WoffError::BadHeader);
    }

    // Preset table tags per WOFF2 spec.
    const PRESET_TAGS: [&[u8; 4]; 63] = [
        b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post",
        b"cvt ", b"fpgm", b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT",
        b"EBLC", b"gasp", b"hdmx", b"kern", b"LTSH", b"PCLT", b"VDMX", b"vhea",
        b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"EBSC", b"JSTF", b"MATH",
        b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt", b"avar",
        b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar",
        b"gvar", b"hsty", b"just", b"lcar", b"mort", b"morx", b"opbd", b"prop",
        b"trak", b"Zapf", b"Silf", b"Glat", b"Gloc", b"Feat", b"Sill",
    ];

    // Parse table directory s variable-length encoding.
    let mut pos = 48usize;
    let mut entries: Vec<(u32, u32, u32, u8)> = Vec::with_capacity(num_tables); // (tag, orig_len, transform_len, transform_ver)
    for _ in 0..num_tables {
        if pos >= data.len() { return Err(WoffError::BadHeader); }
        let flags = data[pos];
        pos += 1;
        let tag_idx = (flags & 0x3F) as usize;
        let transform_ver = (flags >> 6) & 0x03;
        let tag = if tag_idx == 63 {
            // Custom tag: 4 bytes follow.
            if pos + 4 > data.len() { return Err(WoffError::BadHeader); }
            let t = read_u32(data, pos).ok_or(WoffError::BadHeader)?;
            pos += 4;
            t
        } else {
            let bytes = PRESET_TAGS[tag_idx];
            u32::from_be_bytes(*bytes)
        };
        let (orig_len, p) = read_uint_base128(data, pos)?;
        pos = p;
        // Transform length only present for glyf/loca with transform_ver == 0
        // (default transform applied) OR explicit transform.
        let tag_bytes = tag.to_be_bytes();
        let is_glyf_or_loca = &tag_bytes == b"glyf" || &tag_bytes == b"loca";
        let has_transform = if is_glyf_or_loca {
            transform_ver == 0
        } else {
            transform_ver != 0
        };
        let transform_len = if has_transform {
            let (l, p2) = read_uint_base128(data, pos)?;
            pos = p2;
            l
        } else {
            orig_len
        };
        entries.push((tag, orig_len, transform_len, transform_ver));
    }

    // Compressed data start = pos. Brotli stream of total_compressed_size bytes.
    let comp_end = pos + total_compressed_size;
    if comp_end > data.len() { return Err(WoffError::OutOfBounds); }
    let compressed = &data[pos..comp_end];
    let mut decompressed: Vec<u8> = Vec::new();
    {
        use std::io::Read;
        let mut reader = brotli::Decompressor::new(compressed, 65536);
        reader.read_to_end(&mut decompressed).map_err(|_| WoffError::Decompress)?;
    }

    // Split decompressed do per-entry slices podle transform_len.
    let mut entry_data: Vec<Vec<u8>> = Vec::with_capacity(num_tables);
    let mut data_offset = 0usize;
    for (_tag, _orig_len, transform_len, _) in &entries {
        let len = *transform_len as usize;
        if data_offset + len > decompressed.len() { return Err(WoffError::OutOfBounds); }
        entry_data.push(decompressed[data_offset..data_offset + len].to_vec());
        data_offset += len;
    }

    // Detekuj glyf transform (transform_ver == 0). Pri presence:
    // 1. Reverse glyf transform -> (glyf_bytes, loca_bytes)
    // 2. Substitute glyf + loca v output (loca's transform_len je 0)
    let mut synthesized_glyf: Option<Vec<u8>> = None;
    let mut synthesized_loca: Option<Vec<u8>> = None;
    for (i, (tag, _, _, transform_ver)) in entries.iter().enumerate() {
        let tag_bytes = tag.to_be_bytes();
        if &tag_bytes == b"glyf" && *transform_ver == 0 {
            let (gf, lo) = reverse_glyf_transform(&entry_data[i])?;
            synthesized_glyf = Some(gf);
            synthesized_loca = Some(lo);
        }
    }

    // Vystup sfnt: header + table directory + table data (4-byte aligned).
    let largest_pow2 = {
        let mut p = 1; while p * 2 <= num_tables { p *= 2; } p
    };
    let entry_selector = (largest_pow2 as f32).log2() as u16;
    let search_range = (largest_pow2 * 16) as u16;
    let range_shift = (num_tables * 16 - search_range as usize) as u16;

    let mut out = Vec::with_capacity(decompressed.len() + 12 + num_tables * 16);
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    let dir_start = out.len();
    out.resize(dir_start + num_tables * 16, 0);

    let mut directory: Vec<(u32, u32, u32, u32)> = Vec::new();
    for (i, (tag, orig_len, _transform_len, _)) in entries.iter().enumerate() {
        let tag_bytes = tag.to_be_bytes();
        let table_data: &[u8] = if &tag_bytes == b"glyf" && synthesized_glyf.is_some() {
            synthesized_glyf.as_ref().unwrap()
        } else if &tag_bytes == b"loca" && synthesized_loca.is_some() {
            synthesized_loca.as_ref().unwrap()
        } else {
            &entry_data[i]
        };
        let len = table_data.len();
        let table_offset = out.len();
        out.extend_from_slice(table_data);
        let pad = (4 - (len % 4)) % 4;
        for _ in 0..pad { out.push(0); }
        // Pri glyf/loca transform: actual length z reversed bytes (ne orig_len).
        // Pri jinych: orig_len je validni.
        let dir_len = if (&tag_bytes == b"glyf" && synthesized_glyf.is_some())
                      || (&tag_bytes == b"loca" && synthesized_loca.is_some()) {
            len as u32
        } else {
            *orig_len
        };
        directory.push((*tag, 0u32, table_offset as u32, dir_len));
    }
    directory.sort_by_key(|(tag, _, _, _)| *tag);
    for (i, (tag, checksum, offset, length)) in directory.iter().enumerate() {
        let dir_off = dir_start + i * 16;
        out[dir_off..dir_off + 4].copy_from_slice(&tag.to_be_bytes());
        out[dir_off + 4..dir_off + 8].copy_from_slice(&checksum.to_be_bytes());
        out[dir_off + 8..dir_off + 12].copy_from_slice(&offset.to_be_bytes());
        out[dir_off + 12..dir_off + 16].copy_from_slice(&length.to_be_bytes());
    }
    Ok(out)
}

// ─── WOFF2 glyf transform reversal ──────────────────────────────────────
//
// Implementace dle Google's woff2 reference (Apache-2.0). Spec section 5.1.
//
// Layout transformovaneho glyf streamu:
//   - 36B header: reserved(u16), flags(u16), numGlyphs(u16), indexFormat(u16),
//     7x stream sizes (u32) - nContour, nPoints, flag, glyph, composite,
//     bbox, instruction
//   - 7 streams concatenated (sizes from header)
//   - Optional overlap_simple_bitmap (numGlyphs+7)>>3 bytes pri flags bit 0
//
// Per-glyph:
//   nContour == 0xFFFF (-1): composite - copy az do !MORE_COMPONENTS
//   nContour == 0:           empty
//   nContour > 0:            simple - read nPoints, flags, triplet decode
//
// Output: standardni sfnt glyf + loca (synthesized z glyf offsets).

#[derive(Debug, Clone, Copy)]
struct Point { x: i32, y: i32, on_curve: bool }

/// Decoder s tracking offset.
struct Stream<'a> { data: &'a [u8], pos: usize }
impl<'a> Stream<'a> {
    fn new(data: &'a [u8]) -> Self { Stream { data, pos: 0 } }
    fn remaining(&self) -> usize { self.data.len() - self.pos }
    fn read_u8(&mut self) -> Result<u8, WoffError> {
        if self.pos >= self.data.len() { return Err(WoffError::OutOfBounds); }
        let v = self.data[self.pos]; self.pos += 1; Ok(v)
    }
    fn read_u16(&mut self) -> Result<u16, WoffError> {
        if self.pos + 2 > self.data.len() { return Err(WoffError::OutOfBounds); }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos+1]]);
        self.pos += 2; Ok(v)
    }
    fn read_u32(&mut self) -> Result<u32, WoffError> {
        if self.pos + 4 > self.data.len() { return Err(WoffError::OutOfBounds); }
        let v = u32::from_be_bytes([self.data[self.pos], self.data[self.pos+1],
            self.data[self.pos+2], self.data[self.pos+3]]);
        self.pos += 4; Ok(v)
    }
    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], WoffError> {
        if self.pos + n > self.data.len() { return Err(WoffError::OutOfBounds); }
        let s = &self.data[self.pos..self.pos+n]; self.pos += n; Ok(s)
    }
    fn skip(&mut self, n: usize) -> Result<(), WoffError> {
        if self.pos + n > self.data.len() { return Err(WoffError::OutOfBounds); }
        self.pos += n; Ok(())
    }
    fn read_255_ushort(&mut self) -> Result<u32, WoffError> {
        let (v, new_pos) = read_255_uint16(self.data, self.pos)?;
        self.pos = new_pos;
        Ok(v)
    }
}

/// WithSign per WOFF2 spec - flag's bit 0 controls sign:
/// flag&1 -> +baseval, else -baseval.
fn with_sign(flag: u8, baseval: i32) -> i32 {
    if (flag & 1) != 0 { baseval } else { -baseval }
}

/// Triplet decode per WOFF2 spec section 5.2.
/// Vrati Vec<Point> + bytes consumed z glyph stream.
fn triplet_decode(flags: &[u8], data: &[u8], n_points: usize)
    -> Result<(Vec<Point>, usize), WoffError>
{
    let mut points = Vec::with_capacity(n_points);
    let mut x: i32 = 0;
    let mut y: i32 = 0;
    let mut idx = 0usize;
    if n_points > flags.len() { return Err(WoffError::OutOfBounds); }
    for i in 0..n_points {
        let flag = flags[i];
        let on_curve = (flag >> 7) == 0;
        let f = flag & 0x7f;
        let n_data: usize = if f < 84 { 1 } else if f < 120 { 2 } else if f < 124 { 3 } else { 4 };
        if idx + n_data > data.len() { return Err(WoffError::OutOfBounds); }
        let (dx, dy);
        if f < 10 {
            dx = 0;
            dy = with_sign(f, (((f & 14) as i32) << 7) + data[idx] as i32);
        } else if f < 20 {
            dx = with_sign(f, ((((f - 10) & 14) as i32) << 7) + data[idx] as i32);
            dy = 0;
        } else if f < 84 {
            let b0 = (f - 20) as i32;
            let b1 = data[idx] as i32;
            dx = with_sign(f, 1 + (b0 & 0x30) + (b1 >> 4));
            dy = with_sign(f >> 1, 1 + ((b0 & 0x0c) << 2) + (b1 & 0x0f));
        } else if f < 120 {
            let b0 = (f - 84) as i32;
            dx = with_sign(f, 1 + ((b0 / 12) << 8) + data[idx] as i32);
            dy = with_sign(f >> 1, 1 + (((b0 % 12) >> 2) << 8) + data[idx+1] as i32);
        } else if f < 124 {
            let b2 = data[idx+1] as i32;
            dx = with_sign(f, ((data[idx] as i32) << 4) + (b2 >> 4));
            dy = with_sign(f >> 1, ((b2 & 0x0f) << 8) + data[idx+2] as i32);
        } else {
            dx = with_sign(f, ((data[idx] as i32) << 8) + data[idx+1] as i32);
            dy = with_sign(f >> 1, ((data[idx+2] as i32) << 8) + data[idx+3] as i32);
        }
        idx += n_data;
        x = x.wrapping_add(dx);
        y = y.wrapping_add(dy);
        points.push(Point { x, y, on_curve });
    }
    Ok((points, idx))
}

/// Spocita size composite glyph dat (do !MORE_COMPONENTS) + zda WE_HAVE_INSTRUCTIONS.
/// Vrati (size_in_bytes, have_instructions). Stream pointer NEadvanced.
fn size_of_composite(stream: &Stream) -> Result<(usize, bool), WoffError> {
    let mut s = Stream::new(&stream.data[stream.pos..]);
    let start = s.pos;
    let mut have_instr = false;
    loop {
        let flags = s.read_u16()? as u32;
        have_instr |= (flags & 0x0100) != 0;
        // Skip glyph index (u16) + args (2 nebo 4 bytes) + scale (0/2/4/8 B).
        let mut arg_size: usize = 2; // glyphIndex
        if (flags & 0x0001) != 0 { arg_size += 4; } else { arg_size += 2; }
        if (flags & 0x0008) != 0 { arg_size += 2; }
        else if (flags & 0x0040) != 0 { arg_size += 4; }
        else if (flags & 0x0080) != 0 { arg_size += 8; }
        s.skip(arg_size)?;
        if (flags & 0x0020) == 0 { break; } // !MORE_COMPONENTS
    }
    Ok((s.pos - start, have_instr))
}

/// Output sfnt simple glyph - write flags + x_coords + y_coords s repeat
/// optimization. Returns final glyph_size (offset po posledni napsane y coord).
fn store_points(
    points: &[Point],
    n_contours: u16,
    instruction_size: u16,
    has_overlap_bit: bool,
    dst: &mut Vec<u8>,
    flag_offset: usize,
) -> Result<usize, WoffError> {
    const FLAG_ON_CURVE: u8 = 0x01;
    const FLAG_X_SHORT: u8 = 0x02;
    const FLAG_Y_SHORT: u8 = 0x04;
    const FLAG_REPEAT: u8 = 0x08;
    const FLAG_X_SAME: u8 = 0x10;
    const FLAG_Y_SAME: u8 = 0x20;
    const FLAG_OVERLAP: u8 = 0x40;
    let _ = n_contours; let _ = instruction_size;

    let mut last_x = 0i32;
    let mut last_y = 0i32;
    let mut last_flag: i32 = -1;
    let mut repeat_count: u32 = 0;
    let mut x_bytes: usize = 0;
    let mut y_bytes: usize = 0;
    let mut flag_off = flag_offset;
    // Zajistit dost mista pro flags (max n_points + n_points/2 repeat counts).
    if dst.len() < flag_off { dst.resize(flag_off, 0); }

    for (i, p) in points.iter().enumerate() {
        let mut flag: u8 = if p.on_curve { FLAG_ON_CURVE } else { 0 };
        if has_overlap_bit && i == 0 { flag |= FLAG_OVERLAP; }
        let dx = p.x - last_x;
        let dy = p.y - last_y;
        if dx == 0 {
            flag |= FLAG_X_SAME;
        } else if dx > -256 && dx < 256 {
            flag |= FLAG_X_SHORT | (if dx > 0 { FLAG_X_SAME } else { 0 });
            x_bytes += 1;
        } else {
            x_bytes += 2;
        }
        if dy == 0 {
            flag |= FLAG_Y_SAME;
        } else if dy > -256 && dy < 256 {
            flag |= FLAG_Y_SHORT | (if dy > 0 { FLAG_Y_SAME } else { 0 });
            y_bytes += 1;
        } else {
            y_bytes += 2;
        }
        let flag_i = flag as i32;
        if flag_i == last_flag && repeat_count != 255 {
            // OR REPEAT bit na predchozi flag byte
            if flag_off == 0 { return Err(WoffError::OutOfBounds); }
            dst[flag_off - 1] |= FLAG_REPEAT;
            repeat_count += 1;
        } else {
            if repeat_count != 0 {
                if dst.len() <= flag_off { dst.push(0); }
                dst[flag_off] = repeat_count as u8;
                flag_off += 1;
            }
            if dst.len() <= flag_off { dst.push(0); }
            dst[flag_off] = flag;
            flag_off += 1;
            repeat_count = 0;
        }
        last_x = p.x;
        last_y = p.y;
        last_flag = flag_i;
    }
    if repeat_count != 0 {
        if dst.len() <= flag_off { dst.push(0); }
        dst[flag_off] = repeat_count as u8;
        flag_off += 1;
    }

    // Resize dst pro x + y coords.
    let coord_start = flag_off;
    let total = coord_start + x_bytes + y_bytes;
    if dst.len() < total { dst.resize(total, 0); }

    let mut x_off = coord_start;
    let mut y_off = coord_start + x_bytes;
    last_x = 0;
    last_y = 0;
    for p in points.iter() {
        let dx = p.x - last_x;
        if dx == 0 {
            // skip
        } else if dx > -256 && dx < 256 {
            dst[x_off] = dx.unsigned_abs() as u8;
            x_off += 1;
        } else {
            let v = dx as i16;
            dst[x_off..x_off+2].copy_from_slice(&v.to_be_bytes());
            x_off += 2;
        }
        last_x += dx;
        let dy = p.y - last_y;
        if dy == 0 {
            // skip
        } else if dy > -256 && dy < 256 {
            dst[y_off] = dy.unsigned_abs() as u8;
            y_off += 1;
        } else {
            let v = dy as i16;
            dst[y_off..y_off+2].copy_from_slice(&v.to_be_bytes());
            y_off += 2;
        }
        last_y += dy;
    }
    Ok(y_off)
}

/// Spocita bbox z bodu (xMin, yMin, xMax, yMax). Vrati 8 bytes BE int16.
fn compute_bbox(points: &[Point]) -> [u8; 8] {
    if points.is_empty() {
        return [0; 8];
    }
    let mut x_min = points[0].x;
    let mut y_min = points[0].y;
    let mut x_max = points[0].x;
    let mut y_max = points[0].y;
    for p in &points[1..] {
        if p.x < x_min { x_min = p.x; }
        if p.x > x_max { x_max = p.x; }
        if p.y < y_min { y_min = p.y; }
        if p.y > y_max { y_max = p.y; }
    }
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&(x_min as i16).to_be_bytes());
    out[2..4].copy_from_slice(&(y_min as i16).to_be_bytes());
    out[4..6].copy_from_slice(&(x_max as i16).to_be_bytes());
    out[6..8].copy_from_slice(&(y_max as i16).to_be_bytes());
    out
}

/// Reverse WOFF2 glyf transform.
/// Returns (glyf_sfnt_bytes, loca_sfnt_bytes).
fn reverse_glyf_transform(data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), WoffError> {
    if data.len() < 36 { return Err(WoffError::BadHeader); }
    let mut hdr = Stream::new(data);
    let _reserved = hdr.read_u16()?;
    let flags = hdr.read_u16()?;
    let has_overlap_bitmap = (flags & 0x0001) != 0;
    let num_glyphs = hdr.read_u16()? as usize;
    let index_format = hdr.read_u16()?;
    let n_contour_size = hdr.read_u32()? as usize;
    let n_points_size = hdr.read_u32()? as usize;
    let flag_size = hdr.read_u32()? as usize;
    let glyph_size = hdr.read_u32()? as usize;
    let composite_size = hdr.read_u32()? as usize;
    let bbox_size = hdr.read_u32()? as usize;
    let instr_size = hdr.read_u32()? as usize;

    // Stream offsets za 36B header.
    let mut off = 36usize;
    let total_subs = n_contour_size + n_points_size + flag_size + glyph_size
        + composite_size + bbox_size + instr_size;
    if off + total_subs > data.len() { return Err(WoffError::OutOfBounds); }

    let n_contour_data = &data[off..off + n_contour_size]; off += n_contour_size;
    let n_points_data = &data[off..off + n_points_size]; off += n_points_size;
    let flag_data = &data[off..off + flag_size]; off += flag_size;
    let glyph_data = &data[off..off + glyph_size]; off += glyph_size;
    let composite_data = &data[off..off + composite_size]; off += composite_size;
    let bbox_data = &data[off..off + bbox_size]; off += bbox_size;
    let instr_data = &data[off..off + instr_size]; off += instr_size;

    let overlap_bitmap: &[u8] = if has_overlap_bitmap {
        let n = (num_glyphs + 7) >> 3;
        if off + n > data.len() { return Err(WoffError::OutOfBounds); }
        &data[off..off + n]
    } else {
        &[]
    };

    // bbox stream: bbox bitmap (((numGlyphs+31)>>5)<<2 bytes, 32-bit aligned),
    // pak bbox arrays per glyph s bit set.
    let bitmap_len = ((num_glyphs + 31) >> 5) << 2;
    if bbox_data.len() < bitmap_len { return Err(WoffError::OutOfBounds); }
    let bbox_bitmap = &bbox_data[..bitmap_len];
    let mut bbox_stream = Stream::new(&bbox_data[bitmap_len..]);

    let mut n_contour_stream = Stream::new(n_contour_data);
    let mut n_points_stream = Stream::new(n_points_data);
    let mut flag_stream_pos: usize = 0;
    let mut glyph_stream = Stream::new(glyph_data);
    let mut composite_stream = Stream::new(composite_data);
    let mut instr_stream = Stream::new(instr_data);

    let mut glyf_out: Vec<u8> = Vec::with_capacity(num_glyphs * 64);
    let mut loca_offsets: Vec<u32> = Vec::with_capacity(num_glyphs + 1);

    for i in 0..num_glyphs {
        let glyph_start = glyf_out.len();
        loca_offsets.push(glyph_start as u32);
        let n_contours_raw = n_contour_stream.read_u16()?;
        let n_contours = n_contours_raw as i16;
        let have_bbox = (bbox_bitmap[i >> 3] & (0x80 >> (i & 7))) != 0;

        if n_contours_raw == 0xFFFF {
            // Composite glyph
            if !have_bbox { return Err(WoffError::BadHeader); }
            let (csize, have_instr) = size_of_composite(&composite_stream)?;
            let instruction_size: u32 = if have_instr {
                glyph_stream.read_255_ushort()?
            } else { 0 };
            // Output: numContours(-1) + bbox(8) + composite(csize) + (instr)
            glyf_out.extend_from_slice(&(-1i16).to_be_bytes());
            // Bbox z bbox stream
            let bb = bbox_stream.read_bytes(8)?;
            glyf_out.extend_from_slice(bb);
            let comp_bytes = composite_stream.read_bytes(csize)?;
            glyf_out.extend_from_slice(comp_bytes);
            if have_instr {
                glyf_out.extend_from_slice(&(instruction_size as u16).to_be_bytes());
                let ib = instr_stream.read_bytes(instruction_size as usize)?;
                glyf_out.extend_from_slice(ib);
            }
        } else if n_contours > 0 {
            // Simple glyph
            let nc = n_contours as usize;
            let mut n_points_vec = Vec::with_capacity(nc);
            let mut total_pts: usize = 0;
            for _ in 0..nc {
                let np = n_points_stream.read_255_ushort()? as usize;
                n_points_vec.push(np);
                total_pts = total_pts.checked_add(np).ok_or(WoffError::OutOfBounds)?;
            }
            if flag_stream_pos + total_pts > flag_data.len() {
                return Err(WoffError::OutOfBounds);
            }
            let flags_slice = &flag_data[flag_stream_pos..flag_stream_pos + total_pts];
            flag_stream_pos += total_pts;

            let triplet_remaining = &glyph_data[glyph_stream.pos..];
            let (points, consumed) = triplet_decode(flags_slice, triplet_remaining, total_pts)?;
            glyph_stream.skip(consumed)?;

            let instruction_size = glyph_stream.read_255_ushort()?;
            let has_overlap_bit = has_overlap_bitmap
                && (overlap_bitmap[i >> 3] & (0x80 >> (i & 7))) != 0;

            // Output sfnt simple glyph:
            //   numContours u16 (n_contours)
            //   bbox 8B
            //   endPtsOfContours[nc] u16
            //   instructionLength u16
            //   instructions[]
            //   flags + x + y (pres store_points)
            glyf_out.extend_from_slice(&(n_contours as i16).to_be_bytes());
            if have_bbox {
                let bb = bbox_stream.read_bytes(8)?;
                glyf_out.extend_from_slice(bb);
            } else {
                glyf_out.extend_from_slice(&compute_bbox(&points));
            }
            // endPtsOfContours
            let mut end_pt: i32 = -1;
            for ix in 0..nc {
                end_pt += n_points_vec[ix] as i32;
                if end_pt >= 65536 { return Err(WoffError::OutOfBounds); }
                glyf_out.extend_from_slice(&(end_pt as u16).to_be_bytes());
            }
            // instructionLength + instructions
            glyf_out.extend_from_slice(&(instruction_size as u16).to_be_bytes());
            let ib = instr_stream.read_bytes(instruction_size as usize)?;
            glyf_out.extend_from_slice(ib);
            // Flags + x_coords + y_coords pres store_points
            let flag_offset = glyf_out.len();
            let final_size = store_points(&points, n_contours as u16,
                instruction_size as u16, has_overlap_bit, &mut glyf_out, flag_offset)?;
            // Truncate na final_size pri "extra" rezerve
            glyf_out.truncate(final_size);
        }
        // n_contours == 0: empty glyph, prazdny (no bytes)

        // Pad na 4 byte boundary (sfnt convention).
        while glyf_out.len() % 4 != 0 { glyf_out.push(0); }
    }
    loca_offsets.push(glyf_out.len() as u32);

    // Build loca table dle index_format.
    let mut loca_out: Vec<u8> = if index_format == 0 {
        // Short: u16 offset/2
        let mut v = Vec::with_capacity(loca_offsets.len() * 2);
        for o in &loca_offsets {
            v.extend_from_slice(&((*o / 2) as u16).to_be_bytes());
        }
        v
    } else {
        // Long: u32
        let mut v = Vec::with_capacity(loca_offsets.len() * 4);
        for o in &loca_offsets { v.extend_from_slice(&o.to_be_bytes()); }
        v
    };
    // Pad loca na 4 byte boundary.
    while loca_out.len() % 4 != 0 { loca_out.push(0); }

    Ok((glyf_out, loca_out))
}

/// UIntBase128 - WOFF2 variabilni delka u32 (1-5 bytes).
/// MSB = continuation bit, low 7 bits = data. Big-endian (most significant first).
fn read_uint_base128(data: &[u8], pos: usize) -> Result<(u32, usize), WoffError> {
    let mut accumulator: u32 = 0;
    let mut p = pos;
    for i in 0..5 {
        if p >= data.len() { return Err(WoffError::OutOfBounds); }
        let b = data[p];
        p += 1;
        // No leading zeros allowed (except first byte).
        if i == 0 && b == 0x80 { return Err(WoffError::BadHeader); }
        // Overflow check: pred posunem o 7 nesmi byt nejvyssi 7 bitu nastaveny.
        if (accumulator & 0xFE000000) != 0 { return Err(WoffError::BadHeader); }
        accumulator = (accumulator << 7) | ((b & 0x7F) as u32);
        if (b & 0x80) == 0 {
            return Ok((accumulator, p));
        }
    }
    Err(WoffError::BadHeader)
}

/// 255UInt16 - WOFF2 variabilni delka uint16 (1-3 bytes).
fn read_255_uint16(data: &[u8], pos: usize) -> Result<(u32, usize), WoffError> {
    if pos >= data.len() { return Err(WoffError::OutOfBounds); }
    let b0 = data[pos];
    if b0 == 253 {
        // 2-byte BE follows.
        if pos + 3 > data.len() { return Err(WoffError::OutOfBounds); }
        let v = u16::from_be_bytes([data[pos+1], data[pos+2]]) as u32;
        Ok((v, pos + 3))
    } else if b0 == 254 {
        // value = byte + 253*2 = byte + 506
        if pos + 2 > data.len() { return Err(WoffError::OutOfBounds); }
        Ok((data[pos+1] as u32 + 506, pos + 2))
    } else if b0 == 255 {
        // value = byte + 253
        if pos + 2 > data.len() { return Err(WoffError::OutOfBounds); }
        Ok((data[pos+1] as u32 + 253, pos + 2))
    } else {
        Ok((b0 as u32, pos + 1))
    }
}

#[cfg(test)]
#[path = "tests/woff_tests.rs"]
mod tests;
