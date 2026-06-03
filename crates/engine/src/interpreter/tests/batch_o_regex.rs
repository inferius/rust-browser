/// Batch O - RegExp /v flag, named groups, lookbehind (omezene).

use super::helpers::*;

#[test]
fn regex_v_flag_accepted() {
    // /v flag (Unicode sets, ES2024) - akceptujeme stejne jako /u
    let v = run(r#"
        const re = /[a-z]/v;
        return re.test("hello");
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_named_groups_match() {
    // (?<name>...) - named capture groups (Rust regex je podporuje)
    let v = run(r#"
        const re = /(?<year>\d{4})-(?<month>\d{2})/;
        return re.test("2024-06");
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_named_groups_in_match() {
    let v = run(r#"
        const m = "2024-06-15".match(/(?<y>\d{4})-(?<m>\d{2})-(?<d>\d{2})/);
        return m[0];
    "#);
    assert_eq!(as_str(v), "2024-06-15");
}

#[test]
fn regex_alternation() {
    let v = run(r#"
        return /foo|bar|baz/.test("xxx bar xxx");
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_quantifiers() {
    let v = run(r#"
        return /\d{3}/.test("abc 123 def");
    "#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_anchors() {
    assert_eq!(as_bool(run(r#"return /^hello/.test("hello world");"#)), true);
    assert_eq!(as_bool(run(r#"return /world$/.test("hello world");"#)), true);
    assert_eq!(as_bool(run(r#"return /^hello$/.test("hello world");"#)), false);
}

#[test]
fn regex_word_boundary() {
    let v = run(r#"return /\bword\b/.test("a word here");"#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_unicode_flag() {
    // /u flag - Unicode awareness
    let v = run(r#"
        return /\d/u.test("12");
    "#);
    assert_eq!(as_bool(v), true);
}

// ─── Lookbehind (fancy-regex) ───────────────────────────────────────────

#[test]
fn regex_positive_lookbehind() {
    // (?<=$)\d+ - match cisla po dollar znaku
    let v = run(r#"return /(?<=\$)\d+/.test("price: $42");"#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_negative_lookbehind() {
    // (?<!$)\d+ - cisla NE po dollar
    let v = run(r#"return /(?<!\$)\d+/.test("count: 42");"#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_lookbehind_extract() {
    let v = run(r#"
        const m = /(?<=USD\s)\d+/.exec("price USD 100 EUR 200");
        return m[0];
    "#);
    assert_eq!(as_str(v), "100");
}

#[test]
fn regex_backreference() {
    // Backreference \1 - opakuje predchozi capture
    let v = run(r#"return /(\w+)\s\1/.test("hello hello");"#);
    assert_eq!(as_bool(v), true);
}

#[test]
fn regex_backreference_no_match() {
    let v = run(r#"return /(\w+)\s\1/.test("hello world");"#);
    assert_eq!(as_bool(v), false);
}
