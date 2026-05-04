/// JSON.stringify/parse, Date, Error typy.

use super::helpers::*;
use crate::interpreter::JsValue;

// ─── JSON ────────────────────────────────────────────────────────────────

#[test]
fn json_stringify_number() {
    assert_eq!(as_str(run(r#"return JSON.stringify(42);"#)), "42");
}

#[test]
fn json_stringify_string() {
    assert_eq!(as_str(run(r#"return JSON.stringify("hello");"#)), "\"hello\"");
}

#[test]
fn json_stringify_bool_null() {
    assert_eq!(as_str(run(r#"return JSON.stringify(true);"#)), "true");
    assert_eq!(as_str(run(r#"return JSON.stringify(null);"#)), "null");
}

#[test]
fn json_stringify_array() {
    assert_eq!(as_str(run(r#"return JSON.stringify([1,2,3]);"#)), "[1,2,3]");
}

#[test]
fn json_stringify_object() {
    assert_eq!(as_str(run(r#"return JSON.stringify({a:1,b:"x"});"#)), r#"{"a":1,"b":"x"}"#);
}

#[test]
fn json_stringify_nested() {
    assert_eq!(as_str(run(r#"return JSON.stringify({a:[1,2],b:{c:3}});"#)),
        r#"{"a":[1,2],"b":{"c":3}}"#);
}

#[test]
fn json_stringify_undefined_omitted() {
    assert_eq!(as_str(run(r#"return JSON.stringify({a:1,b:undefined,c:2});"#)),
        r#"{"a":1,"c":2}"#);
}

#[test]
fn json_parse_number() {
    assert_eq!(as_num(run(r#"return JSON.parse("42");"#)), 42.0);
}

#[test]
fn json_parse_string() {
    assert_eq!(as_str(run(r#"return JSON.parse('"hello"');"#)), "hello");
}

#[test]
fn json_parse_array() {
    assert_eq!(as_num(run(r#"
        const a = JSON.parse('[1,2,3]');
        return a[1];
    "#)), 2.0);
}

#[test]
fn json_parse_object() {
    assert_eq!(as_num(run(r#"
        const o = JSON.parse('{"x":10,"y":20}');
        return o.x + o.y;
    "#)), 30.0);
}

#[test]
fn json_roundtrip() {
    assert_eq!(as_num(run(r#"
        const obj = {x:42, y:99};
        const s = JSON.stringify(obj);
        const o2 = JSON.parse(s);
        return o2.x + o2.y;
    "#)), 141.0);
    assert_eq!(as_num(run(r#"
        const arr = [1, 2, 3, 4, 5];
        const s = JSON.stringify(arr);
        const a2 = JSON.parse(s);
        return a2[0] + a2[4];
    "#)), 6.0);
}

// ─── Date ────────────────────────────────────────────────────────────────

#[test]
fn date_now_is_number() {
    assert!(matches!(run(r#"return Date.now();"#), JsValue::Number(_)));
}

#[test]
fn date_constructor_epoch() {
    assert_eq!(as_str(run(r#"return new Date(0).toISOString();"#)),
        "1970-01-01T00:00:00.000Z");
}

#[test]
fn date_get_time() {
    assert_eq!(as_num(run(r#"return new Date(1000).getTime();"#)), 1000.0);
}

#[test]
fn date_get_full_year() {
    assert_eq!(as_num(run(r#"return new Date(946684800000).getFullYear();"#)), 2000.0);
}

#[test]
fn date_get_month() {
    assert_eq!(as_num(run(r#"return new Date(946684800000).getMonth();"#)), 0.0);
}

#[test]
fn date_get_date() {
    assert_eq!(as_num(run(r#"return new Date(946684800000).getDate();"#)), 1.0);
}

#[test]
fn date_to_iso_string_known() {
    assert_eq!(as_str(run(r#"return new Date(1718454645500).toISOString();"#)),
        "2024-06-15T12:30:45.500Z");
}

// ─── Error typy ──────────────────────────────────────────────────────────

#[test]
fn error_basic() {
    assert_eq!(as_str(run(r#"
        const e = new Error("oops");
        return e.message;
    "#)), "oops");
}

#[test]
fn error_name() {
    assert_eq!(as_str(run(r#"
        const e = new Error("x");
        return e.name;
    "#)), "Error");
}

#[test]
fn error_type_error() {
    assert_eq!(as_str(run(r#"
        const e = new TypeError("bad type");
        return e.name + ": " + e.message;
    "#)), "TypeError: bad type");
}

#[test]
fn error_range_error() {
    assert_eq!(as_str(run(r#"
        const e = new RangeError("out of range");
        return e.name;
    "#)), "RangeError");
}

#[test]
fn error_throw_catch() {
    assert_eq!(as_str(run(r#"
        let msg = "";
        try {
            throw new TypeError("caught me");
        } catch (e) {
            msg = e.message;
        }
        return msg;
    "#)), "caught me");
}

#[test]
fn error_instanceof_check() {
    assert_eq!(as_str(run(r#"
        try {
            throw new RangeError("r");
        } catch (e) {
            return e.name;
        }
    "#)), "RangeError");
}

#[test]
fn error_no_message() {
    assert_eq!(as_str(run(r#"
        const e = new Error();
        return e.message;
    "#)), "");
}
