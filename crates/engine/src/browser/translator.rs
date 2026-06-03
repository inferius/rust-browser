//! Page translation - detect language + queue translation jobs.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TranslationState {
    Idle,
    DetectingLanguage,
    AwaitingUserChoice,
    Translating,
    Translated,
    Failed,
    Reverted,
}

#[derive(Debug, Clone, Default)]
pub struct PageTranslationState {
    pub state: TranslationState,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub user_declined_for_origin: bool,
    pub auto_translate_languages: Vec<String>,
    pub error: Option<String>,
}

impl Default for TranslationState {
    fn default() -> Self { TranslationState::Idle }
}

#[derive(Default)]
pub struct TranslationManager {
    pub per_tab: HashMap<u64, PageTranslationState>,
    pub never_translate_origins: std::collections::HashSet<String>,
    pub never_translate_languages: std::collections::HashSet<String>,
    pub always_translate_to_target: HashMap<String, String>,  // source -> target
}

impl TranslationManager {
    pub fn new() -> Self { Self::default() }

    pub fn detect(&mut self, tab_id: u64, language: &str) {
        let st = self.per_tab.entry(tab_id).or_default();
        st.source_language = Some(language.into());
        st.state = TranslationState::AwaitingUserChoice;
        if self.never_translate_languages.contains(language) {
            st.state = TranslationState::Idle;
        }
        if let Some(target) = self.always_translate_to_target.get(language).cloned() {
            st.target_language = Some(target);
            st.state = TranslationState::Translating;
        }
    }

    pub fn accept(&mut self, tab_id: u64, target: &str) {
        let st = self.per_tab.entry(tab_id).or_default();
        st.target_language = Some(target.into());
        st.state = TranslationState::Translating;
    }

    pub fn complete(&mut self, tab_id: u64) {
        if let Some(st) = self.per_tab.get_mut(&tab_id) {
            st.state = TranslationState::Translated;
        }
    }

    pub fn fail(&mut self, tab_id: u64, reason: &str) {
        if let Some(st) = self.per_tab.get_mut(&tab_id) {
            st.state = TranslationState::Failed;
            st.error = Some(reason.into());
        }
    }

    pub fn revert(&mut self, tab_id: u64) {
        if let Some(st) = self.per_tab.get_mut(&tab_id) {
            st.state = TranslationState::Reverted;
        }
    }

    pub fn always_translate_pair(&mut self, source: &str, target: &str) {
        self.always_translate_to_target.insert(source.into(), target.into());
    }

    pub fn never_translate_language(&mut self, language: &str) {
        self.never_translate_languages.insert(language.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_awaits_user_choice() {
        let mut m = TranslationManager::new();
        m.detect(1, "fr");
        assert_eq!(m.per_tab[&1].state, TranslationState::AwaitingUserChoice);
    }

    #[test]
    fn auto_translate_for_known_pair() {
        let mut m = TranslationManager::new();
        m.always_translate_pair("fr", "en");
        m.detect(1, "fr");
        assert_eq!(m.per_tab[&1].state, TranslationState::Translating);
        assert_eq!(m.per_tab[&1].target_language.as_deref(), Some("en"));
    }

    #[test]
    fn never_translate_skips() {
        let mut m = TranslationManager::new();
        m.never_translate_language("ja");
        m.detect(1, "ja");
        assert_eq!(m.per_tab[&1].state, TranslationState::Idle);
    }

    #[test]
    fn accept_starts_translating() {
        let mut m = TranslationManager::new();
        m.detect(1, "fr");
        m.accept(1, "en");
        assert_eq!(m.per_tab[&1].state, TranslationState::Translating);
    }

    #[test]
    fn complete_marks_translated() {
        let mut m = TranslationManager::new();
        m.detect(1, "fr");
        m.accept(1, "en");
        m.complete(1);
        assert_eq!(m.per_tab[&1].state, TranslationState::Translated);
    }
}
