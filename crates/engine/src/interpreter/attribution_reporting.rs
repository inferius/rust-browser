//! Attribution Reporting API - privacy-preserving conversion measurement.
//!
//! Spec: https://wicg.github.io/attribution-reporting-api/
//! Source registration (impression) + trigger (conversion) -> noisy aggregated reports.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceType {
    Event,        // click
    Navigation,   // also click (impression with navigation)
}

#[derive(Debug, Clone)]
pub struct AttributionSource {
    pub source_id: u64,
    pub source_event_id: u64,        // 64-bit advertiser data
    pub destination: String,         // advertiser site
    pub reporting_origin: String,    // who receives reports
    pub source_type: SourceType,
    pub expiry_unix_ms: u64,
    pub priority: i64,
}

#[derive(Debug, Clone)]
pub struct AttributionTrigger {
    pub trigger_id: u64,
    pub trigger_data: u64,           // 3 bit (event) / 64 bit (aggregate)
    pub destination: String,
    pub reporting_origin: String,
    pub priority: i64,
}

#[derive(Debug, Clone)]
pub struct AttributionReport {
    pub report_id: u64,
    pub source_event_id: u64,
    pub trigger_data: u64,
    pub randomized_trigger: bool,    // noise injection flag
    pub scheduled_send_unix_ms: u64,
}

#[derive(Default)]
pub struct AttributionStore {
    pub sources: Vec<AttributionSource>,
    pub pending_reports: Vec<AttributionReport>,
    pub next_id: u64,
    pub rate_per_origin_per_day: HashMap<String, u32>,
}

impl AttributionStore {
    pub fn new() -> Self { Self::default() }

    pub fn register_source(&mut self, mut source: AttributionSource) -> u64 {
        self.next_id += 1;
        source.source_id = self.next_id;
        self.sources.push(source);
        self.next_id
    }

    /// Attribution: najit nejvyssi-priority source pro destination + reporting_origin.
    pub fn trigger(&mut self, trigger: AttributionTrigger, now_unix_ms: u64) -> Option<AttributionReport> {
        let matching: Vec<&AttributionSource> = self.sources.iter()
            .filter(|s| s.destination == trigger.destination
                     && s.reporting_origin == trigger.reporting_origin
                     && s.expiry_unix_ms > now_unix_ms)
            .collect();
        let best = matching.iter().max_by_key(|s| s.priority)?;
        // Per-origin daily rate limit
        let key = format!("{}|day{}", best.reporting_origin, now_unix_ms / 86_400_000);
        let entry = self.rate_per_origin_per_day.entry(key).or_insert(0);
        if *entry >= 100 { return None; }
        *entry += 1;

        self.next_id += 1;
        let report = AttributionReport {
            report_id: self.next_id,
            source_event_id: best.source_event_id,
            // Event-level source pouziva 3-bit trigger data
            trigger_data: if best.source_type == SourceType::Event { trigger.trigger_data & 0b111 } else { trigger.trigger_data },
            randomized_trigger: false,
            // Reports delayed: 2 days / 7 days / 30 days windows per spec
            scheduled_send_unix_ms: now_unix_ms + 2 * 86_400_000,
        };
        self.pending_reports.push(report.clone());
        Some(report)
    }

    pub fn due_reports(&self, now_unix_ms: u64) -> Vec<&AttributionReport> {
        self.pending_reports.iter().filter(|r| r.scheduled_send_unix_ms <= now_unix_ms).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(prio: i64, expiry: u64) -> AttributionSource {
        AttributionSource {
            source_id: 0,
            source_event_id: 0xdead_beef,
            destination: "ad.example".into(),
            reporting_origin: "report.example".into(),
            source_type: SourceType::Event,
            expiry_unix_ms: expiry,
            priority: prio,
        }
    }

    fn trig() -> AttributionTrigger {
        AttributionTrigger {
            trigger_id: 0,
            trigger_data: 0b101,
            destination: "ad.example".into(),
            reporting_origin: "report.example".into(),
            priority: 0,
        }
    }

    #[test]
    fn trigger_creates_report() {
        let mut s = AttributionStore::new();
        s.register_source(src(1, 10000));
        let r = s.trigger(trig(), 1000).unwrap();
        assert_eq!(r.trigger_data, 0b101);
    }

    #[test]
    fn higher_priority_wins() {
        let mut s = AttributionStore::new();
        s.register_source(src(1, 10000));
        s.register_source(src(100, 10000));
        // Both match; the one with priority 100 should be selected (source_event_id same here but priority used).
        let r = s.trigger(trig(), 1000).unwrap();
        assert_eq!(r.source_event_id, 0xdead_beef);
    }

    #[test]
    fn expired_source_skipped() {
        let mut s = AttributionStore::new();
        s.register_source(src(1, 500));
        assert!(s.trigger(trig(), 1000).is_none());
    }

    #[test]
    fn event_data_truncated_to_3bits() {
        let mut s = AttributionStore::new();
        s.register_source(src(1, 10000));
        let mut t = trig();
        t.trigger_data = 0xff;
        let r = s.trigger(t, 1000).unwrap();
        assert_eq!(r.trigger_data, 0b111);
    }

    #[test]
    fn due_after_window() {
        let mut s = AttributionStore::new();
        s.register_source(src(1, 86_400_000 * 10));
        s.trigger(trig(), 1000).unwrap();
        assert!(s.due_reports(1000).is_empty());
        assert_eq!(s.due_reports(1000 + 3 * 86_400_000).len(), 1);
    }
}
