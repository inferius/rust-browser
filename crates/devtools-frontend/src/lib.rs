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

/// Lucide icon library - dotahne SVG icons z `<i data-lucide="name">`.
/// Bundled lokalne (z unpkg.com lucide@latest), bez runtime CDN dep.
///
/// DEPRECATED: lucide.js pouziva object spread `{...obj}` (ES2018) ktery
/// nas JS parser NEPODPORUJE. Pres icons rendered server-side pres
/// LUCIDE_ICONS map + replace_lucide_placeholders helper.
pub const LUCIDE_JS: &str = include_str!("../static/lucide.js");

/// Pre-loaded lucide SVG ikony - klic = data-lucide name, hodnota = SVG body
/// (vc `<svg>` tagu). Pres render replace `<i data-lucide="x">...</i>` -> SVG
/// inline. Bez JS createIcons() = no parser dep.
pub fn lucide_svg(name: &str) -> Option<&'static str> {
    match name {
        "accessibility"     => Some(include_str!("../static/lucide/accessibility.svg")),
        "bug"               => Some(include_str!("../static/lucide/bug.svg")),
        "chevron-down"      => Some(include_str!("../static/lucide/chevron-down.svg")),
        "chevrons-right"    => Some(include_str!("../static/lucide/chevrons-right.svg")),
        "circle"            => Some(include_str!("../static/lucide/circle.svg")),
        "database"          => Some(include_str!("../static/lucide/database.svg")),
        "gauge"             => Some(include_str!("../static/lucide/gauge.svg")),
        "globe"             => Some(include_str!("../static/lucide/globe.svg")),
        "hard-drive"        => Some(include_str!("../static/lucide/hard-drive.svg")),
        "info"              => Some(include_str!("../static/lucide/info.svg")),
        "layout-panel-left" => Some(include_str!("../static/lucide/layout-panel-left.svg")),
        "more-horizontal"   => Some(include_str!("../static/lucide/more-horizontal.svg")),
        "paintbrush"        => Some(include_str!("../static/lucide/paintbrush.svg")),
        "plus"              => Some(include_str!("../static/lucide/plus.svg")),
        "rotate-cw"         => Some(include_str!("../static/lucide/rotate-cw.svg")),
        "settings"          => Some(include_str!("../static/lucide/settings.svg")),
        "smartphone"        => Some(include_str!("../static/lucide/smartphone.svg")),
        "terminal"          => Some(include_str!("../static/lucide/terminal.svg")),
        "trash-2"           => Some(include_str!("../static/lucide/trash-2.svg")),
        "x"                 => Some(include_str!("../static/lucide/x.svg")),
        _ => None,
    }
}

/// Replace `<i data-lucide="ICON_NAME" class="...">...</i>` patterns v HTML
/// s pre-loaded SVG markup. Pres host volá pri build_devtools_html.
pub fn replace_lucide_placeholders(html: &str) -> String {
    // Simple regex-free walker. Find `<i data-lucide="`, extract icon name +
    // class attr, replace cely `<i ...></i>` blok s SVG.
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<i data-lucide=\"") {
        out.push_str(&rest[..start]);
        // After `<i data-lucide="`, najit closing `"`.
        let after_lucide = &rest[start + "<i data-lucide=\"".len()..];
        let name_end = match after_lucide.find('"') {
            Some(p) => p,
            None => { out.push_str(&rest[start..]); break; }
        };
        let icon_name = &after_lucide[..name_end];
        // Najit `>` (= konec opening tagu)
        let after_quote = &after_lucide[name_end + 1..];
        let close_gt = match after_quote.find('>') {
            Some(p) => p,
            None => { out.push_str(&rest[start..]); break; }
        };
        // Extrakt attributes mezi name_end+1 a close_gt (jako class="...").
        let attrs_str = &after_quote[..close_gt];
        // Najit `</i>` po `<i ...>`.
        let body_start = after_quote.len().min(close_gt + 1);
        let after_open = &after_quote[body_start..];
        let close_i = match after_open.find("</i>") {
            Some(p) => p,
            None => { out.push_str(&rest[start..]); break; }
        };
        // Get SVG for this icon. Fallback = empty SVG square pri unknown name.
        let svg_full = lucide_svg(icon_name).unwrap_or(
            "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\"></svg>"
        );
        // Inject attrs do svg opening tag - replace prvni `<svg` -> `<svg{attrs_str}`.
        // Aby `class="icon"` from `<i>` se aplikoval na SVG.
        let injected = if let Some(open_end) = svg_full.find("<svg") {
            let mut s = String::with_capacity(svg_full.len() + attrs_str.len() + 4);
            s.push_str(&svg_full[..open_end + 4]);
            s.push_str(attrs_str);
            s.push_str(&svg_full[open_end + 4..]);
            s
        } else {
            svg_full.to_string()
        };
        out.push_str(&injected);
        // Move past `</i>` (4 chars).
        let consumed = start + "<i data-lucide=\"".len() + name_end + 1 + close_gt + 1 + close_i + 4;
        rest = &rest[consumed..];
    }
    out.push_str(rest);
    out
}

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
