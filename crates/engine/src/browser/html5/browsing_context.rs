//! Browsing context, top-level + nested (iframes), session history.
//!
//! Spec: https://html.spec.whatwg.org/multipage/document-sequences.html#browsing-context
//! Each tab is a top-level browsing context; iframes nest below.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SandboxFlag {
    AllowScripts,
    AllowSameOrigin,
    AllowForms,
    AllowPopups,
    AllowModals,
    AllowOrientationLock,
    AllowPointerLock,
    AllowPresentation,
    AllowPopupsToEscapeSandbox,
    AllowTopNavigation,
    AllowTopNavigationByUserActivation,
    AllowDownloads,
    AllowStorageAccessByUserActivation,
}

#[derive(Debug, Clone)]
pub struct BrowsingContext {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub kind: BrowsingContextKind,
    pub current_url: String,
    pub history: Vec<String>,
    pub history_idx: usize,
    pub sandbox_flags: u32,
    pub container_element_id: Option<u64>,    // <iframe> host
    pub group_id: u64,                        // browsing-context group (COOP)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowsingContextKind {
    TopLevel,
    Iframe,
    PopupWindow,
}

#[derive(Default)]
pub struct BrowsingContextRegistry {
    pub contexts: HashMap<u64, BrowsingContext>,
    pub next_id: u64,
    pub next_group_id: u64,
}

impl BrowsingContextRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create_top_level(&mut self) -> u64 {
        self.next_id += 1;
        self.next_group_id += 1;
        let id = self.next_id;
        self.contexts.insert(id, BrowsingContext {
            id, parent_id: None,
            kind: BrowsingContextKind::TopLevel,
            current_url: String::new(),
            history: Vec::new(), history_idx: 0,
            sandbox_flags: 0,
            container_element_id: None,
            group_id: self.next_group_id,
        });
        id
    }

    pub fn create_iframe(&mut self, parent_id: u64, host_element_id: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let parent_group = self.contexts.get(&parent_id).map(|p| p.group_id).unwrap_or(0);
        self.contexts.insert(id, BrowsingContext {
            id, parent_id: Some(parent_id),
            kind: BrowsingContextKind::Iframe,
            current_url: String::new(),
            history: Vec::new(), history_idx: 0,
            sandbox_flags: 0,
            container_element_id: Some(host_element_id),
            group_id: parent_group,
        });
        id
    }

    pub fn navigate(&mut self, id: u64, url: &str) {
        if let Some(c) = self.contexts.get_mut(&id) {
            if c.history_idx + 1 < c.history.len() {
                c.history.truncate(c.history_idx + 1);
            }
            c.history.push(url.into());
            c.history_idx = c.history.len() - 1;
            c.current_url = url.into();
        }
    }

    pub fn back(&mut self, id: u64) -> Option<&str> {
        let c = self.contexts.get_mut(&id)?;
        if c.history_idx == 0 { return None; }
        c.history_idx -= 1;
        c.current_url = c.history[c.history_idx].clone();
        Some(&c.current_url)
    }

    pub fn forward(&mut self, id: u64) -> Option<&str> {
        let c = self.contexts.get_mut(&id)?;
        if c.history_idx + 1 >= c.history.len() { return None; }
        c.history_idx += 1;
        c.current_url = c.history[c.history_idx].clone();
        Some(&c.current_url)
    }

    pub fn set_sandbox(&mut self, id: u64, flag: SandboxFlag) {
        if let Some(c) = self.contexts.get_mut(&id) {
            c.sandbox_flags |= 1 << flag.bit();
        }
    }

    pub fn has_sandbox(&self, id: u64, flag: SandboxFlag) -> bool {
        self.contexts.get(&id).map(|c| (c.sandbox_flags & (1 << flag.bit())) != 0).unwrap_or(false)
    }
}

impl SandboxFlag {
    pub fn bit(&self) -> u32 {
        match self {
            Self::AllowScripts => 0,
            Self::AllowSameOrigin => 1,
            Self::AllowForms => 2,
            Self::AllowPopups => 3,
            Self::AllowModals => 4,
            Self::AllowOrientationLock => 5,
            Self::AllowPointerLock => 6,
            Self::AllowPresentation => 7,
            Self::AllowPopupsToEscapeSandbox => 8,
            Self::AllowTopNavigation => 9,
            Self::AllowTopNavigationByUserActivation => 10,
            Self::AllowDownloads => 11,
            Self::AllowStorageAccessByUserActivation => 12,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_top_level_unique_group() {
        let mut r = BrowsingContextRegistry::new();
        let a = r.create_top_level();
        let b = r.create_top_level();
        assert_ne!(r.contexts[&a].group_id, r.contexts[&b].group_id);
    }

    #[test]
    fn iframe_inherits_group() {
        let mut r = BrowsingContextRegistry::new();
        let top = r.create_top_level();
        let child = r.create_iframe(top, 100);
        assert_eq!(r.contexts[&top].group_id, r.contexts[&child].group_id);
    }

    #[test]
    fn navigate_extends_history() {
        let mut r = BrowsingContextRegistry::new();
        let id = r.create_top_level();
        r.navigate(id, "https://a.com");
        r.navigate(id, "https://b.com");
        assert_eq!(r.contexts[&id].history.len(), 2);
        assert_eq!(r.contexts[&id].current_url, "https://b.com");
    }

    #[test]
    fn back_walks_history() {
        let mut r = BrowsingContextRegistry::new();
        let id = r.create_top_level();
        r.navigate(id, "https://a.com");
        r.navigate(id, "https://b.com");
        let prev = r.back(id);
        assert_eq!(prev, Some("https://a.com"));
    }

    #[test]
    fn navigate_truncates_forward() {
        let mut r = BrowsingContextRegistry::new();
        let id = r.create_top_level();
        r.navigate(id, "https://a.com");
        r.navigate(id, "https://b.com");
        r.back(id);
        r.navigate(id, "https://c.com");
        assert_eq!(r.contexts[&id].history.len(), 2);
    }

    #[test]
    fn sandbox_flag_set() {
        let mut r = BrowsingContextRegistry::new();
        let id = r.create_top_level();
        r.set_sandbox(id, SandboxFlag::AllowScripts);
        assert!(r.has_sandbox(id, SandboxFlag::AllowScripts));
        assert!(!r.has_sandbox(id, SandboxFlag::AllowForms));
    }
}
