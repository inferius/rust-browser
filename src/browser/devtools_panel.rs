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
const FONT_W: f32 = 7.0;
const INDENT_PX: f32 = 16.0;
pub const RESIZE_GRIP_H: f32 = 4.0;
const SCROLLBAR_W: f32 = 10.0;
pub const SEARCH_H: f32 = 28.0;

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
              "X".to_string(), pal.text, true);

    // Theme dot (Ctrl+Shift+T toggle).
    let theme_w = 24.0;
    x_right -= theme_w + 4.0;
    let theme_hover = mouse_x >= x_right && mouse_x < x_right + theme_w
                      && mouse_y >= y && mouse_y < y + h;
    push_rect(cmds, x_right, y, theme_w, h,
              if theme_hover { pal.bg_row_hover } else { pal.bg_toolbar });
    let icon = if pal.is_dark { "*" } else { "o" };
    push_text(cmds, x_right + 9.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
              icon.to_string(), pal.text, true);

    // Inspect toggle.
    let insp_w = 80.0;
    x_right -= insp_w + 4.0;
    let insp_hover = mouse_x >= x_right && mouse_x < x_right + insp_w
                     && mouse_y >= y && mouse_y < y + h;
    let bg = if state.inspect_mode { pal.accent }
             else if insp_hover { pal.bg_row_hover }
             else { pal.bg_button };
    push_rect(cmds, x_right, y, insp_w, h, bg);
    let txt = if state.inspect_mode { pal.text_inverted } else { pal.text };
    push_text(cmds, x_right + 8.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
              "Inspect".to_string(), txt, false);
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

    let split_x = state.elements.split_x.max(200.0).min(win_w - 220.0);

    // Tree pozadi.
    push_rect(cmds, 0.0, body_y, split_x, body_h, pal.bg_panel);
    // Right pane pozadi (styles).
    push_rect(cmds, split_x, body_y, win_w - split_x, body_h, pal.bg_panel_alt);
    // Splitter line.
    push_rect(cmds, split_x, body_y, 1.0, body_h, pal.border);

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
        paint_element_row(cmds, row, state, pal, split_x, y, mouse_x, mouse_y);
    }

    // Scrollbar pro tree.
    if total_h > body_h {
        paint_vertical_scrollbar(cmds, pal, split_x - SCROLLBAR_W, body_y,
                                  body_h, total_h, scroll_y);
    }

    // Right pane: matched styles + computed styles.
    if let Some(sel_id) = state.elements.selected {
        if let Some(bx) = find_layout_box(layout_root, sel_id) {
            paint_styles_pane(cmds, bx, state, pal, split_x, body_y, win_w - split_x, body_h);
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
    let is_sel = state.elements.selected == Some(row.node_id);
    let is_hov = mouse_x < width && mouse_y >= y && mouse_y < y + ROW_H;

    if is_sel {
        push_rect(cmds, 0.0, y, width, ROW_H, pal.bg_row_selected);
        push_rect(cmds, 0.0, y, 3.0, ROW_H, pal.accent);
    } else if is_hov {
        push_rect(cmds, 0.0, y, width, ROW_H, pal.bg_row_hover);
    }

    let text_color_default = if is_sel { pal.text_inverted } else { pal.text };
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
                let caret = if collapsed { ">" } else { "v" };
                push_text(cmds, x - INDENT_PX + 4.0, text_y,
                          caret.to_string(), pal.text_dim, false);
            }
            // <tag
            push_text(cmds, x, text_y, format!("<{}", tag),
                      if is_sel { text_color_default } else { pal.syn_tag }, false);
            x += (tag.len() + 1) as f32 * FONT_W;
            // Attrs.
            for (k, v) in attrs {
                let attr_str = format!(" {}=", k);
                push_text(cmds, x, text_y, attr_str.clone(),
                          if is_sel { text_color_default } else { pal.syn_attr }, false);
                x += attr_str.len() as f32 * FONT_W;
                let val_truncated: String = v.chars().take(40).collect();
                let val_str = if val_truncated.chars().count() < v.chars().count() {
                    format!("\"{}...\"", val_truncated)
                } else {
                    format!("\"{}\"", val_truncated)
                };
                push_text(cmds, x, text_y, val_str.clone(),
                          if is_sel { text_color_default } else { pal.syn_value }, false);
                x += val_str.len() as f32 * FONT_W;
                if x > width - 30.0 { break; }
            }
            // Closing > nebo />.
            let close = if *self_closing { " />" } else { ">" };
            push_text(cmds, x, text_y, close.to_string(),
                      if is_sel { text_color_default } else { pal.syn_tag }, false);
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

    // Stick to bottom: render od konce.
    let max_visible = (log_h / ROW_H) as usize;
    let start = entries.len().saturating_sub(max_visible);
    for (i, (lvl, msg)) in entries.iter().skip(start).enumerate() {
        let ey = log_y + 4.0 + (i as f32) * ROW_H;
        let color = match lvl {
            LogLevel::Info | LogLevel::Result => pal.log_info,
            LogLevel::Warn => pal.log_warn,
            LogLevel::Error => pal.log_error,
            LogLevel::InputEcho => pal.log_input_marker,
        };
        // Marker pred input echo.
        let prefix = match lvl {
            LogLevel::InputEcho => "> ",
            LogLevel::Result => "< ",
            _ => "",
        };
        push_text(cmds, 8.0, ey, format!("{}{}", prefix, msg), color, false);
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

    // Selection highlight.
    if let Some((s, e)) = input.selection_range() {
        let sel_x0 = text_x + (s as f32) * FONT_W;
        let sel_x1 = text_x + (e as f32) * FONT_W;
        push_rect(cmds, sel_x0, text_y - 2.0, sel_x1 - sel_x0, FONT_SIZE + 4.0,
                  pal.bg_row_selected);
    }

    push_text(cmds, text_x, text_y, input.text.clone(), pal.text, false);

    // Cursor (blink pri focusu).
    if focused && state.cursor_visible() {
        let cur_x = text_x + (input.cursor as f32) * FONT_W;
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
    let entries: Vec<(String, u16)> = if state.network.entries.is_empty() {
        if let Some(i) = interp {
            i.network_log.borrow().clone()
        } else {
            Vec::new()
        }
    } else {
        state.network.entries.iter().map(|e| (e.url.clone(), e.status)).collect()
    };

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
    let split_x = 240.0;
    push_rect(cmds, split_x, content_y, 1.0, content_h, pal.border);
    push_rect(cmds, 0.0, content_y, split_x, content_h, pal.bg_panel_alt);

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

    // Right pane: source content + breakpoints.
    if let Some(id) = state.sources.selected_id {
        if let Some(file) = state.sources.files.iter().find(|f| f.id == id) {
            paint_source_content(cmds, file, state, pal, split_x + 1.0, content_y,
                                 win_w - split_x - 1.0, content_h);
        }
    } else {
        push_text(cmds, split_x + 12.0, content_y + 12.0,
                  "Select a source file".to_string(), pal.text_dim, false);
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

    let lines: Vec<&str> = file.content.lines().collect();
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
    _interp: Option<&Interpreter>,
    win_w: f32,
    content_y: f32,
    content_h: f32,
) {
    push_rect(cmds, 0.0, content_y, win_w, content_h, pal.bg_panel);
    push_text_bold(cmds, 12.0, content_y + 12.0, "Application".to_string(), pal.text, false);
    push_text(cmds, 12.0, content_y + 12.0 + ROW_H,
              "(Storage / Cookies / IndexedDB - coming soon)".to_string(),
              pal.text_dim, true);
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
    push_text(cmds, x + 8.0, y + 3.0, format!("[{}] {}",
              if selected { "x" } else { " " }, label),
              if selected { pal.text_inverted } else { pal.text }, false);
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
    push_text(cmds, x + 8.0, y + 3.0, format!("[{}] {}",
              if selected { "x" } else { " " }, label),
              if selected { pal.text_inverted } else { pal.text }, false);
}

// ─── Context menu ───────────────────────────────────────────────────────

fn paint_context_menu(
    cmds: &mut Vec<DisplayCommand>,
    pal: &Palette,
    menu: &crate::devtools::context_menu::ContextMenuState,
    _mouse_x: f32, _mouse_y: f32,
) {
    use crate::devtools::context_menu::MenuItem;
    let item_h = ROW_H + 4.0;
    let mut menu_h = 4.0;
    for item in &menu.items {
        menu_h += match item {
            MenuItem::Action { .. } => item_h,
            MenuItem::Separator => 6.0,
        };
    }
    let menu_w = 240.0;
    push_rect(cmds, menu.x, menu.y, menu_w, menu_h, pal.bg_context_menu);
    push_rect_border(cmds, menu.x, menu.y, menu_w, menu_h, pal.border_strong);

    let mut y = menu.y + 2.0;
    for (i, item) in menu.items.iter().enumerate() {
        match item {
            MenuItem::Action { label, enabled, shortcut, .. } => {
                let hover = menu.hovered == Some(i);
                if hover && *enabled {
                    push_rect(cmds, menu.x + 1.0, y, menu_w - 2.0, item_h,
                              pal.bg_context_menu_hover);
                }
                let txt_color = if *enabled { pal.text } else { pal.text_disabled };
                push_text(cmds, menu.x + 8.0, y + 4.0, label.clone(), txt_color, false);
                if let Some(sh) = shortcut {
                    push_text(cmds, menu.x + menu_w - 8.0 - (sh.len() as f32) * FONT_W,
                              y + 4.0, sh.clone(), pal.text_dim, false);
                }
                y += item_h;
            }
            MenuItem::Separator => {
                push_rect(cmds, menu.x + 6.0, y + 2.0, menu_w - 12.0, 1.0, pal.border);
                y += 6.0;
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

#[derive(Debug, Clone, Copy)]
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
    // Caret zone: x_indent - INDENT_PX..x_indent.
    let caret_x = 8.0 + row.depth as f32 * INDENT_PX - INDENT_PX;
    if let RowKind::Element { has_children: true, .. } = &row.kind {
        if mouse_x >= caret_x && mouse_x < caret_x + INDENT_PX {
            return DevtoolsHit::TreeCaret(row.node_id);
        }
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
    let split_x = 240.0;
    if mouse_x < split_x {
        let row_idx = ((mouse_y - (content_y + ROW_H + 8.0)) / ROW_H) as usize;
        if row_idx < state.sources.files.len() {
            return DevtoolsHit::SourcesFileRow(state.sources.files[row_idx].id);
        }
        return DevtoolsHit::PanelArea;
    }
    // Gutter klik = toggle breakpoint.
    let gutter_w = 50.0;
    if mouse_x < split_x + gutter_w {
        if let Some(file_id) = state.sources.selected_id {
            let scroll_y = state.sources.scroll_y;
            let line_idx = ((mouse_y - content_y + scroll_y) / ROW_H) as usize;
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
    _mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    let header_h = ROW_H + 4.0;
    if mouse_y < content_y + header_h { return DevtoolsHit::PanelArea; }
    let row_y = content_y + header_h + 2.0;
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
        font_family: String::new(),
        strikethrough: false, underline: false,
    });
}

fn push_text_bold(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], italic: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: true,
        italic,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });
}

fn push_text_italic(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], _bold: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: false,
        italic: true,
        font_family: String::new(),
        strikethrough: false, underline: false,
    });
}

fn push_text_underline(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], strikethrough: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: false,
        italic: false,
        font_family: String::new(),
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
