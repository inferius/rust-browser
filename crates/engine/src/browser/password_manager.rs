//! Password manager state - per-origin credentials + autofill prompts.
//!
//! Storage on disk in real impl encrypted via OS keychain. Foundation API only.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PasswordCredential {
    pub origin: String,
    pub username: String,
    pub password: String,
    pub created_unix_ms: u64,
    pub last_used_unix_ms: u64,
    pub auto_sign_in: bool,
    pub federation_origin: Option<String>,    // for federated identity providers
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaveDecision {
    Save,
    Update,
    NeverForThisSite,
    Dismiss,
}

#[derive(Default)]
pub struct PasswordStore {
    /// Origin -> list of credentials (most recent first).
    pub credentials: HashMap<String, Vec<PasswordCredential>>,
    pub never_save_origins: std::collections::HashSet<String>,
}

impl PasswordStore {
    pub fn new() -> Self { Self::default() }

    pub fn save(&mut self, c: PasswordCredential) -> Result<(), String> {
        if self.never_save_origins.contains(&c.origin) {
            return Err("origin opted out".into());
        }
        let list = self.credentials.entry(c.origin.clone()).or_default();
        // If same username exists, update password + last_used.
        if let Some(existing) = list.iter_mut().find(|x| x.username == c.username) {
            existing.password = c.password;
            existing.last_used_unix_ms = c.last_used_unix_ms;
            return Ok(());
        }
        list.insert(0, c);
        Ok(())
    }

    pub fn find_for(&self, origin: &str) -> Vec<&PasswordCredential> {
        self.credentials.get(origin).map(|v| v.iter().collect()).unwrap_or_default()
    }

    pub fn remove(&mut self, origin: &str, username: &str) -> bool {
        if let Some(list) = self.credentials.get_mut(origin) {
            let before = list.len();
            list.retain(|c| c.username != username);
            return list.len() < before;
        }
        false
    }

    pub fn opt_out(&mut self, origin: &str) {
        self.never_save_origins.insert(origin.into());
        self.credentials.remove(origin);
    }

    pub fn update_last_used(&mut self, origin: &str, username: &str, now: u64) {
        if let Some(list) = self.credentials.get_mut(origin) {
            if let Some(c) = list.iter_mut().find(|c| c.username == username) {
                c.last_used_unix_ms = now;
            }
        }
    }

    /// Returns the credential to use for auto-sign-in (1 credential + auto_sign_in flag set).
    pub fn auto_sign_in_for(&self, origin: &str) -> Option<&PasswordCredential> {
        let list = self.credentials.get(origin)?;
        if list.len() != 1 { return None; }
        let c = &list[0];
        if !c.auto_sign_in { return None; }
        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cred(origin: &str, user: &str, pass: &str) -> PasswordCredential {
        PasswordCredential {
            origin: origin.into(), username: user.into(), password: pass.into(),
            created_unix_ms: 0, last_used_unix_ms: 0,
            auto_sign_in: false, federation_origin: None,
        }
    }

    #[test]
    fn save_and_find() {
        let mut s = PasswordStore::new();
        s.save(cred("x.com", "alice", "p1")).unwrap();
        let creds = s.find_for("x.com");
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].username, "alice");
    }

    #[test]
    fn save_updates_existing() {
        let mut s = PasswordStore::new();
        s.save(cred("x.com", "alice", "p1")).unwrap();
        s.save(cred("x.com", "alice", "p2")).unwrap();
        assert_eq!(s.find_for("x.com").len(), 1);
        assert_eq!(s.find_for("x.com")[0].password, "p2");
    }

    #[test]
    fn remove() {
        let mut s = PasswordStore::new();
        s.save(cred("x.com", "alice", "p1")).unwrap();
        assert!(s.remove("x.com", "alice"));
        assert!(!s.remove("x.com", "alice"));
    }

    #[test]
    fn opt_out_blocks_save() {
        let mut s = PasswordStore::new();
        s.opt_out("x.com");
        assert!(s.save(cred("x.com", "u", "p")).is_err());
    }

    #[test]
    fn auto_sign_in_single() {
        let mut s = PasswordStore::new();
        let mut c = cred("x.com", "u", "p");
        c.auto_sign_in = true;
        s.save(c).unwrap();
        assert!(s.auto_sign_in_for("x.com").is_some());
    }

    #[test]
    fn auto_sign_in_blocked_when_multiple() {
        let mut s = PasswordStore::new();
        let mut a = cred("x.com", "u1", "p");
        a.auto_sign_in = true;
        let mut b = cred("x.com", "u2", "p");
        b.auto_sign_in = true;
        s.save(a).unwrap();
        s.save(b).unwrap();
        // 2 credentials -> require user choice
        assert!(s.auto_sign_in_for("x.com").is_none());
    }
}
