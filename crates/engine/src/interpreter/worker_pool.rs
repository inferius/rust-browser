//! Worker thread pool - Dedicated/Shared/Service workers + AudioWorklets.
//!
//! Spec: https://html.spec.whatwg.org/multipage/workers.html
//! Each Worker has its own thread + JS realm; message passing via postMessage.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerKind {
    Dedicated,
    Shared,
    Service,
    AudioWorklet,
    PaintWorklet,
    LayoutWorklet,
    AnimationWorklet,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerState {
    Spawning,
    Running,
    Terminating,
    Terminated,
}

#[derive(Debug, Clone)]
pub struct WorkerHandle {
    pub id: u64,
    pub kind: WorkerKind,
    pub state: WorkerState,
    pub script_url: String,
    pub credentials: String,           // "omit" | "same-origin" | "include"
    pub type_module: bool,             // type: "module" vs "classic"
    pub name: String,
    pub message_queue_in: Vec<Vec<u8>>, // host -> worker
    pub message_queue_out: Vec<Vec<u8>>, // worker -> host
    pub bytes_pending: u64,
}

#[derive(Default)]
pub struct WorkerPool {
    pub workers: HashMap<u64, WorkerHandle>,
    pub next_id: u64,
    pub max_workers: usize,
}

impl WorkerPool {
    pub fn new() -> Self {
        Self { max_workers: 32, ..Self::default() }
    }

    pub fn spawn(&mut self, kind: WorkerKind, script_url: &str, name: &str) -> Result<u64, String> {
        if self.workers.len() >= self.max_workers {
            return Err("worker pool full".into());
        }
        self.next_id += 1;
        let id = self.next_id;
        self.workers.insert(id, WorkerHandle {
            id, kind,
            state: WorkerState::Spawning,
            script_url: script_url.into(),
            credentials: "same-origin".into(),
            type_module: false,
            name: name.into(),
            message_queue_in: Vec::new(),
            message_queue_out: Vec::new(),
            bytes_pending: 0,
        });
        Ok(id)
    }

    pub fn mark_running(&mut self, id: u64) {
        if let Some(w) = self.workers.get_mut(&id) {
            w.state = WorkerState::Running;
        }
    }

    pub fn terminate(&mut self, id: u64) {
        if let Some(w) = self.workers.get_mut(&id) {
            w.state = WorkerState::Terminating;
            w.message_queue_in.clear();
            w.message_queue_out.clear();
        }
    }

    pub fn post_to_worker(&mut self, id: u64, payload: Vec<u8>) -> Result<(), String> {
        let w = self.workers.get_mut(&id).ok_or("worker not found")?;
        if w.state == WorkerState::Terminated || w.state == WorkerState::Terminating {
            return Err("worker terminated".into());
        }
        w.bytes_pending += payload.len() as u64;
        w.message_queue_in.push(payload);
        Ok(())
    }

    pub fn post_to_host(&mut self, id: u64, payload: Vec<u8>) -> Result<(), String> {
        let w = self.workers.get_mut(&id).ok_or("worker not found")?;
        w.message_queue_out.push(payload);
        Ok(())
    }

    pub fn drain_to_host(&mut self, id: u64) -> Vec<Vec<u8>> {
        self.workers.get_mut(&id).map(|w| std::mem::take(&mut w.message_queue_out)).unwrap_or_default()
    }

    pub fn drain_to_worker(&mut self, id: u64) -> Vec<Vec<u8>> {
        self.workers.get_mut(&id).map(|w| std::mem::take(&mut w.message_queue_in)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_returns_id() {
        let mut p = WorkerPool::new();
        let id = p.spawn(WorkerKind::Dedicated, "/w.js", "Worker1").unwrap();
        assert!(p.workers.contains_key(&id));
    }

    #[test]
    fn spawn_rejects_when_full() {
        let mut p = WorkerPool::new();
        p.max_workers = 2;
        p.spawn(WorkerKind::Dedicated, "/a", "").unwrap();
        p.spawn(WorkerKind::Dedicated, "/b", "").unwrap();
        assert!(p.spawn(WorkerKind::Dedicated, "/c", "").is_err());
    }

    #[test]
    fn post_message_queues() {
        let mut p = WorkerPool::new();
        let id = p.spawn(WorkerKind::Dedicated, "/w.js", "").unwrap();
        p.post_to_worker(id, vec![1, 2, 3]).unwrap();
        let drained = p.drain_to_worker(id);
        assert_eq!(drained, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn terminate_blocks_send() {
        let mut p = WorkerPool::new();
        let id = p.spawn(WorkerKind::Dedicated, "/w.js", "").unwrap();
        p.terminate(id);
        assert!(p.post_to_worker(id, vec![1]).is_err());
    }

    #[test]
    fn worker_to_host_queue() {
        let mut p = WorkerPool::new();
        let id = p.spawn(WorkerKind::Dedicated, "/w.js", "").unwrap();
        p.post_to_host(id, vec![9]).unwrap();
        assert_eq!(p.drain_to_host(id), vec![vec![9]]);
    }
}
