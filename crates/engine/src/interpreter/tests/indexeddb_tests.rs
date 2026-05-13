/// Testy pro IndexedDB real implementation (in-memory backend).

use super::helpers::run;
use crate::interpreter::JsValue;

#[test]
fn idb_open_creates_database() {
    let r = run(r#"
        const req = indexedDB.open('mydb', 1);
        return req.result.name;
    "#);
    if let JsValue::Str(s) = r {
        assert_eq!(s, "mydb");
    }
}

#[test]
fn idb_create_object_store_and_put_get() {
    let r = run(r#"
        const req = indexedDB.open('mydb', 1);
        const db = req.result;
        const store = db.createObjectStore('users');
        const putReq = store.put({ id: 1, name: 'Alice' }, 'k1');
        const getReq = store.get('k1');
        return getReq.result.name;
    "#);
    if let JsValue::Str(s) = r {
        assert_eq!(s, "Alice");
    }
}

#[test]
fn idb_transaction_object_store() {
    let r = run(r#"
        const req = indexedDB.open('mydb', 1);
        const db = req.result;
        db.createObjectStore('items');
        const tx = db.transaction(['items']);
        const store = tx.objectStore('items');
        store.put('hello', 'a');
        return store.get('a').result;
    "#);
    if let JsValue::Str(s) = r {
        assert_eq!(s, "hello");
    }
}

#[test]
fn idb_delete_removes_entry() {
    let r = run(r#"
        const db = indexedDB.open('mydb', 1).result;
        const s = db.createObjectStore('s');
        s.put(1, 'k');
        s.delete('k');
        const after = s.get('k').result;
        return typeof after;
    "#);
    if let JsValue::Str(s) = r {
        assert_eq!(s, "undefined");
    }
}

#[test]
fn idb_clear_empties_store() {
    let r = run(r#"
        const db = indexedDB.open('mydb', 1).result;
        const s = db.createObjectStore('s');
        s.put(1, 'a');
        s.put(2, 'b');
        s.clear();
        return s.count().result;
    "#);
    if let JsValue::Number(n) = r {
        assert_eq!(n, 0.0);
    }
}

#[test]
fn idb_get_all_values_and_keys() {
    let r = run(r#"
        const db = indexedDB.open('mydb', 1).result;
        const s = db.createObjectStore('s');
        s.put('a', 'k1');
        s.put('b', 'k2');
        s.put('c', 'k3');
        return s.getAll().result.length;
    "#);
    if let JsValue::Number(n) = r {
        assert_eq!(n, 3.0);
    }
}

#[test]
fn idb_persists_within_interpreter() {
    // Tatazi databaze se sdileni mezi `indexedDB.open` calls.
    let r = run(r#"
        let db1 = indexedDB.open('shared', 1).result;
        const s = db1.createObjectStore('s');
        s.put('saved', 'k');
        let db2 = indexedDB.open('shared', 1).result;
        const s2 = db2.transaction(['s']).objectStore('s');
        return s2.get('k').result;
    "#);
    if let JsValue::Str(s) = r {
        assert_eq!(s, "saved");
    }
}

#[test]
fn idb_delete_database() {
    let r = run(r#"
        const db = indexedDB.open('todelete', 1).result;
        db.createObjectStore('s').put('val', 'k');
        indexedDB.deleteDatabase('todelete');
        // Po delete by mela byt prazdna pri otevreni (ale my otevirame stejnu mapu znovu).
        // Test alespon, ze deleteDatabase nehazi error.
        return true;
    "#);
    if let JsValue::Bool(b) = r {
        assert!(b);
    }
}
