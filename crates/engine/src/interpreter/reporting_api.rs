//! Reporting API - CSP violations, deprecations, crashes -> report endpoints.
//!
//! Spec: https://w3c.github.io/reporting/
//! Pres Reporting-Endpoints header server-defined endpoint. Reports queued
//! a posted in batches.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Report {
    pub report_type: String,        // "csp-violation", "deprecation", "intervention", "crash"
    pub url: String,                // document URL
    pub age_ms: u64,
    pub user_agent: String,
    pub body: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ReportingEndpoint {
    pub name: String,
    pub url: String,
}

#[derive(Default)]
pub struct ReportingService {
    pub endpoints: HashMap<String, ReportingEndpoint>,
    pub queue: Vec<Report>,
    pub max_queue: usize,
}

impl ReportingService {
    pub fn new() -> Self {
        Self { endpoints: HashMap::new(), queue: Vec::new(), max_queue: 100 }
    }

    /// Parse `Reporting-Endpoints: name=url, name2=url2`.
    pub fn parse_header(&mut self, header: &str) {
        for entry in header.split(',') {
            let entry = entry.trim();
            if let Some(eq) = entry.find('=') {
                let name = entry[..eq].trim().to_string();
                let url = entry[eq+1..].trim().trim_matches('"').to_string();
                self.endpoints.insert(name.clone(), ReportingEndpoint { name, url });
            }
        }
    }

    pub fn queue_report(&mut self, report: Report) {
        self.queue.push(report);
        while self.queue.len() > self.max_queue {
            self.queue.remove(0);
        }
    }

    /// Drain queue + return reports grouped by endpoint. Real impl POST k endpoint.
    pub fn drain(&mut self) -> Vec<Report> {
        std::mem::take(&mut self.queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_endpoints_header() {
        let mut s = ReportingService::new();
        s.parse_header(r#"default="https://x.com/report", csp-endpoint="https://x.com/csp""#);
        assert_eq!(s.endpoints.len(), 2);
        assert_eq!(s.endpoints.get("default").unwrap().url, "https://x.com/report");
    }

    #[test]
    fn queue_and_drain() {
        let mut s = ReportingService::new();
        s.queue_report(Report {
            report_type: "csp-violation".into(),
            url: "https://x.com/".into(),
            age_ms: 0, user_agent: "RWE".into(),
            body: HashMap::new(),
        });
        let drained = s.drain();
        assert_eq!(drained.len(), 1);
        assert!(s.queue.is_empty());
    }

    #[test]
    fn queue_capped() {
        let mut s = ReportingService::new();
        s.max_queue = 3;
        for i in 0..10 {
            s.queue_report(Report {
                report_type: format!("type{}", i),
                url: "/".into(), age_ms: 0,
                user_agent: "".into(), body: HashMap::new(),
            });
        }
        assert_eq!(s.queue.len(), 3);
    }
}
