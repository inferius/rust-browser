//! Bookmark storage - hierarchical folders + tags.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub tags: Vec<String>,
    pub added_unix_ms: u64,
    pub icon_url: Option<String>,
    pub folder_id: u64,
}

#[derive(Debug, Clone)]
pub struct BookmarkFolder {
    pub id: u64,
    pub name: String,
    pub parent_id: Option<u64>,
    pub child_folder_ids: Vec<u64>,
    pub bookmark_ids: Vec<u64>,
}

#[derive(Default)]
pub struct BookmarkTree {
    pub folders: HashMap<u64, BookmarkFolder>,
    pub bookmarks: HashMap<u64, Bookmark>,
    pub root_id: u64,
    pub next_id: u64,
}

impl BookmarkTree {
    pub fn new() -> Self {
        let mut t = Self::default();
        t.next_id = 1;
        let root = BookmarkFolder {
            id: 1, name: "Root".into(), parent_id: None,
            child_folder_ids: Vec::new(), bookmark_ids: Vec::new(),
        };
        t.folders.insert(1, root);
        t.root_id = 1;
        t
    }

    pub fn create_folder(&mut self, parent_id: u64, name: &str) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let folder = BookmarkFolder {
            id, name: name.into(), parent_id: Some(parent_id),
            child_folder_ids: Vec::new(), bookmark_ids: Vec::new(),
        };
        self.folders.insert(id, folder);
        if let Some(p) = self.folders.get_mut(&parent_id) {
            p.child_folder_ids.push(id);
        }
        id
    }

    pub fn add(&mut self, folder_id: u64, url: &str, title: &str, now: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let b = Bookmark {
            id, url: url.into(), title: title.into(),
            tags: Vec::new(), added_unix_ms: now,
            icon_url: None, folder_id,
        };
        self.bookmarks.insert(id, b);
        if let Some(f) = self.folders.get_mut(&folder_id) {
            f.bookmark_ids.push(id);
        }
        id
    }

    pub fn remove_bookmark(&mut self, id: u64) -> bool {
        if let Some(b) = self.bookmarks.remove(&id) {
            if let Some(f) = self.folders.get_mut(&b.folder_id) {
                f.bookmark_ids.retain(|x| *x != id);
            }
            return true;
        }
        false
    }

    pub fn move_bookmark(&mut self, id: u64, new_folder: u64) -> bool {
        let Some(b) = self.bookmarks.get(&id) else { return false; };
        let old_folder = b.folder_id;
        if let Some(f) = self.folders.get_mut(&old_folder) {
            f.bookmark_ids.retain(|x| *x != id);
        }
        if let Some(f) = self.folders.get_mut(&new_folder) {
            f.bookmark_ids.push(id);
        }
        self.bookmarks.get_mut(&id).unwrap().folder_id = new_folder;
        true
    }

    pub fn search(&self, query: &str) -> Vec<&Bookmark> {
        let q = query.to_ascii_lowercase();
        self.bookmarks.values()
            .filter(|b| b.title.to_ascii_lowercase().contains(&q)
                     || b.url.to_ascii_lowercase().contains(&q))
            .collect()
    }

    pub fn tag(&mut self, id: u64, tag: &str) -> bool {
        if let Some(b) = self.bookmarks.get_mut(&id) {
            if !b.tags.iter().any(|t| t == tag) { b.tags.push(tag.into()); }
            return true;
        }
        false
    }

    pub fn by_tag(&self, tag: &str) -> Vec<&Bookmark> {
        self.bookmarks.values().filter(|b| b.tags.iter().any(|t| t == tag)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_exists() {
        let t = BookmarkTree::new();
        assert!(t.folders.contains_key(&t.root_id));
    }

    #[test]
    fn create_folder_under_root() {
        let mut t = BookmarkTree::new();
        let f = t.create_folder(t.root_id, "Work");
        assert!(t.folders.contains_key(&f));
        assert!(t.folders[&t.root_id].child_folder_ids.contains(&f));
    }

    #[test]
    fn add_and_remove() {
        let mut t = BookmarkTree::new();
        let id = t.add(t.root_id, "https://x.com", "X", 0);
        assert!(t.bookmarks.contains_key(&id));
        assert!(t.remove_bookmark(id));
        assert!(!t.bookmarks.contains_key(&id));
    }

    #[test]
    fn move_between_folders() {
        let mut t = BookmarkTree::new();
        let f1 = t.create_folder(t.root_id, "A");
        let f2 = t.create_folder(t.root_id, "B");
        let id = t.add(f1, "https://x.com", "X", 0);
        t.move_bookmark(id, f2);
        assert!(t.folders[&f2].bookmark_ids.contains(&id));
        assert!(!t.folders[&f1].bookmark_ids.contains(&id));
    }

    #[test]
    fn search_matches_title_or_url() {
        let mut t = BookmarkTree::new();
        t.add(t.root_id, "https://example.com", "Example Domain", 0);
        t.add(t.root_id, "https://other.org", "Different", 0);
        assert_eq!(t.search("example").len(), 1);
        assert_eq!(t.search("Different").len(), 1);
    }

    #[test]
    fn tag_and_lookup() {
        let mut t = BookmarkTree::new();
        let id = t.add(t.root_id, "https://x.com", "X", 0);
        t.tag(id, "work");
        assert_eq!(t.by_tag("work").len(), 1);
    }
}
