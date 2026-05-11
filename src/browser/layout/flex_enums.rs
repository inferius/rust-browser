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
