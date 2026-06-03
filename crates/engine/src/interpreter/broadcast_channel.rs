//! BroadcastChannel - same-origin cross-tab pub/sub.
//!
//! Spec: https://html.spec.whatwg.org/multipage/web-messaging.html#broadcasting-to-other-browsing-contexts
//!
//! Per-channel name + per-origin. Pri postMessage broadcast vsem ostatnim
//! BroadcastChannel se shodnym name/origin (krome senderu).

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub enum BroadcastMessage {
    Text(String),
    Json(String),
}

/// Channel subscriber - callback queue.
pub struct BroadcastSubscriber {
    pub id: u64,
    pub queue: RefCell<Vec<BroadcastMessage>>,
}

impl BroadcastSubscriber {
    pub fn new(id: u64) -> Self {
        Self { id, queue: RefCell::new(Vec::new()) }
    }

    pub fn drain(&self) -> Vec<BroadcastMessage> {
        std::mem::take(&mut *self.queue.borrow_mut())
    }
}

/// Per-origin per-channel-name registry subscribers.
#[derive(Default)]
pub struct BroadcastRegistry {
    /// (origin, channel_name) -> Vec<subscriber>
    pub channels: HashMap<(String, String), Vec<Rc<BroadcastSubscriber>>>,
    pub next_id: u64,
}

impl BroadcastRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn subscribe(&mut self, origin: &str, channel: &str) -> Rc<BroadcastSubscriber> {
        self.next_id += 1;
        let sub = Rc::new(BroadcastSubscriber::new(self.next_id));
        let key = (origin.to_string(), channel.to_string());
        self.channels.entry(key).or_default().push(Rc::clone(&sub));
        sub
    }

    pub fn unsubscribe(&mut self, origin: &str, channel: &str, id: u64) {
        let key = (origin.to_string(), channel.to_string());
        if let Some(subs) = self.channels.get_mut(&key) {
            subs.retain(|s| s.id != id);
        }
    }

    /// Post message - vsechny subs s shodnym (origin, channel) KROME sender_id.
    pub fn post(&self, origin: &str, channel: &str, sender_id: u64, msg: BroadcastMessage) {
        let key = (origin.to_string(), channel.to_string());
        if let Some(subs) = self.channels.get(&key) {
            for s in subs {
                if s.id == sender_id { continue; }
                s.queue.borrow_mut().push(msg.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_and_post() {
        let mut r = BroadcastRegistry::new();
        let a = r.subscribe("https://example.com", "chat");
        let b = r.subscribe("https://example.com", "chat");
        r.post("https://example.com", "chat", a.id, BroadcastMessage::Text("hi".into()));
        // b dostal, a ne.
        assert_eq!(b.drain().len(), 1);
        assert_eq!(a.drain().len(), 0);
    }

    #[test]
    fn cross_origin_isolation() {
        let mut r = BroadcastRegistry::new();
        let a = r.subscribe("https://a.com", "chat");
        let b = r.subscribe("https://b.com", "chat");
        r.post("https://a.com", "chat", a.id, BroadcastMessage::Text("hi".into()));
        assert_eq!(b.drain().len(), 0); // jine origin
    }

    #[test]
    fn channel_isolation() {
        let mut r = BroadcastRegistry::new();
        let a = r.subscribe("https://x.com", "chat");
        let b = r.subscribe("https://x.com", "notify");
        r.post("https://x.com", "chat", a.id, BroadcastMessage::Text("hi".into()));
        assert_eq!(b.drain().len(), 0); // jiny channel
    }

    #[test]
    fn unsubscribe_stops_delivery() {
        let mut r = BroadcastRegistry::new();
        let a = r.subscribe("https://x.com", "ch");
        let b = r.subscribe("https://x.com", "ch");
        r.unsubscribe("https://x.com", "ch", b.id);
        r.post("https://x.com", "ch", a.id, BroadcastMessage::Text("hi".into()));
        assert_eq!(b.drain().len(), 0);
    }
}
