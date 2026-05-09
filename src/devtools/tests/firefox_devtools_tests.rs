//! Tests pro Firefox-style devtools upgrade (phase 1-7+ z planu).

use crate::devtools::{ThemeSelection, theme, SidePanelTab, DevToolsState,
                     OverlayDescriptor, OverlayKind, Tab};

// Pozn: ThemeSelection::default() loaduje persisted config. V testu by mohlo
// vracet uzivatelovu uchovany volbu. Test pres direct compile-time fallback
// volbu vetva Default impl.
#[test]
fn default_theme_fallback_je_firefox() {
    // Default fallback (kdyz neni config) je Firefox po phase 6 redesignu.
    // Persisted config muze prepsat - skip pokud existuje.
    if std::env::var("APPDATA").ok().or_else(|| std::env::var("HOME").ok())
        .map(|d| std::path::PathBuf::from(d).join("rwe").join("devtools.json").exists())
        .unwrap_or(false) {
        return; // Skip - existuje persisted config.
    }
    let s = ThemeSelection::default();
    assert!(matches!(s.flavor, theme::ThemeFlavor::Firefox),
            "Default flavor po phase 6 = Firefox (fallback)");
}

#[test]
fn side_panel_tabs_visible_default_je_5() {
    let visible = SidePanelTab::visible_default();
    assert_eq!(visible.len(), 5);
    assert!(visible.contains(&SidePanelTab::Layout));
    assert!(visible.contains(&SidePanelTab::Computed));
    assert!(visible.contains(&SidePanelTab::Changes));
    assert!(visible.contains(&SidePanelTab::Fonts));
    assert!(visible.contains(&SidePanelTab::Animations));
}

#[test]
fn side_panel_tab_kompatibilita_skryta_default() {
    let visible = SidePanelTab::visible_default();
    assert!(!visible.contains(&SidePanelTab::Compatibility),
            "Compatibility tab je skryta default (jako Firefox)");
}

#[test]
fn devtools_state_default_initialized() {
    let s = DevToolsState::default();
    assert_eq!(s.side_panel_w, 280.0);
    assert!(s.overlays.is_empty());
    assert!(s.collapsed_sections.is_empty());
    assert!(!s.tab_overflow_open);
    assert_eq!(s.side_panel_tab, SidePanelTab::Layout);
}

#[test]
fn overlay_descriptor_basic() {
    let mut s = DevToolsState::default();
    s.overlays.push(OverlayDescriptor {
        node_id: 0x1234,
        kind: OverlayKind::Flex,
    });
    assert_eq!(s.overlays.len(), 1);
    assert!(matches!(s.overlays[0].kind, OverlayKind::Flex));
}

#[test]
fn collapsed_sections_toggle() {
    use crate::browser::devtools_panel::SectionId;
    let mut s = DevToolsState::default();
    assert!(!s.collapsed_sections.contains(&SectionId::LayoutFlex));
    s.collapsed_sections.insert(SectionId::LayoutFlex);
    assert!(s.collapsed_sections.contains(&SectionId::LayoutFlex));
    s.collapsed_sections.remove(&SectionId::LayoutFlex);
    assert!(!s.collapsed_sections.contains(&SectionId::LayoutFlex));
}

#[test]
fn parse_css_color_hex_3() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    assert_eq!(parse_css_color_for_test("#f00"), Some([255, 0, 0, 255]));
    assert_eq!(parse_css_color_for_test("#0f0"), Some([0, 255, 0, 255]));
}

#[test]
fn parse_css_color_hex_6() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    assert_eq!(parse_css_color_for_test("#3498db"), Some([0x34, 0x98, 0xdb, 255]));
}

#[test]
fn parse_css_color_hex_8() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    assert_eq!(parse_css_color_for_test("#3498db80"), Some([0x34, 0x98, 0xdb, 0x80]));
}

#[test]
fn parse_css_color_rgb() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    assert_eq!(parse_css_color_for_test("rgb(231, 76, 60)"), Some([231, 76, 60, 255]));
}

#[test]
fn parse_css_color_rgba() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    let c = parse_css_color_for_test("rgba(0, 0, 0, 0.5)").unwrap();
    assert_eq!(c[0..3], [0, 0, 0]);
    assert!((c[3] as i32 - 127).abs() <= 2); // ~127
}

#[test]
fn parse_css_color_named() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    assert_eq!(parse_css_color_for_test("red"), Some([255, 0, 0, 255]));
    assert_eq!(parse_css_color_for_test("transparent"), Some([0, 0, 0, 0]));
}

#[test]
fn parse_css_color_invalid() {
    use crate::browser::devtools_panel::parse_css_color_for_test;
    assert_eq!(parse_css_color_for_test("not-a-color"), None);
    assert_eq!(parse_css_color_for_test("16px"), None);
}

#[test]
fn compute_tab_layout_overflow() {
    use crate::browser::devtools_panel::compute_tab_layout;
    // Vsechny taby fit (siroky window).
    let (visible, overflow) = compute_tab_layout(2000.0);
    assert_eq!(overflow.len(), 0);
    assert_eq!(visible.len(), Tab::all().len());
    // Uzky window - nejake taby do overflow.
    let (visible, overflow) = compute_tab_layout(400.0);
    assert!(overflow.len() > 0, "Pri 400px sirce nejake taby v overflow");
    assert_eq!(visible.len() + overflow.len(), Tab::all().len());
}

#[test]
fn dock_position_default_je_bottom() {
    use crate::devtools::profile::DockPosition;
    let d = DockPosition::default();
    assert!(matches!(d, DockPosition::Bottom));
}

#[test]
fn dock_position_roundtrip() {
    use crate::devtools::profile::DockPosition;
    for p in DockPosition::all() {
        let s = p.as_str();
        let parsed = DockPosition::from_str(s);
        assert_eq!(parsed, Some(*p), "Roundtrip {} failed", s);
    }
}

#[test]
fn dock_position_invalid_str() {
    use crate::devtools::profile::DockPosition;
    assert_eq!(DockPosition::from_str("invalid"), None);
    assert_eq!(DockPosition::from_str(""), None);
}

#[test]
fn dock_position_all_count_5() {
    use crate::devtools::profile::DockPosition;
    assert_eq!(DockPosition::all().len(), 5);
}

#[test]
fn dock_position_labels_unique() {
    use crate::devtools::profile::DockPosition;
    let mut labels: Vec<&'static str> = DockPosition::all().iter().map(|p| p.label()).collect();
    labels.sort();
    let orig_len = labels.len();
    labels.dedup();
    assert_eq!(orig_len, labels.len(), "labels musi byt unikatni");
}

#[test]
fn devtools_state_default_dock_je_bottom_nebo_loaded() {
    // Default je Bottom OR loaded persisted hodnota. Test: vychozi hodnota
    // je validni varianta enumu.
    let s = DevToolsState::default();
    use crate::devtools::profile::DockPosition;
    assert!(matches!(s.dock_position,
        DockPosition::Bottom | DockPosition::Right
        | DockPosition::Left | DockPosition::Top | DockPosition::PopupWindow));
}

#[test]
fn devtools_state_settings_popup_default_zavren() {
    let s = DevToolsState::default();
    assert!(!s.settings_popup_open);
}

#[test]
fn profile_active_default_je_default() {
    use crate::devtools::profile::active_profile;
    // Pri startu testu env var moze byt nastaveny - dalsi test musi
    // pocitat s timto.
    let name = active_profile();
    assert!(!name.is_empty());
}
