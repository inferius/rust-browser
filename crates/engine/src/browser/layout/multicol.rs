//! CSS Multi-column Layout (CSS Multicol Level 1).
//!
//! Spec: https://www.w3.org/TR/css-multicol-1/
//! Properties: column-count, column-width, column-gap, column-rule-*, column-fill,
//!             column-span, break-before/after/inside.
//!
//! Algorithm:
//! 1. If column-width set: N = floor((width + gap) / (column-width + gap))
//! 2. If column-count set: use directly, columns share equally
//! 3. column-fill: balance (default) -> equal height; auto -> fill first, overflow

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnFill {
    Auto,
    Balance,
    BalanceAll,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreakRule {
    Auto,
    Avoid,
    Always,
    Page,
    Left,
    Right,
    Column,
    Recto,
    Verso,
}

#[derive(Debug, Clone, Copy)]
pub struct MulticolConfig {
    pub container_width: f32,
    pub column_count: Option<u32>,
    pub column_width: Option<f32>,
    pub column_gap: f32,
    pub column_fill: ColumnFill,
}

#[derive(Debug, Clone, Copy)]
pub struct MulticolResolved {
    pub used_count: u32,
    pub used_column_width: f32,
    pub used_gap: f32,
}

pub fn resolve_multicol(cfg: &MulticolConfig) -> MulticolResolved {
    let gap = cfg.column_gap.max(0.0);
    let width = cfg.container_width.max(0.0);

    match (cfg.column_count, cfg.column_width) {
        (Some(n), None) => {
            let n = n.max(1);
            let total_gap = gap * (n - 1) as f32;
            let col_w = ((width - total_gap) / n as f32).max(0.0);
            MulticolResolved { used_count: n, used_column_width: col_w, used_gap: gap }
        }
        (None, Some(w)) => {
            let w = w.max(1.0);
            let count = (((width + gap) / (w + gap)).floor() as i32).max(1) as u32;
            let total_gap = gap * (count - 1) as f32;
            let col_w = ((width - total_gap) / count as f32).max(0.0);
            MulticolResolved { used_count: count, used_column_width: col_w, used_gap: gap }
        }
        (Some(n), Some(w)) => {
            // Both: count is upper limit, width is preferred.
            let nmax = n.max(1);
            let w = w.max(1.0);
            let count_from_w = (((width + gap) / (w + gap)).floor() as i32).max(1) as u32;
            let count = nmax.min(count_from_w);
            let total_gap = gap * (count - 1) as f32;
            let col_w = ((width - total_gap) / count as f32).max(0.0);
            MulticolResolved { used_count: count, used_column_width: col_w, used_gap: gap }
        }
        (None, None) => {
            MulticolResolved { used_count: 1, used_column_width: width, used_gap: 0.0 }
        }
    }
}

/// Balance algorithm: equal column height for column-fill: balance.
/// Vstup: content_height (total stacked), num_columns
/// Vystup: target column height (each column tries to be this tall)
pub fn balance_height(content_height: f32, num_columns: u32) -> f32 {
    if num_columns == 0 { return content_height; }
    (content_height / num_columns as f32).ceil()
}

/// Compute column rect (relative to container).
pub fn column_rect(index: u32, resolved: &MulticolResolved, column_height: f32) -> (f32, f32, f32, f32) {
    let x = (resolved.used_column_width + resolved.used_gap) * index as f32;
    (x, 0.0, resolved.used_column_width, column_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_only() {
        let r = resolve_multicol(&MulticolConfig {
            container_width: 600.0,
            column_count: Some(3),
            column_width: None,
            column_gap: 20.0,
            column_fill: ColumnFill::Balance,
        });
        assert_eq!(r.used_count, 3);
        // (600 - 40) / 3 ~ 186.67
        assert!((r.used_column_width - 186.66).abs() < 1.0);
    }

    #[test]
    fn width_only_derives_count() {
        let r = resolve_multicol(&MulticolConfig {
            container_width: 600.0,
            column_count: None,
            column_width: Some(200.0),
            column_gap: 10.0,
            column_fill: ColumnFill::Balance,
        });
        // (600 + 10) / 210 = 2.9 -> 2 cols
        assert_eq!(r.used_count, 2);
    }

    #[test]
    fn both_count_caps() {
        let r = resolve_multicol(&MulticolConfig {
            container_width: 1000.0,
            column_count: Some(2),
            column_width: Some(100.0),
            column_gap: 10.0,
            column_fill: ColumnFill::Balance,
        });
        assert_eq!(r.used_count, 2);
    }

    #[test]
    fn balance_round_up() {
        assert_eq!(balance_height(100.0, 3), 34.0);
    }

    #[test]
    fn column_rect_offsets() {
        let r = MulticolResolved { used_count: 3, used_column_width: 100.0, used_gap: 20.0 };
        let (x, _, w, h) = column_rect(1, &r, 200.0);
        assert!((x - 120.0).abs() < 0.01);
        assert_eq!(w, 100.0);
        assert_eq!(h, 200.0);
    }

    #[test]
    fn empty_container_returns_zero_width() {
        let r = resolve_multicol(&MulticolConfig {
            container_width: 0.0,
            column_count: Some(3),
            column_width: None,
            column_gap: 10.0,
            column_fill: ColumnFill::Auto,
        });
        assert_eq!(r.used_column_width, 0.0);
    }
}
