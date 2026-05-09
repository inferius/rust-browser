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
}

impl Tab {
    pub fn new(html: String, css: String, url: Option<String>, path: Option<PathBuf>) -> Self {
        let title = url.clone()
            .map(|u| u.split('/').last().unwrap_or(&u).to_string())
            .unwrap_or_else(|| "Nova zalozka".to_string());
        let favicon_url = url.as_ref().map(|u| derive_favicon_url(u, &html));
        Self {
            url, path, html, css, title, favicon_url,
            favicon_bytes: None,
            scroll_y: 0.0, scroll_x: 0.0,
            history: Vec::new(),
            history_idx: 0,
        }
    }

    pub fn empty() -> Self {
        Self {
            url: Some("about:newtab".to_string()),
            path: None,
            html: NEW_TAB_HTML.to_string(),
            css: NEW_TAB_CSS.to_string(),
            title: "Nova zalozka".to_string(),
            favicon_url: None,
            favicon_bytes: None,
            scroll_y: 0.0, scroll_x: 0.0,
            history: Vec::new(),
            history_idx: 0,
        }
    }
}

/// Najdi favicon URL: <link rel="icon" href="...">, fallback /favicon.ico.
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
<div class="card"><h3>Konzole</h3><p>F12 -> tab Konzole</p></div>
<div class="card"><h3>Inspektor</h3><p>F12 -> Elements + Hover na element</p></div>
<div class="card"><h3>Sit</h3><p>F12 -> tab Sit</p></div>
<div class="card"><h3>Nastaveni</h3><p>Ozubene kolo v toolbaru</p></div>
</div>
</div>
</body></html>"#;

const NEW_TAB_CSS: &str = r#"
body { font-family: 'Inter', sans-serif; background: #1a1a1f; color: #e8e6df; margin: 0; padding: 0; }
.container { max-width: 800px; margin: 80px auto; padding: 40px; text-align: center; }
h1 { color: #69a1ff; font-size: 48px; margin-bottom: 16px; }
.subtitle { color: #a1a1ae; font-size: 16px; margin-bottom: 48px; }
.cards { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
.card { background: #2a2932; padding: 24px; border-radius: 8px; border: 1px solid #4c4c55; }
.card h3 { color: #69a1ff; margin-top: 0; }
.card p { color: #a1a1ae; font-size: 14px; }
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
