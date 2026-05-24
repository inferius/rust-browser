//! CSS Subgrid (CSS Grid Level 2).
//!
//! Spec: https://www.w3.org/TR/css-grid-2/#subgrids
//! `grid-template-rows/columns: subgrid` makes a grid item inherit parent track sizes.
//!
//! Subgrid:
//! - Item must be display: grid (or inline-grid)
//! - In axis where subgrid is set, item uses parent's track sizing
//! - Tracks are NOT computed independently; they map to parent's tracks
//! - Item can still have line-name lists for resolving named lines
//! - Span lines align to parent's lines
//! - Gutters inherit from parent unless overridden

#[derive(Debug, Clone)]
pub struct SubgridAxis {
    pub enabled: bool,
    pub line_names: Vec<String>,   // line names defined locally (extra)
    pub start_line: i32,           // grid-row/column-start (1-based, neg = from end)
    pub end_line: i32,
    pub span: u32,                 // explicit span, 0 if auto
}

impl Default for SubgridAxis {
    fn default() -> Self {
        Self { enabled: false, line_names: Vec::new(), start_line: 1, end_line: 1, span: 1 }
    }
}

/// Map subgrid axis to parent track indices.
/// Returns vec of (parent_track_index, used_size) pairs.
pub fn map_to_parent_tracks(
    sub: &SubgridAxis,
    parent_track_sizes: &[f32],
) -> Vec<(usize, f32)> {
    if !sub.enabled || parent_track_sizes.is_empty() { return Vec::new(); }
    let start = resolve_line(sub.start_line, parent_track_sizes.len() as i32 + 1);
    let end = if sub.span > 0 { start + sub.span as usize }
              else { resolve_line(sub.end_line, parent_track_sizes.len() as i32 + 1) };
    let s = start.min(end);
    let e = start.max(end).min(parent_track_sizes.len() + 1);
    if s >= e { return Vec::new(); }
    (s..e).filter(|&i| i > 0 && i <= parent_track_sizes.len())
          .map(|i| (i - 1, parent_track_sizes[i - 1]))
          .collect()
}

fn resolve_line(line: i32, total_lines: i32) -> usize {
    // total_lines = num_tracks + 1 (e.g. 4 tracks -> lines 1..5).
    // Positive: 1-based as-is. Negative: -1 = last line (= total_lines), -2 = total_lines - 1, ...
    if line > 0 { line as usize }
    else if line < 0 { (total_lines + line + 1).max(1) as usize }
    else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_to_parent_tracks() {
        let sub = SubgridAxis {
            enabled: true,
            line_names: Vec::new(),
            start_line: 1,
            end_line: 4,
            span: 0,
        };
        let parent = vec![100.0, 200.0, 150.0, 300.0];
        let mapped = map_to_parent_tracks(&sub, &parent);
        assert_eq!(mapped.len(), 3);
        assert_eq!(mapped[0], (0, 100.0));
        assert_eq!(mapped[1], (1, 200.0));
        assert_eq!(mapped[2], (2, 150.0));
    }

    #[test]
    fn span_keyword() {
        let sub = SubgridAxis {
            enabled: true,
            line_names: Vec::new(),
            start_line: 2,
            end_line: 0,
            span: 2,
        };
        let parent = vec![100.0, 200.0, 150.0, 300.0];
        let mapped = map_to_parent_tracks(&sub, &parent);
        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0].0, 1);
        assert_eq!(mapped[1].0, 2);
    }

    #[test]
    fn disabled_returns_empty() {
        let sub = SubgridAxis::default();
        let parent = vec![100.0, 200.0];
        assert!(map_to_parent_tracks(&sub, &parent).is_empty());
    }

    #[test]
    fn negative_line_from_end() {
        let sub = SubgridAxis {
            enabled: true,
            line_names: Vec::new(),
            start_line: -2,
            end_line: -1,
            span: 0,
        };
        let parent = vec![100.0, 200.0, 150.0, 300.0];
        let mapped = map_to_parent_tracks(&sub, &parent);
        // -2 -> line 4, -1 -> line 5 (after last track), so we get track 4 (index 3)
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].0, 3);
    }

    #[test]
    fn empty_parent_returns_empty() {
        let sub = SubgridAxis { enabled: true, ..Default::default() };
        assert!(map_to_parent_tracks(&sub, &[]).is_empty());
    }
}
