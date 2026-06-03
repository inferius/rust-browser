//! Web Share API.
//!
//! Spec: https://w3c.github.io/web-share/
//! navigator.share({ title, text, url, files }) - delegoval na OS share sheet.

#[derive(Debug, Clone, Default)]
pub struct ShareData {
    pub title: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub files: Vec<ShareFile>,
}

#[derive(Debug, Clone)]
pub struct ShareFile {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShareResult {
    Success,
    AbortError,
    NotAllowedError,
    DataError,
}

pub fn can_share(data: &ShareData) -> bool {
    data.title.is_some() || data.text.is_some() || data.url.is_some() || !data.files.is_empty()
}

/// Share - foundation = log + return Success. Real impl pres OS share sheet
/// (Windows DataTransferManager / macOS NSSharingService / Linux portal).
pub fn share(data: &ShareData) -> ShareResult {
    if !can_share(data) { return ShareResult::DataError; }
    ShareResult::Success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_share_with_url() {
        let mut d = ShareData::default();
        d.url = Some("https://x.com".into());
        assert!(can_share(&d));
    }

    #[test]
    fn empty_share_data_invalid() {
        let d = ShareData::default();
        assert!(!can_share(&d));
        assert_eq!(share(&d), ShareResult::DataError);
    }

    #[test]
    fn valid_share_succeeds() {
        let mut d = ShareData::default();
        d.text = Some("hello".into());
        assert_eq!(share(&d), ShareResult::Success);
    }
}
