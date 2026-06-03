/// Promise + async/await + then/catch/finally + all/allSettled.

use super::helpers::*;
use crate::interpreter::JsValue;

#[test]
fn promise_resolve_static() {
    assert_eq!(as_num(run(r#"
        const p = Promise.resolve(42);
        let result = 0;
        p.then(v => { result = v; });
        return result;
    "#)), 42.0);
}

#[test]
fn promise_reject_static() {
    assert_eq!(as_str(run(r#"
        const p = Promise.reject("error");
        let msg = "";
        p.catch(r => { msg = r; });
        return msg;
    "#)), "error");
}

#[test]
fn promise_constructor_resolve() {
    assert_eq!(as_num(run(r#"
        const p = new Promise((resolve, reject) => {
            resolve(100);
        });
        let result = 0;
        p.then(v => { result = v; });
        return result;
    "#)), 100.0);
}

#[test]
fn promise_constructor_reject() {
    assert_eq!(as_str(run(r#"
        const p = new Promise((resolve, reject) => {
            reject("fail");
        });
        let msg = "";
        p.catch(r => { msg = r; });
        return msg;
    "#)), "fail");
}

#[test]
fn promise_then_chain() {
    assert_eq!(as_num(run(r#"
        let result = 0;
        Promise.resolve(10)
            .then(v => v * 2)
            .then(v => { result = v; });
        return result;
    "#)), 20.0);
}

#[test]
fn promise_catch_after_then() {
    assert_eq!(as_str(run(r#"
        let caught = "";
        Promise.reject("boom")
            .then(v => v + "!")
            .catch(e => { caught = e; });
        return caught;
    "#)), "boom");
}

#[test]
fn promise_finally_runs() {
    assert_eq!(as_bool(run(r#"
        let ran = false;
        Promise.resolve(1).finally(() => { ran = true; });
        return ran;
    "#)), true);
}

#[test]
fn promise_all_fulfilled() {
    assert_eq!(as_num(run(r#"
        const results = [];
        Promise.all([
            Promise.resolve(1),
            Promise.resolve(2),
            Promise.resolve(3),
        ]).then(arr => {
            results.push(arr[0]);
            results.push(arr[1]);
            results.push(arr[2]);
        });
        return results[0] + results[1] + results[2];
    "#)), 6.0);
}

#[test]
fn promise_all_rejected() {
    assert_eq!(as_str(run(r#"
        let reason = "";
        Promise.all([
            Promise.resolve(1),
            Promise.reject("error"),
            Promise.resolve(3),
        ]).catch(r => { reason = r; });
        return reason;
    "#)), "error");
}

#[test]
fn promise_all_settled() {
    assert_eq!(as_num(run(r#"
        let count = 0;
        Promise.allSettled([
            Promise.resolve(1),
            Promise.reject("x"),
            Promise.resolve(3),
        ]).then(results => { count = results.length; });
        return count;
    "#)), 3.0);
}

#[test]
fn promise_constructor_throw_rejects() {
    assert_eq!(as_str(run(r#"
        let caught = "";
        const p = new Promise((resolve, reject) => {
            throw new Error("executor threw");
        });
        p.catch(e => { caught = e.message; });
        return caught;
    "#)), "executor threw");
}

// ─── async/await ─────────────────────────────────────────────────────────

#[test]
fn async_fn_returns_promise() {
    assert!(matches!(
        run(r#"
            async function f() { return 42; }
            return f();
        "#),
        JsValue::Object(_)
    ));
}

#[test]
fn await_unwraps_promise() {
    assert_eq!(as_num(run(r#"
        async function f() { return 42; }
        const p = f();
        let result = 0;
        p.then(v => { result = v; });
        return result;
    "#)), 42.0);
}

#[test]
fn await_resolved_promise() {
    assert_eq!(as_num(run(r#"
        async function f() {
            const v = await Promise.resolve(99);
            return v;
        }
        let result = 0;
        f().then(v => { result = v; });
        return result;
    "#)), 99.0);
}

#[test]
fn await_chained() {
    assert_eq!(as_num(run(r#"
        async function double(x) {
            return x * 2;
        }
        async function main() {
            const a = await double(5);
            const b = await double(a);
            return b;
        }
        let result = 0;
        main().then(v => { result = v; });
        return result;
    "#)), 20.0);
}

#[test]
fn await_rejected_becomes_catch() {
    assert_eq!(as_str(run(r#"
        async function f() {
            try {
                await Promise.reject("bad");
            } catch (e) {
                return "caught: " + e;
            }
            return "ok";
        }
        let result = "";
        f().then(v => { result = v; });
        return result;
    "#)), "caught: bad");
}

#[test]
fn async_arrow() {
    assert_eq!(as_num(run(r#"
        const add = async (a, b) => a + b;
        let result = 0;
        add(3, 4).then(v => { result = v; });
        return result;
    "#)), 7.0);
}

#[test]
fn async_fn_throw_rejects_promise() {
    assert_eq!(as_str(run(r#"
        async function f() {
            throw new Error("async error");
        }
        let msg = "";
        f().catch(e => { msg = e.message; });
        return msg;
    "#)), "async error");
}

#[test]
fn async_fn_decl_stmt() {
    assert_eq!(as_num(run(r#"
        async function compute(n) {
            const doubled = await Promise.resolve(n * 2);
            return doubled + 1;
        }
        let result = 0;
        compute(10).then(v => { result = v; });
        return result;
    "#)), 21.0);
}
