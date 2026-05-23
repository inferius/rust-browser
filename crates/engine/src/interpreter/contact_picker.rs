//! Contact Picker API.
//!
//! Spec: https://w3c.github.io/contact-picker/
//! navigator.contacts.select(['name', 'email']) - vraci selected user contacts.

use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct Contact {
    pub name: Vec<String>,
    pub email: Vec<String>,
    pub tel: Vec<String>,
    pub address: Vec<String>,
    pub icon: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContactProperty {
    Name, Email, Tel, Address, Icon,
}

impl ContactProperty {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "name" => Some(Self::Name),
            "email" => Some(Self::Email),
            "tel" => Some(Self::Tel),
            "address" => Some(Self::Address),
            "icon" => Some(Self::Icon),
            _ => None,
        }
    }
}

pub struct ContactsManager {
    /// Stub address book - real: OS contacts API.
    pub stub_contacts: Vec<Contact>,
    pub permission_granted: bool,
}

impl Default for ContactsManager {
    fn default() -> Self {
        Self { stub_contacts: Vec::new(), permission_granted: false }
    }
}

impl ContactsManager {
    pub fn new() -> Self { Self::default() }

    pub fn supported_properties() -> Vec<ContactProperty> {
        vec![ContactProperty::Name, ContactProperty::Email,
             ContactProperty::Tel, ContactProperty::Address,
             ContactProperty::Icon]
    }

    /// Select contacts - user pick. Foundation: empty pri no permission.
    pub fn select(&self, props: &HashSet<ContactProperty>, multiple: bool) -> Vec<Contact> {
        if !self.permission_granted { return Vec::new(); }
        let _ = props;
        if multiple { self.stub_contacts.clone() }
        else { self.stub_contacts.iter().take(1).cloned().collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_blocked_without_permission() {
        let m = ContactsManager::new();
        let props: HashSet<ContactProperty> = [ContactProperty::Name].into_iter().collect();
        assert!(m.select(&props, false).is_empty());
    }

    #[test]
    fn select_with_permission() {
        let mut m = ContactsManager::new();
        m.permission_granted = true;
        m.stub_contacts.push(Contact {
            name: vec!["Alice".into()],
            ..Default::default()
        });
        let props: HashSet<ContactProperty> = [ContactProperty::Name].into_iter().collect();
        assert_eq!(m.select(&props, false).len(), 1);
    }

    #[test]
    fn multiple_returns_all() {
        let mut m = ContactsManager::new();
        m.permission_granted = true;
        for n in &["A", "B", "C"] {
            m.stub_contacts.push(Contact {
                name: vec![(*n).into()],
                ..Default::default()
            });
        }
        let props: HashSet<ContactProperty> = [ContactProperty::Name].into_iter().collect();
        assert_eq!(m.select(&props, true).len(), 3);
        assert_eq!(m.select(&props, false).len(), 1);
    }

    #[test]
    fn supported_props_list() {
        let s = ContactsManager::supported_properties();
        assert!(s.contains(&ContactProperty::Email));
    }
}
