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
// Local shift helper - shift_command_x v render je pub(super).
fn shift_cmd_x(cmd: &mut DisplayCommand, dx: f32) {
    use DisplayCommand::*;
    match cmd {
        Rect { x, .. } => *x += dx,
        Text { x, .. } => *x += dx,
        Border { x, .. } => *x += dx,
        Image { x, .. } => *x += dx,
        Gradient { x, .. } => *x += dx,
        Shadow { x, .. } => *x += dx,
        _ => {}
    }
}
use crate::devtools::{DevToolsState, Tab};
use crate::devtools::theme::Palette;
use crate::devtools::model::elements::{ElementRow, RowKind};
use crate::devtools::model::console::LogLevel;
use crate::interpreter::Interpreter;

pub const ROW_H: f32 = 18.0;
pub const TAB_H: f32 = 30.0;
const TOOLBAR_BTN_H: f32 = 22.0;
const FONT_SIZE: f32 = 12.0;
/// Fallback advance kdyz fontdue load selze.
const FONT_W: f32 = 7.2;
pub const INDENT_PX: f32 = 16.0;
pub const RESIZE_GRIP_H: f32 = 4.0;
const SCROLLBAR_W: f32 = 10.0;
pub const SEARCH_H: f32 = 28.0;
const SPLITTER_HIT_PX: f32 = 6.0;
/// Custom font family pro vsechen DevTools text.
/// CamingoMono = code (selectors, declarations, tree tags, attr values, console).
/// Inter = sans-serif pro UI chrome (labels, headings, buttons).
const DT_FONT: &str = "CamingoMono";
const DT_FONT_BOLD: &str = "CamingoMono-Bold";
const DT_FONT_ITALIC: &str = "CamingoMono-Italic";
const DT_UI_FONT: &str = "Inter";
const DT_UI_FONT_BOLD: &str = "Inter-Bold";
const DT_UI_FONT_ITALIC: &str = "Inter-Italic";

/// Najdi byte offset v textu, jehoz x-pozice je nejblize `target_x`.
/// Pouziva se pro kliknuti mysi na text input - prevede pixel pos na cursor pos.
pub fn dt_byte_idx_at_x(text: &str, target_x: f32) -> usize {
    if target_x <= 0.0 { return 0; }
    let mut acc = 0.0f32;
    let mut last_byte = 0;
    for (byte_off, ch) in text.char_indices() {
        let w = dt_text_width(&ch.to_string());
        let mid = acc + w * 0.5;
        if target_x < mid { return byte_off; }
        acc += w;
        last_byte = byte_off + ch.len_utf8();
    }
    last_byte
}

/// Realna sirka textu v CamingoMono pri FONT_SIZE - musi pasovat na render side
/// (oba pouzivaji fontdue.metrics().advance_width).
pub fn dt_text_width(text: &str) -> f32 {
    use std::sync::OnceLock;
    static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    let f = FONT.get_or_init(|| {
        let candidates = [
            "static/fonts/CamingoMono-Light.ttf",
            "fonts/CamingoMono-Light.ttf",
        ];
        for p in candidates.iter() {
            if let Ok(d) = std::fs::read(p) {
                if let Ok(font) = fontdue::Font::from_bytes(d, fontdue::FontSettings::default()) {
                    return Some(font);
                }
            }
        }
        None
    });
    match f.as_ref() {
        Some(font) => text.chars().map(|c| font.metrics(c, FONT_SIZE).advance_width).sum(),
        None => text.chars().count() as f32 * FONT_W,
    }
}

/// Set active profile - volat z CLI parsing pred prvnim devtools state load.
pub fn set_profile(name: &str) {
    crate::devtools::profile::set_active_profile(name.to_string());
}

// ─── Material Symbols icons (Google Fonts, OFL) ─────────────────────────
//
// Codepoints (Outlined): chevron_right E5CC, expand_more E5CF, close E5CD,
// light_mode E518, dark_mode E51C, center_focus_strong E3B4.

const ICON_FONT: &str = "MaterialSymbolsOutlined";
const ICON_SIZE: f32 = 16.0;
const ICON_CHEVRON_RIGHT: char = '\u{E5CC}';
const ICON_EXPAND_MORE: char = '\u{E5CF}';
const ICON_CLOSE: char = '\u{E5CD}';
const ICON_LIGHT_MODE: char = '\u{E518}';
const ICON_DARK_MODE: char = '\u{E51C}';
const ICON_INSPECT: char = '\u{E3B4}';
const ICON_SETTINGS: char = '\u{E8B8}'; // settings gear

fn push_icon(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, ch: char, color: [u8; 4]) {
    cmds.push(DisplayCommand::Text {
        x, y, content: ch.to_string(), color,
        font_size: ICON_SIZE, bold: false, italic: false,
        font_family: ICON_FONT.into(),
        strikethrough: false, underline: false,
    });
}

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
    use crate::devtools::profile::DockPosition;
    let s = state.panel_h.min(match state.dock_position {
        DockPosition::Left | DockPosition::Right => win_w * 0.7,
        _ => win_h * 0.7,
    });
    // Panel rect dle dock position.
    let (panel_x, panel_y, panel_w, panel_h) = match state.dock_position {
        DockPosition::Bottom | DockPosition::PopupWindow =>
            (0.0, win_h - s, win_w, s),
        DockPosition::Top => (0.0, 0.0, win_w, s),
        DockPosition::Left => (0.0, 0.0, s, win_h),
        DockPosition::Right => (win_w - s, 0.0, s, win_h),
    };

    // Pozadi panelu.
    push_rect(cmds, panel_x, panel_y, panel_w, panel_h, pal.bg_panel);
    // Border separator (proti page) - na strane co dock dotyka page.
    match state.dock_position {
        DockPosition::Bottom => push_rect(cmds, panel_x, panel_y, panel_w, 1.0, pal.border_strong),
        DockPosition::Top => push_rect(cmds, panel_x, panel_y + panel_h - 1.0, panel_w, 1.0, pal.border_strong),
        DockPosition::Left => push_rect(cmds, panel_x + panel_w - 1.0, panel_y, 1.0, panel_h, pal.border_strong),
        DockPosition::Right => push_rect(cmds, panel_x, panel_y, 1.0, panel_h, pal.border_strong),
        DockPosition::PopupWindow => push_rect(cmds, panel_x, panel_y, panel_w, 1.0, pal.border_strong),
    }

    // Resize grip - draggable area na hranice dotyku page.
    let (grip_x, grip_y, grip_w, grip_h) = match state.dock_position {
        DockPosition::Bottom | DockPosition::PopupWindow =>
            (panel_x, panel_y, panel_w, RESIZE_GRIP_H),
        DockPosition::Top =>
            (panel_x, panel_y + panel_h - RESIZE_GRIP_H, panel_w, RESIZE_GRIP_H),
        DockPosition::Left =>
            (panel_x + panel_w - RESIZE_GRIP_H, panel_y, RESIZE_GRIP_H, panel_h),
        DockPosition::Right =>
            (panel_x, panel_y, RESIZE_GRIP_H, panel_h),
    };
    let grip_hover = mouse_x >= grip_x && mouse_x < grip_x + grip_w
        && mouse_y >= grip_y && mouse_y < grip_y + grip_h;
    let grip_color = if grip_hover { pal.accent } else { pal.border };
    push_rect(cmds, grip_x, grip_y, grip_w, grip_h, grip_color);

    // Toolbar (taby + akce). Pro vertical docks (Left/Right) vleze pod grip
    // jako horizontal strip nahore panelu.
    let toolbar_y = match state.dock_position {
        DockPosition::Bottom | DockPosition::PopupWindow => panel_y + RESIZE_GRIP_H,
        DockPosition::Top => panel_y,
        DockPosition::Left | DockPosition::Right => panel_y, // vertical dock - toolbar nahore
    };
    let toolbar_x = match state.dock_position {
        DockPosition::Right => panel_x + RESIZE_GRIP_H, // grip vlevo
        _ => panel_x,
    };
    let toolbar_w = match state.dock_position {
        DockPosition::Left => panel_w - RESIZE_GRIP_H, // grip vpravo
        DockPosition::Right => panel_w - RESIZE_GRIP_H, // grip vlevo
        _ => panel_w,
    };
    push_rect(cmds, toolbar_x, toolbar_y, toolbar_w, TAB_H, pal.bg_toolbar);
    push_rect(cmds, toolbar_x, toolbar_y + TAB_H - 1.0, toolbar_w, 1.0, pal.border);

    // Pri non-Bottom dock se obsah vykresli s panel_x offset pres translation
    // wrap. Existujici tab paint funkce predpokladaji x=0 origin -> emit do
    // local buffer + pak shift do panel coords.
    let needs_x_shift = panel_x.abs() > 0.5;
    let mut local_cmds: Vec<DisplayCommand> = if needs_x_shift { Vec::new() } else { Vec::new() };
    let target_cmds: &mut Vec<DisplayCommand> = if needs_x_shift { &mut local_cmds } else { cmds };

    paint_tabs(target_cmds, state, &pal, toolbar_y, toolbar_w, mouse_x - panel_x, mouse_y);
    paint_toolbar_actions(target_cmds, state, &pal, toolbar_y, toolbar_w, mouse_x - panel_x, mouse_y);

    // Content area.
    let content_y = toolbar_y + TAB_H;
    let content_h = match state.dock_position {
        DockPosition::Bottom | DockPosition::PopupWindow | DockPosition::Top =>
            panel_h - RESIZE_GRIP_H - TAB_H,
        DockPosition::Left | DockPosition::Right =>
            panel_h - TAB_H,
    };
    let content_w = toolbar_w;
    if content_h <= 0.0 {
        if needs_x_shift {
            for mut cmd in local_cmds {
                shift_cmd_x(&mut cmd, panel_x);
                cmds.push(cmd);
            }
        }
        return;
    }

    let m_x_local = mouse_x - panel_x;
    match state.tab {
        Tab::Elements => paint_elements_tab(target_cmds, layout_root, state, &pal, content_w, content_y, content_h, m_x_local, mouse_y),
        Tab::Console => paint_console_tab(target_cmds, state, &pal, interp, content_w, content_y, content_h, m_x_local, mouse_y),
        Tab::Network => paint_network_tab(target_cmds, state, &pal, interp, content_w, content_y, content_h, m_x_local, mouse_y),
        Tab::Sources => paint_sources_tab(target_cmds, state, &pal, content_w, content_y, content_h, m_x_local, mouse_y),
        Tab::Performance => paint_performance_tab(target_cmds, state, &pal, content_w, content_y, content_h),
        Tab::Application => paint_application_tab(target_cmds, state, &pal, interp, content_w, content_y, content_h),
        Tab::Settings => paint_settings_tab(target_cmds, state, &pal, content_w, content_y, content_h, m_x_local, mouse_y),
    }
    // Flush local buffer s x shift.
    if needs_x_shift {
        for mut cmd in local_cmds {
            shift_cmd_x(&mut cmd, panel_x);
            cmds.push(cmd);
        }
    }

    // Settings popup (dock chooser + theme).
    if state.settings_popup_open {
        paint_settings_popup(cmds, state, &pal, win_w, win_h, mouse_x, mouse_y);
    }
    // Class manager popup.
    if state.class_manager_open {
        paint_class_manager(cmds, state, layout_root, &pal, win_w, win_h, mouse_x, mouse_y);
    }
    // Color picker popup.
    if state.color_picker.is_some() {
        paint_color_picker(cmds, state, &pal);
    }
    // Context menu vykresli pres vsechno (z-order top).
    if let Some(menu) = &state.context_menu {
        paint_context_menu(cmds, &pal, menu, mouse_x, mouse_y);
    }
}

fn paint_class_manager(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    layout_root: &LayoutBox,
    pal: &Palette,
    win_w: f32,
    win_h: f32,
    _mouse_x: f32, _mouse_y: f32,
) {
    let pop_w = 280.0;
    let pop_h = 240.0;
    let px = (win_w - pop_w) * 0.5;
    let py = (win_h - pop_h) * 0.5;
    push_rect(cmds, 0.0, 0.0, win_w, win_h, [0, 0, 0, 100]);
    push_rect(cmds, px, py, pop_w, pop_h, pal.bg_panel);
    push_rect_border(cmds, px, py, pop_w, pop_h, pal.border_strong);
    push_rect(cmds, px, py, pop_w, 32.0, pal.bg_panel_alt);
    push_ui_text(cmds, px + 12.0, py + 8.0, "Tridy elementu".to_string(), pal.text, true);

    // Class list pro selected element.
    let mut sy = py + 44.0;
    if let Some(sel_id) = state.elements.selected {
        if let Some(bx) = find_layout_box(layout_root, sel_id) {
            let class_attr = bx.node.as_ref().and_then(|n|
                n.attributes.borrow().get("class").cloned()
            ).unwrap_or_default();
            if class_attr.is_empty() {
                push_ui_text_italic(cmds, px + 16.0, sy,
                                    "Element nema tridy".to_string(), pal.text_dim);
            } else {
                push_ui_text(cmds, px + 16.0, sy, "Aktivni tridy:".to_string(), pal.text_dim, false);
                sy += ROW_H + 4.0;
                for cls in class_attr.split_whitespace() {
                    // Checkbox + class name.
                    push_rect_border(cmds, px + 16.0, sy + 2.0, 12.0, 12.0, pal.border);
                    push_rect(cmds, px + 18.0, sy + 4.0, 8.0, 8.0, pal.accent);
                    push_text(cmds, px + 36.0, sy, format!(".{}", cls), pal.syn_attr, false);
                    sy += ROW_H;
                }
            }
        }
    }
    // Hint.
    push_ui_text_italic(cmds, px + 16.0, py + pop_h - 24.0,
                        "Klik mimo zavre".to_string(), pal.text_disabled);
}

fn paint_settings_popup(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    win_w: f32,
    win_h: f32,
    mouse_x: f32, mouse_y: f32,
) {
    use crate::devtools::profile::DockPosition;
    let pop_w = 320.0;
    let pop_h = 280.0;
    let px = (win_w - pop_w) * 0.5;
    let py = (win_h - pop_h) * 0.5;
    // Backdrop (semi-transparent).
    push_rect(cmds, 0.0, 0.0, win_w, win_h, [0, 0, 0, 100]);
    // Popup card.
    push_rect(cmds, px, py, pop_w, pop_h, pal.bg_panel);
    push_rect_border(cmds, px, py, pop_w, pop_h, pal.border_strong);
    // Header.
    push_rect(cmds, px, py, pop_w, 32.0, pal.bg_panel_alt);
    push_ui_text(cmds, px + 12.0, py + 8.0, "Nastaveni DevTools".to_string(), pal.text, true);
    // Close X vpravo.
    let close_x = px + pop_w - 28.0;
    let close_hover = mouse_x >= close_x && mouse_x < close_x + 24.0
                      && mouse_y >= py + 4.0 && mouse_y < py + 28.0;
    if close_hover {
        push_rect(cmds, close_x, py + 4.0, 24.0, 24.0, pal.bg_row_hover);
    }
    push_icon(cmds, close_x + 4.0, py + 8.0, ICON_CLOSE, pal.text);

    // Section: Pozice docku.
    let mut sy = py + 44.0;
    push_ui_text(cmds, px + 16.0, sy, "Pozice docku".to_string(), pal.text_dim, true);
    sy += ROW_H + 4.0;
    for pos in DockPosition::all() {
        let active = state.dock_position == *pos;
        let row_y = sy;
        let row_hov = mouse_x >= px && mouse_x < px + pop_w
                      && mouse_y >= row_y && mouse_y < row_y + ROW_H + 2.0;
        if row_hov {
            push_rect(cmds, px + 8.0, row_y, pop_w - 16.0, ROW_H + 2.0, pal.bg_row_hover);
        }
        // Radio dot.
        let dot_x = px + 16.0;
        let dot_y = row_y + 5.0;
        push_rect_border(cmds, dot_x, dot_y, 10.0, 10.0, pal.border);
        if active {
            push_rect(cmds, dot_x + 2.0, dot_y + 2.0, 6.0, 6.0, pal.accent);
        }
        push_ui_text(cmds, px + 36.0, row_y + 2.0, pos.label().to_string(), pal.text, false);
        sy += ROW_H + 2.0;
    }

    // Section: Profil.
    sy += 8.0;
    push_ui_text(cmds, px + 16.0, sy, format!("Profil: {}", crate::devtools::profile::active_profile()),
                 pal.text_dim, true);
}

/// HSV trojuhelnik / box + hue slider color picker. Klasicky CSS color
/// editor: SV box (gradient) + hue slider + RGB/HEX inputs.
fn paint_color_picker(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
) {
    let Some(cp) = &state.color_picker else { return };
    let pop_w = 240.0;
    let pop_h = 220.0;
    let px = cp.anchor_x;
    let py = cp.anchor_y;
    push_rect(cmds, px, py, pop_w, pop_h, pal.bg_panel);
    push_rect_border(cmds, px, py, pop_w, pop_h, pal.border_strong);

    // SV gradient box (placeholder - solid square s aktualni barvou).
    let sv_x = px + 10.0;
    let sv_y = py + 10.0;
    let sv_w = pop_w - 20.0;
    let sv_h = 120.0;
    // Aktualne emit jen solid color - real gradient = vyzaduje multi-color shader.
    push_rect(cmds, sv_x, sv_y, sv_w, sv_h, cp.rgba);
    push_rect_border(cmds, sv_x, sv_y, sv_w, sv_h, pal.border);

    // Hue slider (proste 6-segment rainbow approximation).
    let hue_y = sv_y + sv_h + 8.0;
    let hue_h = 12.0;
    let segments = [
        [255, 0, 0, 255], [255, 255, 0, 255], [0, 255, 0, 255],
        [0, 255, 255, 255], [0, 0, 255, 255], [255, 0, 255, 255], [255, 0, 0, 255],
    ];
    let seg_w = sv_w / 6.0;
    for i in 0..6 {
        push_rect(cmds, sv_x + (i as f32) * seg_w, hue_y, seg_w, hue_h, segments[i]);
    }
    // Hue marker.
    let mx = sv_x + (cp.hue / 360.0) * sv_w;
    push_rect(cmds, mx - 1.0, hue_y - 2.0, 3.0, hue_h + 4.0, [255, 255, 255, 255]);
    push_rect_border(cmds, mx - 1.0, hue_y - 2.0, 3.0, hue_h + 4.0, [0, 0, 0, 255]);

    // HEX label + value.
    let info_y = hue_y + hue_h + 12.0;
    let hex = format!("#{:02x}{:02x}{:02x}", cp.rgba[0], cp.rgba[1], cp.rgba[2]);
    push_ui_text(cmds, sv_x, info_y, "HEX:".to_string(), pal.text_dim, true);
    push_text(cmds, sv_x + 36.0, info_y, hex, pal.text, false);
    push_ui_text(cmds, sv_x, info_y + 18.0,
                 format!("RGB: {} {} {}", cp.rgba[0], cp.rgba[1], cp.rgba[2]),
                 pal.text, false);
    push_ui_text(cmds, sv_x, info_y + 36.0,
                 "Klik mimo zavre".to_string(), pal.text_disabled, false);
}

/// Hit-test pro settings popup. Vraci akci nebo None.
pub enum SettingsPopupAction {
    SelectDock(crate::devtools::profile::DockPosition),
    Close,
    Dismiss,
}

pub fn settings_popup_hit(
    state: &DevToolsState, win_w: f32, win_h: f32, mouse_x: f32, mouse_y: f32,
) -> Option<SettingsPopupAction> {
    use crate::devtools::profile::DockPosition;
    if !state.settings_popup_open { return None; }
    let pop_w = 320.0;
    let pop_h = 280.0;
    let px = (win_w - pop_w) * 0.5;
    let py = (win_h - pop_h) * 0.5;
    // Outside popup -> dismiss.
    if mouse_x < px || mouse_x >= px + pop_w || mouse_y < py || mouse_y >= py + pop_h {
        return Some(SettingsPopupAction::Dismiss);
    }
    // Close X.
    let close_x = px + pop_w - 28.0;
    if mouse_x >= close_x && mouse_x < close_x + 24.0
       && mouse_y >= py + 4.0 && mouse_y < py + 28.0 {
        return Some(SettingsPopupAction::Close);
    }
    // Dock options.
    let mut sy = py + 44.0 + ROW_H + 4.0;
    for pos in DockPosition::all() {
        if mouse_y >= sy && mouse_y < sy + ROW_H + 2.0 {
            return Some(SettingsPopupAction::SelectDock(*pos));
        }
        sy += ROW_H + 2.0;
    }
    None
}

// ─── Tabs ────────────────────────────────────────────────────────────────

/// Tab strip layout: vraci (visible_tabs, overflow_tabs). Pri uzkem okne se
/// posledni taby presunuji do overflow ▼ menu.
pub fn compute_tab_layout(win_w: f32) -> (Vec<Tab>, Vec<Tab>) {
    let max_x = toolbar_actions_x(win_w) - 100.0; // rezerva pro Inspect/Theme/Close
    let mut visible = Vec::new();
    let mut overflow = Vec::new();
    let mut x = 8.0;
    let overflow_btn_w = 22.0;
    for t in Tab::all() {
        let w = (t.label().len() as f32) * FONT_W + 18.0;
        if x + w + overflow_btn_w + 2.0 <= max_x {
            visible.push(*t);
            x += w + 2.0;
        } else {
            overflow.push(*t);
        }
    }
    (visible, overflow)
}

fn tab_rect_in_visible(visible: &[Tab], idx: usize, toolbar_y: f32) -> (f32, f32, f32, f32) {
    let mut x = 8.0;
    for (i, t) in visible.iter().enumerate() {
        let w = (t.label().len() as f32) * FONT_W + 18.0;
        if i == idx {
            return (x, toolbar_y + 4.0, w, TAB_H - 4.0);
        }
        x += w + 2.0;
    }
    (x, toolbar_y + 4.0, 60.0, TAB_H - 4.0)
}

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

/// Pozice ▼ overflow buttonu - hned za poslednim visible tabom.
pub fn tab_overflow_btn_rect(win_w: f32, toolbar_y: f32) -> (f32, f32, f32, f32) {
    let (visible, _) = compute_tab_layout(win_w);
    let mut x = 8.0;
    for t in &visible {
        x += (t.label().len() as f32) * FONT_W + 18.0 + 2.0;
    }
    (x, toolbar_y + 4.0, 22.0, TAB_H - 4.0)
}

fn paint_tabs(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    toolbar_y: f32,
    win_w: f32,
    mouse_x: f32, mouse_y: f32,
) {
    let (visible, overflow) = compute_tab_layout(win_w);
    for (i, t) in visible.iter().enumerate() {
        let (x, y, w, h) = tab_rect_in_visible(&visible, i, toolbar_y);
        let active = state.tab == *t;
        let hovered = mouse_x >= x && mouse_x < x + w && mouse_y >= y && mouse_y < y + h;
        let bg = if active { pal.bg_tab_active }
                 else if hovered { pal.bg_row_hover }
                 else { pal.bg_toolbar };
        push_rect(cmds, x, y, w, h, bg);
        if active {
            push_rect(cmds, x, y + h - 2.0, w, 2.0, pal.accent);
        }
        let txt_color = if active { pal.text } else { pal.text_dim };
        push_ui_text(cmds, x + 9.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
                     t.label().to_string(), txt_color, active);
    }
    // Overflow ▼ button + popup menu.
    if !overflow.is_empty() {
        let (bx, by, bw, bh) = tab_overflow_btn_rect(win_w, toolbar_y);
        let hov = mouse_x >= bx && mouse_x < bx + bw && mouse_y >= by && mouse_y < by + bh;
        let bg = if state.tab_overflow_open { pal.bg_tab_active }
                 else if hov { pal.bg_row_hover }
                 else { pal.bg_toolbar };
        push_rect(cmds, bx, by, bw, bh, bg);
        // Material expand_more icon.
        push_icon(cmds, bx + 3.0, by + (bh - ICON_SIZE) * 0.5, ICON_EXPAND_MORE, pal.text);

        if state.tab_overflow_open {
            // Popup menu pod ▼ button.
            let item_h = ROW_H + 4.0;
            let menu_w = 180.0;
            let menu_x = bx + bw - menu_w;
            let menu_y = by + bh + 2.0;
            let menu_h = (overflow.len() as f32) * item_h + 4.0;
            push_rect(cmds, menu_x, menu_y, menu_w, menu_h, pal.bg_context_menu);
            push_rect_border(cmds, menu_x, menu_y, menu_w, menu_h, pal.border);
            for (i, t) in overflow.iter().enumerate() {
                let iy = menu_y + 2.0 + (i as f32) * item_h;
                let ihov = mouse_x >= menu_x && mouse_x < menu_x + menu_w
                       && mouse_y >= iy && mouse_y < iy + item_h;
                if ihov {
                    push_rect(cmds, menu_x + 1.0, iy, menu_w - 2.0, item_h, pal.bg_context_menu_hover);
                }
                push_text(cmds, menu_x + 12.0, iy + 4.0, t.label().to_string(), pal.text, false);
            }
        }
    }
}

/// Hit-test pro overflow popup item. Vraci index v overflow listu.
pub fn tab_overflow_popup_hit(state: &DevToolsState, win_w: f32, toolbar_y: f32, mouse_x: f32, mouse_y: f32) -> Option<usize> {
    if !state.tab_overflow_open { return None; }
    let (_, overflow) = compute_tab_layout(win_w);
    if overflow.is_empty() { return None; }
    let (bx, by, bw, bh) = tab_overflow_btn_rect(win_w, toolbar_y);
    let item_h = ROW_H + 4.0;
    let menu_w = 180.0;
    let menu_x = bx + bw - menu_w;
    let menu_y = by + bh + 2.0;
    if mouse_x < menu_x || mouse_x >= menu_x + menu_w { return None; }
    let i = ((mouse_y - menu_y - 2.0) / item_h) as i32;
    if i < 0 || i as usize >= overflow.len() { return None; }
    Some(i as usize)
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
    push_icon(cmds, x_right + 4.0, y + (h - ICON_SIZE) * 0.5, ICON_CLOSE, pal.text);

    // Settings gear (otevre dock chooser popup).
    let set_w = 24.0;
    x_right -= set_w + 4.0;
    let set_hover = mouse_x >= x_right && mouse_x < x_right + set_w
                    && mouse_y >= y && mouse_y < y + h;
    let set_bg = if state.settings_popup_open { pal.bg_tab_active }
                 else if set_hover { pal.bg_row_hover }
                 else { pal.bg_toolbar };
    push_rect(cmds, x_right, y, set_w, h, set_bg);
    push_icon(cmds, x_right + 4.0, y + (h - ICON_SIZE) * 0.5, ICON_SETTINGS, pal.text);

    // Theme dot (Ctrl+Shift+T toggle): sun/moon icon.
    let theme_w = 24.0;
    x_right -= theme_w + 4.0;
    let theme_hover = mouse_x >= x_right && mouse_x < x_right + theme_w
                      && mouse_y >= y && mouse_y < y + h;
    let theme_bg = if theme_hover { pal.bg_row_hover } else { pal.bg_toolbar };
    push_rect(cmds, x_right, y, theme_w, h, theme_bg);
    let theme_icon = if pal.is_dark { ICON_DARK_MODE } else { ICON_LIGHT_MODE };
    push_icon(cmds, x_right + 4.0, y + (h - ICON_SIZE) * 0.5, theme_icon, pal.text);

    // Inspect toggle: icon + label.
    let insp_w = 90.0;
    x_right -= insp_w + 4.0;
    let insp_hover = mouse_x >= x_right && mouse_x < x_right + insp_w
                     && mouse_y >= y && mouse_y < y + h;
    let bg = if state.inspect_mode { pal.accent }
             else if insp_hover { pal.bg_row_hover }
             else { pal.bg_button };
    push_rect(cmds, x_right, y, insp_w, h, bg);
    let txt = if state.inspect_mode { pal.text_on_accent } else { pal.text };
    push_icon(cmds, x_right + 4.0, y + (h - ICON_SIZE) * 0.5, ICON_INSPECT, txt);
    push_ui_text(cmds, x_right + 24.0, y + (h - FONT_SIZE) * 0.5 + 1.0,
                 "Inspect".to_string(), txt, false);
}

/// Styles pane top toolbar (filter bar): :hov / .cls / + state buttons.
pub fn paint_styles_toolbar(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32,
    _mouse_x: f32, _mouse_y: f32,
) {
    let h = 26.0;
    push_rect(cmds, x, y, w, h, pal.bg_panel_alt);
    push_rect(cmds, x, y + h - 1.0, w, 1.0, pal.border);
    let input_x = x + 8.0;
    let input_w = w * 0.5;
    push_rect(cmds, input_x, y + 4.0, input_w, h - 8.0, pal.bg_input);
    push_rect_border(cmds, input_x, y + 4.0, input_w, h - 8.0, pal.border);
    push_ui_text_italic(cmds, input_x + 6.0, y + 7.0,
                        "Filtr stylu".to_string(), pal.text_disabled);

    // :hov, .cls, + buttons vpravo s active state highlight.
    let mut bx_x = x + w - 8.0;
    let active_states = [
        (":hov", state.force_hover || state.force_focus || state.force_active),
        (".cls", state.class_manager_open),
        ("+", false),
    ];
    for (label, active) in active_states.iter().rev() {
        let bw = (label.len() as f32) * FONT_W + 8.0;
        bx_x -= bw + 4.0;
        let bg = if *active { pal.accent } else { pal.bg_button };
        let txt = if *active { pal.text_on_accent } else { pal.text_dim };
        push_rect(cmds, bx_x, y + 4.0, bw, h - 8.0, bg);
        push_ui_text(cmds, bx_x + 4.0, y + 7.0, label.to_string(), txt, false);
    }
}

/// Hit-test pro :hov/.cls/+ buttons v styles toolbar. Vraci index 0/1/2 nebo None.
pub fn styles_toolbar_btn_hit(
    x: f32, y: f32, w: f32, mouse_x: f32, mouse_y: f32,
) -> Option<usize> {
    let h = 26.0;
    if mouse_y < y || mouse_y >= y + h { return None; }
    let mut bx_x = x + w - 8.0;
    for (idx_rev, label) in ["+", ".cls", ":hov"].iter().enumerate() {
        let bw = (label.len() as f32) * FONT_W + 8.0;
        bx_x -= bw + 4.0;
        if mouse_x >= bx_x && mouse_x < bx_x + bw {
            return Some(2 - idx_rev); // 0=hov, 1=cls, 2=+
        }
    }
    None
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

    // Three-column layout: tree | styles | side panel.
    // split_x = tree | styles boundary; styles_end = win_w - side_panel_w.
    let default_tree_split = (win_w - state.side_panel_w) * 0.45;
    let tree_split = if state.elements.split_x < 1.0 { default_tree_split }
                     else { state.elements.split_x.max(200.0).min(win_w - state.side_panel_w - 200.0) };
    let side_panel_w = state.side_panel_w.clamp(180.0, win_w - 400.0);
    let styles_end = win_w - side_panel_w;

    // Z-order:
    // 1. Tree bg (left)
    // 2. Tree rows (clipped na tree_split)
    // 3. Styles bg + splitter
    // 4. Styles content
    // 5. Side panel bg + splitter
    // 6. Side panel content

    push_rect(cmds, 0.0, body_y, tree_split, body_h, pal.bg_panel);

    // Render rows visible v scroll window.
    let rows = &state.elements.rows;
    let total_h = rows.len() as f32 * ROW_H;
    let scroll_y = state.elements.scroll_y.min((total_h - body_h).max(0.0)).max(0.0);
    let visible_rows = ((body_h / ROW_H).ceil() as usize) + 1;
    let start_row = (scroll_y / ROW_H) as usize;

    for (visual_idx, row_idx) in (start_row..rows.len().min(start_row + visible_rows)).enumerate() {
        let row = &rows[row_idx];
        let y = body_y + (visual_idx as f32) * ROW_H - (scroll_y % ROW_H);
        if y < body_y || y + ROW_H > body_y + body_h { continue; }
        paint_element_row(cmds, row, state, pal, tree_split - SCROLLBAR_W - 4.0, y, mouse_x, mouse_y);
    }

    if total_h > body_h {
        paint_vertical_scrollbar(cmds, pal, tree_split - SCROLLBAR_W, body_y,
                                  body_h, total_h, scroll_y);
    }

    // Styles pane (middle).
    push_rect(cmds, tree_split, body_y, styles_end - tree_split, body_h, pal.bg_panel_alt);
    push_rect(cmds, tree_split, body_y, 2.0, body_h, pal.border_strong);

    if let Some(sel_id) = state.elements.selected {
        if let Some(bx) = find_layout_box(layout_root, sel_id) {
            paint_styles_pane(cmds, bx, state, pal, tree_split + 2.0, body_y, styles_end - tree_split - 2.0, body_h);
        }
    } else {
        push_text(cmds, tree_split + 12.0, body_y + 12.0,
                  "Vyberte element pro zobrazeni stylu".to_string(),
                  pal.text_dim, false);
    }

    // Side panel (right column).
    push_rect(cmds, styles_end, body_y, side_panel_w, body_h, pal.bg_panel);
    push_rect(cmds, styles_end, body_y, 2.0, body_h, pal.border_strong);
    paint_side_panel(cmds, layout_root, state, pal, styles_end + 2.0, body_y, side_panel_w - 2.0, body_h, mouse_x, mouse_y);
}

// ─── Side panel (Layout/Computed/Animations/Fonts/...) ──────────────────

fn paint_side_panel(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, h: f32,
    mouse_x: f32, mouse_y: f32,
) {
    use crate::devtools::SidePanelTab;
    // Sub-tab strip nahore.
    let strip_h = TAB_H;
    push_rect(cmds, x, y, w, strip_h, pal.bg_toolbar);
    push_rect(cmds, x, y + strip_h - 1.0, w, 1.0, pal.border);
    let visible = SidePanelTab::visible_default();
    let mut tx = x + 8.0;
    for t in visible {
        let label = t.label();
        let tw = (label.len() as f32) * FONT_W + 12.0;
        let active = state.side_panel_tab == *t;
        let hov = mouse_x >= tx && mouse_x < tx + tw && mouse_y >= y + 4.0 && mouse_y < y + strip_h - 4.0;
        let bg = if active { pal.bg_tab_active }
                 else if hov { pal.bg_row_hover }
                 else { pal.bg_toolbar };
        push_rect(cmds, tx, y + 4.0, tw, strip_h - 8.0, bg);
        if active {
            push_rect(cmds, tx, y + strip_h - 4.0, tw, 2.0, pal.accent);
        }
        let col = if active { pal.text } else { pal.text_dim };
        push_ui_text(cmds, tx + 6.0, y + 8.0, label.to_string(), col, active);
        tx += tw + 2.0;
    }
    let body_y = y + strip_h;
    let body_h = h - strip_h;
    push_rect(cmds, x, body_y, w, body_h, pal.bg_panel);

    // Body content per tab.
    if let Some(sel_id) = state.elements.selected {
        if let Some(bx) = find_layout_box(layout_root, sel_id) {
            match state.side_panel_tab {
                SidePanelTab::Layout => paint_side_layout(cmds, bx, state, pal, x, body_y, w, body_h),
                SidePanelTab::Computed => paint_side_computed(cmds, state, pal, x, body_y, w, body_h),
                SidePanelTab::Changes => paint_side_changes(cmds, state, pal, x, body_y, w, body_h),
                SidePanelTab::Compatibility => paint_side_compat(cmds, state, pal, x, body_y, w, body_h),
                SidePanelTab::Fonts => paint_side_fonts(cmds, bx, state, pal, x, body_y, w, body_h),
                SidePanelTab::Animations => paint_side_animations(cmds, bx, state, pal, x, body_y, w, body_h),
            }
        }
    } else {
        push_text(cmds, x + 12.0, body_y + 12.0,
                  "Vyberte element".to_string(), pal.text_dim, false);
    }
}

/// Helper - vykresli collapsible section header. Vrati novy y (po headeru).
fn paint_section_header(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    id: &SectionId,
    title: &str,
    x: f32, y: f32, w: f32,
) -> f32 {
    let collapsed = state.collapsed_sections.contains(id);
    let icon = if collapsed { ICON_CHEVRON_RIGHT } else { ICON_EXPAND_MORE };
    push_rect(cmds, x, y, w, ROW_H + 4.0, pal.bg_panel_alt);
    push_icon(cmds, x + 4.0, y + (ROW_H - ICON_SIZE) * 0.5, icon, pal.text);
    push_text(cmds, x + 22.0, y + 4.0, title.to_string(), pal.text, true);
    y + ROW_H + 4.0
}

/// Parse "1.5s" / "200ms" -> seconds.
fn parse_duration(s: &str) -> f32 {
    let s = s.trim();
    if let Some(num_str) = s.strip_suffix("ms") {
        num_str.parse::<f32>().unwrap_or(0.0) / 1000.0
    } else if let Some(num_str) = s.strip_suffix('s') {
        num_str.parse::<f32>().unwrap_or(0.0)
    } else {
        s.parse::<f32>().unwrap_or(1.0)
    }
}

/// Flex item diagram (Firefox-style): basis -> final size visualization.
/// Pri growth: ukaz basis (modry rect) + extra grow (zelena pruh napravo).
/// Pri shrink: ukaz basis (modry rect, sirsi) + cervena indikace ktera
/// cast byla ubrana.
fn paint_flex_item_diagram(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    pal: &Palette,
    x: f32, y: f32, w: f32,
) -> f32 {
    let _final_size = bx.rect.width.max(bx.rect.height);
    // Diagram bounds.
    let dx = x + 8.0;
    let dy = y + 8.0;
    let dw = (w - 16.0).max(100.0);
    let dh = 60.0;
    // Bg.
    push_rect(cmds, dx, dy, dw, dh, pal.bg_panel_alt);
    push_rect_border(cmds, dx, dy, dw, dh, pal.border);

    // Basis (modry rect uvnitr).
    let basis_str = if bx.flex_basis.is_empty() || bx.flex_basis == "auto" {
        "auto".to_string()
    } else {
        bx.flex_basis.clone()
    };
    let basis_w = (dw * 0.6).min(dw - 4.0);
    push_rect(cmds, dx + 2.0, dy + 4.0, basis_w, dh - 8.0, [69, 161, 255, 180]);
    push_ui_text(cmds, dx + 8.0, dy + dh - 22.0, "basis".to_string(), pal.text_dim, false);
    push_ui_text(cmds, dx + 8.0, dy + dh - 8.0, basis_str, [255, 255, 255, 255], true);

    // Growth zona (zelena pruh za basis).
    if bx.flex_grow > 0.0 {
        let grow_w = (dw - basis_w - 4.0).max(0.0);
        if grow_w > 1.0 {
            push_rect(cmds, dx + basis_w + 2.0, dy + 4.0, grow_w, dh - 8.0, [148, 222, 124, 180]);
            push_ui_text(cmds, dx + basis_w + 8.0, dy + dh - 22.0,
                         format!("grow {}", bx.flex_grow), pal.text_dim, false);
        }
    }

    // Final size info.
    push_ui_text(cmds, x, dy + dh + 8.0,
                 format!("Vysledna velikost: {:.0} x {:.0} px",
                         bx.rect.width, bx.rect.height),
                 pal.text, true);

    y + dh + 32.0
}

/// Firefox-style nested box model viz: margin (oranzova) -> border (zluta)
/// -> padding (zelena) -> content (modra), s rozmery na kazde strane.
fn paint_box_model_viz(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    pal: &Palette,
    x: f32, y: f32, w: f32,
) -> f32 {
    let m_t = bx.margin_top.unwrap_or(bx.margin);
    let m_r = bx.margin_right.unwrap_or(bx.margin);
    let m_b = bx.margin_bottom.unwrap_or(bx.margin);
    let m_l = bx.margin_left.unwrap_or(bx.margin);
    let p_t = bx.padding_top.unwrap_or(bx.padding);
    let p_r = bx.padding_right.unwrap_or(bx.padding);
    let p_b = bx.padding_bottom.unwrap_or(bx.padding);
    let p_l = bx.padding_left.unwrap_or(bx.padding);
    let bw = bx.border_width;

    // Box dimensions (visual nested box).
    let box_w = w.min(280.0);
    let box_h = 180.0;
    let cx = x + (w - box_w) * 0.5;
    let cy = y;

    // Margin layer (orange).
    push_rect(cmds, cx, cy, box_w, box_h, pal.overlay_margin);
    let inset = 24.0;
    // Border layer (yellow).
    push_rect(cmds, cx + inset, cy + inset, box_w - 2.0 * inset, box_h - 2.0 * inset, pal.overlay_border);
    // Padding layer (green).
    let inset2 = 48.0;
    push_rect(cmds, cx + inset2, cy + inset2, box_w - 2.0 * inset2, box_h - 2.0 * inset2, pal.overlay_padding);
    // Content layer (blue).
    let inset3 = 72.0;
    push_rect(cmds, cx + inset3, cy + inset3, box_w - 2.0 * inset3, box_h - 2.0 * inset3, pal.overlay_content);

    // Labels per layer (text uvnitr ramecku).
    let lbl_color = pal.text;
    // Margin labels (4 sides).
    push_ui_text(cmds, cx + box_w * 0.5 - 8.0, cy + 4.0, format!("{:.0}", m_t), lbl_color, false);
    push_ui_text(cmds, cx + box_w * 0.5 - 8.0, cy + box_h - 18.0, format!("{:.0}", m_b), lbl_color, false);
    push_ui_text(cmds, cx + 4.0, cy + box_h * 0.5 - 7.0, format!("{:.0}", m_l), lbl_color, false);
    push_ui_text(cmds, cx + box_w - 24.0, cy + box_h * 0.5 - 7.0, format!("{:.0}", m_r), lbl_color, false);
    // Border (single value).
    push_ui_text(cmds, cx + box_w * 0.5 - 8.0, cy + inset + 2.0, format!("{:.0}", bw), lbl_color, false);
    // Padding labels.
    push_ui_text(cmds, cx + box_w * 0.5 - 8.0, cy + inset2 + 2.0, format!("{:.0}", p_t), lbl_color, false);
    push_ui_text(cmds, cx + box_w * 0.5 - 8.0, cy + box_h - inset2 - 16.0, format!("{:.0}", p_b), lbl_color, false);
    push_ui_text(cmds, cx + inset2 + 2.0, cy + box_h * 0.5 - 7.0, format!("{:.0}", p_l), lbl_color, false);
    push_ui_text(cmds, cx + box_w - inset2 - 22.0, cy + box_h * 0.5 - 7.0, format!("{:.0}", p_r), lbl_color, false);
    // Content size (center).
    push_ui_text(cmds, cx + box_w * 0.5 - 30.0, cy + box_h * 0.5 - 7.0,
                 format!("{:.0} x {:.0}", bx.rect.width, bx.rect.height), lbl_color, true);
    // Section labels (rohy).
    push_ui_text(cmds, cx + 2.0, cy + box_h - 16.0, "margin".to_string(), pal.text_dim, false);
    push_ui_text(cmds, cx + inset + 2.0, cy + box_h - inset - 16.0, "border".to_string(), pal.text_dim, false);
    push_ui_text(cmds, cx + inset2 + 2.0, cy + box_h - inset2 - 16.0, "padding".to_string(), pal.text_dim, false);

    y + box_h + 12.0
}

fn paint_side_layout(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, _h: f32,
) {
    let mut sy = y + 4.0;
    let pad_x = x + 12.0;

    let node_id = bx.node.as_ref().map(|n| std::rc::Rc::as_ptr(n) as usize).unwrap_or(0);
    use crate::devtools::OverlayKind;

    // Pri vybranem flex item: diagram s basis/final/grow/shrink.
    // Detect: parent je flex container (lookup via node.parent if exists).
    let is_flex_item = bx.node.as_ref().and_then(|n| n.parent.borrow().upgrade())
        .map(|p| {
            // Naivni detect: parent ma display: flex z attrs nebo .style.
            // Pro real check by potreba parent LayoutBox - skip pro ted.
            let _ = p;
            true
        }).unwrap_or(false);
    if is_flex_item && (bx.flex_grow > 0.0 || bx.flex_shrink != 1.0 || !bx.flex_basis.is_empty()) {
        sy = paint_section_header(cmds, state, pal, &SectionId::LayoutFlex,
                                   "Polozka flex z rodice", x, sy, w);
        if !state.collapsed_sections.contains(&SectionId::LayoutFlex) {
            sy = paint_flex_item_diagram(cmds, bx, pal, x + 8.0, sy + 4.0, w - 16.0);
        }
    }

    // Flex container section.
    if matches!(bx.display, crate::browser::layout::Display::Flex) {
        sy = paint_section_header(cmds, state, pal, &SectionId::LayoutFlex,
                                   "Flex container", x, sy, w);
        if !state.collapsed_sections.contains(&SectionId::LayoutFlex) {
            // Overlay toggle indicator: kruhove oznaceni + label.
            let active = state.overlays.iter().any(|o| o.node_id == node_id && o.kind == OverlayKind::Flex);
            let dot_col = if active { pal.accent } else { pal.text_dim };
            push_rect(cmds, pad_x, sy + 4.0, 8.0, 8.0, dot_col);
            push_text(cmds, pad_x + 14.0, sy,
                      format!("Overlay: {}", if active { "ZAP" } else { "VYP" }),
                      pal.text, false);
            sy += ROW_H;
            push_text(cmds, pad_x, sy, format!("flex-direction: {}", bx.flex_direction),
                      pal.text_dim, false);
            sy += ROW_H;
            push_text(cmds, pad_x, sy, format!("flex-wrap: {}", bx.flex_wrap),
                      pal.text_dim, false);
            sy += ROW_H;
            push_text(cmds, pad_x, sy, format!("gap: {:.0} {:.0}", bx.row_gap, bx.column_gap),
                      pal.text_dim, false);
            sy += ROW_H + 4.0;
        }
    }

    // Grid container section.
    if matches!(bx.display, crate::browser::layout::Display::Grid) {
        sy = paint_section_header(cmds, state, pal, &SectionId::LayoutGrid,
                                   "Grid container", x, sy, w);
        if !state.collapsed_sections.contains(&SectionId::LayoutGrid) {
            let active = state.overlays.iter().any(|o| o.node_id == node_id && o.kind == OverlayKind::Grid);
            let dot_col = if active { pal.accent } else { pal.text_dim };
            push_rect(cmds, pad_x, sy + 4.0, 8.0, 8.0, dot_col);
            push_text(cmds, pad_x + 14.0, sy,
                      format!("Overlay: {}", if active { "ZAP" } else { "VYP" }),
                      pal.text, false);
            sy += ROW_H;
            // Grid template tracks.
            if !bx.grid_template_columns.is_empty() {
                push_text(cmds, pad_x, sy,
                          format!("columns: {}", bx.grid_template_columns),
                          pal.text_dim, false);
                sy += ROW_H;
            }
            if !bx.grid_template_rows.is_empty() {
                push_text(cmds, pad_x, sy,
                          format!("rows: {}", bx.grid_template_rows),
                          pal.text_dim, false);
                sy += ROW_H;
            }
            if !bx.grid_template_areas.is_empty() {
                push_text(cmds, pad_x, sy,
                          format!("areas: {}", bx.grid_template_areas.replace('\n', " | ")),
                          pal.text_dim, false);
                sy += ROW_H;
            }
            push_text(cmds, pad_x, sy, format!("gap: {:.0} {:.0}", bx.row_gap, bx.column_gap),
                      pal.text_dim, false);
            sy += ROW_H + 4.0;
        }
    }

    // Box model section.
    sy = paint_section_header(cmds, state, pal, &SectionId::LayoutBoxModel,
                               "Model boxu", x, sy, w);
    if !state.collapsed_sections.contains(&SectionId::LayoutBoxModel) {
        sy = paint_box_model_viz(cmds, bx, pal, x + 8.0, sy + 4.0, w - 16.0);
    }

    // Box properties section.
    sy = paint_section_header(cmds, state, pal, &SectionId::LayoutBoxProps,
                               "Vlastnosti box modelu", x, sy, w);
    if !state.collapsed_sections.contains(&SectionId::LayoutBoxProps) {
        push_text(cmds, pad_x, sy, format!("display: {:?}", bx.display), pal.text, false);
        sy += ROW_H;
        push_text(cmds, pad_x, sy, format!("position: {:?}", bx.position), pal.text, false);
        sy += ROW_H;
        push_text(cmds, pad_x, sy, format!("line-height: {:.2}", bx.line_height), pal.text, false);
    }
}

fn paint_side_computed(
    cmds: &mut Vec<DisplayCommand>,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, h: f32,
) {
    // Filter input bar nahore.
    let filter_h = 26.0;
    push_rect(cmds, x, y, w, filter_h, pal.bg_panel_alt);
    push_rect(cmds, x, y + filter_h - 1.0, w, 1.0, pal.border);
    push_rect(cmds, x + 8.0, y + 4.0, w - 16.0, filter_h - 8.0, pal.bg_input);
    push_rect_border(cmds, x + 8.0, y + 4.0, w - 16.0, filter_h - 8.0, pal.border);
    let display_filter = if state.styles.filter.is_empty() {
        push_ui_text_italic(cmds, x + 14.0, y + 7.0,
                            "Filtr (jmeno prop)".to_string(), pal.text_disabled);
        String::new()
    } else {
        push_text(cmds, x + 14.0, y + 7.0, state.styles.filter.clone(), pal.text, false);
        state.styles.filter.clone()
    };

    // Body: sorted props + filter match.
    let body_y = y + filter_h;
    let scroll = state.styles.scroll_y;
    let mut sy = body_y + 8.0 - scroll;
    let max_y = y + h;
    let pad_x = x + 12.0;
    let filter = display_filter.to_lowercase();
    let mut count = 0;
    for (k, v) in &state.styles.computed {
        if sy >= max_y { break; }
        if !filter.is_empty() && !k.contains(&filter) { continue; }
        if sy + ROW_H >= body_y {
            push_text(cmds, pad_x, sy, format!("{}:", k), pal.syn_property, false);
            // Color swatch pri color value.
            let mut value_x = pad_x + 140.0;
            if let Some(c) = parse_css_color(v.trim()) {
                push_rect(cmds, value_x, sy + 3.0, 12.0, 12.0, c);
                push_rect_border(cmds, value_x, sy + 3.0, 12.0, 12.0, pal.border);
                value_x += 16.0;
            }
            push_text(cmds, value_x, sy, v.clone(), pal.text, false);
            count += 1;
        }
        sy += ROW_H;
    }
    if count == 0 && !state.styles.computed.is_empty() {
        push_ui_text_italic(cmds, pad_x, body_y + 12.0,
                            "Zadne shody filtru".to_string(), pal.text_dim);
    }
}

fn paint_side_changes(
    cmds: &mut Vec<DisplayCommand>,
    _state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, _w: f32, _h: f32,
) {
    push_text(cmds, x + 12.0, y + 12.0,
              "Zmeny CSS - zatim prazdne".to_string(), pal.text_dim, true);
}

fn paint_side_compat(
    cmds: &mut Vec<DisplayCommand>,
    _state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, _w: f32, _h: f32,
) {
    push_text(cmds, x + 12.0, y + 12.0,
              "Kompatibilita - browser support per prop".to_string(), pal.text_dim, true);
}

fn paint_side_fonts(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, _h: f32,
) {
    let mut sy = y + 4.0;
    let pad_x = x + 12.0;
    sy = paint_section_header(cmds, state, pal, &SectionId::FontsUsed,
                               "Pouzity font v elementu", x, sy, w);
    if !state.collapsed_sections.contains(&SectionId::FontsUsed) {
        let family = if bx.font_family.is_empty() { "default (Times Roman)".to_string() }
                     else { bx.font_family.clone() };
        push_text(cmds, pad_x, sy, family.clone(), pal.text, false);
        sy += ROW_H;
        push_text(cmds, pad_x, sy,
                  format!("{}px / {} / {}", bx.font_size as i32,
                          if bx.bold { "bold (700)" } else { "normal (400)" },
                          if bx.italic { "italic" } else { "normal" }),
                  pal.text_dim, false);
        sy += ROW_H;
        // Glyph preview (sample text).
        let preview = "AaBbCcDd 0123";
        push_rect(cmds, pad_x, sy + 4.0, w - 24.0, 32.0, pal.bg_panel_alt);
        cmds.push(DisplayCommand::Text {
            x: pad_x + 8.0, y: sy + 8.0,
            content: preview.to_string(),
            color: pal.text,
            font_size: 18.0, bold: bx.bold, italic: bx.italic,
            font_family: family,
            strikethrough: false, underline: false,
        });
        sy += 40.0;
        push_ui_text(cmds, pad_x, sy,
                     format!("line-height: {:.2}", bx.line_height), pal.text_dim, false);
    }
    // @font-face section.
    sy += 8.0;
    sy = paint_section_header(cmds, state, pal, &SectionId::FontsFaces,
                               "@font-face deklarace", x, sy, w);
    if !state.collapsed_sections.contains(&SectionId::FontsFaces) {
        if state.styles.font_faces.is_empty() {
            push_ui_text_italic(cmds, pad_x, sy,
                                "Zadne @font-face v dokumentu".to_string(),
                                pal.text_dim);
        } else {
            for (family, src, weight, style) in &state.styles.font_faces {
                push_text(cmds, pad_x, sy, family.clone(), pal.syn_attr, false);
                sy += ROW_H;
                let src_short = if src.chars().count() > 50 {
                    format!("{}...", src.chars().take(50).collect::<String>())
                } else { src.clone() };
                push_text(cmds, pad_x + 8.0, sy, src_short, pal.text_dim, false);
                sy += ROW_H;
                if !weight.is_empty() || !style.is_empty() {
                    push_text(cmds, pad_x + 8.0, sy,
                              format!("{} {}", weight, style), pal.text_disabled, false);
                    sy += ROW_H;
                }
                sy += 4.0;
            }
        }
    }
}

fn paint_side_animations(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, _h: f32,
) {
    let mut sy = y + 4.0;
    let pad_x = x + 12.0;
    sy = paint_section_header(cmds, state, pal, &SectionId::AnimationsList,
                               "Animace na elementu", x, sy, w);
    if !state.collapsed_sections.contains(&SectionId::AnimationsList) {
        let lookup = |key: &str| state.styles.computed.iter()
            .find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let name = lookup("animation-name");
        if let Some(n) = name.filter(|s| !s.is_empty() && s != "none") {
            push_text(cmds, pad_x, sy, format!("name: {}", n), pal.text, false);
            sy += ROW_H;
            let duration_str = lookup("animation-duration").unwrap_or_else(|| "1s".to_string());
            if let Some(v) = lookup("animation-duration") {
                push_text(cmds, pad_x, sy, format!("duration: {}", v), pal.text_dim, false);
                sy += ROW_H;
            }
            if let Some(v) = lookup("animation-iteration-count") {
                push_text(cmds, pad_x, sy, format!("iterations: {}", v), pal.text_dim, false);
                sy += ROW_H;
            }
            if let Some(v) = lookup("animation-timing-function") {
                push_text(cmds, pad_x, sy, format!("timing: {}", v), pal.text_dim, false);
                sy += ROW_H;
            }
            // Timeline scrubber.
            sy += 8.0;
            push_ui_text(cmds, pad_x, sy, "Timeline".to_string(), pal.text_dim, true);
            sy += ROW_H;
            let timeline_w = w - 24.0;
            let track_y = sy;
            push_rect(cmds, pad_x, track_y, timeline_w, 20.0, pal.bg_panel_alt);
            push_rect_border(cmds, pad_x, track_y, timeline_w, 20.0, pal.border);
            // Playhead - approximate progress (frame_counter % duration).
            let dur_seconds = parse_duration(&duration_str).max(0.1);
            let progress = ((state.frame_counter as f32 / 60.0) % dur_seconds) / dur_seconds;
            let head_x = pad_x + progress * timeline_w;
            push_rect(cmds, head_x - 1.0, track_y - 2.0, 3.0, 24.0, pal.accent);
            sy += 28.0;
            // Keyframes ticks.
            for tick in &[0.0, 0.25, 0.5, 0.75, 1.0] {
                let tx = pad_x + tick * timeline_w;
                push_rect(cmds, tx, track_y + 22.0, 1.0, 4.0, pal.text_dim);
                push_ui_text(cmds, tx - 10.0, track_y + 28.0,
                             format!("{:.0}%", tick * 100.0), pal.text_disabled, false);
            }
        } else {
            push_ui_text_italic(cmds, pad_x, sy,
                                "Zadne aktivni animace".to_string(), pal.text_dim);
        }
    }
    // Transitions.
    let transition = state.styles.computed.iter()
        .find(|(k, _)| k == "transition" || k == "transition-property")
        .map(|(_, v)| v.clone());
    if let Some(t) = transition.filter(|s| !s.is_empty() && s != "none" && s != "all") {
        sy += 8.0;
        push_ui_text(cmds, pad_x, sy, format!("transition: {}", t), pal.text_dim, true);
    }
    let _ = bx;
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
    let display_text = if state.elements.search.query.text.is_empty() {
        placeholder_text.to_string()
    } else {
        state.elements.search.query.text.clone()
    };
    let txt_color = if state.elements.search.query.text.is_empty() {
        pal.text_disabled
    } else {
        pal.text
    };
    push_text(cmds, input_x + 6.0, input_y + 4.0, display_text, txt_color, false);

    // Match counter.
    let s = &state.elements.search;
    let counter = if s.query.text.is_empty() {
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
        RowKind::Element { tag, attrs, self_closing, has_children, inline_text } => {
            // Caret expand/collapse pro elementy s detmi.
            let collapsed = state.elements.collapsed.contains(&row.node_id);
            let mut x = x_indent;
            if *has_children {
                let caret_color = if is_sel { text_color_default } else { pal.text_dim };
                let icon = if collapsed { ICON_CHEVRON_RIGHT } else { ICON_EXPAND_MORE };
                push_icon(cmds, x - INDENT_PX, y + (ROW_H - ICON_SIZE) * 0.5, icon, caret_color);
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
            x += close.chars().count() as f32 * FONT_W;
            // Pri collapsed s detmi: ukaz inline text preview kdyz kratky,
            // jinak "..." (Firefox-style).
            if *has_children && collapsed {
                if let Some(text) = inline_text {
                    let text_color = if is_sel { text_color_default } else { pal.syn_text_node };
                    push_text(cmds, x, text_y, text.clone(), text_color, false);
                    x += text.chars().count() as f32 * FONT_W;
                    push_text(cmds, x, text_y, format!("</{}>", tag), tag_color, false);
                } else {
                    push_text(cmds, x + 2.0, text_y, "...".to_string(), pal.text_dim, false);
                    x += 2.0 + 3.0 * FONT_W;
                    push_text(cmds, x, text_y, format!("</{}>", tag), tag_color, false);
                }
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
    let text_x = x_indent + dt_text_width(&prefix);

    // Selection highlight - real-advance based.
    if let Some((s, e)) = edit.buffer.selection_range() {
        let sx = text_x + dt_text_width(&edit.buffer.text[..s]);
        let ex = text_x + dt_text_width(&edit.buffer.text[..e]);
        push_rect(cmds, sx, text_y - 2.0, ex - sx, FONT_SIZE + 4.0, pal.bg_row_selected);
    }
    push_text(cmds, text_x, text_y, edit.buffer.text.clone(), pal.text, false);
    if state.cursor_visible() {
        let cx = text_x + dt_text_width(&edit.buffer.text[..edit.buffer.cursor]);
        push_rect(cmds, cx, text_y - 2.0, 1.0, FONT_SIZE + 4.0, pal.text);
    }
}

/// Vykresli jeden CSS declaration radek se swatchem barvy a chip-rendering var().
/// Format: `  prop: value;` kde value muze obsahovat var(--name) -> chip a/nebo
/// color literal -> swatch.
fn paint_decl_line(
    cmds: &mut Vec<DisplayCommand>,
    pal: &Palette,
    swatch_zones: &mut Vec<(f32, f32, f32, f32, [u8; 4])>,
    var_zones: &mut Vec<(f32, f32, f32, f32, String)>,
    x: f32, y: f32,
    property: &str,
    value: &str,
    important: bool,
    overridden: bool,
) {
    let mut cx = x;
    push_text(cmds, cx, y, format!("  {}: ", property), pal.syn_property, false);
    cx += dt_text_width(&format!("  {}: ", property));

    // Color swatch detection - prefix value parsing.
    if let Some(c) = parse_css_color(value.trim()) {
        push_rect(cmds, cx, y + 3.0, 12.0, 12.0, c);
        push_rect(cmds, cx, y + 3.0, 12.0, 1.0, pal.border);
        push_rect(cmds, cx, y + 14.0, 12.0, 1.0, pal.border);
        push_rect(cmds, cx, y + 3.0, 1.0, 12.0, pal.border);
        push_rect(cmds, cx + 11.0, y + 3.0, 1.0, 12.0, pal.border);
        swatch_zones.push((cx, y + 3.0, 12.0, 12.0, c));
        cx += 16.0;
    }

    // Render value chunks: var(--x) chips + plain text.
    let mut i = 0;
    let bytes = value.as_bytes();
    while i < bytes.len() {
        if value[i..].starts_with("var(") {
            let end = value[i+4..].find(')').map(|e| e + i + 4).unwrap_or(value.len());
            let inner = &value[i+4..end];
            // Var name = first arg before comma.
            let name = inner.split(',').next().unwrap_or(inner).trim();
            let chip_w = dt_text_width(name) + 14.0;
            let txt_color = if overridden { pal.text_disabled } else { pal.text };
            push_rect(cmds, cx, y + 2.0, chip_w, 14.0, pal.bg_button);
            push_rect_border(cmds, cx, y + 2.0, chip_w, 14.0, pal.border);
            push_text(cmds, cx + 4.0, y, name.to_string(), txt_color, false);
            var_zones.push((cx, y + 2.0, chip_w, 14.0, name.to_string()));
            cx += chip_w + 2.0;
            i = end + 1;
        } else {
            // Find next "var(" to break chunk.
            let next_var = value[i..].find("var(").map(|n| n + i);
            let chunk_end = next_var.unwrap_or(value.len());
            let chunk = &value[i..chunk_end];
            if !chunk.is_empty() {
                let col = if overridden { pal.text_disabled } else { pal.text };
                push_text(cmds, cx, y, chunk.to_string(), col, false);
                cx += dt_text_width(chunk);
            }
            i = chunk_end;
        }
    }
    if important {
        push_text(cmds, cx, y, " !important".to_string(), pal.syn_keyword, false);
        cx += dt_text_width(" !important");
    }
    push_text(cmds, cx, y, ";".to_string(), pal.text_dim, false);
    if overridden {
        // Strikethrough cary.
        let line_w = cx - x;
        push_rect(cmds, x, y + 8.0, line_w, 1.0, pal.text_disabled);
    }
}

/// Test-exposed wrapper.
#[cfg(test)]
pub fn parse_css_color_for_test(s: &str) -> Option<[u8; 4]> {
    parse_css_color(s)
}

/// Parse CSS color literal -> RGBA. Pokrita: #rgb, #rrggbb, #rrggbbaa,
/// rgb()/rgba(), hsl()/hsla() (heuristic), nazvy ('red'/'blue'/...).
fn parse_css_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim();
    if s.starts_with('#') {
        let hex = &s[1..];
        let parse2 = |p: &str| u8::from_str_radix(p, 16).ok();
        match hex.len() {
            3 => {
                let r = parse2(&format!("{}{}", &hex[0..1], &hex[0..1]))?;
                let g = parse2(&format!("{}{}", &hex[1..2], &hex[1..2]))?;
                let b = parse2(&format!("{}{}", &hex[2..3], &hex[2..3]))?;
                Some([r, g, b, 255])
            }
            6 => Some([parse2(&hex[0..2])?, parse2(&hex[2..4])?, parse2(&hex[4..6])?, 255]),
            8 => Some([parse2(&hex[0..2])?, parse2(&hex[2..4])?, parse2(&hex[4..6])?, parse2(&hex[6..8])?]),
            _ => None,
        }
    } else if s.starts_with("rgb(") || s.starts_with("rgba(") {
        let open = s.find('(')?;
        let close = s.find(')')?;
        let parts: Vec<&str> = s[open+1..close].split(',').collect();
        if parts.len() < 3 { return None; }
        let r = parts[0].trim().parse::<f32>().ok()? as u8;
        let g = parts[1].trim().parse::<f32>().ok()? as u8;
        let b = parts[2].trim().parse::<f32>().ok()? as u8;
        let a = parts.get(3).and_then(|p| p.trim().parse::<f32>().ok())
            .map(|f| (f * 255.0) as u8).unwrap_or(255);
        Some([r, g, b, a])
    } else {
        // Named colors - pokryte zakladni.
        let named = match s.to_lowercase().as_str() {
            "red" => [255, 0, 0, 255],
            "green" => [0, 128, 0, 255],
            "blue" => [0, 0, 255, 255],
            "white" => [255, 255, 255, 255],
            "black" => [0, 0, 0, 255],
            "yellow" => [255, 255, 0, 255],
            "cyan" => [0, 255, 255, 255],
            "magenta" => [255, 0, 255, 255],
            "gray" | "grey" => [128, 128, 128, 255],
            "orange" => [255, 165, 0, 255],
            "purple" => [128, 0, 128, 255],
            "pink" => [255, 192, 203, 255],
            "brown" => [165, 42, 42, 255],
            "transparent" => [0, 0, 0, 0],
            _ => return None,
        };
        Some(named)
    }
}

fn paint_styles_pane(
    cmds: &mut Vec<DisplayCommand>,
    bx: &LayoutBox,
    state: &DevToolsState,
    pal: &Palette,
    x: f32, y: f32, w: f32, h: f32,
) {
    // Reset swatch + var zones (per-frame populated v paint_decl_line).
    state.styles.swatch_zones.borrow_mut().clear();
    state.styles.var_zones.borrow_mut().clear();
    // Toolbar nahore (filter + :hov/.cls/+ buttons).
    paint_styles_toolbar(cmds, state, pal, x, y, w, 0.0, 0.0);
    let toolbar_h = 26.0;
    let total_h = state.styles.estimate_total_h();
    let max_scroll = (total_h - h + toolbar_h).max(0.0);
    let scroll = state.styles.scroll_y.clamp(0.0, max_scroll);
    let mut sy = y + toolbar_h + 8.0 - scroll;
    let max_y = y + h;
    let min_y = y + toolbar_h;
    let pad_x = x + 12.0;
    // Scrollbar emit pri overflow (track + thumb pri pravem okraji pane).
    if total_h > h {
        let bar_w = SCROLLBAR_W;
        let bar_x = x + w - bar_w;
        push_rect(cmds, bar_x, y, bar_w, h, pal.bg_panel_alt);
        let thumb_h = (h * h / total_h).max(20.0);
        let thumb_y = if max_scroll > 0.0 {
            y + (scroll / max_scroll) * (h - thumb_h)
        } else { y };
        push_rect(cmds, bar_x + 2.0, thumb_y, bar_w - 4.0, thumb_h, pal.border_strong);
    }

    // Visibility check - skip render rows out of [min_y, max_y].
    let in_view = |sy: f32| sy + ROW_H >= min_y && sy < max_y;

    // Section: matched rules header.
    if in_view(sy) {
        push_ui_text(cmds, pad_x, sy, "Vybrane styly".to_string(), pal.text, true);
    }
    sy += ROW_H + 2.0;

    if state.styles.matched_rules.is_empty() {
        if in_view(sy) {
            push_ui_text_italic(cmds, pad_x, sy, "(zadna shoda)".to_string(), pal.text_dim);
        }
        sy += ROW_H + 8.0;
    } else {
        let mut last_inherited_from: Option<String> = None;
        for rule in &state.styles.matched_rules {
            if sy >= max_y { break; }
            // Inherited group header.
            if rule.inherited_from != last_inherited_from {
                if let Some(tag) = &rule.inherited_from {
                    sy += 4.0;
                    if in_view(sy) {
                        push_ui_text(cmds, pad_x, sy,
                                     format!("Pododedeno z {}", tag),
                                     pal.text_dim, true);
                    }
                    sy += ROW_H + 4.0;
                }
                last_inherited_from = rule.inherited_from.clone();
            }
            let src_label = match &rule.source {
                crate::devtools::model::styles::RuleSource::UserAgent => "user agent".to_string(),
                crate::devtools::model::styles::RuleSource::Inline => "inline".to_string(),
                crate::devtools::model::styles::RuleSource::StyleBlock { index } => format!("<style> #{}", index),
                crate::devtools::model::styles::RuleSource::External { url } => {
                    // Vytvor "filename:line" label - line zatim 0 (TODO line track).
                    let fname = url.split('/').last().unwrap_or(url).to_string();
                    fname
                },
            };
            if in_view(sy) {
                // Selektor + specificity badge + source label vpravo.
                let sel_str = format!("{} {{", rule.selector);
                push_text(cmds, pad_x, sy, sel_str.clone(), pal.syn_property, false);
                // Specificity badge (a, b, c) format: ID, CLASS, TYPE counts.
                // u32 packed: high 8 = a, mid 8 = b, low 8 = c.
                let a = (rule.specificity >> 16) & 0xFF;
                let b = (rule.specificity >> 8) & 0xFF;
                let c = rule.specificity & 0xFF;
                let badge = format!("({},{},{})", a, b, c);
                let badge_x = pad_x + dt_text_width(&sel_str) + 8.0;
                push_text(cmds, badge_x, sy, badge.clone(), pal.text_disabled, false);
                let label_w = dt_text_width(&src_label);
                let style_pane_w = bx.rect.width.max(300.0);
                let src_x = pad_x + style_pane_w - label_w - 24.0;
                push_text(cmds, src_x, sy, src_label, pal.text_dim, true);
            }
            sy += ROW_H;
            for d in &rule.declarations {
                if sy >= max_y { return; }
                if in_view(sy) {
                    // Highlight bg pri matchu var_highlight (po jump).
                    let highlight = state.var_highlight.as_ref()
                        .map(|(n, _)| n == &d.property).unwrap_or(false);
                    if highlight {
                        push_rect(cmds, pad_x - 4.0, sy, 250.0, ROW_H, pal.bg_row_selected);
                    }
                    let mut sw = state.styles.swatch_zones.borrow_mut();
                    let mut vz = state.styles.var_zones.borrow_mut();
                    paint_decl_line(cmds, pal, &mut sw, &mut vz, pad_x, sy, &d.property, &d.value, d.important, d.overridden);
                }
                sy += ROW_H;
            }
            if in_view(sy) {
                push_text(cmds, pad_x, sy, "}".to_string(), pal.syn_property, false);
            }
            sy += ROW_H + 4.0;
            // Divider line mezi rules.
            if in_view(sy) {
                push_rect(cmds, pad_x, sy, 200.0, 1.0, pal.border);
            }
            sy += 4.0;
        }
    }

    // Computed values - z cascade vystupu (state.styles.computed) + box rect.
    if sy >= max_y { return; }
    if in_view(sy) {
        push_text_bold(cmds, pad_x, sy, "Computed".to_string(), pal.text, false);
    }
    sy += ROW_H + 2.0;

    let filter = state.styles.filter.to_lowercase();
    for (k, v) in &state.styles.computed {
        if sy >= max_y { return; }
        if !filter.is_empty() && !k.contains(&filter) { continue; }
        if in_view(sy) {
            push_text(cmds, pad_x, sy, format!("{}:", k), pal.syn_property, false);
            push_text(cmds, pad_x + 160.0, sy, v.clone(), pal.text, false);
        }
        sy += ROW_H;
    }

    // Box info (rect / margin / padding z LayoutBox).
    if sy >= max_y { return; }
    sy += 8.0;
    if in_view(sy) {
        push_text_bold(cmds, pad_x, sy, "Box".to_string(), pal.text, false);
    }
    sy += ROW_H + 2.0;
    // Asymmetric margin/padding: top/right/bottom/left wins, jinak shorthand.
    let p_t = bx.padding_top.unwrap_or(bx.padding);
    let p_r = bx.padding_right.unwrap_or(bx.padding);
    let p_b = bx.padding_bottom.unwrap_or(bx.padding);
    let p_l = bx.padding_left.unwrap_or(bx.padding);
    let m_t = bx.margin_top.unwrap_or(bx.margin);
    let m_r = bx.margin_right.unwrap_or(bx.margin);
    let m_b = bx.margin_bottom.unwrap_or(bx.margin);
    let m_l = bx.margin_left.unwrap_or(bx.margin);
    let box_info = vec![
        ("rect", format!("x={:.0} y={:.0} w={:.0} h={:.0}",
            bx.rect.x, bx.rect.y, bx.rect.width, bx.rect.height)),
        ("padding", format!("{:.0} {:.0} {:.0} {:.0}", p_t, p_r, p_b, p_l)),
        ("margin", format!("{:.0} {:.0} {:.0} {:.0}", m_t, m_r, m_b, m_l)),
        ("border-width", format!("{:.0}", bx.border_width)),
    ];
    for (k, v) in &box_info {
        if sy >= max_y { break; }
        if in_view(sy) {
            push_text(cmds, pad_x, sy, format!("{}:", k), pal.syn_property, false);
            push_text(cmds, pad_x + 160.0, sy, v.clone(), pal.text, false);
        }
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
    let text_x = prompt_x + dt_text_width("> ");

    // Selection highlight - real-advance based.
    if let Some((s, e)) = input.selection_range() {
        let sel_x0 = text_x + dt_text_width(&input.text[..s]);
        let sel_x1 = text_x + dt_text_width(&input.text[..e]);
        push_rect(cmds, sel_x0, text_y - 2.0, sel_x1 - sel_x0, FONT_SIZE + 4.0,
                  pal.bg_row_selected);
    }

    push_text(cmds, text_x, text_y, input.text.clone(), pal.text, false);

    if focused && state.cursor_visible() {
        let cur_x = text_x + dt_text_width(&input.text[..input.cursor]);
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
/// Page-side overlay paint: flex/grid container visualization (Firefox-style).
/// Pro kazdy aktivni overlay v state.overlays vykresli na strance:
/// - Flex: dashed border container, solid border per item, gap stripes,
///   free space crosshatch
/// - Grid: line numbers, area names (TODO), infinite extension
pub fn paint_inspector_overlays(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    state: &DevToolsState,
    scroll_y: f32,
) {
    if !state.panel_open { return; }
    use crate::devtools::{OverlayKind};
    for ov in &state.overlays {
        let Some(bx) = find_layout_box(layout_root, ov.node_id) else { continue };
        match ov.kind {
            OverlayKind::Flex => paint_flex_overlay(cmds, bx, scroll_y),
            OverlayKind::Grid => paint_grid_overlay(cmds, bx, scroll_y),
        }
    }
}

fn paint_flex_overlay(cmds: &mut Vec<DisplayCommand>, bx: &LayoutBox, scroll_y: f32) {
    let purple: [u8; 4] = [173, 127, 232, 255];
    let purple_dim: [u8; 4] = [173, 127, 232, 80];
    let r = &bx.rect;
    let cx = r.x;
    let cy = r.y - scroll_y;
    let cw = r.width;
    let ch = r.height;
    // Container dashed border.
    push_dashed_border(cmds, cx, cy, cw, ch, 2.0, purple);
    // Per-item solid border + gap stripes.
    let row_dir = !bx.flex_direction.contains("column");
    let mut prev_end: Option<f32> = None;
    for child in &bx.children {
        let cr = &child.rect;
        let ix = cr.x;
        let iy = cr.y - scroll_y;
        let iw = cr.width;
        let ih = cr.height;
        // Solid 1px border.
        push_rect(cmds, ix, iy, iw, 1.0, purple);
        push_rect(cmds, ix, iy + ih - 1.0, iw, 1.0, purple);
        push_rect(cmds, ix, iy, 1.0, ih, purple);
        push_rect(cmds, ix + iw - 1.0, iy, 1.0, ih, purple);
        // Gap stripe pred itemom.
        if let Some(pe) = prev_end {
            if row_dir {
                let gap_w = ix - pe;
                if gap_w > 0.5 {
                    push_rect(cmds, pe, iy, gap_w, ih, purple_dim);
                }
            } else {
                let gap_h = iy - pe;
                if gap_h > 0.5 {
                    push_rect(cmds, ix, pe, iw, gap_h, purple_dim);
                }
            }
        }
        prev_end = Some(if row_dir { ix + iw } else { iy + ih });
    }
    // Free space (zbytek po itemech v container) - crosshatch v koncove zone.
    if let Some(end) = prev_end {
        if row_dir {
            let free_w = (cx + cw) - end;
            if free_w > 1.0 {
                push_rect(cmds, end, cy, free_w, ch, [173, 127, 232, 30]);
            }
        } else {
            let free_h = (cy + ch) - end;
            if free_h > 1.0 {
                push_rect(cmds, cx, end, cw, free_h, [173, 127, 232, 30]);
            }
        }
    }
}

fn paint_grid_overlay(cmds: &mut Vec<DisplayCommand>, bx: &LayoutBox, scroll_y: f32) {
    let cyan: [u8; 4] = [100, 200, 255, 255];
    let r = &bx.rect;
    let cx = r.x;
    let cy = r.y - scroll_y;
    let cw = r.width;
    let ch = r.height;
    push_dashed_border(cmds, cx, cy, cw, ch, 2.0, cyan);
    // Per-item border (cells).
    for child in &bx.children {
        let cr = &child.rect;
        push_rect(cmds, cr.x, cr.y - scroll_y, cr.width, 1.0, cyan);
        push_rect(cmds, cr.x, cr.y - scroll_y + cr.height - 1.0, cr.width, 1.0, cyan);
        push_rect(cmds, cr.x, cr.y - scroll_y, 1.0, cr.height, cyan);
        push_rect(cmds, cr.x + cr.width - 1.0, cr.y - scroll_y, 1.0, cr.height, cyan);
    }
}

/// Dashed border 1px - vykresli pres push_rect mensi segments.
fn push_dashed_border(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, w: f32, h: f32, dash: f32, color: [u8; 4]) {
    let gap = dash;
    // Top + bottom.
    let mut dx = 0.0;
    while dx < w {
        let seg = dash.min(w - dx);
        push_rect(cmds, x + dx, y, seg, 1.0, color);
        push_rect(cmds, x + dx, y + h - 1.0, seg, 1.0, color);
        dx += dash + gap;
    }
    // Left + right.
    let mut dy = 0.0;
    while dy < h {
        let seg = dash.min(h - dy);
        push_rect(cmds, x, y + dy, 1.0, seg, color);
        push_rect(cmds, x + w - 1.0, y + dy, 1.0, seg, color);
        dy += dash + gap;
    }
}

pub fn paint_element_highlight(
    cmds: &mut Vec<DisplayCommand>,
    layout_root: &LayoutBox,
    state: &DevToolsState,
    scroll_y: f32,
) {
    // Highlight jen pri hoveru v devtools tree (Firefox-style). Selected
    // element ZUSTAVA v tree highlighted ale na page overlay pouze pri hover.
    // Driv hovered.or(selected) -> trvaly visualni overlay. Ted hovered only.
    if !state.panel_open { return; }
    let pal = state.palette();
    let Some(node_id) = state.elements.hovered else { return };
    let Some(bx) = find_layout_box(layout_root, node_id) else { return };

    // Rect obsahu = bx.rect (uz po margin/padding pripravne v build_box).
    // Asymmetric margin/padding: top/right/bottom/left wins, jinak shorthand.
    let r = &bx.rect;
    let p_t = bx.padding_top.unwrap_or(bx.padding);
    let p_r = bx.padding_right.unwrap_or(bx.padding);
    let p_b = bx.padding_bottom.unwrap_or(bx.padding);
    let p_l = bx.padding_left.unwrap_or(bx.padding);
    let m_t = bx.margin_top.unwrap_or(bx.margin);
    let m_r = bx.margin_right.unwrap_or(bx.margin);
    let m_b = bx.margin_bottom.unwrap_or(bx.margin);
    let m_l = bx.margin_left.unwrap_or(bx.margin);
    let bw = bx.border_width.max(0.0);

    let content_x = r.x;
    let content_y = r.y - scroll_y;
    let content_w = r.width;
    let content_h = r.height;

    // Box rect = content. Padding box = +padding na vsech stranach.
    // Border box = +border. Margin box = +margin.
    // Vykresli je ako 4 vrstvy (margin -> border -> padding -> content), kazda jen
    // ramecek (vnejsi minus vnitrni).
    // Margin rect (oranzova) - asymmetric per side.
    let mx = content_x - p_l - bw - m_l;
    let my = content_y - p_t - bw - m_t;
    let mw = content_w + p_l + p_r + 2.0 * bw + m_l + m_r;
    let mh = content_h + p_t + p_b + 2.0 * bw + m_t + m_b;
    push_rect(cmds, mx, my, mw, mh, pal.overlay_margin);

    // Border rect (zluta).
    let bx_x = content_x - p_l - bw;
    let by_y = content_y - p_t - bw;
    let bw_w = content_w + p_l + p_r + 2.0 * bw;
    let bh_h = content_h + p_t + p_b + 2.0 * bw;
    push_rect(cmds, bx_x, by_y, bw_w, bh_h, pal.overlay_border);

    // Padding rect (zelena) - asymmetric per side.
    let px = content_x - p_l;
    let py = content_y - p_t;
    let pw = content_w + p_l + p_r;
    let ph = content_h + p_t + p_b;
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
    /// Drag splitter v Elements tab (resize tree vs styles).
    SplitterDrag,
    /// Drag scrollbar thumb (target panel).
    ScrollbarThumb(crate::devtools::ScrollTarget),
    /// Klik na ▼ tab overflow button - toggle popup.
    TabOverflowToggle,
    /// Klik na overflow popup item.
    TabOverflowPick(Tab),
    /// Klik na side panel sub-tab.
    SidePanelTabClick(crate::devtools::SidePanelTab),
    /// Klik na side panel splitter (mezi styles a side panelem).
    SidePanelSplitterDrag,
    /// Klik na collapsible section header (toggle expand).
    SectionToggle(SectionId),
    /// Klik na flex/grid overlay toggle v Layout sub-tabu.
    OverlayToggle(crate::devtools::OverlayKind, usize),
    /// Klik na settings gear button v toolbaru.
    SettingsToggle,
    /// Klik v settings popupu - vyber dock position.
    SettingsDock(crate::devtools::profile::DockPosition),
    /// Klik mimo settings popup nebo na X.
    SettingsClose,
    /// Color picker: hue slider klik (hue 0..360).
    ColorPickerHue(f32),
    /// Color picker: SV box klik - normalized (saturation 0..1, value 0..1).
    ColorPickerSV(f32, f32),
    /// Color picker: klik mimo -> close.
    ColorPickerClose,
    /// Klik na color swatch v styles pane -> open color picker.
    OpenColorPicker { anchor_x: f32, anchor_y: f32, color: [u8; 4] },
    /// Klik na :hov toolbar button - cycle hover/focus/active force.
    ForcePseudoToggle,
    /// Klik na .cls toolbar button - toggle class manager popup.
    ClassManagerToggle,
    /// Klik na + toolbar button - add new rule (TODO).
    AddNewRule,
    /// Klik na var() chip v styles pane - jump na :root rule s definici.
    JumpToVar(String),
    /// Class manager: toggle class na selected node.
    ClassManagerToggleClass(String),
}

/// Stable ID pro collapsible sections - persistuje state napric framem.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SectionId {
    LayoutFlex,
    LayoutGrid,
    LayoutBoxModel,
    LayoutBoxProps,
    StylesMatched,
    StylesComputed,
    StylesBox,
    FontsUsed,
    FontsFaces,
    AnimationsList,
    ChangesList,
    CompatibilityList,
}

pub fn devtools_hit_test(
    state: &DevToolsState,
    layout_root: &LayoutBox,
    win_w: f32,
    win_h: f32,
    mouse_x: f32, mouse_y: f32,
) -> DevtoolsHit {
    if !state.panel_open { return DevtoolsHit::None; }
    // Class manager popup: outside click -> dismiss; checkbox click -> toggle.
    if state.class_manager_open {
        let pop_w = 280.0;
        let pop_h = 240.0;
        let px = (win_w - pop_w) * 0.5;
        let py = (win_h - pop_h) * 0.5;
        if mouse_x < px || mouse_x >= px + pop_w
           || mouse_y < py || mouse_y >= py + pop_h {
            return DevtoolsHit::ClassManagerToggle;
        }
        // Checkbox row hit-test - per class.
        if let Some(sel_id) = state.elements.selected {
            if let Some(bx) = find_layout_box(layout_root, sel_id) {
                let class_attr = bx.node.as_ref().and_then(|n|
                    n.attributes.borrow().get("class").cloned()
                ).unwrap_or_default();
                let mut sy = py + 44.0 + ROW_H + 4.0;
                for cls in class_attr.split_whitespace() {
                    if mouse_y >= sy && mouse_y < sy + ROW_H {
                        return DevtoolsHit::ClassManagerToggleClass(cls.to_string());
                    }
                    sy += ROW_H;
                }
            }
        }
        return DevtoolsHit::PanelArea;
    }
    // Color picker priority: klik mimo popup -> close.
    if let Some(cp) = &state.color_picker {
        let pop_w = 240.0;
        let pop_h = 220.0;
        if mouse_x < cp.anchor_x || mouse_x >= cp.anchor_x + pop_w
           || mouse_y < cp.anchor_y || mouse_y >= cp.anchor_y + pop_h {
            return DevtoolsHit::ColorPickerClose;
        }
        let sv_x = cp.anchor_x + 10.0;
        let sv_y = cp.anchor_y + 10.0;
        let sv_w = pop_w - 20.0;
        let sv_h = 120.0;
        // SV box klik -> sat/val.
        if mouse_x >= sv_x && mouse_x < sv_x + sv_w
           && mouse_y >= sv_y && mouse_y < sv_y + sv_h {
            let s = ((mouse_x - sv_x) / sv_w).clamp(0.0, 1.0);
            let v = 1.0 - ((mouse_y - sv_y) / sv_h).clamp(0.0, 1.0);
            return DevtoolsHit::ColorPickerSV(s, v);
        }
        // Hue slider hit.
        let hue_y = sv_y + sv_h + 8.0;
        if mouse_y >= hue_y && mouse_y < hue_y + 12.0
           && mouse_x >= sv_x && mouse_x < sv_x + sv_w {
            let frac = ((mouse_x - sv_x) / sv_w).clamp(0.0, 1.0);
            return DevtoolsHit::ColorPickerHue(frac * 360.0);
        }
        return DevtoolsHit::PanelArea;
    }
    use crate::devtools::profile::DockPosition;
    let s = state.panel_h.min(match state.dock_position {
        DockPosition::Left | DockPosition::Right => win_w * 0.7,
        _ => win_h * 0.7,
    });
    let (panel_x, panel_y, panel_w, panel_h) = match state.dock_position {
        DockPosition::Bottom | DockPosition::PopupWindow =>
            (0.0, win_h - s, win_w, s),
        DockPosition::Top => (0.0, 0.0, win_w, s),
        DockPosition::Left => (0.0, 0.0, s, win_h),
        DockPosition::Right => (win_w - s, 0.0, s, win_h),
    };
    let _ = (panel_x, panel_w); // pouzite dale - nehlasi unused.

    // Settings popup ma prioritu nad cely panel hit-test.
    if state.settings_popup_open {
        if let Some(action) = settings_popup_hit(state, win_w, win_h, mouse_x, mouse_y) {
            return match action {
                SettingsPopupAction::SelectDock(p) => DevtoolsHit::SettingsDock(p),
                SettingsPopupAction::Close | SettingsPopupAction::Dismiss => DevtoolsHit::SettingsClose,
            };
        }
    }

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

    // Mimo panel rect -> None (per-dock check).
    if mouse_x < panel_x || mouse_x >= panel_x + panel_w
       || mouse_y < panel_y || mouse_y >= panel_y + panel_h {
        return DevtoolsHit::None;
    }
    // Resize grip dle dock position.
    let grip_hit = match state.dock_position {
        DockPosition::Bottom | DockPosition::PopupWindow =>
            mouse_y >= panel_y && mouse_y < panel_y + RESIZE_GRIP_H,
        DockPosition::Top =>
            mouse_y >= panel_y + panel_h - RESIZE_GRIP_H && mouse_y < panel_y + panel_h,
        DockPosition::Left =>
            mouse_x >= panel_x + panel_w - RESIZE_GRIP_H && mouse_x < panel_x + panel_w,
        DockPosition::Right =>
            mouse_x >= panel_x && mouse_x < panel_x + RESIZE_GRIP_H,
    };
    if grip_hit {
        return DevtoolsHit::ResizeGrip;
    }
    let toolbar_y = panel_y + RESIZE_GRIP_H;
    // Tab overflow popup hit-test (priorita pred bezne tab cliky).
    if state.tab_overflow_open {
        if let Some(idx) = tab_overflow_popup_hit(state, win_w, toolbar_y, mouse_x, mouse_y) {
            let (_, overflow) = compute_tab_layout(win_w);
            if let Some(t) = overflow.get(idx).copied() {
                return DevtoolsHit::TabOverflowPick(t);
            }
        }
    }
    if mouse_y < toolbar_y + TAB_H {
        // Visible taby (vrame compute_tab_layout).
        let (visible, _) = compute_tab_layout(win_w);
        for (i, t) in visible.iter().enumerate() {
            let (tx, ty, tw, th) = tab_rect_in_visible(&visible, i, toolbar_y);
            if mouse_x >= tx && mouse_x < tx + tw && mouse_y >= ty && mouse_y < ty + th {
                return DevtoolsHit::TabClick(*t);
            }
        }
        // Overflow ▼ button.
        let (bx, by, bw, bh) = tab_overflow_btn_rect(win_w, toolbar_y);
        let (_, overflow) = compute_tab_layout(win_w);
        if !overflow.is_empty()
           && mouse_x >= bx && mouse_x < bx + bw
           && mouse_y >= by && mouse_y < by + bh {
            return DevtoolsHit::TabOverflowToggle;
        }
        // Toolbar actions vpravo.
        let mut x_right = toolbar_actions_x(win_w);
        // Close.
        let close_w = 24.0;
        x_right -= close_w;
        if mouse_x >= x_right && mouse_x < x_right + close_w {
            return DevtoolsHit::Close;
        }
        // Settings gear.
        let set_w = 24.0;
        x_right -= set_w + 4.0;
        if mouse_x >= x_right && mouse_x < x_right + set_w {
            return DevtoolsHit::SettingsToggle;
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
    // Shift mouse_x do panel-local coords pri non-Bottom dock (paint pouziva
    // x=0 origin a flushuje pres panel_x shift, hit-test musi delat opak).
    let local_mx = mouse_x - panel_x;
    let content_w = panel_w;
    match state.tab {
        Tab::Elements => hit_test_elements(state, layout_root, content_w, content_y, content_h, local_mx, mouse_y),
        Tab::Console => {
            let input_h = 32.0;
            let input_y = content_y + content_h - input_h;
            if mouse_y >= input_y { return DevtoolsHit::ConsoleInput; }
            DevtoolsHit::PanelArea
        }
        Tab::Sources => hit_test_sources(state, content_w, content_y, content_h, local_mx, mouse_y),
        Tab::Network => hit_test_network(state, content_w, content_y, content_h, local_mx, mouse_y),
        Tab::Settings => hit_test_settings(state, content_y, local_mx, mouse_y),
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
    let body_h = content_h - search_h;
    // Three-column geometry.
    let default_tree_split = (win_w - state.side_panel_w) * 0.45;
    let tree_split = if state.elements.split_x < 1.0 { default_tree_split }
                     else { state.elements.split_x.max(200.0).min(win_w - state.side_panel_w - 200.0) };
    let side_panel_w = state.side_panel_w.clamp(180.0, win_w - 400.0);
    let styles_end = win_w - side_panel_w;

    // Tree splitter zone.
    if (mouse_x - tree_split).abs() < SPLITTER_HIT_PX && mouse_y >= body_y && mouse_y < body_y + body_h {
        return DevtoolsHit::SplitterDrag;
    }
    // Side panel splitter zone.
    if (mouse_x - styles_end).abs() < SPLITTER_HIT_PX && mouse_y >= body_y && mouse_y < body_y + body_h {
        return DevtoolsHit::SidePanelSplitterDrag;
    }

    // Side panel area (right column).
    if mouse_x >= styles_end {
        // Sub-tab strip nahore.
        if mouse_y < body_y + TAB_H {
            use crate::devtools::SidePanelTab;
            let mut tx = styles_end + 8.0;
            for t in SidePanelTab::visible_default() {
                let label = t.label();
                let tw = (label.len() as f32) * FONT_W + 12.0;
                if mouse_x >= tx && mouse_x < tx + tw {
                    return DevtoolsHit::SidePanelTabClick(*t);
                }
                tx += tw + 2.0;
            }
        }
        // Section header hit-test (toggle collapse). Headers podle sub-tabu.
        let body_y_inner = body_y + TAB_H;
        if mouse_y >= body_y_inner {
            // Per-sub-tab section list (poradi musi pasovat na paint).
            let sections: &[SectionId] = match state.side_panel_tab {
                crate::devtools::SidePanelTab::Layout => &[
                    SectionId::LayoutFlex, SectionId::LayoutGrid,
                    SectionId::LayoutBoxModel, SectionId::LayoutBoxProps,
                ],
                crate::devtools::SidePanelTab::Fonts => &[SectionId::FontsUsed, SectionId::FontsFaces],
                crate::devtools::SidePanelTab::Animations => &[SectionId::AnimationsList],
                _ => &[],
            };
            let mut sy = body_y_inner + 4.0;
            for id in sections {
                let header_h = ROW_H + 4.0;
                if mouse_x >= styles_end && mouse_x < styles_end + side_panel_w
                   && mouse_y >= sy && mouse_y < sy + header_h {
                    return DevtoolsHit::SectionToggle(id.clone());
                }
                sy += header_h;
                // Aproximace - ne-presne ale dostatecne pro top-level sections.
                if !state.collapsed_sections.contains(id) {
                    sy += ROW_H * 4.0; // typicke obsah
                }
            }
        }
        return DevtoolsHit::PanelArea;
    }

    // Styles pane area.
    if mouse_x >= tree_split {
        // Toolbar buttons hit (top 26px).
        if mouse_y >= body_y && mouse_y < body_y + 26.0 {
            if let Some(idx) = styles_toolbar_btn_hit(tree_split, body_y, styles_end - tree_split, mouse_x, mouse_y) {
                return match idx {
                    0 => DevtoolsHit::ForcePseudoToggle,
                    1 => DevtoolsHit::ClassManagerToggle,
                    _ => DevtoolsHit::AddNewRule,
                };
            }
        }
        // Color swatch hit (zone cached pri last paint).
        let zones = state.styles.swatch_zones.borrow();
        for (zx, zy, zw, zh, col) in zones.iter() {
            if mouse_x >= *zx && mouse_x < zx + zw
               && mouse_y >= *zy && mouse_y < zy + zh {
                return DevtoolsHit::OpenColorPicker {
                    anchor_x: *zx,
                    anchor_y: zy + zh + 4.0,
                    color: *col,
                };
            }
        }
        drop(zones);
        // Var chip hit.
        let vzones = state.styles.var_zones.borrow();
        for (zx, zy, zw, zh, name) in vzones.iter() {
            if mouse_x >= *zx && mouse_x < zx + zw
               && mouse_y >= *zy && mouse_y < zy + zh {
                return DevtoolsHit::JumpToVar(name.clone());
            }
        }
        drop(vzones);
        let total_h = state.styles.computed.len() as f32 * ROW_H * 4.0;
        if total_h > body_h {
            let sb_x = styles_end - SCROLLBAR_W;
            if mouse_x >= sb_x && mouse_x < sb_x + SCROLLBAR_W
               && mouse_y >= body_y && mouse_y < body_y + body_h {
                return DevtoolsHit::ScrollbarThumb(crate::devtools::ScrollTarget::StylesPane);
            }
        }
        return DevtoolsHit::PanelArea;
    }
    let split_x = tree_split;

    // Levy pane - check tree scrollbar thumb.
    let rows = &state.elements.rows;
    let total_h = rows.len() as f32 * ROW_H;
    if total_h > body_h {
        let sb_x = split_x - SCROLLBAR_W;
        if mouse_x >= sb_x && mouse_x < sb_x + SCROLLBAR_W
           && mouse_y >= body_y && mouse_y < body_y + body_h {
            return DevtoolsHit::ScrollbarThumb(crate::devtools::ScrollTarget::ElementsTree);
        }
    }

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

/// Bold text - default Inter (UI). Code-style bold text pouzit
/// push_text_code_bold (CamingoMono-Bold).
fn push_text_bold(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], italic: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: true,
        italic,
        font_family: if italic { DT_UI_FONT_ITALIC.into() } else { DT_UI_FONT_BOLD.into() },
        strikethrough: false, underline: false,
    });
}

#[allow(dead_code)]
fn push_text_code_bold(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], italic: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: true,
        italic,
        font_family: DT_FONT_BOLD.into(),
        strikethrough: false, underline: false,
    });
}

/// UI text - sans-serif Inter pro headings, labels, panel chrome.
fn push_ui_text(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4], bold: bool) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold,
        italic: false,
        font_family: if bold { DT_UI_FONT_BOLD.into() } else { DT_UI_FONT.into() },
        strikethrough: false, underline: false,
    });
}

fn push_ui_text_italic(cmds: &mut Vec<DisplayCommand>, x: f32, y: f32, content: String, color: [u8; 4]) {
    cmds.push(DisplayCommand::Text {
        x, y, content, color,
        font_size: FONT_SIZE, bold: false,
        italic: true,
        font_family: DT_UI_FONT_ITALIC.into(),
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
