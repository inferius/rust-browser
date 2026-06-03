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
    // ICU CLDR pouziva non-breaking space (U+00A0) jako tisicovy oddelovac
    assert_eq!(as_str(v), "1\u{00A0}234\u{00A0}567,89");
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
    // ICU CLDR: NBSP jako tisicovy oddelovac
    assert_eq!(as_str(v), "1\u{00A0}234,5");
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
fn intl_collator_case_sensitive() {
    // Real ICU: case-sensitive default, "Same" != "same"
    let v = run(r#"
        const col = new Intl.Collator("en-US");
        return col.compare("Same", "same") !== 0;
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn intl_collator_identical() {
    let v = run(r#"
        const col = new Intl.Collator("en-US");
        return col.compare("apple", "apple");
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

#[test]
fn intl_plural_rules_arabic() {
    // Arabic ma vsechny kategorie zero/one/two/few/many/other
    let v = run(r#"
        const pr = new Intl.PluralRules("ar");
        return pr.select(0) + ":" + pr.select(1) + ":" + pr.select(2);
    "#);
    assert_eq!(as_str(v), "zero:one:two");
}

#[test]
fn intl_plural_rules_polish() {
    let v = run(r#"
        const pr = new Intl.PluralRules("pl");
        return pr.select(1) + ":" + pr.select(2) + ":" + pr.select(5);
    "#);
    // pl: 1=one, 2-4=few, 5+=many
    assert_eq!(as_str(v), "one:few:many");
}

#[test]
fn intl_number_format_arabic() {
    // Arabic locale ma uplne jine cisla (Eastern Arabic numerals)
    let v = run(r#"
        const fmt = new Intl.NumberFormat("ar");
        return typeof fmt.format(123);
    "#);
    assert_eq!(as_str(v), "string");
}

#[test]
fn intl_number_format_thai() {
    let v = run(r#"
        const fmt = new Intl.NumberFormat("th");
        return typeof fmt.format(1234);
    "#);
    assert_eq!(as_str(v), "string");
}

#[test]
fn intl_datetime_de_format() {
    let v = run(r#"
        const fmt = new Intl.DateTimeFormat("de-DE");
        return typeof fmt.format(new Date(0));
    "#);
    assert_eq!(as_str(v), "string");
}

#[test]
fn intl_collator_czech_diacritics() {
    // Real ICU: ceske razeni - 'a' vs 'á' vs 'b'
    // Czech sort: a < á < b
    let v = run(r#"
        const col = new Intl.Collator("cs-CZ");
        return col.compare("á", "b") < 0;
    "#);
    assert_eq!(as_bool(v), true);
}
