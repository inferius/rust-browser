//! DevTools - sjednoceny model + state pro inline (wgpu) + static (HTML) frontends.
//!
//! Architektura: `DevToolsState` drzi vsechny data per panel (Elements/Console/Network/
//! Sources/Performance/Application). Frontends (browser/devtools_panel.rs +
//! debug_view/devtools.rs) cti state a renderuji ho - wgpu DisplayCommands resp HTML.
//!
//! Lifecycle: Renderer drzi `Rc<RefCell<DevToolsState>>`. Panel toggle (F12) jen meni
//! `panel_h`. Selection a state survive napric toggles. Page-side overlay (highlight
//! rect na vybrany element) je nezavisly na panel_h - vykresli se vzdy pri Some(selected).

pub mod theme;
pub mod model;
pub mod context_menu;
pub mod search;
pub mod focus;
pub mod debug_runner;
pub mod profile;
pub mod history;
pub mod bookmarks;
pub mod downloads;
pub mod session;

use std::collections::HashSet;

#[cfg(test)]
#[path = "tests/firefox_devtools_tests.rs"]
mod firefox_tests;
use theme::{ThemeSelection, Palette, resolve_palette};
use model::elements::ElementRow;
use model::console::{ConsoleInput, LogEntry, AutocompleteState};
use model::network::{NetworkEntry, NetworkFilter};
use model::sources::SourcesState;
use model::performance::PerformanceState;
use model::styles::StylesState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Elements,
    Console,
    Network,
    Sources,
    Performance,
    Application,
    Settings,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Elements => "Elements",
            Tab::Console => "Console",
            Tab::Network => "Network",
            Tab::Sources => "Sources",
            Tab::Performance => "Performance",
            Tab::Application => "Application",
            Tab::Settings => "Settings",
        }
    }

    pub fn all() -> &'static [Tab] {
        // Settings tab odstranen z visible tabs - dostupne pres gear popup
        // (klik ozubene kolo v pravem toolbaru otevre nastaveni dock+theme+flavor).
        &[
            Tab::Elements, Tab::Console, Tab::Network,
            Tab::Sources, Tab::Performance, Tab::Application,
        ]
    }
}

#[derive(Debug)]
pub struct DevToolsState {
    pub theme: ThemeSelection,
    pub tab: Tab,
    pub panel_h: f32,
    pub panel_open: bool,

    pub elements: ElementsState,
    pub console: ConsoleState,
    pub network: NetworkState,
    pub sources: SourcesState,
    pub performance: PerformanceState,
    pub styles: StylesState,

    pub focus: focus::FocusTarget,
    pub context_menu: Option<context_menu::ContextMenuState>,
    pub inspect_mode: bool,

    /// Frame counter pro cursor blink.
    pub frame_counter: u64,
    /// Tab overflow popup (Firefox-style ▼ pri uzkem okne).
    pub tab_overflow_open: bool,
    /// Selected side panel sub-tab v Inspectoru.
    pub side_panel_tab: SidePanelTab,
    /// Aktivni overlay descriptors (flex/grid visualization na strance).
    pub overlays: Vec<OverlayDescriptor>,
    /// Collapsible sections - set obsahuje IDs ktere user collapsed.
    pub collapsed_sections: HashSet<crate::browser::devtools_panel::SectionId>,
    /// Side panel sirka v px (right column = vypocitano/rozlozeni/...).
    pub side_panel_w: f32,
    /// Sub-tab overflow dropdown otevren - ▼ chevron pri zmenseni panelu.
    pub side_panel_overflow_open: bool,
    /// Aktivni match-preview selector (highlight elementu matching selectoru).
    /// None = bez highlight. Toggle pres ctverecek vlevo od selectoru.
    pub match_preview_selector: Option<String>,
    /// Animations panel: aktualni stav prehravani.
    pub animations_paused: bool,
    /// Animations panel: speed multiplier (0.25/0.5/1.0/2.0/4.0).
    pub animations_speed: f32,
    /// Pri pause: progress 0..1 frozen pro paint.
    pub animations_paused_at: Option<f32>,
    /// Dock position devtools panelu (Bottom/Right/Left/Top/Popup).
    pub dock_position: profile::DockPosition,
    /// Settings popup state (kdyz user otevre dock chooser dialog).
    pub settings_popup_open: bool,
    /// Color picker popup state. Some = aktivni (user kliknul na color swatch).
    pub color_picker: Option<ColorPickerState>,
    /// Force pseudo-classes na selected element (Firefox :hov toolbar).
    pub force_hover: bool,
    pub force_focus: bool,
    pub force_active: bool,
    /// Class manager popup (.cls button) - pri kliku ukazuje add/toggle classes.
    pub class_manager_open: bool,
    /// Highlight target var() definice po jump (Some(name) na N frames).
    pub var_highlight: Option<(String, u32)>,
    /// Tooltip state - hover nad swatch/chip ukaze popup s detailem.
    pub tooltip: Option<TooltipState>,
    /// Changes log - tracking inline CSS edits pres devtools.
    pub changes: Vec<ChangeEntry>,
}

#[derive(Debug, Clone)]
pub struct ChangeEntry {
    pub timestamp_ts: u64,
    pub kind: ChangeKind,
    pub target_node_id: usize,
    pub property: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    StyleEdit,
    AttrEdit,
    ClassToggle,
    TextEdit,
}

#[derive(Debug, Clone)]
pub struct TooltipState {
    pub x: f32,
    pub y: f32,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ColorPickerState {
    /// Anchor pozice (kde popup vyskoci) - typicky pod swatch.
    pub anchor_x: f32,
    pub anchor_y: f32,
    /// Aktualne editovana barva (RGBA 0..255).
    pub rgba: [u8; 4],
    /// HSV hue 0..360 pro slider.
    pub hue: f32,
    /// Saturation 0..1.
    pub sat: f32,
    /// Value 0..1.
    pub val: f32,
    /// Source identifikator pro write-back: (node_id, property).
    pub target: Option<(usize, String)>,
    /// HEX input field text (editable). Default sync z rgba.
    pub hex_input: String,
    /// True kdyz uzivatel klikl do hex fieldu - keystroke jdou tam.
    pub hex_focused: bool,
    /// Per-channel R/G/B input texts (editable).
    pub rgb_inputs: [String; 3],
    /// Index focused RGB inputu (0/1/2) nebo None.
    pub rgb_focused: Option<usize>,
}

impl Default for ColorPickerState {
    fn default() -> Self {
        Self {
            anchor_x: 0.0, anchor_y: 0.0,
            rgba: [255, 0, 0, 255],
            hue: 0.0, sat: 1.0, val: 1.0,
            target: None,
            hex_input: "ff0000".to_string(),
            hex_focused: false,
            rgb_inputs: ["255".to_string(), "0".to_string(), "0".to_string()],
            rgb_focused: None,
        }
    }
}

impl ColorPickerState {
    /// Sync hex/rgb input texts z aktualni rgba (po SV/hue klik).
    pub fn sync_inputs_from_rgba(&mut self) {
        self.hex_input = format!("{:02x}{:02x}{:02x}", self.rgba[0], self.rgba[1], self.rgba[2]);
        self.rgb_inputs = [
            self.rgba[0].to_string(),
            self.rgba[1].to_string(),
            self.rgba[2].to_string(),
        ];
    }
    /// Apply hex_input -> rgba. True pokud parse OK.
    pub fn apply_hex(&mut self) -> bool {
        let s = self.hex_input.trim().trim_start_matches('#');
        if s.len() != 6 { return false; }
        let r = u8::from_str_radix(&s[0..2], 16);
        let g = u8::from_str_radix(&s[2..4], 16);
        let b = u8::from_str_radix(&s[4..6], 16);
        match (r, g, b) {
            (Ok(r), Ok(g), Ok(b)) => {
                self.rgba = [r, g, b, 255];
                let (h, s_v, v) = rgb_to_hsv(r, g, b);
                self.hue = h;
                self.sat = s_v;
                self.val = v;
                self.sync_inputs_from_rgba();
                true
            }
            _ => false
        }
    }
    /// Apply rgb_inputs[i] -> rgba.
    pub fn apply_rgb(&mut self, i: usize) -> bool {
        if i >= 3 { return false; }
        let v = self.rgb_inputs[i].trim().parse::<u32>().ok().filter(|x| *x <= 255);
        match v {
            Some(v) => {
                self.rgba[i] = v as u8;
                let (h, s_v, vv) = rgb_to_hsv(self.rgba[0], self.rgba[1], self.rgba[2]);
                self.hue = h; self.sat = s_v; self.val = vv;
                self.sync_inputs_from_rgba();
                true
            }
            None => false,
        }
    }
}

/// RGB (0..255) -> HSV (h:0..360, s:0..1, v:0..1).
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let d = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { d / max };
    let h = if d == 0.0 { 0.0 }
            else if max == rf { 60.0 * (((gf - bf) / d) % 6.0) }
            else if max == gf { 60.0 * (((bf - rf) / d) + 2.0) }
            else { 60.0 * (((rf - gf) / d) + 4.0) };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

/// HSV (h:0..360, s:0..1, v:0..1) -> RGB (0..255).
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 4] {
    let c = v * s;
    let h6 = h / 60.0;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (rp, gp, bp) = match h6 as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [
        ((rp + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((gp + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((bp + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        255,
    ]
}

/// Side panel sub-tab v Inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidePanelTab {
    Layout,
    Computed,
    Changes,
    Compatibility,
    Fonts,
    Animations,
}

impl SidePanelTab {
    pub fn label(self) -> &'static str {
        match self {
            SidePanelTab::Layout => "Rozlozeni",
            SidePanelTab::Computed => "Vypocitano",
            SidePanelTab::Changes => "Zmeny",
            SidePanelTab::Compatibility => "Kompatibilita",
            SidePanelTab::Fonts => "Pisma",
            SidePanelTab::Animations => "Animace",
        }
    }
    /// Sub-taby viditelne v UI default; ostatni za ▼ menu.
    pub fn visible_default() -> &'static [SidePanelTab] {
        &[SidePanelTab::Layout, SidePanelTab::Computed,
          SidePanelTab::Changes, SidePanelTab::Fonts, SidePanelTab::Animations]
    }
    pub fn all() -> &'static [SidePanelTab] {
        &[SidePanelTab::Layout, SidePanelTab::Computed, SidePanelTab::Changes,
          SidePanelTab::Compatibility, SidePanelTab::Fonts, SidePanelTab::Animations]
    }
}

/// Visualizace flex/grid container na strance (Firefox-style overlay).
#[derive(Debug, Clone)]
pub struct OverlayDescriptor {
    pub node_id: usize,
    pub kind: OverlayKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Flex,
    Grid,
}

impl Default for DevToolsState {
    fn default() -> Self {
        DevToolsState {
            theme: ThemeSelection::default(),
            tab: Tab::Elements,
            panel_h: 320.0,
            panel_open: false,
            elements: ElementsState::default(),
            console: ConsoleState::default(),
            network: NetworkState::default(),
            sources: SourcesState::default(),
            performance: PerformanceState::default(),
            styles: StylesState::default(),
            focus: focus::FocusTarget::Page,
            context_menu: None,
            inspect_mode: false,
            frame_counter: 0,
            tab_overflow_open: false,
            side_panel_tab: SidePanelTab::Layout,
            overlays: Vec::new(),
            collapsed_sections: HashSet::new(),
            side_panel_w: 280.0,
            side_panel_overflow_open: false,
            match_preview_selector: None,
            animations_paused: false,
            animations_speed: 1.0,
            animations_paused_at: None,
            dock_position: profile::load_dock_position(),
            settings_popup_open: false,
            color_picker: None,
            force_hover: false,
            force_focus: false,
            force_active: false,
            class_manager_open: false,
            var_highlight: None,
            tooltip: None,
            changes: Vec::new(),
        }
    }
}

impl DevToolsState {
    pub fn palette(&self) -> Palette {
        resolve_palette(self.theme)
    }

    pub fn cursor_visible(&self) -> bool {
        // Blink ~500ms (assume ~60fps -> 30 frames per phase).
        (self.frame_counter / 30) % 2 == 0
    }

    pub fn tick_frame(&mut self) {
        self.frame_counter = self.frame_counter.wrapping_add(1);
        // Decay var_highlight counter.
        if let Some((_, n)) = self.var_highlight.as_mut() {
            if *n > 0 { *n -= 1; }
        }
        if matches!(self.var_highlight, Some((_, 0))) {
            self.var_highlight = None;
        }
    }
}

#[derive(Debug, Default)]
pub struct ElementsState {
    pub rows: Vec<ElementRow>,
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    pub scroll_y: f32,
    pub collapsed: HashSet<usize>,
    pub search: ElementsSearch,
    /// Sirka levego (tree) sloupce v pixelech, drag-resize.
    pub split_x: f32,
    /// Aktualne editovany element field (attr/text/style).
    pub edit: Option<EditState>,
    /// Drag state - tree-styles splitter resize.
    pub dragging_split: bool,
    /// Drag state - styles-side panel splitter resize.
    pub dragging_side_split: bool,
    /// Scrollbar thumb drag state - (target panel, click_offset_v_thumb).
    pub dragging_scrollbar: Option<ScrollTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    ElementsTree,
    StylesPane,
    Console,
    Sources,
}

#[derive(Debug, Clone)]
pub struct EditState {
    pub target: EditTarget,
    pub buffer: model::console::ConsoleInput,
}

#[derive(Debug, Clone)]
pub enum EditTarget {
    /// Editace existujiciho atributu - nahradi value.
    AttributeValue { node_id: usize, attr: String },
    /// Editace nazvu atributu (krok 1 pri AddAttribute).
    AttributeName { node_id: usize, value: String },
    /// Editace text node value.
    TextNode { node_id: usize },
    /// Editace CSS property v Computed/Styles panelu - aplikuje jako inline style.
    InlineStyleProperty { node_id: usize, property: String },
}

#[derive(Debug, Default)]
pub struct ElementsSearch {
    pub open: bool,
    pub query: model::text_buffer::SimpleStringBuffer,
    pub matches: Vec<usize>,
    pub current: usize,
    pub mode: SearchMode,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Auto,
    Css,
    XPath,
    Tag,
}

#[derive(Debug, Default)]
pub struct ConsoleState {
    pub log: Vec<LogEntry>,
    pub input: ConsoleInput,
    pub autocomplete: Option<AutocompleteState>,
    pub scroll_y: f32,
    /// Auto-scroll k poslednimu radku pri prichodu noveho logu.
    pub stick_to_bottom: bool,
}

impl ConsoleState {
    pub fn push_log(&mut self, entry: LogEntry) {
        self.log.push(entry);
        if self.log.len() > 1000 {
            self.log.remove(0);
        }
        self.stick_to_bottom = true;
    }
}

#[derive(Debug, Default)]
pub struct NetworkState {
    pub entries: Vec<NetworkEntry>,
    pub filter: NetworkFilter,
    pub selected: Option<usize>,
    pub scroll_y: f32,
    /// True kdyz user kliknul na row - zobrazit detail popup.
    pub detail_open: bool,
}

impl Default for NetworkFilter {
    fn default() -> Self { NetworkFilter::All }
}
