/// Batch R - Workers, SharedArrayBuffer, Atomics, Uint8Array.

use super::helpers::*;

// ─── Worker stub ─────────────────────────────────────────────────────────

#[test]
fn worker_construct() {
    let v = run(r#"
        const w = new Worker("worker.js");
        return w.url;
    "#);
    assert_eq!(as_str(v), "worker.js");
}

#[test]
fn worker_post_message() {
    let v = run(r#"
        const w = new Worker("test");
        w.postMessage({type: "hello"});
        return typeof w.postMessage({}); // undefined
    "#);
    assert_eq!(as_str(v), "undefined");
}

// ─── SharedArrayBuffer / ArrayBuffer ─────────────────────────────────────

#[test]
fn shared_array_buffer_byte_length() {
    let v = run(r#"
        const sab = new SharedArrayBuffer(16);
        return sab.byteLength;
    "#);
    assert_eq!(as_num(v), 16.0);
}

#[test]
fn array_buffer_byte_length() {
    let v = run(r#"
        const ab = new ArrayBuffer(8);
        return ab.byteLength;
    "#);
    assert_eq!(as_num(v), 8.0);
}

// ─── Uint8Array ──────────────────────────────────────────────────────────

#[test]
fn uint8_array_from_size() {
    let v = run(r#"
        const arr = new Uint8Array(10);
        return arr.length;
    "#);
    assert_eq!(as_num(v), 10.0);
}

#[test]
fn uint8_array_from_array() {
    let v = run(r#"
        const arr = new Uint8Array([1, 2, 3, 4]);
        return arr.length;
    "#);
    assert_eq!(as_num(v), 4.0);
}

// ─── Atomics ─────────────────────────────────────────────────────────────

#[test]
fn atomics_load_store() {
    let v = run(r#"
        const buf = new Uint8Array(4);
        Atomics.store(buf, 0, 42);
        return Atomics.load(buf, 0);
    "#);
    assert_eq!(as_num(v), 42.0);
}

#[test]
fn atomics_add() {
    let v = run(r#"
        const buf = new Uint8Array(4);
        Atomics.store(buf, 0, 10);
        const old = Atomics.add(buf, 0, 5);
        return old + ":" + Atomics.load(buf, 0);
    "#);
    assert_eq!(as_str(v), "10:15");
}

#[test]
fn atomics_compare_exchange() {
    let v = run(r#"
        const buf = new Uint8Array(4);
        Atomics.store(buf, 0, 100);
        const old = Atomics.compareExchange(buf, 0, 100, 200);
        return old + ":" + Atomics.load(buf, 0);
    "#);
    assert_eq!(as_str(v), "100:200");
}

#[test]
fn atomics_compare_exchange_no_match() {
    let v = run(r#"
        const buf = new Uint8Array(4);
        Atomics.store(buf, 0, 100);
        // expected != current, takze nezmeni
        const old = Atomics.compareExchange(buf, 0, 999, 200);
        return old + ":" + Atomics.load(buf, 0);
    "#);
    assert_eq!(as_str(v), "100:100");
}
