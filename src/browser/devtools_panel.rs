//! In-window DevTools panel - Chrome-like overlay nad spodnim okrajem okna.
//!
//! 3 taby: Elements, Console, Network. F12 toggle. Two-way binding mezi tree a viewport:
//! - Click v tree -> highlight rect na viewport
//! - Click ve viewport (inspect mode) -> select v tree
//!
//! Renderuje se jako sada paint::DisplayCommand (Rect, Border, Text). Hit-test pres
//! get_devtools_hit + handle_devtools_click.

use std::rc::Rc;
use crate::browser::paint::DisplayCommand;
use crate::browser::layout::LayoutBox;
use crate::browser::dom::NodeData;
use crate::interpreter::Interpreter;

const PANEL_BG: [u8; 4] = [40, 42, 50, 250];
const PANEL_BG_LIGHT: [u8; 4] = [55, 58, 68, 255];
const TAB_BG_ACTIVE: [u8; 4] = [70, 75, 90, 255];
const BORDER_COLOR: [u8; 4] = [30, 32, 38, 255];
const TEXT_COLOR: [u8; 4] = [220, 222, 230, 255];
const TEXT_DIM: [u8; 4] = [150, 152, 160, 255];
const TEXT_TAG: [u8; 4] = [120, 200, 255, 255];
const TEXT_ATTR: [u8; 4] = [220, 180, 120, 255];
const TEXT_VAL: [u8; 4] = [200, 240, 150, 255];
const TEXT_SELECTED_BG: [u8; 4] = [70, 110, 180, 255];
const TAB_TEXT: [u8; 4] = [180, 182, 190, 255];

const ROW_H: f32 = 18.0;
const TAB_H: f32 = 28.0;
const FONT_SIZE: f32 = 12.0;
const INDENT_PX: f32 = 14.0;
pub const RESIZE_GRIP_H: f32 = 4.0;

/// Vykresli panel dolu pres celou sirku. Vola se z render flow.
pub fn paint_devtools_panel(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    selected_id: Option<usize>,
    tab: u8,
    tree_scroll: f32,
    inspect_mode: bool,
    console_input: &str,
    interp: Option<&Interpreter>,
    win_w: f32,
    win_h: f32,
    panel_h: f32,
    _mouse_x: f32, _mouse_y: f32,
) {
    if panel_h <= 0.0 { return; }
    let panel_y = win_h - panel_h;

    // Pozadi panelu.
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: panel_y, w: win_w, h: panel_h,
        color: PANEL_BG, radius: 0.0,
    });
    // Resize grip - 4px tall draggable area at top edge.
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: panel_y, w: win_w, h: RESIZE_GRIP_H,
        color: [80, 84, 96, 255], radius: 0.0,
    });
    // Hint dots in middle.
    let dots_x = win_w * 0.5 - 12.0;
    for i in 0..3 {
        cmds.push(DisplayCommand::Rect {
            x: dots_x + (i as f32) * 8.0, y: panel_y + 1.0,
            w: 4.0, h: 2.0,
            color: [140, 144, 156, 255], radius: 1.0,
        });
    }

    // Toolbar - taby + inspect button. Posunuto pod resize grip.
    let toolbar_y = panel_y + RESIZE_GRIP_H;
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: toolbar_y, w: win_w, h: TAB_H,
        color: PANEL_BG_LIGHT, radius: 0.0,
    });

    let tabs = ["Elements", "Console", "Network"];
    let mut tab_x = 8.0;
    for (i, name) in tabs.iter().enumerate() {
        let tab_w = (name.len() as f32) * 7.5 + 16.0;
        let active = tab == i as u8;
        if active {
            cmds.push(DisplayCommand::Rect {
                x: tab_x, y: toolbar_y + 4.0, w: tab_w, h: TAB_H - 4.0,
                color: TAB_BG_ACTIVE, radius: 4.0,
            });
        }
        cmds.push(DisplayCommand::Text {
            x: tab_x + 8.0, y: toolbar_y + 8.0,
            content: name.to_string(),
            color: if active { TEXT_COLOR } else { TAB_TEXT },
            font_size: FONT_SIZE, bold: active,
            italic: false,
            font_family: String::new(),
            strikethrough: false, underline: false,
        });
        tab_x += tab_w + 4.0;
    }

    // Clear button (pri Console tab).
    if tab == 1 {
        let clr_x = win_w - 200.0;
        cmds.push(DisplayCommand::Rect {
            x: clr_x, y: toolbar_y + 4.0, w: 92.0, h: TAB_H - 4.0,
            color: PANEL_BG, radius: 4.0,
        });
        cmds.push(DisplayCommand::Text {
            x: clr_x + 18.0, y: toolbar_y + 8.0,
            content: "Clear".to_string(),
            color: TEXT_COLOR,
            font_size: FONT_SIZE, bold: false,
            italic: false,
            font_family: String::new(),
            strikethrough: false, underline: false,
        });
    }
    // Inspect mode toggle button (vpravo).
    let insp_x = win_w - 100.0;
    cmds.push(DisplayCommand::Rect {
        x: insp_x, y: toolbar_y + 4.0, w: 92.0, h: TAB_H - 4.0,
        color: if inspect_mode { TEXT_SELECTED_BG } else { PANEL_BG },
        radius: 4.0,
    });
    cmds.push(DisplayCommand::Text {
        x: insp_x + 8.0, y: toolbar_y + 8.0,
        content: if inspect_mode { "Inspect ON".to_string() } else { "Inspect".to_string() },
        color: TEXT_COLOR,
        font_size: FONT_SIZE, bold: false,
        italic: false,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });

    // Content area.
    let content_y = toolbar_y + TAB_H;
    let content_h = panel_h - TAB_H;

    match tab {
        0 => paint_elements_tab(cmds, layout_root, selected_id, tree_scroll, win_w, content_y, content_h),
        1 => paint_console_tab(cmds, console_input, interp, win_w, content_y, content_h),
        2 => paint_network_tab(cmds, interp, win_w, content_y, content_h),
        _ => {}
    }
}

/// Elements tab: DOM tree + (optional) styles panel napravo.
fn paint_elements_tab(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    selected_id: Option<usize>,
    tree_scroll: f32,
    win_w: f32, content_y: f32, content_h: f32,
) {
    // Dva sloupce: tree (60%) + styles (40%).
    let split_x = win_w * 0.6;

    // Tree pozadi.
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: content_y, w: split_x, h: content_h,
        color: PANEL_BG, radius: 0.0,
    });
    // Right styles pozadi.
    cmds.push(DisplayCommand::Rect {
        x: split_x, y: content_y, w: win_w - split_x, h: content_h,
        color: PANEL_BG_LIGHT, radius: 0.0,
    });
    // Vertikalni separator.
    cmds.push(DisplayCommand::Rect {
        x: split_x, y: content_y, w: 1.0, h: content_h,
        color: BORDER_COLOR, radius: 0.0,
    });

    // Build flat tree list pro zobrazeni.
    let mut rows: Vec<TreeRow> = Vec::new();
    if let Some(node) = layout_root.node.as_ref() {
        walk_dom(node, 0, &mut rows);
    }

    // Render rows visible v scroll window.
    let visible_rows = (content_h / ROW_H) as usize + 1;
    let start_row = (tree_scroll / ROW_H) as usize;
    for (visual_idx, row_idx) in (start_row..rows.len().min(start_row + visible_rows)).enumerate() {
        let row = &rows[row_idx];
        let y = content_y + (visual_idx as f32) * ROW_H;
        let is_sel = selected_id.map(|s| s == row.node_id).unwrap_or(false);
        if is_sel {
            cmds.push(DisplayCommand::Rect {
                x: 0.0, y, w: split_x, h: ROW_H,
                color: TEXT_SELECTED_BG, radius: 0.0,
            });
        }
        let x = 8.0 + row.depth as f32 * INDENT_PX;
        // Tag + atributy formatovany jako <tag attr="val">
        cmds.push(DisplayCommand::Text {
            x, y: y + 3.0,
            content: format!("<{}", row.tag),
            color: TEXT_TAG,
            font_size: FONT_SIZE, bold: false,
            italic: false,
            font_family: String::new(),
            strikethrough: false, underline: false,
        });
        let mut tx = x + (row.tag.len() as f32 + 1.0) * 7.0;
        for (k, v) in &row.attrs {
            let attr_str = format!(" {}=", k);
            cmds.push(DisplayCommand::Text {
                x: tx, y: y + 3.0,
                content: attr_str.clone(),
                color: TEXT_ATTR,
                font_size: FONT_SIZE, bold: false,
                italic: false,
                font_family: String::new(),
                strikethrough: false, underline: false,
            });
            tx += attr_str.len() as f32 * 7.0;
            let val_str = format!("\"{}\"", v);
            cmds.push(DisplayCommand::Text {
                x: tx, y: y + 3.0,
                content: val_str.clone(),
                color: TEXT_VAL,
                font_size: FONT_SIZE, bold: false,
                italic: false,
                font_family: String::new(),
                strikethrough: false, underline: false,
            });
            tx += val_str.len() as f32 * 7.0;
            if tx > split_x - 50.0 { break; }
        }
        cmds.push(DisplayCommand::Text {
            x: tx, y: y + 3.0,
            content: ">".to_string(),
            color: TEXT_TAG,
            font_size: FONT_SIZE, bold: false,
            italic: false,
            font_family: String::new(),
            strikethrough: false, underline: false,
        });
    }

    // Right panel: pri vybranem elementu zobraz computed styles.
    if let Some(sel_id) = selected_id {
        if let Some(bx) = find_layout_box(layout_root, sel_id) {
            let mut sy = content_y + 8.0;
            let style_rows = vec![
                ("display", format!("{:?}", bx.display)),
                ("position", format!("{:?}", bx.position)),
                ("rect", format!("x={:.0} y={:.0} w={:.0} h={:.0}",
                    bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height)),
                ("padding", format!("{:.0}", bx.padding)),
                ("margin", format!("{:.0}", bx.margin)),
                ("font-size", format!("{:.0}px", bx.font_size)),
                ("line-height", format!("{:.2}", bx.line_height)),
                ("color", match bx.text_color {
                    Some(c) => format!("rgb({},{},{})", c[0], c[1], c[2]),
                    None => "(default)".to_string(),
                }),
                ("bg-color", match bx.bg_color {
                    Some(c) => format!("rgba({},{},{},{})", c[0], c[1], c[2], c[3]),
                    None => "(transparent)".to_string(),
                }),
            ];
            for (k, v) in &style_rows {
                if sy + ROW_H > content_y + content_h { break; }
                cmds.push(DisplayCommand::Text {
                    x: split_x + 8.0, y: sy,
                    content: format!("{}:", k),
                    color: TEXT_ATTR,
                    font_size: FONT_SIZE, bold: false,
                    italic: false,
                    font_family: String::new(),
                    strikethrough: false, underline: false,
                });
                cmds.push(DisplayCommand::Text {
                    x: split_x + 110.0, y: sy,
                    content: v.clone(),
                    color: TEXT_VAL,
                    font_size: FONT_SIZE, bold: false,
                    italic: false,
                    font_family: String::new(),
                    strikethrough: false, underline: false,
                });
                sy += ROW_H;
            }
        }
    }
}

/// Console tab: log z interpreter.console_log + input radek dole.
fn paint_console_tab(
    cmds: &mut Vec<DisplayCommand>,
    console_input: &str,
    interp: Option<&Interpreter>,
    win_w: f32, content_y: f32, content_h: f32,
) {
    // Input bar dole.
    let input_h = 28.0;
    let input_y = content_y + content_h - input_h;
    let log_h = content_h - input_h;

    // Log area pozadi.
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: content_y, w: win_w, h: log_h,
        color: PANEL_BG, radius: 0.0,
    });

    // Vykresli log entries (poslednich N at se vejde).
    if let Some(interp) = interp {
        let log = interp.console_log.borrow();
        let max_lines = (log_h / ROW_H) as usize;
        let start = log.len().saturating_sub(max_lines);
        for (i, entry) in log[start..].iter().enumerate() {
            let y = content_y + (i as f32) * ROW_H + 4.0;
            let (level, message) = (&entry.0, &entry.1);
            let (color, prefix): ([u8; 4], &str) = match level.as_str() {
                "error" => ([255, 100, 100, 255], "[error]"),
                "warn" => ([255, 200, 100, 255], "[warn]"),
                "info" => ([100, 200, 255, 255], "[info]"),
                _ => (TEXT_COLOR, ""),
            };
            cmds.push(DisplayCommand::Text {
                x: 8.0, y,
                content: format!("{} {}", prefix, message),
                color, font_size: FONT_SIZE, bold: false,
                italic: false,
                font_family: String::new(),
                strikethrough: false, underline: false,
            });
        }
    }

    // Input bar.
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: input_y, w: win_w, h: input_h,
        color: PANEL_BG_LIGHT, radius: 0.0,
    });
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: input_y, w: win_w, h: 1.0,
        color: BORDER_COLOR, radius: 0.0,
    });
    cmds.push(DisplayCommand::Text {
        x: 8.0, y: input_y + 8.0,
        content: ">".to_string(),
        color: TEXT_DIM, font_size: FONT_SIZE, bold: true,
        italic: false,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });
    cmds.push(DisplayCommand::Text {
        x: 22.0, y: input_y + 8.0,
        content: format!("{}_", console_input),
        color: TEXT_COLOR, font_size: FONT_SIZE, bold: false,
        italic: false,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });
}

/// Network tab: log z interpreter.network_log.
fn paint_network_tab(
    cmds: &mut Vec<DisplayCommand>,
    interp: Option<&Interpreter>,
    win_w: f32, content_y: f32, content_h: f32,
) {
    cmds.push(DisplayCommand::Rect {
        x: 0.0, y: content_y, w: win_w, h: content_h,
        color: PANEL_BG, radius: 0.0,
    });
    // Header.
    cmds.push(DisplayCommand::Text {
        x: 8.0, y: content_y + 4.0,
        content: "URL".to_string(),
        color: TEXT_DIM, font_size: FONT_SIZE, bold: true,
        italic: false,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });
    cmds.push(DisplayCommand::Text {
        x: win_w * 0.7, y: content_y + 4.0,
        content: "Status".to_string(),
        color: TEXT_DIM, font_size: FONT_SIZE, bold: true,
        italic: false,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });

    if let Some(interp) = interp {
        let log = interp.network_log.borrow();
        for (i, entry) in log.iter().enumerate() {
            let y = content_y + (i as f32 + 1.0) * ROW_H + 4.0;
            if y + ROW_H > content_y + content_h { break; }
            let (url, status) = (&entry.0, entry.1);
            cmds.push(DisplayCommand::Text {
                x: 8.0, y,
                content: url.clone(),
                color: TEXT_COLOR, font_size: FONT_SIZE, bold: false,
                italic: false,
                font_family: String::new(),
                strikethrough: false, underline: false,
            });
            cmds.push(DisplayCommand::Text {
                x: win_w * 0.7, y,
                content: format!("{}", status),
                color: if status >= 400 { [255, 100, 100, 255] }
                       else if status >= 300 { [255, 200, 100, 255] }
                       else { [100, 220, 100, 255] },
                font_size: FONT_SIZE, bold: false,
                italic: false,
                font_family: String::new(),
                strikethrough: false, underline: false,
            });
        }
    }
}

/// Tree row pomocna struktura pro flat-list zobrazeni stromu.
struct TreeRow {
    depth: usize,
    tag: String,
    attrs: Vec<(String, String)>,
    node_id: usize,
}

fn walk_dom(node: &Rc<NodeData>, depth: usize, rows: &mut Vec<TreeRow>) {
    let tag = node.tag_name().unwrap_or_default();
    if tag.is_empty() {
        // Text/comment node - skip v stromu pro citelnost.
    } else {
        let attrs: Vec<(String, String)> = node.attributes.borrow().iter()
            .filter(|(k, _)| !k.is_empty())
            .map(|(k, v)| (k.clone(), if v.len() > 30 { format!("{}...", &v[..27]) } else { v.clone() }))
            .take(3)
            .collect();
        rows.push(TreeRow {
            depth,
            tag: tag.clone(),
            attrs,
            node_id: Rc::as_ptr(node) as usize,
        });
    }
    for child in node.children.borrow().iter() {
        walk_dom(child, depth + 1, rows);
    }
}

/// Najdi LayoutBox podle node_id (Rc::as_ptr).
fn find_layout_box(bx: &LayoutBox, node_id: usize) -> Option<&LayoutBox> {
    if let Some(n) = &bx.node {
        if Rc::as_ptr(n) as usize == node_id {
            return Some(bx);
        }
    }
    for ch in &bx.children {
        if let Some(found) = find_layout_box(ch, node_id) {
            return Some(found);
        }
    }
    None
}

/// Najdi obrazovkove rect (x, y, w, h) pro element po scrollu.
pub fn find_box_rect_by_id(bx: &LayoutBox, node_id: usize, scroll_y: f32) -> Option<(f32, f32, f32, f32)> {
    if let Some(found) = find_layout_box(bx, node_id) {
        return Some((found.rect.x, found.rect.y - scroll_y, found.rect.width, found.rect.height));
    }
    None
}

/// Hit-test na DevTools panel: vrati (handler_kind, value).
/// HandlerKind = 0=tab_click(idx=value), 1=tree_row(node_id), 2=inspect_toggle, 3=resize_drag.
pub enum DevtoolsHit {
    TabClick(u8),
    TreeRow(usize),
    InspectToggle,
    /// Mouse je na resize grip - klient zacne resize drag.
    ResizeGrip,
    /// Clear console button (pri Console tab).
    ClearConsole,
    None,
}

pub fn devtools_hit_test(
    layout_root: &LayoutBox,
    tab: u8,
    tree_scroll: f32,
    win_w: f32,
    win_h: f32,
    panel_h: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    if panel_h <= 0.0 { return DevtoolsHit::None; }
    let panel_y = win_h - panel_h;
    if mouse_y < panel_y { return DevtoolsHit::None; }
    // Resize grip detection - prvni 4px panelu.
    if mouse_y < panel_y + RESIZE_GRIP_H {
        return DevtoolsHit::ResizeGrip;
    }
    let toolbar_y = panel_y + RESIZE_GRIP_H;
    if mouse_y < toolbar_y + TAB_H {
        // Toolbar - tabs nebo inspect.
        let tabs = ["Elements", "Console", "Network"];
        let mut tab_x = 8.0;
        for (i, name) in tabs.iter().enumerate() {
            let tab_w = (name.len() as f32) * 7.5 + 16.0;
            if mouse_x >= tab_x && mouse_x < tab_x + tab_w {
                return DevtoolsHit::TabClick(i as u8);
            }
            tab_x += tab_w + 4.0;
        }
        // Clear console button (pri Console tab).
        if tab == 1 {
            let clr_x = win_w - 200.0;
            if mouse_x >= clr_x && mouse_x < clr_x + 92.0 {
                return DevtoolsHit::ClearConsole;
            }
        }
        // Inspect button (vpravo).
        let insp_x = win_w - 100.0;
        if mouse_x >= insp_x && mouse_x < insp_x + 92.0 {
            return DevtoolsHit::InspectToggle;
        }
        return DevtoolsHit::None;
    }
    // Content area.
    let content_y = toolbar_y + TAB_H;
    if tab == 0 {
        // Elements tree click.
        let split_x = win_w * 0.6;
        if mouse_x < split_x {
            let mut rows: Vec<TreeRow> = Vec::new();
            if let Some(node) = layout_root.node.as_ref() {
                walk_dom(node, 0, &mut rows);
            }
            let row_idx = ((mouse_y - content_y + tree_scroll) / ROW_H) as usize;
            if row_idx < rows.len() {
                return DevtoolsHit::TreeRow(rows[row_idx].node_id);
            }
        }
    }
    DevtoolsHit::None
}

/// Najdi DOM node podle ekrnoveho rect (inverze find_box_rect_by_id) - pro inspect mode.
pub fn pick_node_at_screen_pos(
    bx: &LayoutBox,
    mouse_x: f32, mouse_y: f32, scroll_y: f32,
) -> Option<usize> {
    let py = mouse_y + scroll_y;
    pick_recursive(bx, mouse_x, py)
}

fn pick_recursive(bx: &LayoutBox, mx: f32, my: f32) -> Option<usize> {
    // Picknout deepest match - DFS posledni odpovidajici child.
    let mut best: Option<usize> = None;
    for ch in &bx.children {
        if let Some(found) = pick_recursive(ch, mx, my) {
            best = Some(found);
        }
    }
    if best.is_some() { return best; }
    if mx >= bx.rect.x && mx < bx.rect.x + bx.rect.width
       && my >= bx.rect.y && my < bx.rect.y + bx.rect.height {
        if let Some(n) = &bx.node {
            return Some(Rc::as_ptr(n) as usize);
        }
    }
    None
}
