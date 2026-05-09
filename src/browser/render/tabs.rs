//! Multi-tab state pro shell mode. Kazdy tab drzi vlastni Document URL +
//! scroll state + history. Pri switch_to(idx) se aktivni tab nahraje a
//! zbyle se zachovaji (deferred load - kazdy tab ma svuj snapshot html/css).
//!
//! Pro minimum viable: tab zna jen URL/path + cached html/css. Aktivni
//! tab se loaduje pri switch (potrebuje re-parse). Future: per-tab
//! interpreter + document instance + layout cache pro fast switch.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Tab {
    pub url: Option<String>,
    pub path: Option<PathBuf>,
    pub html: String,
    pub css: String,
    pub title: String,
    pub favicon_url: Option<String>,
    /// Cached favicon bytes (PNG/ICO/SVG). Loaded async on tab create.
    pub favicon_bytes: Option<Vec<u8>>,
    pub scroll_y: f32,
    pub scroll_x: f32,
    pub history: Vec<String>,
    pub history_idx: usize,
    /// Per-tab Document - sdileny clone Rc na tab swap. None = pred prvni
    /// load (re-parse z html). Some = kesovany doc s JS state pro fast switch.
    pub document_root: Option<std::rc::Rc<crate::browser::dom::NodeData>>,
    /// Pinned tab - menci sirka, prvni v poradi, nejde zavrit krome unpinu.
    pub pinned: bool,
    /// Loading state - pri navigate na URL true, po dokonceni false.
    /// Vyuzite pro busy indicator v tab chip.
    pub loading: bool,
}

impl Tab {
    pub fn new(html: String, css: String, url: Option<String>, path: Option<PathBuf>) -> Self {
        // Title prefer z <title>...</title>, fallback URL last segment.
        let title = extract_title(&html).unwrap_or_else(|| {
            url.clone()
                .map(|u| u.split('/').last().unwrap_or(&u).to_string())
                .unwrap_or_else(|| "Nova zalozka".to_string())
        });
        let favicon_url = url.as_ref().map(|u| derive_favicon_url(u, &html));
        // Fetch favicon bytes (sync). HTTP only; file:// URL = skip.
        let favicon_bytes = favicon_url.as_ref()
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
            .and_then(|u| crate::browser::render::fetch_image_bytes(u));
        Self {
            url, path, html, css, title, favicon_url,
            favicon_bytes,
            scroll_y: 0.0, scroll_x: 0.0,
            history: Vec::new(),
            history_idx: 0,
            document_root: None,
            pinned: false,
            loading: false,
        }
    }

    pub fn empty() -> Self {
        let (html, css) = render_about_newtab();
        Self {
            url: Some("about:newtab".to_string()),
            path: None,
            html, css,
            title: "Nova zalozka".to_string(),
            favicon_url: None,
            favicon_bytes: None,
            scroll_y: 0.0, scroll_x: 0.0,
            history: Vec::new(),
            history_idx: 0,
            document_root: None,
            pinned: false,
            loading: false,
        }
    }
}

/// Render about:newtab page s top-sites z history + bookmarks shortcuts.
pub fn render_about_newtab() -> (String, String) {
    let history = crate::devtools::history::load_history();
    let bookmarks = crate::devtools::bookmarks::load_bookmarks();
    // Top 8 nejnavstevovanejsich URL z history (count occurrences).
    let mut counts: std::collections::HashMap<String, (u32, String)> = std::collections::HashMap::new();
    for h in &history {
        let entry = counts.entry(h.url.clone()).or_insert((0, h.title.clone()));
        entry.0 += 1;
    }
    let mut top: Vec<(String, u32, String)> = counts.into_iter()
        .map(|(u, (c, t))| (u, c, t))
        .collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    let top_8: Vec<&(String, u32, String)> = top.iter().take(8).collect();
    let recent_section = if top_8.is_empty() {
        "<p class=hint>Zadna historie - po navstivenim stranek se zobrazi top sites</p>".to_string()
    } else {
        let cards = top_8.iter().map(|(url, _, title)| {
            let title_short: String = title.chars().take(24).collect();
            format!("<a href=\"{}\" class=card><h3>{}</h3><p>{}</p></a>",
                    html_escape_local(url),
                    html_escape_local(&title_short),
                    html_escape_local(&url.chars().take(40).collect::<String>()))
        }).collect::<Vec<_>>().join("\n");
        format!("<h2>Top sites</h2><div class=cards>{}</div>", cards)
    };
    let bm_section = if bookmarks.is_empty() {
        String::new()
    } else {
        let chips = bookmarks.iter().take(20).map(|b|
            format!("<a href=\"{}\" class=chip>{}</a>",
                    html_escape_local(&b.url), html_escape_local(&b.title))
        ).collect::<Vec<_>>().join("\n");
        format!("<h2>Zalozky</h2><div class=chips>{}</div>", chips)
    };
    let html = format!(r#"<!DOCTYPE html>
<html><head><title>Nova zalozka</title></head>
<body>
<div class=container>
<h1>Rust Web Engine</h1>
<p class=subtitle>Vlastni prohlizec, vlastni renderovaci jadro.</p>
{recent}
{bms}
<h2>Stranky</h2>
<div class=cards>
<a href="about:config" class=card><h3>Nastaveni</h3><p>Profil, dock, theme</p></a>
<a href="about:history" class=card><h3>Historie</h3><p>Vsechny navstevy</p></a>
<a href="about:bookmarks" class=card><h3>Zalozky</h3><p>Ulozene odkazy</p></a>
</div>
<p class=hint>Ctrl+L adresa, Ctrl+T novy tab, Ctrl+W zavrit, F12 devtools, Ctrl+D bookmark</p>
</div>
</body></html>"#, recent = recent_section, bms = bm_section);
    let css = r#"
body { font-family: 'Inter', sans-serif; background: #1a1a1f; color: #e8e6df; margin: 0; padding: 0; }
.container { max-width: 900px; margin: 60px auto; padding: 40px; }
h1 { color: #69a1ff; font-size: 48px; margin-bottom: 16px; text-align: center; }
h2 { color: #94de7c; font-size: 18px; margin-top: 32px; margin-bottom: 12px; }
.subtitle { color: #a1a1ae; font-size: 16px; margin-bottom: 32px; text-align: center; }
.cards { display: grid; grid-template-columns: 1fr 1fr 1fr 1fr; gap: 12px; }
.card { background: #2a2932; padding: 16px; border-radius: 8px; border: 1px solid #4c4c55; text-decoration: none; display: block; }
.card:hover { background: #383744; border-color: #69a1ff; }
.card h3 { color: #69a1ff; margin-top: 0; margin-bottom: 6px; font-size: 14px; }
.card p { color: #a1a1ae; font-size: 11px; margin: 0; }
.chips { display: flex; flex-wrap: wrap; gap: 8px; }
.chip { background: #2a2932; padding: 6px 12px; border-radius: 16px; color: #e8e6df; text-decoration: none; font-size: 13px; }
.chip:hover { background: #69a1ff; color: white; }
.hint { color: #6d6d7c; font-size: 12px; margin-top: 32px; font-style: italic; text-align: center; }
"#;
    (html, css.to_string())
}

fn html_escape_local(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
     .replace('"', "&quot;")
}

/// Najdi favicon URL: <link rel="icon" href="...">, fallback /favicon.ico.
/// Extrahuj <title>...</title> z HTML (case-insensitive, prvni vyskyt).
/// None pokud chybi nebo je prazdny.
fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let tag_end = lower[start..].find('>').map(|e| start + e + 1)?;
    let close = lower[tag_end..].find("</title>").map(|e| tag_end + e)?;
    let raw = html[tag_end..close].trim();
    if raw.is_empty() { None } else { Some(raw.to_string()) }
}

fn derive_favicon_url(base_url: &str, html: &str) -> String {
    // Naivni parse <link rel="icon" href="...">.
    let lower = html.to_lowercase();
    let mut idx = 0;
    while let Some(off) = lower[idx..].find("<link") {
        let start = idx + off;
        let end = lower[start..].find('>').map(|e| start + e).unwrap_or(html.len());
        let tag = &lower[start..end];
        if tag.contains("rel=\"icon\"") || tag.contains("rel='icon'")
           || tag.contains("rel=\"shortcut icon\"") {
            // Extract href.
            if let Some(h) = tag.find("href=") {
                let after = &tag[h + 5..];
                let q = after.chars().next().unwrap_or('"');
                if q == '"' || q == '\'' {
                    let after2 = &after[1..];
                    if let Some(close) = after2.find(q) {
                        let href = &html[start + h + 6 .. start + h + 6 + close];
                        return resolve_favicon(base_url, href);
                    }
                }
            }
        }
        idx = end + 1;
    }
    // Fallback /favicon.ico.
    resolve_favicon(base_url, "/favicon.ico")
}

fn resolve_favicon(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        let scheme = base.split(":").next().unwrap_or("https");
        return format!("{}:{}", scheme, href);
    }
    if href.starts_with('/') {
        // Absolute path - vezmi base origin.
        if let Some(scheme_end) = base.find("://") {
            let after_scheme = &base[scheme_end + 3..];
            let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
            return format!("{}://{}{}", &base[..scheme_end], &after_scheme[..host_end], href);
        }
    }
    // Relative - append k base.
    let base_dir = base.rsplit('/').nth(0).map(|_| {
        let mut b = base.to_string();
        if !b.ends_with('/') {
            if let Some(p) = b.rfind('/') {
                b.truncate(p + 1);
            }
        }
        b
    }).unwrap_or_else(|| base.to_string());
    format!("{}{}", base_dir, href)
}

/// Render about:history page - cely seznam navstivenych URL.
pub fn render_about_history() -> (String, String) {
    render_about_history_filtered("")
}

pub fn render_about_history_filtered(query: &str) -> (String, String) {
    let history = crate::devtools::history::load_history();
    let q_low = query.to_lowercase();
    let filtered: Vec<&crate::devtools::history::HistoryEntry> = if q_low.is_empty() {
        history.iter().collect()
    } else {
        history.iter().filter(|h|
            h.url.to_lowercase().contains(&q_low) || h.title.to_lowercase().contains(&q_low)
        ).collect()
    };
    let total = filtered.len();
    let rows = if filtered.is_empty() {
        format!("<tr><td colspan=2 class='empty'>Zadna {} polozka</td></tr>",
                if q_low.is_empty() { "historie" } else { "shoda" })
    } else {
        filtered.iter().rev().take(500).map(|h| {
            let date = format_ts(h.visited_at);
            format!("<tr><td><a href=\"{}\">{}</a></td><td class=date>{}</td></tr>",
                    html_escape(&h.url), html_escape(&h.title), date)
        }).collect::<Vec<_>>().join("\n")
    };
    let html = format!(r#"<!DOCTYPE html><html><head><title>Historie</title></head>
<body>
<div class=cfg>
<h1>Historie</h1>
<p class=subtitle>{total} polozek (max 500 zobrazeno)</p>
<table>
<thead><tr><th>Stranka</th><th>Cas</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
</div>
</body></html>"#, total = total, rows = rows);
    let css = r#"
body { font-family: 'Inter', sans-serif; background: #1a1a1f; color: #e8e6df; margin: 0; padding: 32px; }
.cfg { max-width: 900px; margin: 0 auto; }
h1 { color: #69a1ff; font-size: 32px; }
.subtitle { color: #a1a1ae; font-size: 14px; margin-bottom: 24px; }
table { width: 100%; border-collapse: collapse; }
th { text-align: left; color: #94de7c; padding: 8px; border-bottom: 1px solid #4c4c55; }
td { padding: 8px; border-bottom: 1px solid #2a2932; }
td a { color: #69a1ff; text-decoration: none; }
td a:hover { text-decoration: underline; }
td.date { color: #a1a1ae; font-size: 12px; width: 200px; }
.empty { color: #a1a1ae; font-style: italic; text-align: center; padding: 24px; }
"#;
    (html, css.to_string())
}

/// Render about:bookmarks page.
pub fn render_about_bookmarks() -> (String, String) {
    let bookmarks = crate::devtools::bookmarks::load_bookmarks();
    let rows = if bookmarks.is_empty() {
        "<li class='empty'>Zadne zalozky - Ctrl+D na strance je prida</li>".to_string()
    } else {
        bookmarks.iter().map(|b|
            format!("<li><a href=\"{}\">{}</a> <small>{}</small></li>",
                    html_escape(&b.url), html_escape(&b.title), html_escape(&b.url))
        ).collect::<Vec<_>>().join("\n")
    };
    let html = format!(r#"<!DOCTYPE html><html><head><title>Zalozky</title></head>
<body>
<div class=cfg>
<h1>Zalozky</h1>
<ul>{rows}</ul>
</div>
</body></html>"#, rows = rows);
    let css = r#"
body { font-family: 'Inter', sans-serif; background: #1a1a1f; color: #e8e6df; margin: 0; padding: 32px; }
.cfg { max-width: 800px; margin: 0 auto; }
h1 { color: #69a1ff; font-size: 32px; margin-bottom: 16px; }
ul { list-style: none; padding: 0; }
li { background: #2a2932; padding: 12px 16px; margin-bottom: 4px; border-radius: 6px; }
li a { color: #69a1ff; text-decoration: none; font-weight: 600; }
li a:hover { text-decoration: underline; }
li small { color: #a1a1ae; margin-left: 8px; font-size: 12px; }
.empty { color: #a1a1ae; font-style: italic; }
"#;
    (html, css.to_string())
}

fn format_ts(ts: u64) -> String {
    if ts == 0 { return String::new(); }
    let now = crate::devtools::history::now_ts();
    let age = now.saturating_sub(ts);
    if age < 60 { format!("pred {} s", age) }
    else if age < 3600 { format!("pred {} min", age / 60) }
    else if age < 86400 { format!("pred {} h", age / 3600) }
    else { format!("pred {} dny", age / 86400) }
}

/// Render about:config page from current profile state.
pub fn render_about_config() -> (String, String) {
    let profile = crate::devtools::profile::active_profile();
    let dock = crate::devtools::profile::load_dock_position();
    let bookmarks = crate::devtools::bookmarks::load_bookmarks();
    let history = crate::devtools::history::load_history();
    let html = format!(r#"<!DOCTYPE html><html><head><title>Nastaveni</title></head>
<body>
<div class="cfg">
<h1>Nastaveni</h1>
<section>
<h2>Profil</h2>
<p><strong>Aktivni:</strong> {profile}</p>
<p><strong>Dock pozice:</strong> {dock}</p>
</section>
<section>
<h2>Zalozky ({bm_count})</h2>
{bm_list}
</section>
<section>
<h2>Historie ({hist_count})</h2>
{hist_list}
</section>
</div>
</body></html>"#,
        profile = profile,
        dock = dock.label(),
        bm_count = bookmarks.len(),
        bm_list = if bookmarks.is_empty() { "<p class='empty'>Zadne zalozky</p>".to_string() }
                  else {
                      bookmarks.iter().take(50).map(|b|
                          format!("<div class='bm'><strong>{}</strong> <small>{}</small></div>",
                                  html_escape(&b.title), html_escape(&b.url))
                      ).collect::<Vec<_>>().join("\n")
                  },
        hist_count = history.len(),
        hist_list = if history.is_empty() { "<p class='empty'>Zadna historie</p>".to_string() }
                    else {
                        history.iter().rev().take(50).map(|h|
                            format!("<div class='h'><strong>{}</strong> <small>{}</small></div>",
                                    html_escape(&h.title), html_escape(&h.url))
                        ).collect::<Vec<_>>().join("\n")
                    },
    );
    let css = r#"
body { font-family: 'Inter', sans-serif; background: #1a1a1f; color: #e8e6df; margin: 0; padding: 32px; }
.cfg { max-width: 900px; margin: 0 auto; }
h1 { color: #69a1ff; font-size: 32px; }
h2 { color: #94de7c; font-size: 20px; margin-top: 32px; border-bottom: 1px solid #4c4c55; padding-bottom: 8px; }
section { margin-bottom: 24px; }
.bm, .h { background: #2a2932; padding: 8px 12px; margin-bottom: 4px; border-radius: 4px; }
.bm small, .h small { color: #a1a1ae; margin-left: 8px; }
.empty { color: #a1a1ae; font-style: italic; }
strong { color: #e8e6df; }
"#;
    (html, css.to_string())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
     .replace('"', "&quot;")
}

const NEW_TAB_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>Nova zalozka</title></head>
<body>
<div class="container">
<h1>Rust Web Engine</h1>
<p class="subtitle">Vlastni prohlizec, vlastni renderovaci jadro.</p>
<div class="cards">
<a href="about:config" class="card"><h3>Nastaveni</h3><p>Profil, dock pozice, theme</p></a>
<a href="https://example.com" class="card"><h3>Example</h3><p>Test stranka</p></a>
<a href="https://news.ycombinator.com" class="card"><h3>Hacker News</h3><p>Tech news</p></a>
<a href="https://github.com" class="card"><h3>GitHub</h3><p>Code repos</p></a>
</div>
<p class="hint">Ctrl+L pro adresu, Ctrl+T novy tab, Ctrl+W zavrit, F12 devtools</p>
</div>
</body></html>"#;

const NEW_TAB_CSS: &str = r#"
body { font-family: 'Inter', sans-serif; background: #1a1a1f; color: #e8e6df; margin: 0; padding: 0; }
.container { max-width: 800px; margin: 80px auto; padding: 40px; text-align: center; }
h1 { color: #69a1ff; font-size: 48px; margin-bottom: 16px; }
.subtitle { color: #a1a1ae; font-size: 16px; margin-bottom: 48px; }
.cards { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
.card { background: #2a2932; padding: 24px; border-radius: 8px; border: 1px solid #4c4c55; text-decoration: none; display: block; }
.card:hover { background: #383744; }
.card h3 { color: #69a1ff; margin-top: 0; }
.card p { color: #a1a1ae; font-size: 14px; }
.hint { color: #6d6d7c; font-size: 12px; margin-top: 32px; font-style: italic; }
"#;

#[derive(Debug)]
pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl Default for TabManager {
    fn default() -> Self {
        Self { tabs: vec![Tab::empty()], active: 0 }
    }
}

impl TabManager {
    pub fn new(initial: Tab) -> Self {
        Self { tabs: vec![initial], active: 0 }
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.tabs.len() { self.active = idx; }
    }

    pub fn open(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    pub fn close(&mut self, idx: usize) {
        if self.tabs.len() <= 1 { return; }
        if idx >= self.tabs.len() { return; }
        self.tabs.remove(idx);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if self.active > idx {
            self.active -= 1;
        }
    }

    pub fn next(&mut self) {
        if self.tabs.len() > 0 {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn prev(&mut self) {
        if self.tabs.len() > 0 {
            self.active = if self.active == 0 { self.tabs.len() - 1 } else { self.active - 1 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_default_empty_je_jeden_tab() {
        let m = TabManager::default();
        assert_eq!(m.tabs.len(), 1);
        assert_eq!(m.active, 0);
    }

    #[test]
    fn tab_open_zvysi_active() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        assert_eq!(m.tabs.len(), 2);
        assert_eq!(m.active, 1);
    }

    #[test]
    fn tab_close_active_posune_active_dolu() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        assert_eq!(m.active, 2);
        m.close(2);
        assert_eq!(m.tabs.len(), 2);
        assert_eq!(m.active, 1);
    }

    #[test]
    fn tab_close_neumozni_jediny_tab() {
        let mut m = TabManager::default();
        m.close(0);
        assert_eq!(m.tabs.len(), 1, "Posledni tab nelze zavrit");
    }

    #[test]
    fn tab_next_wraparound() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        m.switch_to(0);
        m.next();
        assert_eq!(m.active, 1);
        m.next();
        assert_eq!(m.active, 2);
        m.next();
        assert_eq!(m.active, 0, "Wraparound");
    }

    #[test]
    fn tab_prev_wraparound() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.switch_to(0);
        m.prev();
        assert_eq!(m.active, 1, "Wrap z 0 na last");
    }

    #[test]
    fn tab_close_after_active_neposune_active() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        m.switch_to(1);
        m.close(2);
        assert_eq!(m.active, 1, "Close vyssi nez active = beze zmeny");
    }

    #[test]
    fn tab_close_below_active_posune_active() {
        let mut m = TabManager::default();
        m.open(Tab::empty());
        m.open(Tab::empty());
        m.switch_to(2);
        m.close(0);
        assert_eq!(m.active, 1, "Close pod active = active - 1");
    }
}
