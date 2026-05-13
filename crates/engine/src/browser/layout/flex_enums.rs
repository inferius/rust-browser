//! Typed CSS Flex layout properties. Nahrazuje `flex_direction: String`,
//! `flex_wrap: String`, `justify_content: String`, `align_items: String`,
//! `align_self: String`, `align_content: String`, `justify_self: String`,
//! `justify_items: String` v LayoutBox.
//!
//! Drive flex.rs per-frame `parse_flex_direction(&bx.flex_direction)` etc.
//! Po refactoru cascade parsuje JEDNOU pri cascade, hot loop reads typed.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for FlexDirection {
    fn default() -> Self { FlexDirection::Row }
}

impl FlexDirection {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "row-reverse" => Self::RowReverse,
            "column" => Self::Column,
            "column-reverse" => Self::ColumnReverse,
            _ => Self::Row,
        }
    }
    #[inline]
    pub fn is_row(&self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }
    #[inline]
    pub fn is_reverse(&self) -> bool {
        matches!(self, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
    }
}

impl fmt::Display for FlexDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FlexDirection::Row => "row",
            FlexDirection::RowReverse => "row-reverse",
            FlexDirection::Column => "column",
            FlexDirection::ColumnReverse => "column-reverse",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl Default for FlexWrap {
    fn default() -> Self { FlexWrap::NoWrap }
}

impl FlexWrap {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "wrap" => Self::Wrap,
            "wrap-reverse" => Self::WrapReverse,
            _ => Self::NoWrap,
        }
    }
}

impl fmt::Display for FlexWrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FlexWrap::NoWrap => "nowrap",
            FlexWrap::Wrap => "wrap",
            FlexWrap::WrapReverse => "wrap-reverse",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Start,
    End,
}

impl Default for JustifyContent {
    fn default() -> Self { JustifyContent::FlexStart }
}

impl JustifyContent {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "flex-end" => Self::FlexEnd,
            "end" => Self::End,
            "start" => Self::Start,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            _ => Self::FlexStart,
        }
    }
}

impl fmt::Display for JustifyContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            JustifyContent::FlexStart => "flex-start",
            JustifyContent::FlexEnd => "flex-end",
            JustifyContent::Center => "center",
            JustifyContent::SpaceBetween => "space-between",
            JustifyContent::SpaceAround => "space-around",
            JustifyContent::SpaceEvenly => "space-evenly",
            JustifyContent::Start => "start",
            JustifyContent::End => "end",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignItems {
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
}

impl Default for AlignItems {
    fn default() -> Self { AlignItems::Stretch }  // CSS default for flex
}

impl AlignItems {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "flex-start" | "start" => Self::FlexStart,
            "flex-end" | "end" => Self::FlexEnd,
            "center" => Self::Center,
            "baseline" => Self::Baseline,
            "stretch" => Self::Stretch,
            _ => Self::Stretch,
        }
    }
}

impl fmt::Display for AlignItems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AlignItems::FlexStart => "flex-start",
            AlignItems::FlexEnd => "flex-end",
            AlignItems::Center => "center",
            AlignItems::Stretch => "stretch",
            AlignItems::Baseline => "baseline",
        })
    }
}

/// AlignSelf per-item: AlignItems + Auto sentinel (= use parent's align-items).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignSelf {
    Auto,
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
}

impl Default for AlignSelf {
    fn default() -> Self { AlignSelf::Auto }
}

impl AlignSelf {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Self::Auto,
            "flex-start" | "start" => Self::FlexStart,
            "flex-end" | "end" => Self::FlexEnd,
            "center" => Self::Center,
            "baseline" => Self::Baseline,
            "stretch" => Self::Stretch,
            _ => Self::Auto,
        }
    }
    #[inline]
    pub fn is_auto(&self) -> bool { matches!(self, AlignSelf::Auto) }
    /// Resolve do AlignItems pres parent align-items (pri Auto).
    #[inline]
    pub fn resolve(&self, parent: AlignItems) -> AlignItems {
        match self {
            AlignSelf::Auto => parent,
            AlignSelf::FlexStart => AlignItems::FlexStart,
            AlignSelf::FlexEnd => AlignItems::FlexEnd,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::Stretch => AlignItems::Stretch,
            AlignSelf::Baseline => AlignItems::Baseline,
        }
    }
}

impl fmt::Display for AlignSelf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AlignSelf::Auto => "auto",
            AlignSelf::FlexStart => "flex-start",
            AlignSelf::FlexEnd => "flex-end",
            AlignSelf::Center => "center",
            AlignSelf::Stretch => "stretch",
            AlignSelf::Baseline => "baseline",
        })
    }
}

/// AlignContent (multi-line flex / grid): JustifyContent variants + Stretch + Normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignContent {
    Normal,
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Start,
    End,
}

impl Default for AlignContent {
    fn default() -> Self { AlignContent::Normal }
}

impl AlignContent {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "start" => Self::Start,
            "end" => Self::End,
            "center" => Self::Center,
            "stretch" => Self::Stretch,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            _ => Self::Normal,
        }
    }
    #[inline]
    pub fn is_normal_or_stretch(&self) -> bool {
        matches!(self, AlignContent::Normal | AlignContent::Stretch)
    }
}

impl fmt::Display for AlignContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AlignContent::Normal => "normal",
            AlignContent::FlexStart => "flex-start",
            AlignContent::FlexEnd => "flex-end",
            AlignContent::Center => "center",
            AlignContent::Stretch => "stretch",
            AlignContent::SpaceBetween => "space-between",
            AlignContent::SpaceAround => "space-around",
            AlignContent::SpaceEvenly => "space-evenly",
            AlignContent::Start => "start",
            AlignContent::End => "end",
        })
    }
}

/// CSS object-fit: replaced element scaling within content box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectFit {
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl Default for ObjectFit {
    fn default() -> Self { ObjectFit::Fill }
}

impl ObjectFit {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "contain" => Self::Contain,
            "cover" => Self::Cover,
            "none" => Self::None,
            "scale-down" => Self::ScaleDown,
            _ => Self::Fill,
        }
    }
}

impl fmt::Display for ObjectFit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ObjectFit::Fill => "fill",
            ObjectFit::Contain => "contain",
            ObjectFit::Cover => "cover",
            ObjectFit::None => "none",
            ObjectFit::ScaleDown => "scale-down",
        })
    }
}

/// CSS border-collapse: separate (default) | collapse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BorderCollapse {
    Separate,
    Collapse,
}

impl Default for BorderCollapse {
    fn default() -> Self { BorderCollapse::Separate }
}

impl BorderCollapse {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "collapse" => Self::Collapse,
            _ => Self::Separate,
        }
    }
    #[inline]
    pub fn is_collapse(&self) -> bool { matches!(self, Self::Collapse) }
}

impl fmt::Display for BorderCollapse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BorderCollapse::Separate => "separate",
            BorderCollapse::Collapse => "collapse",
        })
    }
}

/// CSS table-layout: auto (default) | fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableLayout {
    Auto,
    Fixed,
}

impl Default for TableLayout {
    fn default() -> Self { TableLayout::Auto }
}

impl TableLayout {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "fixed" => Self::Fixed,
            _ => Self::Auto,
        }
    }
}

impl fmt::Display for TableLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TableLayout::Auto => "auto",
            TableLayout::Fixed => "fixed",
        })
    }
}

/// CSS image-rendering hint pro upscaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageRendering {
    Auto,
    Smooth,
    HighQuality,
    CrispEdges,
    Pixelated,
}

impl Default for ImageRendering {
    fn default() -> Self { ImageRendering::Auto }
}

impl ImageRendering {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "smooth" => Self::Smooth,
            "high-quality" => Self::HighQuality,
            "crisp-edges" => Self::CrispEdges,
            "pixelated" => Self::Pixelated,
            _ => Self::Auto,
        }
    }
    #[inline]
    pub fn is_pixelated(&self) -> bool { matches!(self, Self::Pixelated | Self::CrispEdges) }
}

impl fmt::Display for ImageRendering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ImageRendering::Auto => "auto",
            ImageRendering::Smooth => "smooth",
            ImageRendering::HighQuality => "high-quality",
            ImageRendering::CrispEdges => "crisp-edges",
            ImageRendering::Pixelated => "pixelated",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

impl Default for BoxSizing {
    fn default() -> Self { BoxSizing::ContentBox }
}

impl BoxSizing {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "border-box" => Self::BorderBox,
            _ => Self::ContentBox,
        }
    }
    #[inline]
    pub fn is_border_box(&self) -> bool { matches!(self, Self::BorderBox) }
}

impl fmt::Display for BoxSizing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BoxSizing::ContentBox => "content-box",
            BoxSizing::BorderBox => "border-box",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flex_direction_parse() {
        assert_eq!(FlexDirection::parse("row"), FlexDirection::Row);
        assert_eq!(FlexDirection::parse("ROW-REVERSE"), FlexDirection::RowReverse);
        assert_eq!(FlexDirection::parse("column"), FlexDirection::Column);
        assert_eq!(FlexDirection::parse(""), FlexDirection::Row);
        assert_eq!(FlexDirection::parse("garbage"), FlexDirection::Row);
    }

    #[test]
    fn flex_direction_predicates() {
        assert!(FlexDirection::Row.is_row());
        assert!(FlexDirection::RowReverse.is_row());
        assert!(!FlexDirection::Column.is_row());
        assert!(FlexDirection::RowReverse.is_reverse());
        assert!(FlexDirection::ColumnReverse.is_reverse());
        assert!(!FlexDirection::Row.is_reverse());
    }

    #[test]
    fn flex_wrap_parse() {
        assert_eq!(FlexWrap::parse("nowrap"), FlexWrap::NoWrap);
        assert_eq!(FlexWrap::parse("wrap"), FlexWrap::Wrap);
        assert_eq!(FlexWrap::parse("WRAP-REVERSE"), FlexWrap::WrapReverse);
        assert_eq!(FlexWrap::parse(""), FlexWrap::NoWrap);
    }

    #[test]
    fn justify_content_parse() {
        assert_eq!(JustifyContent::parse("flex-start"), JustifyContent::FlexStart);
        assert_eq!(JustifyContent::parse("CENTER"), JustifyContent::Center);
        assert_eq!(JustifyContent::parse("space-between"), JustifyContent::SpaceBetween);
        assert_eq!(JustifyContent::parse("garbage"), JustifyContent::FlexStart);
    }

    #[test]
    fn align_items_parse() {
        assert_eq!(AlignItems::parse("stretch"), AlignItems::Stretch);
        assert_eq!(AlignItems::parse("start"), AlignItems::FlexStart);
        assert_eq!(AlignItems::parse("baseline"), AlignItems::Baseline);
        assert_eq!(AlignItems::parse(""), AlignItems::Stretch);
    }

    #[test]
    fn box_sizing_parse() {
        assert_eq!(BoxSizing::parse("border-box"), BoxSizing::BorderBox);
        assert_eq!(BoxSizing::parse("content-box"), BoxSizing::ContentBox);
        assert_eq!(BoxSizing::parse(""), BoxSizing::ContentBox);
    }

    #[test]
    fn box_sizing_predicates() {
        assert!(BoxSizing::BorderBox.is_border_box());
        assert!(!BoxSizing::ContentBox.is_border_box());
    }
}
