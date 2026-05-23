//! Notification API foundation.
//!
//! Spec: https://notifications.spec.whatwg.org/

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NotificationPermission {
    Default,
    Granted,
    Denied,
}

#[derive(Debug, Clone)]
pub struct NotificationData {
    pub title: String,
    pub body: Option<String>,
    pub icon: Option<String>,
    pub badge: Option<String>,
    pub tag: Option<String>,
    pub silent: bool,
    pub require_interaction: bool,
    pub timestamp_ms: u64,
}

#[derive(Default)]
pub struct NotificationService {
    pub permission: NotificationPermission,
    /// Active notifications queue - real impl by sent na OS notification daemon.
    pub queue: VecDeque<NotificationData>,
    pub max_queue: usize,
}

impl Default for NotificationPermission {
    fn default() -> Self { NotificationPermission::Default }
}

impl NotificationService {
    pub fn new() -> Self {
        Self {
            permission: NotificationPermission::Default,
            queue: VecDeque::new(),
            max_queue: 50,
        }
    }

    pub fn request_permission(&mut self, grant: bool) -> NotificationPermission {
        self.permission = if grant { NotificationPermission::Granted } else { NotificationPermission::Denied };
        self.permission
    }

    pub fn show(&mut self, n: NotificationData) -> bool {
        if self.permission != NotificationPermission::Granted { return false; }
        // Replace existing s same tag (= unique per tag).
        if let Some(tag) = &n.tag {
            self.queue.retain(|x| x.tag.as_deref() != Some(tag.as_str()));
        }
        self.queue.push_back(n);
        while self.queue.len() > self.max_queue {
            self.queue.pop_front();
        }
        true
    }

    pub fn close(&mut self, tag: &str) -> bool {
        let before = self.queue.len();
        self.queue.retain(|n| n.tag.as_deref() != Some(tag));
        self.queue.len() < before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(title: &str, tag: Option<&str>) -> NotificationData {
        NotificationData {
            title: title.into(),
            body: None, icon: None, badge: None,
            tag: tag.map(String::from),
            silent: false, require_interaction: false,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn show_blocked_without_permission() {
        let mut s = NotificationService::new();
        assert!(!s.show(make("hi", None)));
    }

    #[test]
    fn show_after_grant() {
        let mut s = NotificationService::new();
        s.request_permission(true);
        assert!(s.show(make("hi", None)));
        assert_eq!(s.queue.len(), 1);
    }

    #[test]
    fn tag_replaces_existing() {
        let mut s = NotificationService::new();
        s.request_permission(true);
        s.show(make("v1", Some("msg")));
        s.show(make("v2", Some("msg")));
        assert_eq!(s.queue.len(), 1);
        assert_eq!(s.queue[0].title, "v2");
    }

    #[test]
    fn close_by_tag() {
        let mut s = NotificationService::new();
        s.request_permission(true);
        s.show(make("a", Some("t1")));
        s.show(make("b", Some("t2")));
        assert!(s.close("t1"));
        assert_eq!(s.queue.len(), 1);
    }
}
