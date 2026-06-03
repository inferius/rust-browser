//! Block Formatting Context (BFC) helpers.
//!
//! Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting
//! BFC is established by:
//! - root element (html)
//! - floats
//! - absolute/fixed positioned elements
//! - display: inline-block / table-cell / table-caption
//! - overflow != visible
//! - display: flow-root (explicit BFC trigger)
//! - flex/grid items (technically inner FCs)
//!
//! Margin collapse rules (W3C CSS2.1 8.3.1):
//! 1. Adjacent siblings: collapse vert margins
//! 2. Parent + first/last child: collapse if no padding/border separating
//! 3. Empty block: top + bottom collapse together
//! 4. Negative margins: max(positive) + min(negative)

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BfcTrigger {
    None,
    Root,
    Float,
    AbsolutePos,
    InlineBlock,
    OverflowNotVisible,
    FlowRoot,
    FlexItem,
    GridItem,
    TableCell,
    TableCaption,
    Contain,           // contain: layout/paint/strict
}

/// Detekce BFC vyvolavace z computed style strings.
pub fn detect_bfc_trigger(
    is_root: bool,
    display: &str,
    position: &str,
    float: &str,
    overflow: &str,
    contain: &str,
) -> BfcTrigger {
    if is_root { return BfcTrigger::Root; }
    if float == "left" || float == "right" { return BfcTrigger::Float; }
    if position == "absolute" || position == "fixed" { return BfcTrigger::AbsolutePos; }
    match display {
        "inline-block" => return BfcTrigger::InlineBlock,
        "flow-root" => return BfcTrigger::FlowRoot,
        "flex" | "inline-flex" => return BfcTrigger::FlexItem,
        "grid" | "inline-grid" => return BfcTrigger::GridItem,
        "table-cell" => return BfcTrigger::TableCell,
        "table-caption" => return BfcTrigger::TableCaption,
        _ => {}
    }
    if overflow != "visible" && !overflow.is_empty() { return BfcTrigger::OverflowNotVisible; }
    if contain.contains("layout") || contain.contains("paint") || contain == "strict" || contain == "content" {
        return BfcTrigger::Contain;
    }
    BfcTrigger::None
}

/// Margin collapse mezi dvema adjacent siblings.
/// Returns single effective margin podle CSS 2.1 spec.
pub fn collapse_margins(prev_bottom: f32, next_top: f32) -> f32 {
    if prev_bottom >= 0.0 && next_top >= 0.0 {
        prev_bottom.max(next_top)
    } else if prev_bottom <= 0.0 && next_top <= 0.0 {
        prev_bottom.min(next_top)
    } else {
        prev_bottom + next_top
    }
}

/// Parent-child margin collapse: parent's top margin can collapse with child's top
/// if neither padding nor border separates them.
pub fn parent_child_collapse(
    parent_margin_top: f32,
    parent_padding_top: f32,
    parent_border_top: f32,
    child_margin_top: f32,
) -> (f32, f32) {
    if parent_padding_top > 0.0 || parent_border_top > 0.0 {
        return (parent_margin_top, child_margin_top);
    }
    let merged = collapse_margins(parent_margin_top, child_margin_top);
    (merged, 0.0)
}

/// Empty block: top + bottom margin collapse if no min-height, height, padding, border.
pub fn empty_block_collapse(
    margin_top: f32,
    margin_bottom: f32,
    height: f32,
    padding_v: f32,
    border_v: f32,
) -> f32 {
    if height > 0.0 || padding_v > 0.0 || border_v > 0.0 {
        margin_top + margin_bottom
    } else {
        collapse_margins(margin_top, margin_bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_bfc() {
        let t = detect_bfc_trigger(true, "block", "static", "none", "visible", "none");
        assert_eq!(t, BfcTrigger::Root);
    }

    #[test]
    fn overflow_hidden_establishes_bfc() {
        let t = detect_bfc_trigger(false, "block", "static", "none", "hidden", "none");
        assert_eq!(t, BfcTrigger::OverflowNotVisible);
    }

    #[test]
    fn flow_root_explicit() {
        let t = detect_bfc_trigger(false, "flow-root", "static", "none", "visible", "none");
        assert_eq!(t, BfcTrigger::FlowRoot);
    }

    #[test]
    fn float_establishes() {
        let t = detect_bfc_trigger(false, "block", "static", "left", "visible", "none");
        assert_eq!(t, BfcTrigger::Float);
    }

    #[test]
    fn collapse_positive_takes_max() {
        assert_eq!(collapse_margins(10.0, 20.0), 20.0);
        assert_eq!(collapse_margins(30.0, 5.0), 30.0);
    }

    #[test]
    fn collapse_negative_takes_min() {
        assert_eq!(collapse_margins(-10.0, -5.0), -10.0);
    }

    #[test]
    fn collapse_mixed_sums() {
        assert_eq!(collapse_margins(20.0, -5.0), 15.0);
        assert_eq!(collapse_margins(-10.0, 30.0), 20.0);
    }

    #[test]
    fn parent_child_no_border() {
        let (p, c) = parent_child_collapse(10.0, 0.0, 0.0, 20.0);
        assert_eq!(p, 20.0);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn parent_child_with_padding() {
        let (p, c) = parent_child_collapse(10.0, 5.0, 0.0, 20.0);
        assert_eq!(p, 10.0);
        assert_eq!(c, 20.0);
    }

    #[test]
    fn empty_block_collapses_self() {
        // No height/padding/border -> top + bottom collapse
        assert_eq!(empty_block_collapse(10.0, 20.0, 0.0, 0.0, 0.0), 20.0);
        // height > 0 -> no collapse
        assert_eq!(empty_block_collapse(10.0, 20.0, 100.0, 0.0, 0.0), 30.0);
    }

    #[test]
    fn contain_layout_establishes() {
        let t = detect_bfc_trigger(false, "block", "static", "none", "visible", "layout");
        assert_eq!(t, BfcTrigger::Contain);
    }
}
