//! DevTools theming - paleta + OS auto-detect + Chrome/Firefox flavors.
//!
//! Theme = (Mode, Flavor):
//!   Mode = Light / Dark / Auto (sync s OS)
//!   Flavor = Chrome / Firefox (barvy syntaxe + zvyrazneni padding/margin)

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeFlavor {
    Chrome,
    Firefox,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeSelection {
    pub mode: ThemeMode,
    pub flavor: ThemeFlavor,
}

impl Default for ThemeSelection {
    fn default() -> Self {
        // Default: Firefox dark (po phase 6 redesignu). Try load from config file
        // s fallback na Firefox+Auto.
        load_persisted().unwrap_or(ThemeSelection {
            mode: ThemeMode::Auto, flavor: ThemeFlavor::Firefox,
        })
    }
}

fn config_path() -> Option<std::path::PathBuf> {
    // ~/.rwe/profiles/<active>/devtools.json. Migrace z legacy
    // ~/AppData/Roaming/rwe/devtools.json one-shot.
    super::profile::migrate_legacy_config();
    super::profile::devtools_config_path()
}

fn load_persisted() -> Option<ThemeSelection> {
    let path = config_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    parse_config(&content)
}

fn parse_config(s: &str) -> Option<ThemeSelection> {
    // Lite JSON: { "mode": "auto|light|dark", "flavor": "chrome|firefox" }
    let mode = extract_str_value(s, "mode")?;
    let flavor = extract_str_value(s, "flavor")?;
    let mode = match mode.as_str() {
        "auto" => ThemeMode::Auto,
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        _ => return None,
    };
    let flavor = match flavor.as_str() {
        "chrome" => ThemeFlavor::Chrome,
        "firefox" => ThemeFlavor::Firefox,
        _ => return None,
    };
    Some(ThemeSelection { mode, flavor })
}

fn extract_str_value(s: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let idx = s.find(&pattern)?;
    let after = &s[idx + pattern.len()..];
    let colon = after.find(':')?;
    let after = &after[colon + 1..];
    let q1 = after.find('"')?;
    let after = &after[q1 + 1..];
    let q2 = after.find('"')?;
    Some(after[..q2].to_string())
}

/// Ulozi aktualne zvolenou theme do config souboru.
pub fn save_persisted(sel: ThemeSelection) {
    let Some(path) = config_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mode_s = match sel.mode {
        ThemeMode::Auto => "auto",
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    };
    let flavor_s = match sel.flavor {
        ThemeFlavor::Chrome => "chrome",
        ThemeFlavor::Firefox => "firefox",
    };
    let content = format!("{{\n  \"mode\": \"{}\",\n  \"flavor\": \"{}\"\n}}\n", mode_s, flavor_s);
    let _ = std::fs::write(&path, content);
}

/// Resolved palette - vsechny barvy uz konkretni RGBA byty.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub is_dark: bool,
    // Backgrounds
    pub bg_panel: [u8; 4],
    pub bg_panel_alt: [u8; 4],
    pub bg_toolbar: [u8; 4],
    pub bg_input: [u8; 4],
    pub bg_input_focus: [u8; 4],
    pub bg_row_hover: [u8; 4],
    pub bg_row_selected: [u8; 4],
    pub bg_row_selected_inactive: [u8; 4],
    pub bg_tab_active: [u8; 4],
    pub bg_button: [u8; 4],
    pub bg_button_hover: [u8; 4],
    pub bg_context_menu: [u8; 4],
    pub bg_context_menu_hover: [u8; 4],
    // Borders
    pub border: [u8; 4],
    pub border_strong: [u8; 4],
    pub border_focus: [u8; 4],
    pub accent: [u8; 4],
    // Text
    pub text: [u8; 4],
    pub text_dim: [u8; 4],
    pub text_disabled: [u8; 4],
    pub text_inverted: [u8; 4],
    /// Text barva pres accent/selected bg - vzdy vysoky kontrast (typicky bila).
    pub text_on_accent: [u8; 4],
    // Syntax highlight (Elements tree + Sources)
    pub syn_tag: [u8; 4],
    pub syn_attr: [u8; 4],
    pub syn_value: [u8; 4],
    pub syn_text_node: [u8; 4],
    pub syn_comment: [u8; 4],
    pub syn_doctype: [u8; 4],
    pub syn_punct: [u8; 4],
    pub syn_keyword: [u8; 4],
    pub syn_string: [u8; 4],
    pub syn_number: [u8; 4],
    pub syn_property: [u8; 4],
    pub syn_function: [u8; 4],
    // Console levels
    pub log_info: [u8; 4],
    pub log_warn: [u8; 4],
    pub log_error: [u8; 4],
    pub log_input_marker: [u8; 4],
    // Network status
    pub net_2xx: [u8; 4],
    pub net_3xx: [u8; 4],
    pub net_4xx: [u8; 4],
    pub net_5xx: [u8; 4],
    // Element highlight overlay (page side, Chrome-like)
    pub overlay_content: [u8; 4],   // modra (content rect)
    pub overlay_padding: [u8; 4],   // zelena
    pub overlay_border: [u8; 4],    // zluta
    pub overlay_margin: [u8; 4],    // oranzova
    pub overlay_dim: [u8; 4],       // outside dim
    pub overlay_label_bg: [u8; 4],
    pub overlay_label_text: [u8; 4],
}

/// OS theme detect - cti registry/setting raz pri startu, cache vysledek.
/// Vraci true pokud OS preferuje dark mode, false pri light/neznamem.
pub fn detect_os_dark_mode() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            // Registry: HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize
            //   AppsUseLightTheme (DWORD): 0 = dark, 1 = light
            let out = Command::new("reg")
                .args(&[
                    "query",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                    "/v", "AppsUseLightTheme",
                ])
                .output();
            if let Ok(o) = out {
                let s = String::from_utf8_lossy(&o.stdout);
                if let Some(line) = s.lines().find(|l| l.contains("AppsUseLightTheme")) {
                    if line.contains("0x0") {
                        return true;
                    } else if line.contains("0x1") {
                        return false;
                    }
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            // defaults read -g AppleInterfaceStyle -> "Dark" pri dark mode, error pri light
            let out = Command::new("defaults")
                .args(&["read", "-g", "AppleInterfaceStyle"])
                .output();
            if let Ok(o) = out {
                let s = String::from_utf8_lossy(&o.stdout);
                if s.trim().eq_ignore_ascii_case("Dark") {
                    return true;
                }
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use std::process::Command;
            // GNOME: gsettings get org.gnome.desktop.interface color-scheme -> 'prefer-dark'
            let out = Command::new("gsettings")
                .args(&["get", "org.gnome.desktop.interface", "color-scheme"])
                .output();
            if let Ok(o) = out {
                let s = String::from_utf8_lossy(&o.stdout);
                if s.contains("dark") {
                    return true;
                }
            }
        }
        false
    })
}

pub fn resolve_palette(sel: ThemeSelection) -> Palette {
    let is_dark = match sel.mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::Auto => detect_os_dark_mode(),
    };
    match (is_dark, sel.flavor) {
        (true, ThemeFlavor::Chrome) => chrome_dark(),
        (false, ThemeFlavor::Chrome) => chrome_light(),
        (true, ThemeFlavor::Firefox) => firefox_dark(),
        (false, ThemeFlavor::Firefox) => firefox_light(),
    }
}

// ─── Chrome paleta (dark) ────────────────────────────────────────────────
fn chrome_dark() -> Palette {
    Palette {
        is_dark: true,
        bg_panel: [32, 33, 36, 255],
        bg_panel_alt: [41, 42, 45, 255],
        bg_toolbar: [41, 42, 45, 255],
        bg_input: [48, 49, 52, 255],
        bg_input_focus: [60, 61, 64, 255],
        bg_row_hover: [56, 58, 62, 255],
        bg_row_selected: [10, 132, 255, 255],
        bg_row_selected_inactive: [60, 64, 70, 255],
        bg_tab_active: [60, 61, 64, 255],
        bg_button: [56, 58, 62, 255],
        bg_button_hover: [70, 72, 78, 255],
        bg_context_menu: [50, 52, 56, 255],
        bg_context_menu_hover: [70, 72, 78, 255],
        border: [60, 62, 66, 255],
        border_strong: [90, 92, 96, 255],
        border_focus: [10, 132, 255, 255],
        accent: [138, 180, 248, 255],
        text: [232, 234, 237, 255],
        text_dim: [154, 160, 166, 255],
        text_disabled: [100, 103, 108, 255],
        text_inverted: [32, 33, 36, 255],
        text_on_accent: [255, 255, 255, 255],
        syn_tag: [137, 221, 255, 255],
        syn_attr: [255, 200, 110, 255],
        syn_value: [149, 232, 145, 255],
        syn_text_node: [200, 200, 200, 255],
        syn_comment: [120, 130, 145, 255],
        syn_doctype: [180, 180, 200, 255],
        syn_punct: [180, 180, 180, 255],
        syn_keyword: [197, 134, 192, 255],
        syn_string: [149, 232, 145, 255],
        syn_number: [255, 200, 110, 255],
        syn_property: [137, 221, 255, 255],
        syn_function: [220, 220, 170, 255],
        log_info: [232, 234, 237, 255],
        log_warn: [255, 195, 80, 255],
        log_error: [255, 100, 100, 255],
        log_input_marker: [138, 180, 248, 255],
        net_2xx: [149, 232, 145, 255],
        net_3xx: [137, 221, 255, 255],
        net_4xx: [255, 195, 80, 255],
        net_5xx: [255, 100, 100, 255],
        overlay_content: [111, 168, 220, 130],
        overlay_padding: [147, 196, 125, 130],
        overlay_border: [255, 229, 153, 130],
        overlay_margin: [246, 178, 107, 130],
        overlay_dim: [0, 0, 0, 60],
        overlay_label_bg: [50, 52, 56, 240],
        overlay_label_text: [232, 234, 237, 255],
    }
}

// ─── Chrome paleta (light) ───────────────────────────────────────────────
fn chrome_light() -> Palette {
    Palette {
        is_dark: false,
        bg_panel: [255, 255, 255, 255],
        bg_panel_alt: [241, 243, 244, 255],
        bg_toolbar: [241, 243, 244, 255],
        bg_input: [255, 255, 255, 255],
        bg_input_focus: [255, 255, 255, 255],
        bg_row_hover: [232, 240, 254, 255],
        bg_row_selected: [26, 115, 232, 255],
        bg_row_selected_inactive: [220, 226, 234, 255],
        bg_tab_active: [255, 255, 255, 255],
        bg_button: [241, 243, 244, 255],
        bg_button_hover: [218, 220, 224, 255],
        bg_context_menu: [255, 255, 255, 255],
        bg_context_menu_hover: [232, 240, 254, 255],
        border: [218, 220, 224, 255],
        border_strong: [180, 183, 188, 255],
        border_focus: [26, 115, 232, 255],
        accent: [26, 115, 232, 255],
        text: [32, 33, 36, 255],
        text_dim: [95, 99, 104, 255],
        text_disabled: [180, 183, 188, 255],
        text_inverted: [255, 255, 255, 255],
        text_on_accent: [255, 255, 255, 255],
        syn_tag: [136, 18, 128, 255],
        syn_attr: [153, 69, 0, 255],
        syn_value: [26, 17, 153, 255],
        syn_text_node: [32, 33, 36, 255],
        syn_comment: [120, 124, 130, 255],
        syn_doctype: [120, 124, 130, 255],
        syn_punct: [120, 124, 130, 255],
        syn_keyword: [170, 13, 145, 255],
        syn_string: [196, 26, 22, 255],
        syn_number: [28, 0, 207, 255],
        syn_property: [136, 18, 128, 255],
        syn_function: [73, 81, 188, 255],
        log_info: [32, 33, 36, 255],
        log_warn: [201, 110, 0, 255],
        log_error: [197, 34, 31, 255],
        log_input_marker: [26, 115, 232, 255],
        net_2xx: [29, 134, 73, 255],
        net_3xx: [26, 115, 232, 255],
        net_4xx: [201, 110, 0, 255],
        net_5xx: [197, 34, 31, 255],
        overlay_content: [111, 168, 220, 130],
        overlay_padding: [147, 196, 125, 130],
        overlay_border: [255, 229, 153, 130],
        overlay_margin: [246, 178, 107, 130],
        overlay_dim: [0, 0, 0, 30],
        overlay_label_bg: [50, 52, 56, 240],
        overlay_label_text: [255, 255, 255, 255],
    }
}

// ─── Firefox paleta (dark) ───────────────────────────────────────────────
fn firefox_dark() -> Palette {
    Palette {
        is_dark: true,
        bg_panel: [35, 34, 43, 255],
        bg_panel_alt: [42, 41, 50, 255],
        bg_toolbar: [42, 41, 50, 255],
        bg_input: [27, 27, 35, 255],
        bg_input_focus: [35, 34, 43, 255],
        bg_row_hover: [56, 56, 65, 255],
        bg_row_selected: [69, 161, 255, 255],
        bg_row_selected_inactive: [56, 56, 65, 255],
        bg_tab_active: [27, 27, 35, 255],
        bg_button: [56, 56, 65, 255],
        bg_button_hover: [76, 76, 85, 255],
        bg_context_menu: [42, 41, 50, 255],
        bg_context_menu_hover: [69, 161, 255, 255],
        border: [56, 56, 65, 255],
        border_strong: [76, 76, 85, 255],
        border_focus: [69, 161, 255, 255],
        accent: [69, 161, 255, 255],
        text: [251, 251, 254, 255],
        text_dim: [191, 191, 201, 255],
        text_disabled: [109, 109, 124, 255],
        text_inverted: [27, 27, 35, 255],
        text_on_accent: [255, 255, 255, 255],
        syn_tag: [105, 198, 255, 255],
        syn_attr: [148, 222, 124, 255],
        syn_value: [254, 191, 84, 255],
        syn_text_node: [251, 251, 254, 255],
        syn_comment: [161, 161, 174, 255],
        syn_doctype: [191, 191, 201, 255],
        syn_punct: [191, 191, 201, 255],
        syn_keyword: [199, 146, 234, 255],
        syn_string: [254, 191, 84, 255],
        syn_number: [255, 117, 100, 255],
        syn_property: [105, 198, 255, 255],
        syn_function: [148, 222, 124, 255],
        log_info: [251, 251, 254, 255],
        log_warn: [255, 200, 96, 255],
        log_error: [255, 117, 100, 255],
        log_input_marker: [69, 161, 255, 255],
        net_2xx: [148, 222, 124, 255],
        net_3xx: [105, 198, 255, 255],
        net_4xx: [255, 200, 96, 255],
        net_5xx: [255, 117, 100, 255],
        overlay_content: [69, 161, 255, 130],
        overlay_padding: [148, 222, 124, 130],
        overlay_border: [254, 191, 84, 130],
        overlay_margin: [255, 117, 100, 130],
        overlay_dim: [0, 0, 0, 60],
        overlay_label_bg: [42, 41, 50, 240],
        overlay_label_text: [251, 251, 254, 255],
    }
}

// ─── Firefox paleta (light) ──────────────────────────────────────────────
fn firefox_light() -> Palette {
    Palette {
        is_dark: false,
        bg_panel: [249, 249, 251, 255],
        bg_panel_alt: [240, 240, 244, 255],
        bg_toolbar: [240, 240, 244, 255],
        bg_input: [255, 255, 255, 255],
        bg_input_focus: [255, 255, 255, 255],
        bg_row_hover: [232, 236, 245, 255],
        bg_row_selected: [0, 96, 223, 255],
        bg_row_selected_inactive: [220, 226, 234, 255],
        bg_tab_active: [255, 255, 255, 255],
        bg_button: [240, 240, 244, 255],
        bg_button_hover: [220, 226, 234, 255],
        bg_context_menu: [255, 255, 255, 255],
        bg_context_menu_hover: [232, 236, 245, 255],
        border: [220, 220, 224, 255],
        border_strong: [187, 187, 195, 255],
        border_focus: [0, 96, 223, 255],
        accent: [0, 96, 223, 255],
        text: [21, 20, 26, 255],
        text_dim: [89, 89, 101, 255],
        text_disabled: [159, 159, 173, 255],
        text_inverted: [255, 255, 255, 255],
        text_on_accent: [255, 255, 255, 255],
        syn_tag: [99, 28, 145, 255],
        syn_attr: [137, 33, 82, 255],
        syn_value: [183, 64, 0, 255],
        syn_text_node: [21, 20, 26, 255],
        syn_comment: [89, 89, 101, 255],
        syn_doctype: [89, 89, 101, 255],
        syn_punct: [89, 89, 101, 255],
        syn_keyword: [137, 33, 82, 255],
        syn_string: [99, 28, 145, 255],
        syn_number: [183, 64, 0, 255],
        syn_property: [99, 28, 145, 255],
        syn_function: [29, 79, 127, 255],
        log_info: [21, 20, 26, 255],
        log_warn: [183, 64, 0, 255],
        log_error: [192, 23, 36, 255],
        log_input_marker: [0, 96, 223, 255],
        net_2xx: [29, 134, 73, 255],
        net_3xx: [0, 96, 223, 255],
        net_4xx: [183, 64, 0, 255],
        net_5xx: [192, 23, 36, 255],
        overlay_content: [0, 96, 223, 100],
        overlay_padding: [148, 222, 124, 130],
        overlay_border: [254, 191, 84, 130],
        overlay_margin: [255, 117, 100, 130],
        overlay_dim: [0, 0, 0, 30],
        overlay_label_bg: [21, 20, 26, 240],
        overlay_label_text: [255, 255, 255, 255],
    }
}
