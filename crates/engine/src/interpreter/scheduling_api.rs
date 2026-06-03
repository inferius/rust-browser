//! Scheduling APIs - scheduler.postTask, isInputPending, yield.
//!
//! Spec: https://wicg.github.io/scheduling-apis/
//! Priority-based task scheduling pres event loop.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    UserBlocking = 0,   // user-visible, must complete v frame
    UserVisible = 1,    // visible nepost-completion
    Background = 2,     // not visible
}

impl TaskPriority {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user-blocking" => Self::UserBlocking,
            "background" => Self::Background,
            _ => Self::UserVisible,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: u64,
    pub priority: TaskPriority,
    pub callback_id: usize,
}

#[derive(Default)]
pub struct Scheduler {
    pub queues: [VecDeque<Task>; 3],   // per priority
    pub next_id: u64,
    pub input_pending: bool,
}

impl Scheduler {
    pub fn new() -> Self { Self::default() }

    pub fn post_task(&mut self, priority: TaskPriority, callback_id: usize) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let t = Task { id, priority, callback_id };
        let idx = priority as usize;
        self.queues[idx].push_back(t);
        id
    }

    /// Pop next task per priority order (UserBlocking > UserVisible > Background).
    pub fn next(&mut self) -> Option<Task> {
        for q in self.queues.iter_mut() {
            if let Some(t) = q.pop_front() { return Some(t); }
        }
        None
    }

    pub fn is_input_pending(&self) -> bool {
        self.input_pending
    }

    pub fn set_input_pending(&mut self, pending: bool) {
        self.input_pending = pending;
    }

    /// `scheduler.yield()` - signal yield k browser. Foundation: clear input flag.
    pub fn yield_to_browser(&mut self) {
        self.input_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_returns_unique_ids() {
        let mut s = Scheduler::new();
        let a = s.post_task(TaskPriority::UserVisible, 1);
        let b = s.post_task(TaskPriority::UserVisible, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn priority_order() {
        let mut s = Scheduler::new();
        s.post_task(TaskPriority::Background, 1);
        s.post_task(TaskPriority::UserBlocking, 2);
        s.post_task(TaskPriority::UserVisible, 3);
        // First UserBlocking (=0).
        assert_eq!(s.next().unwrap().callback_id, 2);
        assert_eq!(s.next().unwrap().callback_id, 3);
        assert_eq!(s.next().unwrap().callback_id, 1);
    }

    #[test]
    fn input_pending_flag() {
        let mut s = Scheduler::new();
        assert!(!s.is_input_pending());
        s.set_input_pending(true);
        assert!(s.is_input_pending());
        s.yield_to_browser();
        assert!(!s.is_input_pending());
    }

    #[test]
    fn parse_priority() {
        assert_eq!(TaskPriority::parse("user-blocking"), TaskPriority::UserBlocking);
        assert_eq!(TaskPriority::parse("background"), TaskPriority::Background);
        assert_eq!(TaskPriority::parse("unknown"), TaskPriority::UserVisible);
    }
}
