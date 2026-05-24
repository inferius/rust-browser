//! Intl.PluralRules - CLDR plural categories.
//!
//! Spec: https://tc39.es/ecma402/#sec-intl-pluralrules-constructor

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

/// Compute plural category per CLDR rules (subset of locales).
pub fn category(locale: &str, n: f64) -> PluralCategory {
    let abs = n.abs();
    let is_int = abs.fract() == 0.0;
    let i = abs.floor() as i64;
    let lang = locale.split('-').next().unwrap_or("");

    match lang {
        "en" => {
            if is_int && i == 1 && abs == 1.0 { PluralCategory::One }
            else { PluralCategory::Other }
        }
        "cs" | "sk" => {
            if is_int && i == 1 { PluralCategory::One }
            else if is_int && (2..=4).contains(&i) { PluralCategory::Few }
            else if !is_int { PluralCategory::Many }
            else { PluralCategory::Other }
        }
        "ru" | "uk" | "be" => {
            let mod10 = i % 10;
            let mod100 = i % 100;
            if mod10 == 1 && mod100 != 11 { PluralCategory::One }
            else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) { PluralCategory::Few }
            else { PluralCategory::Many }
        }
        "pl" => {
            let mod10 = i % 10;
            let mod100 = i % 100;
            if i == 1 { PluralCategory::One }
            else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) { PluralCategory::Few }
            else { PluralCategory::Many }
        }
        "ar" => {
            if abs == 0.0 { PluralCategory::Zero }
            else if abs == 1.0 { PluralCategory::One }
            else if abs == 2.0 { PluralCategory::Two }
            else if (3..=10).contains(&(i % 100)) { PluralCategory::Few }
            else if (11..=99).contains(&(i % 100)) { PluralCategory::Many }
            else { PluralCategory::Other }
        }
        "ja" | "zh" | "ko" | "th" | "vi" => PluralCategory::Other,
        _ => {
            if is_int && i == 1 { PluralCategory::One } else { PluralCategory::Other }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_one_other() {
        assert_eq!(category("en", 1.0), PluralCategory::One);
        assert_eq!(category("en", 2.0), PluralCategory::Other);
        assert_eq!(category("en", 0.0), PluralCategory::Other);
    }

    #[test]
    fn czech_few() {
        assert_eq!(category("cs", 1.0), PluralCategory::One);
        assert_eq!(category("cs", 2.0), PluralCategory::Few);
        assert_eq!(category("cs", 5.0), PluralCategory::Other);
        assert_eq!(category("cs", 1.5), PluralCategory::Many);
    }

    #[test]
    fn russian_categories() {
        assert_eq!(category("ru", 1.0), PluralCategory::One);
        assert_eq!(category("ru", 21.0), PluralCategory::One);
        assert_eq!(category("ru", 22.0), PluralCategory::Few);
        assert_eq!(category("ru", 11.0), PluralCategory::Many);
    }

    #[test]
    fn arabic_zero_two() {
        assert_eq!(category("ar", 0.0), PluralCategory::Zero);
        assert_eq!(category("ar", 1.0), PluralCategory::One);
        assert_eq!(category("ar", 2.0), PluralCategory::Two);
    }

    #[test]
    fn chinese_always_other() {
        assert_eq!(category("zh", 1.0), PluralCategory::Other);
        assert_eq!(category("zh", 100.0), PluralCategory::Other);
    }
}
