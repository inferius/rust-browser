//! Web Locks API - exclusive / shared locks pres origin.
//!
//! Spec: https://w3c.github.io/web-locks/
//!
//! navigator.locks.request('name', callback) - exclusive default. callback
//! prijme lock objekt, lock auto-release na callback resolution.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockMode {
    Exclusive,
    Shared,
}

#[derive(Debug)]
pub struct Lock {
    pub name: String,
    pub mode: LockMode,
    pub client_id: u64,
}

pub struct LockManager {
    /// name -> (active holders, waiting queue).
    pub locks: HashMap<String, LockState>,
    pub next_client_id: u64,
}

#[derive(Debug)]
pub struct LockState {
    pub holders: Vec<Lock>,
    pub queue: Vec<PendingRequest>,
}

#[derive(Debug)]
pub struct PendingRequest {
    pub name: String,
    pub mode: LockMode,
    pub client_id: u64,
}

impl Default for LockManager {
    fn default() -> Self {
        Self { locks: HashMap::new(), next_client_id: 1 }
    }
}

impl LockManager {
    pub fn new() -> Self { Self::default() }

    /// Pokus o lock - vraci Some(Lock) pokud immediate, None pri queued.
    pub fn request(&mut self, name: &str, mode: LockMode) -> Option<Lock> {
        self.next_client_id += 1;
        let cid = self.next_client_id;
        let state = self.locks.entry(name.into()).or_insert(LockState {
            holders: Vec::new(),
            queue: Vec::new(),
        });
        // Lze granular?
        let can_grant = match mode {
            LockMode::Exclusive => state.holders.is_empty(),
            LockMode::Shared => state.holders.iter().all(|h| h.mode == LockMode::Shared),
        };
        if can_grant {
            let lock = Lock { name: name.into(), mode, client_id: cid };
            state.holders.push(Lock { name: lock.name.clone(), mode, client_id: cid });
            Some(lock)
        } else {
            state.queue.push(PendingRequest {
                name: name.into(), mode, client_id: cid,
            });
            None
        }
    }

    /// Release lock - grant queued kde to lze.
    pub fn release(&mut self, name: &str, client_id: u64) {
        let state = match self.locks.get_mut(name) { Some(s) => s, None => return };
        state.holders.retain(|h| h.client_id != client_id);
        // Pop queue front kdyz mode compatible.
        while let Some(req) = state.queue.first() {
            let can = match req.mode {
                LockMode::Exclusive => state.holders.is_empty(),
                LockMode::Shared => state.holders.iter().all(|h| h.mode == LockMode::Shared),
            };
            if !can { break; }
            let req = state.queue.remove(0);
            state.holders.push(Lock { name: req.name, mode: req.mode, client_id: req.client_id });
        }
    }

    pub fn query(&self) -> Vec<&Lock> {
        self.locks.values().flat_map(|s| s.holders.iter()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_lock_blocks_second() {
        let mut m = LockManager::new();
        let l1 = m.request("resource", LockMode::Exclusive);
        let l2 = m.request("resource", LockMode::Exclusive);
        assert!(l1.is_some());
        assert!(l2.is_none()); // queued
    }

    #[test]
    fn shared_locks_concurrent() {
        let mut m = LockManager::new();
        assert!(m.request("res", LockMode::Shared).is_some());
        assert!(m.request("res", LockMode::Shared).is_some());
        assert!(m.request("res", LockMode::Shared).is_some());
    }

    #[test]
    fn shared_blocks_exclusive() {
        let mut m = LockManager::new();
        m.request("r", LockMode::Shared);
        assert!(m.request("r", LockMode::Exclusive).is_none());
    }

    #[test]
    fn release_grants_queued() {
        let mut m = LockManager::new();
        let l1 = m.request("r", LockMode::Exclusive).unwrap();
        let _l2 = m.request("r", LockMode::Exclusive); // queued
        m.release("r", l1.client_id);
        let q = m.query();
        // After release, queue grants l2 -> 1 holder still.
        assert_eq!(q.len(), 1);
    }
}
