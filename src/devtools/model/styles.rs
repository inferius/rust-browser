//! Computed styles panel: matching CSS rules + computed values, Chrome-like.

#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub selector: String,
    pub source: RuleSource,
    pub specificity: u32,
    /// (property, value, important, overridden_by_later) - serazene per source.
    pub declarations: Vec<RuleDecl>,
    /// Pri rule zdedeno z ancestoru (CSS inheritance) Some(tag), jinak None.
    /// Firefox-style group: "Pododědo z {tag}" header v styles pane.
    pub inherited_from: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuleDecl {
    pub property: String,
    pub value: String,
    pub important: bool,
    /// True kdyz neskor v cascade prepsano - render strikethrough.
    pub overridden: bool,
}

#[derive(Debug, Clone)]
pub enum RuleSource {
    UserAgent,
    Inline,
    StyleBlock { index: usize },
    External { url: String },
}

#[derive(Debug, Clone, Default)]
pub struct StylesState {
    pub matched_rules: Vec<MatchedRule>,
    pub computed: Vec<(String, String)>,
    pub filter: String,
    pub scroll_y: f32,
    /// Cache swatch zon (x, y, w, h, rgba) emitnutych v posledni paint
    /// pass. Hit-test cte tento Vec - pri kliku v swatch rect -> open
    /// color picker. Volane interior-mut pres RefCell? Ne - paint_styles_pane
    /// bere &state. Resime pres separate tracking field s lifetime per frame.
    /// Pro ted: pure RefCell na hot path je OK.
    /// (x, y, w, h, color, property_name) - property pro write-back picker.
    pub swatch_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, [u8; 4], String)>>,
    /// Cache var() chip zon: (x, y, w, h, var_name).
    pub var_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
    /// @font-face deklarace ze vsech stylesheets (family, src, weight, style).
    pub font_faces: Vec<(String, String, String, String)>,
}

impl StylesState {
    /// Odhad celkove vysky contentu pri layout konstantach paint_styles_pane:
    /// row_h=18, padding/gaps. Pouziva se pro clamp scroll + scrollbar thumb.
    pub fn estimate_total_h(&self) -> f32 {
        let row_h = 18.0_f32;
        let mut h = 8.0; // top padding
        // "Matched CSS rules" header.
        h += row_h + 2.0;
        if self.matched_rules.is_empty() {
            h += row_h + 8.0;
        } else {
            for r in &self.matched_rules {
                h += row_h; // selector line
                h += r.declarations.len() as f32 * row_h;
                h += row_h + 4.0; // closing brace
            }
        }
        // Computed header.
        h += row_h + 2.0;
        let filter = self.filter.to_lowercase();
        for (k, _) in &self.computed {
            if !filter.is_empty() && !k.contains(&filter) { continue; }
            h += row_h;
        }
        // Box section.
        h += 8.0 + row_h + 2.0;
        h += 4.0 * row_h;
        h
    }
}
