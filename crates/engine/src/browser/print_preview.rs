//! Print preview / PDF export configuration.
//!
//! Spec: CSS Paged Media Level 3 (@page rules).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaperSize {
    A3,
    A4,
    A5,
    Letter,
    Legal,
    Tabloid,
    Statement,
    Executive,
    Custom,
}

impl PaperSize {
    pub fn dimensions_mm(&self) -> (f32, f32) {
        match self {
            Self::A3 => (297.0, 420.0),
            Self::A4 => (210.0, 297.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
            Self::Tabloid => (279.4, 431.8),
            Self::Statement => (139.7, 215.9),
            Self::Executive => (184.15, 266.7),
            Self::Custom => (0.0, 0.0),
        }
    }

    pub fn dimensions_inches(&self) -> (f32, f32) {
        let (w, h) = self.dimensions_mm();
        (w / 25.4, h / 25.4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone)]
pub struct PrintSettings {
    pub paper: PaperSize,
    pub orientation: Orientation,
    pub margin_mm: Margins,
    pub scale_percent: u32,                // 100 = no scaling
    pub print_background: bool,
    pub headers_and_footers: bool,
    pub page_ranges: Vec<PageRange>,
    pub color: ColorMode,
    pub pages_per_sheet: u8,
    pub duplex: DuplexMode,
}

#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub top: f32, pub right: f32, pub bottom: f32, pub left: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct PageRange {
    pub start: u32,                        // 1-based
    pub end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorMode { Color, Monochrome }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DuplexMode { Simplex, ShortEdge, LongEdge }

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper: PaperSize::A4,
            orientation: Orientation::Portrait,
            margin_mm: Margins { top: 10.0, right: 10.0, bottom: 10.0, left: 10.0 },
            scale_percent: 100,
            print_background: false,
            headers_and_footers: true,
            page_ranges: Vec::new(),
            color: ColorMode::Color,
            pages_per_sheet: 1,
            duplex: DuplexMode::Simplex,
        }
    }
}

/// Parse Chrome-style page range string e.g. "1-3,5,7-9".
pub fn parse_page_ranges(input: &str) -> Result<Vec<PageRange>, String> {
    let mut out = Vec::new();
    for tok in input.split(',') {
        let tok = tok.trim();
        if tok.is_empty() { continue; }
        if let Some((a, b)) = tok.split_once('-') {
            let start: u32 = a.trim().parse().map_err(|_| format!("bad range '{}'", tok))?;
            let end: u32 = b.trim().parse().map_err(|_| format!("bad range '{}'", tok))?;
            if start == 0 || end < start { return Err(format!("invalid range '{}'", tok)); }
            out.push(PageRange { start, end });
        } else {
            let n: u32 = tok.parse().map_err(|_| format!("bad page '{}'", tok))?;
            if n == 0 { return Err("page 0 invalid".into()); }
            out.push(PageRange { start: n, end: n });
        }
    }
    Ok(out)
}

/// Resolve effective page list, clamped to total_pages.
pub fn resolve_pages(ranges: &[PageRange], total_pages: u32) -> Vec<u32> {
    if ranges.is_empty() {
        return (1..=total_pages).collect();
    }
    let mut out = Vec::new();
    for r in ranges {
        for p in r.start..=r.end.min(total_pages) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_a4_dimensions() {
        let (w, h) = PaperSize::A4.dimensions_mm();
        assert_eq!(w, 210.0);
        assert_eq!(h, 297.0);
    }

    #[test]
    fn parse_ranges_basic() {
        let r = parse_page_ranges("1-3,5,7-9").unwrap();
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn parse_ranges_empty_ok() {
        let r = parse_page_ranges("").unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn parse_ranges_bad_errors() {
        assert!(parse_page_ranges("5-2").is_err());
        assert!(parse_page_ranges("0").is_err());
        assert!(parse_page_ranges("xyz").is_err());
    }

    #[test]
    fn resolve_pages_empty_means_all() {
        let pages = resolve_pages(&[], 5);
        assert_eq!(pages, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn resolve_pages_clamped() {
        let pages = resolve_pages(&[PageRange { start: 1, end: 100 }], 5);
        assert_eq!(pages, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn default_settings_a4_portrait() {
        let s = PrintSettings::default();
        assert_eq!(s.paper, PaperSize::A4);
        assert_eq!(s.orientation, Orientation::Portrait);
    }
}
