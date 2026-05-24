//! Accessibility user preferences - prefers-reduced-motion, prefers-color-scheme,
//! prefers-contrast, forced-colors.
//!
//! Spec: CSS Media Queries L5.
//!
//! OS-specific detection (Windows registry, macOS NSUserDefaults, Linux GTK
//! settings) = next session. Foundation: API + manual override.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReducedMotion { NoPreference, Reduce }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorScheme { NoPreference, Light, Dark }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrefersContrast { NoPreference, More, Less, Custom }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ForcedColors { None, Active }

#[derive(Debug, Clone, Copy)]
pub struct A11yPrefs {
    pub reduced_motion: ReducedMotion,
    pub color_scheme: ColorScheme,
    pub prefers_contrast: PrefersContrast,
    pub forced_colors: ForcedColors,
}

impl Default for A11yPrefs {
    fn default() -> Self {
        Self {
            reduced_motion: ReducedMotion::NoPreference,
            color_scheme: ColorScheme::NoPreference,
            prefers_contrast: PrefersContrast::NoPreference,
            forced_colors: ForcedColors::None,
        }
    }
}

impl A11yPrefs {
    pub fn new() -> Self { Self::default() }

    /// CSS media query string match - vraci true kdyz query matches current prefs.
    pub fn match_query(&self, query: &str) -> bool {
        let q = query.trim().trim_start_matches('(').trim_end_matches(')').trim();
        match q.to_lowercase().as_str() {
            "prefers-reduced-motion" | "prefers-reduced-motion: reduce" => self.reduced_motion == ReducedMotion::Reduce,
            "prefers-reduced-motion: no-preference" => self.reduced_motion == ReducedMotion::NoPreference,
            "prefers-color-scheme: dark" => self.color_scheme == ColorScheme::Dark,
            "prefers-color-scheme: light" => self.color_scheme == ColorScheme::Light,
            "prefers-contrast: more" => self.prefers_contrast == PrefersContrast::More,
            "prefers-contrast: less" => self.prefers_contrast == PrefersContrast::Less,
            "forced-colors: active" => self.forced_colors == ForcedColors::Active,
            _ => false,
        }
    }

    /// Detect z OS settings - foundation no-op.
    pub fn detect_from_os() -> Self {
        // Real: Windows: SystemParametersInfo SPI_GETCLIENTAREAANIMATION
        //       macOS: NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        //       Linux: gsettings get org.gnome.desktop.interface enable-animations
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_reduced_motion() {
        let mut p = A11yPrefs::new();
        p.reduced_motion = ReducedMotion::Reduce;
        assert!(p.match_query("(prefers-reduced-motion: reduce)"));
        assert!(!p.match_query("(prefers-reduced-motion: no-preference)"));
    }

    #[test]
    fn match_color_scheme() {
        let mut p = A11yPrefs::new();
        p.color_scheme = ColorScheme::Dark;
        assert!(p.match_query("(prefers-color-scheme: dark)"));
        assert!(!p.match_query("(prefers-color-scheme: light)"));
    }

    #[test]
    fn forced_colors_match() {
        let mut p = A11yPrefs::new();
        p.forced_colors = ForcedColors::Active;
        assert!(p.match_query("(forced-colors: active)"));
    }
}
