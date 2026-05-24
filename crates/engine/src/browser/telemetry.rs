//! Lightweight metrics + telemetry collection (Histograms + counters).
//!
//! Inspired by Chromium UMA. Local-only by default; upload requires consent.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Histogram {
    pub name: String,
    pub buckets: Vec<(f64, u64)>,        // (upper_bound, count)
    pub sum: f64,
    pub samples: u64,
}

impl Histogram {
    pub fn exponential(name: &str, min: f64, max: f64, bucket_count: u32) -> Self {
        let mut buckets = Vec::with_capacity(bucket_count as usize);
        let ratio = (max / min).powf(1.0 / (bucket_count as f64 - 1.0));
        let mut b = min;
        for _ in 0..bucket_count {
            buckets.push((b, 0));
            b *= ratio;
        }
        Self { name: name.into(), buckets, sum: 0.0, samples: 0 }
    }

    pub fn record(&mut self, value: f64) {
        for (upper, count) in self.buckets.iter_mut() {
            if value <= *upper {
                *count += 1;
                self.sum += value;
                self.samples += 1;
                return;
            }
        }
        // Above last bucket - bump last.
        if let Some(last) = self.buckets.last_mut() {
            last.1 += 1;
            self.sum += value;
            self.samples += 1;
        }
    }

    pub fn mean(&self) -> f64 {
        if self.samples == 0 { 0.0 } else { self.sum / self.samples as f64 }
    }

    pub fn percentile(&self, p: f64) -> f64 {
        if self.samples == 0 { return 0.0; }
        let target = (self.samples as f64 * p).round() as u64;
        let mut running = 0u64;
        for (upper, count) in &self.buckets {
            running += count;
            if running >= target { return *upper; }
        }
        self.buckets.last().map(|(u, _)| *u).unwrap_or(0.0)
    }
}

#[derive(Default)]
pub struct MetricsStore {
    pub histograms: HashMap<String, Histogram>,
    pub counters: HashMap<String, u64>,
    pub user_consent: bool,
}

impl MetricsStore {
    pub fn new() -> Self { Self::default() }

    pub fn increment(&mut self, name: &str, by: u64) {
        *self.counters.entry(name.into()).or_insert(0) += by;
    }

    pub fn record_value(&mut self, name: &str, value: f64) {
        if let Some(h) = self.histograms.get_mut(name) {
            h.record(value);
        }
    }

    pub fn register(&mut self, histogram: Histogram) {
        self.histograms.insert(histogram.name.clone(), histogram);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_increments_buckets() {
        let mut h = Histogram::exponential("test", 1.0, 1000.0, 10);
        h.record(5.0);
        h.record(50.0);
        h.record(500.0);
        assert_eq!(h.samples, 3);
        assert!(h.mean() > 0.0);
    }

    #[test]
    fn percentile_finds_bucket() {
        let mut h = Histogram::exponential("test", 1.0, 1000.0, 10);
        for _ in 0..100 { h.record(50.0); }
        let p = h.percentile(0.5);
        assert!(p > 0.0);
    }

    #[test]
    fn counter_increments() {
        let mut m = MetricsStore::new();
        m.increment("page_loads", 1);
        m.increment("page_loads", 5);
        assert_eq!(m.counters.get("page_loads"), Some(&6));
    }

    #[test]
    fn record_value_dispatches_to_histogram() {
        let mut m = MetricsStore::new();
        m.register(Histogram::exponential("startup_ms", 1.0, 10000.0, 20));
        m.record_value("startup_ms", 500.0);
        assert_eq!(m.histograms["startup_ms"].samples, 1);
    }

    #[test]
    fn empty_histogram_mean_zero() {
        let h = Histogram::exponential("test", 1.0, 100.0, 5);
        assert_eq!(h.mean(), 0.0);
    }
}
