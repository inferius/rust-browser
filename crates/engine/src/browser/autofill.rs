//! Form autofill - per-field heuristic classification.
//!
//! Chromium reference: components/autofill/core/common/autofill_features.cc

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutofillField {
    Email,
    Username,
    Password,
    NewPassword,
    OneTimeCode,
    FullName,
    GivenName,
    FamilyName,
    AddressLine1,
    AddressLine2,
    AddressCity,
    AddressState,
    AddressZip,
    AddressCountry,
    Phone,
    CreditCardNumber,
    CreditCardName,
    CreditCardExpiry,
    CreditCardCvc,
    Unknown,
}

/// Classify a field by its `autocomplete` attribute or `name`/`id`/label hints.
pub fn classify(autocomplete: Option<&str>, name: &str, id: &str, label: &str, input_type: &str) -> AutofillField {
    // 1. autocomplete attribute takes precedence (HTML5 spec).
    if let Some(ac) = autocomplete {
        if let Some(f) = classify_autocomplete(ac) { return f; }
    }
    // 2. Heuristic match on name/id/label.
    let haystack = format!("{} {} {}", name.to_ascii_lowercase(), id.to_ascii_lowercase(), label.to_ascii_lowercase());
    if input_type == "email" || haystack.contains("email") { return AutofillField::Email; }
    if input_type == "password" {
        if haystack.contains("new") || haystack.contains("confirm") { return AutofillField::NewPassword; }
        return AutofillField::Password;
    }
    if haystack.contains("username") || haystack.contains("user-name") { return AutofillField::Username; }
    if haystack.contains("first") && haystack.contains("name") { return AutofillField::GivenName; }
    if haystack.contains("last") && haystack.contains("name") { return AutofillField::FamilyName; }
    if haystack.contains("full") && haystack.contains("name") { return AutofillField::FullName; }
    if haystack.contains("phone") || haystack.contains("tel") || input_type == "tel" { return AutofillField::Phone; }
    if haystack.contains("city") { return AutofillField::AddressCity; }
    if haystack.contains("state") || haystack.contains("province") { return AutofillField::AddressState; }
    if haystack.contains("zip") || haystack.contains("postal") { return AutofillField::AddressZip; }
    if haystack.contains("country") { return AutofillField::AddressCountry; }
    if haystack.contains("address") || haystack.contains("street") { return AutofillField::AddressLine1; }
    if haystack.contains("cc-number") || haystack.contains("card number") { return AutofillField::CreditCardNumber; }
    if haystack.contains("cc-exp") || haystack.contains("expiry") { return AutofillField::CreditCardExpiry; }
    if haystack.contains("cvc") || haystack.contains("cvv") { return AutofillField::CreditCardCvc; }
    if haystack.contains("otp") || haystack.contains("one-time") { return AutofillField::OneTimeCode; }
    AutofillField::Unknown
}

fn classify_autocomplete(value: &str) -> Option<AutofillField> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "email" => AutofillField::Email,
        "username" => AutofillField::Username,
        "current-password" => AutofillField::Password,
        "new-password" => AutofillField::NewPassword,
        "one-time-code" => AutofillField::OneTimeCode,
        "name" => AutofillField::FullName,
        "given-name" => AutofillField::GivenName,
        "family-name" => AutofillField::FamilyName,
        "address-line1" => AutofillField::AddressLine1,
        "address-line2" => AutofillField::AddressLine2,
        "address-level2" => AutofillField::AddressCity,
        "address-level1" => AutofillField::AddressState,
        "postal-code" => AutofillField::AddressZip,
        "country" | "country-name" => AutofillField::AddressCountry,
        "tel" | "tel-national" => AutofillField::Phone,
        "cc-number" => AutofillField::CreditCardNumber,
        "cc-name" => AutofillField::CreditCardName,
        "cc-exp" => AutofillField::CreditCardExpiry,
        "cc-csc" => AutofillField::CreditCardCvc,
        _ => return None,
    })
}

#[derive(Debug, Clone, Default)]
pub struct AutofillProfile {
    pub values: HashMap<AutofillField, String>,
}

impl AutofillProfile {
    pub fn new() -> Self { Self::default() }

    pub fn set(&mut self, field: AutofillField, value: &str) {
        self.values.insert(field, value.into());
    }

    pub fn fill_into(&self, field: AutofillField) -> Option<&str> {
        self.values.get(&field).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_takes_precedence() {
        let f = classify(Some("email"), "x", "y", "z", "text");
        assert_eq!(f, AutofillField::Email);
    }

    #[test]
    fn name_heuristic_email() {
        let f = classify(None, "user_email", "", "", "text");
        assert_eq!(f, AutofillField::Email);
    }

    #[test]
    fn password_input_kind() {
        let f = classify(None, "passwd", "", "", "password");
        assert_eq!(f, AutofillField::Password);
    }

    #[test]
    fn new_password_detected() {
        let f = classify(None, "new_password", "", "", "password");
        assert_eq!(f, AutofillField::NewPassword);
    }

    #[test]
    fn address_zip() {
        let f = classify(None, "postal_code", "", "", "text");
        assert_eq!(f, AutofillField::AddressZip);
    }

    #[test]
    fn profile_roundtrip() {
        let mut p = AutofillProfile::new();
        p.set(AutofillField::Email, "a@b.com");
        assert_eq!(p.fill_into(AutofillField::Email), Some("a@b.com"));
    }

    #[test]
    fn unknown_default() {
        let f = classify(None, "x", "y", "z", "text");
        assert_eq!(f, AutofillField::Unknown);
    }
}
