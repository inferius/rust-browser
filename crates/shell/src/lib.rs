//! rwe-shell: Browser chrome (tabs, address bar, find bar, bookmarks bar,
//! history, devtools toggle) postavene nad `rwe-engine` rendererem.
//!
//! # Architektura
//!
//! Shell vlastni svoje winit Window + wgpu Surface. Drzi `Arc<Engine>` (sdilene
//! GPU resources) + `Vec<WebView>` (1 per tab). Aktivni WebView renderuje do
//! offscreen texture, shell kompozituje s chrome UI do swap chain.
//!
//! Tohle je Edge/Chromium-like model: shell = host UI, engine = embeddable
//! content view (jako WebView2 / WKWebView / Servo WebView).
//!
//! # Stav
//!
//! Phase 1: shell bin = thin forwarder na engine::run_cli (`browser` mode).
//! Phase 2: embed API kontrakt v engine k dispozici, shell ho zatim nevyuziva.
//! Phase 3-5: postupne migrace state z engine `App` do nasich `ShellState` +
//! `WebView`. Phase 5 = finalni compositor.

/// Verze shell crate. Pouzite v address bar UA string Phase 4+.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod app;

// Re-export embed API pro convenience (uzivatel shellu sahnae do enginu).
pub use rwe_engine::embed::{Engine, EventResponse, InputEvent, WebView};

use std::path::PathBuf;

/// Spusti shell-driven okno. Shell vlastni Window + Renderer + WebView a
/// renderuje stranku pres `WebView::render_via` -> offscreen RT ->
/// `Renderer::present_external_to_swap_chain` -> swap chain.
///
/// Phase 4c step 3 stav: bez chrome bar (no tabs/addr/find/bookmarks).
/// Validacni cesta - dokazuje ze shell crate je samostatny host enginu.
/// Chrome UI prijde v Phase 5+.
pub fn run_window(
    html: String,
    css: String,
    base_url: Option<String>,
    local_path: Option<PathBuf>,
) -> Result<(), String> {
    // Shell bin spousti UI v dedikovanem worker threadu (256 MB stack
    // pro hluboky JS interp recursion). winit 0.30 strict main-thread
    // check - pres `any_thread(true)` opt-out na Windows.
    #[cfg(target_os = "windows")]
    let event_loop = {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .map_err(|e| format!("event_loop: {e}"))?
    };
    #[cfg(not(target_os = "windows"))]
    let event_loop = winit::event_loop::EventLoop::new()
        .map_err(|e| format!("event_loop: {e}"))?;
    let mut app = app::ShellApp::new(html, css, base_url, local_path);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}
