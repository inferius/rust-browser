//! Form constraint validation - HTML5 `<input>` validity states.
//!
//! Spec: https://html.spec.whatwg.org/multipage/form-control-infrastructure.html

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidityFlag {
    ValueMissing,
    TypeMismatch,
    PatternMismatch,
    TooLong,
    TooShort,
    RangeUnderflow,
    RangeOverflow,
    StepMismatch,
    BadInput,
    CustomError,
}

#[derive(Debug, Clone, Default)]
pub struct ValidityState {
    pub flags: u32,
    pub custom_error_message: String,
}

impl ValidityState {
    pub fn set(&mut self, flag: ValidityFlag) {
        self.flags |= 1 << flag.bit_index();
    }
    pub fn clear(&mut self, flag: ValidityFlag) {
        self.flags &= !(1 << flag.bit_index());
    }
    pub fn has(&self, flag: ValidityFlag) -> bool {
        (self.flags & (1 << flag.bit_index())) != 0
    }
    pub fn is_valid(&self) -> bool { self.flags == 0 }
}

impl ValidityFlag {
    pub fn bit_index(&self) -> u32 {
        match self {
            Self::ValueMissing => 0,
            Self::TypeMismatch => 1,
            Self::PatternMismatch => 2,
            Self::TooLong => 3,
            Self::TooShort => 4,
            Self::RangeUnderflow => 5,
            Self::RangeOverflow => 6,
            Self::StepMismatch => 7,
            Self::BadInput => 8,
            Self::CustomError => 9,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InputConstraints {
    pub required: bool,
    pub pattern: Option<String>,         // regex
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub type_kind: InputType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputType {
    Text,
    Email,
    Url,
    Number,
    Date,
    Time,
    Tel,
    Search,
    Color,
    Range,
    File,
    Checkbox,
    Radio,
    Password,
    Hidden,
}

impl Default for InputType {
    fn default() -> Self { InputType::Text }
}

/// Validate a value against constraints; mutates ValidityState.
pub fn validate(value: &str, c: &InputConstraints, state: &mut ValidityState) {
    state.flags = 0;
    if c.required && value.is_empty() {
        state.set(ValidityFlag::ValueMissing);
    }
    if let Some(max) = c.max_length {
        if value.chars().count() > max {
            state.set(ValidityFlag::TooLong);
        }
    }
    if let Some(min) = c.min_length {
        if !value.is_empty() && value.chars().count() < min {
            state.set(ValidityFlag::TooShort);
        }
    }
    if !value.is_empty() {
        if let Some(pattern) = &c.pattern {
            // simplified: require exact full-string match per spec, using fancy-regex-like syntax.
            // we don't actually run regex here; return PatternMismatch only if value doesn't contain
            // any pattern char (placeholder).
            if pattern.is_empty() {
                state.set(ValidityFlag::PatternMismatch);
            }
        }
        match c.type_kind {
            InputType::Email => {
                if !value.contains('@') || !value.contains('.') {
                    state.set(ValidityFlag::TypeMismatch);
                }
            }
            InputType::Url => {
                if !value.contains("://") {
                    state.set(ValidityFlag::TypeMismatch);
                }
            }
            InputType::Number | InputType::Range => {
                match value.parse::<f64>() {
                    Ok(n) => {
                        if let Some(min) = c.min { if n < min { state.set(ValidityFlag::RangeUnderflow); } }
                        if let Some(max) = c.max { if n > max { state.set(ValidityFlag::RangeOverflow); } }
                        if let Some(step) = c.step {
                            let base = c.min.unwrap_or(0.0);
                            let r = ((n - base) / step).abs();
                            if (r - r.round()).abs() > 1e-9 {
                                state.set(ValidityFlag::StepMismatch);
                            }
                        }
                    }
                    Err(_) => state.set(ValidityFlag::BadInput),
                }
            }
            _ => {}
        }
    }
    if !state.custom_error_message.is_empty() {
        state.set(ValidityFlag::CustomError);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_required() {
        let mut s = ValidityState::default();
        let c = InputConstraints { required: true, ..Default::default() };
        validate("", &c, &mut s);
        assert!(s.has(ValidityFlag::ValueMissing));
    }

    #[test]
    fn email_format() {
        let mut s = ValidityState::default();
        let c = InputConstraints { type_kind: InputType::Email, ..Default::default() };
        validate("not-email", &c, &mut s);
        assert!(s.has(ValidityFlag::TypeMismatch));
        s = ValidityState::default();
        validate("a@b.com", &c, &mut s);
        assert!(s.is_valid());
    }

    #[test]
    fn range_underflow() {
        let mut s = ValidityState::default();
        let c = InputConstraints { type_kind: InputType::Number, min: Some(10.0), ..Default::default() };
        validate("5", &c, &mut s);
        assert!(s.has(ValidityFlag::RangeUnderflow));
    }

    #[test]
    fn step_mismatch() {
        let mut s = ValidityState::default();
        let c = InputConstraints { type_kind: InputType::Number, min: Some(0.0), step: Some(5.0), ..Default::default() };
        validate("7", &c, &mut s);
        assert!(s.has(ValidityFlag::StepMismatch));
        s = ValidityState::default();
        validate("10", &c, &mut s);
        assert!(!s.has(ValidityFlag::StepMismatch));
    }

    #[test]
    fn too_long() {
        let mut s = ValidityState::default();
        let c = InputConstraints { max_length: Some(3), ..Default::default() };
        validate("abcd", &c, &mut s);
        assert!(s.has(ValidityFlag::TooLong));
    }

    #[test]
    fn custom_error() {
        let mut s = ValidityState::default();
        s.custom_error_message = "nope".into();
        validate("ok", &InputConstraints::default(), &mut s);
        assert!(s.has(ValidityFlag::CustomError));
    }
}
