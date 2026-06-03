//! Async runtime - microtask + task queues + Promise scheduling.
//!
//! HTML spec: https://html.spec.whatwg.org/multipage/webappapis.html#event-loop
//! Microtasks run to completion between tasks. Promise reactions are microtasks.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskSource {
    DomManipulation,
    UserInteraction,
    Networking,
    HistoryTraversal,
    Render,
    Timer,
    PromiseJobs,
    PostMessage,
    WebSocket,
    IndexedDb,
    BluetoothHidUsb,
    PerformanceTimeline,
}

#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: u64,
    pub source: TaskSource,
    pub callback_id: u64,
    pub scheduled_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Microtask {
    pub callback_id: u64,
    pub args: Vec<u32>,        // opaque arg ids; the JS shim resolves
}

#[derive(Default)]
pub struct EventLoop {
    pub task_queues: Vec<VecDeque<ScheduledTask>>,
    pub microtask_queue: VecDeque<Microtask>,
    pub next_task_id: u64,
    /// Per-event-loop spec: re-entrant microtask flush should be suppressed.
    pub microtask_checkpoint_in_progress: bool,
}

impl EventLoop {
    pub fn new() -> Self {
        let mut e = Self::default();
        // One queue per source (simplest model; real impl per-spec).
        for _ in 0..12 { e.task_queues.push(VecDeque::new()); }
        e
    }

    pub fn enqueue_task(&mut self, source: TaskSource, callback_id: u64, now: u64) -> u64 {
        self.next_task_id += 1;
        let id = self.next_task_id;
        let q = source_to_index(source);
        if let Some(queue) = self.task_queues.get_mut(q) {
            queue.push_back(ScheduledTask { id, source, callback_id, scheduled_unix_ms: now });
        }
        id
    }

    pub fn enqueue_microtask(&mut self, callback_id: u64) {
        self.microtask_queue.push_back(Microtask { callback_id, args: Vec::new() });
    }

    /// Pop next task (cycles through queues by index round-robin).
    pub fn pop_task(&mut self) -> Option<ScheduledTask> {
        for q in self.task_queues.iter_mut() {
            if let Some(t) = q.pop_front() {
                return Some(t);
            }
        }
        None
    }

    /// Drain microtask queue until empty. Returns callbacks to invoke in order.
    pub fn microtask_checkpoint(&mut self) -> Vec<u64> {
        if self.microtask_checkpoint_in_progress { return Vec::new(); }
        self.microtask_checkpoint_in_progress = true;
        let mut out = Vec::new();
        // Drain in order, including microtasks queued during invocation.
        while let Some(mt) = self.microtask_queue.pop_front() {
            out.push(mt.callback_id);
        }
        self.microtask_checkpoint_in_progress = false;
        out
    }
}

fn source_to_index(source: TaskSource) -> usize {
    match source {
        TaskSource::DomManipulation => 0,
        TaskSource::UserInteraction => 1,
        TaskSource::Networking => 2,
        TaskSource::HistoryTraversal => 3,
        TaskSource::Render => 4,
        TaskSource::Timer => 5,
        TaskSource::PromiseJobs => 6,
        TaskSource::PostMessage => 7,
        TaskSource::WebSocket => 8,
        TaskSource::IndexedDb => 9,
        TaskSource::BluetoothHidUsb => 10,
        TaskSource::PerformanceTimeline => 11,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_task_increments_id() {
        let mut e = EventLoop::new();
        let a = e.enqueue_task(TaskSource::Networking, 1, 0);
        let b = e.enqueue_task(TaskSource::Timer, 2, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn pop_task_returns_in_priority_order() {
        let mut e = EventLoop::new();
        e.enqueue_task(TaskSource::Timer, 1, 0);              // queue 5
        e.enqueue_task(TaskSource::DomManipulation, 2, 0);    // queue 0
        let first = e.pop_task().unwrap();
        // DOM (queue 0) comes first
        assert_eq!(first.callback_id, 2);
    }

    #[test]
    fn microtask_drains_in_fifo() {
        let mut e = EventLoop::new();
        e.enqueue_microtask(1);
        e.enqueue_microtask(2);
        e.enqueue_microtask(3);
        assert_eq!(e.microtask_checkpoint(), vec![1, 2, 3]);
    }

    #[test]
    fn microtask_reentrancy_blocked() {
        let mut e = EventLoop::new();
        e.microtask_checkpoint_in_progress = true;
        e.enqueue_microtask(1);
        assert!(e.microtask_checkpoint().is_empty());
    }

    #[test]
    fn empty_queue_pops_none() {
        let mut e = EventLoop::new();
        assert!(e.pop_task().is_none());
    }
}
