//! Spellcheck stub - dictionary lookup + suggestion engine.
//!
//! Foundation only; real impl plugs into hunspell or OS spellchecker.

use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    pub locale: String,             // e.g. "en-US"
    pub words: HashSet<String>,
    pub user_added: HashSet<String>,
}

impl Dictionary {
    pub fn new(locale: &str) -> Self {
        Self { locale: locale.into(), words: HashSet::new(), user_added: HashSet::new() }
    }

    pub fn load_words(&mut self, words: impl IntoIterator<Item = String>) {
        for w in words { self.words.insert(w.to_ascii_lowercase()); }
    }

    pub fn is_known(&self, word: &str) -> bool {
        let lower = word.to_ascii_lowercase();
        self.words.contains(&lower) || self.user_added.contains(&lower)
    }

    pub fn add_to_personal(&mut self, word: &str) {
        self.user_added.insert(word.to_ascii_lowercase());
    }

    /// Return up to N candidate replacements with edit distance 1 against the dictionary.
    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let lower = word.to_ascii_lowercase();
        let mut out = Vec::new();
        for entry in self.words.iter().chain(self.user_added.iter()) {
            if levenshtein_le(&lower, entry, 1) {
                out.push(entry.clone());
                if out.len() >= max { break; }
            }
        }
        out
    }
}

/// True if edit distance between a and b is <= threshold.
pub fn levenshtein_le(a: &str, b: &str, threshold: usize) -> bool {
    let la = a.chars().count();
    let lb = b.chars().count();
    if la.abs_diff(lb) > threshold { return false; }
    let m = a.chars().collect::<Vec<_>>();
    let n = b.chars().collect::<Vec<_>>();
    let mut prev: Vec<usize> = (0..=n.len()).collect();
    let mut curr = vec![0; n.len() + 1];
    for i in 1..=m.len() {
        curr[0] = i;
        let mut row_min = i;
        for j in 1..=n.len() {
            let cost = if m[i - 1] == n[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            if curr[j] < row_min { row_min = curr[j]; }
        }
        if row_min > threshold { return false; }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n.len()] <= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_recognizes_known() {
        let mut d = Dictionary::new("en-US");
        d.load_words(["hello".into(), "world".into()]);
        assert!(d.is_known("hello"));
        assert!(!d.is_known("xyzzy"));
    }

    #[test]
    fn personal_word() {
        let mut d = Dictionary::new("en-US");
        d.add_to_personal("FooBar");
        assert!(d.is_known("foobar"));
    }

    #[test]
    fn levenshtein_within_1() {
        assert!(levenshtein_le("cat", "car", 1));
        assert!(levenshtein_le("cat", "cats", 1));
        assert!(!levenshtein_le("cat", "dogs", 1));
    }

    #[test]
    fn suggest_picks_close() {
        let mut d = Dictionary::new("en-US");
        d.load_words(["cat".into(), "cot".into(), "dog".into()]);
        let suggestions = d.suggest("cit", 5);
        // both cat and cot are edit-distance 1
        assert_eq!(suggestions.len(), 2);
    }

    #[test]
    fn case_insensitive() {
        let mut d = Dictionary::new("en-US");
        d.load_words(["Apple".into()]);
        assert!(d.is_known("APPLE"));
        assert!(d.is_known("apple"));
    }
}
