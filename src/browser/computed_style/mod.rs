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
    pub visibility: Visibility,
    pub cursor: Cursor,

    // ─── Display / Position (batch 2) ─────────────────────────────────
    pub display: Display,
    pub position: PositionKind,
    pub z_index: ZIndex,

    // ─── Text properties (batch 9) ────────────────────────────────────
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,

    // ─── Writing / Box (batch 10) ─────────────────────────────────────
    pub writing_mode: WritingMode,
    pub direction: Direction,
    pub box_sizing: BoxSizing,
    pub pointer_events: PointerEvents,

    // ─── Overflow / Float (batch 11) ──────────────────────────────────
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub float: Float,
    pub clear: Clear,

    // ─── Flex (batch 12) ──────────────────────────────────────────────
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,

    // ─── Flex/Grid alignment (batch 13) ───────────────────────────────
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub align_self: AlignSelf,

    // ─── Flex item + gap (batch 14) ───────────────────────────────────
    pub flex_basis: FlexBasis,
    pub order: i32,
    pub row_gap: Length,
    pub column_gap: Length,

    // ─── Border widths (batch 15) ─────────────────────────────────────
    pub border_top_width: Length,
    pub border_right_width: Length,
    pub border_bottom_width: Length,
    pub border_left_width: Length,

    // ─── Border colors (batch 16) ─────────────────────────────────────
    pub border_top_color: Color,
    pub border_right_color: Color,
    pub border_bottom_color: Color,
    pub border_left_color: Color,

    // ─── Border styles (batch 17) ─────────────────────────────────────
    pub border_top_style: BorderStyle,
    pub border_right_style: BorderStyle,
    pub border_bottom_style: BorderStyle,
    pub border_left_style: BorderStyle,
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
            visibility: Visibility::Visible,
            cursor: Cursor::Auto,
            // background-color initial = transparent. Stejna hodnota jako
            // ComputedStyle.background_color drive (rgba 0,0,0,0).
            display: Display::Inline,           // CSS spec initial pro non-replaced
            position: PositionKind::Static,
            z_index: ZIndex::Auto,
            text_align: TextAlign::Start,
            white_space: WhiteSpace::Normal,
            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Normal,
            writing_mode: WritingMode::HorizontalTb,
            direction: Direction::Ltr,
            box_sizing: BoxSizing::ContentBox,
            pointer_events: PointerEvents::Auto,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            float: Float::None,
            clear: Clear::None,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Nowrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            align_content: AlignContent::Normal,
            align_self: AlignSelf::Auto,
            flex_basis: FlexBasis::Auto,
            order: 0,
            row_gap: Length::Px(0.0),
            column_gap: Length::Px(0.0),
            // CSS spec: border-*-width initial = `medium` = 3px. Engine
            // pouziva 0 default (UA stylesheet) - bez border declaration
            // engine ne-vykresluje border. Spravne: pouzit 0 jako "no
            // explicit border" sentinel; border-style:none neg overrides.
            border_top_width: Length::Px(0.0),
            border_right_width: Length::Px(0.0),
            border_bottom_width: Length::Px(0.0),
            border_left_width: Length::Px(0.0),
            // CSS spec: border-*-color initial = currentColor (= text color).
            // Engine drz currentColor sentinel, paint resolve proti cs.color.
            border_top_color: Color::CurrentColor,
            border_right_color: Color::CurrentColor,
            border_bottom_color: Color::CurrentColor,
            border_left_color: Color::CurrentColor,
            border_top_style: BorderStyle::None,
            border_right_style: BorderStyle::None,
            border_bottom_style: BorderStyle::None,
            border_left_style: BorderStyle::None,
        }
    }
}

/// CSS `border-*-style` (CSS Backgrounds L3 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Hidden,
    Dotted,
    Dashed,
    Solid,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl BorderStyle {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "hidden" => Self::Hidden,
            "dotted" => Self::Dotted,
            "dashed" => Self::Dashed,
            "solid" => Self::Solid,
            "double" => Self::Double,
            "groove" => Self::Groove,
            "ridge" => Self::Ridge,
            "inset" => Self::Inset,
            "outset" => Self::Outset,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Hidden => "hidden",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::Solid => "solid",
            Self::Double => "double",
            Self::Groove => "groove",
            Self::Ridge => "ridge",
            Self::Inset => "inset",
            Self::Outset => "outset",
        }
    }
}

/// CSS `flex-basis` (CSS Flexbox L1 §7.2). Auto | Content | <length>.
#[derive(Debug, Clone, PartialEq)]
pub enum FlexBasis {
    Auto,
    Content,
    Length(Length),
}

impl FlexBasis {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") { return Some(Self::Auto); }
        if t.eq_ignore_ascii_case("content") { return Some(Self::Content); }
        Length::parse(t).map(Self::Length)
    }
    pub fn css_string(&self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Content => "content".into(),
            Self::Length(_) => "<length>".into(),
        }
    }
}

/// CSS `justify-content` (CSS Box Alignment L3 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Normal,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Left,
    Right,
}

impl JustifyContent {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            "stretch" => Self::Stretch,
            "left" => Self::Left,
            "right" => Self::Right,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
            Self::SpaceEvenly => "space-evenly",
            Self::Stretch => "stretch",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// CSS `align-items` (CSS Box Alignment L3 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Normal,
    Stretch,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    Baseline,
    SelfStart,
    SelfEnd,
}

impl AlignItems {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "stretch" => Self::Stretch,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "baseline" => Self::Baseline,
            "self-start" => Self::SelfStart,
            "self-end" => Self::SelfEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Stretch => "stretch",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Baseline => "baseline",
            Self::SelfStart => "self-start",
            Self::SelfEnd => "self-end",
        }
    }
}

/// CSS `align-content` (CSS Box Alignment L3 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignContent {
    Normal,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Baseline,
}

impl AlignContent {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            "stretch" => Self::Stretch,
            "baseline" => Self::Baseline,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
            Self::SpaceEvenly => "space-evenly",
            Self::Stretch => "stretch",
            Self::Baseline => "baseline",
        }
    }
}

/// CSS `align-self` (CSS Box Alignment L3 §5.4). Auto = follow align-items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignSelf {
    Auto,
    Normal,
    Stretch,
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    Baseline,
    SelfStart,
    SelfEnd,
}

impl AlignSelf {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "normal" => Self::Normal,
            "stretch" => Self::Stretch,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "baseline" => Self::Baseline,
            "self-start" => Self::SelfStart,
            "self-end" => Self::SelfEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Normal => "normal",
            Self::Stretch => "stretch",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Baseline => "baseline",
            Self::SelfStart => "self-start",
            Self::SelfEnd => "self-end",
        }
    }
}

/// CSS `flex-direction` (CSS Flexbox L1 §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "row" => Self::Row,
            "row-reverse" => Self::RowReverse,
            "column" => Self::Column,
            "column-reverse" => Self::ColumnReverse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::RowReverse => "row-reverse",
            Self::Column => "column",
            Self::ColumnReverse => "column-reverse",
        }
    }
    pub fn is_row(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }
}

/// CSS `flex-wrap` (CSS Flexbox L1 §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "nowrap" => Self::Nowrap,
            "wrap" => Self::Wrap,
            "wrap-reverse" => Self::WrapReverse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Nowrap => "nowrap",
            Self::Wrap => "wrap",
            Self::WrapReverse => "wrap-reverse",
        }
    }
}

/// CSS `overflow` (CSS Overflow L3 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
    Clip,
}

impl Overflow {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "scroll" => Self::Scroll,
            "auto" => Self::Auto,
            "clip" => Self::Clip,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Scroll => "scroll",
            Self::Auto => "auto",
            Self::Clip => "clip",
        }
    }
    /// Pri Auto/Scroll/Hidden je box scrollable nebo clipped.
    pub fn is_scrollable(self) -> bool {
        matches!(self, Self::Auto | Self::Scroll)
    }
}

/// CSS `float` (CSS Floats L1 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
    InlineStart,
    InlineEnd,
}

impl Float {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "left" => Self::Left,
            "right" => Self::Right,
            "inline-start" => Self::InlineStart,
            "inline-end" => Self::InlineEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Left => "left",
            Self::Right => "right",
            Self::InlineStart => "inline-start",
            Self::InlineEnd => "inline-end",
        }
    }
}

/// CSS `clear` (CSS Floats L1 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
    InlineStart,
    InlineEnd,
}

impl Clear {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "left" => Self::Left,
            "right" => Self::Right,
            "both" => Self::Both,
            "inline-start" => Self::InlineStart,
            "inline-end" => Self::InlineEnd,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Left => "left",
            Self::Right => "right",
            Self::Both => "both",
            Self::InlineStart => "inline-start",
            Self::InlineEnd => "inline-end",
        }
    }
}

/// CSS `writing-mode` (CSS Writing Modes L3 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "horizontal-tb" => Self::HorizontalTb,
            "vertical-rl" => Self::VerticalRl,
            "vertical-lr" => Self::VerticalLr,
            "sideways-rl" => Self::SidewaysRl,
            "sideways-lr" => Self::SidewaysLr,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::HorizontalTb => "horizontal-tb",
            Self::VerticalRl => "vertical-rl",
            Self::VerticalLr => "vertical-lr",
            Self::SidewaysRl => "sideways-rl",
            Self::SidewaysLr => "sideways-lr",
        }
    }
}

/// CSS `direction` (CSS Writing Modes L3 §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "ltr" => Self::Ltr,
            "rtl" => Self::Rtl,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

/// CSS `box-sizing` (CSS UI L4 §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

impl BoxSizing {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "content-box" => Self::ContentBox,
            "border-box" => Self::BorderBox,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::ContentBox => "content-box",
            Self::BorderBox => "border-box",
        }
    }
}

/// CSS `pointer-events` (CSS UI L4 §3.1). Subset - mostly Auto vs None.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    Auto,
    None,
}

impl PointerEvents {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "none" => Self::None,
            // SVG values (visiblePainted, visibleFill, etc.) treat as Auto
            // pro non-SVG context. Vsechny ostatni neznamy -> None match
            // failure.
            "visible" | "visiblepainted" | "visiblefill" | "visiblestroke"
                | "painted" | "fill" | "stroke" | "all" => Self::Auto,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }
}

/// CSS `text-align` (CSS Text L4 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
    MatchParent,
}

impl TextAlign {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "start" => Self::Start,
            "end" => Self::End,
            "left" => Self::Left,
            "right" => Self::Right,
            "center" => Self::Center,
            "justify" => Self::Justify,
            "match-parent" => Self::MatchParent,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
            Self::MatchParent => "match-parent",
        }
    }
}

/// CSS `white-space` (CSS Text L4 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

impl WhiteSpace {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "nowrap" => Self::Nowrap,
            "pre" => Self::Pre,
            "pre-wrap" => Self::PreWrap,
            "pre-line" => Self::PreLine,
            "break-spaces" => Self::BreakSpaces,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Nowrap => "nowrap",
            Self::Pre => "pre",
            Self::PreWrap => "pre-wrap",
            Self::PreLine => "pre-line",
            Self::BreakSpaces => "break-spaces",
        }
    }
}

/// CSS `word-break` (CSS Text L4 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBreak {
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

impl WordBreak {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "break-all" => Self::BreakAll,
            "keep-all" => Self::KeepAll,
            "break-word" => Self::BreakWord,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::BreakAll => "break-all",
            Self::KeepAll => "keep-all",
            Self::BreakWord => "break-word",
        }
    }
}

/// CSS `overflow-wrap` / `word-wrap` (CSS Text L4 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowWrap {
    Normal,
    BreakWord,
    Anywhere,
}

impl OverflowWrap {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "normal" => Self::Normal,
            "break-word" => Self::BreakWord,
            "anywhere" => Self::Anywhere,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::BreakWord => "break-word",
            Self::Anywhere => "anywhere",
        }
    }
}

/// CSS `visibility` (CSS Display L3 §11). Inherited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapse,
}

impl Visibility {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "collapse" => Self::Collapse,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Collapse => "collapse",
        }
    }
}

/// CSS `display` (CSS Display L3). Subset reflektujici layout backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Contents,
    Table,
    TableRow,
    TableCell,
    TableHeaderCell,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableColumn,
    TableColumnGroup,
    TableCaption,
    ListItem,
    Ruby,
}

impl Display {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "none" => Self::None,
            "block" => Self::Block,
            "inline" => Self::Inline,
            "inline-block" => Self::InlineBlock,
            "flex" => Self::Flex,
            "inline-flex" => Self::InlineFlex,
            "grid" => Self::Grid,
            "inline-grid" => Self::InlineGrid,
            "contents" => Self::Contents,
            "table" => Self::Table,
            "table-row" => Self::TableRow,
            "table-cell" => Self::TableCell,
            "table-header-cell" => Self::TableHeaderCell,
            "table-row-group" => Self::TableRowGroup,
            "table-header-group" => Self::TableHeaderGroup,
            "table-footer-group" => Self::TableFooterGroup,
            "table-column" => Self::TableColumn,
            "table-column-group" => Self::TableColumnGroup,
            "table-caption" => Self::TableCaption,
            "list-item" => Self::ListItem,
            "ruby" => Self::Ruby,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::Flex => "flex",
            Self::InlineFlex => "inline-flex",
            Self::Grid => "grid",
            Self::InlineGrid => "inline-grid",
            Self::Contents => "contents",
            Self::Table => "table",
            Self::TableRow => "table-row",
            Self::TableCell => "table-cell",
            Self::TableHeaderCell => "table-header-cell",
            Self::TableRowGroup => "table-row-group",
            Self::TableHeaderGroup => "table-header-group",
            Self::TableFooterGroup => "table-footer-group",
            Self::TableColumn => "table-column",
            Self::TableColumnGroup => "table-column-group",
            Self::TableCaption => "table-caption",
            Self::ListItem => "list-item",
            Self::Ruby => "ruby",
        }
    }
}

/// CSS `position` (CSS Position L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionKind {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl PositionKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "static" => Self::Static,
            "relative" => Self::Relative,
            "absolute" => Self::Absolute,
            "fixed" => Self::Fixed,
            "sticky" => Self::Sticky,
            _ => return None,
        })
    }
    pub fn css_string(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Relative => "relative",
            Self::Absolute => "absolute",
            Self::Fixed => "fixed",
            Self::Sticky => "sticky",
        }
    }
}

/// CSS `z-index` (CSS Position L3 §9.9). `auto` ne stack context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZIndex {
    Auto,
    Value(i32),
}

impl ZIndex {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("auto") { return Some(Self::Auto); }
        t.parse::<i32>().ok().map(Self::Value)
    }
    pub fn css_string(self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Value(n) => n.to_string(),
        }
    }
}

/// CSS `cursor` (CSS UI L4 §8.1). Inherited. Subset typed; ostatni Custom.
#[derive(Debug, Clone, PartialEq)]
pub enum Cursor {
    Auto,
    Default,
    Pointer,
    Text,
    Move,
    NotAllowed,
    Grab,
    Grabbing,
    Wait,
    Help,
    Crosshair,
    Progress,
    Custom(String),     // url() nebo neznamy keyword
}

impl Cursor {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "default" => Self::Default,
            "pointer" => Self::Pointer,
            "text" => Self::Text,
            "move" => Self::Move,
            "not-allowed" => Self::NotAllowed,
            "grab" => Self::Grab,
            "grabbing" => Self::Grabbing,
            "wait" => Self::Wait,
            "help" => Self::Help,
            "crosshair" => Self::Crosshair,
            "progress" => Self::Progress,
            other => Self::Custom(other.to_string()),
        }
    }
    pub fn css_string(&self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Default => "default".into(),
            Self::Pointer => "pointer".into(),
            Self::Text => "text".into(),
            Self::Move => "move".into(),
            Self::NotAllowed => "not-allowed".into(),
            Self::Grab => "grab".into(),
            Self::Grabbing => "grabbing".into(),
            Self::Wait => "wait".into(),
            Self::Help => "help".into(),
            Self::Crosshair => "crosshair".into(),
            Self::Progress => "progress".into(),
            Self::Custom(s) => s.clone(),
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
