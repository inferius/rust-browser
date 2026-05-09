//! Computed styles panel: matching CSS rules + computed values, Chrome-like.

#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub selector: String,
    pub source: RuleSource,
    pub specificity: u32,
    /// (property, value, important, overridden_by_later) - serazene per source.
    pub declarations: Vec<RuleDecl>,
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
}
