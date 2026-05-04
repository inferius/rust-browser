/// Batch K - ES2023+ moderni features.
/// Array immutable varianty, findLast, copyWithin, Set ops, groupBy,
/// Promise.withResolvers/try, Error.cause.

use super::helpers::*;

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
