//! Typed CSS <length-percentage> + sentinel hodnoty (auto / none).
//!
//! Nahrazuje pattern `min_width_v: String` + `parse_length(s)` + `ends_with('%')`
//! checks rozesetych po codebase. Vyhody:
//! - Parse-once: cascade vola `CssLength::parse(value)` jednou.
//! - Type safety: nelze omylem pridat "garbage" + silent 0 fallback.
//! - `resolve(ctx)` na callsite s explicit context (parent_size, font_size, viewport).
//! - `Auto` / `None` distinct od `Px(0.0)`.
//!
//! Pokryti:
//! - px / em / rem
//! - %
//! - vw / vh / vmin / vmax
//! - pt / pc / in / cm / mm
//! - auto (size = automatic)
//! - none (no constraint - pro max-width / max-height)
//! - calc() - opaque string fallback (TODO proper expression tree)

use std::fmt;

/// Resolve context. Posila se do `CssLength::resolve` aby relativni jednotky
/// (em, %, vw, vh) byly konvertovany na px.
#[derive(Debug, Clone, Copy)]
pub struct ResolveCtx {
    /// Parent size pro `%` resolution (typicky inner_w nebo inner_h).
    pub parent_size: f32,
    /// Element font-size pro `em`.
    pub font_size: f32,
    /// Root font-size pro `rem`.
    pub root_font_size: f32,
    /// Viewport width pro `vw`.
    pub viewport_w: f32,
    /// Viewport height pro `vh`.
    pub viewport_h: f32,
}

impl Default for ResolveCtx {
    fn default() -> Self {
        // CRITICAL: viewport_w/h ctene z MATH_VIEWPORT thread-local (set
        // cascade_with_viewport pri layout). Bez tohoto vsechny callsity ktery
        // pouzily `..Default::default()` (cca 20 mist v flex/grid/block) by
        // resolvovaly `100vw` / `50vh` proti HARDCODED 1024/768 = max-width
        // ignorovalo realny viewport -> rozjete sizy na 1280px window.
        let (vw, vh) = super::super::cascade::MATH_VIEWPORT.with(|c| *c.borrow());
        // Fallback (cascade_with_viewport jeste ne-volane): 1024/768.
        let vw = if vw > 0.0 { vw } else { 1024.0 };
        let vh = if vh > 0.0 { vh } else { 768.0 };
        ResolveCtx {
            parent_size: 0.0,
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_w: vw,
            viewport_h: vh,
        }
    }
}

impl ResolveCtx {
    /// Stejny ctx ale s jinym parent_size (handy pri swap mezi width / height axis).
    pub fn with_parent(self, parent_size: f32) -> Self {
        ResolveCtx { parent_size, ..self }
    }
}

/// Parsed CSS length nebo procentualni hodnota. Default = Auto.
#[derive(Debug, Clone, PartialEq)]
pub enum CssLength {
    /// Absolutni / relativni length k font/viewport.
    Px(f32),
    Em(f32),
    Rem(f32),
    /// % parent size (0..1 ratio; 100% = 1.0).
    Percent(f32),
    Vw(f32),
    Vh(f32),
    VMin(f32),
    VMax(f32),
    /// Default - `width: auto`, `min-width: auto`, atd.
    Auto,
    /// `max-width: none` - bez horni meze.
    None,
    /// calc(...) - opaque hodnota, resolve pres legacy parse_length_ctx
    /// (TODO proper expression tree pri rebuild calc parsing).
    Calc(String),
}

impl Default for CssLength {
    fn default() -> Self { CssLength::Auto }
}

impl fmt::Display for CssLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CssLength::Px(v) => write!(f, "{}px", v),
            CssLength::Em(v) => write!(f, "{}em", v),
            CssLength::Rem(v) => write!(f, "{}rem", v),
            CssLength::Percent(v) => write!(f, "{}%", v * 100.0),
            CssLength::Vw(v) => write!(f, "{}vw", v),
            CssLength::Vh(v) => write!(f, "{}vh", v),
            CssLength::VMin(v) => write!(f, "{}vmin", v),
            CssLength::VMax(v) => write!(f, "{}vmax", v),
            CssLength::Auto => write!(f, "auto"),
            CssLength::None => write!(f, "none"),
            CssLength::Calc(s) => write!(f, "calc({})", s),
        }
    }
}

impl CssLength {
    /// Parse z CSS value string. Empty / unknown -> Auto.
    /// Tolerantni: zachova case-insensitive jednotky.
    pub fn parse(s: &str) -> Self {
        let v = s.trim();
        if v.is_empty() || v.eq_ignore_ascii_case("auto") {
            return CssLength::Auto;
        }
        if v.eq_ignore_ascii_case("none") {
            return CssLength::None;
        }
        // calc(...) opaque - resolve cestou legacy parse_length_ctx.
        let lc = v.to_ascii_lowercase();
        if lc.starts_with("calc(") {
            // Strip "calc(" + ")".
            let inner = if let Some(stripped) = v.strip_prefix("calc(").or_else(|| v.strip_prefix("CALC(")) {
                stripped.trim_end_matches(')').to_string()
            } else { v.to_string() };
            return CssLength::Calc(inner);
        }
        // Try suffixes longest-first (rem > em).
        if let Some(num) = strip_unit_ci(v, "rem") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Rem(n); }
        }
        if let Some(num) = strip_unit_ci(v, "em") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Em(n); }
        }
        if let Some(num) = strip_unit_ci(v, "vmin") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::VMin(n); }
        }
        if let Some(num) = strip_unit_ci(v, "vmax") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::VMax(n); }
        }
        if let Some(num) = strip_unit_ci(v, "vw") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Vw(n); }
        }
        if let Some(num) = strip_unit_ci(v, "vh") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Vh(n); }
        }
        if let Some(num) = strip_unit_ci(v, "px") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Px(n); }
        }
        // pt = 1.3333 px (96 dpi / 72 pt-per-inch).
        if let Some(num) = strip_unit_ci(v, "pt") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Px(n * 96.0 / 72.0); }
        }
        // pc = 12pt = 16 px.
        if let Some(num) = strip_unit_ci(v, "pc") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Px(n * 16.0); }
        }
        // in = 96 px.
        if let Some(num) = strip_unit_ci(v, "in") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Px(n * 96.0); }
        }
        // cm = 96/2.54 px ~= 37.795.
        if let Some(num) = strip_unit_ci(v, "cm") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Px(n * 96.0 / 2.54); }
        }
        // mm = cm / 10.
        if let Some(num) = strip_unit_ci(v, "mm") {
            if let Ok(n) = num.parse::<f32>() { return CssLength::Px(n * 96.0 / 25.4); }
        }
        if let Some(pct_str) = v.strip_suffix('%') {
            if let Ok(p) = pct_str.parse::<f32>() {
                return CssLength::Percent(p / 100.0);
            }
        }
        // Unitless number - treat as px (HTML attr width="100" passed as "100" string).
        if let Ok(n) = v.parse::<f32>() {
            return CssLength::Px(n);
        }
        CssLength::Auto
    }

    /// Resolve na konkretni px. `Auto` / `None` -> 0 (caller pouzije
    /// `is_specified()` na rozliseni od skutecne 0).
    pub fn resolve(&self, ctx: &ResolveCtx) -> f32 {
        match self {
            CssLength::Px(v) => *v,
            CssLength::Em(v) => *v * ctx.font_size,
            CssLength::Rem(v) => *v * ctx.root_font_size,
            CssLength::Percent(p) => ctx.parent_size * p,
            CssLength::Vw(v) => *v * ctx.viewport_w / 100.0,
            CssLength::Vh(v) => *v * ctx.viewport_h / 100.0,
            CssLength::VMin(v) => *v * ctx.viewport_w.min(ctx.viewport_h) / 100.0,
            CssLength::VMax(v) => *v * ctx.viewport_w.max(ctx.viewport_h) / 100.0,
            CssLength::Auto | CssLength::None => 0.0,
            CssLength::Calc(s) => {
                // Delegate na legacy parser pres viewport ctx.
                super::parse_length_ctx(s, ctx.viewport_w, ctx.viewport_h, ctx.parent_size)
            }
        }
    }

    /// Resolve, ale `None` -> f32::INFINITY (pro max-width / max-height
    /// kde "no max" znamena bez horni meze).
    pub fn resolve_max(&self, ctx: &ResolveCtx) -> f32 {
        match self {
            CssLength::None => f32::INFINITY,
            CssLength::Auto => f32::INFINITY,
            _ => self.resolve(ctx),
        }
    }

    pub fn is_auto(&self) -> bool { matches!(self, CssLength::Auto) }
    pub fn is_none(&self) -> bool { matches!(self, CssLength::None) }
    pub fn is_specified(&self) -> bool {
        !matches!(self, CssLength::Auto | CssLength::None)
    }
    pub fn is_percent(&self) -> bool { matches!(self, CssLength::Percent(_)) }
    pub fn is_zero(&self) -> bool {
        match self {
            CssLength::Px(v) | CssLength::Em(v) | CssLength::Rem(v)
            | CssLength::Vw(v) | CssLength::Vh(v) | CssLength::VMin(v) | CssLength::VMax(v) => *v == 0.0,
            CssLength::Percent(p) => *p == 0.0,
            CssLength::Auto | CssLength::None | CssLength::Calc(_) => false,
        }
    }
}

/// Strip unit suffix case-insensitive. Vraci numeric part nebo None.
fn strip_unit_ci<'a>(s: &'a str, unit: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let unit_b = unit.as_bytes();
    if bytes.len() < unit_b.len() { return None; }
    let split = bytes.len() - unit_b.len();
    let tail = &bytes[split..];
    if tail.len() != unit_b.len() { return None; }
    for (a, b) in tail.iter().zip(unit_b.iter()) {
        if !a.eq_ignore_ascii_case(b) { return None; }
    }
    Some(&s[..split])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_parent(p: f32) -> ResolveCtx {
        ResolveCtx { parent_size: p, ..Default::default() }
    }

    #[test]
    fn parse_px() {
        assert_eq!(CssLength::parse("100px"), CssLength::Px(100.0));
        assert_eq!(CssLength::parse("  10.5px "), CssLength::Px(10.5));
    }

    #[test]
    fn parse_percent() {
        assert_eq!(CssLength::parse("50%"), CssLength::Percent(0.5));
        assert_eq!(CssLength::parse("100%"), CssLength::Percent(1.0));
    }

    #[test]
    fn parse_em_rem() {
        assert_eq!(CssLength::parse("1.5em"), CssLength::Em(1.5));
        assert_eq!(CssLength::parse("2rem"), CssLength::Rem(2.0));
    }

    #[test]
    fn parse_viewport_units() {
        assert_eq!(CssLength::parse("50vw"), CssLength::Vw(50.0));
        assert_eq!(CssLength::parse("100vh"), CssLength::Vh(100.0));
    }

    #[test]
    fn parse_auto_none() {
        assert_eq!(CssLength::parse("auto"), CssLength::Auto);
        assert_eq!(CssLength::parse("AUTO"), CssLength::Auto);
        assert_eq!(CssLength::parse("none"), CssLength::None);
        assert_eq!(CssLength::parse(""), CssLength::Auto);
    }

    #[test]
    fn parse_pt_pc_in_cm_mm() {
        // pt = 4/3 px
        let v = CssLength::parse("72pt");
        if let CssLength::Px(p) = v { assert!((p - 96.0).abs() < 0.1); } else { panic!(); }
        // in = 96 px
        if let CssLength::Px(p) = CssLength::parse("1in") { assert!((p - 96.0).abs() < 0.1); } else { panic!(); }
    }

    #[test]
    fn parse_unitless_to_px() {
        // HTML attr width="100" passed as "100" - treat as px.
        assert_eq!(CssLength::parse("100"), CssLength::Px(100.0));
    }

    #[test]
    fn resolve_percent() {
        let l = CssLength::parse("50%");
        assert_eq!(l.resolve(&ctx_with_parent(200.0)), 100.0);
    }

    #[test]
    fn resolve_em() {
        let l = CssLength::parse("1.5em");
        let ctx = ResolveCtx { font_size: 16.0, ..Default::default() };
        assert_eq!(l.resolve(&ctx), 24.0);
    }

    #[test]
    fn resolve_max_none_is_infinity() {
        let l = CssLength::parse("none");
        assert!(l.resolve_max(&Default::default()).is_infinite());
    }

    #[test]
    fn resolve_max_specified_is_finite() {
        let l = CssLength::parse("100%");
        let ctx = ctx_with_parent(500.0);
        assert_eq!(l.resolve_max(&ctx), 500.0);
        assert!(!l.resolve_max(&ctx).is_infinite());
    }

    #[test]
    fn is_specified_distinguishes_auto_from_zero() {
        assert!(!CssLength::Auto.is_specified());
        assert!(!CssLength::None.is_specified());
        assert!(CssLength::Px(0.0).is_specified());
        assert!(CssLength::Percent(0.0).is_specified());
    }
}
