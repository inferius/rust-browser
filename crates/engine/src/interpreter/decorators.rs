//! TC39 Decorators Stage 3 - method/class/field decorators.
//!
//! Proposal: https://github.com/tc39/proposal-decorators
//! Stage 3 (2023). Spec includes `@dec class C { @dec method() {} @dec field; }`.
//!
//! Decorator function shape:
//!   (value, context) -> new_value
//! context = { kind: 'method'|'class'|'field'|'getter'|'setter'|'accessor',
//!             name, access: {get, set}, static, private, addInitializer }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecoratorKind {
    Class,
    Method,
    Getter,
    Setter,
    Field,
    Accessor,
}

#[derive(Debug, Clone)]
pub struct DecoratorContext {
    pub kind: DecoratorKind,
    pub name: String,
    pub is_static: bool,
    pub is_private: bool,
    pub initializers: Vec<u64>,     // callback ids to run during construction
}

impl DecoratorContext {
    pub fn new(kind: DecoratorKind, name: &str) -> Self {
        Self { kind, name: name.into(), is_static: false, is_private: false, initializers: Vec::new() }
    }

    pub fn add_initializer(&mut self, callback_id: u64) {
        self.initializers.push(callback_id);
    }
}

#[derive(Debug, Clone)]
pub struct DecoratorApplication {
    pub target: String,                  // class or member name
    pub kind: DecoratorKind,
    pub decorator_callback_ids: Vec<u64>, // applied bottom-up per spec
}

#[derive(Default)]
pub struct DecoratorRegistry {
    pub applications: Vec<DecoratorApplication>,
}

impl DecoratorRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn record(&mut self, app: DecoratorApplication) {
        self.applications.push(app);
    }

    /// Returns the order in which decorators are evaluated for a target.
    /// Per spec: innermost (closest to declaration) first.
    pub fn evaluation_order(&self, target: &str) -> Vec<u64> {
        for app in &self.applications {
            if app.target == target {
                // Per spec, innermost = LAST in source code = FIRST in evaluation.
                let mut out = app.decorator_callback_ids.clone();
                out.reverse();
                return out;
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_default_flags() {
        let c = DecoratorContext::new(DecoratorKind::Method, "doStuff");
        assert!(!c.is_static);
        assert!(!c.is_private);
    }

    #[test]
    fn add_initializer_appends() {
        let mut c = DecoratorContext::new(DecoratorKind::Field, "x");
        c.add_initializer(1);
        c.add_initializer(2);
        assert_eq!(c.initializers, vec![1, 2]);
    }

    #[test]
    fn evaluation_order_reverses() {
        let mut r = DecoratorRegistry::new();
        r.record(DecoratorApplication {
            target: "MyClass".into(),
            kind: DecoratorKind::Class,
            decorator_callback_ids: vec![1, 2, 3],
        });
        // Source order: @a @b @c class MyClass {}
        // Innermost (closest to declaration) = c = id 3.
        assert_eq!(r.evaluation_order("MyClass"), vec![3, 2, 1]);
    }

    #[test]
    fn empty_for_unknown_target() {
        let r = DecoratorRegistry::new();
        assert!(r.evaluation_order("nope").is_empty());
    }

    #[test]
    fn record_multiple_targets() {
        let mut r = DecoratorRegistry::new();
        r.record(DecoratorApplication {
            target: "A".into(),
            kind: DecoratorKind::Method,
            decorator_callback_ids: vec![10],
        });
        r.record(DecoratorApplication {
            target: "B".into(),
            kind: DecoratorKind::Method,
            decorator_callback_ids: vec![20],
        });
        assert_eq!(r.evaluation_order("A"), vec![10]);
        assert_eq!(r.evaluation_order("B"), vec![20]);
    }
}
