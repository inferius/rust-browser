//! Typed CSS overflow values. Nahrazuje `overflow_x: String` / `overflow_y: String`
//! pattern v LayoutBox. Hot path (`matches!(s.as_str(), "hidden"|"scroll"|...)`)
//! pak je `matches!(enum, Overflow::Hidden | Overflow::Scroll)` = jump table
//! O(1) namisto strcmp.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Overflow {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl Default for Overflow {
    fn default() -> Self { Overflow::Visible }
}

impl Overflow {
    /// Parse z CSS string. Unknown / initial / inherit -> Visible (default).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hidden" => Self::Hidden,
            "clip" => Self::Clip,
            "scroll" => Self::Scroll,
            "auto" => Self::Auto,
            // "visible", "", initial, inherit, unset, unknown -> Visible
            _ => Self::Visible,
        }
    }

    /// True pri jakemkoli clip-style overflow (hidden / clip / scroll / auto).
    /// Hot pattern: `matches!(bx.overflow_x.as_str(), "hidden"|"clip"|"scroll"|"auto")`
    #[inline]
    pub fn clips(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    /// True pri scrollable variantach (scroll / auto). Hot pattern:
    /// `bx.overflow_y == "scroll" || bx.overflow_y == "auto"` (scrollbar visible).
    #[inline]
    pub fn scrollable(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }

    /// overflow:scroll zobrazi scrollbar VZDY (i kdyz se obsah vejde) - na
    /// rozdil od auto (jen pri preteceni). CSS spec. Docx: "overflow: scroll
    /// s custom scrollbarem" blok neukazoval zadny scrollbar (bral se jako auto).
    #[inline]
    pub fn always_shows(&self) -> bool {
        matches!(self, Self::Scroll)
    }

    /// True pri hidden / clip - no scrollbar, just clip.
    #[inline]
    pub fn hides(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip)
    }

    /// True pri Visible (no clip, no scrollbar).
    #[inline]
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Visible)
    }
}

impl fmt::Display for Overflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Overflow::Visible => "visible",
            Overflow::Hidden => "hidden",
            Overflow::Clip => "clip",
            Overflow::Scroll => "scroll",
            Overflow::Auto => "auto",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known() {
        assert_eq!(Overflow::parse("hidden"), Overflow::Hidden);
        assert_eq!(Overflow::parse("CLIP"), Overflow::Clip);
        assert_eq!(Overflow::parse("  scroll  "), Overflow::Scroll);
        assert_eq!(Overflow::parse("auto"), Overflow::Auto);
        assert_eq!(Overflow::parse("visible"), Overflow::Visible);
    }

    #[test]
    fn parse_unknown_to_visible() {
        assert_eq!(Overflow::parse(""), Overflow::Visible);
        assert_eq!(Overflow::parse("garbage"), Overflow::Visible);
        assert_eq!(Overflow::parse("initial"), Overflow::Visible);
    }

    #[test]
    fn clips_predicate() {
        assert!(Overflow::Hidden.clips());
        assert!(Overflow::Clip.clips());
        assert!(Overflow::Scroll.clips());
        assert!(Overflow::Auto.clips());
        assert!(!Overflow::Visible.clips());
    }

    #[test]
    fn scrollable_predicate() {
        assert!(Overflow::Scroll.scrollable());
        assert!(Overflow::Auto.scrollable());
        assert!(!Overflow::Hidden.scrollable());
        assert!(!Overflow::Clip.scrollable());
        assert!(!Overflow::Visible.scrollable());
    }

    #[test]
    fn hides_predicate() {
        assert!(Overflow::Hidden.hides());
        assert!(Overflow::Clip.hides());
        assert!(!Overflow::Scroll.hides());
        assert!(!Overflow::Auto.hides());
    }
}
