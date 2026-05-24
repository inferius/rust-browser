/// Flex layout testy - inspirovano CSS Flexbox L1 spec test cases + taffy.
/// (taffy MIT licence, https://github.com/DioxusLabs/taffy)
///
/// Testujeme flex algoritmus per spec 9.7 Layout Algorithm.

#[cfg(test)]
mod tests {
    use crate::browser::layout::*;
    use crate::browser::layout_engine::flex::layout_flex;

    fn make_flex_box(width: f32, height: f32) -> LayoutBox {
        let mut bx = LayoutBox::new();
        bx.rect.width = width;
        bx.rect.height = height;
        bx.display = Display::Flex;
        bx.flex_direction = crate::browser::layout::FlexDirection::parse("row");
        bx
    }

    fn make_child(w: f32, h: f32) -> LayoutBox {
        let mut bx = LayoutBox::new();
        bx.explicit_width = Some(w);
        bx.explicit_height = Some(h);
        bx
    }

    // ─── Direction tests ───────────────────────────────────────────────────

    #[test]
    fn flex_row_basic_layout() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // V row direction children stackuji horizontalne
        assert!(parent.children[0].rect.x < parent.children[1].rect.x);
        assert!(parent.children[1].rect.x < parent.children[2].rect.x);
    }

    #[test]
    fn flex_column_stacks_vertically() {
        let mut parent = make_flex_box(100.0, 300.0);
        parent.flex_direction = crate::browser::layout::FlexDirection::parse("column");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        assert!(parent.children[0].rect.y < parent.children[1].rect.y);
        assert!(parent.children[1].rect.y < parent.children[2].rect.y);
    }

    #[test]
    fn flex_row_reverse() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.flex_direction = crate::browser::layout::FlexDirection::parse("row-reverse");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // Reverse - prvni je vpravo
        assert!(parent.children[0].rect.x > parent.children[1].rect.x);
    }

    #[test]
    fn flex_column_reverse() {
        let mut parent = make_flex_box(100.0, 300.0);
        parent.flex_direction = crate::browser::layout::FlexDirection::parse("column-reverse");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        assert!(parent.children[0].rect.y > parent.children[1].rect.y);
    }

    // ─── Wrap tests ────────────────────────────────────────────────────────

    #[test]
    fn flex_no_wrap_overflow() {
        let mut parent = make_flex_box(100.0, 100.0);
        parent.flex_wrap = crate::browser::layout::FlexWrap::parse("nowrap");
        for _ in 0..3 { parent.children.push(make_child(50.0, 30.0)); }
        layout_flex(&mut parent);
        // Vsechny na 1 line (children y vsichni stejne)
        assert_eq!(parent.children[0].rect.y, parent.children[2].rect.y);
    }

    #[test]
    fn flex_wrap_to_new_line() {
        let mut parent = make_flex_box(100.0, 100.0);
        parent.flex_wrap = crate::browser::layout::FlexWrap::parse("wrap");
        for _ in 0..3 { parent.children.push(make_child(60.0, 30.0)); }
        layout_flex(&mut parent);
        // 3 itemy 60px do 100px container -> 1 per line
        assert!(parent.children[1].rect.y > parent.children[0].rect.y);
    }

    // ─── Justify-content tests ─────────────────────────────────────────────

    #[test]
    fn flex_justify_content_flex_start() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.justify_content = crate::browser::layout::JustifyContent::parse("flex-start");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // Prvni dite zacne na x=0 (relative k inner)
        assert_eq!(parent.children[0].rect.x, 0.0);
    }

    #[test]
    fn flex_justify_content_center() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.justify_content = crate::browser::layout::JustifyContent::parse("center");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // 100 used, 200 free, center -> shift 100
        assert!(parent.children[0].rect.x > 50.0);
    }

    #[test]
    fn flex_justify_content_flex_end() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.justify_content = crate::browser::layout::JustifyContent::parse("flex-end");
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // 1 item 50px, container 300 -> shift 250
        assert!(parent.children[0].rect.x > 200.0);
    }

    #[test]
    fn flex_justify_space_between() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.justify_content = crate::browser::layout::JustifyContent::parse("space-between");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // Prvni na x=0, posledni na konci
        assert_eq!(parent.children[0].rect.x, 0.0);
        // 300 - 150 = 150 free / 2 between = 75 each. last x = 0 + 50 + 75 + 50 + 75 = 250
        assert!((parent.children[2].rect.x - 250.0).abs() < 0.5);
    }

    #[test]
    fn flex_justify_space_around() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.justify_content = crate::browser::layout::JustifyContent::parse("space-around");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // 300 - 150 = 150 free / 3 = 50 per gap, half on edges = 25
        assert!((parent.children[0].rect.x - 25.0).abs() < 0.5);
    }

    #[test]
    fn flex_justify_space_evenly() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.justify_content = crate::browser::layout::JustifyContent::parse("space-evenly");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // 300 - 150 = 150 free / 4 (3+1) = 37.5 per gap
        assert!((parent.children[0].rect.x - 37.5).abs() < 0.5);
    }

    // ─── Align-items tests ─────────────────────────────────────────────────

    #[test]
    fn flex_align_items_flex_start() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.align_items = crate::browser::layout::AlignItems::parse("flex-start");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 50.0));
        layout_flex(&mut parent);
        // Item s vetsi cross size urcuje line, mensi item je nahore
        assert_eq!(parent.children[0].rect.y, parent.children[1].rect.y);
    }

    #[test]
    fn flex_align_items_center() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.align_items = crate::browser::layout::AlignItems::parse("center");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 50.0));
        layout_flex(&mut parent);
        // 30px item v 50px line -> centered (offset 10)
        assert!(parent.children[0].rect.y > parent.children[1].rect.y);
    }

    #[test]
    fn flex_align_items_flex_end() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.align_items = crate::browser::layout::AlignItems::parse("flex-end");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 50.0));
        layout_flex(&mut parent);
        // Item bottom -> y bigger pro mensi
        let bigger_bottom = parent.children[1].rect.y + parent.children[1].rect.height;
        let smaller_bottom = parent.children[0].rect.y + parent.children[0].rect.height;
        assert!((bigger_bottom - smaller_bottom).abs() < 0.5);
    }

    #[test]
    fn flex_align_items_stretch() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.align_items = crate::browser::layout::AlignItems::parse("stretch");
        // Itemy bez explicit height
        let mut a = LayoutBox::new();
        a.explicit_width = Some(50.0);
        let mut b = LayoutBox::new();
        b.explicit_width = Some(50.0);
        parent.children.push(a);
        parent.children.push(b);
        layout_flex(&mut parent);
        // Stretch -> oba prijmou line cross size
        assert_eq!(parent.children[0].rect.height, parent.children[1].rect.height);
    }

    // ─── Gap tests ─────────────────────────────────────────────────────────

    #[test]
    fn flex_gap_horizontal() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.column_gap = 20.0;
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // Druhy item: x = 0 + 50 + 20 = 70
        assert!((parent.children[1].rect.x - 70.0).abs() < 0.5);
    }

    #[test]
    fn flex_gap_vertical_in_wrap() {
        let mut parent = make_flex_box(100.0, 300.0);
        parent.flex_wrap = crate::browser::layout::FlexWrap::parse("wrap");
        parent.row_gap = 15.0;
        parent.align_content = crate::browser::layout::AlignContent::parse("flex-start"); // bez stretch
        for _ in 0..3 { parent.children.push(make_child(60.0, 30.0)); }
        layout_flex(&mut parent);
        // 60px item, 100 container -> 1 per line
        // line 1: y=0, line 2: y = 30 + 15 = 45
        assert!((parent.children[1].rect.y - 45.0).abs() < 0.5);
    }

    // ─── Flex-grow / shrink ────────────────────────────────────────────────

    #[test]
    fn flex_grow_distributes_free_space_equally() {
        let mut parent = make_flex_box(300.0, 100.0);
        let mut a = make_child(50.0, 30.0);
        a.flex_grow = 1.0;
        let mut b = make_child(50.0, 30.0);
        b.flex_grow = 1.0;
        parent.children.push(a);
        parent.children.push(b);
        layout_flex(&mut parent);
        // 300 - 100 = 200 free, distribute 50/50 -> kazdy 50+100=150
        assert!((parent.children[0].rect.width - 150.0).abs() < 0.5);
        assert!((parent.children[1].rect.width - 150.0).abs() < 0.5);
    }

    #[test]
    fn flex_grow_proportional() {
        let mut parent = make_flex_box(300.0, 100.0);
        let mut a = make_child(50.0, 30.0);
        a.flex_grow = 1.0;
        let mut b = make_child(50.0, 30.0);
        b.flex_grow = 2.0;
        parent.children.push(a);
        parent.children.push(b);
        layout_flex(&mut parent);
        // 200 free, 1:2 -> a=66.67, b=133.33; final widths a=116.67, b=183.33
        assert!((parent.children[0].rect.width - 116.67).abs() < 1.0);
        assert!((parent.children[1].rect.width - 183.33).abs() < 1.0);
    }

    #[test]
    fn flex_grow_zero_no_distribute() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        // Bez flex-grow zustanou puvodni
        assert!((parent.children[0].rect.width - 50.0).abs() < 0.5);
    }

    #[test]
    fn flex_shrink_when_overflowing() {
        let mut parent = make_flex_box(100.0, 100.0);
        parent.flex_wrap = crate::browser::layout::FlexWrap::parse("nowrap");
        let mut a = make_child(80.0, 30.0);
        a.flex_shrink = 1.0;
        let mut b = make_child(80.0, 30.0);
        b.flex_shrink = 1.0;
        parent.children.push(a);
        parent.children.push(b);
        layout_flex(&mut parent);
        // 160 v 100 -> shrink. Each pri sirku < 80
        assert!(parent.children[0].rect.width < 80.0);
        assert!(parent.children[1].rect.width < 80.0);
    }

    // ─── Single child / empty ──────────────────────────────────────────────

    #[test]
    fn flex_single_child_basic() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.children.push(make_child(50.0, 30.0));
        layout_flex(&mut parent);
        assert_eq!(parent.children[0].rect.x, 0.0);
        assert_eq!(parent.children[0].rect.y, 0.0);
    }

    #[test]
    fn flex_no_children_doesnt_crash() {
        let mut parent = make_flex_box(300.0, 100.0);
        layout_flex(&mut parent);
        // Just confirm no crash
        assert_eq!(parent.children.len(), 0);
    }

    // ─── Combined scenarios ────────────────────────────────────────────────

    #[test]
    fn flex_grow_with_gap() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.column_gap = 10.0;
        let mut a = make_child(50.0, 30.0);
        a.flex_grow = 1.0;
        let mut b = make_child(50.0, 30.0);
        b.flex_grow = 1.0;
        parent.children.push(a);
        parent.children.push(b);
        layout_flex(&mut parent);
        // 300 - 100 - 10 = 190 free, kazdy +95 -> 145 per item
        assert!((parent.children[0].rect.width - 145.0).abs() < 1.0);
    }

    #[test]
    fn flex_wrap_multi_line_align_center() {
        let mut parent = make_flex_box(100.0, 300.0);
        parent.flex_wrap = crate::browser::layout::FlexWrap::parse("wrap");
        parent.justify_content = crate::browser::layout::JustifyContent::parse("center");
        for _ in 0..4 { parent.children.push(make_child(40.0, 30.0)); }
        layout_flex(&mut parent);
        // 40 + 40 = 80 v 100 = 20 free, center na line shift 10
        assert!(parent.children[0].rect.x > 5.0);
        assert!(parent.children[2].rect.y > parent.children[1].rect.y);
    }

    #[test]
    fn flex_column_with_align_items() {
        let mut parent = make_flex_box(200.0, 300.0);
        parent.flex_direction = crate::browser::layout::FlexDirection::parse("column");
        parent.align_items = crate::browser::layout::AlignItems::parse("center");
        parent.children.push(make_child(50.0, 30.0));
        parent.children.push(make_child(80.0, 30.0));
        layout_flex(&mut parent);
        // V column align cross axis = horizontal centering
        // 50px item v 200 -> shift 75 z startu. Test ze itemy maji ruzne x pri ruznych sizes
        assert!(parent.children[0].rect.x > parent.children[1].rect.x);
    }

    // ─── Updates parent height ─────────────────────────────────────────────

    #[test]
    fn flex_updates_parent_height() {
        let mut parent = make_flex_box(300.0, 0.0);
        parent.children.push(make_child(50.0, 100.0));
        layout_flex(&mut parent);
        assert!(parent.rect.height >= 100.0);
    }

    // ─── CSS `order` property ──────────────────────────────────────────────

    #[test]
    fn flex_order_reverses_items() {
        let mut parent = make_flex_box(300.0, 100.0);
        // DOM index 0=a(order 2), 1=b(order 1), 2=c(order 0).
        // Sortováno: c, b, a. Visually: c.x=0, b.x=50, a.x=100.
        // DOM-indexed: children[0]=a má největší x, children[2]=c má x=0.
        let mut a = make_child(50.0, 30.0); a.flex_order = 2;
        let mut b = make_child(50.0, 30.0); b.flex_order = 1;
        let mut c = make_child(50.0, 30.0); c.flex_order = 0;
        parent.children.push(a);
        parent.children.push(b);
        parent.children.push(c);
        layout_flex(&mut parent);
        assert!(parent.children[0].rect.x > parent.children[1].rect.x);
        assert!(parent.children[1].rect.x > parent.children[2].rect.x);
    }

    #[test]
    fn flex_order_negative_moves_to_front() {
        let mut parent = make_flex_box(300.0, 100.0);
        parent.children.push(make_child(50.0, 30.0));  // order 0
        let mut second = make_child(50.0, 30.0); second.flex_order = -1;
        parent.children.push(second);
        layout_flex(&mut parent);
        // children[1] má order=-1 → ve flex pořadí první → x=0
        // children[0] má order=0 → druhé místo → x=50
        assert!(parent.children[1].rect.x < parent.children[0].rect.x);
    }

    #[test]
    fn flex_wrap_updates_height_with_multi_line() {
        let mut parent = make_flex_box(100.0, 0.0);
        parent.flex_wrap = crate::browser::layout::FlexWrap::parse("wrap");
        for _ in 0..3 { parent.children.push(make_child(60.0, 30.0)); }
        layout_flex(&mut parent);
        // 3 lines * 30 = 90 + padding
        assert!(parent.rect.height >= 90.0);
    }
}
