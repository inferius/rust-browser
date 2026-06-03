//! File System Access API - showOpenFilePicker, showSaveFilePicker, showDirectoryPicker.
//!
//! Spec: https://wicg.github.io/file-system-access/
//! Foundation: handle registry + permission. Real native file dialogs = OS-specific.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileSystemKind {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct FileSystemHandle {
    pub id: u64,
    pub name: String,
    pub kind: FileSystemKind,
    pub path: PathBuf,
    pub readable: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionMode {
    Read,
    ReadWrite,
}

#[derive(Default)]
pub struct FileSystemRegistry {
    pub handles: HashMap<u64, FileSystemHandle>,
    pub next_id: u64,
}

impl FileSystemRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, name: &str, path: PathBuf, kind: FileSystemKind) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.handles.insert(id, FileSystemHandle {
            id,
            name: name.into(),
            kind,
            path,
            readable: true,
            writable: false,
        });
        id
    }

    pub fn request_permission(&mut self, id: u64, mode: PermissionMode) -> bool {
        let h = match self.handles.get_mut(&id) { Some(h) => h, None => return false };
        h.readable = true;
        h.writable = matches!(mode, PermissionMode::ReadWrite);
        true
    }

    pub fn get(&self, id: u64) -> Option<&FileSystemHandle> {
        self.handles.get(&id)
    }

    pub fn revoke(&mut self, id: u64) {
        if let Some(h) = self.handles.get_mut(&id) {
            h.readable = false;
            h.writable = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_file_handle() {
        let mut r = FileSystemRegistry::new();
        let id = r.register("doc.txt", PathBuf::from("/tmp/doc.txt"), FileSystemKind::File);
        assert!(id > 0);
        assert_eq!(r.get(id).unwrap().kind, FileSystemKind::File);
    }

    #[test]
    fn write_permission_granted() {
        let mut r = FileSystemRegistry::new();
        let id = r.register("x", PathBuf::from("/tmp/x"), FileSystemKind::File);
        assert!(!r.get(id).unwrap().writable);
        r.request_permission(id, PermissionMode::ReadWrite);
        assert!(r.get(id).unwrap().writable);
    }

    #[test]
    fn revoke_clears() {
        let mut r = FileSystemRegistry::new();
        let id = r.register("d", PathBuf::from("/tmp/d"), FileSystemKind::Directory);
        r.request_permission(id, PermissionMode::ReadWrite);
        r.revoke(id);
        assert!(!r.get(id).unwrap().readable);
        assert!(!r.get(id).unwrap().writable);
    }
}
