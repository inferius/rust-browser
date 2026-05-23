//! Credential Management API + WebAuthn foundation.
//!
//! Specs:
//! - https://www.w3.org/TR/credential-management-1/
//! - https://www.w3.org/TR/webauthn-3/
//!
//! navigator.credentials.get/store + PublicKeyCredential (FIDO2/WebAuthn).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Credential {
    Password(PasswordCredential),
    Federated(FederatedCredential),
    PublicKey(PublicKeyCredential),
}

#[derive(Debug, Clone)]
pub struct PasswordCredential {
    pub id: String,        // username
    pub password: String,
    pub origin: String,
}

#[derive(Debug, Clone)]
pub struct FederatedCredential {
    pub id: String,
    pub provider: String,   // "https://accounts.google.com"
    pub origin: String,
}

#[derive(Debug, Clone)]
pub struct PublicKeyCredential {
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub user_id: Vec<u8>,
    pub origin: String,
}

#[derive(Default)]
pub struct CredentialStore {
    /// origin -> credentials (zero-knowledge real impl by use OS keychain).
    pub by_origin: HashMap<String, Vec<Credential>>,
}

impl CredentialStore {
    pub fn new() -> Self { Self::default() }

    pub fn store(&mut self, cred: Credential) {
        let origin = match &cred {
            Credential::Password(c) => c.origin.clone(),
            Credential::Federated(c) => c.origin.clone(),
            Credential::PublicKey(c) => c.origin.clone(),
        };
        self.by_origin.entry(origin).or_default().push(cred);
    }

    pub fn get(&self, origin: &str) -> Vec<&Credential> {
        self.by_origin.get(origin).map(|v| v.iter().collect()).unwrap_or_default()
    }

    pub fn delete_for_origin(&mut self, origin: &str) -> usize {
        let n = self.by_origin.get(origin).map(|v| v.len()).unwrap_or(0);
        self.by_origin.remove(origin);
        n
    }

    pub fn prevent_silent_access(&self) {
        // Flag - require user gesture pri next get. Foundation: no-op.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_get_password() {
        let mut s = CredentialStore::new();
        s.store(Credential::Password(PasswordCredential {
            id: "user@example.com".into(),
            password: "pwd".into(),
            origin: "https://example.com".into(),
        }));
        let creds = s.get("https://example.com");
        assert_eq!(creds.len(), 1);
    }

    #[test]
    fn delete_for_origin() {
        let mut s = CredentialStore::new();
        s.store(Credential::Password(PasswordCredential {
            id: "u".into(),
            password: "p".into(),
            origin: "https://x.com".into(),
        }));
        assert_eq!(s.delete_for_origin("https://x.com"), 1);
        assert!(s.get("https://x.com").is_empty());
    }

    #[test]
    fn public_key_credential() {
        let mut s = CredentialStore::new();
        s.store(Credential::PublicKey(PublicKeyCredential {
            credential_id: vec![1, 2, 3, 4],
            public_key: vec![5, 6, 7, 8],
            user_id: vec![9, 10],
            origin: "https://app.com".into(),
        }));
        let creds = s.get("https://app.com");
        assert!(matches!(creds[0], Credential::PublicKey(_)));
    }

    #[test]
    fn origin_isolation() {
        let mut s = CredentialStore::new();
        s.store(Credential::Password(PasswordCredential {
            id: "u".into(), password: "p".into(),
            origin: "https://a.com".into(),
        }));
        assert!(s.get("https://b.com").is_empty());
    }
}
