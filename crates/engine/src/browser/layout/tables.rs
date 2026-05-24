//! CSS Tables - Table Layout Algorithm.
//!
//! Spec: https://www.w3.org/TR/CSS22/tables.html
//! https://www.w3.org/TR/css-tables-3/
//!
//! display: table / table-row-group / table-row / table-cell / table-column / table-caption.
//! Two algorithms: "fixed" (table-layout: fixed) and "auto".
//!
//! Auto algorithm:
//! 1. Collect cells per row, count columns from max row width
//! 2. For each column: min_w = max(cell min content width), max_w = max(cell max content)
//! 3. Distribute available width: cap to max_w first, then grow toward max_w
//! 4. Row heights = max(cell heights in row), pak baseline-align cells in row

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableLayoutKind {
    Auto,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderCollapse {
    Separate,
    Collapse,
}

#[derive(Debug, Clone)]
pub struct TableCellSpec {
    pub row: usize,
    pub col: usize,
    pub row_span: u32,
    pub col_span: u32,
    pub min_width: f32,
    pub max_width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct TableTrack {
    pub min: f32,
    pub max: f32,
    pub computed: f32,
}

/// Compute column widths for auto algorithm.
pub fn compute_column_widths_auto(cells: &[TableCellSpec], num_cols: usize, container_w: f32, border_spacing: f32) -> Vec<f32> {
    if num_cols == 0 { return Vec::new(); }
    let mut tracks: Vec<TableTrack> = vec![TableTrack { min: 0.0, max: 0.0, computed: 0.0 }; num_cols];
    for c in cells {
        if c.col_span > 1 { continue; } // multi-col handled in second pass
        if c.col >= num_cols { continue; }
        if c.min_width > tracks[c.col].min { tracks[c.col].min = c.min_width; }
        if c.max_width > tracks[c.col].max { tracks[c.col].max = c.max_width; }
    }
    // Distribute spanning-cell min/max evenly per spanned column.
    for c in cells {
        if c.col_span <= 1 { continue; }
        let end = (c.col + c.col_span as usize).min(num_cols);
        if c.col >= end { continue; }
        let span_min: f32 = tracks[c.col..end].iter().map(|t| t.min).sum::<f32>() + border_spacing * (c.col_span - 1) as f32;
        let span_max: f32 = tracks[c.col..end].iter().map(|t| t.max).sum::<f32>() + border_spacing * (c.col_span - 1) as f32;
        if c.min_width > span_min {
            let add = (c.min_width - span_min) / c.col_span as f32;
            for t in &mut tracks[c.col..end] { t.min += add; }
        }
        if c.max_width > span_max {
            let add = (c.max_width - span_max) / c.col_span as f32;
            for t in &mut tracks[c.col..end] { t.max += add; }
        }
    }

    let total_min: f32 = tracks.iter().map(|t| t.min).sum::<f32>() + border_spacing * (num_cols.saturating_sub(1)) as f32;
    let total_max: f32 = tracks.iter().map(|t| t.max).sum::<f32>() + border_spacing * (num_cols.saturating_sub(1)) as f32;
    let avail = container_w.max(total_min);

    if avail <= total_min {
        for t in &mut tracks { t.computed = t.min; }
    } else if avail >= total_max {
        // Distribute extra width proportionally to max - min.
        let extra = avail - total_max;
        let denom = tracks.len() as f32;
        for t in &mut tracks { t.computed = t.max + extra / denom; }
    } else {
        // Interpolate between min and max.
        let span = total_max - total_min;
        let avail_in_band = avail - total_min;
        let ratio = if span > 0.0 { avail_in_band / span } else { 0.0 };
        for t in &mut tracks {
            t.computed = t.min + (t.max - t.min) * ratio;
        }
    }
    tracks.into_iter().map(|t| t.computed).collect()
}

/// Fixed algorithm: pouziva pouze first-row cell + colgroup widths.
pub fn compute_column_widths_fixed(col_widths: &[Option<f32>], container_w: f32) -> Vec<f32> {
    let n = col_widths.len();
    if n == 0 { return Vec::new(); }
    let known: f32 = col_widths.iter().filter_map(|w| *w).sum();
    let unknown_count = col_widths.iter().filter(|w| w.is_none()).count();
    let remaining = (container_w - known).max(0.0);
    let per_unknown = if unknown_count > 0 { remaining / unknown_count as f32 } else { 0.0 };
    col_widths.iter().map(|w| w.unwrap_or(per_unknown)).collect()
}

/// Row heights = max cell height per row.
pub fn compute_row_heights(cells: &[TableCellSpec], num_rows: usize) -> Vec<f32> {
    let mut heights = vec![0.0_f32; num_rows];
    for c in cells {
        if c.row_span <= 1 {
            if c.row < num_rows && c.height > heights[c.row] {
                heights[c.row] = c.height;
            }
            continue;
        }
        let end = (c.row + c.row_span as usize).min(num_rows);
        if c.row >= end { continue; }
        let span_h: f32 = heights[c.row..end].iter().sum();
        if c.height > span_h {
            let add = (c.height - span_h) / c.row_span as f32;
            for h in &mut heights[c.row..end] { *h += add; }
        }
    }
    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(row: usize, col: usize, min: f32, max: f32, h: f32) -> TableCellSpec {
        TableCellSpec { row, col, row_span: 1, col_span: 1, min_width: min, max_width: max, height: h }
    }

    #[test]
    fn auto_distribution_max_fit() {
        // 3 columns, plenty of room
        let cells = vec![cell(0, 0, 10.0, 30.0, 20.0), cell(0, 1, 20.0, 50.0, 20.0), cell(0, 2, 10.0, 20.0, 20.0)];
        let w = compute_column_widths_auto(&cells, 3, 500.0, 0.0);
        // total max = 100 + extra 400 / 3 cols
        assert!((w.iter().sum::<f32>() - 500.0).abs() < 1.0);
    }

    #[test]
    fn auto_constrained_to_min() {
        let cells = vec![cell(0, 0, 50.0, 100.0, 20.0), cell(0, 1, 50.0, 100.0, 20.0)];
        let w = compute_column_widths_auto(&cells, 2, 80.0, 0.0);
        // container too small -> clamp to min
        assert!((w[0] - 50.0).abs() < 0.01);
        assert!((w[1] - 50.0).abs() < 0.01);
    }

    #[test]
    fn auto_interpolation() {
        let cells = vec![cell(0, 0, 10.0, 50.0, 20.0), cell(0, 1, 10.0, 50.0, 20.0)];
        let w = compute_column_widths_auto(&cells, 2, 60.0, 0.0);
        // min total 20, max total 100, avail 60 -> half interpolation = each 30
        assert!((w[0] - 30.0).abs() < 0.5);
        assert!((w[1] - 30.0).abs() < 0.5);
    }

    #[test]
    fn fixed_distributes_unknowns() {
        let w = compute_column_widths_fixed(&[Some(100.0), None, None], 500.0);
        assert!((w[0] - 100.0).abs() < 0.01);
        assert!((w[1] - 200.0).abs() < 0.01);
        assert!((w[2] - 200.0).abs() < 0.01);
    }

    #[test]
    fn row_heights_max_per_row() {
        let cells = vec![cell(0, 0, 0.0, 0.0, 20.0), cell(0, 1, 0.0, 0.0, 50.0), cell(1, 0, 0.0, 0.0, 10.0)];
        let h = compute_row_heights(&cells, 2);
        assert_eq!(h[0], 50.0);
        assert_eq!(h[1], 10.0);
    }

    #[test]
    fn row_span_distributes_height() {
        let mut c = cell(0, 0, 0.0, 0.0, 100.0);
        c.row_span = 2;
        let h = compute_row_heights(&[c], 2);
        assert!((h[0] + h[1] - 100.0).abs() < 0.01);
    }
}
