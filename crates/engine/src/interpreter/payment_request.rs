//! Payment Request API.
//!
//! Spec: https://www.w3.org/TR/payment-request/
//! PaymentRequest({methods, details}) -> show() -> show payment sheet.

#[derive(Debug, Clone)]
pub struct PaymentMethodData {
    pub supported_methods: String,    // "basic-card" / "https://apple.com/apple-pay" / etc
    pub data: serde_json_value::Value,
}

mod serde_json_value {
    #[derive(Debug, Clone, Default)]
    pub struct Value(pub String); // simplified placeholder
}

#[derive(Debug, Clone)]
pub struct PaymentDetails {
    pub total: PaymentItem,
    pub display_items: Vec<PaymentItem>,
    pub shipping_options: Vec<ShippingOption>,
}

#[derive(Debug, Clone)]
pub struct PaymentItem {
    pub label: String,
    pub amount_currency: String,
    pub amount_value: String,
}

#[derive(Debug, Clone)]
pub struct ShippingOption {
    pub id: String,
    pub label: String,
    pub amount_currency: String,
    pub amount_value: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaymentRequestState {
    Created,
    Interactive,
    Closed,
}

pub struct PaymentRequest {
    pub methods: Vec<PaymentMethodData>,
    pub details: PaymentDetails,
    pub state: PaymentRequestState,
}

impl PaymentRequest {
    pub fn new(methods: Vec<PaymentMethodData>, details: PaymentDetails) -> Self {
        Self { methods, details, state: PaymentRequestState::Created }
    }

    pub fn show(&mut self) -> bool {
        if self.state != PaymentRequestState::Created { return false; }
        self.state = PaymentRequestState::Interactive;
        true
    }

    pub fn abort(&mut self) -> bool {
        if self.state != PaymentRequestState::Interactive { return false; }
        self.state = PaymentRequestState::Closed;
        true
    }

    /// Foundation can_make_payment - real: check installed payment apps.
    pub fn can_make_payment(&self) -> bool {
        !self.methods.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_method() -> PaymentMethodData {
        PaymentMethodData {
            supported_methods: "basic-card".into(),
            data: Default::default(),
        }
    }

    fn sample_details() -> PaymentDetails {
        PaymentDetails {
            total: PaymentItem {
                label: "Total".into(),
                amount_currency: "USD".into(),
                amount_value: "10.00".into(),
            },
            display_items: vec![],
            shipping_options: vec![],
        }
    }

    #[test]
    fn create_can_make_payment() {
        let r = PaymentRequest::new(vec![sample_method()], sample_details());
        assert!(r.can_make_payment());
    }

    #[test]
    fn no_methods_blocks() {
        let r = PaymentRequest::new(vec![], sample_details());
        assert!(!r.can_make_payment());
    }

    #[test]
    fn show_transitions() {
        let mut r = PaymentRequest::new(vec![sample_method()], sample_details());
        assert!(r.show());
        assert_eq!(r.state, PaymentRequestState::Interactive);
        assert!(!r.show()); // already shown
    }

    #[test]
    fn abort_closes() {
        let mut r = PaymentRequest::new(vec![sample_method()], sample_details());
        r.show();
        r.abort();
        assert_eq!(r.state, PaymentRequestState::Closed);
    }
}
