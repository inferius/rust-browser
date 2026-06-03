//! FormData - multipart-style key/value with file support.
//!
//! Spec: https://xhr.spec.whatwg.org/#interface-formdata

#[derive(Debug, Clone)]
pub enum FormDataValue {
    Text(String),
    Blob {
        blob_id: u64,
        filename: Option<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct FormData {
    pub entries: Vec<(String, FormDataValue)>,
}

impl FormData {
    pub fn new() -> Self { Self::default() }

    pub fn append_text(&mut self, name: &str, value: &str) {
        self.entries.push((name.into(), FormDataValue::Text(value.into())));
    }

    pub fn append_blob(&mut self, name: &str, blob_id: u64, filename: Option<&str>) {
        self.entries.push((name.into(), FormDataValue::Blob {
            blob_id, filename: filename.map(|s| s.into()),
        }));
    }

    pub fn set(&mut self, name: &str, value: FormDataValue) {
        let mut found = false;
        self.entries.retain(|(k, _)| {
            if k == name {
                if found { false } else { found = true; true }
            } else { true }
        });
        if found {
            for (k, v) in self.entries.iter_mut() {
                if k == name { *v = value.clone(); break; }
            }
        } else {
            self.entries.push((name.into(), value));
        }
    }

    pub fn delete(&mut self, name: &str) {
        self.entries.retain(|(k, _)| k != name);
    }

    pub fn get(&self, name: &str) -> Option<&FormDataValue> {
        self.entries.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    pub fn get_all(&self, name: &str) -> Vec<&FormDataValue> {
        self.entries.iter().filter(|(k, _)| k == name).map(|(_, v)| v).collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_text() {
        let mut f = FormData::new();
        f.append_text("name", "Alice");
        if let Some(FormDataValue::Text(v)) = f.get("name") {
            assert_eq!(v, "Alice");
        } else { panic!("expected text"); }
    }

    #[test]
    fn append_multiple() {
        let mut f = FormData::new();
        f.append_text("tag", "a");
        f.append_text("tag", "b");
        assert_eq!(f.get_all("tag").len(), 2);
    }

    #[test]
    fn blob_entry() {
        let mut f = FormData::new();
        f.append_blob("upload", 99, Some("file.txt"));
        if let Some(FormDataValue::Blob { blob_id, filename }) = f.get("upload") {
            assert_eq!(*blob_id, 99);
            assert_eq!(filename.as_deref(), Some("file.txt"));
        } else { panic!("expected blob"); }
    }

    #[test]
    fn set_replaces_all() {
        let mut f = FormData::new();
        f.append_text("k", "a");
        f.append_text("k", "b");
        f.set("k", FormDataValue::Text("z".into()));
        assert_eq!(f.get_all("k").len(), 1);
    }

    #[test]
    fn delete_removes() {
        let mut f = FormData::new();
        f.append_text("k", "v");
        f.delete("k");
        assert!(!f.has("k"));
    }
}
