/// Batch K - ES2023+ moderni features.
/// Array immutable varianty, findLast, copyWithin, Set ops, groupBy,
/// Promise.withResolvers/try, Error.cause.

use super::helpers::*;
use crate::interpreter::JsValue;

// ─── ES2023 immutable Array varianty ────────────────────────────

#[test]
fn array_to_sorted() {
    let v = run(r#"
        const a = [3, 1, 2];
        const b = a.toSorted();
        return a[0] + "," + b[0];
    "#);
    assert_eq!(as_str(v), "3,1");
}

#[test]
fn array_to_reversed() {
    let v = run(r#"
        const a = [1, 2, 3];
        const b = a.toReversed();
        return a[0] + "," + b[0];
    "#);
    assert_eq!(as_str(v), "1,3");
}

#[test]
fn array_to_spliced() {
    let v = run(r#"
        const a = [1, 2, 3, 4];
        const b = a.toSpliced(1, 2, 99, 100);
        return a.length + ":" + b.length + ":" + b[1] + "," + b[2];
    "#);
    assert_eq!(as_str(v), "4:4:99,100");
}

#[test]
fn array_with_method() {
    let v = run(r#"
        const a = [1, 2, 3];
        const b = a.with(1, 99);
        return a[1] + "," + b[1];
    "#);
    assert_eq!(as_str(v), "2,99");
}

#[test]
fn array_with_negative_index() {
    let v = run(r#"
        const a = [1, 2, 3];
        const b = a.with(-1, 42);
        return b[2];
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn array_find_last() {
    let v = run(r#"return [1, 2, 3, 4].findLast(x => x < 4);"#);
    assert_eq!(as_num(v), 3.0);
}

#[test]
fn array_find_last_index() {
    let v = run(r#"return [10, 20, 30, 20].findLastIndex(x => x === 20);"#);
    assert_eq!(as_num(v), 3.0);
}

#[test]
fn array_copy_within() {
    let v = run(r#"
        const a = [1, 2, 3, 4, 5];
        a.copyWithin(0, 3);
        return a.join(",");
    "#);
    assert_eq!(as_str(v), "4,5,3,4,5");
}

// ─── ES2025 Set operace ─────────────────────────────────────────

#[test]
fn set_union() {
    let v = run(r#"
        const a = new Set([1, 2, 3]);
        const b = new Set([3, 4, 5]);
        return a.union(b).size;
    "#);
    assert_eq!(as_num(v), 5.0);
}

#[test]
fn set_intersection() {
    let v = run(r#"
        const a = new Set([1, 2, 3]);
        const b = new Set([2, 3, 4]);
        return a.intersection(b).size;
    "#);
    assert_eq!(as_num(v), 2.0);
}

#[test]
fn set_difference() {
    let v = run(r#"
        const a = new Set([1, 2, 3]);
        const b = new Set([2, 3]);
        const d = a.difference(b);
        return d.size + ":" + d.has(1);
    "#);
    assert_eq!(as_str(v), "1:true");
}

#[test]
fn set_symmetric_difference() {
    let v = run(r#"
        const a = new Set([1, 2, 3]);
        const b = new Set([2, 3, 4]);
        return a.symmetricDifference(b).size;
    "#);
    assert_eq!(as_num(v), 2.0);
}

#[test]
fn set_is_subset_of() {
    assert_eq!(as_bool(run(r#"
        return new Set([1, 2]).isSubsetOf(new Set([1, 2, 3]));
    "#)), true);
    assert_eq!(as_bool(run(r#"
        return new Set([1, 4]).isSubsetOf(new Set([1, 2, 3]));
    "#)), false);
}

#[test]
fn set_is_superset_of() {
    assert_eq!(as_bool(run(r#"
        return new Set([1, 2, 3]).isSupersetOf(new Set([1, 2]));
    "#)), true);
}

#[test]
fn set_is_disjoint_from() {
    assert_eq!(as_bool(run(r#"
        return new Set([1, 2]).isDisjointFrom(new Set([3, 4]));
    "#)), true);
    assert_eq!(as_bool(run(r#"
        return new Set([1, 2]).isDisjointFrom(new Set([2, 3]));
    "#)), false);
}

// ─── Object.groupBy / Map.groupBy (ES2024) ──────────────────────

#[test]
fn object_group_by() {
    let v = run(r#"
        const arr = [1, 2, 3, 4, 5, 6];
        const groups = Object.groupBy(arr, x => x % 2 === 0 ? "even" : "odd");
        return groups.even.length + ":" + groups.odd.length;
    "#);
    assert_eq!(as_str(v), "3:3");
}

#[test]
fn map_group_by() {
    let v = run(r#"
        const arr = ["a", "bb", "ccc", "dd"];
        const groups = Map.groupBy(arr, s => s.length);
        return groups.size;
    "#);
    assert_eq!(as_num(v), 3.0);
}

// ─── Promise.withResolvers / Promise.try ────────────────────────

#[test]
fn promise_with_resolvers_resolve() {
    let v = run(r#"
        const { promise, resolve } = Promise.withResolvers();
        resolve(42);
        let result = 0;
        promise.then(x => { result = x; });
        return result;
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn promise_with_resolvers_reject() {
    let v = run(r#"
        const { promise, reject } = Promise.withResolvers();
        reject("nope");
        let err = null;
        promise.catch(e => { err = e; });
        return err;
    "#);
    assert_eq!(as_str(v), "nope");
}

#[test]
fn promise_try_sync_value() {
    let v = run(r#"
        let result = 0;
        Promise.try(() => 42).then(x => { result = x; });
        return result;
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn promise_try_throws() {
    let v = run(r#"
        let err = null;
        Promise.try(() => { throw new Error("bad"); }).catch(e => { err = e.message; });
        return err;
    "#);
    assert_eq!(as_str(v), "bad");
}

// ─── Error.cause (ES2022) ───────────────────────────────────────

#[test]
fn error_cause() {
    let v = run(r#"
        const original = new Error("low-level");
        const wrapped  = new Error("high-level", { cause: original });
        return wrapped.cause.message;
    "#);
    assert_eq!(as_str(v), "low-level");
}

#[test]
fn error_cause_optional() {
    let v = run(r#"
        const e = new Error("plain");
        return e.cause === undefined;
    "#);
    assert_eq!(as_bool(v), true);
}

// ─── Iterator helpers (ES2025) ─────────────────────────────────────────

#[test]
fn iterator_to_array() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; }
        return gen().toArray();
    "#;
    if let JsValue::Array(a) = run(code) {
        let arr = a.borrow();
        assert_eq!(arr.len(), 3);
    } else {
        panic!("expected Array");
    }
}

#[test]
fn iterator_map() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; }
        return gen().map(x => x * 2).toArray();
    "#;
    if let JsValue::Array(a) = run(code) {
        let arr = a.borrow();
        assert_eq!(arr.len(), 3);
        if let JsValue::Number(n) = &arr[0] { assert_eq!(*n, 2.0); }
        if let JsValue::Number(n) = &arr[2] { assert_eq!(*n, 6.0); }
    }
}

#[test]
fn iterator_filter() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; yield 4; }
        return gen().filter(x => x % 2 === 0).toArray();
    "#;
    if let JsValue::Array(a) = run(code) {
        let arr = a.borrow();
        assert_eq!(arr.len(), 2);
    }
}

#[test]
fn iterator_take() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; yield 4; yield 5; }
        return gen().take(3).toArray();
    "#;
    if let JsValue::Array(a) = run(code) {
        assert_eq!(a.borrow().len(), 3);
    }
}

#[test]
fn iterator_drop() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; yield 4; yield 5; }
        return gen().drop(2).toArray();
    "#;
    if let JsValue::Array(a) = run(code) {
        let arr = a.borrow();
        assert_eq!(arr.len(), 3);
        if let JsValue::Number(n) = &arr[0] { assert_eq!(*n, 3.0); }
    }
}

#[test]
fn iterator_reduce_with_init() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; }
        return gen().reduce((acc, x) => acc + x, 10);
    "#;
    if let JsValue::Number(n) = run(code) {
        assert_eq!(n, 16.0);
    }
}

#[test]
fn iterator_reduce_no_init() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; yield 4; }
        return gen().reduce((acc, x) => acc * x);
    "#;
    if let JsValue::Number(n) = run(code) {
        assert_eq!(n, 24.0);
    }
}

#[test]
fn iterator_some_true() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; }
        return gen().some(x => x === 2);
    "#;
    if let JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn iterator_every_true() {
    let code = r#"
        function* gen() { yield 2; yield 4; yield 6; }
        return gen().every(x => x % 2 === 0);
    "#;
    if let JsValue::Bool(b) = run(code) {
        assert!(b);
    }
}

#[test]
fn iterator_find() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; }
        return gen().find(x => x > 1);
    "#;
    if let JsValue::Number(n) = run(code) {
        assert_eq!(n, 2.0);
    }
}

#[test]
fn iterator_flat_map() {
    let code = r#"
        function* gen() { yield 1; yield 2; yield 3; }
        return gen().flatMap(x => [x, x * 10]).toArray();
    "#;
    if let JsValue::Array(a) = run(code) {
        assert_eq!(a.borrow().len(), 6);
    }
}

#[test]
fn iterator_for_each_side_effect() {
    let code = r#"
        let sum = 0;
        function* gen() { yield 1; yield 2; yield 3; }
        gen().forEach(x => sum += x);
        return sum;
    "#;
    if let JsValue::Number(n) = run(code) {
        assert_eq!(n, 6.0);
    }
}
