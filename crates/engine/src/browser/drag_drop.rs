//! HTML5 Drag & Drop API - DataTransfer + DragEvent state.
//!
//! Spec: https://html.spec.whatwg.org/multipage/dnd.html

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropEffect {
    None,
    Copy,
    Link,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragEffectAllowed {
    None,
    Copy,
    CopyLink,
    CopyMove,
    Link,
    LinkMove,
    Move,
    All,
    Uninitialized,
}

#[derive(Debug, Clone)]
pub struct DragItem {
    pub kind: String,         // "string" | "file"
    pub mime: String,
    pub data: Vec<u8>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DataTransfer {
    pub items: Vec<DragItem>,
    pub effect_allowed: DragEffectAllowed,
    pub drop_effect: DropEffect,
    pub mode: TransferMode,
    pub format_index: HashMap<String, usize>,  // mime -> items index
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferMode {
    ReadWrite,           // during dragstart on source
    ReadOnly,            // during drop on target
    Protected,           // outside event handler
}

impl DataTransfer {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            effect_allowed: DragEffectAllowed::Uninitialized,
            drop_effect: DropEffect::None,
            mode: TransferMode::Protected,
            format_index: HashMap::new(),
        }
    }

    pub fn set_data(&mut self, format: &str, data: &str) -> Result<(), String> {
        if self.mode != TransferMode::ReadWrite {
            return Err("DataTransfer not writable".into());
        }
        let item = DragItem {
            kind: "string".into(),
            mime: format.into(),
            data: data.as_bytes().to_vec(),
            filename: None,
        };
        if let Some(idx) = self.format_index.get(format).copied() {
            self.items[idx] = item;
        } else {
            self.format_index.insert(format.into(), self.items.len());
            self.items.push(item);
        }
        Ok(())
    }

    pub fn get_data(&self, format: &str) -> Option<String> {
        if self.mode == TransferMode::Protected { return None; }
        let idx = self.format_index.get(format).copied()?;
        let item = self.items.get(idx)?;
        Some(String::from_utf8_lossy(&item.data).to_string())
    }

    pub fn clear_data(&mut self, format: Option<&str>) -> Result<(), String> {
        if self.mode != TransferMode::ReadWrite {
            return Err("DataTransfer not writable".into());
        }
        match format {
            Some(f) => {
                if let Some(idx) = self.format_index.remove(f) {
                    self.items.remove(idx);
                    for (_, i) in self.format_index.iter_mut() {
                        if *i > idx { *i -= 1; }
                    }
                }
            }
            None => {
                self.items.clear();
                self.format_index.clear();
            }
        }
        Ok(())
    }

    pub fn types(&self) -> Vec<&str> {
        self.format_index.keys().map(|k| k.as_str()).collect()
    }

    pub fn files(&self) -> Vec<&DragItem> {
        self.items.iter().filter(|i| i.kind == "file").collect()
    }
}

impl Default for DataTransfer {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_data_in_rw_mode() {
        let mut d = DataTransfer::new();
        d.mode = TransferMode::ReadWrite;
        d.set_data("text/plain", "hello").unwrap();
        assert_eq!(d.get_data("text/plain").as_deref(), Some("hello"));
    }

    #[test]
    fn set_data_protected_fails() {
        let mut d = DataTransfer::new();
        assert!(d.set_data("text/plain", "x").is_err());
    }

    #[test]
    fn replace_same_format() {
        let mut d = DataTransfer::new();
        d.mode = TransferMode::ReadWrite;
        d.set_data("x", "1").unwrap();
        d.set_data("x", "2").unwrap();
        assert_eq!(d.items.len(), 1);
        assert_eq!(d.get_data("x").as_deref(), Some("2"));
    }

    #[test]
    fn clear_specific_format() {
        let mut d = DataTransfer::new();
        d.mode = TransferMode::ReadWrite;
        d.set_data("a", "1").unwrap();
        d.set_data("b", "2").unwrap();
        d.clear_data(Some("a")).unwrap();
        assert!(d.get_data("a").is_none());
        assert_eq!(d.get_data("b").as_deref(), Some("2"));
    }

    #[test]
    fn clear_all() {
        let mut d = DataTransfer::new();
        d.mode = TransferMode::ReadWrite;
        d.set_data("a", "1").unwrap();
        d.clear_data(None).unwrap();
        assert!(d.items.is_empty());
    }

    #[test]
    fn protected_mode_blocks_read() {
        let mut d = DataTransfer::new();
        d.mode = TransferMode::ReadWrite;
        d.set_data("x", "1").unwrap();
        d.mode = TransferMode::Protected;
        assert!(d.get_data("x").is_none());
    }
}
