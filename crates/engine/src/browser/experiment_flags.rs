//! Experiment / feature-flag rollout (Chromium "field trials" / chrome://flags).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlagState {
    Default,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct ExperimentFlag {
    pub name: String,
    pub state: FlagState,
    pub default_value: bool,
    pub description: String,
    pub origin_trial_name: Option<String>,
}

#[derive(Default)]
pub struct ExperimentRegistry {
    pub flags: HashMap<String, ExperimentFlag>,
    /// User overrides via chrome://flags survive sessions.
    pub overrides: HashMap<String, bool>,
    /// Field trial bucket (per Chromium variations).
    pub field_trial_groups: HashMap<String, String>,
}

impl ExperimentRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, flag: ExperimentFlag) {
        self.flags.insert(flag.name.clone(), flag);
    }

    pub fn set_override(&mut self, name: &str, enabled: bool) {
        self.overrides.insert(name.into(), enabled);
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        if let Some(v) = self.overrides.get(name).copied() { return v; }
        match self.flags.get(name) {
            Some(f) => match f.state {
                FlagState::Enabled => true,
                FlagState::Disabled => false,
                FlagState::Default => f.default_value,
            },
            None => false,
        }
    }

    pub fn assign_trial(&mut self, trial: &str, group: &str) {
        self.field_trial_groups.insert(trial.into(), group.into());
    }

    pub fn trial_group(&self, trial: &str) -> Option<&str> {
        self.field_trial_groups.get(trial).map(|s| s.as_str())
    }
}

/// Assign user to a percentage-based trial bucket using a stable hash.
pub fn pick_trial_group(user_id_hash: u64, weights: &[(&str, f64)]) -> String {
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    if total <= 0.0 { return "control".into(); }
    let mut roll = (user_id_hash % 1_000_000) as f64 / 1_000_000.0 * total;
    for (name, w) in weights {
        if roll < *w { return name.to_string(); }
        roll -= w;
    }
    weights.last().map(|(n, _)| n.to_string()).unwrap_or_else(|| "control".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(name: &str, default: bool) -> ExperimentFlag {
        ExperimentFlag {
            name: name.into(),
            state: FlagState::Default,
            default_value: default,
            description: "".into(),
            origin_trial_name: None,
        }
    }

    #[test]
    fn default_returns_default_value() {
        let mut r = ExperimentRegistry::new();
        r.register(flag("a", true));
        assert!(r.is_enabled("a"));
        r.register(flag("b", false));
        assert!(!r.is_enabled("b"));
    }

    #[test]
    fn override_takes_precedence() {
        let mut r = ExperimentRegistry::new();
        r.register(flag("a", false));
        r.set_override("a", true);
        assert!(r.is_enabled("a"));
    }

    #[test]
    fn unknown_returns_false() {
        let r = ExperimentRegistry::new();
        assert!(!r.is_enabled("nope"));
    }

    #[test]
    fn explicit_state_overrides_default() {
        let mut r = ExperimentRegistry::new();
        let mut f = flag("a", true);
        f.state = FlagState::Disabled;
        r.register(f);
        assert!(!r.is_enabled("a"));
    }

    #[test]
    fn trial_assignment() {
        let weights = [("treatment", 0.5), ("control", 0.5)];
        let a = pick_trial_group(100_000, &weights);
        let b = pick_trial_group(700_000, &weights);
        assert!(a == "treatment" || a == "control");
        assert!(b == "treatment" || b == "control");
    }
}
