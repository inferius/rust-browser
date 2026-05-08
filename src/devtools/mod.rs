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

use std::collections::HashSet;
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
        &[
            Tab::Elements, Tab::Console, Tab::Network,
            Tab::Sources, Tab::Performance, Tab::Application, Tab::Settings,
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
}

#[derive(Debug, Default)]
pub struct ElementsSearch {
    pub open: bool,
    pub query: String,
    pub cursor: usize,
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
}

impl Default for NetworkFilter {
    fn default() -> Self { NetworkFilter::All }
}
