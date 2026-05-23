//! Heap profiler - object sampling + retainer graph.
//!
//! Chrome DevTools "Memory" tab uses HeapProfiler domain.
//! Two snapshot modes:
//! 1. Allocation sampling - per-allocation byte cost tracking.
//! 2. Heap snapshot - full retainer graph (V8 RetainerVisitor).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectKind {
    Object,
    String,
    Array,
    Function,
    HiddenClass,    // V8 shape
    Closure,
    Code,
    Heap,           // native heap pointer
    Symbol,
    BigInt,
    Synthetic,      // GC root, etc.
}

#[derive(Debug, Clone)]
pub struct HeapObject {
    pub id: u64,
    pub kind: ObjectKind,
    pub size_bytes: u64,
    pub self_size: u64,
    pub class_name: String,
    pub name: String,
    pub edges_out: Vec<HeapEdge>,
}

#[derive(Debug, Clone)]
pub struct HeapEdge {
    pub to_id: u64,
    pub kind: EdgeKind,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeKind {
    Property,       // .foo
    Element,        // [0]
    Internal,       // hidden link
    Hidden,
    Shortcut,
    ContextVar,
    Weak,
}

#[derive(Default)]
pub struct HeapSnapshot {
    pub objects: HashMap<u64, HeapObject>,
    pub roots: Vec<u64>,
}

impl HeapSnapshot {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, obj: HeapObject) {
        self.objects.insert(obj.id, obj);
    }

    pub fn total_size(&self) -> u64 {
        self.objects.values().map(|o| o.self_size).sum()
    }

    /// Per-class aggregation: class_name -> (count, total_bytes).
    pub fn by_class(&self) -> HashMap<String, (u32, u64)> {
        let mut out: HashMap<String, (u32, u64)> = HashMap::new();
        for o in self.objects.values() {
            let entry = out.entry(o.class_name.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += o.self_size;
        }
        out
    }

    /// Reachable set from `roots` by BFS over edges_out.
    pub fn reachable(&self) -> Vec<u64> {
        let mut visited = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<u64> = self.roots.iter().copied().collect();
        while let Some(id) = queue.pop_front() {
            if !visited.insert(id) { continue; }
            if let Some(o) = self.objects.get(&id) {
                for e in &o.edges_out {
                    if !visited.contains(&e.to_id) { queue.push_back(e.to_id); }
                }
            }
        }
        visited.into_iter().collect()
    }

    /// Find shortest retainer path from roots to target.
    pub fn shortest_path_from_root(&self, target: u64) -> Option<Vec<u64>> {
        if self.roots.contains(&target) { return Some(vec![target]); }
        let mut prev: HashMap<u64, u64> = HashMap::new();
        let mut queue: std::collections::VecDeque<u64> = self.roots.iter().copied().collect();
        let mut visited = std::collections::HashSet::new();
        for r in &self.roots { visited.insert(*r); }
        while let Some(id) = queue.pop_front() {
            let Some(o) = self.objects.get(&id) else { continue; };
            for e in &o.edges_out {
                if visited.insert(e.to_id) {
                    prev.insert(e.to_id, id);
                    if e.to_id == target {
                        let mut path = vec![target];
                        let mut cur = target;
                        while let Some(p) = prev.get(&cur).copied() {
                            path.push(p);
                            cur = p;
                            if self.roots.contains(&p) { break; }
                        }
                        path.reverse();
                        return Some(path);
                    }
                    queue.push_back(e.to_id);
                }
            }
        }
        None
    }
}

#[derive(Default)]
pub struct AllocationSampler {
    pub samples: Vec<AllocationSample>,
    pub sample_rate_bytes: u64,        // sample every N bytes allocated
    pub running_total: u64,
    pub next_threshold: u64,
}

#[derive(Debug, Clone)]
pub struct AllocationSample {
    pub size: u64,
    pub stack_trace_id: u64,
    pub timestamp_unix_ms: u64,
}

impl AllocationSampler {
    pub fn new(sample_rate_bytes: u64) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate_bytes,
            running_total: 0,
            next_threshold: sample_rate_bytes,
        }
    }

    /// Record an allocation. May or may not produce a sample.
    pub fn allocate(&mut self, size: u64, stack_trace_id: u64, now: u64) -> bool {
        self.running_total += size;
        if self.running_total >= self.next_threshold {
            self.samples.push(AllocationSample { size, stack_trace_id, timestamp_unix_ms: now });
            self.next_threshold = self.running_total + self.sample_rate_bytes;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(id: u64, class: &str, size: u64) -> HeapObject {
        HeapObject {
            id, kind: ObjectKind::Object,
            size_bytes: size, self_size: size,
            class_name: class.into(), name: String::new(),
            edges_out: Vec::new(),
        }
    }

    #[test]
    fn snapshot_aggregates() {
        let mut s = HeapSnapshot::new();
        s.add(obj(1, "Array", 100));
        s.add(obj(2, "Array", 200));
        s.add(obj(3, "Object", 50));
        let by = s.by_class();
        assert_eq!(by.get("Array"), Some(&(2u32, 300u64)));
        assert_eq!(s.total_size(), 350);
    }

    #[test]
    fn reachable_walks_edges() {
        let mut s = HeapSnapshot::new();
        let mut o = obj(1, "Root", 10);
        o.edges_out.push(HeapEdge { to_id: 2, kind: EdgeKind::Property, label: "child".into() });
        s.add(o);
        s.add(obj(2, "Child", 10));
        s.add(obj(99, "Orphan", 10));
        s.roots = vec![1];
        let r = s.reachable();
        assert!(r.contains(&1));
        assert!(r.contains(&2));
        assert!(!r.contains(&99));
    }

    #[test]
    fn shortest_path() {
        let mut s = HeapSnapshot::new();
        let mut o1 = obj(1, "R", 10);
        o1.edges_out.push(HeapEdge { to_id: 2, kind: EdgeKind::Property, label: "a".into() });
        let mut o2 = obj(2, "M", 10);
        o2.edges_out.push(HeapEdge { to_id: 3, kind: EdgeKind::Property, label: "b".into() });
        s.add(o1);
        s.add(o2);
        s.add(obj(3, "T", 10));
        s.roots = vec![1];
        let path = s.shortest_path_from_root(3).unwrap();
        assert_eq!(path, vec![1, 2, 3]);
    }

    #[test]
    fn sampler_below_threshold_no_sample() {
        let mut s = AllocationSampler::new(1000);
        assert!(!s.allocate(500, 1, 0));
        assert!(s.samples.is_empty());
    }

    #[test]
    fn sampler_crosses_threshold_records() {
        let mut s = AllocationSampler::new(1000);
        assert!(s.allocate(1500, 1, 0));
        assert_eq!(s.samples.len(), 1);
    }
}
