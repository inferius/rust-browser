//! rwe-shell: Browser chrome (tabs, address bar, find bar, bookmarks bar,
//! history, devtools toggle) postavene nad `rwe-engine` rendererem.
//!
//! V Phase 1 je tahle crate pouzhy skelet - shell kod stale zije v engine
//! crate (`browser::render` + `browser::shell_chrome`). Phase 3-5 ho sem
//! presunou: paint, state, input handling, kompozice (engine RT + chrome RT).

/// Verze shell crate. Pouzite v address bar UA string Phase 4+.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
