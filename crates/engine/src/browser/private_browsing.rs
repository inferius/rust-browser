//! Private/incognito browsing - per-session ephemeral state.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionMode {
    Normal,
    Private,
    Guest,                // separate from main profile but persistent across the session
}

#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub mode: SessionMode,
    pub allow_cookies_persist: bool,
    pub allow_history_persist: bool,
    pub allow_cache_persist: bool,
    pub allow_extensions: bool,
    pub allow_download_record: bool,
    pub block_third_party_cookies: bool,
}

impl SessionConfig {
    pub fn normal() -> Self {
        Self {
            mode: SessionMode::Normal,
            allow_cookies_persist: true,
            allow_history_persist: true,
            allow_cache_persist: true,
            allow_extensions: true,
            allow_download_record: true,
            block_third_party_cookies: false,
        }
    }

    pub fn private() -> Self {
        Self {
            mode: SessionMode::Private,
            allow_cookies_persist: false,
            allow_history_persist: false,
            allow_cache_persist: false,
            allow_extensions: false,
            allow_download_record: false,
            block_third_party_cookies: true,
        }
    }

    pub fn guest() -> Self {
        Self {
            mode: SessionMode::Guest,
            allow_cookies_persist: false,
            allow_history_persist: true,         // visible in current session
            allow_cache_persist: false,
            allow_extensions: false,
            allow_download_record: true,
            block_third_party_cookies: false,
        }
    }
}

#[derive(Default)]
pub struct PrivateSessionState {
    pub config: SessionConfig,
    /// Ephemeral cookies for the lifetime of this session only.
    pub ephemeral_cookies_count: u64,
    pub ephemeral_storage_bytes: u64,
}

impl Default for SessionConfig {
    fn default() -> Self { Self::normal() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_blocks_persistence() {
        let c = SessionConfig::private();
        assert!(!c.allow_cookies_persist);
        assert!(!c.allow_history_persist);
        assert!(!c.allow_extensions);
    }

    #[test]
    fn normal_allows_extensions() {
        let c = SessionConfig::normal();
        assert!(c.allow_extensions);
    }

    #[test]
    fn guest_blocks_extensions() {
        let c = SessionConfig::guest();
        assert!(!c.allow_extensions);
        assert!(c.allow_history_persist);
    }

    #[test]
    fn private_blocks_3p_cookies() {
        let c = SessionConfig::private();
        assert!(c.block_third_party_cookies);
    }
}
