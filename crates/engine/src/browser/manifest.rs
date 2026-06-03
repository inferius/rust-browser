//! Web App Manifest - link[rel=manifest] -> install metadata.
//!
//! Spec: https://www.w3.org/TR/appmanifest/

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct WebAppManifest {
    pub name: String,
    pub short_name: String,
    pub description: String,
    pub start_url: String,
    pub scope: String,
    pub display: DisplayMode,
    pub orientation: Orientation,
    pub theme_color: Option<String>,
    pub background_color: Option<String>,
    pub icons: Vec<ManifestIcon>,
    pub categories: Vec<String>,
    pub lang: String,
    pub dir: String,
    pub display_override: Vec<DisplayMode>,
    pub share_target: Option<ShareTarget>,
    pub shortcuts: Vec<AppShortcut>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    Browser,
    MinimalUi,
    Standalone,
    Fullscreen,
    WindowControlsOverlay,
    Tabbed,
}

impl Default for DisplayMode { fn default() -> Self { DisplayMode::Browser } }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    Any,
    Natural,
    Landscape,
    LandscapePrimary,
    LandscapeSecondary,
    Portrait,
    PortraitPrimary,
    PortraitSecondary,
}

impl Default for Orientation { fn default() -> Self { Orientation::Any } }

#[derive(Debug, Clone)]
pub struct ManifestIcon {
    pub src: String,
    pub sizes: String,
    pub mime_type: Option<String>,
    pub purpose: String,        // "any" | "maskable" | "monochrome"
}

#[derive(Debug, Clone, Default)]
pub struct ShareTarget {
    pub action: String,
    pub method: String,
    pub enctype: String,
    pub params: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AppShortcut {
    pub name: String,
    pub short_name: String,
    pub description: String,
    pub url: String,
    pub icons: Vec<ManifestIcon>,
}

pub fn parse_display(s: &str) -> DisplayMode {
    match s.trim().to_ascii_lowercase().as_str() {
        "minimal-ui" => DisplayMode::MinimalUi,
        "standalone" => DisplayMode::Standalone,
        "fullscreen" => DisplayMode::Fullscreen,
        "window-controls-overlay" => DisplayMode::WindowControlsOverlay,
        "tabbed" => DisplayMode::Tabbed,
        _ => DisplayMode::Browser,
    }
}

pub fn parse_orientation(s: &str) -> Orientation {
    match s.trim().to_ascii_lowercase().as_str() {
        "natural" => Orientation::Natural,
        "landscape" => Orientation::Landscape,
        "landscape-primary" => Orientation::LandscapePrimary,
        "landscape-secondary" => Orientation::LandscapeSecondary,
        "portrait" => Orientation::Portrait,
        "portrait-primary" => Orientation::PortraitPrimary,
        "portrait-secondary" => Orientation::PortraitSecondary,
        _ => Orientation::Any,
    }
}

/// Resolve effective display mode: walk display_override list, fall through to display.
pub fn effective_display(m: &WebAppManifest, supported: &[DisplayMode]) -> DisplayMode {
    for d in &m.display_override {
        if supported.contains(d) { return *d; }
    }
    m.display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_display_default_browser() {
        assert_eq!(parse_display("garbage"), DisplayMode::Browser);
        assert_eq!(parse_display("standalone"), DisplayMode::Standalone);
    }

    #[test]
    fn parse_orientation_default_any() {
        assert_eq!(parse_orientation("xyz"), Orientation::Any);
        assert_eq!(parse_orientation("portrait-primary"), Orientation::PortraitPrimary);
    }

    #[test]
    fn effective_display_uses_override() {
        let mut m = WebAppManifest::default();
        m.display = DisplayMode::Standalone;
        m.display_override = vec![DisplayMode::WindowControlsOverlay, DisplayMode::Tabbed];
        let supported = vec![DisplayMode::Tabbed, DisplayMode::Standalone];
        assert_eq!(effective_display(&m, &supported), DisplayMode::Tabbed);
    }

    #[test]
    fn effective_display_falls_back_to_display() {
        let mut m = WebAppManifest::default();
        m.display = DisplayMode::MinimalUi;
        m.display_override = vec![DisplayMode::WindowControlsOverlay];
        let supported = vec![DisplayMode::MinimalUi];
        assert_eq!(effective_display(&m, &supported), DisplayMode::MinimalUi);
    }
}
