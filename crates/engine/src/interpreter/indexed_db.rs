//! IndexedDB foundation - per-origin database + object stores + cursors.
//!
//! Spec: https://www.w3.org/TR/IndexedDB/
//!
//! Foundation: in-memory store. Persistent disk = next session (sled / sqlite).
//! Inspired by Chromium `content/browser/indexed_db/`.

use std::collections::{HashMap, BTreeMap};

#[derive(Debug, Clone)]
pub struct IdbValue {
    pub data: Vec<u8>,   // serialized JS value (structured clone)
}

#[derive(Debug, Default)]
pub struct ObjectStore {
    pub name: String,
    pub key_path: Option<String>,
    pub auto_increment: bool,
    pub next_key: i64,
    /// Records: key (numeric or string serialized) -> value.
    pub records: BTreeMap<String, IdbValue>,
    pub indexes: HashMap<String, IdbIndex>,
}

#[derive(Debug, Default)]
pub struct IdbIndex {
    pub name: String,
    pub key_path: String,
    pub unique: bool,
    /// index_key -> primary_key
    pub entries: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Default)]
pub struct IdbDatabase {
    pub name: String,
    pub version: u32,
    pub stores: HashMap<String, ObjectStore>,
}

#[derive(Default)]
pub struct IndexedDbRegistry {
    /// origin -> db_name -> Database
    pub by_origin: HashMap<String, HashMap<String, IdbDatabase>>,
}

impl IndexedDbRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn open(&mut self, origin: &str, name: &str, version: u32) -> &mut IdbDatabase {
        let dbs = self.by_origin.entry(origin.into()).or_default();
        let db = dbs.entry(name.into()).or_insert_with(|| IdbDatabase {
            name: name.into(),
            version: 0,
            stores: HashMap::new(),
        });
        if db.version < version { db.version = version; }
        db
    }

    pub fn delete(&mut self, origin: &str, name: &str) -> bool {
        if let Some(dbs) = self.by_origin.get_mut(origin) {
            return dbs.remove(name).is_some();
        }
        false
    }
}

impl IdbDatabase {
    pub fn create_object_store(&mut self, name: &str, key_path: Option<&str>, auto_increment: bool) -> &mut ObjectStore {
        let store = ObjectStore {
            name: name.into(),
            key_path: key_path.map(String::from),
            auto_increment,
            next_key: 1,
            records: BTreeMap::new(),
            indexes: HashMap::new(),
        };
        self.stores.insert(name.into(), store);
        self.stores.get_mut(name).unwrap()
    }

    pub fn delete_object_store(&mut self, name: &str) -> bool {
        self.stores.remove(name).is_some()
    }
}

impl ObjectStore {
    /// Put record. Vraci key (auto-generated nebo provided).
    pub fn put(&mut self, key: Option<&str>, value: IdbValue) -> String {
        let key = match key {
            Some(k) => k.to_string(),
            None if self.auto_increment => {
                let k = self.next_key.to_string();
                self.next_key += 1;
                k
            }
            None => return String::new(),
        };
        self.records.insert(key.clone(), value);
        key
    }

    pub fn get(&self, key: &str) -> Option<&IdbValue> {
        self.records.get(key)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.records.remove(key).is_some()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn count(&self) -> usize {
        self.records.len()
    }

    /// Iterate cursor pres records (range query).
    pub fn range<'a>(&'a self, lower: Option<&str>, upper: Option<&str>) -> Vec<(&'a String, &'a IdbValue)> {
        let mut out = Vec::new();
        for (k, v) in self.records.iter() {
            if let Some(lo) = lower { if k.as_str() < lo { continue; } }
            if let Some(hi) = upper { if k.as_str() > hi { continue; } }
            out.push((k, v));
        }
        out
    }

    pub fn create_index(&mut self, name: &str, key_path: &str, unique: bool) {
        self.indexes.insert(name.into(), IdbIndex {
            name: name.into(),
            key_path: key_path.into(),
            unique,
            entries: BTreeMap::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(s: &str) -> IdbValue { IdbValue { data: s.as_bytes().to_vec() } }

    #[test]
    fn open_creates_db() {
        let mut r = IndexedDbRegistry::new();
        let db = r.open("https://x.com", "appdb", 1);
        assert_eq!(db.version, 1);
    }

    #[test]
    fn version_upgrade() {
        let mut r = IndexedDbRegistry::new();
        r.open("https://x.com", "db", 1);
        let db = r.open("https://x.com", "db", 3);
        assert_eq!(db.version, 3);
    }

    #[test]
    fn store_put_get_delete() {
        let mut r = IndexedDbRegistry::new();
        let db = r.open("https://x.com", "db", 1);
        let store = db.create_object_store("items", None, false);
        store.put(Some("k1"), val("v1"));
        assert_eq!(store.get("k1").unwrap().data, b"v1");
        store.delete("k1");
        assert!(store.get("k1").is_none());
    }

    #[test]
    fn auto_increment_key() {
        let mut r = IndexedDbRegistry::new();
        let db = r.open("https://x.com", "db", 1);
        let store = db.create_object_store("items", None, true);
        let k1 = store.put(None, val("a"));
        let k2 = store.put(None, val("b"));
        assert_ne!(k1, k2);
    }

    #[test]
    fn range_query() {
        let mut r = IndexedDbRegistry::new();
        let db = r.open("https://x.com", "db", 1);
        let store = db.create_object_store("items", None, false);
        store.put(Some("a"), val("1"));
        store.put(Some("b"), val("2"));
        store.put(Some("c"), val("3"));
        let r = store.range(Some("a"), Some("b"));
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn origin_isolation() {
        let mut r = IndexedDbRegistry::new();
        r.open("https://a.com", "db", 1);
        r.open("https://b.com", "db", 1);
        assert_eq!(r.by_origin.len(), 2);
    }
}
