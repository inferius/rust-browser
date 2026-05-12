//! CSS <length> / <length-percentage> typed enum.
//!
//! Zachovava puvodni jednotku (Em, Vw, Percent, Calc) - resolution na
//! konkretni px hodnotu az pri layout dispatch s realnym kontextem
//! (parent_size, viewport, font_size).
//!
//! Rozdil oproti `parse_length() -> f32` (layout/length.rs):
//! tam se resolvuje hned pri parsu s default kontextem (vw=1024, vh=768,
//! parent=16). Po L5 cascade ukladame typed `Length` a resolvuje az
//! pri layout, kdy znama real parent dimension.

/// CSS length value zachovavajici jednotku pro pozdejsi context-aware
/// resolution.
///
/// `Clone` (ne Copy) - FitContent + Calc obsahuji Box.
#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    /// Absolute pixels (CSS reference px = 1/96 inch).
    Px(f32),
    /// em - relativni vuci font-size aktualniho elementu.
    Em(f32),
    /// rem - relativni vuci font-size root elementu.
    Rem(f32),
    /// vw - 1% sirky viewportu.
    Vw(f32),
    /// vh - 1% vysky viewportu.
    Vh(f32),
    /// vmin/vmax - 1% mensiho/vetsiho rozmeru viewportu.
    Vmin(f32),
    Vmax(f32),
    /// % - relativni vuci parent dimension.
    Percent(f32),
    /// ch - sirka glyphu '0' (default aprox 0.5 * font-size).
    Ch(f32),
    /// ex - x-height (default aprox 0.5 * font-size).
    Ex(f32),
    /// auto - layout-determined.
    Auto,
    /// none - no max constraint.
    None,
    /// min-content / max-content / fit-content().
    MinContent,
    MaxContent,
    FitContent(Box<Length>),
    /// calc() unresolved tree.
    Calc(CalcExpr),
}

/// Unresolved calc() expression.
/// Stored simplified: leaf nodes ulozeny rekursivne, ops mezi nimi.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcExpr {
    Leaf(Box<Length>),
    Add(Box<CalcExpr>, Box<CalcExpr>),
    Sub(Box<CalcExpr>, Box<CalcExpr>),
    Mul(Box<CalcExpr>, f32),
    Div(Box<CalcExpr>, f32),
}

impl Length {
    /// Resolve na px hodnotu s plnym kontextem.
    /// Pro Auto/None vraci `fallback_px`.
    pub fn resolve_or(
        &self,
        fallback_px: f32,
        font_size_px: f32,
        root_font_size_px: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> f32 {
        self.resolve_full(font_size_px, root_font_size_px, viewport_w, viewport_h, 0.0)
            .unwrap_or(fallback_px)
    }

    /// Resolve s parent_size (pro %).
    pub fn resolve_pct(
        &self,
        parent_size_px: f32,
        font_size_px: f32,
        root_font_size_px: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> Option<f32> {
        self.resolve_full(font_size_px, root_font_size_px, viewport_w, viewport_h, parent_size_px)
    }

    fn resolve_full(
        &self,
        font_size_px: f32,
        root_font_size_px: f32,
        vw: f32,
        vh: f32,
        parent_size: f32,
    ) -> Option<f32> {
        match self {
            Length::Px(v) => Some(*v),
            Length::Em(v) => Some(*v * font_size_px),
            Length::Rem(v) => Some(*v * root_font_size_px),
            Length::Vw(v) => Some(*v * vw / 100.0),
            Length::Vh(v) => Some(*v * vh / 100.0),
            Length::Vmin(v) => Some(*v * vw.min(vh) / 100.0),
            Length::Vmax(v) => Some(*v * vw.max(vh) / 100.0),
            Length::Percent(v) => Some(*v * parent_size / 100.0),
            Length::Ch(v) => Some(*v * font_size_px * 0.5),
            Length::Ex(v) => Some(*v * font_size_px * 0.5),
            Length::Auto | Length::None => None,
            Length::MinContent | Length::MaxContent => None,
            Length::FitContent(inner) => inner.resolve_full(font_size_px, root_font_size_px, vw, vh, parent_size),
            Length::Calc(expr) => Some(expr.eval(font_size_px, root_font_size_px, vw, vh, parent_size)),
        }
    }

    /// Parse CSS length string. None pri invalid (cascade decl je flag-uje
    /// jako invalid a discardne).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() { return None; }
        if s == "auto" { return Some(Length::Auto); }
        if s == "none" { return Some(Length::None); }
        if s == "min-content" { return Some(Length::MinContent); }
        if s == "max-content" { return Some(Length::MaxContent); }
        // calc() / fit-content() = stub Auto pro stage 1.
        if s.starts_with("calc(") || s.starts_with("fit-content(") {
            return Some(Length::Auto);  // TODO L5 stage 2
        }
        // Suffix matching (delsi sufix prvni).
        for (sfx, ctor) in &[
            ("rem", Length::Rem as fn(f32) -> Length),
            ("vmin", Length::Vmin),
            ("vmax", Length::Vmax),
            ("em", Length::Em),
            ("px", Length::Px),
            ("vw", Length::Vw),
            ("vh", Length::Vh),
            ("ch", Length::Ch),
            ("ex", Length::Ex),
        ] {
            if let Some(num) = s.strip_suffix(*sfx) {
                let v: f32 = num.trim().parse().ok()?;
                return Some(ctor(v));
            }
        }
        if let Some(num) = s.strip_suffix('%') {
            let v: f32 = num.trim().parse().ok()?;
            return Some(Length::Percent(v));
        }
        // Naked number = px (CSS spec: unitless treated as 0 pro lengths,
        // ale 0 unitless je vse-platne. Acceptujem jako Px pro robustness).
        if let Ok(v) = s.parse::<f32>() {
            return Some(Length::Px(v));
        }
        None
    }
}

impl CalcExpr {
    pub fn eval(&self, fs: f32, rfs: f32, vw: f32, vh: f32, parent: f32) -> f32 {
        match self {
            CalcExpr::Leaf(l) => l.resolve_full(fs, rfs, vw, vh, parent).unwrap_or(0.0),
            CalcExpr::Add(a, b) => a.eval(fs, rfs, vw, vh, parent) + b.eval(fs, rfs, vw, vh, parent),
            CalcExpr::Sub(a, b) => a.eval(fs, rfs, vw, vh, parent) - b.eval(fs, rfs, vw, vh, parent),
            CalcExpr::Mul(a, k) => a.eval(fs, rfs, vw, vh, parent) * k,
            CalcExpr::Div(a, k) => a.eval(fs, rfs, vw, vh, parent) / k,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_px() {
        assert_eq!(Length::parse("100px"), Some(Length::Px(100.0)));
    }

    #[test]
    fn parse_em() {
        assert_eq!(Length::parse("1.5em"), Some(Length::Em(1.5)));
    }

    #[test]
    fn parse_percent() {
        assert_eq!(Length::parse("50%"), Some(Length::Percent(50.0)));
    }

    #[test]
    fn parse_auto() {
        assert_eq!(Length::parse("auto"), Some(Length::Auto));
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(Length::parse("abc"), None);
        assert_eq!(Length::parse(""), None);
    }

    #[test]
    fn resolve_px() {
        assert_eq!(Length::Px(100.0).resolve_or(0.0, 16.0, 16.0, 1024.0, 768.0), 100.0);
    }

    #[test]
    fn resolve_em() {
        // 1.5em pri font-size 20 = 30px
        assert_eq!(Length::Em(1.5).resolve_or(0.0, 20.0, 16.0, 1024.0, 768.0), 30.0);
    }

    #[test]
    fn resolve_rem() {
        // 2rem pri root 16 = 32 (font-size irrelevant)
        assert_eq!(Length::Rem(2.0).resolve_or(0.0, 99.0, 16.0, 1024.0, 768.0), 32.0);
    }

    #[test]
    fn resolve_percent_with_parent() {
        // 50% z parent 400 = 200
        assert_eq!(Length::Percent(50.0).resolve_pct(400.0, 16.0, 16.0, 1024.0, 768.0), Some(200.0));
    }

    #[test]
    fn resolve_vw() {
        // 10vw z viewport 1000 = 100
        assert_eq!(Length::Vw(10.0).resolve_or(0.0, 16.0, 16.0, 1000.0, 800.0), 100.0);
    }

    #[test]
    fn auto_returns_fallback() {
        assert_eq!(Length::Auto.resolve_or(42.0, 16.0, 16.0, 1024.0, 768.0), 42.0);
    }
}
