//! Navigation API - moderni history nahrada s entry list + intercept.
//!
//! Spec: https://html.spec.whatwg.org/multipage/nav-history-apis.html
//! navigation.navigate(url, opts), navigation.entries(), navigation.currentEntry.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavigationType {
    Push,
    Replace,
    Reload,
    Traverse,
}

#[derive(Debug, Clone)]
pub struct NavigationEntry {
    pub id: u64,
    pub key: String,                   // stable across reloads of same entry
    pub url: String,
    pub state: Option<String>,         // structured-cloned state (JSON-serialized placeholder)
    pub same_document: bool,
}

#[derive(Default)]
pub struct NavigationApi {
    pub entries: Vec<NavigationEntry>,
    pub current_index: usize,
    pub next_id: u64,
    pub key_counter: u64,
    /// Intercept handler ids - cislo registered intercept handlers per entry.
    pub intercepts: HashMap<u64, Vec<String>>,
}

impl NavigationApi {
    pub fn new() -> Self { Self::default() }

    fn alloc(&mut self, url: &str, state: Option<String>, same_doc: bool) -> NavigationEntry {
        self.next_id += 1;
        self.key_counter += 1;
        NavigationEntry {
            id: self.next_id,
            key: format!("nav-{}", self.key_counter),
            url: url.into(),
            state,
            same_document: same_doc,
        }
    }

    pub fn navigate(&mut self, url: &str, replace: bool, state: Option<String>) -> u64 {
        let entry = self.alloc(url, state, true);
        let id = entry.id;
        if replace && !self.entries.is_empty() {
            self.entries[self.current_index] = entry;
        } else {
            // truncate forward stack
            if self.current_index + 1 < self.entries.len() {
                self.entries.truncate(self.current_index + 1);
            }
            if self.entries.is_empty() {
                self.entries.push(entry);
            } else {
                self.entries.push(entry);
                self.current_index += 1;
            }
        }
        id
    }

    pub fn back(&mut self) -> Option<&NavigationEntry> {
        if self.current_index == 0 { return None; }
        self.current_index -= 1;
        Some(&self.entries[self.current_index])
    }

    pub fn forward(&mut self) -> Option<&NavigationEntry> {
        if self.current_index + 1 >= self.entries.len() { return None; }
        self.current_index += 1;
        Some(&self.entries[self.current_index])
    }

    pub fn traverse_to(&mut self, key: &str) -> Option<&NavigationEntry> {
        let idx = self.entries.iter().position(|e| e.key == key)?;
        self.current_index = idx;
        Some(&self.entries[idx])
    }

    pub fn current_entry(&self) -> Option<&NavigationEntry> {
        self.entries.get(self.current_index)
    }

    pub fn can_go_back(&self) -> bool { self.current_index > 0 }
    pub fn can_go_forward(&self) -> bool { self.current_index + 1 < self.entries.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigate_push_appends() {
        let mut n = NavigationApi::new();
        n.navigate("/a", false, None);
        n.navigate("/b", false, None);
        assert_eq!(n.entries.len(), 2);
        assert_eq!(n.current_entry().unwrap().url, "/b");
    }

    #[test]
    fn replace_overwrites_current() {
        let mut n = NavigationApi::new();
        n.navigate("/a", false, None);
        n.navigate("/b", true, None);
        assert_eq!(n.entries.len(), 1);
        assert_eq!(n.current_entry().unwrap().url, "/b");
    }

    #[test]
    fn back_forward_round_trip() {
        let mut n = NavigationApi::new();
        n.navigate("/a", false, None);
        n.navigate("/b", false, None);
        n.navigate("/c", false, None);
        n.back();
        assert_eq!(n.current_entry().unwrap().url, "/b");
        n.back();
        assert_eq!(n.current_entry().unwrap().url, "/a");
        n.forward();
        assert_eq!(n.current_entry().unwrap().url, "/b");
    }

    #[test]
    fn forward_truncation() {
        let mut n = NavigationApi::new();
        n.navigate("/a", false, None);
        n.navigate("/b", false, None);
        n.back();
        n.navigate("/c", false, None);
        assert_eq!(n.entries.len(), 2);
        assert_eq!(n.current_entry().unwrap().url, "/c");
    }

    #[test]
    fn traverse_by_key() {
        let mut n = NavigationApi::new();
        n.navigate("/a", false, None);
        n.navigate("/b", false, None);
        let key = n.entries[0].key.clone();
        n.traverse_to(&key);
        assert_eq!(n.current_entry().unwrap().url, "/a");
    }
}
