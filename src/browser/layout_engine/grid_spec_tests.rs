/// Grid layout spec testy - rozsahly suite per CSS Grid L1.

#[cfg(test)]
mod tests {
    use crate::browser::layout::*;
    use crate::browser::layout_engine::grid::{layout_grid, resolve_tracks};

    fn parent(w: f32, h: f32) -> LayoutBox {
        let mut b = LayoutBox::new();
        b.rect.width = w;
        b.rect.height = h;
        b.display = Display::Grid;
        b
    }
    fn child() -> LayoutBox { LayoutBox::new() }
    fn sized_child(w: f32, h: f32) -> LayoutBox {
        let mut b = LayoutBox::new();
        if w > 0.0 { b.explicit_width = Some(w); }
        if h > 0.0 { b.explicit_height = Some(h); }
        b
    }

    // ─── resolve_tracks: fixed sizes ─────────────────────────────────────
    #[test] fn gs_fixed_100px() { let t = resolve_tracks("100px", 500.0, 0.0); assert_eq!(t, vec![100.0]); }
    #[test] fn gs_fixed_2cols() { let t = resolve_tracks("100px 200px", 500.0, 0.0); assert_eq!(t, vec![100.0, 200.0]); }
    #[test] fn gs_fixed_3cols() { let t = resolve_tracks("50px 100px 50px", 500.0, 0.0); assert_eq!(t, vec![50.0, 100.0, 50.0]); }

    // ─── resolve_tracks: percent ─────────────────────────────────────────
    #[test] fn gs_percent_50() { let t = resolve_tracks("50%", 200.0, 0.0); assert_eq!(t, vec![100.0]); }
    #[test] fn gs_percent_25_75() { let t = resolve_tracks("25% 75%", 400.0, 0.0); assert_eq!(t[0], 100.0); assert_eq!(t[1], 300.0); }
    #[test] fn gs_percent_3_equal() { let t = resolve_tracks("33% 33% 34%", 1000.0, 0.0); assert!((t[0] - 330.0).abs() < 1.0); }

    // ─── resolve_tracks: fr ─────────────────────────────────────────────
    #[test] fn gs_fr_single() { let t = resolve_tracks("1fr", 300.0, 0.0); assert_eq!(t, vec![300.0]); }
    #[test] fn gs_fr_2_equal() { let t = resolve_tracks("1fr 1fr", 200.0, 0.0); assert_eq!(t, vec![100.0, 100.0]); }
    #[test] fn gs_fr_3_equal() { let t = resolve_tracks("1fr 1fr 1fr", 300.0, 0.0); assert_eq!(t, vec![100.0, 100.0, 100.0]); }
    #[test] fn gs_fr_1_2_1() { let t = resolve_tracks("1fr 2fr 1fr", 400.0, 0.0); assert_eq!(t, vec![100.0, 200.0, 100.0]); }
    #[test] fn gs_fr_2_3() { let t = resolve_tracks("2fr 3fr", 500.0, 0.0); assert_eq!(t, vec![200.0, 300.0]); }
    #[test] fn gs_fr_4_1() { let t = resolve_tracks("4fr 1fr", 500.0, 0.0); assert_eq!(t, vec![400.0, 100.0]); }

    // ─── resolve_tracks: mixed ───────────────────────────────────────────
    #[test] fn gs_mix_100px_1fr() { let t = resolve_tracks("100px 1fr", 500.0, 0.0); assert_eq!(t, vec![100.0, 400.0]); }
    #[test] fn gs_mix_1fr_100px() { let t = resolve_tracks("1fr 100px", 500.0, 0.0); assert_eq!(t, vec![400.0, 100.0]); }
    #[test] fn gs_mix_50_1fr_50() { let t = resolve_tracks("50px 1fr 50px", 300.0, 0.0); assert_eq!(t, vec![50.0, 200.0, 50.0]); }
    #[test] fn gs_mix_percent_fr() { let t = resolve_tracks("25% 1fr 1fr", 400.0, 0.0); assert_eq!(t[0], 100.0); assert_eq!(t[1], 150.0); assert_eq!(t[2], 150.0); }

    // ─── resolve_tracks: gap ─────────────────────────────────────────────
    #[test] fn gs_gap_2_tracks() { let t = resolve_tracks("1fr 1fr", 200.0, 20.0); assert_eq!(t, vec![90.0, 90.0]); }
    #[test] fn gs_gap_3_tracks() { let t = resolve_tracks("1fr 1fr 1fr", 320.0, 10.0); assert_eq!(t, vec![100.0, 100.0, 100.0]); }
    #[test] fn gs_gap_with_fixed() { let t = resolve_tracks("100px 1fr 1fr", 320.0, 10.0); assert_eq!(t[0], 100.0); }

    // ─── resolve_tracks: repeat() ────────────────────────────────────────
    #[test] fn gs_repeat_2() { let t = resolve_tracks("repeat(2, 100px)", 300.0, 0.0); assert_eq!(t, vec![100.0, 100.0]); }
    #[test] fn gs_repeat_3() { let t = resolve_tracks("repeat(3, 1fr)", 300.0, 0.0); assert_eq!(t, vec![100.0, 100.0, 100.0]); }
    #[test] fn gs_repeat_5_fr() { let t = resolve_tracks("repeat(5, 1fr)", 500.0, 0.0); assert_eq!(t.len(), 5); assert!((t[0] - 100.0).abs() < 1.0); }
    #[test] fn gs_repeat_with_fixed() { let t = resolve_tracks("100px repeat(2, 1fr)", 300.0, 0.0); assert_eq!(t.len(), 3); assert_eq!(t[0], 100.0); }
    #[test] fn gs_repeat_complex() { let t = resolve_tracks("repeat(2, 50px 100px)", 500.0, 0.0); assert_eq!(t, vec![50.0, 100.0, 50.0, 100.0]); }

    // ─── resolve_tracks: minmax ──────────────────────────────────────────
    #[test] fn gs_minmax_basic() { let t = resolve_tracks("minmax(100px, 200px)", 500.0, 0.0); assert_eq!(t, vec![200.0]); }
    #[test] fn gs_minmax_2() { let t = resolve_tracks("minmax(100px, 200px) minmax(50px, 150px)", 500.0, 0.0); assert_eq!(t, vec![200.0, 150.0]); }

    // ─── resolve_tracks: auto ────────────────────────────────────────────
    #[test] fn gs_auto_single() { let t = resolve_tracks("auto", 200.0, 0.0); assert_eq!(t, vec![200.0]); }
    #[test] fn gs_auto_3() { let t = resolve_tracks("auto auto auto", 300.0, 0.0); assert_eq!(t, vec![100.0, 100.0, 100.0]); }
    #[test] fn gs_auto_with_fixed() { let t = resolve_tracks("100px auto auto", 300.0, 0.0); assert_eq!(t[0], 100.0); assert_eq!(t[1], 100.0); }

    // ─── resolve_tracks: named lines ─────────────────────────────────────
    #[test] fn gs_named_lines() { let t = resolve_tracks("[a] 100px [b] 200px [c]", 500.0, 0.0); assert_eq!(t, vec![100.0, 200.0]); }
    #[test] fn gs_named_with_fr() { let t = resolve_tracks("[start] 1fr [middle] 2fr [end]", 300.0, 0.0); assert_eq!(t, vec![100.0, 200.0]); }

    // ─── resolve_tracks: edge cases ──────────────────────────────────────
    #[test] fn gs_empty_returns_empty() { let t = resolve_tracks("", 500.0, 0.0); assert_eq!(t, Vec::<f32>::new()); }
    #[test] fn gs_zero_container() { let t = resolve_tracks("1fr 1fr", 0.0, 0.0); assert_eq!(t, vec![0.0, 0.0]); }

    // ─── Layout: 2D placement ────────────────────────────────────────────
    #[test] fn gs_layout_2x2() { let mut p = parent(200.0, 200.0); p.grid_template_columns = "1fr 1fr".into(); for _ in 0..4 { p.children.push(child()); } layout_grid(&mut p); assert_eq!(p.children[0].rect.x, 0.0); assert_eq!(p.children[1].rect.x, 100.0); assert_ne!(p.children[2].rect.y, p.children[0].rect.y); }
    #[test] fn gs_layout_3x3() { let mut p = parent(300.0, 300.0); p.grid_template_columns = "1fr 1fr 1fr".into(); for _ in 0..9 { p.children.push(child()); } layout_grid(&mut p); assert_eq!(p.children[0].rect.y, p.children[2].rect.y); assert_ne!(p.children[3].rect.y, p.children[0].rect.y); }
    #[test] fn gs_layout_4x1() { let mut p = parent(400.0, 100.0); p.grid_template_columns = "1fr 1fr 1fr 1fr".into(); for _ in 0..4 { p.children.push(child()); } layout_grid(&mut p); assert_eq!(p.children[3].rect.x, 300.0); }
    #[test] fn gs_layout_1x4() { let mut p = parent(100.0, 400.0); p.grid_template_columns = "1fr".into(); for _ in 0..4 { p.children.push(sized_child(0.0, 50.0)); } layout_grid(&mut p); assert_eq!(p.children[3].rect.y, 150.0); }

    // ─── Layout: with gap ────────────────────────────────────────────────
    #[test] fn gs_layout_gap_horizontal() { let mut p = parent(220.0, 200.0); p.grid_template_columns = "1fr 1fr".into(); p.column_gap = 20.0; for _ in 0..2 { p.children.push(child()); } layout_grid(&mut p); assert_eq!(p.children[0].rect.x, 0.0); assert_eq!(p.children[1].rect.x, 120.0); }
    #[test] fn gs_layout_gap_vertical() { let mut p = parent(200.0, 200.0); p.grid_template_columns = "1fr".into(); p.row_gap = 10.0; for _ in 0..2 { p.children.push(sized_child(0.0, 50.0)); } layout_grid(&mut p); assert_eq!(p.children[1].rect.y, 60.0); }
    #[test] fn gs_layout_gap_both() { let mut p = parent(220.0, 220.0); p.grid_template_columns = "1fr 1fr".into(); p.column_gap = 20.0; p.row_gap = 20.0; for _ in 0..4 { p.children.push(sized_child(0.0, 50.0)); } layout_grid(&mut p); assert_eq!(p.children[3].rect.x, 120.0); assert_eq!(p.children[3].rect.y, 70.0); }

    // ─── Layout: explicit child sizes ────────────────────────────────────
    #[test] fn gs_layout_explicit_widths() { let mut p = parent(300.0, 200.0); p.grid_template_columns = "1fr 1fr 1fr".into(); for _ in 0..3 { p.children.push(sized_child(80.0, 0.0)); } layout_grid(&mut p); assert_eq!(p.children[0].rect.width, 80.0); }
    #[test] fn gs_layout_explicit_heights() { let mut p = parent(300.0, 200.0); p.grid_template_columns = "1fr 1fr".into(); for _ in 0..2 { p.children.push(sized_child(0.0, 80.0)); } layout_grid(&mut p); assert_eq!(p.children[0].rect.height, 80.0); }

    // ─── Layout: edge cases ──────────────────────────────────────────────
    #[test] fn gs_layout_no_children() { let mut p = parent(300.0, 200.0); p.grid_template_columns = "1fr 1fr".into(); layout_grid(&mut p); assert_eq!(p.children.len(), 0); }
    #[test] fn gs_layout_single_child() { let mut p = parent(300.0, 200.0); p.grid_template_columns = "1fr 1fr".into(); p.children.push(child()); layout_grid(&mut p); assert_eq!(p.children[0].rect.x, 0.0); }
    #[test] fn gs_layout_more_items_than_cells() { let mut p = parent(200.0, 100.0); p.grid_template_columns = "1fr 1fr".into(); for _ in 0..6 { p.children.push(sized_child(0.0, 30.0)); } layout_grid(&mut p); assert!(p.children[5].rect.y > 0.0); }

    // ─── Layout: parent height auto-grow ─────────────────────────────────
    #[test] fn gs_height_auto_grows() { let mut p = parent(200.0, 0.0); p.grid_template_columns = "1fr 1fr".into(); for _ in 0..4 { p.children.push(sized_child(0.0, 50.0)); } layout_grid(&mut p); assert!(p.rect.height >= 100.0); }
    #[test] fn gs_height_with_gap() { let mut p = parent(200.0, 0.0); p.grid_template_columns = "1fr 1fr".into(); p.row_gap = 10.0; for _ in 0..4 { p.children.push(sized_child(0.0, 50.0)); } layout_grid(&mut p); assert!(p.rect.height >= 110.0); }

    // ─── Smoke: many items ──────────────────────────────────────────────
    #[test] fn gs_smoke_50_items() { let mut p = parent(500.0, 0.0); p.grid_template_columns = "1fr 1fr 1fr 1fr 1fr".into(); for _ in 0..50 { p.children.push(sized_child(0.0, 30.0)); } layout_grid(&mut p); assert!(p.rect.height >= 300.0); }
    #[test] fn gs_smoke_100_items() { let mut p = parent(1000.0, 0.0); p.grid_template_columns = "repeat(10, 1fr)".into(); for _ in 0..100 { p.children.push(sized_child(0.0, 30.0)); } layout_grid(&mut p); assert!(p.rect.height >= 300.0); }
}
