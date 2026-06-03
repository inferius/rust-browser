//! Tab groups - Chrome-style colored groupings of related tabs.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GroupColor {
    Grey,
    Blue,
    Red,
    Yellow,
    Green,
    Pink,
    Purple,
    Cyan,
    Orange,
}

#[derive(Debug, Clone)]
pub struct TabGroup {
    pub id: u64,
    pub title: String,
    pub color: GroupColor,
    pub collapsed: bool,
    pub tab_ids: Vec<u64>,
}

#[derive(Default)]
pub struct TabGroupManager {
    pub groups: HashMap<u64, TabGroup>,
    pub next_id: u64,
}

impl TabGroupManager {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, title: &str, color: GroupColor) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.groups.insert(id, TabGroup {
            id, title: title.into(), color,
            collapsed: false, tab_ids: Vec::new(),
        });
        id
    }

    pub fn add_tab(&mut self, group_id: u64, tab_id: u64) -> bool {
        // Remove from any existing group first.
        for g in self.groups.values_mut() {
            g.tab_ids.retain(|t| *t != tab_id);
        }
        if let Some(g) = self.groups.get_mut(&group_id) {
            g.tab_ids.push(tab_id);
            return true;
        }
        false
    }

    pub fn remove_tab(&mut self, tab_id: u64) {
        for g in self.groups.values_mut() {
            g.tab_ids.retain(|t| *t != tab_id);
        }
    }

    pub fn group_of_tab(&self, tab_id: u64) -> Option<u64> {
        for g in self.groups.values() {
            if g.tab_ids.contains(&tab_id) { return Some(g.id); }
        }
        None
    }

    pub fn collapse(&mut self, group_id: u64, collapsed: bool) {
        if let Some(g) = self.groups.get_mut(&group_id) {
            g.collapsed = collapsed;
        }
    }

    pub fn delete(&mut self, group_id: u64) -> Vec<u64> {
        if let Some(g) = self.groups.remove(&group_id) {
            return g.tab_ids;
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_returns_unique_ids() {
        let mut m = TabGroupManager::new();
        let a = m.create("Work", GroupColor::Blue);
        let b = m.create("Fun", GroupColor::Red);
        assert_ne!(a, b);
    }

    #[test]
    fn add_tab_to_group() {
        let mut m = TabGroupManager::new();
        let g = m.create("X", GroupColor::Blue);
        assert!(m.add_tab(g, 100));
        assert_eq!(m.group_of_tab(100), Some(g));
    }

    #[test]
    fn moving_tab_removes_from_old() {
        let mut m = TabGroupManager::new();
        let g1 = m.create("A", GroupColor::Blue);
        let g2 = m.create("B", GroupColor::Red);
        m.add_tab(g1, 100);
        m.add_tab(g2, 100);
        assert_eq!(m.group_of_tab(100), Some(g2));
    }

    #[test]
    fn collapse_state() {
        let mut m = TabGroupManager::new();
        let g = m.create("X", GroupColor::Blue);
        m.collapse(g, true);
        assert!(m.groups[&g].collapsed);
    }

    #[test]
    fn delete_returns_tab_ids() {
        let mut m = TabGroupManager::new();
        let g = m.create("X", GroupColor::Blue);
        m.add_tab(g, 100);
        m.add_tab(g, 200);
        let ids = m.delete(g);
        assert_eq!(ids, vec![100, 200]);
        assert!(m.groups.is_empty());
    }
}
