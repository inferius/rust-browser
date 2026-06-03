//! Performance API - performance.now(), performance.mark/measure, entries.
//!
//! Spec: https://www.w3.org/TR/performance-timeline/

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PerfEntryType {
    Mark,
    Measure,
    Navigation,
    Resource,
    Paint,
    Element,
    LongTask,
    LayoutShift,
    LargestContentfulPaint,
    FirstInput,
    Event,
}

#[derive(Debug, Clone)]
pub struct PerfEntry {
    pub name: String,
    pub entry_type: PerfEntryType,
    pub start_time_ms: f64,
    pub duration_ms: f64,
}

pub struct Performance {
    pub time_origin: Instant,
    pub entries: Vec<PerfEntry>,
    pub max_entries: usize,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            time_origin: Instant::now(),
            entries: Vec::new(),
            max_entries: 1000,
        }
    }
}

impl Performance {
    pub fn new() -> Self { Self::default() }

    /// `performance.now()` - DOMHighResTimeStamp in milliseconds.
    pub fn now(&self) -> f64 {
        self.time_origin.elapsed().as_secs_f64() * 1000.0
    }

    /// `performance.mark(name)` - record point-in-time.
    pub fn mark(&mut self, name: &str) {
        self.push_entry(PerfEntry {
            name: name.into(),
            entry_type: PerfEntryType::Mark,
            start_time_ms: self.now(),
            duration_ms: 0.0,
        });
    }

    /// `performance.measure(name, start_mark, end_mark)` - record duration mezi mark events.
    pub fn measure(&mut self, name: &str, start_mark: Option<&str>, end_mark: Option<&str>) {
        let start = start_mark.and_then(|n| self.find_mark(n)).unwrap_or(0.0);
        let end = end_mark.and_then(|n| self.find_mark(n)).unwrap_or_else(|| self.now());
        let now_v = self.now();
        self.push_entry(PerfEntry {
            name: name.into(),
            entry_type: PerfEntryType::Measure,
            start_time_ms: start,
            duration_ms: (end - start).max(0.0),
        });
        let _ = now_v;
    }

    fn find_mark(&self, name: &str) -> Option<f64> {
        self.entries.iter()
            .filter(|e| e.entry_type == PerfEntryType::Mark && e.name == name)
            .last()
            .map(|e| e.start_time_ms)
    }

    pub fn clear_marks(&mut self, name: Option<&str>) {
        match name {
            Some(n) => self.entries.retain(|e| !(e.entry_type == PerfEntryType::Mark && e.name == n)),
            None => self.entries.retain(|e| e.entry_type != PerfEntryType::Mark),
        }
    }

    pub fn clear_measures(&mut self, name: Option<&str>) {
        match name {
            Some(n) => self.entries.retain(|e| !(e.entry_type == PerfEntryType::Measure && e.name == n)),
            None => self.entries.retain(|e| e.entry_type != PerfEntryType::Measure),
        }
    }

    pub fn get_entries(&self) -> &[PerfEntry] {
        &self.entries
    }

    pub fn get_entries_by_type(&self, t: PerfEntryType) -> Vec<&PerfEntry> {
        self.entries.iter().filter(|e| e.entry_type == t).collect()
    }

    pub fn get_entries_by_name(&self, name: &str) -> Vec<&PerfEntry> {
        self.entries.iter().filter(|e| e.name == name).collect()
    }

    fn push_entry(&mut self, e: PerfEntry) {
        self.entries.push(e);
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_monotonic() {
        let p = Performance::new();
        let a = p.now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = p.now();
        assert!(b > a);
    }

    #[test]
    fn mark_and_measure() {
        let mut p = Performance::new();
        p.mark("start");
        std::thread::sleep(std::time::Duration::from_millis(5));
        p.mark("end");
        p.measure("dur", Some("start"), Some("end"));
        let m = p.get_entries_by_name("dur");
        assert_eq!(m.len(), 1);
        assert!(m[0].duration_ms >= 4.0);
    }

    #[test]
    fn clear_marks() {
        let mut p = Performance::new();
        p.mark("a");
        p.mark("b");
        p.clear_marks(Some("a"));
        assert_eq!(p.get_entries_by_name("a").len(), 0);
        assert_eq!(p.get_entries_by_name("b").len(), 1);
    }

    #[test]
    fn entries_by_type() {
        let mut p = Performance::new();
        p.mark("x");
        p.measure("y", None, None);
        assert_eq!(p.get_entries_by_type(PerfEntryType::Mark).len(), 1);
        assert_eq!(p.get_entries_by_type(PerfEntryType::Measure).len(), 1);
    }
}
