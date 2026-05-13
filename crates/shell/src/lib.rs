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

// Re-export embed API pro convenience (uzivatel shellu sahnae do enginu).
pub use rwe_engine::embed::{Engine, EventResponse, InputEvent, WebView};
