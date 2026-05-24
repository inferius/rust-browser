//! Source map lookup pres devtools - generated line/col -> original.
//!
//! parse_source_map V3 + VLQ decode existuje (devtools/model/sources.rs).
//! Tady pridavame binary search lookup pres mappings sorted by generated pos.
//!
//! Inspired by Chromium `third_party/blink/renderer/core/inspector/v8_inspector_string.cc`
//! + Source Maps L3 spec.

#[derive(Debug, Clone)]
pub struct Mapping {
    pub generated_line: u32,
    pub generated_column: u32,
    pub source_idx: Option<u32>,
    pub original_line: Option<u32>,
    pub original_column: Option<u32>,
    pub name_idx: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    pub version: u32,
    pub sources: Vec<String>,
    pub names: Vec<String>,
    pub mappings: Vec<Mapping>, // sorted by (generated_line, generated_column)
}

impl SourceMap {
    /// Lookup original location pro generated (line, column).
    /// Binary search pres mappings - vraci nejbližší <= record.
    pub fn lookup(&self, gen_line: u32, gen_col: u32) -> Option<&Mapping> {
        // Find last mapping <= (line, col).
        let idx = self.mappings.partition_point(|m| {
            (m.generated_line, m.generated_column) <= (gen_line, gen_col)
        });
        if idx == 0 { return None; }
        Some(&self.mappings[idx - 1])
    }

    /// Translate (gen_line, gen_col) -> (source_file, orig_line, orig_col, name).
    pub fn translate(&self, gen_line: u32, gen_col: u32)
        -> Option<(String, u32, u32, Option<String>)>
    {
        let m = self.lookup(gen_line, gen_col)?;
        let source = m.source_idx.and_then(|i| self.sources.get(i as usize))?.clone();
        let line = m.original_line?;
        let col = m.original_column?;
        let name = m.name_idx.and_then(|i| self.names.get(i as usize)).cloned();
        Some((source, line, col, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_map() -> SourceMap {
        SourceMap {
            version: 3,
            sources: vec!["src/app.ts".into(), "src/util.ts".into()],
            names: vec!["foo".into(), "bar".into()],
            mappings: vec![
                Mapping { generated_line: 0, generated_column: 0,
                    source_idx: Some(0), original_line: Some(10), original_column: Some(0),
                    name_idx: None },
                Mapping { generated_line: 0, generated_column: 20,
                    source_idx: Some(0), original_line: Some(10), original_column: Some(15),
                    name_idx: Some(0) },
                Mapping { generated_line: 5, generated_column: 0,
                    source_idx: Some(1), original_line: Some(3), original_column: Some(0),
                    name_idx: Some(1) },
            ],
        }
    }

    #[test]
    fn lookup_exact() {
        let m = sample_map();
        let r = m.lookup(0, 0).unwrap();
        assert_eq!(r.source_idx, Some(0));
        assert_eq!(r.original_line, Some(10));
    }

    #[test]
    fn lookup_between_picks_lower() {
        let m = sample_map();
        let r = m.lookup(0, 10).unwrap();
        // Between (0,0) a (0,20) -> bere (0,0).
        assert_eq!(r.generated_column, 0);
    }

    #[test]
    fn lookup_higher_line() {
        let m = sample_map();
        let r = m.lookup(5, 0).unwrap();
        assert_eq!(r.source_idx, Some(1));
        assert_eq!(r.original_line, Some(3));
    }

    #[test]
    fn lookup_before_first() {
        let m = SourceMap {
            mappings: vec![Mapping {
                generated_line: 5, generated_column: 0,
                source_idx: Some(0), original_line: Some(1), original_column: Some(0),
                name_idx: None,
            }],
            ..Default::default()
        };
        assert!(m.lookup(0, 0).is_none());
    }

    #[test]
    fn translate_full_info() {
        let m = sample_map();
        let (src, line, col, name) = m.translate(0, 25).unwrap();
        assert_eq!(src, "src/app.ts");
        assert_eq!(line, 10);
        assert_eq!(col, 15);
        assert_eq!(name, Some("foo".into()));
    }
}
