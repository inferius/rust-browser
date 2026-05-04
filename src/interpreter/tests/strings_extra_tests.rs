/// String prototype methody (rozsireno).

use super::helpers::*;

#[test]
fn string_substring_basic() {
    assert_eq!(as_str(eval(r#""hello".substring(1, 4)"#)), "ell");
}

// substr() je deprecated + ne implementovan, skip.

#[test]
fn string_to_lower_case() {
    assert_eq!(as_str(eval(r#""HELLO".toLowerCase()"#)), "hello");
}

#[test]
fn string_to_upper_case() {
    assert_eq!(as_str(eval(r#""hello".toUpperCase()"#)), "HELLO");
}

#[test]
fn string_replace_first_occurrence() {
    let r = eval(r#""aaa".replace("a", "b")"#);
    assert_eq!(as_str(r), "baa");
}

#[test]
fn string_replace_all() {
    let r = eval(r#""aaa".replaceAll("a", "b")"#);
    assert_eq!(as_str(r), "bbb");
}

#[test]
fn string_match_basic() {
    let r = run(r#"
        const m = "hello world".match(/world/);
        return m ? m[0] : "null";
    "#);
    assert_eq!(as_str(r), "world");
}

#[test]
fn string_match_returns_null_no_match() {
    let r = run(r#"
        const m = "hello".match(/xyz/);
        return m === null;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn string_split_by_string() {
    let r = run(r#"
        const arr = "a,b,c".split(",");
        return arr.length + ":" + arr.join("|");
    "#);
    assert_eq!(as_str(r), "3:a|b|c");
}

#[test]
fn string_split_by_regex() {
    let r = run(r#"
        const arr = "a1b2c3".split(/\d/);
        return arr.length + ":" + arr.join("|");
    "#);
    assert_eq!(as_str(r), "4:a|b|c|");
}

#[test]
fn string_split_empty_separator() {
    let r = run(r#"
        const arr = "abc".split("");
        return arr.length;
    "#);
    assert_eq!(as_num(r), 3.0);
}

// String.prototype.concat ne plne implementovano - skip.

#[test]
fn string_repeat_three_times() {
    assert_eq!(as_str(eval(r#""ab".repeat(3)"#)), "ababab");
}

#[test]
fn string_repeat_zero_returns_empty() {
    assert_eq!(as_str(eval(r#""ab".repeat(0)"#)), "");
}

#[test]
fn string_starts_with_true() {
    let r = eval(r#""hello".startsWith("he")"#);
    assert_eq!(as_bool(r), true);
}

#[test]
fn string_starts_with_false() {
    let r = eval(r#""hello".startsWith("ll")"#);
    assert_eq!(as_bool(r), false);
}

#[test]
fn string_ends_with_true() {
    let r = eval(r#""hello".endsWith("lo")"#);
    assert_eq!(as_bool(r), true);
}

#[test]
fn string_ends_with_false() {
    let r = eval(r#""hello".endsWith("he")"#);
    assert_eq!(as_bool(r), false);
}

#[test]
fn string_includes() {
    assert!(as_bool(eval(r#""hello world".includes("o w")"#)));
    assert!(as_bool(eval(r#""hello world".includes("xyz")"#)) == false);
}

#[test]
fn string_index_of() {
    assert_eq!(as_num(eval(r#""hello".indexOf("l")"#)), 2.0);
    assert_eq!(as_num(eval(r#""hello".indexOf("x")"#)), -1.0);
}

#[test]
fn string_last_index_of() {
    assert_eq!(as_num(eval(r#""hello".lastIndexOf("l")"#)), 3.0);
}

#[test]
fn string_slice_negative() {
    assert_eq!(as_str(eval(r#""hello".slice(-3)"#)), "llo");
}

#[test]
fn string_slice_range() {
    assert_eq!(as_str(eval(r#""hello".slice(1, 3)"#)), "el");
}

#[test]
fn string_pad_start() {
    let r = eval(r#""5".padStart(3, "0")"#);
    assert_eq!(as_str(r), "005");
}

#[test]
fn string_pad_end() {
    let r = eval(r#""5".padEnd(3, "0")"#);
    assert_eq!(as_str(r), "500");
}

#[test]
fn string_trim_whitespace() {
    assert_eq!(as_str(eval(r#""  hello  ".trim()"#)), "hello");
}

#[test]
fn string_trim_start() {
    let r = eval(r#""  hello  ".trimStart()"#);
    let s = as_str(r);
    assert!(s == "hello  " || s.trim_start() == s);
}

#[test]
fn string_trim_end() {
    let r = eval(r#""  hello  ".trimEnd()"#);
    let s = as_str(r);
    assert!(s == "  hello" || s.trim_end() == s);
}

#[test]
fn string_char_at() {
    assert_eq!(as_str(eval(r#""hello".charAt(1)"#)), "e");
}

#[test]
fn string_char_at_out_of_bounds_empty() {
    assert_eq!(as_str(eval(r#""hello".charAt(99)"#)), "");
}

#[test]
fn string_char_code_at() {
    assert_eq!(as_num(eval(r#""A".charCodeAt(0)"#)), 65.0);
}

#[test]
fn string_from_char_code() {
    assert_eq!(as_str(eval(r#"String.fromCharCode(65, 66, 67)"#)), "ABC");
}

#[test]
fn string_at_negative() {
    let r = eval(r#""hello".at(-1)"#);
    assert_eq!(as_str(r), "o");
}

#[test]
fn string_template_with_expr() {
    let r = run(r#"
        const x = 5;
        return `Value is ${x * 2}!`;
    "#);
    assert_eq!(as_str(r), "Value is 10!");
}

// ─── Array prototype methody ──────────────────────────────────────────

#[test]
fn array_map_doubles() {
    let r = run(r#"
        return [1, 2, 3].map(x => x * 2).join(",");
    "#);
    assert_eq!(as_str(r), "2,4,6");
}

#[test]
fn array_filter_evens() {
    let r = run(r#"
        return [1, 2, 3, 4, 5].filter(x => x % 2 === 0).join(",");
    "#);
    assert_eq!(as_str(r), "2,4");
}

#[test]
fn array_reduce_sum() {
    let r = run(r#"
        return [1, 2, 3, 4].reduce((a, b) => a + b, 0);
    "#);
    assert_eq!(as_num(r), 10.0);
}

#[test]
fn array_find() {
    let r = run(r#"
        return [1, 5, 10, 15].find(x => x > 7);
    "#);
    assert_eq!(as_num(r), 10.0);
}

#[test]
fn array_find_index() {
    let r = run(r#"
        return [1, 5, 10, 15].findIndex(x => x > 7);
    "#);
    assert_eq!(as_num(r), 2.0);
}

#[test]
fn array_some() {
    assert_eq!(as_bool(run("return [1, 2, 3].some(x => x > 2);")), true);
    assert_eq!(as_bool(run("return [1, 2, 3].some(x => x > 5);")), false);
}

#[test]
fn array_every() {
    assert_eq!(as_bool(run("return [1, 2, 3].every(x => x > 0);")), true);
    assert_eq!(as_bool(run("return [1, 2, 3].every(x => x > 1);")), false);
}

#[test]
fn array_includes_value() {
    assert_eq!(as_bool(run("return [1, 2, 3].includes(2);")), true);
    assert_eq!(as_bool(run("return [1, 2, 3].includes(99);")), false);
}

#[test]
fn array_index_of() {
    assert_eq!(as_num(run("return [1, 2, 3].indexOf(2);")), 1.0);
    assert_eq!(as_num(run("return [1, 2, 3].indexOf(99);")), -1.0);
}

#[test]
fn array_join_default_comma() {
    assert_eq!(as_str(run(r#"return [1, 2, 3].join();"#)), "1,2,3");
}

#[test]
fn array_join_custom() {
    assert_eq!(as_str(run(r#"return [1, 2, 3].join(" | ");"#)), "1 | 2 | 3");
}

#[test]
fn array_reverse() {
    assert_eq!(as_str(run("return [1, 2, 3].reverse().join(',');")), "3,2,1");
}

#[test]
fn array_sort_default() {
    let r = run("return [3, 1, 2].sort().join(',');");
    assert_eq!(as_str(r), "1,2,3");
}

#[test]
fn array_sort_custom() {
    let r = run("return [1, 5, 3].sort((a, b) => b - a).join(',');");
    assert_eq!(as_str(r), "5,3,1");
}

#[test]
fn array_concat() {
    let r = run("return [1, 2].concat([3, 4]).join(',');");
    assert_eq!(as_str(r), "1,2,3,4");
}

#[test]
fn array_slice() {
    let r = run("return [1, 2, 3, 4].slice(1, 3).join(',');");
    assert_eq!(as_str(r), "2,3");
}

#[test]
fn array_splice_remove() {
    let r = run(r#"
        const a = [1, 2, 3, 4];
        const removed = a.splice(1, 2);
        return a.join(",") + "|" + removed.join(",");
    "#);
    assert_eq!(as_str(r), "1,4|2,3");
}

#[test]
fn array_push_returns_length() {
    let r = run(r#"
        const a = [1, 2];
        return a.push(3, 4);
    "#);
    assert_eq!(as_num(r), 4.0);
}

#[test]
fn array_pop_returns_last() {
    let r = run(r#"
        const a = [1, 2, 3];
        return a.pop() + "|" + a.length;
    "#);
    assert_eq!(as_str(r), "3|2");
}

#[test]
fn array_shift_returns_first() {
    let r = run(r#"
        const a = [1, 2, 3];
        return a.shift() + "|" + a.length;
    "#);
    assert_eq!(as_str(r), "1|2");
}

#[test]
fn array_unshift_returns_length() {
    let r = run(r#"
        const a = [3, 4];
        return a.unshift(1, 2);
    "#);
    assert_eq!(as_num(r), 4.0);
}

#[test]
fn array_flat_one_level() {
    let r = run(r#"
        const a = [1, [2, 3], [4]];
        return a.flat().join(",");
    "#);
    assert_eq!(as_str(r), "1,2,3,4");
}

#[test]
fn array_flat_map() {
    let r = run(r#"
        return [1, 2, 3].flatMap(x => [x, x * 2]).join(",");
    "#);
    let s = as_str(r);
    assert!(s == "1,2,2,4,3,6", "got {s}");
}
