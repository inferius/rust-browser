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
        // WOFF2 - zatim ne supported. Vrati input (fontdue selze parsovani -> font ne nahran).
        return data.to_vec();
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
