//! RustWebEngine DevTools Frontend (D3 + B5 redesign).
//!
//! Single-file Firefox-like Theme + i18n (cs/en) + Lucide ikony + 3-column
//! Inspector + Animations panel. Druhy WebView v shell aplikaci (D4)
//! `load_html(devtools_frontend::INDEX_HTML)` ho rendruje. Komunikace s
//! engine target adapterem (D2) probiha pres `window.cdp.send(method, params)`
//! JS API (D6 - JS binding).
//!
//! ## Layout
//!
//! INDEX_HTML obsahuje vsechny panely (Inspector / Console / Debugger /
//! Network / Style Editor / Performance / Memory / Storage / Accessibility /
//! Application) v jednom HTML. Tab strip pres `data-panel` attributes prepiná
//! display:none/none. Style sheets inline v `<style>` bloku.
//!
//! ## API
//!
//! ```text
//! window.cdp.send(method, params) -> Promise<Result | Error>
//! window.cdp.on(event_name, callback) -> register listener
//! ```
//!
//! CDP JS klient (cdp.js) se injectne do `<script id="cdp-js"></script>` placeholderu
//! v shell::build_devtools_html().

/// Single-file devtools frontend HTML (Firefox-like dark theme + i18n).
pub const INDEX_HTML: &str = include_str!("../static/index.html");

/// CDP JS client - window.cdp.send/on wrapper okolo native binding.
pub const CDP_JS: &str = include_str!("../static/cdp.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_not_empty() {
        assert!(!INDEX_HTML.is_empty());
        assert!(!CDP_JS.is_empty());
    }

    #[test]
    fn index_references_panels() {
        // Index.html ma navigaci na vsechny panely.
        assert!(INDEX_HTML.contains("data-panel=\"inspector\""));
        assert!(INDEX_HTML.contains("data-panel=\"console\""));
        assert!(INDEX_HTML.contains("data-panel=\"debugger\""));
        assert!(INDEX_HTML.contains("data-panel=\"network\""));
    }

    #[test]
    fn cdp_js_exposes_send() {
        assert!(CDP_JS.contains("send"));
        assert!(CDP_JS.contains("cdp"));
    }

    #[test]
    fn index_has_cdp_placeholder() {
        // Shell musi mit kam vlozit cdp.js.
        assert!(INDEX_HTML.contains("<script id=\"cdp-js\""));
    }
}
