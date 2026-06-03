//! Proxy + Reflect - ECMA-262 ProxyExoticObject.
//!
//! Proxy traps: get, set, has, deleteProperty, ownKeys, getOwnPropertyDescriptor,
//! defineProperty, preventExtensions, isExtensible, getPrototypeOf, setPrototypeOf,
//! apply, construct.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProxyTrap {
    Get,
    Set,
    Has,
    DeleteProperty,
    OwnKeys,
    GetOwnPropertyDescriptor,
    DefineProperty,
    PreventExtensions,
    IsExtensible,
    GetPrototypeOf,
    SetPrototypeOf,
    Apply,
    Construct,
}

impl ProxyTrap {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "get" => Self::Get,
            "set" => Self::Set,
            "has" => Self::Has,
            "deleteProperty" => Self::DeleteProperty,
            "ownKeys" => Self::OwnKeys,
            "getOwnPropertyDescriptor" => Self::GetOwnPropertyDescriptor,
            "defineProperty" => Self::DefineProperty,
            "preventExtensions" => Self::PreventExtensions,
            "isExtensible" => Self::IsExtensible,
            "getPrototypeOf" => Self::GetPrototypeOf,
            "setPrototypeOf" => Self::SetPrototypeOf,
            "apply" => Self::Apply,
            "construct" => Self::Construct,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Get => "get", Self::Set => "set", Self::Has => "has",
            Self::DeleteProperty => "deleteProperty",
            Self::OwnKeys => "ownKeys",
            Self::GetOwnPropertyDescriptor => "getOwnPropertyDescriptor",
            Self::DefineProperty => "defineProperty",
            Self::PreventExtensions => "preventExtensions",
            Self::IsExtensible => "isExtensible",
            Self::GetPrototypeOf => "getPrototypeOf",
            Self::SetPrototypeOf => "setPrototypeOf",
            Self::Apply => "apply",
            Self::Construct => "construct",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyDef {
    pub target_id: u64,
    pub handler_id: u64,
    pub revoked: bool,
    pub registered_traps: Vec<ProxyTrap>,
}

#[derive(Default)]
pub struct ProxyRegistry {
    pub proxies: std::collections::HashMap<u64, ProxyDef>,
    pub next_id: u64,
}

impl ProxyRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, target_id: u64, handler_id: u64, traps: Vec<ProxyTrap>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.proxies.insert(id, ProxyDef {
            target_id, handler_id, revoked: false,
            registered_traps: traps,
        });
        id
    }

    /// Proxy.revocable() returns a revoker function.
    pub fn revoke(&mut self, proxy_id: u64) -> bool {
        if let Some(p) = self.proxies.get_mut(&proxy_id) {
            p.revoked = true;
            true
        } else { false }
    }

    pub fn is_revoked(&self, proxy_id: u64) -> bool {
        self.proxies.get(&proxy_id).map(|p| p.revoked).unwrap_or(false)
    }

    pub fn has_trap(&self, proxy_id: u64, trap: ProxyTrap) -> bool {
        self.proxies.get(&proxy_id).map(|p| p.registered_traps.contains(&trap)).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_round_trip() {
        let t = ProxyTrap::from_name("get").unwrap();
        assert_eq!(t.name(), "get");
    }

    #[test]
    fn unknown_trap_none() {
        assert!(ProxyTrap::from_name("xyz").is_none());
    }

    #[test]
    fn create_and_lookup() {
        let mut r = ProxyRegistry::new();
        let id = r.create(1, 2, vec![ProxyTrap::Get]);
        assert!(r.has_trap(id, ProxyTrap::Get));
        assert!(!r.has_trap(id, ProxyTrap::Set));
    }

    #[test]
    fn revoke_marks_inaccessible() {
        let mut r = ProxyRegistry::new();
        let id = r.create(1, 2, vec![]);
        assert!(r.revoke(id));
        assert!(r.is_revoked(id));
    }

    #[test]
    fn revoke_unknown_returns_false() {
        let mut r = ProxyRegistry::new();
        assert!(!r.revoke(999));
    }
}
