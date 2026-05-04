/// Pole - literal, mutace, metody (push/pop, map, filter, reduce, ...).

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn array_literal_access() {
    assert_eq!(as_num(run(r#"
        let arr = [10, 20, 30];
        return arr[1];
    "#)), 20.0);
}

#[test]
fn array_mutation() {
    assert_eq!(as_num(run(r#"
        let arr = [1, 2, 3];
        arr[0] = 99;
        return arr[0];
    "#)), 99.0);
}

#[test]
fn array_length() {
    assert_eq!(as_num(run(r#"
        let arr = [1, 2, 3];
        return arr.length;
    "#)), 3.0);
}

#[test]
fn array_push_pop() {
    assert_eq!(as_num(run(r#"
        const a = [1, 2, 3];
        a.push(4);
        return a.pop();
    "#)), 4.0);
}

#[test]
fn array_map() {
    assert_eq!(as_num(run(r#"
        const a = [1, 2, 3];
        const b = a.map(x => x * 2);
        return b[2];
    "#)), 6.0);
}

#[test]
fn array_filter() {
    assert_eq!(as_num(run(r#"
        const a = [1, 2, 3, 4, 5];
        const b = a.filter(x => x % 2 === 0);
        return b.length;
    "#)), 2.0);
}

#[test]
fn array_reduce() {
    assert_eq!(as_num(run(r#"
        const a = [1, 2, 3, 4];
        return a.reduce((acc, x) => acc + x, 0);
    "#)), 10.0);
}

#[test]
fn array_find() {
    assert_eq!(as_num(run(r#"
        const a = [1, 2, 3, 4];
        return a.find(x => x > 2);
    "#)), 3.0);
}

#[test]
fn array_includes() {
    assert!(as_bool(run(r#"return [1, 2, 3].includes(2);"#)));
    assert!(!as_bool(run(r#"return [1, 2, 3].includes(5);"#)));
}

#[test]
fn array_join() {
    assert_eq!(as_str(run(r#"return [1, 2, 3].join("-");"#)), "1-2-3");
}

#[test]
fn array_slice() {
    assert_eq!(as_num(run(r#"
        const a = [1, 2, 3, 4, 5];
        return a.slice(1, 3).length;
    "#)), 2.0);
}

#[test]
fn array_every_some() {
    assert!(as_bool(run(r#"return [2, 4, 6].every(x => x % 2 === 0);"#)));
    assert!(!as_bool(run(r#"return [1, 2, 3].every(x => x % 2 === 0);"#)));
    assert!(as_bool(run(r#"return [1, 2, 3].some(x => x % 2 === 0);"#)));
    assert!(!as_bool(run(r#"return [1, 3, 5].some(x => x % 2 === 0);"#)));
}

#[test]
fn array_flat() {
    assert_eq!(as_num(run(r#"
        return [[1, 2], [3, 4]].flat().length;
    "#)), 4.0);
}

#[test]
fn array_flat_depth() {
    assert_eq!(as_num(run(r#"
        return [1, [2, 3], [4]].flat().length;
    "#)), 4.0);
    assert_eq!(as_num(run(r#"
        return [[1, [2]], [3]].flat(2).length;
    "#)), 3.0);
}

#[test]
fn array_flat_map() {
    assert_eq!(as_num(run(r#"
        return [1, 2, 3].flatMap(x => [x, x * 2]).length;
    "#)), 6.0);
}

#[test]
fn array_isarray() {
    assert!(as_bool(run(r#"return Array.isArray([1, 2, 3]);"#)));
    assert!(!as_bool(run(r#"return Array.isArray("hello");"#)));
}

#[test]
fn array_foreach() {
    assert_eq!(as_num(run(r#"
        let sum = 0;
        [1, 2, 3].forEach(x => { sum += x; });
        return sum;
    "#)), 6.0);
}

#[test]
fn array_at() {
    assert_eq!(as_num(run(r#"return [1,2,3].at(0);"#)), 1.0);
    assert_eq!(as_num(run(r#"return [1,2,3].at(-1);"#)), 3.0);
    assert_eq!(as_num(run(r#"return [1,2,3].at(-2);"#)), 2.0);
    assert!(matches!(run(r#"return [1,2,3].at(10);"#), JsValue::Undefined));
}

#[test]
fn array_find_index() {
    assert_eq!(as_num(run(r#"
        return [10, 20, 30].findIndex(x => x > 15);
    "#)), 1.0);
    assert_eq!(as_num(run(r#"
        return [10, 20, 30].findIndex(x => x > 100);
    "#)), -1.0);
}

#[test]
fn array_of() {
    assert_eq!(as_num(run(r#"
        const a = Array.of(1, 2, 3);
        return a.length;
    "#)), 3.0);
}

#[test]
fn array_from_set() {
    assert_eq!(as_num(run(r#"
        const s = new Set([1, 2, 3]);
        const a = Array.from(s);
        return a.length;
    "#)), 3.0);
}
