//! Encrypted Media Extensions (EME) - DRM-protected playback.
//!
//! Spec: https://www.w3.org/TR/encrypted-media/
//! navigator.requestMediaKeySystemAccess(keySystem, [config])
//!   .createMediaKeys() -> setMediaKeys() on HTMLMediaElement.
//! Key systems: org.w3.clearkey (test), com.widevine.alpha, com.microsoft.playready.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeySystemSupport {
    NotSupported,
    Supported,
    Required,         // "encryption" config required this
    Optional,
}

#[derive(Debug, Clone)]
pub struct KeySystemConfig {
    pub init_data_types: Vec<String>,      // ["cenc", "keyids", "webm"]
    pub audio_capabilities: Vec<MediaCapability>,
    pub video_capabilities: Vec<MediaCapability>,
    pub persistent_state: KeySystemSupport,
    pub distinctive_identifier: KeySystemSupport,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct MediaCapability {
    pub content_type: String,              // MIME including codecs
    pub robustness: String,                // empty | SW_SECURE_CRYPTO | HW_SECURE_ALL ...
    pub encryption_scheme: Option<String>, // cenc / cbcs
}

#[derive(Debug, Clone)]
pub struct KeySystemAccess {
    pub key_system: String,
    pub config: KeySystemConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionType {
    Temporary,
    PersistentLicense,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageType {
    LicenseRequest,
    LicenseRenewal,
    LicenseRelease,
    IndividualizationRequest,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyStatus {
    Usable,
    Expired,
    Released,
    OutputRestricted,
    OutputDownscaled,
    StatusPending,
    InternalError,
}

#[derive(Debug, Clone)]
pub struct MediaKeySession {
    pub session_id: String,
    pub session_type: SessionType,
    pub key_statuses: HashMap<Vec<u8>, KeyStatus>,
    pub expiration_unix_ms: Option<u64>,
    pub closed: bool,
    pub message_queue: Vec<(MessageType, Vec<u8>)>,
}

#[derive(Default)]
pub struct MediaKeys {
    pub sessions: HashMap<String, MediaKeySession>,
    pub key_system: String,
    pub server_certificate: Option<Vec<u8>>,
    pub next_session_idx: u64,
}

impl MediaKeys {
    pub fn new(key_system: &str) -> Self {
        Self { key_system: key_system.into(), ..Self::default() }
    }

    pub fn create_session(&mut self, session_type: SessionType) -> String {
        self.next_session_idx += 1;
        let session_id = format!("sess-{}-{}", self.key_system, self.next_session_idx);
        self.sessions.insert(session_id.clone(), MediaKeySession {
            session_id: session_id.clone(),
            session_type,
            key_statuses: HashMap::new(),
            expiration_unix_ms: None,
            closed: false,
            message_queue: Vec::new(),
        });
        session_id
    }

    /// Generate a license-request message (real impl talks to CDM).
    pub fn generate_request(&mut self, session_id: &str, _init_data_type: &str, _init_data: &[u8]) -> Result<(), String> {
        let s = self.sessions.get_mut(session_id).ok_or("session not found")?;
        s.message_queue.push((MessageType::LicenseRequest, b"placeholder-license-request".to_vec()));
        Ok(())
    }

    /// Caller-provided license response is applied to update key statuses.
    pub fn update(&mut self, session_id: &str, response: &[u8]) -> Result<(), String> {
        let s = self.sessions.get_mut(session_id).ok_or("session not found")?;
        // Pretend response carries a key id and status.
        if response.len() >= 1 {
            let key_id = response[..response.len().min(16)].to_vec();
            s.key_statuses.insert(key_id, KeyStatus::Usable);
        }
        Ok(())
    }

    pub fn close(&mut self, session_id: &str) -> Result<(), String> {
        let s = self.sessions.get_mut(session_id).ok_or("session not found")?;
        s.closed = true;
        Ok(())
    }
}

pub fn supports_key_system(key_system: &str) -> bool {
    matches!(key_system,
        "org.w3.clearkey" | "com.widevine.alpha" | "com.microsoft.playready" | "com.apple.fps"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_clearkey() {
        assert!(supports_key_system("org.w3.clearkey"));
        assert!(!supports_key_system("foo.bar"));
    }

    #[test]
    fn create_session_returns_id() {
        let mut mk = MediaKeys::new("org.w3.clearkey");
        let id = mk.create_session(SessionType::Temporary);
        assert!(mk.sessions.contains_key(&id));
    }

    #[test]
    fn generate_request_queues_message() {
        let mut mk = MediaKeys::new("org.w3.clearkey");
        let sid = mk.create_session(SessionType::Temporary);
        mk.generate_request(&sid, "cenc", b"init").unwrap();
        let s = &mk.sessions[&sid];
        assert_eq!(s.message_queue.len(), 1);
        assert_eq!(s.message_queue[0].0, MessageType::LicenseRequest);
    }

    #[test]
    fn update_records_usable_key() {
        let mut mk = MediaKeys::new("org.w3.clearkey");
        let sid = mk.create_session(SessionType::Temporary);
        mk.update(&sid, &[1; 16]).unwrap();
        let s = &mk.sessions[&sid];
        assert_eq!(s.key_statuses.len(), 1);
        assert_eq!(*s.key_statuses.values().next().unwrap(), KeyStatus::Usable);
    }

    #[test]
    fn close_marks_session_closed() {
        let mut mk = MediaKeys::new("org.w3.clearkey");
        let sid = mk.create_session(SessionType::Temporary);
        mk.close(&sid).unwrap();
        assert!(mk.sessions[&sid].closed);
    }
}
