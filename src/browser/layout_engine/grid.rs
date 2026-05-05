/// Grid layout - vlastni implementace.
///
/// Inspirovano `taffy` (MIT licence) + CSS Grid L1 spec
/// (https://www.w3.org/TR/css-grid-1/).
///
/// Aktualne velmi minimalisticka impl - rozdeli children rovnomerne do
/// grid cells podle grid-template-columns/rows. Plnejsi impl by vyzadovala
/// track sizing algoritmus (intrinsic sizes, fr units, minmax, repeat).

use super::super::layout::LayoutBox;

/// Grid layout entry-point - rozdeli children do gridu.
/// Pouzije bx.grid_template_columns / bx.grid_template_rows.
pub fn layout_grid(bx: &mut LayoutBox) {
    let inner_x = bx.rect.x + bx.padding + bx.margin + bx.border_width;
    let inner_y = bx.rect.y + bx.padding + bx.margin + bx.border_width;
    let inner_w = bx.rect.width - 2.0 * (bx.padding + bx.margin + bx.border_width);

    if bx.children.is_empty() { return; }

    // Parse track count z grid-template-columns
    let cols = parse_track_count(&bx.grid_template_columns).max(1);
    let rows_explicit = parse_track_count(&bx.grid_template_rows);
    let row_gap = bx.row_gap;
    let col_gap = bx.column_gap;

    let item_count = bx.children.len();
    let rows = if rows_explicit > 0 {
        rows_explicit
    } else {
        item_count.div_ceil(cols)
    };

    let cell_w = (inner_w - col_gap * (cols.saturating_sub(1) as f32)) / cols as f32;
    let default_row_h = 50.0_f32;

    for (i, child) in bx.children.iter_mut().enumerate() {
        let row = i / cols;
        let col = i % cols;
        if row >= rows { break; }
        let cell_h = child.explicit_height.unwrap_or(default_row_h);
        child.rect.x = inner_x + col as f32 * (cell_w + col_gap);
        child.rect.y = inner_y + row as f32 * (cell_h + row_gap);
        child.rect.width = child.explicit_width.unwrap_or(cell_w);
        child.rect.height = cell_h;
        super::super::layout::layout_block(child);
    }

    // Update parent height
    let used_rows = (item_count.div_ceil(cols)).min(rows);
    let total_h = used_rows as f32 * default_row_h
        + row_gap * (used_rows.saturating_sub(1) as f32)
        + 2.0 * (bx.padding + bx.border_width);
    if bx.rect.height < total_h {
        bx.rect.height = total_h;
    }
}

/// Pocet tracku z grid-template-columns / grid-template-rows.
/// Velmi zjednodusene - count whitespace-separated tokens, ignore [name].
fn parse_track_count(s: &str) -> usize {
    if s.is_empty() { return 0; }
    let mut count = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            // Skip [line-name]
            for cc in chars.by_ref() {
                if cc == ']' { break; }
            }
            continue;
        }
        if c.is_whitespace() { continue; }
        // Posbiraj token
        while let Some(&cc) = chars.peek() {
            if cc.is_whitespace() || cc == '[' { break; }
            chars.next();
        }
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_count_basic() {
        assert_eq!(parse_track_count("1fr 1fr 1fr"), 3);
        assert_eq!(parse_track_count("100px 200px"), 2);
        assert_eq!(parse_track_count(""), 0);
    }

    #[test]
    fn track_count_with_named_lines() {
        assert_eq!(parse_track_count("[start] 1fr [middle] 2fr [end]"), 2);
    }
}
