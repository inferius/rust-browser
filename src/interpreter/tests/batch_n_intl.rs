/// Batch N - Intl (NumberFormat, DateTimeFormat, Collator, PluralRules).

use super::helpers::*;

#[test]
fn intl_number_format_en_us() {
    let v = run(r#"
        const fmt = new Intl.NumberFormat("en-US");
        return fmt.format(1234567.89);
    "#);
    assert_eq!(as_str(v), "1,234,567.89");
}

#[test]
fn intl_number_format_cs_cz() {
    let v = run(r#"
        const fmt = new Intl.NumberFormat("cs-CZ");
        return fmt.format(1234567.89);
    "#);
    assert_eq!(as_str(v), "1 234 567,89");
}

#[test]
fn intl_number_format_de_de() {
    let v = run(r#"
        const fmt = new Intl.NumberFormat("de-DE");
        return fmt.format(1234567.89);
    "#);
    assert_eq!(as_str(v), "1.234.567,89");
}

#[test]
fn number_to_locale_string_with_locale() {
    let v = run(r#"return (1234.5).toLocaleString("cs-CZ");"#);
    assert_eq!(as_str(v), "1 234,5");
}

#[test]
fn intl_datetime_format_en_us() {
    let v = run(r#"
        const fmt = new Intl.DateTimeFormat("en-US");
        return fmt.format(new Date(1718454645500));
    "#);
    // 2024-06-15 12:30:45 UTC
    assert!(as_str(v).contains("2024"));
}

#[test]
fn intl_datetime_format_cs() {
    let v = run(r#"
        const fmt = new Intl.DateTimeFormat("cs-CZ");
        return fmt.format(new Date(1718454645500));
    "#);
    let s = as_str(v);
    // "15. 6. 2024 ..."
    assert!(s.contains("2024") && s.contains("15"));
}

#[test]
fn intl_collator_compare() {
    let v = run(r#"
        const col = new Intl.Collator("en-US");
        return col.compare("apple", "banana");
    "#);
    assert_eq!(as_num(v), -1.0);
}

#[test]
fn intl_collator_equal() {
    let v = run(r#"
        const col = new Intl.Collator("en-US");
        return col.compare("Same", "same");
    "#);
    assert_eq!(as_num(v), 0.0);
}

#[test]
fn intl_plural_rules_en() {
    let v = run(r#"
        const pr = new Intl.PluralRules("en-US");
        return pr.select(1) + ":" + pr.select(2);
    "#);
    assert_eq!(as_str(v), "one:other");
}

#[test]
fn intl_plural_rules_cs() {
    let v = run(r#"
        const pr = new Intl.PluralRules("cs-CZ");
        return pr.select(1) + ":" + pr.select(3) + ":" + pr.select(10);
    "#);
    assert_eq!(as_str(v), "one:few:other");
}
