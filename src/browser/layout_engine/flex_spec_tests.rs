/// Flex layout spec testy - rozsahly suite per CSS Flexbox L1 spec.
/// Inspirovano taffy fixture testy + WPT (Web Platform Tests).
///
/// Kazdy test je separe scenario s:
/// - container size
/// - children s explicit sizes (mozna flex-grow/shrink)
/// - assertions on final positions

#[cfg(test)]
mod tests {
    use crate::browser::layout::*;
    use crate::browser::layout_engine::flex::layout_flex;

    fn parent(w: f32, h: f32) -> LayoutBox {
        let mut b = LayoutBox::new();
        b.rect.width = w;
        b.rect.height = h;
        b.display = Display::Flex;
        b.flex_direction = "row".into();
        b
    }
    fn child(w: f32, h: f32) -> LayoutBox {
        let mut b = LayoutBox::new();
        b.explicit_width = Some(w);
        b.explicit_height = Some(h);
        b
    }

    // ─── Direction: row ──────────────────────────────────────────────────
    #[test] fn fs_row_1_item() { let mut p = parent(200.0, 100.0); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.x, 0.0); }
    #[test] fn fs_row_2_items() { let mut p = parent(200.0, 100.0); p.children.push(child(50.0, 30.0)); p.children.push(child(60.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[1].rect.x, 50.0); }
    #[test] fn fs_row_3_items() { let mut p = parent(300.0, 100.0); for w in [40.0_f32, 50.0, 60.0] { p.children.push(child(w, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[2].rect.x, 90.0); }
    #[test] fn fs_row_5_items_stack() { let mut p = parent(500.0, 100.0); for _ in 0..5 { p.children.push(child(30.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[4].rect.x, 120.0); }
    #[test] fn fs_row_y_constant() { let mut p = parent(300.0, 100.0); for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); }

    // ─── Direction: column ───────────────────────────────────────────────
    #[test] fn fs_col_1_item() { let mut p = parent(100.0, 200.0); p.flex_direction = "column".into(); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.y, 0.0); }
    #[test] fn fs_col_2_items() { let mut p = parent(100.0, 200.0); p.flex_direction = "column".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 40.0)); layout_flex(&mut p); assert_eq!(p.children[1].rect.y, 30.0); }
    #[test] fn fs_col_x_constant() { let mut p = parent(100.0, 200.0); p.flex_direction = "column".into(); for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.x, p.children[2].rect.x); }

    // ─── Direction: row-reverse / column-reverse ─────────────────────────
    #[test] fn fs_row_reverse_2() { let mut p = parent(200.0, 100.0); p.flex_direction = "row-reverse".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert!(p.children[0].rect.x > p.children[1].rect.x); }
    #[test] fn fs_col_reverse_2() { let mut p = parent(100.0, 200.0); p.flex_direction = "column-reverse".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert!(p.children[0].rect.y > p.children[1].rect.y); }

    // ─── Wrap: nowrap ────────────────────────────────────────────────────
    #[test] fn fs_nowrap_no_overflow() { let mut p = parent(300.0, 100.0); p.flex_wrap = "nowrap".into(); for _ in 0..3 { p.children.push(child(80.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); }
    #[test] fn fs_nowrap_with_overflow() { let mut p = parent(100.0, 100.0); p.flex_wrap = "nowrap".into(); for _ in 0..3 { p.children.push(child(80.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); }

    // ─── Wrap: wrap ──────────────────────────────────────────────────────
    #[test] fn fs_wrap_2_lines() { let mut p = parent(100.0, 200.0); p.flex_wrap = "wrap".into(); for _ in 0..3 { p.children.push(child(60.0, 30.0)); } layout_flex(&mut p); assert!(p.children[1].rect.y > p.children[0].rect.y); }
    #[test] fn fs_wrap_exact_fit() { let mut p = parent(100.0, 200.0); p.flex_wrap = "wrap".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[1].rect.y); }
    #[test] fn fs_wrap_3_per_line() { let mut p = parent(150.0, 200.0); p.flex_wrap = "wrap".into(); for _ in 0..6 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); assert_ne!(p.children[3].rect.y, p.children[0].rect.y); }

    // ─── Justify-content ─────────────────────────────────────────────────
    #[test] fn fs_jc_start_first_at_zero() { let mut p = parent(300.0, 100.0); p.justify_content = "flex-start".into(); for _ in 0..2 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.x, 0.0); }
    #[test] fn fs_jc_end_last_at_right() { let mut p = parent(300.0, 100.0); p.justify_content = "flex-end".into(); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert!((p.children[0].rect.x - 250.0).abs() < 0.5); }
    #[test] fn fs_jc_center_centered() { let mut p = parent(300.0, 100.0); p.justify_content = "center".into(); p.children.push(child(100.0, 30.0)); layout_flex(&mut p); assert!((p.children[0].rect.x - 100.0).abs() < 0.5); }
    #[test] fn fs_jc_space_between_first_at_zero() { let mut p = parent(300.0, 100.0); p.justify_content = "space-between".into(); for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.x, 0.0); }
    #[test] fn fs_jc_space_between_3_items_evenly() { let mut p = parent(300.0, 100.0); p.justify_content = "space-between".into(); for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); let g1 = p.children[1].rect.x - (p.children[0].rect.x + 50.0); let g2 = p.children[2].rect.x - (p.children[1].rect.x + 50.0); assert!((g1 - g2).abs() < 0.5); }
    #[test] fn fs_jc_space_around() { let mut p = parent(300.0, 100.0); p.justify_content = "space-around".into(); for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert!(p.children[0].rect.x > 0.0); }
    #[test] fn fs_jc_space_evenly() { let mut p = parent(300.0, 100.0); p.justify_content = "space-evenly".into(); for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); let edge = p.children[0].rect.x; let between = p.children[1].rect.x - (p.children[0].rect.x + 50.0); assert!((edge - between).abs() < 0.5); }

    // ─── Align-items ─────────────────────────────────────────────────────
    #[test] fn fs_ai_start_top_aligned() { let mut p = parent(300.0, 100.0); p.align_items = "flex-start".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 50.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[1].rect.y); }
    #[test] fn fs_ai_center_smaller_pushed_down() { let mut p = parent(300.0, 100.0); p.align_items = "center".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 50.0)); layout_flex(&mut p); assert!(p.children[0].rect.y > p.children[1].rect.y); }
    #[test] fn fs_ai_end_aligned_to_bottom() { let mut p = parent(300.0, 100.0); p.align_items = "flex-end".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 50.0)); layout_flex(&mut p); let b0 = p.children[0].rect.y + p.children[0].rect.height; let b1 = p.children[1].rect.y + p.children[1].rect.height; assert!((b0 - b1).abs() < 0.5); }
    #[test] fn fs_ai_stretch_fills_cross() { let mut p = parent(300.0, 100.0); p.align_items = "stretch".into(); let mut a = LayoutBox::new(); a.explicit_width = Some(50.0); let mut b = LayoutBox::new(); b.explicit_width = Some(50.0); b.explicit_height = Some(80.0); p.children.push(a); p.children.push(b); layout_flex(&mut p); /* a bez explicit height stretchne na container cross (100), b s explicit 80 zustane */ assert_eq!(p.children[0].rect.height, 100.0); assert_eq!(p.children[1].rect.height, 80.0); }

    // ─── Gap ─────────────────────────────────────────────────────────────
    #[test] fn fs_gap_5() { let mut p = parent(300.0, 100.0); p.column_gap = 5.0; p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[1].rect.x, 55.0); }
    #[test] fn fs_gap_10() { let mut p = parent(300.0, 100.0); p.column_gap = 10.0; for _ in 0..3 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[2].rect.x, 120.0); }
    #[test] fn fs_gap_zero_default() { let mut p = parent(300.0, 100.0); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[1].rect.x, 50.0); }
    #[test] fn fs_row_gap_in_wrap() { let mut p = parent(100.0, 300.0); p.flex_wrap = "wrap".into(); p.row_gap = 8.0; for _ in 0..2 { p.children.push(child(60.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[1].rect.y, 38.0); }

    // ─── Flex-grow ───────────────────────────────────────────────────────
    #[test] fn fs_grow_1_distributes_all_free() { let mut p = parent(300.0, 100.0); let mut a = child(50.0, 30.0); a.flex_grow = 1.0; p.children.push(a); layout_flex(&mut p); assert_eq!(p.children[0].rect.width, 300.0); }
    #[test] fn fs_grow_equal_split() { let mut p = parent(300.0, 100.0); for _ in 0..2 { let mut c = child(50.0, 30.0); c.flex_grow = 1.0; p.children.push(c); } layout_flex(&mut p); assert_eq!(p.children[0].rect.width, p.children[1].rect.width); }
    #[test] fn fs_grow_proportional_1_2() { let mut p = parent(300.0, 100.0); let mut a = child(50.0, 30.0); a.flex_grow = 1.0; let mut b = child(50.0, 30.0); b.flex_grow = 2.0; p.children.push(a); p.children.push(b); layout_flex(&mut p); assert!(p.children[1].rect.width > p.children[0].rect.width); }
    #[test] fn fs_grow_zero_no_change() { let mut p = parent(300.0, 100.0); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.width, 50.0); }
    #[test] fn fs_grow_one_only() { let mut p = parent(300.0, 100.0); let mut a = child(50.0, 30.0); a.flex_grow = 1.0; p.children.push(a); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.width, 250.0); assert_eq!(p.children[1].rect.width, 50.0); }

    // ─── Flex-shrink ─────────────────────────────────────────────────────
    #[test] fn fs_shrink_overflowing() { let mut p = parent(100.0, 100.0); let mut a = child(80.0, 30.0); a.flex_shrink = 1.0; let mut b = child(80.0, 30.0); b.flex_shrink = 1.0; p.children.push(a); p.children.push(b); layout_flex(&mut p); assert!(p.children[0].rect.width < 80.0); }
    #[test] fn fs_shrink_zero_no_shrink() { let mut p = parent(100.0, 100.0); p.flex_wrap = "nowrap".into(); let mut a = child(80.0, 30.0); a.flex_shrink = 0.0; let mut b = child(80.0, 30.0); b.flex_shrink = 0.0; p.children.push(a); p.children.push(b); layout_flex(&mut p); assert_eq!(p.children[0].rect.width, 80.0); }

    // ─── Edge cases ──────────────────────────────────────────────────────
    #[test] fn fs_no_children() { let mut p = parent(300.0, 100.0); layout_flex(&mut p); assert_eq!(p.children.len(), 0); }
    #[test] fn fs_single_child() { let mut p = parent(300.0, 100.0); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.x, 0.0); }
    #[test] fn fs_zero_size_children() { let mut p = parent(300.0, 100.0); p.children.push(child(0.0, 0.0)); p.children.push(child(0.0, 0.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.x, 0.0); }
    #[test] fn fs_huge_count() { let mut p = parent(1000.0, 100.0); for _ in 0..50 { p.children.push(child(10.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[49].rect.x, 490.0); }

    // ─── Combined ───────────────────────────────────────────────────────
    #[test] fn fs_grow_with_wrap() { let mut p = parent(150.0, 200.0); p.flex_wrap = "wrap".into(); for _ in 0..3 { let mut c = child(80.0, 30.0); c.flex_grow = 1.0; p.children.push(c); } layout_flex(&mut p); assert!(p.children[1].rect.y > p.children[0].rect.y || p.children[2].rect.y > p.children[0].rect.y); }
    #[test] fn fs_jc_end_with_grow_no_effect() { let mut p = parent(300.0, 100.0); p.justify_content = "flex-end".into(); let mut a = child(50.0, 30.0); a.flex_grow = 1.0; p.children.push(a); layout_flex(&mut p); assert_eq!(p.children[0].rect.width, 300.0); }
    #[test] fn fs_ai_center_in_column() { let mut p = parent(200.0, 300.0); p.flex_direction = "column".into(); p.align_items = "center".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(80.0, 30.0)); layout_flex(&mut p); assert!(p.children[0].rect.x > p.children[1].rect.x); }
    #[test] fn fs_jc_center_in_column() { let mut p = parent(100.0, 300.0); p.flex_direction = "column".into(); p.justify_content = "center".into(); p.children.push(child(50.0, 100.0)); layout_flex(&mut p); assert!(p.children[0].rect.y > 50.0); }
    #[test] fn fs_jc_space_between_in_column() { let mut p = parent(100.0, 300.0); p.flex_direction = "column".into(); p.justify_content = "space-between".into(); p.children.push(child(50.0, 30.0)); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert!(p.children[1].rect.y > 200.0); }

    // ─── Width updates on parent ─────────────────────────────────────────
    #[test] fn fs_parent_height_update() { let mut p = parent(300.0, 0.0); p.children.push(child(50.0, 100.0)); layout_flex(&mut p); assert!(p.rect.height >= 100.0); }
    #[test] fn fs_wrap_parent_height_multiplied() { let mut p = parent(100.0, 0.0); p.flex_wrap = "wrap".into(); for _ in 0..3 { p.children.push(child(60.0, 30.0)); } layout_flex(&mut p); assert!(p.rect.height >= 90.0); }

    // ─── Direction-specific edge cases ───────────────────────────────────
    #[test] fn fs_row_gap_used_in_wrap() { let mut p = parent(100.0, 200.0); p.flex_wrap = "wrap".into(); p.row_gap = 5.0; for _ in 0..3 { p.children.push(child(60.0, 30.0)); } layout_flex(&mut p); let dy = p.children[1].rect.y - p.children[0].rect.y; assert!((dy - 35.0).abs() < 0.5); }
    #[test] fn fs_col_jc_start_first_at_zero() { let mut p = parent(100.0, 300.0); p.flex_direction = "column".into(); p.justify_content = "flex-start".into(); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert_eq!(p.children[0].rect.y, 0.0); }
    #[test] fn fs_col_jc_end() { let mut p = parent(100.0, 300.0); p.flex_direction = "column".into(); p.justify_content = "flex-end".into(); p.children.push(child(50.0, 30.0)); layout_flex(&mut p); assert!((p.children[0].rect.y - 270.0).abs() < 0.5); }

    // ─── Various sizes ───────────────────────────────────────────────────
    #[test] fn fs_uneven_widths() { let mut p = parent(500.0, 100.0); for w in [10.0_f32, 20.0, 30.0, 40.0, 50.0] { p.children.push(child(w, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[4].rect.x, 100.0); }
    #[test] fn fs_uneven_heights_align_start() { let mut p = parent(500.0, 100.0); p.align_items = "flex-start".into(); for h in [10.0_f32, 30.0, 50.0] { p.children.push(child(50.0, h)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); }
    #[test] fn fs_uneven_heights_align_end() { let mut p = parent(500.0, 100.0); p.align_items = "flex-end".into(); for h in [10.0_f32, 30.0, 50.0] { p.children.push(child(50.0, h)); } layout_flex(&mut p); let bottoms: Vec<f32> = p.children.iter().map(|c| c.rect.y + c.rect.height).collect(); assert!((bottoms[0] - bottoms[2]).abs() < 0.5); }
    #[test] fn fs_uneven_heights_align_center() { let mut p = parent(500.0, 100.0); p.align_items = "center".into(); for h in [10.0_f32, 30.0, 50.0] { p.children.push(child(50.0, h)); } layout_flex(&mut p); assert!(p.children[0].rect.y > p.children[2].rect.y); }

    // ─── Multi-line ──────────────────────────────────────────────────────
    #[test] fn fs_3x3_grid_via_wrap() { let mut p = parent(150.0, 300.0); p.flex_wrap = "wrap".into(); for _ in 0..9 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); assert_ne!(p.children[3].rect.y, p.children[0].rect.y); }
    #[test] fn fs_multi_line_consistent_widths() { let mut p = parent(100.0, 300.0); p.flex_wrap = "wrap".into(); for _ in 0..4 { p.children.push(child(50.0, 30.0)); } layout_flex(&mut p); for i in 0..4 { assert_eq!(p.children[i].rect.width, 50.0); } }

    // ─── Larger flex-grow factors ────────────────────────────────────────
    #[test] fn fs_grow_5_to_1() { let mut p = parent(300.0, 100.0); let mut a = child(20.0, 30.0); a.flex_grow = 5.0; let mut b = child(20.0, 30.0); b.flex_grow = 1.0; p.children.push(a); p.children.push(b); layout_flex(&mut p); let total: f32 = p.children.iter().map(|c| c.rect.width).sum(); assert!((total - 300.0).abs() < 0.5); }

    // ─── Performance benchmark (smoke test) ──────────────────────────────
    #[test] fn fs_smoke_100_items() { let mut p = parent(2000.0, 100.0); p.flex_wrap = "wrap".into(); for _ in 0..100 { p.children.push(child(20.0, 30.0)); } layout_flex(&mut p); assert!(p.rect.height >= 30.0); }
    #[test] fn fs_smoke_500_items() { let mut p = parent(5000.0, 100.0); p.flex_wrap = "wrap".into(); for _ in 0..500 { p.children.push(child(10.0, 20.0)); } layout_flex(&mut p); assert!(p.rect.height >= 20.0); }
}
