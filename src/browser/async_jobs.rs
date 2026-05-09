//! Async background jobs - foundation pro non-blocking IO operations.
//!
//! Misto plneho Arc<Mutex> rework Interpreter (30+ souboru), tohle pridava
//! praktickou cestu pro async work: thread spawn + mpsc channel + poll v
//! render loop. Pattern uz pouziva fetch (interpreter::PendingFetch),
//! tady je generalizovany framework pro dalsi APIs:
//! - File IO (read/write)
//! - Image lazy load (atlas warmup pri scroll)
//! - Worker postMessage (mam, ale lze sjednotit)
//! - WebSocket events (mam)
//!
//! AsyncJob trait + AsyncJobsRegistry: per-frame poll pro completed jobs.
//! Result handler = closure ktery se vola s vysledkem v main thread context
//! (kde mame pristup k Interpreter Rc<RefCell>).

use std::sync::mpsc;

/// Trait pro async job - poll() pripravi prip. vysledek + apply na state.
pub trait AsyncJob {
    /// Try receive vysledek bez blockovani. Vraci true pokud job dokoncen
    /// (caller pak job odebere z registry).
    fn poll(&mut self) -> bool;
}

/// Registry async jobs - drain v render loop.
pub struct AsyncJobsRegistry {
    pub jobs: Vec<Box<dyn AsyncJob>>,
}

impl AsyncJobsRegistry {
    pub fn new() -> Self {
        Self { jobs: Vec::new() }
    }

    /// Push novy job do registry.
    pub fn spawn(&mut self, job: Box<dyn AsyncJob>) {
        self.jobs.push(job);
    }

    /// Drain dokoncene jobs - kazdy poll() vraci true = remove. Volat per frame.
    pub fn drain(&mut self) {
        let mut i = 0;
        while i < self.jobs.len() {
            if self.jobs[i].poll() {
                self.jobs.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Pocet aktivnich jobs (pro debugging / status indicator).
    pub fn active_count(&self) -> usize {
        self.jobs.len()
    }
}

impl Default for AsyncJobsRegistry {
    fn default() -> Self { Self::new() }
}

/// Builder helper - spawn closure thread + mpsc kanal, vrati job s Receiver.
/// Caller poskytne complete callback ktery se vola pri try_recv Ok.
pub struct ThreadJob<T: Send + 'static> {
    pub rx: mpsc::Receiver<T>,
    pub on_complete: Option<Box<dyn FnOnce(T)>>,
    /// True kdyz dokoncen + on_complete invoked.
    completed: bool,
}

impl<T: Send + 'static> ThreadJob<T> {
    /// Spawn thread, run worker, send vysledek pres tx. Caller dostane Self.
    pub fn spawn<F, C>(worker: F, on_complete: C) -> Self
    where
        F: FnOnce() -> T + Send + 'static,
        C: FnOnce(T) + 'static,
    {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(worker());
        });
        ThreadJob {
            rx,
            on_complete: Some(Box::new(on_complete)),
            completed: false,
        }
    }
}

impl<T: Send + 'static> AsyncJob for ThreadJob<T> {
    fn poll(&mut self) -> bool {
        if self.completed { return true; }
        match self.rx.try_recv() {
            Ok(value) => {
                if let Some(cb) = self.on_complete.take() {
                    cb(value);
                }
                self.completed = true;
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                // Thread panic ci early drop - mark done.
                self.completed = true;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn thread_job_completes() {
        let result = Arc::new(Mutex::new(None::<i32>));
        let result_clone = Arc::clone(&result);
        let mut reg = AsyncJobsRegistry::new();
        reg.spawn(Box::new(ThreadJob::spawn(
            || 42,
            move |v| { *result_clone.lock().unwrap() = Some(v); },
        )));
        // Pollovani v loop dokud nedokonceno (max 1000 iteraci).
        for _ in 0..1000 {
            reg.drain();
            if reg.active_count() == 0 { break; }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(reg.active_count(), 0);
        assert_eq!(*result.lock().unwrap(), Some(42));
    }

    #[test]
    fn empty_registry_drain_noop() {
        let mut reg = AsyncJobsRegistry::new();
        reg.drain();
        assert_eq!(reg.active_count(), 0);
    }
}
