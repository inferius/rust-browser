//! CascadeDecl - raw + parsed CSS declaration s validity flag.
//!
//! L5 stage 2b: cascade output zachova VSECHNY declarations (vc. invalid)
//! pro devtools strikethrough display + warning icons. Computed style
//! aplikuje JEN valid declarations.
//!
//! CSS spec §3.4: invalid declaration = discard (ne aplikovat). Browser
//! devtools UI ji ale ukazuje preskrtnutou s warning icon - to vyzaduje
//! ulozit raw value + parsed Option.

use super::property::PropertyId;

/// Specificity tuple (a, b, c) per CSS Selectors L4 §16:
/// - a = inline style (1 nebo 0)
/// - b = ID selectors count
/// - c = class/attr/pseudo-class count
/// - d = element/pseudo-element count
/// Sortuje pri cascade resolution (vyssi wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Specificity {
    pub inline: u16,
    pub id: u16,
    pub class: u16,
    pub element: u16,
}

impl Specificity {
    pub const ZERO: Self = Self { inline: 0, id: 0, class: 0, element: 0 };

    pub fn inline_style() -> Self {
        Self { inline: 1, id: 0, class: 0, element: 0 }
    }

    pub fn new(id: u16, class: u16, element: u16) -> Self {
        Self { inline: 0, id, class, element }
    }
}

/// Single CSS declaration s raw vstupem + (po stage 3) typed parsed value.
/// Pri stage 2b ulozime jen raw + valid flag (typed parse az v stage 3).
#[derive(Debug, Clone)]
pub struct CascadeDecl {
    /// Typed property identifier (PropertyId::Unknown pri neznamem name).
    pub property: PropertyId,
    /// Puvodni CSS string presne tak jak user napsal. Pro devtools display.
    pub raw_value: String,
    /// True pokud raw_value je syntakticky valid pro tento property.
    /// False = invalid -> cascade preskoci, devtools strikethrough.
    pub valid: bool,
    /// !important flag (CSS Cascade L4 §6.2).
    pub important: bool,
    /// Specificity selectoru ktery declaration vlozil.
    pub specificity: Specificity,
    /// Origin: User-agent / Author / User (CSS Cascade L4 §6.1).
    pub origin: CascadeOrigin,
    /// Source order index - pri stejne specificity wins later.
    pub source_order: u32,
}

/// CSS Cascade L4 §6.1 - origin urcuje cascade priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeOrigin {
    /// Browser default stylesheet (lowest priority unless !important).
    UserAgent,
    /// User-set styles (e.g. browser preferences).
    User,
    /// Author stylesheets (typical CSS rules + inline style).
    Author,
}

impl CascadeDecl {
    /// Sentinel pro neexistujici declaration.
    pub fn unknown_invalid(raw: String) -> Self {
        Self {
            property: PropertyId::Unknown,
            raw_value: raw,
            valid: false,
            important: false,
            specificity: Specificity::ZERO,
            origin: CascadeOrigin::Author,
            source_order: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specificity_ordering() {
        let s1 = Specificity::new(0, 1, 0);  // .class
        let s2 = Specificity::new(1, 0, 0);  // #id
        let s3 = Specificity::inline_style(); // style=
        assert!(s2 > s1, "id beats class");
        assert!(s3 > s2, "inline beats id");
    }

    #[test]
    fn cascade_origin_distinct() {
        let ua = CascadeOrigin::UserAgent;
        let auth = CascadeOrigin::Author;
        assert_ne!(ua, auth);
    }

    #[test]
    fn unknown_decl() {
        let d = CascadeDecl::unknown_invalid("blah-blah".to_string());
        assert_eq!(d.property, PropertyId::Unknown);
        assert!(!d.valid);
        assert!(!d.important);
        assert_eq!(d.raw_value, "blah-blah");
    }
}
