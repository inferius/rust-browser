//! Service Worker foundation - per-origin registry + lifecycle.
//!
//! SW = background JS proces ktery muze intercept fetch requests, kontrolovat
//! cache, push notifications. Per origin registered (`navigator.serviceWorker.register`).
//!
//! Lifecycle: installing -> installed -> activating -> activated -> redundant.
//!
//! Inspired by Chromium `content/browser/service_worker/`.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwState {
    Installing,
    Installed,
    Activating,
    Activated,
    Redundant,
}

#[derive(Debug, Clone)]
pub struct ServiceWorkerRegistration {
    pub scope: String,        // URL prefix - SW handles all requests under
    pub script_url: String,   // SW script source
    pub state: SwState,
    /// SW JS callbacks - install, activate, fetch handlers.
    /// Stored as opaque ids - real impl ma per-SW JS Interpreter.
    pub install_handler: Option<usize>,
    pub activate_handler: Option<usize>,
    pub fetch_handler: Option<usize>,
}

#[derive(Default)]
pub struct ServiceWorkerRegistry {
    /// Per-origin registrace - origin -> Vec<Registration>.
    pub by_origin: HashMap<String, Vec<Rc<RefCell<ServiceWorkerRegistration>>>>,
}

impl ServiceWorkerRegistry {
    pub fn new() -> Self { Self::default() }

    /// Register novy SW pro origin + scope.
    pub fn register(&mut self, origin: &str, scope: &str, script_url: &str)
        -> Rc<RefCell<ServiceWorkerRegistration>>
    {
        let reg = Rc::new(RefCell::new(ServiceWorkerRegistration {
            scope: scope.to_string(),
            script_url: script_url.to_string(),
            state: SwState::Installing,
            install_handler: None,
            activate_handler: None,
            fetch_handler: None,
        }));
        self.by_origin.entry(origin.to_string()).or_default().push(Rc::clone(&reg));
        reg
    }

    /// Najdi SW ktery match URL (= URL prefix s registration scope).
    pub fn match_url(&self, url: &str) -> Option<Rc<RefCell<ServiceWorkerRegistration>>> {
        let origin = origin_of(url);
        let regs = self.by_origin.get(&origin)?;
        for r in regs {
            let reg = r.borrow();
            if reg.state == SwState::Activated && url.starts_with(&reg.scope) {
                return Some(Rc::clone(r));
            }
        }
        None
    }

    /// Unregister SW.
    pub fn unregister(&mut self, origin: &str, scope: &str) -> bool {
        let regs = match self.by_origin.get_mut(origin) { Some(r) => r, None => return false };
        let before = regs.len();
        regs.retain(|r| r.borrow().scope != scope);
        regs.len() < before
    }

    /// Lifecycle transition. Volat z install/activate kompletni.
    pub fn transition(reg: &Rc<RefCell<ServiceWorkerRegistration>>, new_state: SwState) {
        reg.borrow_mut().state = new_state;
    }
}

fn origin_of(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) {
        let host = rest.split('/').next().unwrap_or("");
        let scheme = if url.starts_with("https:") { "https" } else { "http" };
        format!("{}://{}", scheme, host)
    } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_match() {
        let mut reg = ServiceWorkerRegistry::new();
        let r = reg.register("https://example.com", "https://example.com/app/", "https://example.com/sw.js");
        ServiceWorkerRegistry::transition(&r, SwState::Activated);
        let matched = reg.match_url("https://example.com/app/page");
        assert!(matched.is_some());
    }

    #[test]
    fn no_match_different_origin() {
        let mut reg = ServiceWorkerRegistry::new();
        let r = reg.register("https://example.com", "https://example.com/", "/sw.js");
        ServiceWorkerRegistry::transition(&r, SwState::Activated);
        assert!(reg.match_url("https://evil.com/x").is_none());
    }

    #[test]
    fn no_match_non_activated() {
        let mut reg = ServiceWorkerRegistry::new();
        let _r = reg.register("https://example.com", "https://example.com/", "/sw.js");
        // state Installing - ne aktivni jeste.
        assert!(reg.match_url("https://example.com/x").is_none());
    }

    #[test]
    fn unregister_removes() {
        let mut reg = ServiceWorkerRegistry::new();
        reg.register("https://example.com", "https://example.com/", "/sw.js");
        assert!(reg.unregister("https://example.com", "https://example.com/"));
        assert!(reg.match_url("https://example.com/x").is_none());
    }

    #[test]
    fn lifecycle_transition() {
        let mut reg = ServiceWorkerRegistry::new();
        let r = reg.register("https://x.com", "https://x.com/", "/sw.js");
        assert_eq!(r.borrow().state, SwState::Installing);
        ServiceWorkerRegistry::transition(&r, SwState::Installed);
        assert_eq!(r.borrow().state, SwState::Installed);
        ServiceWorkerRegistry::transition(&r, SwState::Activated);
        assert_eq!(r.borrow().state, SwState::Activated);
    }
}
