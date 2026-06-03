//! Topics API - on-device interest topics nahrazujici third-party cookies.
//!
//! Spec: https://patcg-individual-drafts.github.io/topics/
//! document.browsingTopics() - vraci [0..3] topics z taxonomy V2 (~349 entries).
//! Per-week epoch, 25% random shuffle, only for participating origins.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Topic {
    pub id: u32,             // taxonomy id (1..349)
    pub label: String,       // "/Travel & Transportation/Hotels & Accommodations"
    pub model_version: u32,
    pub epoch_id: u64,
}

#[derive(Default)]
pub struct TopicsApi {
    /// per-host visit history (host -> top-level visit count this epoch).
    pub visit_counts: HashMap<String, u32>,
    /// per-epoch top 5 topics (last 3 epochs visible).
    pub epochs: Vec<EpochTopics>,
    pub current_epoch: u64,
    /// Origins ktere mohou ucastnit (CHIPS-style allowlist).
    pub participants: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct EpochTopics {
    pub epoch_id: u64,
    pub topics: Vec<Topic>, // top-5
}

impl TopicsApi {
    pub fn new() -> Self { Self::default() }

    pub fn record_visit(&mut self, host: &str) {
        *self.visit_counts.entry(host.into()).or_insert(0) += 1;
    }

    pub fn add_participant(&mut self, origin: &str) {
        self.participants.insert(origin.into());
    }

    /// Computed top topics for the current epoch from visit counts via lookup table.
    /// Real impl: classifier model (BERT-lite) on host -> topic.
    pub fn compute_epoch(&mut self, classifier: &HashMap<String, u32>, taxonomy: &HashMap<u32, String>) {
        let mut counts: HashMap<u32, u32> = HashMap::new();
        for (host, n) in &self.visit_counts {
            if let Some(tid) = classifier.get(host) {
                *counts.entry(*tid).or_insert(0) += n;
            }
        }
        let mut ranked: Vec<(u32, u32)> = counts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        ranked.truncate(5);
        let epoch = EpochTopics {
            epoch_id: self.current_epoch,
            topics: ranked.into_iter().map(|(id, _)| Topic {
                id,
                label: taxonomy.get(&id).cloned().unwrap_or_default(),
                model_version: 4,
                epoch_id: self.current_epoch,
            }).collect(),
        };
        self.epochs.push(epoch);
        if self.epochs.len() > 3 { let _ = self.epochs.remove(0); }
        self.current_epoch += 1;
        self.visit_counts.clear();
    }

    /// document.browsingTopics() returns up to 3 (one per recent epoch, random pick from top-5).
    pub fn browsing_topics(&self, caller_origin: &str, rand_index: usize) -> Vec<Topic> {
        if !self.participants.contains(caller_origin) { return Vec::new(); }
        self.epochs.iter().filter_map(|e| {
            if e.topics.is_empty() { return None; }
            Some(e.topics[rand_index % e.topics.len()].clone())
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taxonomy() -> HashMap<u32, String> {
        let mut m = HashMap::new();
        m.insert(1, "/Travel".into());
        m.insert(2, "/Sports".into());
        m
    }
    fn classifier() -> HashMap<String, u32> {
        let mut m = HashMap::new();
        m.insert("hotels.com".into(), 1);
        m.insert("espn.com".into(), 2);
        m
    }

    #[test]
    fn epoch_computed_from_visits() {
        let mut t = TopicsApi::new();
        t.record_visit("hotels.com");
        t.record_visit("hotels.com");
        t.record_visit("espn.com");
        t.compute_epoch(&classifier(), &taxonomy());
        assert_eq!(t.epochs.len(), 1);
        assert_eq!(t.epochs[0].topics[0].id, 1); // hotels has more visits
    }

    #[test]
    fn participation_required() {
        let mut t = TopicsApi::new();
        t.record_visit("hotels.com");
        t.compute_epoch(&classifier(), &taxonomy());
        assert!(t.browsing_topics("https://other.com", 0).is_empty());
        t.add_participant("https://other.com");
        assert_eq!(t.browsing_topics("https://other.com", 0).len(), 1);
    }

    #[test]
    fn epoch_window_3() {
        let mut t = TopicsApi::new();
        for _ in 0..5 {
            t.record_visit("hotels.com");
            t.compute_epoch(&classifier(), &taxonomy());
        }
        assert_eq!(t.epochs.len(), 3);
    }
}
