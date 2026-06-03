//! `@supports`, `@media`, `@container`, `@layer` evaluation.
//!
//! Spec: https://www.w3.org/TR/css-conditional-3/
//! @supports (display: grid) {} -> match dle UA support.
//! @media (min-width: 600px) {} -> viewport query.
//! @container (inline-size > 300px) {} -> container queries.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeatureSupport {
    Supported,
    NotSupported,
    Unknown,
}

/// Stub: in real engine, this calls into the style/property registry.
pub fn supports(property: &str, value: &str, known: &HashMap<(String, String), bool>) -> FeatureSupport {
    if let Some(b) = known.get(&(property.to_string(), value.to_string())) {
        return if *b { FeatureSupport::Supported } else { FeatureSupport::NotSupported };
    }
    if let Some(b) = known.get(&(property.to_string(), "*".to_string())) {
        return if *b { FeatureSupport::Supported } else { FeatureSupport::NotSupported };
    }
    FeatureSupport::Unknown
}

/// Boolean tree for @supports/@media expressions.
#[derive(Debug, Clone)]
pub enum CondExpr {
    Feature(String, String),
    Not(Box<CondExpr>),
    And(Vec<CondExpr>),
    Or(Vec<CondExpr>),
    MediaFeature(String, MediaValue),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaValue {
    Length(f32),                // px
    Aspect(f32, f32),
    Boolean(bool),
    Resolution(f32),            // dppx
    Keyword(String),
}

#[derive(Debug, Clone, Default)]
pub struct MediaContext {
    pub width: f32,
    pub height: f32,
    pub device_pixel_ratio: f32,
    pub color_scheme: String,      // "light" | "dark"
    pub reduced_motion: bool,
    pub hover: HoverCapability,
    pub pointer: PointerCapability,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HoverCapability { None, OnDemand, Hover }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerCapability { None, Coarse, Fine }

impl Default for HoverCapability { fn default() -> Self { HoverCapability::Hover } }
impl Default for PointerCapability { fn default() -> Self { PointerCapability::Fine } }

pub fn evaluate(
    expr: &CondExpr,
    supports_table: &HashMap<(String, String), bool>,
    media: &MediaContext,
) -> bool {
    match expr {
        CondExpr::Feature(prop, val) => matches!(supports(prop, val, supports_table), FeatureSupport::Supported),
        CondExpr::Not(e) => !evaluate(e, supports_table, media),
        CondExpr::And(items) => items.iter().all(|e| evaluate(e, supports_table, media)),
        CondExpr::Or(items) => items.iter().any(|e| evaluate(e, supports_table, media)),
        CondExpr::MediaFeature(name, value) => match (name.as_str(), value) {
            ("min-width", MediaValue::Length(px)) => media.width >= *px,
            ("max-width", MediaValue::Length(px)) => media.width <= *px,
            ("min-height", MediaValue::Length(px)) => media.height >= *px,
            ("max-height", MediaValue::Length(px)) => media.height <= *px,
            ("prefers-color-scheme", MediaValue::Keyword(k)) => media.color_scheme == *k,
            ("prefers-reduced-motion", MediaValue::Keyword(k)) =>
                (k == "reduce") == media.reduced_motion,
            ("any-hover", MediaValue::Keyword(k)) => match k.as_str() {
                "none" => media.hover == HoverCapability::None,
                "hover" => media.hover == HoverCapability::Hover,
                _ => false,
            },
            ("any-pointer", MediaValue::Keyword(k)) => match k.as_str() {
                "none" => media.pointer == PointerCapability::None,
                "coarse" => media.pointer == PointerCapability::Coarse,
                "fine" => media.pointer == PointerCapability::Fine,
                _ => false,
            },
            ("min-resolution", MediaValue::Resolution(d)) => media.device_pixel_ratio >= *d,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_with(props: &[(&str, &str, bool)]) -> HashMap<(String, String), bool> {
        let mut t = HashMap::new();
        for (p, v, b) in props {
            t.insert((p.to_string(), v.to_string()), *b);
        }
        t
    }

    #[test]
    fn supports_known() {
        let t = table_with(&[("display", "grid", true), ("display", "flex", true)]);
        assert_eq!(supports("display", "grid", &t), FeatureSupport::Supported);
        assert_eq!(supports("display", "subgrid", &t), FeatureSupport::Unknown);
    }

    #[test]
    fn supports_negation() {
        let t = table_with(&[("display", "grid", true)]);
        let m = MediaContext::default();
        let expr = CondExpr::Not(Box::new(CondExpr::Feature("display".into(), "subgrid".into())));
        // Unknown is treated as not supported here -> not(unknown) = not(false) = true.
        // Our evaluate: Feature returns false for Unknown, Not negates -> true.
        assert!(evaluate(&expr, &t, &m));
    }

    #[test]
    fn media_min_width() {
        let m = MediaContext { width: 800.0, ..Default::default() };
        let e = CondExpr::MediaFeature("min-width".into(), MediaValue::Length(600.0));
        assert!(evaluate(&e, &HashMap::new(), &m));
    }

    #[test]
    fn media_prefers_dark() {
        let m = MediaContext { color_scheme: "dark".into(), ..Default::default() };
        let e = CondExpr::MediaFeature("prefers-color-scheme".into(), MediaValue::Keyword("dark".into()));
        assert!(evaluate(&e, &HashMap::new(), &m));
    }

    #[test]
    fn and_short_circuit() {
        let m = MediaContext { width: 200.0, ..Default::default() };
        let e = CondExpr::And(vec![
            CondExpr::MediaFeature("min-width".into(), MediaValue::Length(100.0)),
            CondExpr::MediaFeature("min-width".into(), MediaValue::Length(500.0)),
        ]);
        assert!(!evaluate(&e, &HashMap::new(), &m));
    }
}
