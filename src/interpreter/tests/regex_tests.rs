/// RegExp literal + new RegExp + test/exec/match/replace/search/split.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn regex_literal_test() {
    assert_eq!(as_bool(run(r#"return /hello/.test("say hello world");"#)), true);
    assert_eq!(as_bool(run(r#"return /hello/.test("goodbye");"#)), false);
}

#[test]
fn regex_flags_ignore_case() {
    assert_eq!(as_bool(run(r#"return /HELLO/i.test("hello world");"#)), true);
}

#[test]
fn regex_exec_returns_match() {
    assert_eq!(as_str(run(r#"
        const m = /(\d+)/.exec("abc 123 def");
        return m[1];
    "#)), "123");
}

#[test]
fn regex_exec_no_match() {
    assert!(matches!(run(r#"return /xyz/.exec("hello");"#), JsValue::Null));
}

#[test]
fn regex_source_flags() {
    assert_eq!(as_str(run(r#"
        const re = /foo/gi;
        return re.source + "/" + re.flags;
    "#)), "foo/gi");
}

#[test]
fn string_match_global() {
    assert_eq!(as_num(run(r#"
        const matches = "one1two2three3".match(/\d/g);
        return matches.length;
    "#)), 3.0);
}

#[test]
fn string_match_no_g() {
    assert_eq!(as_str(run(r#"
        const m = "price: 42".match(/(\d+)/);
        return m[1];
    "#)), "42");
}

#[test]
fn string_match_null() {
    assert!(matches!(run(r#"return "hello".match(/\d/);"#), JsValue::Null));
}

#[test]
fn string_replace_regex() {
    assert_eq!(as_str(run(r#"return "hello world".replace(/o/, "0");"#)), "hell0 world");
}

#[test]
fn string_replace_all_regex() {
    assert_eq!(as_str(run(r#"return "hello world".replaceAll(/o/g, "0");"#)), "hell0 w0rld");
}

#[test]
fn string_search_regex() {
    assert_eq!(as_num(run(r#"return "abc 123".search(/\d+/);"#)), 4.0);
    assert_eq!(as_num(run(r#"return "abc".search(/\d+/);"#)), -1.0);
}

#[test]
fn string_split_regex() {
    assert_eq!(as_num(run(r#"
        const parts = "one1two2three".split(/\d/);
        return parts.length;
    "#)), 3.0);
}

#[test]
fn new_regexp_constructor() {
    assert_eq!(as_bool(run(r#"
        const re = new RegExp("\\d+", "g");
        return re.test("abc 123");
    "#)), true);
}

#[test]
fn regex_to_string() {
    assert_eq!(as_str(run(r#"
        const re = /foo/gi;
        return re.toString();
    "#)), "/foo/gi");
}
