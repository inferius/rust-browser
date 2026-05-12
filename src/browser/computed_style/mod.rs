//! Typed ComputedStyle per CSS spec.
//!
//! L5 refactor: nahrazuje `HashMap<String, String>` cascade output typed
//! struct. Cascade pln-parsuje hodnoty pri build, ne pri kazdem cteni v
//! build_box_inner.
//!
//! Vyhody:
//! - Perf: zadny hash lookup + re-parse per node read
//! - Type safety: PropertyId enum + typed value (compile-time check)
//! - Pamet: pole struct < HashMap + String alloc per property
//! - Cleaner code: `cs.background_color` vs `s.get("background-color").and_then(parse_color)`
//!
//! Naming: snake_case 1:1 mapping CSS kebab-case property names.
//! (CSS `background-color` -> Rust `background_color`). Pro JS
//! getComputedStyle: snake -> camel mapper (background_color -> backgroundColor).
//!
//! Stage 1 (current): struct kostry + zakladni typy. Naplnovani postupne
//! z cascade.rs (dual-write s puvodni HashMap).

pub mod color;
pub mod length;
pub mod property;
pub mod cascade_decl;

pub use color::Color;
pub use length::Length;
pub use property::PropertyId;
pub use cascade_decl::{CascadeDecl, CascadeOrigin, Specificity};

use std::collections::HashMap;

/// Per-element typed computed style. Node ptr (Rc::as_ptr usize) -> resolved
/// ComputedStyle. L5 stage 2c: definovan, naplnovan v stage 3 dual-write
/// z cascade. layout/paint zatim cte z StyleMap (HashMap<String,String>).
pub type ComputedStyleMap = std::collections::HashMap<usize, ComputedStyle>;

/// Per-element collected declarations (vsechny, vc. invalid). Pro devtools
/// strikethrough display - layout neuses, ulozeno cisto pro devtools UI
/// (L5 stage 5).
pub type DeclarationsMap = std::collections::HashMap<usize, Vec<CascadeDecl>>;

/// Cascade output bundle. Dual-write step zachova obe puvodni HashMap +
/// nove typed mapping pro postupnou migraci.
#[derive(Debug, Default)]
pub struct CascadeOutput {
    /// Legacy stringly mapping. Layout/paint/animations zatim cti odsud.
    /// Po stage 4 dropnout.
    pub style_map: HashMap<usize, HashMap<String, String>>,
    /// Typed computed styles. Po stage 3 plnit; po stage 4 main API.
    pub computed: ComputedStyleMap,
    /// All declarations vc. invalid. Pro devtools (stage 5).
    pub declarations: DeclarationsMap,
}

/// Resolved computed style per element (CSS Cascade L4 §4.1 specified->
/// computed value mapping).
///
/// Pole jsou pre-parsed typed values. Pro hodnoty zavisle na kontextu
/// (% delky, em/rem) zachovavame Length enum a resolver volame az pri
/// layout dispatch s realnym parent/viewport sizem.
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    // ─── Color / Background ────────────────────────────────────────────
    pub color: Color,                          // CSS `color` (text)
    pub background_color: Color,               // CSS `background-color`
    // (background_image, position, size, repeat etc. zatim HashMap fallback)

    // ─── Font ──────────────────────────────────────────────────────────
    pub font_size: Length,                     // computed length, default 16px
    pub font_weight: u32,                      // 1..1000 (normal=400, bold=700)
    pub font_style_italic: bool,               // italic | oblique = true
    pub line_height: LineHeight,
    pub font_family: Vec<FontFamily>,

    // ─── Box model (Length keep unit for percent resolution) ──────────
    pub margin_top: Length,
    pub margin_right: Length,
    pub margin_bottom: Length,
    pub margin_left: Length,
    pub padding_top: Length,
    pub padding_right: Length,
    pub padding_bottom: Length,
    pub padding_left: Length,
    pub width: Length,                         // Auto = no explicit
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,

    // ─── Position offset ──────────────────────────────────────────────
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,

    // ─── Visual ─────────────────────────────────────────────────────────
    pub opacity: f32,                          // 0..1, default 1
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self::initial()
    }
}

impl ComputedStyle {
    /// CSS spec initial value pro vsechny properties.
    ///
    /// Inherited properties (color, font-*, line-height) maji "inherit"
    /// jako vychozi pri cascade resolution; tady vracime spec initial
    /// pro root element (drive nez cascade inheritance).
    pub fn initial() -> Self {
        Self {
            color: Color::Rgba { r: 0, g: 0, b: 0, a: 255 },
            background_color: Color::Rgba { r: 0, g: 0, b: 0, a: 0 }, // transparent
            font_size: Length::Px(16.0),
            font_weight: 400,
            font_style_italic: false,
            line_height: LineHeight::Normal,
            font_family: Vec::new(),
            margin_top: Length::Px(0.0),
            margin_right: Length::Px(0.0),
            margin_bottom: Length::Px(0.0),
            margin_left: Length::Px(0.0),
            padding_top: Length::Px(0.0),
            padding_right: Length::Px(0.0),
            padding_bottom: Length::Px(0.0),
            padding_left: Length::Px(0.0),
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            max_width: Length::None,
            max_height: Length::None,
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            opacity: 1.0,
        }
    }
}

/// CSS `line-height`: `normal` | <number> | <length>.
/// Number = multiplier on font-size; Length = absolute computed.
#[derive(Debug, Clone, PartialEq)]
pub enum LineHeight {
    Normal,
    Multiplier(f32),
    Length(Length),
}

impl LineHeight {
    pub fn resolve(&self, font_size_px: f32) -> f32 {
        match self {
            LineHeight::Normal => font_size_px * 1.2,
            LineHeight::Multiplier(m) => font_size_px * m,
            LineHeight::Length(l) => l.resolve_or(0.0, font_size_px, font_size_px, 1024.0, 768.0),
        }
    }
}

/// CSS `font-family` token: konkretni font nebo generic alias.
#[derive(Debug, Clone, PartialEq)]
pub enum FontFamily {
    Named(String),                              // "Arial", "Helvetica"
    Generic(GenericFamily),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
    UiSerif,
    UiSansSerif,
    UiMonospace,
    UiRounded,
    Emoji,
    Math,
    Fangsong,
}

impl GenericFamily {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "serif" => Self::Serif,
            "sans-serif" => Self::SansSerif,
            "monospace" => Self::Monospace,
            "cursive" => Self::Cursive,
            "fantasy" => Self::Fantasy,
            "system-ui" => Self::SystemUi,
            "ui-serif" => Self::UiSerif,
            "ui-sans-serif" => Self::UiSansSerif,
            "ui-monospace" => Self::UiMonospace,
            "ui-rounded" => Self::UiRounded,
            "emoji" => Self::Emoji,
            "math" => Self::Math,
            "fangsong" => Self::Fangsong,
            _ => return None,
        })
    }
}
