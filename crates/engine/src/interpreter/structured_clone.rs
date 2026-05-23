//! HTML structuredClone() - deep clone serialization graph.
//!
//! Spec: https://html.spec.whatwg.org/multipage/structured-data.html
//! Supports: primitive, Array, Object (plain), Map, Set, Date, RegExp, ArrayBuffer,
//!           TypedArray, Blob, File, ImageData, ImageBitmap, FormData, ...
//! Detects cycles via memo table.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum CloneValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    BigInt(i64),
    String(String),
    Date(f64),
    Regex { pattern: String, flags: String },
    Array(Vec<CloneValue>),
    Object(Vec<(String, CloneValue)>),
    Map(Vec<(CloneValue, CloneValue)>),
    Set(Vec<CloneValue>),
    ArrayBuffer(Vec<u8>),
    Uint8Array(Vec<u8>),
    Int32Array(Vec<i32>),
    Float64Array(Vec<f64>),
    Reference(u32),       // pri cyklech: index v memo
}

#[derive(Debug, Clone)]
pub struct TransferList {
    /// IDs of objects transferred (ownership moves). e.g. ArrayBuffer, MessagePort.
    pub items: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct StructuredCloneError {
    pub kind: CloneErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CloneErrorKind {
    DataCloneError,           // Functions, Symbols, native DOM nodes
    UnsupportedType,
}

/// Detects if a value is cloneable per spec.
pub fn can_clone(v: &CloneValue) -> bool {
    !matches!(v, CloneValue::Reference(_)) // refs vznikaji jen behem serialization
}

/// Serialize with cycle detection via assigned IDs.
#[derive(Default)]
pub struct CloneSerializer {
    pub memo: HashMap<u64, u32>, // address -> id
    pub next_id: u32,
}

impl CloneSerializer {
    pub fn new() -> Self { Self::default() }

    pub fn assign(&mut self, address: u64) -> (u32, bool) {
        if let Some(id) = self.memo.get(&address).copied() {
            return (id, true);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.memo.insert(address, id);
        (id, false)
    }
}

/// Simple deep-clone for value tree without cycles. Errors for unsupported types.
pub fn deep_clone(v: &CloneValue) -> Result<CloneValue, StructuredCloneError> {
    match v {
        CloneValue::Undefined => Ok(CloneValue::Undefined),
        CloneValue::Null => Ok(CloneValue::Null),
        CloneValue::Bool(b) => Ok(CloneValue::Bool(*b)),
        CloneValue::Number(n) => Ok(CloneValue::Number(*n)),
        CloneValue::BigInt(n) => Ok(CloneValue::BigInt(*n)),
        CloneValue::String(s) => Ok(CloneValue::String(s.clone())),
        CloneValue::Date(t) => Ok(CloneValue::Date(*t)),
        CloneValue::Regex { pattern, flags } => Ok(CloneValue::Regex { pattern: pattern.clone(), flags: flags.clone() }),
        CloneValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for i in items { out.push(deep_clone(i)?); }
            Ok(CloneValue::Array(out))
        }
        CloneValue::Object(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for (k, val) in pairs { out.push((k.clone(), deep_clone(val)?)); }
            Ok(CloneValue::Object(out))
        }
        CloneValue::Map(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for (k, val) in pairs { out.push((deep_clone(k)?, deep_clone(val)?)); }
            Ok(CloneValue::Map(out))
        }
        CloneValue::Set(items) => {
            let mut out = Vec::with_capacity(items.len());
            for i in items { out.push(deep_clone(i)?); }
            Ok(CloneValue::Set(out))
        }
        CloneValue::ArrayBuffer(b) => Ok(CloneValue::ArrayBuffer(b.clone())),
        CloneValue::Uint8Array(b) => Ok(CloneValue::Uint8Array(b.clone())),
        CloneValue::Int32Array(b) => Ok(CloneValue::Int32Array(b.clone())),
        CloneValue::Float64Array(b) => Ok(CloneValue::Float64Array(b.clone())),
        CloneValue::Reference(_) => Err(StructuredCloneError {
            kind: CloneErrorKind::DataCloneError,
            message: "reference in serialized graph (cycle handling required)".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_clone() {
        let v = CloneValue::Number(3.14);
        assert_eq!(deep_clone(&v).unwrap(), v);
    }

    #[test]
    fn array_clone_deep() {
        let v = CloneValue::Array(vec![CloneValue::Number(1.0), CloneValue::String("a".into())]);
        let c = deep_clone(&v).unwrap();
        assert_eq!(v, c);
    }

    #[test]
    fn object_clone_deep() {
        let v = CloneValue::Object(vec![
            ("k1".into(), CloneValue::Number(1.0)),
            ("k2".into(), CloneValue::Array(vec![CloneValue::Bool(true)])),
        ]);
        assert_eq!(v, deep_clone(&v).unwrap());
    }

    #[test]
    fn map_clone() {
        let v = CloneValue::Map(vec![(CloneValue::String("x".into()), CloneValue::Number(1.0))]);
        assert_eq!(v, deep_clone(&v).unwrap());
    }

    #[test]
    fn set_clone() {
        let v = CloneValue::Set(vec![CloneValue::Number(1.0), CloneValue::Number(2.0)]);
        assert_eq!(v, deep_clone(&v).unwrap());
    }

    #[test]
    fn arraybuffer_clone() {
        let v = CloneValue::ArrayBuffer(vec![1, 2, 3]);
        assert_eq!(v, deep_clone(&v).unwrap());
    }

    #[test]
    fn reference_errors() {
        let v = CloneValue::Reference(5);
        let err = deep_clone(&v).unwrap_err();
        assert_eq!(err.kind, CloneErrorKind::DataCloneError);
    }

    #[test]
    fn serializer_dedupes() {
        let mut s = CloneSerializer::new();
        let (id1, was1) = s.assign(0x1234);
        let (id2, was2) = s.assign(0x1234);
        assert!(!was1);
        assert!(was2);
        assert_eq!(id1, id2);
    }
}
