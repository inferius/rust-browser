/// String literaly, koncenace, metody.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn string_concat() {
    assert_eq!(as_str(eval(r#""hello" + " " + "world""#)), "hello world");
}

#[test]
fn string_coercion() {
    assert_eq!(as_str(eval(r#""val: " + 42"#)), "val: 42");
}

#[test]
fn string_includes() {
    assert!(as_bool(run(r#"return "hello world".includes("world");"#)));
    assert!(!as_bool(run(r#"return "hello world".includes("xyz");"#)));
}

#[test]
fn string_starts_ends_with() {
    assert!(as_bool(run(r#"return "hello".startsWith("he");"#)));
    assert!(as_bool(run(r#"return "hello".endsWith("lo");"#)));
    assert!(!as_bool(run(r#"return "hello".startsWith("lo");"#)));
}

#[test]
fn string_slice() {
    assert_eq!(as_str(run(r#"return "hello world".slice(6);"#)), "world");
    assert_eq!(as_str(run(r#"return "hello world".slice(0, 5);"#)), "hello");
}

#[test]
fn string_split() {
    assert_eq!(as_num(run(r#"return "a,b,c".split(",").length;"#)), 3.0);
    assert_eq!(as_str(run(r#"return "a,b,c".split(",")[1];"#)), "b");
}

#[test]
fn string_trim() {
    assert_eq!(as_str(run(r#"return "  hello  ".trim();"#)), "hello");
    assert_eq!(as_str(run(r#"return "  hello  ".trimStart();"#)), "hello  ");
    assert_eq!(as_str(run(r#"return "  hello  ".trimEnd();"#)), "  hello");
}

#[test]
fn string_to_upper_lower() {
    assert_eq!(as_str(run(r#"return "Hello".toUpperCase();"#)), "HELLO");
    assert_eq!(as_str(run(r#"return "Hello".toLowerCase();"#)), "hello");
}

#[test]
fn string_pad() {
    assert_eq!(as_str(run(r#"return "5".padStart(3, "0");"#)), "005");
    assert_eq!(as_str(run(r#"return "5".padEnd(3, "0");"#)), "500");
}

#[test]
fn string_repeat() {
    assert_eq!(as_str(run(r#"return "ab".repeat(3);"#)), "ababab");
}

#[test]
fn string_replace() {
    assert_eq!(as_str(run(r#"return "hello world".replace("world", "JS");"#)), "hello JS");
}

#[test]
fn string_index_of() {
    assert_eq!(as_num(run(r#"return "hello".indexOf("l");"#)), 2.0);
    assert_eq!(as_num(run(r#"return "hello".indexOf("x");"#)), -1.0);
}

#[test]
fn string_at() {
    assert_eq!(as_str(eval(r#""hello".at(0)"#)), "h");
    assert_eq!(as_str(eval(r#""hello".at(-1)"#)), "o");
    assert!(matches!(eval(r#""hello".at(10)"#), JsValue::Undefined));
}

#[test]
fn string_from_char_code() {
    assert_eq!(as_str(eval("String.fromCharCode(65, 66, 67)")), "ABC");
    assert_eq!(as_str(eval("String.fromCharCode(72, 105)")), "Hi");
}

#[test]
fn string_char_code_at() {
    assert_eq!(as_num(eval(r#""A".charCodeAt(0)"#)), 65.0);
    assert_eq!(as_num(eval(r#""ABC".charCodeAt(1)"#)), 66.0);
    assert!(eval(r#""A".charCodeAt(10)"#).to_number().is_nan());
}

#[test]
fn for_of_with_string() {
    assert_eq!(as_num(run(r#"
        let count = 0;
        for (const ch of "abc") { count++; }
        return count;
    "#)), 3.0);
}

#[test]
fn array_destructuring_from_string() {
    assert_eq!(as_str(run(r#"
        const [a, b] = "hi";
        return a + b;
    "#)), "hi");
}
