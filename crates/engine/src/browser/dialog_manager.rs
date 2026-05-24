//! window.alert/confirm/prompt - modal dialog queue.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DialogKind {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

#[derive(Debug, Clone)]
pub struct DialogRequest {
    pub id: u64,
    pub kind: DialogKind,
    pub message: String,
    pub default_value: Option<String>,    // for Prompt
    pub source_origin: String,
}

#[derive(Debug, Clone)]
pub struct DialogResponse {
    pub accepted: bool,
    pub text: Option<String>,
}

#[derive(Default)]
pub struct DialogManager {
    pub queue: VecDeque<DialogRequest>,
    pub current: Option<DialogRequest>,
    pub next_id: u64,
    /// Per-origin throttling - block spammy alert loops.
    pub suppress_origins: std::collections::HashSet<String>,
}

impl DialogManager {
    pub fn new() -> Self { Self::default() }

    pub fn enqueue(&mut self, kind: DialogKind, message: &str, default_value: Option<&str>, origin: &str) -> Option<u64> {
        if self.suppress_origins.contains(origin) { return None; }
        self.next_id += 1;
        let id = self.next_id;
        let req = DialogRequest {
            id, kind,
            message: message.into(),
            default_value: default_value.map(|s| s.into()),
            source_origin: origin.into(),
        };
        if self.current.is_none() {
            self.current = Some(req);
        } else {
            self.queue.push_back(req);
        }
        Some(id)
    }

    pub fn resolve(&mut self, response: DialogResponse) -> Option<(DialogRequest, DialogResponse)> {
        let req = self.current.take()?;
        // Pull next.
        self.current = self.queue.pop_front();
        Some((req, response))
    }

    pub fn suppress_origin(&mut self, origin: &str) {
        self.suppress_origins.insert(origin.into());
    }

    pub fn pending_count(&self) -> usize {
        self.queue.len() + if self.current.is_some() { 1 } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_and_resolve() {
        let mut m = DialogManager::new();
        m.enqueue(DialogKind::Alert, "hello", None, "x.com");
        let (req, _) = m.resolve(DialogResponse { accepted: true, text: None }).unwrap();
        assert_eq!(req.message, "hello");
    }

    #[test]
    fn second_dialog_queued() {
        let mut m = DialogManager::new();
        m.enqueue(DialogKind::Alert, "a", None, "x.com");
        m.enqueue(DialogKind::Alert, "b", None, "x.com");
        assert_eq!(m.pending_count(), 2);
        m.resolve(DialogResponse { accepted: true, text: None });
        assert_eq!(m.pending_count(), 1);
    }

    #[test]
    fn suppress_blocks() {
        let mut m = DialogManager::new();
        m.suppress_origin("x.com");
        assert!(m.enqueue(DialogKind::Alert, "ignored", None, "x.com").is_none());
    }

    #[test]
    fn prompt_carries_default() {
        let mut m = DialogManager::new();
        m.enqueue(DialogKind::Prompt, "name?", Some("Alice"), "x.com");
        let cur = m.current.as_ref().unwrap();
        assert_eq!(cur.default_value.as_deref(), Some("Alice"));
    }
}
