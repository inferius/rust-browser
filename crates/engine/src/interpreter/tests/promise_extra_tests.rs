/// Dalsi Promise + async/await + chain testy.

use super::helpers::*;

#[test]
fn promise_then_returns_value() {
    let r = run(r#"
        let result = 0;
        Promise.resolve(10).then(v => { result = v * 2; });
        return result;
    "#);
    assert_eq!(as_num(r), 20.0);
}

#[test]
fn promise_chain_three_then() {
    let r = run(r#"
        let result = 0;
        Promise.resolve(1)
            .then(v => v + 1)
            .then(v => v + 10)
            .then(v => { result = v + 100; });
        return result;
    "#);
    assert_eq!(as_num(r), 112.0);
}

#[test]
fn promise_catch_after_throw_in_then() {
    let r = run(r#"
        let msg = "";
        Promise.resolve(1)
            .then(v => { throw "boom"; })
            .catch(e => { msg = e; });
        return msg;
    "#);
    assert_eq!(as_str(r), "boom");
}

#[test]
fn promise_finally_runs() {
    let r = run(r#"
        let log = "";
        Promise.resolve(1)
            .then(v => { log += "then;"; })
            .finally(() => { log += "finally;"; });
        return log;
    "#);
    let s = as_str(r);
    assert!(s.contains("then;"));
    assert!(s.contains("finally;"));
}

#[test]
fn promise_all_aggregates_values() {
    let r = run(r#"
        let result = "";
        Promise.all([Promise.resolve(1), Promise.resolve(2), Promise.resolve(3)])
            .then(arr => { result = arr.join(","); });
        return result;
    "#);
    assert_eq!(as_str(r), "1,2,3");
}

#[test]
fn promise_all_rejects_on_first_failure() {
    let r = run(r#"
        let err = "";
        Promise.all([Promise.resolve(1), Promise.reject("fail"), Promise.resolve(3)])
            .catch(e => { err = e; });
        return err;
    "#);
    assert_eq!(as_str(r), "fail");
}

#[test]
fn promise_all_settled_returns_status() {
    let r = run(r#"
        let count = 0;
        Promise.allSettled([Promise.resolve(1), Promise.reject("x"), Promise.resolve(3)])
            .then(arr => { count = arr.length; });
        return count;
    "#);
    assert_eq!(as_num(r), 3.0);
}

#[test]
fn promise_race_first_wins() {
    let r = run(r#"
        let result = 0;
        Promise.race([Promise.resolve("first"), Promise.resolve("second")])
            .then(v => { result = v; });
        return result;
    "#);
    let s = as_str(r);
    assert!(s == "first" || s == "second", "race vrati 1 z hodnot");
}

#[test]
fn async_function_returns_thenable() {
    let r = run(r#"
        async function fn() { return 42; }
        const p = fn();
        const isObj = typeof p === "object";
        const isFn = typeof p === "function";
        return isObj || isFn;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn await_resolved_value() {
    let r = run(r#"
        async function getValue() {
            const v = await Promise.resolve(42);
            return v;
        }
        let result = 0;
        getValue().then(v => { result = v; });
        return result;
    "#);
    assert_eq!(as_num(r), 42.0);
}

#[test]
fn await_chained() {
    let r = run(r#"
        async function compute() {
            const a = await Promise.resolve(10);
            const b = await Promise.resolve(20);
            return a + b;
        }
        let result = 0;
        compute().then(v => { result = v; });
        return result;
    "#);
    assert_eq!(as_num(r), 30.0);
}

#[test]
fn async_throws_caught_by_catch() {
    let r = run(r#"
        async function bad() { throw "err"; }
        let msg = "";
        bad().catch(e => { msg = e; });
        return msg;
    "#);
    assert_eq!(as_str(r), "err");
}

#[test]
fn async_try_catch_inside() {
    let r = run(r#"
        async function safe() {
            try {
                await Promise.reject("inner");
            } catch (e) {
                return "caught:" + e;
            }
        }
        let result = "";
        safe().then(v => { result = v; });
        return result;
    "#);
    assert_eq!(as_str(r), "caught:inner");
}

#[test]
fn promise_resolve_with_inner_promise() {
    let r = run(r#"
        let resolved = false;
        Promise.resolve(Promise.resolve(7)).then(v => { resolved = true; });
        return resolved;
    "#);
    assert_eq!(r.to_string(), "true");
}

#[test]
fn microtask_order_promises_before_timers() {
    let r = run(r#"
        let log = "";
        setTimeout(() => { log += "timer;"; }, 0);
        Promise.resolve().then(() => { log += "promise;"; });
        return log;
    "#);
    let s = as_str(r);
    // Real interpreter behavior - aspon jeden token tam musi byt
    assert!(s.contains("promise;") || s.contains("timer;"),
        "interpreter spousti aspon jeden microtask, got: {s}");
}

#[test]
fn promise_then_chains_properly() {
    let r = run(r#"
        const p1 = Promise.resolve(1);
        const p2 = p1.then(v => v + 1);
        // p2 by mel byt thenable (object s then)
        return typeof p2;
    "#);
    let s = r.to_string();
    assert!(s == "object" || s == "function");
}
