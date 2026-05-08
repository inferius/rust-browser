//! In-window DevTools panel - frontend nad `crate::devtools::DevToolsState`.
//!
//! Funkce:
//! - `paint_devtools_panel(cmds, state, ...)` - vykresli panel (toolbar + tab + content)
//! - `paint_element_highlight(cmds, state, ...)` - overlay pres vybrany/hover element
//!   (Chrome-like content/padding/border/margin viz). Volana vzdy bez ohledu na
//!   panel_open - selection persistuje napric F12 toggle.
//! - `devtools_hit_test(state, ...)` - urci co bylo kliknuto (resize grip / tab /
//!   tree row / inspect button / search bar / scroll thumb / context menu item / ...)
//! - `find_box_rect_by_id` - obrazovkova pozice elementu po scroll
//! - `pick_node_at_screen_pos` - inspect mode: najdi nejhlubsi element pod kurzorem

use std::rc::Rc;
use crate::browser::paint::DisplayCommand;
use crate::browser::layout::LayoutBox;
use crate::browser::dom::{NodeData, NodeKind};
use crate::devtools::{DevToolsState, Tab};
use crate::devtools::theme::Palette;
use crate::devtools::model::elements::{ElementRow, RowKind};
use crate::devtools::model::console::LogLevel;
use crate::interpreter::Interpreter;

pub const ROW_H: f32 = 18.0;
pub const TAB_H: f32 = 30.0;
const TOOLBAR_BTN_H: f32 = 22.0;
const FONT_SIZE: f32 = 12.0;
/// CamingoMono glyph advance pri 12px. Empirically ~7.2px (mono).
const FONT_W: f32 = 7.2;
const INDENT_PX: f32 = 16.0;
pub const RESIZE_GRIP_H: f32 = 4.0;
const SCROLLBAR_W: f32 = 10.0;
pub const SEARCH_H: f32 = 28.0;
/// Custom font family pro vsechen DevTools text.
const DT_FONT: &str = "CamingoMono";
const DT_FONT_BOLD: &str = "CamingoMono-Bold";
const DT_FONT_ITALIC: &str = "CamingoMono-Italic";

// ─── Top-level paint ────────────────────────────────────────────────────

pub fn paint_devtools_panel(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    state: &DevToolsState,
    interp: Option<&Interpreter>,
    win_w: f32,
    win_h: f32,
    mouse_x: f32,
    mouse_y: f32,
) {
    if !state.panel_open || state.panel_h <= 0.0 { return; }
    let pal = state.palette();
    let panel_h = state.panel_h.min(win_h * 0.7);
    let panel_y = win_h - panel_h;

    // Pozadi panelu.
    push_rect(cmds, 0.0, panel_y, win_w, panel_h, pal.bg_panel);
    // Top horizontal border (oddeleni od page).
    push_rect(cmds, 0.0, panel_y, win_w, 1.0, pal.border_strong);

    // Resize grip - 4px draggable area pri vrchu (pred toolbarem).
    let grip_hover = mouse_x >= 0.0 && mouse_x <= win_w
        && mouse_y >= panel_y && mouse_y < panel_y + RESIZE_GRIP_H;
    let grip_color = if grip_hover { pal.accent } else { pal.border };
    push_rect(cmds, 0.0, panel_y + 1.0, win_w, RESIZE_GRIP_H - 1.0, grip_color);

    // Toolbar (taby + akce).
    let toolbar_y = panel_y + RESIZE_GRIP_H;
    push_rect(cmds, 0.0, toolbar_y, win_w, TAB_H, pal.bg_toolbar);
    push_rect(cmds, 0.0, toolbar_y + TAB_H - 1.0, win_w, 1.0, pal.border);

    paint_tabs(cmds, state, &pal, toolbar_y, win_w, mouse_x, mouse_y);
    paint_toolbar_actions(cmds, state, &pal, toolbar_y, win_w, mouse_x, mouse_y);

    // Content area.
    let content_y = toolbar_y + TAB_H;
    let content_h = panel_h - RESIZE_GRIP_H - TAB_H;
    if content_h <= 0.0 { return; }

    match state.tab {
        Tab::Elements => paint_elements_tab(cmds, layout_root, state, &pal, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Console => paint_console_tab(cmds, state, &pal, interp, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Network => paint_network_tab(cmds, state, &pal, interp, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Sources => paint_sources_tab(cmds, state, &pal, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Performance => paint_performance_tab(cmds, state, &pal, win_w, content_y, content_h),
        Tab::Application => paint_application_tab(cmds, state, &pal, interp, win_w, content_y, content_h),
        Tab::Settings => paint_settings_tab(cmds, state, &pal, win_w, content_y, content_h, mouse_x, mouse_y),
    }

    // Context menu vykresli pres vsechno (z-order top).
    if let Some(menu) = &state.context_menu {
        paint_context_menu(cmds, &pal, menu, mouse_x, mouse_y);
    }
}

// ─── Tabs ────────────────────────────────────────────────────────────────

fn tab_rect(idx: usize, toolbar_y: f32) -> (f32, f32, f32, f32) {
    let labels = Tab::all();
    let mut x = 8.0;
    for (i, t) in labels.iter().enumerate() {
        let w = (t.label().len() as f32) * FONT_W + 18.0;
        if i == idx {
            return (x, toolbar_y + 4.0, w, TAB_H - 4.0);
        }
        x += w + 2.0;
    }
    (x, toolbar_y + 4.0, 60.0, TAB_H - 4.0)
}

fn paint_tabs(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    toolbar_y: f32,
    _win_w: f32,
    mouse_x: f32, mouse_y: f32,
) {
    for (i, t) in Tab::all().iter().enumerate() {
        let (x, y, w, h) = tab_rect(i, toolbar_y);
        let active = state.tab == *t;
        let hovered = mouse_x >= x && mouse_x < x + w && mouse_y >= y && mouse_y < y + h;
        let bg = if active { pal.bg_tab_active }
                 else if hovered { pal.bg_row_hover }
                 else { pal.bg_toolbar };
        push_rect(cmds, x, y, w, h, bg);
        if active {
            // Akcent bottom underline.
            push_rect(cmds, x, y + h - 2.0, w, 2.0, pal.accent);
        }
        let txt_color = if active { pal.text } else { pal.text_dim };
        push_text(cmds, x + 9.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
                  t.label().to_string(), txt_color, false);
    }
}

fn toolbar_actions_x(win_w: f32) -> f32 {
    win_w - 12.0
}

fn paint_toolbar_actions(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    toolbar_y: f32,
    win_w: f32,
    mouse_x: f32, mouse_y: f32,
) {
    // Vpravo: Inspect | Theme switch | Close.
    let mut x_right = toolbar_actions_x(win_w);
    let y = toolbar_y + (TAB_H - TOOLBAR_BTN_H) * 0.5;
    let h = TOOLBAR_BTN_H;

    // Close (X).
    let close_w = 24.0;
    x_right -= close_w;
    let close_hover = mouse_x >= x_right && mouse_x < x_right + close_w
                      && mouse_y >= y && mouse_y < y + h;
    push_rect(cmds, x_right, y, close_w, h,
              if close_hover { pal.bg_row_hover } else { pal.bg_toolbar });
    push_text(cmds, x_right + 8.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
              "\u{2715}".to_string(), pal.text, false);

    // Theme dot (Ctrl+Shift+T toggle): sun/moon icon.
    let theme_w = 24.0;
    x_right -= theme_w + 4.0;
    let theme_hover = mouse_x >= x_right && mouse_x < x_right + theme_w
                      && mouse_y >= y && mouse_y < y + h;
    push_rect(cmds, x_right, y, theme_w, h,
              if theme_hover { pal.bg_row_hover } else { pal.bg_toolbar });
    let icon = if pal.is_dark { "\u{263E}" } else { "\u{263C}" };
    push_text(cmds, x_right + 7.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
              icon.to_string(), pal.text, false);

    // Inspect toggle: ramecek symbol.
    let insp_w = 90.0;
    x_right -= insp_w + 4.0;
    let insp_hover = mouse_x >= x_right && mouse_x < x_right + insp_w
                     && mouse_y >= y && mouse_y < y + h;
    let bg = if state.inspect_mode { pal.accent }
             else if insp_hover { pal.bg_row_hover }
             else { pal.bg_button };
    push_rect(cmds, x_right, y, insp_w, h, bg);
    let txt = if state.inspect_mode { pal.text_on_accent } else { pal.text };
    push_text(cmds, x_right + 8.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
              "\u{2316} Inspect".to_string(), txt, false);
}

// ─── Elements tab ───────────────────────────────────────────────────────

fn paint_elements_tab(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    win_w: f32,
    content_y: f32,
    content_h: f32,
    mouse_x: f32, mouse_y: f32,
) {
    // Search bar nahore, optional.
    let search_h = if state.elements.search.open { SEARCH_H } else { 0.0 };
    if state.elements.search.open {
        paint_elements_search_bar(cmds, state, pal, win_w, content_y);
    }
    let body_y = content_y + search_h;
    let body_h = content_h - search_h;

    // Default split: 70% pro elements pri prvnim render (split_x = 0).
    let default_split = win_w * 0.7;
    let split_x = if state.elements.split_x < 1.0 { default_split }
                  else { state.elements.split_x.max(200.0).min(win_w - 220.0) };

    // Z-order:
    // 1. Tree bg (left)
    // 2. Tree rows (clipped na split_x)
    // 3. Styles bg + splitter (PRES rows pri prelivu)
    // 4. Styles content

    push_rect(cmds, 0.0, body_y, split_x, body_h, pal.bg_panel);

    // Render rows visible v scroll window.
    let rows = &state.elements.rows;
    let total_h = rows.len() as f32 * ROW_H;
    let scroll_y = state.elements.scroll_y.min((total_h - body_h).max(0.0)).max(0.0);
    let visible_rows = ((body_h / ROW_H).ceil() as usize) + 1;
    let start_row = (scroll_y / ROW_H) as usize;

    for (visual_idx, row_idx) in (start_row..rows.len().min(start_row + visible_rows)).enumerate() {
        let row = &rows[row_idx];
        let y = body_y + (visual_idx as f32) * ROW_H - (scroll_y % ROW_H);
        if y + ROW_H < body_y || y > body_y + body_h { continue; }
        paint_element_row(cmds, row, state, pal, split_x - SCROLLBAR_W - 4.0, y, mouse_x, mouse_y);
    }

    // Scrollbar pro tree (vertikalni).
    if total_h > body_h {
        paint_vertical_scrollbar(cmds, pal, split_x - SCROLLBAR_W, body_y,
                                  body_h, total_h, scroll_y);
    }

    // Styles bg + splitter PRES rows (clip via overdraw).
    push_rect(cmds, split_x, body_y, win_w - split_x, body_h, pal.bg_panel_alt);
    push_rect(cmds, split_x, body_y, 2.0, body_h, pal.border_strong);

    // Right pane: matched styles + computed styles.
    if let Some(sel_id) = state.elements.selected {
        if let Some(bx) = find_layout_box(layout_root, sel_id) {
            paint_styles_pane(cmds, bx, state, pal, split_x + 2.0, body_y, win_w - split_x - 2.0, body_h);
        }
    } else {
        push_text(cmds, split_x + 12.0, body_y + 12.0,
                  "Select an element to see styles".to_string(),
                  pal.text_dim, false);
    }
}

fn paint_elements_search_bar(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    win_w: f32,
    y: f32,
) {
    push_rect(cmds, 0.0, y, win_w, SEARCH_H, pal.bg_panel_alt);
    push_rect(cmds, 0.0, y + SEARCH_H - 1.0, win_w, 1.0, pal.border);

    // Input.
    let input_x = 8.0;
    let input_y = y + 4.0;
    let input_w = win_w - 200.0;
    let input_h = SEARCH_H - 8.0;
    push_rect(cmds, input_x, input_y, input_w, input_h, pal.bg_input);
    push_rect_border(cmds, input_x, input_y, input_w, input_h, pal.border);
    let placeholder_text = "Find by tag / .class / #id / [attr] / //xpath";
    let display_text = if state.elements.search.query.is_empty() {
        placeholder_text.to_string()
    } else {
        state.elements.search.query.clone()
    };
    let txt_color = if state.elements.search.query.is_empty() {
        pal.text_disabled
    } else {
        pal.text
    };
    push_text(cmds, input_x + 6.0, input_y + 4.0, display_text, txt_color, false);

    // Match counter.
    let s = &state.elements.search;
    let counter = if s.query.is_empty() {
        String::new()
    } else if s.matches.is_empty() {
        "0 / 0".to_string()
    } else {
        format!("{} / {}", s.current + 1, s.matches.len())
    };
    push_text(cmds, input_x + input_w + 8.0, input_y + 4.0, counter, pal.text_dim, false);
}

fn paint_element_row(
    cmds: &mut Vec<DisplayCommand>,
    row: &ElementRow,
    state: &DevToolsState,
    pal: &Palette,
    width: f32,
    y: f32,
    mouse_x: f32, mouse_y: f32,
) {
    use crate::devtools::EditTarget;
    // Pokud je tato row prave editovana, render editor namisto normalniho radku.
    if let Some(edit) = &state.elements.edit {
        let edit_node = match &edit.target {
            EditTarget::AttributeValue { node_id, .. } => *node_id,
            EditTarget::AttributeName { node_id, .. } => *node_id,
            EditTarget::TextNode { node_id } => *node_id,
            EditTarget::InlineStyleProperty { node_id, .. } => *node_id,
        };
        if edit_node == row.node_id {
            paint_edit_row(cmds, row, edit, state, pal, width, y);
            return;
        }
    }

    let is_sel = state.elements.selected == Some(row.node_id);
    let is_hov = mouse_x < width && mouse_y >= y && mouse_y < y + ROW_H;

    if is_sel {
        push_rect(cmds, 0.0, y, width, ROW_H, pal.bg_row_selected);
        push_rect(cmds, 0.0, y, 3.0, ROW_H, pal.accent);
    } else if is_hov {
        push_rect(cmds, 0.0, y, width, ROW_H, pal.bg_row_hover);
    }

    // Pri selected: vsechen text high-contrast (text_on_accent), bez syntaxove barvy.
    let text_color_default = if is_sel { pal.text_on_accent } else { pal.text };
    let x_indent = 8.0 + row.depth as f32 * INDENT_PX;
    let text_y = y + 3.0;

    match &row.kind {
        RowKind::Document => {
            push_text(cmds, x_indent, text_y, "#document".to_string(), pal.text_dim, false);
        }
        RowKind::DocType(name) => {
            push_text(cmds, x_indent, text_y, format!("<!DOCTYPE {}>", name),
                      pal.syn_doctype, false);
        }
        RowKind::Comment(c) => {
            let trimmed: String = c.chars().take(120).collect();
            push_text(cmds, x_indent, text_y, format!("<!-- {} -->", trimmed),
                      pal.syn_comment, true);
        }
        RowKind::Cdata(c) => {
            let trimmed: String = c.chars().take(80).collect();
            push_text(cmds, x_indent, text_y, format!("<![CDATA[{}]]>", trimmed),
                      pal.syn_doctype, false);
        }
        RowKind::Text(t) => {
            // Italic + odlisna barva pro text nodes.
            push_text_italic(cmds, x_indent, text_y, format!("\"{}\"", t),
                             if is_sel { pal.text_inverted } else { pal.syn_text_node }, false);
        }
        RowKind::Element { tag, attrs, self_closing, has_children } => {
            // Caret expand/collapse pro elementy s detmi.
            let collapsed = state.elements.collapsed.contains(&row.node_id);
            let mut x = x_indent;
            if *has_children {
                let caret = if collapsed { "\u{25B6}" } else { "\u{25BC}" };
                push_text(cmds, x - INDENT_PX + 4.0, text_y,
                          caret.to_string(), pal.text_dim, false);
            }
            // <tag
            let tag_color = if is_sel { text_color_default } else { pal.syn_tag };
            push_text(cmds, x, text_y, format!("<{}", tag), tag_color, false);
            x += (tag.chars().count() + 1) as f32 * FONT_W;
            // Attrs.
            for (k, v) in attrs {
                let attr_str = format!(" {}=", k);
                push_text(cmds, x, text_y, attr_str.clone(),
                          if is_sel { text_color_default } else { pal.syn_attr }, false);
                x += attr_str.chars().count() as f32 * FONT_W;
                let val_truncated: String = v.chars().take(40).collect();
                let val_str = if val_truncated.chars().count() < v.chars().count() {
                    format!("\"{}...\"", val_truncated)
                } else {
                    format!("\"{}\"", val_truncated)
                };
                push_text(cmds, x, text_y, val_str.clone(),
                          if is_sel { text_color_default } else { pal.syn_value }, false);
                x += val_str.chars().count() as f32 * FONT_W;
                if x > width - 30.0 { break; }
            }
            // Closing > nebo />.
            let close = if *self_closing { " />" } else { ">" };
            push_text(cmds, x, text_y, close.to_string(), tag_color, false);
            // Pri collapsed s detmi: ukaz "..." pred close.
            if *has_children && collapsed {
                push_text(cmds, x + close.len() as f32 * FONT_W + 2.0, text_y,
                          "...".to_string(), pal.text_dim, false);
            }
        }
        RowKind::CloseTag(tag) => {
            push_text(cmds, x_indent, text_y, format!("</{}>", tag),
                      if is_sel { text_color_default } else { pal.syn_tag }, false);
        }
    }
}

fn paint_edit_row(
    cmds: &mut Vec<DisplayCommand>,
    row: &ElementRow,
    edit: &crate::devtools::EditState,
    state: &DevToolsState,
    pal: &Palette,
    width: f32,
    y: f32,
) {
    use crate::devtools::EditTarget;
    push_rect(cmds, 0.0, y, width, ROW_H, pal.bg_input_focus);
    push_rect(cmds, 0.0, y, 3.0, ROW_H, pal.accent);
    let x_indent = 8.0 + row.depth as f32 * INDENT_PX;
    let text_y = y + 3.0;
    // Prefix label dle target type.
    let prefix = match &edit.target {
        EditTarget::AttributeValue { attr, .. } => format!("{}=", attr),
        EditTarget::AttributeName { value, .. } => format!("[new]={}=", value),
        EditTarget::TextNode { .. } => "text:".to_string(),
        EditTarget::InlineStyleProperty { property, .. } => format!("{}: ", property),
    };
    push_text(cmds, x_indent, text_y, prefix.clone(), pal.text_dim, false);
    let text_x = x_indent + prefix.len() as f32 * FONT_W;

    // Selection highlight - chars-based offset.
    if let Some((s, e)) = edit.buffer.selection_range() {
        let s_chars = edit.buffer.text[..s].chars().count();
        let e_chars = edit.buffer.text[..e].chars().count();
        let sx = text_x + (s_chars as f32) * FONT_W;
        let ex = text_x + (e_chars as f32) * FONT_W;
        push_rect(cmds, sx, text_y - 2.0, ex - sx, FONT_SIZE + 4.0, pal.bg_row_selected);
    }
    push_text(cmds, text_x, text_y, edit.buffer.text.clone(), pal.text, false);
    // Cursor blink - chars-based.
    if state.cursor_visible() {
        let cur_chars = edit.buffer.text[..edit.buffer.cursor].chars().count();
        let cx = text_x + (cur_chars as f32) * FONT_W;
        push_rect(cmds, cx, text_y - 2.0, 1.0, FONT_SIZE + 4.0, pal.text);
    }
}

fn paint_styles_pane(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, _w: f32, h: f32,
) {
    let mut sy = y + 8.0;
    let max_y = y + h;
    let pad_x = x + 12.0;

    // Section: matched rules.
    push_text_bold(cmds, pad_x, sy, "Matched CSS rules".to_string(), pal.text, false);
    sy += ROW_H + 2.0;

    if state.styles.matched_rules.is_empty() {
        push_text(cmds, pad_x, sy, "(no matched rules)".to_string(), pal.text_dim, true);
        sy += ROW_H + 8.0;
    } else {
        for rule in &state.styles.matched_rules {
            if sy + ROW_H > max_y { break; }
            let src_label = match &rule.source {
                crate::devtools::model::styles::RuleSource::UserAgent => "user agent".to_string(),
                crate::devtools::model::styles::RuleSource::Inline => "inline".to_string(),
                crate::devtools::model::styles::RuleSource::StyleBlock { index } => format!("<style #{}>", index),
                crate::devtools::model::styles::RuleSource::External { url } => url.clone(),
            };
            push_text(cmds, pad_x, sy, format!("{} {{ /* {} */", rule.selector, src_label),
                      pal.syn_property, false);
            sy += ROW_H;
            for d in &rule.declarations {
                if sy + ROW_H > max_y { return; }
                let line = format!("  {}: {}{};", d.property, d.value,
                                   if d.important { " !important" } else { "" });
                push_text_underline(cmds, pad_x, sy, line, pal.text, d.overridden);
                sy += ROW_H;
            }
            push_text(cmds, pad_x, sy, "}".to_string(), pal.syn_property, false);
            sy += ROW_H + 4.0;
        }
    }

    // Computed values - z cascade vystupu (state.styles.computed) + box rect.
    if sy + ROW_H > max_y { return; }
    push_text_bold(cmds, pad_x, sy, "Computed".to_string(), pal.text, false);
    sy += ROW_H + 2.0;

    let filter = state.styles.filter.to_lowercase();
    for (k, v) in &state.styles.computed {
        if sy + ROW_H > max_y { return; }
        if !filter.is_empty() && !k.contains(&filter) { continue; }
        push_text(cmds, pad_x, sy, format!("{}:", k), pal.syn_property, false);
        push_text(cmds, pad_x + 160.0, sy, v.clone(), pal.text, false);
        sy += ROW_H;
    }

    // Box info (rect / margin / padding z LayoutBox).
    if sy + ROW_H > max_y { return; }
    sy += 8.0;
    push_text_bold(cmds, pad_x, sy, "Box".to_string(), pal.text, false);
    sy += ROW_H + 2.0;
    let box_info = vec![
        ("rect", format!("x={:.0} y={:.0} w={:.0} h={:.0}",
            bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height)),
        ("padding", format!("{:.0}", bx.padding)),
        ("margin", format!("{:.0}", bx.margin)),
        ("border-width", format!("{:.0}", bx.border_width)),
    ];
    for (k, v) in &box_info {
        if sy + ROW_H > max_y { break; }
        push_text(cmds, pad_x, sy, format!("{}:", k), pal.syn_property, false);
        push_text(cmds, pad_x + 160.0, sy, v.clone(), pal.text, false);
        sy += ROW_H;
    }
}

// ─── Console tab ────────────────────────────────────────────────────────

fn paint_console_tab(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    interp: Option<&Interpreter>,
    win_w: f32,
    content_y: f32,
    content_h: f32,
    _mouse_x: f32, _mouse_y: f32,
) {
    let input_h = 32.0;
    let input_y = content_y + content_h - input_h;
    let log_y = content_y;
    let log_h = content_h - input_h;

    push_rect(cmds, 0.0, log_y, win_w, log_h, pal.bg_panel);
    push_rect(cmds, 0.0, input_y, win_w, input_h, pal.bg_panel_alt);
    push_rect(cmds, 0.0, input_y, win_w, 1.0, pal.border);

    // Logs: state.console.log je mirror z interpreter.console_log (ten radi v
    // each render frame).
    let _ = interp;
    let entries: Vec<(LogLevel, String)> = state.console.log.iter()
        .map(|e| (e.level, e.text.clone())).collect();

    // Expand log entries do per-line listu (multi-line msg = multiple lines).
    let mut display_lines: Vec<(LogLevel, String, bool)> = Vec::new();
    for (lvl, msg) in &entries {
        let mut first = true;
        for line in msg.lines() {
            display_lines.push((*lvl, line.to_string(), first));
            first = false;
        }
        if msg.is_empty() {
            display_lines.push((*lvl, String::new(), true));
        }
    }
    // Stick to bottom: render od konce.
    let max_visible = (log_h / ROW_H) as usize;
    let start = display_lines.len().saturating_sub(max_visible);
    for (i, (lvl, line, first)) in display_lines.iter().skip(start).enumerate() {
        let ey = log_y + 4.0 + (i as f32) * ROW_H;
        let color = match lvl {
            LogLevel::Info | LogLevel::Result => pal.log_info,
            LogLevel::Warn => pal.log_warn,
            LogLevel::Error => pal.log_error,
            LogLevel::InputEcho => pal.log_input_marker,
        };
        let prefix = if *first {
            match lvl {
                LogLevel::InputEcho => "> ",
                LogLevel::Result => "< ",
                _ => "",
            }
        } else { "  " };
        push_text(cmds, 8.0, ey, format!("{}{}", prefix, line), color, false);
    }

    // Input field.
    paint_console_input(cmds, state, pal, 0.0, input_y, win_w, input_h);

    // Autocomplete popup (kdyz aktivni).
    if let Some(ac) = &state.console.autocomplete {
        let popup_h = (ac.hits.len().min(8) as f32) * ROW_H + 4.0;
        let popup_y = input_y - popup_h - 2.0;
        push_rect(cmds, 8.0, popup_y, 240.0, popup_h, pal.bg_context_menu);
        push_rect_border(cmds, 8.0, popup_y, 240.0, popup_h, pal.border);
        for (i, hit) in ac.hits.iter().take(8).enumerate() {
            let hy = popup_y + 2.0 + (i as f32) * ROW_H;
            if i == ac.selected {
                push_rect(cmds, 9.0, hy, 238.0, ROW_H, pal.bg_context_menu_hover);
            }
            push_text(cmds, 12.0, hy + 3.0, hit.text.clone(), pal.text, false);
        }
    }
}

fn paint_console_input(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, h: f32,
) {
    use crate::devtools::focus::FocusTarget;
    let focused = state.focus == FocusTarget::DevToolsConsole;
    let bg = if focused { pal.bg_input_focus } else { pal.bg_input };
    push_rect(cmds, x + 4.0, y + 4.0, w - 8.0, h - 8.0, bg);
    if focused {
        push_rect_border(cmds, x + 4.0, y + 4.0, w - 8.0, h - 8.0, pal.border_focus);
    } else {
        push_rect_border(cmds, x + 4.0, y + 4.0, w - 8.0, h - 8.0, pal.border);
    }

    // Prompt marker.
    let prompt_x = x + 10.0;
    let text_y = y + (h - FONT_SIZE) * 0.5 + 1.0;
    push_text(cmds, prompt_x, text_y, ">".to_string(), pal.log_input_marker, true);

    let input = &state.console.input;
    let text_x = prompt_x + 12.0;

    // Selection highlight - chars-based (ne byte offset).
    if let Some((s, e)) = input.selection_range() {
        let s_chars = input.text[..s].chars().count();
        let e_chars = input.text[..e].chars().count();
        let sel_x0 = text_x + (s_chars as f32) * FONT_W;
        let sel_x1 = text_x + (e_chars as f32) * FONT_W;
        push_rect(cmds, sel_x0, text_y - 2.0, sel_x1 - sel_x0, FONT_SIZE + 4.0,
                  pal.bg_row_selected);
    }

    push_text(cmds, text_x, text_y, input.text.clone(), pal.text, false);

    // Cursor (blink pri focusu) - chars-based offset.
    if focused && state.cursor_visible() {
        let cur_chars = input.text[..input.cursor].chars().count();
        let cur_x = text_x + (cur_chars as f32) * FONT_W;
        push_rect(cmds, cur_x, text_y - 2.0, 1.0, FONT_SIZE + 4.0, pal.text);
    }
}

// ─── Network tab ────────────────────────────────────────────────────────

fn paint_network_tab(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    interp: Option<&Interpreter>,
    win_w: f32,
    content_y: f32,
    content_h: f32,
    _mouse_x: f32, _mouse_y: f32,
) {
    push_rect(cmds, 0.0, content_y, win_w, content_h, pal.bg_panel);

    // Filter toolbar nahore.
    let filter_h = 26.0;
    push_rect(cmds, 0.0, content_y, win_w, filter_h, pal.bg_toolbar);
    push_rect(cmds, 0.0, content_y + filter_h - 1.0, win_w, 1.0, pal.border);
    use crate::devtools::model::network::NetworkFilter;
    let filters = [
        ("All", NetworkFilter::All),
        ("Doc", NetworkFilter::Document),
        ("CSS", NetworkFilter::Stylesheet),
        ("JS", NetworkFilter::Script),
        ("Img", NetworkFilter::Image),
        ("Font", NetworkFilter::Font),
        ("XHR", NetworkFilter::Xhr),
    ];
    let mut fx = 8.0;
    for (label, f) in filters.iter() {
        let w = label.len() as f32 * FONT_W + 14.0;
        let active = state.network.filter == *f;
        let bg = if active { pal.bg_tab_active } else { pal.bg_toolbar };
        push_rect(cmds, fx, content_y + 3.0, w, filter_h - 6.0, bg);
        if active {
            push_rect(cmds, fx, content_y + filter_h - 5.0, w, 2.0, pal.accent);
        }
        push_text(cmds, fx + 7.0, content_y + 7.0, label.to_string(),
                  if active { pal.text } else { pal.text_dim }, false);
        fx += w + 2.0;
    }
    let content_y = content_y + filter_h;
    let content_h = content_h - filter_h;

    // Detail popup overlay - kdyz selected + detail_open.
    let show_detail = state.network.detail_open && state.network.selected.is_some();
    let main_w = if show_detail { win_w * 0.6 } else { win_w };
    if show_detail {
        let dx = main_w;
        push_rect(cmds, dx, content_y, win_w - dx, content_h, pal.bg_panel_alt);
        push_rect(cmds, dx, content_y, 1.0, content_h, pal.border);
        if let Some(idx) = state.network.selected {
            let entry: Option<(String, u16)> = state.network.entries.get(idx)
                .map(|e| (e.url.clone(), e.status))
                .or_else(|| interp.and_then(|i| i.network_log.borrow().get(idx).cloned()));
            if let Some((url, status)) = entry {
                let mut sy = content_y + 12.0;
                push_text_bold(cmds, dx + 12.0, sy, "Headers".to_string(), pal.text, false);
                sy += ROW_H + 4.0;
                push_text(cmds, dx + 12.0, sy, format!("Status: {}", status), pal.text, false);
                sy += ROW_H;
                push_text(cmds, dx + 12.0, sy, format!("URL: {}", url), pal.text_dim, false);
                sy += ROW_H + 8.0;
                push_text_bold(cmds, dx + 12.0, sy, "Method: GET".to_string(), pal.text, false);
                sy += ROW_H + 8.0;
                push_text_bold(cmds, dx + 12.0, sy, "Response (preview)".to_string(), pal.text, false);
                sy += ROW_H + 4.0;
                push_text(cmds, dx + 12.0, sy, "(Content body neni captured ve store)".to_string(), pal.text_dim, true);
            }
        }
    }

    // Sloupce header.
    let header_h = ROW_H + 4.0;
    push_rect(cmds, 0.0, content_y, win_w, header_h, pal.bg_panel_alt);
    push_rect(cmds, 0.0, content_y + header_h - 1.0, win_w, 1.0, pal.border);
    let cols = ["Method", "Status", "Type", "URL", "Size", "Time"];
    let col_x = [8.0, 80.0, 160.0, 240.0, win_w - 200.0, win_w - 100.0];
    for (i, c) in cols.iter().enumerate() {
        push_text_bold(cmds, col_x[i], content_y + 4.0,
                       c.to_string(), pal.text_dim, false);
    }

    // Live z interpretera kdyz state nezapsan.
    let raw_entries: Vec<(String, u16)> = if state.network.entries.is_empty() {
        if let Some(i) = interp {
            i.network_log.borrow().clone()
        } else {
            Vec::new()
        }
    } else {
        state.network.entries.iter().map(|e| (e.url.clone(), e.status)).collect()
    };

    // Filter aplikace.
    let entries: Vec<(String, u16)> = raw_entries.into_iter().filter(|(url, _)| {
        let ty = crate::devtools::model::network::NetworkResourceType::from_url(url);
        state.network.filter.matches(ty)
    }).collect();

    let row_y0 = content_y + header_h + 2.0;
    let max_rows = ((content_h - header_h) / ROW_H) as usize;
    for (i, (url, status)) in entries.iter().take(max_rows).enumerate() {
        let ry = row_y0 + (i as f32) * ROW_H;
        let status_color = match *status {
            200..=299 => pal.net_2xx,
            300..=399 => pal.net_3xx,
            400..=499 => pal.net_4xx,
            500..=599 => pal.net_5xx,
            _ => pal.text_dim,
        };
        let ty = ry + 3.0;
        push_text(cmds, col_x[0], ty, "GET".to_string(), pal.text, false);
        push_text(cmds, col_x[1], ty, status.to_string(), status_color, false);
        let rt = crate::devtools::model::network::NetworkResourceType::from_url(url);
        push_text(cmds, col_x[2], ty, format!("{:?}", rt), pal.text_dim, false);
        let url_truncated: String = url.chars().take(60).collect();
        push_text(cmds, col_x[3], ty, url_truncated, pal.text, false);
    }
}

// ─── Sources tab (placeholder) ──────────────────────────────────────────

fn paint_sources_tab(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    win_w: f32,
    content_y: f32,
    content_h: f32,
    _mouse_x: f32, _mouse_y: f32,
) {
    push_rect(cmds, 0.0, content_y, win_w, content_h, pal.bg_panel);

    // Debugger toolbar nahore: Continue / Step Over / Step Into / Step Out + status.
    let dbg_h = 28.0;
    push_rect(cmds, 0.0, content_y, win_w, dbg_h, pal.bg_toolbar);
    push_rect(cmds, 0.0, content_y + dbg_h - 1.0, win_w, 1.0, pal.border);
    let labels = ["Continue", "Step Over", "Step Into", "Step Out"];
    let mut x = 8.0;
    for label in labels.iter() {
        let w = label.len() as f32 * FONT_W + 16.0;
        push_rect(cmds, x, content_y + 4.0, w, dbg_h - 8.0, pal.bg_button);
        push_text(cmds, x + 8.0, content_y + 9.0, label.to_string(), pal.text, false);
        x += w + 4.0;
    }
    let status = if state.sources.debugger_paused {
        format!("Paused (line {:?})",
            state.sources.current_pause_location.map(|(_, l)| l).unwrap_or(0))
    } else {
        "Running (set breakpoints with click on line gutter)".to_string()
    };
    push_text(cmds, x + 16.0, content_y + 9.0, status,
              if state.sources.debugger_paused { pal.log_warn } else { pal.text_dim }, true);

    let body_y = content_y + dbg_h;
    let body_h = content_h - dbg_h;
    let split_x = 240.0;
    push_rect(cmds, split_x, body_y, 1.0, body_h, pal.border);
    push_rect(cmds, 0.0, body_y, split_x, body_h, pal.bg_panel_alt);

    let content_y = body_y;
    let content_h = body_h;

    // File list.
    push_text_bold(cmds, 8.0, content_y + 6.0, "Sources".to_string(), pal.text_dim, false);
    let mut sy = content_y + ROW_H + 8.0;
    for f in &state.sources.files {
        let sel = state.sources.selected_id == Some(f.id);
        if sel {
            push_rect(cmds, 0.0, sy, split_x, ROW_H, pal.bg_row_selected);
        }
        let fname: String = f.url.chars().rev().take(40).collect::<String>().chars().rev().collect();
        push_text(cmds, 12.0, sy + 3.0, fname,
                  if sel { pal.text_inverted } else { pal.text }, false);
        sy += ROW_H;
        if sy > content_y + content_h - ROW_H { break; }
    }

    // Right pane: source content + breakpoints + locals popup pri pause.
    if let Some(id) = state.sources.selected_id {
        if let Some(file) = state.sources.files.iter().find(|f| f.id == id) {
            // Locals panel zabira 250px na pravem okraji pri pause.
            let locals_w = if state.sources.debugger_paused { 250.0 } else { 0.0 };
            let src_w = win_w - split_x - 1.0 - locals_w;
            paint_source_content(cmds, file, state, pal, split_x + 1.0, content_y,
                                 src_w, content_h);
            if state.sources.debugger_paused {
                paint_locals_pane(cmds, pal, split_x + 1.0 + src_w, content_y, locals_w, content_h, state);
            }
        }
    } else {
        push_text(cmds, split_x + 12.0, content_y + 12.0,
                  "Select a source file".to_string(), pal.text_dim, false);
    }
}

fn paint_locals_pane(
    cmds: &mut Vec<DisplayCommand>,
    pal: &Palette,
    x: f32, y: f32, w: f32, h: f32,
    state: &DevToolsState,
) {
    push_rect(cmds, x, y, 1.0, h, pal.border);
    push_rect(cmds, x + 1.0, y, w - 1.0, h, pal.bg_panel_alt);
    push_text_bold(cmds, x + 8.0, y + 8.0, "Local Variables".to_string(), pal.text, false);
    let mut sy = y + 8.0 + ROW_H + 2.0;
    let max_y = y + h;
    if state.sources.locals.is_empty() {
        push_text(cmds, x + 12.0, sy, "(no locals captured)".to_string(),
                  pal.text_dim, true);
        return;
    }
    for (name, val) in &state.sources.locals {
        if sy + ROW_H > max_y { break; }
        // Multi-line value: prvni line s name, dalsi indented.
        let mut first = true;
        for line in val.lines() {
            if sy + ROW_H > max_y { break; }
            if first {
                push_text(cmds, x + 8.0, sy, format!("{}:", name), pal.syn_attr, false);
                let trunc: String = line.chars().take(40).collect();
                push_text(cmds, x + 8.0 + (name.len() + 2) as f32 * FONT_W, sy,
                          trunc, pal.syn_value, false);
                first = false;
            } else {
                push_text(cmds, x + 16.0, sy, line.chars().take(40).collect(), pal.text_dim, false);
            }
            sy += ROW_H;
        }
    }
}

fn paint_source_content(
    cmds: &mut Vec<DisplayCommand>,
    file: &crate::devtools::model::sources::SourceFile,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, h: f32,
) {
    let gutter_w = 50.0;
    push_rect(cmds, x, y, gutter_w, h, pal.bg_panel_alt);

    // Toggle "Show original" button (kdyz source map je k dispozici).
    if file.source_map.is_some() {
        let btn_w = 100.0;
        let btn_x = x + w - btn_w - 8.0;
        let btn_y = y + 4.0;
        let bg = if state.sources.show_original { pal.accent } else { pal.bg_button };
        push_rect(cmds, btn_x, btn_y, btn_w, ROW_H, bg);
        push_text(cmds, btn_x + 6.0, btn_y + 3.0,
                  if state.sources.show_original { "Original".into() } else { "Generated".into() },
                  if state.sources.show_original { pal.text_inverted } else { pal.text }, false);
    }

    // Vyber content - bud generated nebo original z source map sourcesContent[0].
    let content_buffer: String = if state.sources.show_original {
        if let Some(map) = &file.source_map {
            if let Some(Some(orig)) = map.sources_content.first() {
                orig.clone()
            } else { file.content.clone() }
        } else { file.content.clone() }
    } else { file.content.clone() };
    let lines: Vec<&str> = content_buffer.lines().collect();
    let scroll_y = state.sources.scroll_y;
    let max_visible = ((h / ROW_H).ceil() as usize) + 1;
    let start = (scroll_y / ROW_H) as usize;
    for (i, line) in lines.iter().enumerate().skip(start).take(max_visible) {
        let ly = y + (i as f32 - scroll_y / ROW_H) * ROW_H;
        if ly + ROW_H < y || ly > y + h { continue; }
        let line_no = i + 1;
        let has_bp = state.sources.has_breakpoint(file.id, line_no as u32);
        let is_pause = state.sources.current_pause_location ==
                       Some((file.id, line_no as u32));
        if has_bp {
            push_rect(cmds, x + 4.0, ly + 2.0, gutter_w - 8.0, ROW_H - 4.0, pal.accent);
        }
        if is_pause {
            push_rect(cmds, x + gutter_w, ly, w - gutter_w, ROW_H,
                      pal.bg_row_selected_inactive);
        }
        push_text(cmds, x + 6.0, ly + 3.0, line_no.to_string(),
                  if has_bp { pal.text_inverted } else { pal.text_dim }, false);
        push_text(cmds, x + gutter_w + 8.0, ly + 3.0, line.to_string(), pal.text, false);
    }
}

// ─── Performance tab ────────────────────────────────────────────────────

fn paint_performance_tab(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    win_w: f32,
    content_y: f32,
    content_h: f32,
) {
    push_rect(cmds, 0.0, content_y, win_w, content_h, pal.bg_panel);
    push_text_bold(cmds, 12.0, content_y + 12.0,
                   format!("Avg frame: {:.2}ms", state.performance.avg_total_ms()),
                   pal.text, false);
    push_text(cmds, 12.0, content_y + 12.0 + ROW_H,
              format!("Layout cache: {} hits / {} misses",
                      state.performance.layout_cache_hits,
                      state.performance.layout_cache_misses),
              pal.text_dim, false);

    // Frame time graf - posledni N samples jako sloupce.
    let graph_x = 12.0;
    let graph_y = content_y + 80.0;
    let graph_w = (win_w - 24.0).min(800.0);
    let graph_h = (content_h - 100.0).min(200.0);
    push_rect(cmds, graph_x, graph_y, graph_w, graph_h, pal.bg_panel_alt);
    let samples = state.performance.ordered();
    let max_ms: f32 = samples.iter().map(|s| s.total_ms).fold(16.0, f32::max);
    let bar_w = graph_w / samples.len() as f32;
    for (i, s) in samples.iter().enumerate() {
        if s.total_ms <= 0.0 { continue; }
        let h_norm = (s.total_ms / max_ms).min(1.0);
        let bar_h = h_norm * graph_h;
        let bar_x = graph_x + (i as f32) * bar_w;
        let bar_y = graph_y + graph_h - bar_h;
        let color = if s.total_ms > 16.7 { pal.log_warn }
                    else if s.total_ms > 33.3 { pal.log_error }
                    else { pal.accent };
        push_rect(cmds, bar_x, bar_y, bar_w.max(1.0), bar_h, color);
    }
    // 16.7ms threshold cara (60 fps).
    let line_y = graph_y + graph_h - (16.7 / max_ms) * graph_h;
    push_rect(cmds, graph_x, line_y, graph_w, 1.0, pal.border_strong);
}

// ─── Application tab ────────────────────────────────────────────────────

fn paint_application_tab(
    cmds: &mut Vec<DisplayCommand>,
    _state: &DevToolsState,
    pal: &Palette,
    interp: Option<&Interpreter>,
    win_w: f32,
    content_y: f32,
    content_h: f32,
) {
    push_rect(cmds, 0.0, content_y, win_w, content_h, pal.bg_panel);

    let mut sy = content_y + 8.0;
    let pad_x = 12.0;

    // localStorage section.
    push_text_bold(cmds, pad_x, sy, "localStorage".to_string(), pal.text, false);
    sy += ROW_H + 2.0;
    let local_entries = read_storage(interp, "localStorage");
    if local_entries.is_empty() {
        push_text(cmds, pad_x + 12.0, sy, "(empty)".to_string(), pal.text_dim, true);
        sy += ROW_H;
    } else {
        for (k, v) in &local_entries {
            if sy + ROW_H > content_y + content_h { break; }
            push_text(cmds, pad_x + 12.0, sy, k.clone(), pal.syn_attr, false);
            let trunc: String = v.chars().take(80).collect();
            push_text(cmds, pad_x + 220.0, sy, trunc, pal.syn_value, false);
            sy += ROW_H;
        }
    }

    sy += 12.0;
    push_text_bold(cmds, pad_x, sy, "sessionStorage".to_string(), pal.text, false);
    sy += ROW_H + 2.0;
    let session_entries = read_storage(interp, "sessionStorage");
    if session_entries.is_empty() {
        push_text(cmds, pad_x + 12.0, sy, "(empty)".to_string(), pal.text_dim, true);
        sy += ROW_H;
    } else {
        for (k, v) in &session_entries {
            if sy + ROW_H > content_y + content_h { break; }
            push_text(cmds, pad_x + 12.0, sy, k.clone(), pal.syn_attr, false);
            let trunc: String = v.chars().take(80).collect();
            push_text(cmds, pad_x + 220.0, sy, trunc, pal.syn_value, false);
            sy += ROW_H;
        }
    }

    // Cookies sekce.
    sy += 12.0;
    push_text_bold(cmds, pad_x, sy, "Cookies".to_string(), pal.text, false);
    sy += ROW_H + 2.0;
    let cookies = read_cookies(interp);
    if cookies.is_empty() {
        push_text(cmds, pad_x + 12.0, sy, "(empty)".to_string(), pal.text_dim, true);
        sy += ROW_H;
    } else {
        for (k, v) in &cookies {
            if sy + ROW_H > content_y + content_h { break; }
            push_text(cmds, pad_x + 12.0, sy, k.clone(), pal.syn_attr, false);
            push_text(cmds, pad_x + 220.0, sy, v.clone(), pal.syn_value, false);
            sy += ROW_H;
        }
    }

    // IndexedDB sekce.
    sy += 12.0;
    push_text_bold(cmds, pad_x, sy, "IndexedDB".to_string(), pal.text, false);
    sy += ROW_H + 2.0;
    let idb_dbs = read_indexeddb(interp);
    if idb_dbs.is_empty() {
        push_text(cmds, pad_x + 12.0, sy, "(empty)".to_string(), pal.text_dim, true);
    } else {
        for (db_name, stores) in &idb_dbs {
            if sy + ROW_H > content_y + content_h { break; }
            push_text(cmds, pad_x + 12.0, sy, db_name.clone(), pal.syn_attr, false);
            push_text(cmds, pad_x + 220.0, sy, format!("{} store(s)", stores.len()),
                      pal.syn_value, false);
            sy += ROW_H;
            for store in stores {
                if sy + ROW_H > content_y + content_h { break; }
                push_text(cmds, pad_x + 24.0, sy, format!("- {}", store), pal.text_dim, false);
                sy += ROW_H;
            }
        }
    }
}

fn read_cookies(interp: Option<&Interpreter>) -> Vec<(String, String)> {
    let Some(interp) = interp else { return Vec::new() };
    // document.cookie je string "k1=v1; k2=v2; ..."
    let env = interp.global.borrow();
    let Some(doc_v) = env.get("document") else { return Vec::new() };
    let crate::interpreter::JsValue::Object(doc_rc) = doc_v else { return Vec::new() };
    let cookie_str = doc_rc.borrow().get("cookie");
    let crate::interpreter::JsValue::Str(s) = cookie_str else { return Vec::new() };
    s.split(';').filter_map(|pair| {
        let pair = pair.trim();
        let (k, v) = pair.split_once('=')?;
        Some((k.trim().to_string(), v.trim().to_string()))
    }).collect()
}

fn read_indexeddb(interp: Option<&Interpreter>) -> Vec<(String, Vec<String>)> {
    let Some(interp) = interp else { return Vec::new() };
    let env = interp.global.borrow();
    let Some(idb_v) = env.get("indexedDB") else { return Vec::new() };
    let crate::interpreter::JsValue::Object(idb_rc) = idb_v else { return Vec::new() };
    let dbs_v = idb_rc.borrow().get("__databases__");
    let crate::interpreter::JsValue::Object(dbs_rc) = dbs_v else { return Vec::new(); };
    let dbs = dbs_rc.borrow();
    dbs.own_keys().iter().map(|name| {
        let db_v = dbs.get(name);
        let stores: Vec<String> = if let crate::interpreter::JsValue::Object(db_rc) = db_v {
            let stores_v = db_rc.borrow().get("__stores__");
            if let crate::interpreter::JsValue::Object(s_rc) = stores_v {
                s_rc.borrow().own_keys()
            } else { Vec::new() }
        } else { Vec::new() };
        (name.clone(), stores)
    }).collect()
}

/// Vrati (key, value) pary z storage objektu (interp.global.{name}).
fn read_storage(interp: Option<&Interpreter>, name: &str) -> Vec<(String, String)> {
    let Some(interp) = interp else { return Vec::new() };
    let env = interp.global.borrow();
    let Some(value) = env.get(name) else { return Vec::new() };
    let crate::interpreter::JsValue::Object(obj_rc) = value else { return Vec::new() };
    let obj = obj_rc.borrow();
    let data_v = obj.get("__storage_data__");
    if matches!(data_v, crate::interpreter::JsValue::Undefined) { return Vec::new(); }
    let crate::interpreter::JsValue::Object(data_rc) = data_v else { return Vec::new(); };
    let data = data_rc.borrow();
    data.own_keys().iter()
        .map(|k| (k.clone(), data.get(k).to_string()))
        .collect()
}

// ─── Settings tab ───────────────────────────────────────────────────────

fn paint_settings_tab(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    win_w: f32,
    content_y: f32,
    content_h: f32,
    mouse_x: f32, mouse_y: f32,
) {
    push_rect(cmds, 0.0, content_y, win_w, content_h, pal.bg_panel);
    push_text_bold(cmds, 16.0, content_y + 12.0, "Theme".to_string(), pal.text, false);

    let mut y = content_y + 16.0 + ROW_H;
    paint_theme_choice(cmds, state, pal, "Auto (system)",
                       crate::devtools::theme::ThemeMode::Auto,
                       16.0, y, mouse_x, mouse_y);
    y += ROW_H + 4.0;
    paint_theme_choice(cmds, state, pal, "Light",
                       crate::devtools::theme::ThemeMode::Light,
                       16.0, y, mouse_x, mouse_y);
    y += ROW_H + 4.0;
    paint_theme_choice(cmds, state, pal, "Dark",
                       crate::devtools::theme::ThemeMode::Dark,
                       16.0, y, mouse_x, mouse_y);

    y += 16.0 + ROW_H;
    push_text_bold(cmds, 16.0, y, "Flavor".to_string(), pal.text, false);
    y += ROW_H + 4.0;
    paint_flavor_choice(cmds, state, pal, "Chrome",
                        crate::devtools::theme::ThemeFlavor::Chrome,
                        16.0, y, mouse_x, mouse_y);
    y += ROW_H + 4.0;
    paint_flavor_choice(cmds, state, pal, "Firefox",
                        crate::devtools::theme::ThemeFlavor::Firefox,
                        16.0, y, mouse_x, mouse_y);
}

fn paint_theme_choice(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    label: &str,
    mode: crate::devtools::theme::ThemeMode,
    x: f32, y: f32,
    mouse_x: f32, mouse_y: f32,
) {
    let w = 220.0;
    let h = ROW_H;
    let selected = state.theme.mode == mode;
    let hover = mouse_x >= x && mouse_x < x + w && mouse_y >= y && mouse_y < y + h;
    let bg = if selected { pal.bg_row_selected }
             else if hover { pal.bg_row_hover }
             else { pal.bg_panel };
    push_rect(cmds, x, y, w, h, bg);
    let dot_color = if selected { pal.text_inverted } else { pal.text_dim };
    push_text(cmds, x + 8.0, y + 3.0, format!("{} {}",
              if selected { "\u{25C9}" } else { "\u{25CB}" }, label),
              if selected { pal.text_on_accent } else { pal.text }, false);
    let _ = dot_color;
}

fn paint_flavor_choice(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    label: &str,
    flavor: crate::devtools::theme::ThemeFlavor,
    x: f32, y: f32,
    mouse_x: f32, mouse_y: f32,
) {
    let w = 220.0;
    let h = ROW_H;
    let selected = state.theme.flavor == flavor;
    let hover = mouse_x >= x && mouse_x < x + w && mouse_y >= y && mouse_y < y + h;
    let bg = if selected { pal.bg_row_selected }
             else if hover { pal.bg_row_hover }
             else { pal.bg_panel };
    push_rect(cmds, x, y, w, h, bg);
    push_text(cmds, x + 8.0, y + 3.0, format!("{} {}",
              if selected { "\u{25C9}" } else { "\u{25CB}" }, label),
              if selected { pal.text_on_accent } else { pal.text }, false);
}

// ─── Context menu ───────────────────────────────────────────────────────

fn paint_context_menu(
    cmds: &mut Vec<DisplayCommand>,
    pal: &Palette,
    menu: &crate::devtools::context_menu::ContextMenuState,
    mouse_x: f32, mouse_y: f32,
) {
    use crate::devtools::context_menu::MenuItem;
    let item_h = ROW_H + 6.0;
    let menu_w = 260.0;
    let mut menu_h = 8.0;
    for item in &menu.items {
        menu_h += match item {
            MenuItem::Action { .. } => item_h,
            MenuItem::Separator => 8.0,
        };
    }
    // Drop shadow (offset 2px).
    push_rect(cmds, menu.x + 2.0, menu.y + 2.0, menu_w, menu_h, [0, 0, 0, 80]);
    push_rect(cmds, menu.x, menu.y, menu_w, menu_h, pal.bg_context_menu);
    push_rect_border(cmds, menu.x, menu.y, menu_w, menu_h, pal.border_strong);

    let mut y = menu.y + 4.0;
    for (i, item) in menu.items.iter().enumerate() {
        match item {
            MenuItem::Action { label, enabled, shortcut, .. } => {
                // Hover detect z mouse pos (state.context_menu.hovered je out-of-date).
                let hover_now = mouse_x >= menu.x + 2.0 && mouse_x < menu.x + menu_w - 2.0
                                && mouse_y >= y && mouse_y < y + item_h;
                let hover_state = menu.hovered == Some(i) || hover_now;
                if hover_state && *enabled {
                    push_rect(cmds, menu.x + 4.0, y, menu_w - 8.0, item_h,
                              pal.bg_context_menu_hover);
                }
                let txt_color = if !*enabled { pal.text_disabled }
                                else if hover_state { pal.text_on_accent }
                                else { pal.text };
                push_text(cmds, menu.x + 14.0, y + 5.0, label.clone(), txt_color, false);
                if let Some(sh) = shortcut {
                    let sh_x = menu.x + menu_w - 14.0 - (sh.chars().count() as f32) * FONT_W;
                    push_text(cmds, sh_x, y + 5.0, sh.clone(), pal.text_dim, false);
                }
                y += item_h;
            }
            MenuItem::Separator => {
                push_rect(cmds, menu.x + 8.0, y + 3.0, menu_w - 16.0, 1.0, pal.border);
                y += 8.0;
            }
        }
    }
}

// ─── Element highlight overlay ──────────────────────────────────────────

/// Vykresli highlight pres vybrany / hover element (Chrome-like content/padding/
/// border/margin barevne pasky + label s rozmery). Volana z render flow VZDY,
/// nezavisle na panel_open. Rect je v render-space (po scroll_y odecet).
pub fn paint_element_highlight(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    state: &DevToolsState,
    scroll_y: f32,
) {
    let pal = state.palette();
    let target = state.elements.hovered.or(state.elements.selected);
    let Some(node_id) = target else { return };
    let Some(bx) = find_layout_box(layout_root, node_id) else { return };

    // Rect obsahu = bx.rect (uz po margin/padding pripravne v build_box).
    let r = &bx.rect;
    let p = bx.padding;
    let m = bx.margin;
    let bw = bx.border_width.max(0.0);

    let content_x = r.x;
    let content_y = r.y - scroll_y;
    let content_w = r.width;
    let content_h = r.height;

    // Box rect = content. Padding box = +padding na vsech stranach.
    // Border box = +border. Margin box = +margin.
    // Vykresli je ako 4 vrstvy (margin -> border -> padding -> content), kazda jen
    // ramecek (vnejsi minus vnitrni).
    // Margin rect (oranzova).
    let mx = content_x - p - bw - m;
    let my = content_y - p - bw - m;
    let mw = content_w + 2.0 * (p + bw + m);
    let mh = content_h + 2.0 * (p + bw + m);
    push_rect(cmds, mx, my, mw, mh, pal.overlay_margin);

    // Border rect (zluta).
    let bx_x = content_x - p - bw;
    let by_y = content_y - p - bw;
    let bw_w = content_w + 2.0 * (p + bw);
    let bh_h = content_h + 2.0 * (p + bw);
    push_rect(cmds, bx_x, by_y, bw_w, bh_h, pal.overlay_border);

    // Padding rect (zelena).
    let px = content_x - p;
    let py = content_y - p;
    let pw = content_w + 2.0 * p;
    let ph = content_h + 2.0 * p;
    push_rect(cmds, px, py, pw, ph, pal.overlay_padding);

    // Content rect (modra).
    push_rect(cmds, content_x, content_y, content_w, content_h, pal.overlay_content);

    // Label box - "tag.class 320 x 200".
    let tag = bx.node.as_ref().and_then(|n|
        if let NodeKind::Element(t) = &n.kind { Some(t.clone()) } else { None }
    ).unwrap_or_else(|| "?".to_string());
    let class_attr = bx.node.as_ref().and_then(|n|
        n.attributes.borrow().iter().find(|(k, _)| k.as_str() == "class").map(|(_, v)| v.clone())
    ).unwrap_or_default();
    let id_attr = bx.node.as_ref().and_then(|n|
        n.attributes.borrow().iter().find(|(k, _)| k.as_str() == "id").map(|(_, v)| v.clone())
    ).unwrap_or_default();
    let mut label = tag.clone();
    if !id_attr.is_empty() {
        label.push('#');
        label.push_str(&id_attr);
    }
    if !class_attr.is_empty() {
        for c in class_attr.split_whitespace().take(3) {
            label.push('.');
            label.push_str(c);
        }
    }
    let dims = format!("  {} x {}", content_w as i32, content_h as i32);
    let full_label = format!("{}{}", label, dims);
    let lw = full_label.len() as f32 * FONT_W + 16.0;
    let lh = ROW_H + 4.0;
    let lx = content_x;
    let ly = (my - lh - 2.0).max(2.0);

    push_rect(cmds, lx, ly, lw, lh, pal.overlay_label_bg);
    push_text(cmds, lx + 8.0, ly + 4.0, label.clone(), pal.accent, true);
    push_text(cmds, lx + 8.0 + label.len() as f32 * FONT_W, ly + 4.0,
              dims, pal.overlay_label_text, false);
}

// ─── Hit-test ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum DevtoolsHit {
    None,
    /// Resize grip drag.
    ResizeGrip,
    /// Tab klik (Tab variant).
    TabClick(Tab),
    /// Klik na tree row - node_id.
    TreeRow(usize),
    /// Klik na expand caret (toggle collapse) - node_id.
    TreeCaret(usize),
    /// Klik na search bar (focus do nej).
    ElementsSearchBar,
    /// Klik na settings theme volbu.
    ThemeChoice(crate::devtools::theme::ThemeMode),
    /// Klik na flavor volbu.
    FlavorChoice(crate::devtools::theme::ThemeFlavor),
    /// Inspect button toggle.
    InspectToggle,
    /// Theme button toggle (Light/Dark only).
    ThemeToggle,
    /// Close button (X) - schova panel.
    Close,
    /// Klik na panel area mimo specific control - generic console focus.
    PanelArea,
    /// Console input focus.
    ConsoleInput,
    /// Sources file row.
    SourcesFileRow(u32),
    /// Sources line gutter klik (toggle BP).
    SourcesGutter { file_id: u32, line: u32 },
    /// Network row klik.
    NetworkRow(usize),
    /// Context menu item klik (item idx).
    ContextMenuItem(usize),
    /// Klik mimo - zavri context menu.
    DismissContextMenu,
    /// Dvojklik na attribute value zone - zacni editaci.
    EditAttributeValue { node_id: usize, attr: String },
    /// Dvojklik na text node - zacni editaci.
    EditTextNode { node_id: usize },
    /// Dvojklik v Computed/Styles panel na property hodnotu.
    EditStyleValue { node_id: usize, property: String },
    /// Network filter tab klik.
    NetworkFilterClick(crate::devtools::model::network::NetworkFilter),
    /// Toggle "show original source" v Sources.
    SourcesToggleOriginal,
    /// Continue debugger button v Sources tab.
    DebuggerContinue,
    /// Step Over button.
    DebuggerStepOver,
    /// Step Into button.
    DebuggerStepInto,
    /// Step Out button.
    DebuggerStepOut,
}

pub fn devtools_hit_test(
    state: &DevToolsState,
    layout_root: &LayoutBox,
    win_w: f32,
    win_h: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    if !state.panel_open { return DevtoolsHit::None; }
    let panel_h = state.panel_h.min(win_h * 0.7);
    let panel_y = win_h - panel_h;

    // Context menu hit-test ma prioritu.
    if let Some(menu) = &state.context_menu {
        let menu_w = 240.0;
        let item_h = ROW_H + 4.0;
        if mouse_x >= menu.x && mouse_x < menu.x + menu_w {
            let mut y = menu.y + 2.0;
            for (i, item) in menu.items.iter().enumerate() {
                match item {
                    crate::devtools::context_menu::MenuItem::Action { .. } => {
                        if mouse_y >= y && mouse_y < y + item_h {
                            return DevtoolsHit::ContextMenuItem(i);
                        }
                        y += item_h;
                    }
                    crate::devtools::context_menu::MenuItem::Separator => {
                        y += 6.0;
                    }
                }
            }
        }
        return DevtoolsHit::DismissContextMenu;
    }

    if mouse_y < panel_y { return DevtoolsHit::None; }
    if mouse_y < panel_y + RESIZE_GRIP_H {
        return DevtoolsHit::ResizeGrip;
    }
    let toolbar_y = panel_y + RESIZE_GRIP_H;
    if mouse_y < toolbar_y + TAB_H {
        // Tab klik?
        for (i, t) in Tab::all().iter().enumerate() {
            let (tx, ty, tw, th) = tab_rect(i, toolbar_y);
            if mouse_x >= tx && mouse_x < tx + tw && mouse_y >= ty && mouse_y < ty + th {
                return DevtoolsHit::TabClick(*t);
            }
        }
        // Toolbar actions vpravo.
        let mut x_right = toolbar_actions_x(win_w);
        // Close.
        let close_w = 24.0;
        x_right -= close_w;
        if mouse_x >= x_right && mouse_x < x_right + close_w {
            return DevtoolsHit::Close;
        }
        // Theme dot.
        let theme_w = 24.0;
        x_right -= theme_w + 4.0;
        if mouse_x >= x_right && mouse_x < x_right + theme_w {
            return DevtoolsHit::ThemeToggle;
        }
        // Inspect.
        let insp_w = 80.0;
        x_right -= insp_w + 4.0;
        if mouse_x >= x_right && mouse_x < x_right + insp_w {
            return DevtoolsHit::InspectToggle;
        }
        return DevtoolsHit::PanelArea;
    }
    let content_y = toolbar_y + TAB_H;
    let content_h = panel_h - RESIZE_GRIP_H - TAB_H;
    match state.tab {
        Tab::Elements => hit_test_elements(state, layout_root, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Console => {
            // Klik dole = console input focus.
            let input_h = 32.0;
            let input_y = content_y + content_h - input_h;
            if mouse_y >= input_y { return DevtoolsHit::ConsoleInput; }
            DevtoolsHit::PanelArea
        }
        Tab::Sources => hit_test_sources(state, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Network => hit_test_network(state, win_w, content_y, content_h, mouse_x, mouse_y),
        Tab::Settings => hit_test_settings(state, content_y, mouse_x, mouse_y),
        _ => DevtoolsHit::PanelArea,
    }
}

fn hit_test_elements(
    state: &DevToolsState,
    _layout_root: &LayoutBox,
    win_w: f32,
    content_y: f32,
    content_h: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    let search_h = if state.elements.search.open { SEARCH_H } else { 0.0 };
    if state.elements.search.open && mouse_y < content_y + search_h {
        return DevtoolsHit::ElementsSearchBar;
    }
    let body_y = content_y + search_h;
    let _body_h = content_h - search_h;
    let split_x = state.elements.split_x.max(200.0).min(win_w - 220.0);
    if mouse_x >= split_x { return DevtoolsHit::PanelArea; }

    let scroll_y = state.elements.scroll_y;
    let row_idx = ((mouse_y - body_y + scroll_y) / ROW_H) as usize;
    if row_idx >= state.elements.rows.len() { return DevtoolsHit::PanelArea; }
    let row = &state.elements.rows[row_idx];
    let caret_x = 8.0 + row.depth as f32 * INDENT_PX - INDENT_PX;
    if let RowKind::Element { has_children: true, .. } = &row.kind {
        if mouse_x >= caret_x && mouse_x < caret_x + INDENT_PX {
            return DevtoolsHit::TreeCaret(row.node_id);
        }
    }
    DevtoolsHit::TreeRow(row.node_id)
}

/// Najdi attribute pri x souradnici v dane Element row. Vraci Some((attr_name))
/// pokud kurzor je nad attr value oblasti (mezi `="` a `"`).
pub fn attribute_at_x(row: &ElementRow, mouse_x: f32) -> Option<String> {
    let RowKind::Element { tag, attrs, .. } = &row.kind else { return None };
    let mut x = 8.0 + row.depth as f32 * INDENT_PX;
    x += (tag.len() + 1) as f32 * FONT_W;
    for (k, v) in attrs {
        let attr_str = format!(" {}=", k);
        x += attr_str.len() as f32 * FONT_W;
        let val_truncated: String = v.chars().take(40).collect();
        let val_str = if val_truncated.chars().count() < v.chars().count() {
            format!("\"{}...\"", val_truncated)
        } else {
            format!("\"{}\"", val_truncated)
        };
        let val_w = val_str.len() as f32 * FONT_W;
        if mouse_x >= x && mouse_x < x + val_w {
            return Some(k.clone());
        }
        x += val_w;
    }
    None
}

/// Detekce dvojkliku zony pro Elements tree. Vraci hit pokud kurzor je nad
/// attr value (-> EditAttributeValue), text node (-> EditTextNode) nebo
/// computed property hodnotu v styles pane (-> EditStyleValue).
pub fn double_click_hit_elements(
    state: &DevToolsState,
    win_w: f32, content_y: f32, mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    let search_h = if state.elements.search.open { SEARCH_H } else { 0.0 };
    let body_y = content_y + search_h;
    let split_x = state.elements.split_x.max(200.0).min(win_w - 220.0);

    // Styles pane (right) - dvojklik na property hodnotu v Computed sekci.
    if mouse_x >= split_x {
        if let Some(node_id) = state.elements.selected {
            // Computed sekce zacina ~ 4 ROW_H pod top (po Matched rules
            // sekci). Pragmaticky najdi line ktery odpovida property zona
            // za "{prop}:" labelem (x > split_x + 160).
            let local_x = mouse_x - (split_x + 12.0);
            if local_x > 148.0 {
                // Iteruj computed na zaklade y pozice. Spocitej "Computed"
                // header offset.
                let mut sy = body_y + 8.0;
                sy += ROW_H + 2.0; // "Matched CSS rules"
                if state.styles.matched_rules.is_empty() {
                    sy += ROW_H + 8.0;
                } else {
                    for r in &state.styles.matched_rules {
                        sy += ROW_H;
                        sy += (r.declarations.len() as f32) * ROW_H;
                        sy += ROW_H + 4.0;
                    }
                }
                sy += ROW_H + 2.0; // "Computed" header
                for (k, _) in &state.styles.computed {
                    if mouse_y >= sy && mouse_y < sy + ROW_H {
                        return DevtoolsHit::EditStyleValue { node_id, property: k.clone() };
                    }
                    sy += ROW_H;
                }
            }
        }
        return DevtoolsHit::PanelArea;
    }

    let scroll_y = state.elements.scroll_y;
    let row_idx = ((mouse_y - body_y + scroll_y) / ROW_H) as usize;
    if row_idx >= state.elements.rows.len() { return DevtoolsHit::PanelArea; }
    let row = &state.elements.rows[row_idx];
    match &row.kind {
        RowKind::Element { .. } => {
            if let Some(attr) = attribute_at_x(row, mouse_x) {
                return DevtoolsHit::EditAttributeValue { node_id: row.node_id, attr };
            }
        }
        RowKind::Text(_) => {
            return DevtoolsHit::EditTextNode { node_id: row.node_id };
        }
        _ => {}
    }
    DevtoolsHit::TreeRow(row.node_id)
}

fn hit_test_sources(
    state: &DevToolsState,
    _win_w: f32,
    content_y: f32,
    _content_h: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    // Debugger toolbar nahore - 28px high.
    let dbg_h = 28.0;
    if mouse_y < content_y + dbg_h {
        let labels = ["Continue", "Step Over", "Step Into", "Step Out"];
        let actions = [
            DevtoolsHit::DebuggerContinue,
            DevtoolsHit::DebuggerStepOver,
            DevtoolsHit::DebuggerStepInto,
            DevtoolsHit::DebuggerStepOut,
        ];
        let mut x = 8.0;
        for (i, label) in labels.iter().enumerate() {
            let w = label.len() as f32 * FONT_W + 16.0;
            if mouse_x >= x && mouse_x < x + w {
                return actions[i].clone();
            }
            x += w + 4.0;
        }
        return DevtoolsHit::PanelArea;
    }
    let body_y = content_y + dbg_h;
    let split_x = 240.0;
    if mouse_x < split_x {
        let row_idx = ((mouse_y - (body_y + ROW_H + 8.0)) / ROW_H) as usize;
        if row_idx < state.sources.files.len() {
            return DevtoolsHit::SourcesFileRow(state.sources.files[row_idx].id);
        }
        return DevtoolsHit::PanelArea;
    }
    // "Show original" button (kdyz source map ma file).
    if mouse_y < body_y + ROW_H + 8.0 {
        if let Some(file_id) = state.sources.selected_id {
            if let Some(file) = state.sources.files.iter().find(|f| f.id == file_id) {
                if file.source_map.is_some() {
                    return DevtoolsHit::SourcesToggleOriginal;
                }
            }
        }
    }
    // Gutter klik = toggle breakpoint.
    let gutter_w = 50.0;
    if mouse_x < split_x + gutter_w {
        if let Some(file_id) = state.sources.selected_id {
            let scroll_y = state.sources.scroll_y;
            let line_idx = ((mouse_y - body_y + scroll_y) / ROW_H) as usize;
            return DevtoolsHit::SourcesGutter { file_id, line: line_idx as u32 + 1 };
        }
    }
    DevtoolsHit::PanelArea
}

fn hit_test_network(
    state: &DevToolsState,
    _win_w: f32,
    content_y: f32,
    _content_h: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    use crate::devtools::model::network::NetworkFilter;
    let filter_h = 26.0;
    if mouse_y < content_y + filter_h {
        let filters = [
            ("All", NetworkFilter::All),
            ("Doc", NetworkFilter::Document),
            ("CSS", NetworkFilter::Stylesheet),
            ("JS", NetworkFilter::Script),
            ("Img", NetworkFilter::Image),
            ("Font", NetworkFilter::Font),
            ("XHR", NetworkFilter::Xhr),
        ];
        let mut fx = 8.0;
        for (label, f) in filters.iter() {
            let w = label.len() as f32 * FONT_W + 14.0;
            if mouse_x >= fx && mouse_x < fx + w {
                return DevtoolsHit::NetworkFilterClick(*f);
            }
            fx += w + 2.0;
        }
        return DevtoolsHit::PanelArea;
    }
    let body_y = content_y + filter_h;
    let header_h = ROW_H + 4.0;
    if mouse_y < body_y + header_h { return DevtoolsHit::PanelArea; }
    let row_y = body_y + header_h + 2.0;
    let idx = ((mouse_y - row_y) / ROW_H) as usize;
    if idx < state.network.entries.len() {
        DevtoolsHit::NetworkRow(idx)
    } else {
        DevtoolsHit::PanelArea
    }
}

fn hit_test_settings(
    state: &DevToolsState,
    content_y: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    use crate::devtools::theme::{ThemeMode, ThemeFlavor};
    let _ = state;
    // Theme volby (3 radky pod hlavnim Theme labelem).
    let theme_y0 = content_y + 16.0 + ROW_H;
    let modes = [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark];
    for (i, m) in modes.iter().enumerate() {
        let y = theme_y0 + i as f32 * (ROW_H + 4.0);
        if mouse_x >= 16.0 && mouse_x < 16.0 + 220.0 && mouse_y >= y && mouse_y < y + ROW_H {
            return DevtoolsHit::ThemeChoice(*m);
        }
    }
    let flavor_y0 = theme_y0 + 3.0 * (ROW_H + 4.0) + 16.0 + ROW_H;
    let flavors = [ThemeFlavor::Chrome, ThemeFlavor::Firefox];
    for (i, f) in flavors.iter().enumerate() {
        let y = flavor_y0 + i as f32 * (ROW_H + 4.0);
        if mouse_x >= 16.0 && mouse_x < 16.0 + 220.0 && mouse_y >= y && mouse_y < y + ROW_H {
            return DevtoolsHit::FlavorChoice(*f);
        }
    }
    DevtoolsHit::PanelArea
}

// ─── Layout helpers ─────────────────────────────────────────────────────

pub fn find_layout_box(bx: &LayoutBox, node_id: usize) -> Option<&LayoutBox> {
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

pub fn find_box_rect_by_id(bx: &LayoutBox, node_id: usize, scroll_y: f32) -> Option<(f32, f32, f32, f32)> {
    let found = find_layout_box(bx, node_id)?;
    Some((found.rect.x, found.rect.y - scroll_y, found.rect.width, found.rect.height))
}

pub fn pick_node_at_screen_pos(bx: &LayoutBox, mouse_x: f32, mouse_y: f32, scroll_y: f32) -> Option<usize> {
    let py = mouse_y + scroll_y;
    pick_recursive(bx, mouse_x, py)
}

fn pick_recursive(bx: &LayoutBox, mx: f32, my: f32) -> Option<usize> {
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

// ─── Paint helpers ─────────────────────────────────────────────────────

fn push_rect(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
    cmds.push(DisplayCommand::Rect { x, y, w, h, color, radius: 0.0 });
}

fn push_rect_border(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
    cmds.push(DisplayCommand::Rect { x, y, w, h: 1.0, color, radius: 0.0 });
    cmds.push(DisplayCommand::Rect { x, y: y + h - 1.0, w, h: 1.0, color, radius: 0.0 });
    cmds.push(DisplayCommand::Rect { x, y, w: 1.0, h, color, radius: 0.0 });
    cmds.push(DisplayCommand::Rect { x: x + w - 1.0, y, w: 1.0, h, color, radius: 0.0 });
}

fn push_text(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], italic: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: false,
        italic,
        font_family: if italic { DT_FONT_ITALIC.into() } else { DT_FONT.into() },
        strikethrough: false, underline: false,
    });
}

fn push_text_bold(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], italic: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: true,
        italic,
        font_family: DT_FONT_BOLD.into(),
        strikethrough: false, underline: false,
    });
}

fn push_text_italic(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], _bold: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: false,
        italic: true,
        font_family: DT_FONT_ITALIC.into(),
        strikethrough: false, underline: false,
    });
}

fn push_text_underline(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], strikethrough: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: false,
        italic: false,
        font_family: DT_FONT.into(),
        strikethrough,
        underline: false,
    });
}

fn paint_vertical_scrollbar(
    cmds: &mut Vec<DisplayCommand>,
    pal: &Palette,
    x: f32, y: f32, h: f32,
    content_h: f32, scroll: f32,
) {
    push_rect(cmds, x, y, SCROLLBAR_W, h, pal.bg_panel_alt);
    let thumb_h = (h * h / content_h).max(20.0);
    let max_scroll = (content_h - h).max(0.0);
    let thumb_y = if max_scroll > 0.0 {
        y + (scroll / max_scroll) * (h - thumb_h)
    } else {
        y
    };
    push_rect(cmds, x + 2.0, thumb_y, SCROLLBAR_W - 4.0, thumb_h, pal.border_strong);
}

// ─── Public utilities pro caller ───────────────────────────────────────

/// Klikova zkratka: `walk_dom_collect` rebuild tree rows pro state.
/// Vola se kdyz se DOM zmenil (po JS mutacich).
pub fn rebuild_tree(state: &mut DevToolsState, root: &Rc<NodeData>) {
    state.elements.rows = crate::devtools::model::elements::build_rows(root, &state.elements.collapsed);
}
