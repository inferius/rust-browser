//! Sources panel: registr scriptu + breakpoints.

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: u32,
    pub url: String,            // "<inline #1>" / "https://.../app.js" / "file:///...js"
    pub content: String,
    pub language: SourceLang,
    /// Optional source-map URL z `//# sourceMappingURL=` komentare.
    pub source_map_url: Option<String>,
    /// Parsed source map (po fetch + decode). None pri chybe nebo absentnim.
    pub source_map: Option<SourceMap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLang {
    JavaScript,
    Css,
    Html,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct SourcesState {
    pub files: Vec<SourceFile>,
    pub selected_id: Option<u32>,
    pub breakpoints: HashSet<Breakpoint>,
    pub scroll_y: f32,
    pub debugger_paused: bool,
    pub current_pause_location: Option<(u32, u32)>, // (file_id, line)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Breakpoint {
    pub file_id: u32,
    pub line: u32, // 1-based
}

impl SourcesState {
    pub fn add_file(&mut self, url: String, content: String, language: SourceLang) -> u32 {
        let id = self.files.len() as u32;
        let source_map_url = detect_source_map_url(&content, language);
        self.files.push(SourceFile {
            id,
            url,
            content,
            language,
            source_map_url,
            source_map: None,
        });
        id
    }

    /// Spusti fetch + parse source mapu pro file s `id`. Bere fetcher closure
    /// (URL -> Option<String>) - typicky `crate::browser::render::fetch_text_url`.
    pub fn load_source_map<F: Fn(&str) -> Option<String>>(&mut self, file_id: u32, base_url: &str, fetch: F) {
        let Some(file) = self.files.iter_mut().find(|f| f.id == file_id) else { return };
        if file.source_map.is_some() { return }
        let Some(map_url) = file.source_map_url.clone() else { return };
        // Resolve relative.
        let resolved = if map_url.starts_with("http://") || map_url.starts_with("https://")
            || map_url.starts_with("file:") || map_url.starts_with("data:") {
            map_url.clone()
        } else if let Some(base_dir_end) = base_url.rfind('/') {
            format!("{}/{}", &base_url[..base_dir_end], map_url)
        } else {
            map_url.clone()
        };
        // Data URI shortcut.
        if let Some(rest) = resolved.strip_prefix("data:application/json") {
            if let Some(b64_idx) = rest.find(",base64") {
                let _ = b64_idx;
            } else if let Some(comma) = rest.find(',') {
                let body = &rest[comma+1..];
                if let Some(map) = parse_source_map(body) {
                    file.source_map = Some(map);
                }
                return;
            }
        }
        if let Some(content) = fetch(&resolved) {
            if let Some(map) = parse_source_map(&content) {
                file.source_map = Some(map);
            }
        }
    }

    /// Map generated (line, col) -> original (file, line, col) pres source map
    /// nejblizsi predchazejici segment. None pri absent map nebo nematch.
    pub fn map_position(&self, file_id: u32, gen_line: u32, gen_col: u32) -> Option<(String, u32, u32)> {
        let file = self.files.iter().find(|f| f.id == file_id)?;
        let map = file.source_map.as_ref()?;
        let segs = map.mappings.get(gen_line as usize)?;
        // Najdi nejvetsi seg.gen_col <= gen_col.
        let mut best: Option<&MapSegment> = None;
        for s in segs {
            if s.gen_col <= gen_col {
                best = Some(s);
            } else { break; }
        }
        let s = best?;
        let src_idx = s.src_idx? as usize;
        let src_name = map.sources.get(src_idx).cloned()?;
        Some((src_name, s.src_line?, s.src_col?))
    }

    pub fn toggle_breakpoint(&mut self, file_id: u32, line: u32) -> bool {
        let bp = Breakpoint { file_id, line };
        if self.breakpoints.contains(&bp) {
            self.breakpoints.remove(&bp);
            false
        } else {
            self.breakpoints.insert(bp);
            true
        }
    }

    pub fn has_breakpoint(&self, file_id: u32, line: u32) -> bool {
        self.breakpoints.contains(&Breakpoint { file_id, line })
    }
}

fn detect_source_map_url(src: &str, lang: SourceLang) -> Option<String> {
    let prefix = match lang {
        SourceLang::JavaScript => "//# sourceMappingURL=",
        SourceLang::Css => "/*# sourceMappingURL=",
        _ => return None,
    };
    for line in src.lines().rev().take(5) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest.trim_end_matches("*/").trim().to_string());
        }
    }
    None
}

// ─── Source Map parsing (V3 format) ──────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SourceMap {
    pub version: u32,
    pub sources: Vec<String>,
    pub sources_content: Vec<Option<String>>,
    pub names: Vec<String>,
    /// Decoded mappings. Index = generated line (0-based).
    /// Each entry is list of segments: (gen_col, src_idx, src_line, src_col, name_idx?)
    pub mappings: Vec<Vec<MapSegment>>,
}

#[derive(Debug, Clone, Copy)]
pub struct MapSegment {
    pub gen_col: u32,
    pub src_idx: Option<u32>,
    pub src_line: Option<u32>,
    pub src_col: Option<u32>,
    pub name_idx: Option<u32>,
}

/// Parsuje source map JSON. Tolerant: pri parse error vraci None.
pub fn parse_source_map(json: &str) -> Option<SourceMap> {
    let parsed = lite_json_parse(json)?;
    let LiteJson::Object(obj) = parsed else { return None };

    let version = obj.iter().find(|(k, _)| k == "version")
        .and_then(|(_, v)| if let LiteJson::Number(n) = v { Some(*n as u32) } else { None })
        .unwrap_or(3);

    let sources: Vec<String> = match obj.iter().find(|(k, _)| k == "sources") {
        Some((_, LiteJson::Array(arr))) => arr.iter()
            .filter_map(|v| if let LiteJson::String(s) = v { Some(s.clone()) } else { None })
            .collect(),
        _ => Vec::new(),
    };

    let sources_content: Vec<Option<String>> = match obj.iter().find(|(k, _)| k == "sourcesContent") {
        Some((_, LiteJson::Array(arr))) => arr.iter()
            .map(|v| match v {
                LiteJson::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    let names: Vec<String> = match obj.iter().find(|(k, _)| k == "names") {
        Some((_, LiteJson::Array(arr))) => arr.iter()
            .filter_map(|v| if let LiteJson::String(s) = v { Some(s.clone()) } else { None })
            .collect(),
        _ => Vec::new(),
    };

    let mappings_str = match obj.iter().find(|(k, _)| k == "mappings") {
        Some((_, LiteJson::String(s))) => s.clone(),
        _ => String::new(),
    };

    let mappings = decode_mappings(&mappings_str);

    Some(SourceMap {
        version,
        sources,
        sources_content,
        names,
        mappings,
    })
}

/// Decode mappings string format (per spec).
/// Skupiny radku oddelene `;`, segmenty v radku oddelene `,`.
/// Kazdy segment je 1, 4 nebo 5 VLQ values:
///   [gen_col_delta]
///   [gen_col_delta, src_idx_delta, src_line_delta, src_col_delta]
///   [gen_col_delta, src_idx_delta, src_line_delta, src_col_delta, name_idx_delta]
/// Vsechny delta-encoded vuci predchozimu segmentu (pro col/idx/line/name jsou
/// state hodnoty drzeny napric segmenty + radky, ale gen_col se resetuje per radek).
fn decode_mappings(s: &str) -> Vec<Vec<MapSegment>> {
    let mut out = Vec::new();
    let mut state_src_idx: i64 = 0;
    let mut state_src_line: i64 = 0;
    let mut state_src_col: i64 = 0;
    let mut state_name_idx: i64 = 0;

    for line in s.split(';') {
        let mut segs = Vec::new();
        let mut state_gen_col: i64 = 0;
        if !line.is_empty() {
            for seg in line.split(',') {
                let values = decode_vlq_seq(seg);
                if values.is_empty() { continue; }
                state_gen_col += values[0];
                let mut src_idx = None;
                let mut src_line = None;
                let mut src_col = None;
                let mut name_idx = None;
                if values.len() >= 4 {
                    state_src_idx += values[1];
                    state_src_line += values[2];
                    state_src_col += values[3];
                    src_idx = Some(state_src_idx as u32);
                    src_line = Some(state_src_line as u32);
                    src_col = Some(state_src_col as u32);
                }
                if values.len() >= 5 {
                    state_name_idx += values[4];
                    name_idx = Some(state_name_idx as u32);
                }
                segs.push(MapSegment {
                    gen_col: state_gen_col as u32,
                    src_idx,
                    src_line,
                    src_col,
                    name_idx,
                });
            }
        }
        out.push(segs);
    }
    out
}

/// Decode base64-VLQ sekvenci.
/// Per spec: kazdy VLQ value je 1+ base64 chars. Per char 6 bitu. Posledni 2 bity:
/// continuation bit (bit 5) + (na prvni group) sign bit (bit 0).
/// Vraci signed integers.
fn decode_vlq_seq(s: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut value: i64 = 0;
    let mut shift: u32 = 0;
    let mut first = true;
    let mut sign_bit = 0i64;
    for ch in s.chars() {
        let digit = match base64_char_to_value(ch) {
            Some(d) => d as i64,
            None => continue,
        };
        let cont = digit & 0b100000;
        let raw = digit & 0b011111;
        if first {
            sign_bit = raw & 0b1;
            value = (raw >> 1) as i64;
            shift = 4;
            first = false;
        } else {
            value |= raw << shift;
            shift += 5;
        }
        if cont == 0 {
            let signed = if sign_bit == 1 { -value } else { value };
            out.push(signed);
            value = 0;
            shift = 0;
            first = true;
        }
    }
    out
}

fn base64_char_to_value(c: char) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a' + 26),
        '0'..='9' => Some(c as u8 - b'0' + 52),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

// ─── Lite JSON parser (just for source maps, ne full spec) ───────────────

#[derive(Debug, Clone)]
enum LiteJson {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<LiteJson>),
    Object(Vec<(String, LiteJson)>),
}

fn lite_json_parse(s: &str) -> Option<LiteJson> {
    let mut p = JsonParser { src: s.as_bytes(), pos: 0 };
    p.skip_ws();
    let v = p.parse_value()?;
    Some(v)
}

struct JsonParser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' { self.pos += 1; }
            else { break; }
        }
    }

    fn parse_value(&mut self) -> Option<LiteJson> {
        self.skip_ws();
        match self.peek()? {
            b'"' => self.parse_string().map(LiteJson::String),
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b't' | b'f' => self.parse_bool(),
            b'n' => self.parse_null(),
            b'-' | b'0'..=b'9' => self.parse_number().map(LiteJson::Number),
            _ => None,
        }
    }

    fn parse_string(&mut self) -> Option<String> {
        self.skip_ws();
        if self.peek()? != b'"' { return None; }
        self.pos += 1;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == b'"' { return Some(out); }
            if c == b'\\' {
                let esc = self.peek()?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'b' => out.push('\x08'),
                    b'f' => out.push('\x0c'),
                    b'u' => {
                        // 4 hex digits
                        if self.pos + 4 > self.src.len() { return None; }
                        let hex = std::str::from_utf8(&self.src[self.pos..self.pos+4]).ok()?;
                        let code = u32::from_str_radix(hex, 16).ok()?;
                        self.pos += 4;
                        if let Some(ch) = char::from_u32(code) { out.push(ch); }
                    }
                    _ => return None,
                }
            } else {
                out.push(c as char);
            }
        }
        None
    }

    fn parse_object(&mut self) -> Option<LiteJson> {
        self.skip_ws();
        if self.peek()? != b'{' { return None; }
        self.pos += 1;
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') { self.pos += 1; return Some(LiteJson::Object(out)); }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek()? != b':' { return None; }
            self.pos += 1;
            let val = self.parse_value()?;
            out.push((key, val));
            self.skip_ws();
            match self.peek()? {
                b',' => { self.pos += 1; }
                b'}' => { self.pos += 1; return Some(LiteJson::Object(out)); }
                _ => return None,
            }
        }
    }

    fn parse_array(&mut self) -> Option<LiteJson> {
        self.skip_ws();
        if self.peek()? != b'[' { return None; }
        self.pos += 1;
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') { self.pos += 1; return Some(LiteJson::Array(out)); }
        loop {
            let val = self.parse_value()?;
            out.push(val);
            self.skip_ws();
            match self.peek()? {
                b',' => { self.pos += 1; }
                b']' => { self.pos += 1; return Some(LiteJson::Array(out)); }
                _ => return None,
            }
        }
    }

    fn parse_bool(&mut self) -> Option<LiteJson> {
        if self.src[self.pos..].starts_with(b"true") { self.pos += 4; Some(LiteJson::Bool(true)) }
        else if self.src[self.pos..].starts_with(b"false") { self.pos += 5; Some(LiteJson::Bool(false)) }
        else { None }
    }

    fn parse_null(&mut self) -> Option<LiteJson> {
        if self.src[self.pos..].starts_with(b"null") { self.pos += 4; Some(LiteJson::Null) }
        else { None }
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.pos;
        if self.peek() == Some(b'-') { self.pos += 1; }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos]).ok()?.parse().ok()
    }
}

#[cfg(test)]
#[path = "../tests/sources_tests.rs"]
mod tests;
