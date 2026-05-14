//! RustWebEngine DevTools Frontend (D3).
//!
//! Static HTML/CSS/JS pages bundled jako `&'static str` resources. Druhy
//! WebView v shell aplikaci (D4) load_html(devtools_frontend::INDEX_HTML)
//! je rendruje. Komunikace s engine target adapterem (D2) probiha pres
//! `window.cdp.send(method, params)` JS API (D6 - JS binding).
//!
//! ## Pages
//!
//! - `INDEX_HTML` - root layout s tab strip + nested iframe pro vybrany panel
//! - `ELEMENTS_HTML` - DOM tree + Styles + Computed
//! - `CONSOLE_HTML` - log + input prompt
//! - `SOURCES_HTML` - source listing + breakpoints + call stack
//! - `NETWORK_HTML` - request table + filter
//! - `PERFORMANCE_HTML` - frame metrics graf
//!
//! ## API
//!
//! ```text
//! window.cdp.send(method, params) -> Promise<Result | Error>
//! window.cdp.on(event_name, callback) -> register listener
//! ```
//!
//! Implementace JS API budou pres native binding v interpreter (D6) -
//! `cdp.send` blokujici dispatchne pres DevtoolsTarget v same-process scenario.

/// Root index.html - tab strip layout + container pro nested panel.
pub const INDEX_HTML: &str = include_str!("../static/index.html");

/// Elements panel - DOM tree + Styles pane.
pub const ELEMENTS_HTML: &str = include_str!("../static/elements.html");

/// Console panel - log scrollback + input.
pub const CONSOLE_HTML: &str = include_str!("../static/console.html");

/// Sources panel - file listing + breakpoints.
pub const SOURCES_HTML: &str = include_str!("../static/sources.html");

/// Network panel - request table.
pub const NETWORK_HTML: &str = include_str!("../static/network.html");

/// Performance panel - metrics chart.
pub const PERFORMANCE_HTML: &str = include_str!("../static/performance.html");

/// Shared CSS theme - dark Chrome devtools-style.
pub const THEME_CSS: &str = include_str!("../static/theme.css");

/// CDP JS client - window.cdp.send/on wrapper okolo native binding.
pub const CDP_JS: &str = include_str!("../static/cdp.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_not_empty() {
        assert!(!INDEX_HTML.is_empty());
        assert!(!ELEMENTS_HTML.is_empty());
        assert!(!CONSOLE_HTML.is_empty());
        assert!(!SOURCES_HTML.is_empty());
        assert!(!NETWORK_HTML.is_empty());
        assert!(!PERFORMANCE_HTML.is_empty());
        assert!(!THEME_CSS.is_empty());
        assert!(!CDP_JS.is_empty());
    }

    #[test]
    fn index_references_panels() {
        // Index.html ma navigaci na vsechny panely.
        assert!(INDEX_HTML.contains("Elements"));
        assert!(INDEX_HTML.contains("Console"));
        assert!(INDEX_HTML.contains("Sources"));
        assert!(INDEX_HTML.contains("Network"));
    }

    #[test]
    fn cdp_js_exposes_send() {
        assert!(CDP_JS.contains("send"));
        assert!(CDP_JS.contains("cdp"));
    }
}
