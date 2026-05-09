//! Right-click kontextove menu - sdilena infrastruktura per-tab.

#[derive(Debug, Clone)]
pub enum MenuItem {
    Action {
        label: String,
        action: MenuAction,
        enabled: bool,
        shortcut: Option<String>,
    },
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    // Shell tab/bookmark context menu
    TabClose(usize),
    TabCloseOthers(usize),
    TabDuplicate(usize),
    TabReload(usize),
    TabPinToggle(usize),
    /// Pridat/odebrat tab do skupiny - color [r,g,b,a] nebo None (clear).
    TabSetGroup(usize, Option<[u8; 4]>),
    BookmarkOpen(String),
    BookmarkDelete(String),

    // Elements tab
    EditAttribute { node_id: usize, attr: String },
    AddAttribute { node_id: usize },
    DeleteElement { node_id: usize },
    DuplicateElement { node_id: usize },
    CopyOuterHtml { node_id: usize },
    CopyInnerHtml { node_id: usize },
    CopySelector { node_id: usize },
    CopyXPath { node_id: usize },
    ScrollIntoView { node_id: usize },
    ExpandAll { node_id: usize },
    CollapseAll { node_id: usize },
    BreakOnSubtreeMod { node_id: usize },
    BreakOnAttrMod { node_id: usize },
    BreakOnRemoval { node_id: usize },

    // Console tab
    Copy,
    SelectAll,
    Cut,
    Paste,
    ClearConsole,
    SaveConsoleAs,
    CopyLogEntry { idx: usize },
    DeleteLogEntry { idx: usize },

    // Network tab
    CopyUrl { idx: usize },
    CopyAsCurl { idx: usize },
    Replay { idx: usize },
    BlockUrl { idx: usize },
    CopyResponse { idx: usize },

    // Sources tab
    AddBreakpoint { file_id: u32, line: u32 },
    AddConditionalBreakpoint { file_id: u32, line: u32 },
    DisableAllBreakpoints,
    RemoveAllBreakpoints,
    ContinueToHere { file_id: u32, line: u32 },
    RevealInSources { file_id: u32 },

    // General
    Custom { id: String },
}

#[derive(Debug, Clone)]
pub struct ContextMenuState {
    pub x: f32,
    pub y: f32,
    pub items: Vec<MenuItem>,
    pub hovered: Option<usize>,
}

impl ContextMenuState {
    pub fn new(x: f32, y: f32, items: Vec<MenuItem>) -> Self {
        ContextMenuState { x, y, items, hovered: None }
    }

    /// Idx -> realne MenuItem index, preskakuje separators.
    pub fn action_at(&self, idx: usize) -> Option<&MenuAction> {
        match self.items.get(idx)? {
            MenuItem::Action { action, enabled, .. } if *enabled => Some(action),
            _ => None,
        }
    }
}

// ─── Builders pro per-tab menu ──────────────────────────────────────────

pub fn elements_row_menu(node_id: usize) -> Vec<MenuItem> {
    vec![
        action("Edit as HTML", MenuAction::EditAttribute { node_id, attr: "*outer*".into() }),
        action("Add attribute", MenuAction::AddAttribute { node_id }),
        MenuItem::Separator,
        action("Copy outerHTML", MenuAction::CopyOuterHtml { node_id }),
        action("Copy innerHTML", MenuAction::CopyInnerHtml { node_id }),
        action("Copy selector", MenuAction::CopySelector { node_id }),
        action("Copy XPath", MenuAction::CopyXPath { node_id }),
        MenuItem::Separator,
        action("Duplicate element", MenuAction::DuplicateElement { node_id }),
        action("Delete element", MenuAction::DeleteElement { node_id }),
        action("Scroll into view", MenuAction::ScrollIntoView { node_id }),
        MenuItem::Separator,
        action("Expand all", MenuAction::ExpandAll { node_id }),
        action("Collapse all", MenuAction::CollapseAll { node_id }),
        MenuItem::Separator,
        action("Break on subtree modifications", MenuAction::BreakOnSubtreeMod { node_id }),
        action("Break on attribute modifications", MenuAction::BreakOnAttrMod { node_id }),
        action("Break on node removal", MenuAction::BreakOnRemoval { node_id }),
    ]
}

pub fn console_text_menu() -> Vec<MenuItem> {
    vec![
        action("Cut", MenuAction::Cut),
        action("Copy", MenuAction::Copy),
        action("Paste", MenuAction::Paste),
        MenuItem::Separator,
        action("Select all", MenuAction::SelectAll),
        MenuItem::Separator,
        action("Clear console", MenuAction::ClearConsole),
        action("Save as...", MenuAction::SaveConsoleAs),
    ]
}

pub fn console_log_menu(idx: usize) -> Vec<MenuItem> {
    vec![
        action("Copy", MenuAction::CopyLogEntry { idx }),
        MenuItem::Separator,
        action("Delete entry", MenuAction::DeleteLogEntry { idx }),
        action("Clear console", MenuAction::ClearConsole),
        action("Save as...", MenuAction::SaveConsoleAs),
    ]
}

pub fn network_row_menu(idx: usize) -> Vec<MenuItem> {
    vec![
        action("Copy URL", MenuAction::CopyUrl { idx }),
        action("Copy as cURL", MenuAction::CopyAsCurl { idx }),
        action("Copy response", MenuAction::CopyResponse { idx }),
        MenuItem::Separator,
        action("Replay request", MenuAction::Replay { idx }),
        action("Block this URL", MenuAction::BlockUrl { idx }),
    ]
}

pub fn sources_line_menu(file_id: u32, line: u32) -> Vec<MenuItem> {
    vec![
        action("Add breakpoint", MenuAction::AddBreakpoint { file_id, line }),
        action("Add conditional breakpoint...", MenuAction::AddConditionalBreakpoint { file_id, line }),
        MenuItem::Separator,
        action("Continue to here", MenuAction::ContinueToHere { file_id, line }),
        MenuItem::Separator,
        action("Disable all breakpoints", MenuAction::DisableAllBreakpoints),
        action("Remove all breakpoints", MenuAction::RemoveAllBreakpoints),
    ]
}

fn action(label: &str, a: MenuAction) -> MenuItem {
    MenuItem::Action {
        label: label.to_string(),
        action: a,
        enabled: true,
        shortcut: None,
    }
}
