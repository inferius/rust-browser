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
    /// Set rozbalenych shorthand groups v Computed panelu (padding, margin, ...).
    /// Default = collapsed (jen shorthand value, sub-props skryte).
    pub computed_expanded: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Cache (x, y, w, h, shorthand_name) clickable chevron zon v Computed panelu.
    pub computed_chevron_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
    /// Cache (x, y, w, h, property_name) zon clickable na value v styles pane.
    /// Klik = otevrit editor. Per-frame populated.
    pub decl_value_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
    /// Aktivni edit value: Some((property, buffer)). Dopisovani pres KeyboardInput.
    pub editing_value: Option<(String, String)>,
    /// Cache (x, y, w, h, selector) clickable match-preview ctverecku.
    pub match_toggle_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
    /// Cache (x, y, w, h, source_label) clickable source linku (filename:line).
    pub source_link_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
    /// Cache (x, y, w, h, section_id_str) section header click zon v side panelu.
    /// Per-frame populated v paint_section_header. Hit-test cte presny x/y.
    pub section_header_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
    /// Pridavam novy inline style decl - phase: typuju prop, pak value.
    pub adding_inline_decl: Option<AddingInlineDecl>,
    /// Cache (x, y, w, h) overflow chevron buttonu v side panel sub-tab strip.
    /// None = chevron neviditelny (vsechny tabs vejde se).
    pub overflow_chevron_zone: std::cell::RefCell<Option<(f32, f32, f32, f32)>>,
    /// Cache (x, y, w, h, tab_idx) klikatelnych dropdown items.
    pub overflow_dropdown_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, usize)>>,
    /// Cache (x, y, w, h, action) animations panel toolbar buttonu.
    /// action: "pause" / "speed" / "restart".
    pub animations_btn_zones: std::cell::RefCell<Vec<(f32, f32, f32, f32, String)>>,
}

/// State pri pridavani noveho inline stylu na selected element.
/// Phase Property -> Value. Tab/Enter prepne, druhy Enter = apply.
#[derive(Debug, Clone)]
pub struct AddingInlineDecl {
    pub phase: AddPhase,
    pub prop_buffer: String,
    pub value_buffer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddPhase {
    Property,
    Value,
}

/// Vrati shorthand jmeno pokud `prop` je sub-property nejakeho shorthand.
/// Napriklad "padding-top" -> Some("padding"), "background-color" -> Some("background").
pub fn shorthand_for(prop: &str) -> Option<&'static str> {
    let shorthands: &[&str] = &[
        "padding", "margin", "border", "border-radius", "border-width",
        "border-style", "border-color", "background", "font", "transition",
        "animation", "outline", "list-style", "flex", "grid", "grid-area",
        "grid-template", "place-items", "place-content", "place-self",
        "inset", "overflow", "gap", "scroll-margin", "scroll-padding",
    ];
    for sh in shorthands {
        if prop == *sh { return None; }
        if prop.starts_with(sh) && prop.as_bytes().get(sh.len()) == Some(&b'-') {
            return Some(*sh);
        }
    }
    None
}

/// Set vsech shorthand jmen co mohou mit sub-props.
pub fn is_shorthand(prop: &str) -> bool {
    matches!(prop, "padding" | "margin" | "border" | "border-radius"
        | "border-width" | "border-style" | "border-color" | "background"
        | "font" | "transition" | "animation" | "outline" | "list-style"
        | "flex" | "grid" | "grid-area" | "grid-template" | "place-items"
        | "place-content" | "place-self" | "inset" | "overflow" | "gap"
        | "scroll-margin" | "scroll-padding")
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
