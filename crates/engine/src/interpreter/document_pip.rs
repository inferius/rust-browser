//! Document Picture-in-Picture API - PiP s arbitrary HTML content (ne jen video).
//!
//! Spec: https://wicg.github.io/document-picture-in-picture/
//! window.documentPictureInPicture.requestWindow({width, height}) - vraci
//! mini window kde lze put any DOM.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocumentPipState {
    Closed,
    Open,
}

pub struct DocumentPipWindow {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub state: DocumentPipState,
    /// DOM root ID v PiP window (separate Document).
    pub doc_root_id: usize,
}

#[derive(Default)]
pub struct DocumentPipService {
    pub active: Option<DocumentPipWindow>,
    pub next_id: u64,
}

impl DocumentPipService {
    pub fn new() -> Self { Self::default() }

    pub fn request_window(&mut self, width: u32, height: u32) -> Option<u64> {
        if self.active.is_some() { return None; } // only one
        self.next_id += 1;
        let id = self.next_id;
        self.active = Some(DocumentPipWindow {
            id, width, height,
            state: DocumentPipState::Open,
            doc_root_id: 0,
        });
        Some(id)
    }

    pub fn close(&mut self) {
        if let Some(w) = self.active.as_mut() {
            w.state = DocumentPipState::Closed;
        }
        self.active = None;
    }

    pub fn is_active(&self) -> bool {
        self.active.as_ref().map(|w| w.state == DocumentPipState::Open).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_creates_window() {
        let mut s = DocumentPipService::new();
        let id = s.request_window(400, 300);
        assert!(id.is_some());
        assert!(s.is_active());
    }

    #[test]
    fn only_one_at_time() {
        let mut s = DocumentPipService::new();
        s.request_window(400, 300);
        assert!(s.request_window(200, 200).is_none());
    }

    #[test]
    fn close_clears() {
        let mut s = DocumentPipService::new();
        s.request_window(400, 300);
        s.close();
        assert!(!s.is_active());
    }
}
