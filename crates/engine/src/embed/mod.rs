//! `embed` modul - veřejné API pro vlození enginu jako "webview" do hostujici
//! aplikace (Edge/Chromium model). Shell crate je první uzivatel; třetí strana
//! muze postavit vlastní okno + UI a embed nas engine pres tutu API.
//!
//! # Architektonicky model
//!
//! ```text
//! +---------------------------------------------------------+
//! |  HOST APP (shell crate, custom UI, third-party)         |
//! |                                                         |
//! |  winit::Window  +  wgpu::Surface (swap chain)           |
//! |       |                                                 |
//! |       v                                                 |
//! |  +---------- Arc<Engine> -----------------+             |
//! |  |  wgpu::Device + Queue (shared)         |             |
//! |  |  GlyphAtlas + ImageAtlas (shared)      |             |
//! |  |  Settings, font registry               |             |
//! |  +----------------------------------------+             |
//! |       |                                                 |
//! |       +---> WebView (tab 1)  -> offscreen Texture       |
//! |       +---> WebView (tab 2)  -> offscreen Texture       |
//! |       +---> WebView (tab N)  -> offscreen Texture       |
//! |                                                         |
//! |  Compositor: shell shader = page texture + chrome paint |
//! +---------------------------------------------------------+
//! ```
//!
//! # Migrace
//!
//! Phase 2 (tato faze): API kontrakt + stubs s `todo!()`. Starý
//! `browser::render::App` je stale plne funkcni.
//!
//! Phase 3+: postupne migrace stavu z `App` do `WebView`. Phase 5 sloti shell
//! kompositor a engine prejde na offscreen RT only.

pub mod engine;
pub mod event;
pub mod webview;

pub use engine::{Engine, EngineSettings};
pub use event::{
    CursorIcon, EventResponse, InputEvent, KeyModifiers, MouseButton, NavigationMethod,
    NavigationRequest, NavigationResult, NavigationTarget,
};
pub use webview::WebView;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_settings_default_sane() {
        let s = EngineSettings::default();
        assert!(!s.default_font_family.is_empty());
        assert!(s.default_font_size > 0.0);
        assert!(s.user_agent.starts_with("RustWebEngine/"));
        assert!(s.max_webviews >= 1);
    }

    #[test]
    fn cursor_icon_default_is_default() {
        assert_eq!(CursorIcon::default(), CursorIcon::Default);
    }

    #[test]
    fn key_modifiers_default_all_false() {
        let m = KeyModifiers::default();
        assert!(!m.shift && !m.ctrl && !m.alt && !m.meta);
    }

    #[test]
    fn event_response_default_clean() {
        let r = EventResponse::default();
        assert!(!r.dirty);
        assert!(r.navigation.is_none());
        assert!(r.cursor.is_none());
        assert!(!r.new_console_logs);
        assert!(r.title_changed.is_none());
    }
}
