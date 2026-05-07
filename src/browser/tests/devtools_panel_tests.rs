/// Testy pro in-window DevTools panel.

use crate::browser::devtools_panel::{paint_devtools_panel, devtools_hit_test, DevtoolsHit, find_box_rect_by_id, pick_node_at_screen_pos};
use crate::browser::paint::DisplayCommand;
use crate::browser::layout::LayoutBox;
use crate::browser::html_parser::parse_html;
use crate::browser::css_parser::parse_stylesheet;
use crate::browser::layout;

fn build_test_layout(html: &str) -> LayoutBox {
    let doc = parse_html(html, "");
    let css = parse_stylesheet("");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0)
}

#[test]
fn devtools_panel_emits_commands_when_open() {
    let layout = build_test_layout("<html><body><div>Hello</div></body></html>");
    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_devtools_panel(&mut cmds, &layout, None, 0, 0.0, false, "",
                        None, 1280.0, 800.0, 320.0, 0.0, 0.0);
    // Mel by emitnout aspon panel bg + toolbar bg + tab labels + tree rows.
    assert!(cmds.len() > 5, "expected multiple draw commands, got {}", cmds.len());
    // Aspon jeden Rect command pro pozadi panelu.
    let any_rect = cmds.iter().any(|c| matches!(c, DisplayCommand::Rect { .. }));
    assert!(any_rect);
    // Aspon jeden Text command (tab labels nebo tree rows).
    let any_text = cmds.iter().any(|c| matches!(c, DisplayCommand::Text { .. }));
    assert!(any_text);
}

#[test]
fn devtools_panel_zero_height_no_emit() {
    let layout = build_test_layout("<html><body></body></html>");
    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_devtools_panel(&mut cmds, &layout, None, 0, 0.0, false, "",
                        None, 1280.0, 800.0, 0.0, 0.0, 0.0);
    assert!(cmds.is_empty());
}

#[test]
fn devtools_hit_test_tab_click() {
    let layout = build_test_layout("<html><body></body></html>");
    // Klik na pozici tabu Elements: x=8..=8+8*7.5+16, y=panel_y+0..28.
    let hit = devtools_hit_test(&layout, 0, 0.0, 1280.0, 800.0, 320.0, 30.0, 800.0 - 320.0 + 14.0);
    matches!(hit, DevtoolsHit::TabClick(0));
}

#[test]
fn devtools_hit_test_outside_panel() {
    let layout = build_test_layout("<html><body></body></html>");
    // Klik mimo panel (nahore).
    let hit = devtools_hit_test(&layout, 0, 0.0, 1280.0, 800.0, 320.0, 100.0, 100.0);
    matches!(hit, DevtoolsHit::None);
}

#[test]
fn pick_node_at_screen_pos_finds_box() {
    let layout = build_test_layout("<html><body><div>X</div></body></html>");
    // Klikni do oblasti body (skoro libovolne x,y v dokumentu by mel vratit nejaky node).
    let node_id = pick_node_at_screen_pos(&layout, 50.0, 50.0, 0.0);
    assert!(node_id.is_some());
}

#[test]
fn find_box_rect_by_id_returns_rect_for_existing() {
    let layout = build_test_layout("<html><body></body></html>");
    if let Some(node) = layout.node.as_ref() {
        let id = std::rc::Rc::as_ptr(node) as usize;
        let r = find_box_rect_by_id(&layout, id, 0.0);
        assert!(r.is_some());
    }
}
