//! Cross-Origin Embedder Policy (COEP) + Cross-Origin Opener Policy (COOP) + CORP.
//!
//! Specs:
//! - https://html.spec.whatwg.org/multipage/origin.html#cross-origin-embedder-policies
//! - https://html.spec.whatwg.org/multipage/origin.html#cross-origin-opener-policies
//! - https://fetch.spec.whatwg.org/#cross-origin-resource-policy-header
//!
//! Crossover: COOP isolates browsing-context groups (Spectre mitigation).
//! COEP requires that every subresource opts in via CORP or CORS.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coep {
    UnsafeNone,
    RequireCorp,
    Credentialless,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coop {
    UnsafeNone,
    SameOriginAllowPopups,
    SameOrigin,
    SameOriginPlusCoep,
    NoopenerAllowPopups,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Corp {
    SameOrigin,
    SameSite,
    CrossOrigin,
}

impl Coep {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "require-corp" => Self::RequireCorp,
            "credentialless" => Self::Credentialless,
            _ => Self::UnsafeNone,
        }
    }
}

impl Coop {
    pub fn parse(s: &str) -> Self {
        let lower = s.trim().to_ascii_lowercase();
        // parameters after value, e.g. "same-origin-allow-popups; report-to=foo"
        let main = lower.split(';').next().unwrap_or("").trim();
        match main {
            "same-origin-allow-popups" => Self::SameOriginAllowPopups,
            "same-origin" => Self::SameOrigin,
            "same-origin-plus-coep" => Self::SameOriginPlusCoep,
            "noopener-allow-popups" => Self::NoopenerAllowPopups,
            _ => Self::UnsafeNone,
        }
    }
}

impl Corp {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "same-origin" => Some(Self::SameOrigin),
            "same-site" => Some(Self::SameSite),
            "cross-origin" => Some(Self::CrossOrigin),
            _ => None,
        }
    }
}

/// Check if a subresource is allowed in a COEP: require-corp document.
pub fn coep_allows_response(
    coep: Coep,
    is_same_origin: bool,
    response_corp: Option<Corp>,
    response_has_cors: bool,
    is_credentialless_request: bool,
) -> bool {
    if coep == Coep::UnsafeNone { return true; }
    if is_same_origin { return true; }
    if coep == Coep::Credentialless && is_credentialless_request { return true; }
    if response_has_cors { return true; }
    match response_corp {
        Some(Corp::CrossOrigin) => true,
        Some(Corp::SameSite) | Some(Corp::SameOrigin) => false,
        None => false,
    }
}

/// COOP isolation check between two browsing contexts during navigation.
/// Returns true if BCG (browsing-context-group) must be switched (isolated).
pub fn coop_requires_isolation(prev: Coop, next: Coop, is_same_origin: bool) -> bool {
    if prev == Coop::UnsafeNone && next == Coop::UnsafeNone { return false; }
    if prev == next && is_same_origin { return false; }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coep_parse() {
        assert_eq!(Coep::parse("require-corp"), Coep::RequireCorp);
        assert_eq!(Coep::parse("Credentialless"), Coep::Credentialless);
        assert_eq!(Coep::parse("unknown"), Coep::UnsafeNone);
    }

    #[test]
    fn coop_parse_strips_params() {
        assert_eq!(Coop::parse("same-origin; report-to=\"r\""), Coop::SameOrigin);
    }

    #[test]
    fn coep_unsafe_none_allows_all() {
        assert!(coep_allows_response(Coep::UnsafeNone, false, None, false, false));
    }

    #[test]
    fn coep_require_corp_blocks_no_corp() {
        assert!(!coep_allows_response(Coep::RequireCorp, false, None, false, false));
    }

    #[test]
    fn coep_require_corp_allows_cross_origin_corp() {
        assert!(coep_allows_response(Coep::RequireCorp, false, Some(Corp::CrossOrigin), false, false));
    }

    #[test]
    fn coep_credentialless_allows_credentialless_request() {
        assert!(coep_allows_response(Coep::Credentialless, false, None, false, true));
    }

    #[test]
    fn coop_same_value_same_origin_no_isolation() {
        assert!(!coop_requires_isolation(Coop::SameOrigin, Coop::SameOrigin, true));
    }

    #[test]
    fn coop_cross_origin_isolation() {
        assert!(coop_requires_isolation(Coop::SameOrigin, Coop::SameOrigin, false));
    }

    #[test]
    fn coop_change_isolation() {
        assert!(coop_requires_isolation(Coop::UnsafeNone, Coop::SameOrigin, true));
    }
}
