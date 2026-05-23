//! FedCM (Federated Credential Management) - browser-mediated SSO bez third-party cookies.
//!
//! Spec: https://w3c-fedid.github.io/FedCM/
//! navigator.credentials.get({identity: {providers: [{configURL, clientId, ...}]}}).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct IdpConfig {
    pub config_url: String,             // /.well-known/web-identity entry
    pub client_id: String,
    pub nonce: Option<String>,
    pub fields: Vec<String>,            // ["name", "email", "picture"]
    pub login_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IdentityProviderAccount {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture_url: Option<String>,
    pub approved_clients: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FedCmState {
    Idle,
    AccountsFetched,
    UserChose,
    TokenIssued,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FedCmFlow {
    pub state: FedCmState,
    pub config: IdpConfig,
    pub accounts: Vec<IdentityProviderAccount>,
    pub chosen_id: Option<String>,
    pub token: Option<String>,
    pub error: Option<String>,
}

impl FedCmFlow {
    pub fn new(config: IdpConfig) -> Self {
        Self {
            state: FedCmState::Idle,
            config,
            accounts: Vec::new(),
            chosen_id: None,
            token: None,
            error: None,
        }
    }

    pub fn ingest_accounts(&mut self, accounts: Vec<IdentityProviderAccount>) {
        self.accounts = accounts;
        self.state = FedCmState::AccountsFetched;
    }

    pub fn choose(&mut self, id: &str) -> Result<(), String> {
        if !self.accounts.iter().any(|a| a.id == id) {
            return Err(format!("unknown account {}", id));
        }
        self.chosen_id = Some(id.into());
        self.state = FedCmState::UserChose;
        Ok(())
    }

    pub fn issue_token(&mut self, token: &str) {
        self.token = Some(token.into());
        self.state = FedCmState::TokenIssued;
    }

    pub fn fail(&mut self, reason: &str) {
        self.error = Some(reason.into());
        self.state = FedCmState::Failed;
    }

    /// Returning user shortcut - account uz approved client, lze auto-sign-in.
    pub fn is_returning_user(&self) -> bool {
        let Some(id) = &self.chosen_id else { return false; };
        self.accounts.iter().find(|a| &a.id == id)
            .map(|a| a.approved_clients.contains(&self.config.client_id))
            .unwrap_or(false)
    }
}

#[derive(Default)]
pub struct FedCmManager {
    pub flows: HashMap<u64, FedCmFlow>,
    pub next_id: u64,
}

impl FedCmManager {
    pub fn new() -> Self { Self::default() }

    pub fn start(&mut self, config: IdpConfig) -> u64 {
        self.next_id += 1;
        self.flows.insert(self.next_id, FedCmFlow::new(config));
        self.next_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> IdpConfig {
        IdpConfig {
            config_url: "https://idp.example/.well-known/web-identity".into(),
            client_id: "abc".into(),
            nonce: Some("n1".into()),
            fields: vec!["email".into()],
            login_hint: None,
        }
    }

    #[test]
    fn flow_progresses() {
        let mut f = FedCmFlow::new(cfg());
        assert_eq!(f.state, FedCmState::Idle);
        f.ingest_accounts(vec![IdentityProviderAccount {
            id: "u1".into(), email: "a@b.com".into(), name: "A".into(),
            picture_url: None, approved_clients: vec![],
        }]);
        assert_eq!(f.state, FedCmState::AccountsFetched);
        f.choose("u1").unwrap();
        f.issue_token("eyJ...");
        assert_eq!(f.state, FedCmState::TokenIssued);
        assert_eq!(f.token.as_deref(), Some("eyJ..."));
    }

    #[test]
    fn choose_unknown_fails() {
        let mut f = FedCmFlow::new(cfg());
        f.ingest_accounts(vec![]);
        assert!(f.choose("missing").is_err());
    }

    #[test]
    fn returning_user_detected() {
        let mut f = FedCmFlow::new(cfg());
        f.ingest_accounts(vec![IdentityProviderAccount {
            id: "u1".into(), email: "a@b.com".into(), name: "A".into(),
            picture_url: None, approved_clients: vec!["abc".into()],
        }]);
        f.choose("u1").unwrap();
        assert!(f.is_returning_user());
    }
}
