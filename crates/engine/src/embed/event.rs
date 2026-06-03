//! Vstupni eventy + odpovedi pro embeddable WebView.
//!
//! Engine NEVI o winit ani konkretni window backend - hostujici aplikace
//! prevadi sve eventy do tehto neutralnich variant. Shell crate ma helpery
//! z `winit::WindowEvent` (pridane v Phase 4).

use std::path::PathBuf;

/// Modifiers state - shift/ctrl/alt/super. WebView vidi co bylo zmacnnuto v dobe
/// eventu. Hostujici aplikace si drzi vlastni state machine + plni tohle struct.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Cmd na macOS, Win key na Windows, Super na Linuxu.
    pub meta: bool,
}

/// Tlacitka mysi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    /// Back / Forward thumb buttons + jakkoli dalsi.
    Other(u16),
}

/// Neutralni input event. Hostujici aplikace mapuje winit/inou knihovnu sem.
///
/// Pozice (x, y) jsou v **CSS px** (logical), ne physical. Hostujici aplikace
/// si zvladne HiDPI scale_factor div pred dorucenim.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Mys se posunula. Coords v viewport-relativnich CSS px.
    /// `coalesced` = predchozi raw mouse positions ktere host slouci do toho
    /// dispatch (pres frame slot). Empty pri single event. PointerEvent JS API
    /// `getCoalescedEvents()` cte tento seznam = umoznuje JS dostat full-rate
    /// raw events pres single dispatch (drawing apps, hry).
    MouseMove { x: f32, y: f32, modifiers: KeyModifiers, coalesced: Vec<(f32, f32)> },
    /// Mys click down. Coords v CSS px.
    MouseDown { x: f32, y: f32, button: MouseButton, modifiers: KeyModifiers },
    /// Mys click up. Coords v CSS px.
    MouseUp { x: f32, y: f32, button: MouseButton, modifiers: KeyModifiers },
    /// Mys opustila viewport (hover state musi byt cleared).
    MouseLeave,
    /// Scroll wheel delta (CSS px). Trackpad = pixel-precise; mouse wheel
    /// hostujici aplikace prevadi z line-based.
    Scroll { dx: f32, dy: f32, x: f32, y: f32, modifiers: KeyModifiers },
    /// Klavesa stisknuta. `key` je logicky nazev ("Enter", "ArrowLeft", "a", ...).
    /// Hostujici aplikace je odpovedna za key mapping (winit::Key -> nas string).
    KeyDown { key: String, modifiers: KeyModifiers },
    /// Klavesa pustena.
    KeyUp { key: String, modifiers: KeyModifiers },
    /// Tisknutelne char input (IME / dead keys / repeat). Jeden grapheme cluster.
    TextInput { text: String },
    /// Focus do/z viewport.
    FocusChanged { focused: bool },
    /// Viewport resize. Volat samostatne pres `WebView::resize` - InputEvent
    /// varianta je pro shell-routed pripady (input bar resize triggernuti relayoutu).
    Resize { width: u32, height: u32, scale_factor: f32 },
}

/// Odpoved z `WebView::handle_input`. Pro koordinaci se shell hostem
/// (kdy treba prekreslit, jestli event chce navigaci, atd.).
#[derive(Debug, Clone, Default)]
pub struct EventResponse {
    /// WebView se zmenil - shell musi reinvokovat `WebView::render` a prekompozit.
    pub dirty: bool,
    /// JS pozadal o navigaci (window.location, form submit, anchor click). Shell
    /// rozhodne jestli povolit / zamenit URL bar / odhodit do noveho tabu.
    pub navigation: Option<NavigationRequest>,
    /// Cursor shape ktery shell pouzije nad viewport (CSS `cursor`).
    pub cursor: Option<CursorIcon>,
    /// JS zaznamenal `console.log` - shell ho muze ukazat v devtools panelu.
    pub new_console_logs: bool,
    /// Stranka nahlasila title (`document.title = ...`) - shell aktualizuje tab.
    pub title_changed: Option<String>,
}

/// Pozadavek na navigaci (anchor click, form submit, JS).
#[derive(Debug, Clone)]
pub struct NavigationRequest {
    pub url: String,
    pub method: NavigationMethod,
    pub body: Option<Vec<u8>>,
    pub target: NavigationTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationTarget {
    /// Nahradit aktualni page (default).
    Self_,
    /// Otevrit v novem tab (target="_blank").
    NewTab,
    /// Named frame target (zatim treated jako Self_).
    Named(String),
}

/// Vysledek `WebView::load_url` / `load_html`.
#[derive(Debug, Clone)]
pub struct NavigationResult {
    /// Final URL (po HTTP redirectech, ...).
    pub url: String,
    /// HTTP status pokud sla pres http(s); 0 pro file://.
    pub status: u16,
    /// Pocet stylesheets nactenych / fetched.
    pub stylesheet_count: usize,
    /// Local file path pokud sla pres file:// (pro relative resolve).
    pub local_path: Option<PathBuf>,
}

/// CSS cursor shape names ktere WebView signaluje zpet shellu.
/// Subset CSS L3 cursor property - hostujici aplikace mapuje na sve enum
/// (winit::CursorIcon, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorIcon {
    Default,
    Pointer,
    Text,
    Wait,
    Help,
    Crosshair,
    Move,
    NotAllowed,
    Grab,
    Grabbing,
    /// CSS `col-resize` / `row-resize` / `ew-resize` / `ns-resize`.
    ResizeEw,
    ResizeNs,
    ResizeNesw,
    ResizeNwse,
}

impl Default for CursorIcon {
    fn default() -> Self { CursorIcon::Default }
}
