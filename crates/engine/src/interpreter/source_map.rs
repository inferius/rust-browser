//! Source Map v3 - inverse of compilation, restores original positions.
//!
//! Spec: https://sourcemaps.info/spec.html
//! V3 format: { version: 3, sources: [...], names: [...], mappings: "AAAA;..." }
//! mappings: per generated line, semicolon-separated.
//! Per generated segment: comma-separated VLQ-encoded ints (generatedCol, sourceIdx, srcLine, srcCol, [nameIdx]).

#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    pub version: u32,
    pub sources: Vec<String>,
    pub names: Vec<String>,
    pub source_root: Option<String>,
    pub mappings: Vec<Vec<Mapping>>,    // per generated line
    pub file: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    pub generated_column: u32,
    pub source_index: Option<u32>,
    pub source_line: Option<u32>,
    pub source_column: Option<u32>,
    pub name_index: Option<u32>,
}

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// VLQ encode a signed int per source-map v3 base64 VLQ encoding.
pub fn vlq_encode(value: i32) -> String {
    let mut out = String::new();
    let mut v = if value < 0 { ((-value as u32) << 1) | 1 } else { (value as u32) << 1 };
    loop {
        let mut digit = v & 0b11111;
        v >>= 5;
        if v > 0 { digit |= 0b100000; }
        out.push(BASE64[digit as usize] as char);
        if v == 0 { break; }
    }
    out
}

/// VLQ decode, returns (value, bytes_consumed).
pub fn vlq_decode(s: &[u8]) -> Option<(i32, usize)> {
    let mut shift = 0u32;
    let mut value = 0u32;
    let mut idx = 0;
    loop {
        if idx >= s.len() { return None; }
        let digit = base64_char(s[idx])?;
        idx += 1;
        let chunk = digit & 0b11111;
        value |= (chunk as u32) << shift;
        if (digit & 0b100000) == 0 { break; }
        shift += 5;
        if shift > 31 { return None; }
    }
    let signed = if (value & 1) != 0 {
        let abs = (value >> 1) as i32;
        if abs == 0 { i32::MIN } else { -abs }
    } else {
        (value >> 1) as i32
    };
    Some((signed, idx))
}

fn base64_char(c: u8) -> Option<u32> {
    BASE64.iter().position(|b| *b == c).map(|p| p as u32)
}

/// Parse mappings string (e.g. "AAAA,SAAS;CAAC,...") into structured mappings.
pub fn parse_mappings(input: &str) -> Vec<Vec<Mapping>> {
    let mut all = Vec::new();
    let mut source_idx: i32 = 0;
    let mut source_line: i32 = 0;
    let mut source_col: i32 = 0;
    let mut name_idx: i32 = 0;
    for line in input.split(';') {
        let mut gen_col: i32 = 0;
        let mut line_maps = Vec::new();
        for segment in line.split(',') {
            if segment.is_empty() { continue; }
            let bytes = segment.as_bytes();
            let mut idx = 0;
            let fields = read_vlqs(bytes, &mut idx);
            if fields.is_empty() { continue; }
            gen_col += fields[0];
            let mut m = Mapping {
                generated_column: gen_col as u32,
                source_index: None, source_line: None, source_column: None, name_index: None,
            };
            if fields.len() >= 4 {
                source_idx += fields[1];
                source_line += fields[2];
                source_col += fields[3];
                m.source_index = Some(source_idx as u32);
                m.source_line = Some(source_line as u32);
                m.source_column = Some(source_col as u32);
            }
            if fields.len() >= 5 {
                name_idx += fields[4];
                m.name_index = Some(name_idx as u32);
            }
            line_maps.push(m);
        }
        all.push(line_maps);
    }
    all
}

fn read_vlqs(bytes: &[u8], idx: &mut usize) -> Vec<i32> {
    let mut out = Vec::new();
    while *idx < bytes.len() {
        let Some((v, n)) = vlq_decode(&bytes[*idx..]) else { break; };
        out.push(v);
        *idx += n;
    }
    out
}

impl SourceMap {
    /// Look up original position for (generated_line, generated_col).
    /// Returns (source_file, line, column, name).
    pub fn original_position(&self, gen_line: u32, gen_col: u32) -> Option<(String, u32, u32, Option<String>)> {
        let line_maps = self.mappings.get(gen_line as usize)?;
        // Find closest mapping with generated_column <= gen_col.
        let mut best: Option<&Mapping> = None;
        for m in line_maps {
            if m.generated_column <= gen_col {
                best = Some(m);
            } else { break; }
        }
        let m = best?;
        let src_idx = m.source_index? as usize;
        let src_file = self.sources.get(src_idx)?.clone();
        let name = m.name_index.and_then(|i| self.names.get(i as usize).cloned());
        Some((src_file, m.source_line?, m.source_column?, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_round_trip_zero() {
        let s = vlq_encode(0);
        let (back, _) = vlq_decode(s.as_bytes()).unwrap();
        assert_eq!(back, 0);
    }

    #[test]
    fn vlq_round_trip_positive() {
        for v in [1, 5, 10, 100, 1000, 65535] {
            let s = vlq_encode(v);
            let (back, _) = vlq_decode(s.as_bytes()).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn vlq_round_trip_negative() {
        for v in [-1, -5, -10, -100] {
            let s = vlq_encode(v);
            let (back, _) = vlq_decode(s.as_bytes()).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn parse_simple_mapping() {
        // "AAAA" = [0,0,0,0] = genCol 0, srcIdx 0, srcLine 0, srcCol 0
        let m = parse_mappings("AAAA");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].len(), 1);
        assert_eq!(m[0][0].source_index, Some(0));
    }

    #[test]
    fn parse_empty_lines() {
        let m = parse_mappings(";;");
        assert_eq!(m.len(), 3);
        assert!(m.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn original_position_lookup() {
        let mut sm = SourceMap::default();
        sm.version = 3;
        sm.sources = vec!["a.ts".into()];
        sm.mappings = parse_mappings("AAAA");
        let pos = sm.original_position(0, 0).unwrap();
        assert_eq!(pos.0, "a.ts");
        assert_eq!(pos.1, 0);
        assert_eq!(pos.2, 0);
    }
}
