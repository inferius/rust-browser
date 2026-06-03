//! OS clipboard integration bridge.
//!
//! Real backend = arboard (cross-platform). This is the per-app surface that
//! sits between Renderer (sandbox) and the actual OS clipboard via IPC.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipboardOp {
    Read,
    Write,
}

#[derive(Debug, Clone)]
pub struct ClipboardRequest {
    pub origin: String,
    pub op: ClipboardOp,
    pub format_mime: String,
    pub size_bytes: u64,
    pub timestamp_unix_ms: u64,
}

#[derive(Default)]
pub struct OsClipboardBridge {
    /// User must approve each new origin for read access.
    pub allowed_read_origins: std::collections::HashSet<String>,
    /// Last paste content (if any) - bridge cache.
    pub cache: HashMap<String, Vec<u8>>,         // format -> bytes
    pub last_request: Option<ClipboardRequest>,
    /// Track recent writes to suppress duplicate notifications.
    pub recent_writes: Vec<String>,
    pub user_gesture_required: bool,
}

impl OsClipboardBridge {
    pub fn new() -> Self {
        Self { user_gesture_required: true, ..Self::default() }
    }

    pub fn request_read(&mut self, request: ClipboardRequest, user_gesture: bool) -> Result<Option<&[u8]>, String> {
        if self.user_gesture_required && !user_gesture {
            return Err("requires user gesture".into());
        }
        if !self.allowed_read_origins.contains(&request.origin) {
            return Err(format!("origin {} not approved for clipboard read", request.origin));
        }
        self.last_request = Some(request.clone());
        Ok(self.cache.get(&request.format_mime).map(|v| v.as_slice()))
    }

    pub fn request_write(&mut self, request: ClipboardRequest, bytes: Vec<u8>, user_gesture: bool) -> Result<(), String> {
        if self.user_gesture_required && !user_gesture {
            return Err("requires user gesture".into());
        }
        let fmt = request.format_mime.clone();
        self.cache.insert(fmt.clone(), bytes);
        if !self.recent_writes.contains(&fmt) {
            self.recent_writes.push(fmt);
        }
        self.last_request = Some(request);
        Ok(())
    }

    pub fn approve_origin_for_read(&mut self, origin: &str) {
        self.allowed_read_origins.insert(origin.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(op: ClipboardOp, origin: &str, fmt: &str) -> ClipboardRequest {
        ClipboardRequest {
            origin: origin.into(),
            op, format_mime: fmt.into(),
            size_bytes: 0,
            timestamp_unix_ms: 0,
        }
    }

    #[test]
    fn write_requires_gesture() {
        let mut b = OsClipboardBridge::new();
        let r = b.request_write(req(ClipboardOp::Write, "x.com", "text/plain"), b"hi".to_vec(), false);
        assert!(r.is_err());
    }

    #[test]
    fn write_with_gesture() {
        let mut b = OsClipboardBridge::new();
        let r = b.request_write(req(ClipboardOp::Write, "x.com", "text/plain"), b"hi".to_vec(), true);
        assert!(r.is_ok());
        assert!(b.cache.contains_key("text/plain"));
    }

    #[test]
    fn read_requires_approved_origin() {
        let mut b = OsClipboardBridge::new();
        let r = b.request_read(req(ClipboardOp::Read, "x.com", "text/plain"), true);
        assert!(r.is_err());
    }

    #[test]
    fn read_after_approval() {
        let mut b = OsClipboardBridge::new();
        b.request_write(req(ClipboardOp::Write, "y.com", "text/plain"), b"hi".to_vec(), true).unwrap();
        b.approve_origin_for_read("x.com");
        let bytes = b.request_read(req(ClipboardOp::Read, "x.com", "text/plain"), true).unwrap();
        assert_eq!(bytes.unwrap(), b"hi");
    }
}
