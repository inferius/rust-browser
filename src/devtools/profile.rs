//! User profiles - per-uzivatel ulozeny config (theme, dock position, panel
//! sizes, ...). Profiles zije v `~/.rwe/profiles/<name>/`. Default profile
//! = "default". CLI flag `--profile=NAME` muze prepnout.
//!
//! Files per profile:
//!   devtools.json    - theme + dock position + panel sizes
//!   bookmarks.json   - bookmarks (shell mode)
//!   history.json     - browsing history (shell mode)

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Bottom,
    Right,
    Left,
    Top,
    /// Separate popup window (TODO - pro ted = treated as Bottom).
    PopupWindow,
}

impl Default for DockPosition {
    fn default() -> Self { DockPosition::Bottom }
}

impl DockPosition {
    pub fn label(self) -> &'static str {
        match self {
            DockPosition::Bottom => "Dole",
            DockPosition::Right => "Vpravo",
            DockPosition::Left => "Vlevo",
            DockPosition::Top => "Nahore",
            DockPosition::PopupWindow => "Nove okno",
        }
    }
    pub fn all() -> &'static [DockPosition] {
        &[DockPosition::Bottom, DockPosition::Right,
          DockPosition::Left, DockPosition::Top, DockPosition::PopupWindow]
    }
    pub fn as_str(self) -> &'static str {
        match self {
            DockPosition::Bottom => "bottom",
            DockPosition::Right => "right",
            DockPosition::Left => "left",
            DockPosition::Top => "top",
            DockPosition::PopupWindow => "popup",
        }
    }
    pub fn from_str(s: &str) -> Option<DockPosition> {
        match s {
            "bottom" => Some(DockPosition::Bottom),
            "right" => Some(DockPosition::Right),
            "left" => Some(DockPosition::Left),
            "top" => Some(DockPosition::Top),
            "popup" => Some(DockPosition::PopupWindow),
            _ => None,
        }
    }
}

/// Resolve profile directory: ~/.rwe/profiles/<name>. Default name = "default".
pub fn profile_dir(name: &str) -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    } else {
        std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config"))
    }?;
    let dir = base.join("rwe").join("profiles").join(name);
    Some(dir)
}

pub fn ensure_profile_dir(name: &str) -> Option<PathBuf> {
    let dir = profile_dir(name)?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok()?;
    }
    Some(dir)
}

/// List existujicich profilu (kazda subdir v ~/.rwe/profiles/).
pub fn list_profiles() -> Vec<String> {
    let Some(base) = profile_dir("").and_then(|p| p.parent().map(|p| p.to_path_buf())) else { return vec!["default".to_string()] };
    let Ok(entries) = std::fs::read_dir(&base) else { return vec!["default".to_string()] };
    let mut out: Vec<String> = entries.filter_map(|e| {
        let e = e.ok()?;
        if e.file_type().ok()?.is_dir() {
            e.file_name().to_str().map(|s| s.to_string())
        } else { None }
    }).collect();
    if !out.contains(&"default".to_string()) {
        out.insert(0, "default".to_string());
    }
    out.sort();
    out
}

/// Aktualni active profile name. Read once z env nebo CLI, persistuje pres
/// zivot procesu. Default "default".
static ACTIVE_PROFILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn set_active_profile(name: String) {
    let _ = ACTIVE_PROFILE.set(name);
}

pub fn active_profile() -> &'static str {
    ACTIVE_PROFILE.get_or_init(|| {
        std::env::var("RWE_PROFILE").unwrap_or_else(|_| "default".to_string())
    }).as_str()
}

/// Path k devtools.json v aktualnim profilu.
pub fn devtools_config_path() -> Option<PathBuf> {
    let name = active_profile().to_string();
    let dir = ensure_profile_dir(&name)?;
    Some(dir.join("devtools.json"))
}

/// Migrate legacy config z ~/AppData/Roaming/rwe/devtools.json (pre-profile)
/// do default profile dir. One-shot kdyz default profile config neexistuje.
pub fn migrate_legacy_config() {
    let Some(legacy) = legacy_config_path() else { return };
    let Some(target) = devtools_config_path() else { return };
    if target.exists() || !legacy.exists() { return; }
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::copy(&legacy, &target);
}

/// Lite read/write dock_position z devtools.json. JSON format simple klicove
/// hodnoty - reuse existing parse_config logiky kdyz potreba.
pub fn load_dock_position() -> DockPosition {
    let Some(path) = devtools_config_path() else { return DockPosition::default(); };
    let Ok(content) = std::fs::read_to_string(&path) else { return DockPosition::default(); };
    extract_str_value(&content, "dock")
        .and_then(|s| DockPosition::from_str(&s))
        .unwrap_or_default()
}

pub fn save_dock_position(pos: DockPosition) {
    let Some(path) = devtools_config_path() else { return };
    // Read existujici config + nahrad/pridej "dock" klic. Naivni JSON merge
    // (nas format je flat key-value).
    let existing = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
    let mut mode = extract_str_value(&existing, "mode").unwrap_or_else(|| "auto".to_string());
    let mut flavor = extract_str_value(&existing, "flavor").unwrap_or_else(|| "firefox".to_string());
    let _ = (&mut mode, &mut flavor);
    let json = format!(
        "{{\n  \"mode\": \"{}\",\n  \"flavor\": \"{}\",\n  \"dock\": \"{}\"\n}}\n",
        mode, flavor, pos.as_str()
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, json);
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

fn legacy_config_path() -> Option<PathBuf> {
    let base: PathBuf = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("APPDATA").ok()?)
    } else {
        PathBuf::from(std::env::var("HOME").ok()?).join(".config")
    };
    Some(base.join("rwe").join("devtools.json"))
}
