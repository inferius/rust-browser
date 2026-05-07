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
        let (orig_len, p) = read_255_uint16(data, pos)?;
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
            let (l, p2) = read_255_uint16(data, pos)?;
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
        let mut reader = brotli::Decompressor::new(compressed, 4096);
        reader.read_to_end(&mut decompressed).map_err(|_| WoffError::Decompress)?;
    }

    // Detekce: pokud nejaka tabulka je glyf/loca s transform_ver=0, NELZE
    // ji bez glyf transform reversal nasloucha. Vrat TransformNotImplemented.
    for (tag, _, _, transform_ver) in &entries {
        let tag_bytes = tag.to_be_bytes();
        let is_glyf_or_loca = &tag_bytes == b"glyf" || &tag_bytes == b"loca";
        if is_glyf_or_loca && *transform_ver == 0 {
            return Err(WoffError::TransformNotImplemented);
        }
    }

    // Bez glyf transformaci: decompressed obsahuje tables konkatenovane v poradi
    // dle table directory. Vystup sfnt s table directory + zarovnanimi.
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

    let mut data_offset = 0usize;
    let mut directory: Vec<(u32, u32, u32, u32)> = Vec::new();
    for (tag, orig_len, transform_len, _) in &entries {
        let len = *transform_len as usize;
        if data_offset + len > decompressed.len() { return Err(WoffError::OutOfBounds); }
        let table_data = &decompressed[data_offset..data_offset + len];
        data_offset += len;
        let table_offset = out.len();
        out.extend_from_slice(table_data);
        let pad = (4 - (len % 4)) % 4;
        for _ in 0..pad { out.push(0); }
        directory.push((*tag, 0u32, table_offset as u32, *orig_len));
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
mod tests {
    use super::*;

    #[test]
    fn maybe_decode_passes_through_non_woff() {
        let data = vec![0u8; 100];
        let out = maybe_decode_woff(&data);
        assert_eq!(out, data);
    }

    #[test]
    fn decode_rejects_bad_signature() {
        let data = vec![b'X'; 50];
        assert!(decode_woff1(&data).is_err());
    }

    #[test]
    fn decode_rejects_too_short() {
        let data = b"wOFF";
        assert!(decode_woff1(data).is_err());
    }

    /// Vyrobi minimalisticky valid WOFF s jednou tabulkou (raw, no compression).
    /// Header je presne 44 bytes (8B sig+flavor, 4B length, 2B numTables, 2B reserved,
    /// 4B totalSfntSize, 2B+2B major/minor, 4*5=20B meta+priv = 44).
    fn make_minimal_woff() -> Vec<u8> {
        let table_data: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let table_tag: u32 = 0x6e616d65; // 'name'
        let mut out = Vec::new();
        out.extend_from_slice(b"wOFF"); // 4
        out.extend_from_slice(&0x00010000u32.to_be_bytes()); // flavor TTF (4)
        let total_len = 44u32 + 20 + table_data.len() as u32;
        out.extend_from_slice(&total_len.to_be_bytes()); // length (4) -> 16
        out.extend_from_slice(&1u16.to_be_bytes()); // numTables (2) -> 18
        out.extend_from_slice(&0u16.to_be_bytes()); // reserved (2) -> 20
        out.extend_from_slice(&(12 + 16 + table_data.len() as u32).to_be_bytes()); // totalSfntSize (4) -> 24
        out.extend_from_slice(&0u16.to_be_bytes()); // majorVersion (2) -> 26
        out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion (2) -> 28
        out.extend_from_slice(&0u32.to_be_bytes()); // metaOffset (4) -> 32
        out.extend_from_slice(&0u32.to_be_bytes()); // metaLength (4) -> 36
        out.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength (4) -> 40
        out.extend_from_slice(&0u32.to_be_bytes()); // privOffset (4) -> 44
        out.extend_from_slice(&0u32.to_be_bytes()); // privLength (4) -> 48 - WAIT to je 48
        // Pozn: spec je 44, takze metaOrigLength + privOffset + privLength musi byt JEN 12B nebo
        // jsme uplne mimo. Spravna struktura:
        // metaOffset(4) + metaLength(4) + metaOrigLength(4) + privOffset(4) + privLength(4) = 20 BYTES
        // Takze 24 + 20 = 44. To jsme dali. Ale ve skutku zkontroluj Rev.

        // Resign: zruseno. Proste vrat 44B header s 20B meta+priv.
        out.truncate(0);
        out.extend_from_slice(b"wOFF");
        out.extend_from_slice(&0x00010000u32.to_be_bytes());
        out.extend_from_slice(&total_len.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&(12 + 16 + table_data.len() as u32).to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // major
        out.extend_from_slice(&0u16.to_be_bytes()); // minor
        // 5x u32 = 20B
        out.extend_from_slice(&[0u8; 20]);
        // Total tak daleko: 4+4+4+2+2+4+2+2+20 = 44 ✓

        // Table directory entry (20B)
        out.extend_from_slice(&table_tag.to_be_bytes());
        out.extend_from_slice(&64u32.to_be_bytes()); // offset (44 + 20 = 64)
        out.extend_from_slice(&(table_data.len() as u32).to_be_bytes()); // compLength
        out.extend_from_slice(&(table_data.len() as u32).to_be_bytes()); // origLength
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum
        // Raw table data at offset 64
        out.extend_from_slice(&table_data);
        out
    }

    #[test]
    fn read_255_uint16_short_form() {
        let data = [42u8, 0];
        let (v, p) = read_255_uint16(&data, 0).unwrap();
        assert_eq!(v, 42);
        assert_eq!(p, 1);
    }

    #[test]
    fn read_255_uint16_two_byte_high_low() {
        // 253 marker -> next 2 bytes BE = 1000.
        let data = [253u8, 0x03, 0xE8];
        let (v, p) = read_255_uint16(&data, 0).unwrap();
        assert_eq!(v, 1000);
        assert_eq!(p, 3);
    }

    #[test]
    fn read_255_uint16_offset_506() {
        let data = [254u8, 100];
        let (v, _) = read_255_uint16(&data, 0).unwrap();
        assert_eq!(v, 606); // 100 + 506
    }

    #[test]
    fn read_255_uint16_offset_253() {
        let data = [255u8, 100];
        let (v, _) = read_255_uint16(&data, 0).unwrap();
        assert_eq!(v, 353); // 100 + 253
    }

    #[test]
    fn decode_woff2_rejects_bad_signature() {
        let data = vec![b'X'; 100];
        assert!(decode_woff2(&data).is_err());
    }

    #[test]
    fn decode_minimal_woff_to_sfnt() {
        let woff = make_minimal_woff();
        let sfnt = decode_woff1(&woff).expect("decode");
        // sfnt musi zacit s 0x00010000 (TTF flavor).
        assert_eq!(&sfnt[0..4], &[0x00, 0x01, 0x00, 0x00]);
        // numTables = 1.
        assert_eq!(u16::from_be_bytes([sfnt[4], sfnt[5]]), 1);
        // Table tag 'name' v directory.
        assert_eq!(&sfnt[12..16], b"name");
    }
}
