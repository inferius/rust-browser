/// Testy pro in-window DevTools panel.

use crate::browser::devtools_panel::{paint_devtools_panel, devtools_hit_test, DevtoolsHit, find_box_rect_by_id, pick_node_at_screen_pos, rebuild_tree};
use crate::browser::paint::DisplayCommand;
use crate::browser::layout::LayoutBox;
use crate::browser::html_parser::parse_html;
use crate::browser::css_parser::parse_stylesheet;
use crate::browser::layout;
use crate::devtools::{DevToolsState, Tab};

fn build_test_layout(html: &str) -> (LayoutBox, std::rc::Rc<crate::browser::dom::NodeData>) {
    let doc = parse_html(html, "");
    let css = parse_stylesheet("");
    let style_map = crate::browser::cascade::cascade(&doc.root, &[css]);
    let lr = layout::layout_tree(&doc.root, &style_map, 1024.0, 768.0);
    (lr, doc.root)
}

fn make_state(root: &std::rc::Rc<crate::browser::dom::NodeData>) -> DevToolsState {
    let mut s = DevToolsState::default();
    s.panel_open = true;
    rebuild_tree(&mut s, root);
    s
}

#[test]
fn devtools_panel_emits_commands_when_open() {
    let (layout, root) = build_test_layout("<html><body><div>Hello</div></body></html>");
    let state = make_state(&root);
    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_devtools_panel(&mut cmds, &layout, &state, None, 1280.0, 800.0, 0.0, 0.0);
    assert!(cmds.len() > 5, "expected multiple draw commands, got {}", cmds.len());
    assert!(cmds.iter().any(|c| matches!(c, DisplayCommand::Rect { .. })));
    assert!(cmds.iter().any(|c| matches!(c, DisplayCommand::Text { .. })));
}

#[test]
fn devtools_panel_closed_no_emit() {
    let (layout, root) = build_test_layout("<html><body></body></html>");
    let mut state = make_state(&root);
    state.panel_open = false;
    let mut cmds: Vec<DisplayCommand> = Vec::new();
    paint_devtools_panel(&mut cmds, &layout, &state, None, 1280.0, 800.0, 0.0, 0.0);
    assert!(cmds.is_empty());
}

#[test]
fn devtools_hit_test_tab_click() {
    let (layout, root) = build_test_layout("<html><body></body></html>");
    let state = make_state(&root);
    // Tab Elements je na x ~= 8..(8 + len * 7 + 18). Klik dovnitr.
    let panel_y = 800.0 - state.panel_h;
    let hit = devtools_hit_test(&state, &layout, 1280.0, 800.0, 30.0, panel_y + 14.0);
    if let DevtoolsHit::TabClick(t) = hit {
        assert_eq!(t, Tab::Elements);
    } else {
        panic!("expected TabClick, got {:?}", hit);
    }
}

#[test]
fn devtools_hit_test_outside_panel() {
    let (layout, root) = build_test_layout("<html><body></body></html>");
    let state = make_state(&root);
    // Klik mimo panel (nahore).
    let hit = devtools_hit_test(&state, &layout, 1280.0, 800.0, 100.0, 100.0);
    assert!(matches!(hit, DevtoolsHit::None));
}

#[test]
fn pick_node_at_screen_pos_finds_box() {
    let (layout, _root) = build_test_layout("<html><body><div>X</div></body></html>");
    let node_id = pick_node_at_screen_pos(&layout, 50.0, 50.0, 0.0);
    assert!(node_id.is_some());
}

#[test]
fn find_box_rect_by_id_returns_rect_for_existing() {
    let (layout, _root) = build_test_layout("<html><body></body></html>");
    if let Some(node) = layout.node.as_ref() {
        let id = std::rc::Rc::as_ptr(node) as usize;
        let r = find_box_rect_by_id(&layout, id, 0.0);
        assert!(r.is_some());
    }
}

#[test]
fn rebuild_tree_includes_text_nodes() {
    let (_layout, root) = build_test_layout("<html><body><p>Hello world</p></body></html>");
    let mut state = DevToolsState::default();
    rebuild_tree(&mut state, &root);
    use crate::devtools::model::elements::RowKind;
    let has_text = state.elements.rows.iter().any(|r|
        if let RowKind::Text(t) = &r.kind { t.contains("Hello") } else { false });
    assert!(has_text, "text node 'Hello world' should be in rows");
}
