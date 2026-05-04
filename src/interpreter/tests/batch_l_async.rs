/// Batch L - async generatory, for-await-of, WeakRef, top-level await.

use super::helpers::*;

// ─── Async generators ────────────────────────────────────────────────────

#[test]
fn async_generator_basic() {
    let v = run(r#"
        async function* gen() {
            yield 1;
            yield 2;
            yield 3;
        }
        const it = gen();
        const a = it.next().value;
        const b = it.next().value;
        const c = it.next().value;
        return a + b + c;
    "#);
    assert_eq!(as_num(v), 6.0);
}

#[test]
fn async_generator_for_of() {
    // V sync impl async generator iterace pres for-of
    let v = run(r#"
        async function* range(n) {
            for (let i = 0; i < n; i++) yield i;
        }
        let sum = 0;
        for (const x of range(5)) sum += x;
        return sum;
    "#);
    assert_eq!(as_num(v), 10.0);
}

// ─── for await of ────────────────────────────────────────────────────────

#[test]
fn for_await_of_array() {
    // for await funguje i na bezne pole (kazdy prvek je hned vyresen)
    let v = run(r#"
        async function main() {
            let sum = 0;
            for await (const x of [1, 2, 3, 4]) {
                sum += x;
            }
            return sum;
        }
        let result = 0;
        main().then(v => { result = v; });
        return result;
    "#);
    assert_eq!(as_num(v), 10.0);
}

#[test]
fn for_await_of_unwraps_promises() {
    // for-await-of unwrapuje Promise hodnoty
    let v = run(r#"
        async function main() {
            const arr = [Promise.resolve(10), Promise.resolve(20)];
            let sum = 0;
            for await (const x of arr) sum += x;
            return sum;
        }
        let result = 0;
        main().then(v => { result = v; });
        return result;
    "#);
    assert_eq!(as_num(v), 30.0);
}

// ─── WeakRef ─────────────────────────────────────────────────────────────

#[test]
fn weak_ref_deref() {
    let v = run(r#"
        const obj = { x: 42 };
        const ref = new WeakRef(obj);
        return ref.deref().x;
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn weak_ref_typeof() {
    // WeakRef je objekt
    let v = run(r#"
        const ref = new WeakRef({});
        return typeof ref;
    "#);
    assert_eq!(as_str(v), "object");
}

// ─── FinalizationRegistry ────────────────────────────────────────────────

#[test]
fn finalization_registry_register() {
    // Stub - register/unregister vraci undefined
    let v = run(r#"
        const reg = new FinalizationRegistry(() => {});
        return typeof reg.register({}, "token");
    "#);
    assert_eq!(as_str(v), "undefined");
}

// ─── Top-level await ─────────────────────────────────────────────────────

#[test]
fn top_level_await_resolves() {
    // V naseho sync runtime top-level await funguje (Promise je hned vyreseny)
    let v = run(r#"
        const x = await Promise.resolve(42);
        return x;
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn top_level_await_chain() {
    let v = run(r#"
        const a = await Promise.resolve(10);
        const b = await Promise.resolve(20);
        return a + b;
    "#);
    assert_eq!(as_num(v), 30.0);
}
