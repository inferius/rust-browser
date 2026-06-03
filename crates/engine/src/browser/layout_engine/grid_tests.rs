/// Grid layout testy - inspirovano CSS Grid L1 spec + taffy.

#[cfg(test)]
mod tests {
    use crate::browser::layout::*;
    use crate::browser::layout_engine::grid::layout_grid;

    fn make_grid_box(width: f32, height: f32) -> LayoutBox {
        let mut bx = LayoutBox::new();
        bx.rect.width = width;
        bx.rect.height = height;
        bx.display = Display::Grid;
        bx
    }

    fn make_child(w: f32, h: f32) -> LayoutBox {
        let mut bx = LayoutBox::new();
        if w > 0.0 { bx.explicit_width = Some(w); }
        if h > 0.0 { bx.explicit_height = Some(h); }
        bx
    }

    #[test]
    fn grid_3_columns_distributes_equally() {
        let mut parent = make_grid_box(300.0, 300.0);
        parent.grid_template_columns = "1fr 1fr 1fr".into();
        for _ in 0..3 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // 3 cols, 300 / 3 = 100
        assert!((parent.children[0].rect.x - 0.0).abs() < 1.0);
        assert!((parent.children[1].rect.x - 100.0).abs() < 1.0);
        assert!((parent.children[2].rect.x - 200.0).abs() < 1.0);
    }

    #[test]
    fn grid_2_columns_wraps_to_rows() {
        let mut parent = make_grid_box(200.0, 300.0);
        parent.grid_template_columns = "1fr 1fr".into();
        for _ in 0..4 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // 2 cols, 4 items -> 2 rows
        assert!(parent.children[2].rect.y > parent.children[0].rect.y);
        assert_eq!(parent.children[2].rect.x, parent.children[0].rect.x);
    }

    #[test]
    fn grid_with_gap() {
        let mut parent = make_grid_box(300.0, 300.0);
        parent.grid_template_columns = "1fr 1fr".into();
        parent.column_gap = 20.0;
        parent.row_gap = 10.0;
        for _ in 0..4 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // 2 cols s gap 20: cell_w = (300 - 20) / 2 = 140
        assert!((parent.children[1].rect.x - (140.0 + 20.0)).abs() < 1.0);
    }

    #[test]
    fn grid_named_lines_in_template_dont_crash() {
        let mut parent = make_grid_box(300.0, 300.0);
        parent.grid_template_columns = "[start] 1fr [middle] 2fr [end]".into();
        for _ in 0..2 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // 2 tracks
        assert!(parent.children[0].rect.x < parent.children[1].rect.x);
    }

    #[test]
    fn grid_single_column_default() {
        let mut parent = make_grid_box(300.0, 300.0);
        // Bez explicit grid-template-columns -> 1 column
        for _ in 0..3 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // Vsechny stejny x
        assert_eq!(parent.children[0].rect.x, parent.children[2].rect.x);
        // Stack vertically
        assert!(parent.children[2].rect.y > parent.children[0].rect.y);
    }

    #[test]
    fn grid_empty_no_crash() {
        let mut parent = make_grid_box(300.0, 300.0);
        parent.grid_template_columns = "1fr 1fr".into();
        layout_grid(&mut parent);
        assert_eq!(parent.children.len(), 0);
    }

    #[test]
    fn grid_updates_parent_height() {
        let mut parent = make_grid_box(300.0, 0.0);
        parent.grid_template_columns = "1fr 1fr".into();
        for _ in 0..4 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // 2 rows * 50 default = 100
        assert!(parent.rect.height >= 100.0);
    }

    #[test]
    fn grid_4_columns_with_explicit_widths() {
        let mut parent = make_grid_box(400.0, 200.0);
        parent.grid_template_columns = "100px 100px 100px 100px".into();
        for _ in 0..4 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // Vsech 4 cols, kazdy 100 px (rovnomerne pres count)
        assert_eq!(parent.children[0].rect.x, 0.0);
        assert!((parent.children[3].rect.x - 300.0).abs() < 1.0);
    }

    // ─── Track resolution (fr / %, repeat, minmax) ────────────────────────

    #[test]
    fn grid_resolve_tracks_fr_distributes_free_space() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("1fr 2fr 1fr", 400.0, 0.0);
        assert_eq!(tracks.len(), 3);
        // 4 fr total, free 400 -> base 100. tracks: 100, 200, 100
        assert!((tracks[0] - 100.0).abs() < 1.0);
        assert!((tracks[1] - 200.0).abs() < 1.0);
        assert!((tracks[2] - 100.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_with_fixed_and_fr() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("100px 1fr 1fr", 500.0, 0.0);
        // Fixed 100, free 400, 2 fr -> 200 each
        assert!((tracks[0] - 100.0).abs() < 1.0);
        assert!((tracks[1] - 200.0).abs() < 1.0);
        assert!((tracks[2] - 200.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_with_percent() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("25% 50% 25%", 400.0, 0.0);
        assert!((tracks[0] - 100.0).abs() < 1.0);
        assert!((tracks[1] - 200.0).abs() < 1.0);
        assert!((tracks[2] - 100.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_with_repeat() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("repeat(3, 1fr)", 300.0, 0.0);
        assert_eq!(tracks.len(), 3);
        assert!((tracks[0] - 100.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_with_repeat_mixed() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("100px repeat(2, 1fr) 50px", 400.0, 0.0);
        assert_eq!(tracks.len(), 4);
        assert!((tracks[0] - 100.0).abs() < 1.0);
        assert!((tracks[3] - 50.0).abs() < 1.0);
        // free = 400 - 150 = 250, 2fr -> 125 each
        assert!((tracks[1] - 125.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_with_minmax() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("minmax(100px, 1fr) minmax(50px, 200px)", 400.0, 0.0);
        // minmax vrati max
        assert_eq!(tracks.len(), 2);
    }

    #[test]
    fn grid_resolve_tracks_with_gap_reduces_free() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("1fr 1fr", 200.0, 20.0);
        // 200 - 20 gap = 180 free, 90 each
        assert!((tracks[0] - 90.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_auto() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("auto auto auto", 300.0, 0.0);
        // 3 auto, 300 / 3 = 100 each
        assert!((tracks[0] - 100.0).abs() < 1.0);
    }

    #[test]
    fn grid_resolve_tracks_named_lines_skipped() {
        use crate::browser::layout_engine::grid::resolve_tracks;
        let tracks = resolve_tracks("[start] 1fr [middle] 1fr [end]", 200.0, 0.0);
        assert_eq!(tracks.len(), 2);
        assert!((tracks[0] - 100.0).abs() < 1.0);
    }

    #[test]
    fn grid_with_fr_layout() {
        let mut parent = make_grid_box(400.0, 200.0);
        parent.grid_template_columns = "1fr 2fr 1fr".into();
        for _ in 0..3 { parent.children.push(make_child(0.0, 50.0)); }
        layout_grid(&mut parent);
        // Item 0: 100, item 1: 200, item 2: 100
        assert!((parent.children[0].rect.width - 100.0).abs() < 1.0);
        assert!((parent.children[1].rect.width - 200.0).abs() < 1.0);
        assert!((parent.children[2].rect.width - 100.0).abs() < 1.0);
    }

    /// Repro from mileneckaseznamka.cz - right-container display=grid (auto-flow row)
    /// has top-container with display=flex + min-height:93px. Grid auto-row should
    /// size to top-container intrinsic (~120) NOT inflate to viewport h.
    #[test]
    fn grid_auto_row_flex_child_intrinsic_not_inflated() {
        // Outer grid: 1 row implicit, 1 col, no template -> grid-auto-rows: auto
        let mut grid = make_grid_box(925.0, 0.0);  // rect.h=0 -> auto height
        // Make it explicit height=0 so any-auto-row branch fires.
        grid.explicit_height = None;
        grid.align_content = crate::browser::layout::AlignContent::parse("flex-start");

        // top-container: flex row, min-height 93, padding 10/20, 1 child h=100
        let mut top = LayoutBox::new();
        top.display = Display::Flex;
        top.flex_direction = crate::browser::layout::FlexDirection::parse("row");
        top.align_items = crate::browser::layout::AlignItems::parse("center");
        top.justify_content = crate::browser::layout::JustifyContent::parse("space-between");
        top.padding_top = Some(10.0);
        top.padding_bottom = Some(10.0);
        top.padding_left = Some(20.0);
        top.padding_right = Some(20.0);
        top.min_height = crate::browser::layout::CssLength::Px(93.0);
        // Add child with explicit h=100 (simulates img.republic-map)
        let mut img_block = LayoutBox::new();
        img_block.display = Display::Flex;
        img_block.flex_direction = crate::browser::layout::FlexDirection::parse("row");
        let mut img = LayoutBox::new();
        img.display = Display::Block;
        img.explicit_width = Some(100.0);
        img.explicit_height = Some(100.0);
        img_block.children.push(img);
        top.children.push(img_block);

        grid.children.push(top);
        layout_grid(&mut grid);

        let h = grid.children[0].rect.height;
        eprintln!("top-container rect.h={:.0} grid.h={:.0}", h, grid.rect.height);
        assert!(h >= 93.0 && h <= 200.0,
            "top-container rect.h should be ~120 (content+padding) or >=93 (min-height), got {}", h);
    }
}
