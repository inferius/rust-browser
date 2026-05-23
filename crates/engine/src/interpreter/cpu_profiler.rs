//! Sampling CPU profiler - per-tick stack snapshot aggregation.
//!
//! Output compatible with Chrome DevTools "Performance" tab.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ProfileNode {
    pub id: u32,
    pub function_name: String,
    pub script_id: u64,
    pub url: String,
    pub line: u32,
    pub column: u32,
    pub hit_count: u32,
    pub children: Vec<u32>,
    pub parent: Option<u32>,
    pub deopt_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileSample {
    pub node_id: u32,
    pub timestamp_us: u64,
}

#[derive(Default)]
pub struct CpuProfile {
    pub nodes: HashMap<u32, ProfileNode>,
    pub samples: Vec<ProfileSample>,
    pub start_time_us: u64,
    pub end_time_us: u64,
    pub next_node_id: u32,
    pub sample_interval_us: u32,
}

impl CpuProfile {
    pub fn new(sample_interval_us: u32) -> Self {
        Self { sample_interval_us, ..Self::default() }
    }

    pub fn ensure_root(&mut self) -> u32 {
        if self.nodes.contains_key(&1) { return 1; }
        self.next_node_id = 1;
        self.nodes.insert(1, ProfileNode {
            id: 1, function_name: "(root)".into(), script_id: 0,
            url: String::new(), line: 0, column: 0,
            hit_count: 0, children: Vec::new(), parent: None,
            deopt_reason: None,
        });
        1
    }

    pub fn create_node(&mut self, parent: u32, function_name: &str, script_id: u64, url: &str, line: u32, column: u32) -> u32 {
        self.next_node_id += 1;
        let id = self.next_node_id;
        self.nodes.insert(id, ProfileNode {
            id, function_name: function_name.into(),
            script_id, url: url.into(), line, column,
            hit_count: 0, children: Vec::new(),
            parent: Some(parent), deopt_reason: None,
        });
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.children.push(id);
        }
        id
    }

    pub fn record_sample(&mut self, leaf_node_id: u32, timestamp_us: u64) {
        if let Some(n) = self.nodes.get_mut(&leaf_node_id) {
            n.hit_count += 1;
        }
        self.samples.push(ProfileSample { node_id: leaf_node_id, timestamp_us });
    }

    pub fn self_time_us(&self, node_id: u32) -> u64 {
        let mut total = 0u64;
        for s in &self.samples {
            if s.node_id == node_id { total += self.sample_interval_us as u64; }
        }
        total
    }

    pub fn total_time_us(&self, node_id: u32) -> u64 {
        let mut total = self.self_time_us(node_id);
        if let Some(n) = self.nodes.get(&node_id) {
            for c in &n.children {
                total += self.total_time_us(*c);
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_exists_after_ensure() {
        let mut p = CpuProfile::new(1000);
        let id = p.ensure_root();
        assert_eq!(id, 1);
        assert!(p.nodes.contains_key(&1));
    }

    #[test]
    fn create_node_parented() {
        let mut p = CpuProfile::new(1000);
        let root = p.ensure_root();
        let n = p.create_node(root, "foo", 0, "x.js", 1, 1);
        assert_eq!(p.nodes[&n].parent, Some(root));
        assert!(p.nodes[&root].children.contains(&n));
    }

    #[test]
    fn record_sample_increments_hit() {
        let mut p = CpuProfile::new(1000);
        let root = p.ensure_root();
        let n = p.create_node(root, "f", 0, "x", 1, 1);
        p.record_sample(n, 0);
        p.record_sample(n, 1000);
        assert_eq!(p.nodes[&n].hit_count, 2);
    }

    #[test]
    fn self_time_in_microseconds() {
        let mut p = CpuProfile::new(1000);
        let root = p.ensure_root();
        let n = p.create_node(root, "f", 0, "x", 1, 1);
        p.record_sample(n, 0);
        p.record_sample(n, 1000);
        p.record_sample(n, 2000);
        assert_eq!(p.self_time_us(n), 3000);
    }

    #[test]
    fn total_time_includes_children() {
        let mut p = CpuProfile::new(1000);
        let root = p.ensure_root();
        let parent = p.create_node(root, "p", 0, "x", 1, 1);
        let child = p.create_node(parent, "c", 0, "x", 2, 1);
        p.record_sample(parent, 0);
        p.record_sample(child, 1000);
        // parent self = 1000, child = 1000 -> total 2000
        assert_eq!(p.total_time_us(parent), 2000);
    }
}
