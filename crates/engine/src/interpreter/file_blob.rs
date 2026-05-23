//! Blob / File / FileReader - binary data primitives.
//!
//! Spec: https://w3c.github.io/FileAPI/
//! Blob.slice(start, end, contentType) -> new Blob view.
//! FileReader.readAsText/ArrayBuffer/DataURL/BinaryString.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Blob {
    pub id: u64,
    pub bytes: Vec<u8>,             // could be Arc<[u8]> in real impl
    pub content_type: String,
    pub source_file_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub id: u64,
    pub name: String,
    pub last_modified_unix_ms: u64,
    pub webkit_relative_path: String,
}

#[derive(Default)]
pub struct BlobRegistry {
    pub blobs: HashMap<u64, Blob>,
    pub files: HashMap<u64, FileMetadata>,
    /// blob: URL -> blob id mapping.
    pub blob_urls: HashMap<String, u64>,
    pub next_id: u64,
}

impl BlobRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create_blob(&mut self, bytes: Vec<u8>, content_type: &str) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.blobs.insert(id, Blob {
            id, bytes, content_type: content_type.into(),
            source_file_id: None,
        });
        id
    }

    pub fn create_file(&mut self, bytes: Vec<u8>, name: &str, content_type: &str, last_mod: u64) -> u64 {
        let blob_id = self.create_blob(bytes, content_type);
        self.blobs.get_mut(&blob_id).unwrap().source_file_id = Some(blob_id);
        self.files.insert(blob_id, FileMetadata {
            id: blob_id,
            name: name.into(),
            last_modified_unix_ms: last_mod,
            webkit_relative_path: String::new(),
        });
        blob_id
    }

    pub fn slice(&mut self, blob_id: u64, start: usize, end: Option<usize>, content_type: Option<&str>) -> Option<u64> {
        let src = self.blobs.get(&blob_id)?.clone();
        let end = end.unwrap_or(src.bytes.len()).min(src.bytes.len());
        let start = start.min(end);
        let ct = content_type.unwrap_or(&src.content_type).to_string();
        let bytes = src.bytes[start..end].to_vec();
        Some(self.create_blob(bytes, &ct))
    }

    pub fn create_object_url(&mut self, blob_id: u64) -> String {
        let url = format!("blob:rwe-engine/{:x}", blob_id);
        self.blob_urls.insert(url.clone(), blob_id);
        url
    }

    pub fn revoke_object_url(&mut self, url: &str) -> bool {
        self.blob_urls.remove(url).is_some()
    }

    pub fn resolve_url(&self, url: &str) -> Option<&Blob> {
        let id = self.blob_urls.get(url)?;
        self.blobs.get(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileReaderState {
    Empty,
    Loading,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileReaderResultKind {
    Text,
    ArrayBuffer,
    DataUrl,
    BinaryString,
}

#[derive(Debug, Clone)]
pub struct FileReader {
    pub state: FileReaderState,
    pub result_text: Option<String>,
    pub result_bytes: Option<Vec<u8>>,
    pub error: Option<String>,
}

impl FileReader {
    pub fn new() -> Self {
        Self { state: FileReaderState::Empty, result_text: None, result_bytes: None, error: None }
    }

    pub fn read(&mut self, blob: &Blob, kind: FileReaderResultKind) {
        self.state = FileReaderState::Loading;
        match kind {
            FileReaderResultKind::ArrayBuffer => {
                self.result_bytes = Some(blob.bytes.clone());
            }
            FileReaderResultKind::Text => {
                self.result_text = Some(String::from_utf8_lossy(&blob.bytes).to_string());
            }
            FileReaderResultKind::BinaryString => {
                self.result_text = Some(blob.bytes.iter().map(|b| *b as char).collect());
            }
            FileReaderResultKind::DataUrl => {
                let b64 = base64_encode(&blob.bytes);
                self.result_text = Some(format!("data:{};base64,{}", blob.content_type, b64));
            }
        }
        self.state = FileReaderState::Done;
    }
}

impl Default for FileReader { fn default() -> Self { Self::new() } }

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(BASE64[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 { out.push(BASE64[((n >> 6) & 0x3f) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(BASE64[(n & 0x3f) as usize] as char); } else { out.push('='); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_blob_returns_id() {
        let mut r = BlobRegistry::new();
        let id = r.create_blob(b"hello".to_vec(), "text/plain");
        assert!(r.blobs.contains_key(&id));
    }

    #[test]
    fn slice_creates_subset() {
        let mut r = BlobRegistry::new();
        let id = r.create_blob(b"helloworld".to_vec(), "text/plain");
        let sliced = r.slice(id, 5, Some(10), None).unwrap();
        assert_eq!(r.blobs[&sliced].bytes, b"world");
    }

    #[test]
    fn object_url_roundtrip() {
        let mut r = BlobRegistry::new();
        let id = r.create_blob(b"x".to_vec(), "text/plain");
        let url = r.create_object_url(id);
        assert!(r.resolve_url(&url).is_some());
        r.revoke_object_url(&url);
        assert!(r.resolve_url(&url).is_none());
    }

    #[test]
    fn file_reader_text() {
        let mut r = BlobRegistry::new();
        let id = r.create_blob(b"hi".to_vec(), "text/plain");
        let mut fr = FileReader::new();
        fr.read(r.blobs.get(&id).unwrap(), FileReaderResultKind::Text);
        assert_eq!(fr.result_text.as_deref(), Some("hi"));
    }

    #[test]
    fn file_reader_data_url() {
        let mut r = BlobRegistry::new();
        let id = r.create_blob(b"hi".to_vec(), "text/plain");
        let mut fr = FileReader::new();
        fr.read(r.blobs.get(&id).unwrap(), FileReaderResultKind::DataUrl);
        assert!(fr.result_text.unwrap().starts_with("data:text/plain;base64,"));
    }

    #[test]
    fn create_file_has_metadata() {
        let mut r = BlobRegistry::new();
        let id = r.create_file(b"data".to_vec(), "x.txt", "text/plain", 1000);
        assert_eq!(r.files[&id].name, "x.txt");
    }
}
