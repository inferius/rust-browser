//! V8 Inspector Protocol - Runtime + Debugger + Profiler domains.
//!
//! Spec: https://chromedevtools.github.io/devtools-protocol/v8/

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InspectorDomain {
    Runtime,
    Debugger,
    Profiler,
    Console,
    HeapProfiler,
    Schema,
}

#[derive(Debug, Clone)]
pub struct InspectorMessage {
    pub id: u64,
    pub method: String,
    pub params: String,         // JSON-encoded
}

#[derive(Debug, Clone)]
pub struct InspectorResponse {
    pub id: u64,
    pub result: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InspectorEvent {
    pub method: String,
    pub params: String,
}

#[derive(Default)]
pub struct InspectorChannel {
    pub enabled_domains: Vec<InspectorDomain>,
    pub pending_responses: HashMap<u64, InspectorResponse>,
    pub event_queue: Vec<InspectorEvent>,
}

impl InspectorChannel {
    pub fn new() -> Self { Self::default() }

    pub fn enable(&mut self, domain: InspectorDomain) {
        if !self.enabled_domains.contains(&domain) {
            self.enabled_domains.push(domain);
        }
    }

    pub fn disable(&mut self, domain: InspectorDomain) {
        self.enabled_domains.retain(|d| *d != domain);
    }

    pub fn is_enabled(&self, domain: InspectorDomain) -> bool {
        self.enabled_domains.contains(&domain)
    }

    pub fn respond(&mut self, id: u64, result: &str) {
        self.pending_responses.insert(id, InspectorResponse {
            id, result: result.into(), error: None,
        });
    }

    pub fn respond_error(&mut self, id: u64, error: &str) {
        self.pending_responses.insert(id, InspectorResponse {
            id, result: String::new(), error: Some(error.into()),
        });
    }

    pub fn emit_event(&mut self, method: &str, params: &str) {
        self.event_queue.push(InspectorEvent {
            method: method.into(), params: params.into(),
        });
    }

    pub fn dispatch(&mut self, msg: &InspectorMessage) {
        // Domain.method - split first to detect domain.
        let domain_name = msg.method.split('.').next().unwrap_or("");
        let domain = match domain_name {
            "Runtime" => InspectorDomain::Runtime,
            "Debugger" => InspectorDomain::Debugger,
            "Profiler" => InspectorDomain::Profiler,
            "Console" => InspectorDomain::Console,
            "HeapProfiler" => InspectorDomain::HeapProfiler,
            "Schema" => InspectorDomain::Schema,
            _ => {
                self.respond_error(msg.id, &format!("unknown domain {}", domain_name));
                return;
            }
        };
        if !self.is_enabled(domain) {
            self.respond_error(msg.id, &format!("domain {:?} not enabled", domain));
            return;
        }
        // Stub success.
        self.respond(msg.id, "{}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_domain() {
        let mut c = InspectorChannel::new();
        c.enable(InspectorDomain::Runtime);
        assert!(c.is_enabled(InspectorDomain::Runtime));
    }

    #[test]
    fn dispatch_unknown_domain_errors() {
        let mut c = InspectorChannel::new();
        c.dispatch(&InspectorMessage { id: 1, method: "Garbage.thing".into(), params: "".into() });
        assert!(c.pending_responses[&1].error.is_some());
    }

    #[test]
    fn dispatch_disabled_domain_errors() {
        let mut c = InspectorChannel::new();
        c.dispatch(&InspectorMessage { id: 1, method: "Runtime.evaluate".into(), params: "".into() });
        assert!(c.pending_responses[&1].error.is_some());
    }

    #[test]
    fn dispatch_enabled_returns_ok() {
        let mut c = InspectorChannel::new();
        c.enable(InspectorDomain::Runtime);
        c.dispatch(&InspectorMessage { id: 1, method: "Runtime.evaluate".into(), params: "".into() });
        assert!(c.pending_responses[&1].error.is_none());
    }

    #[test]
    fn emit_event_queued() {
        let mut c = InspectorChannel::new();
        c.emit_event("Runtime.consoleAPICalled", "{}");
        assert_eq!(c.event_queue.len(), 1);
    }
}
