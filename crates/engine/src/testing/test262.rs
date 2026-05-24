//! Test262 - official ECMAScript conformance tests.
//!
//! https://github.com/tc39/test262
//! Each .js file has YAML frontmatter (`---` blocks) describing feature flags,
//! includes, and expected outcome (negative tests + error name).

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Test262Frontmatter {
    pub description: String,
    pub features: Vec<String>,
    pub includes: Vec<String>,
    pub flags: Vec<String>,           // module, async, raw, noStrict, onlyStrict, ...
    pub negative: Option<Negative>,
    pub locale: Vec<String>,
    pub author: String,
    pub es5id: Option<String>,
    pub es6id: Option<String>,
    pub esid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Negative {
    pub phase: String,          // "parse" | "resolution" | "runtime"
    pub type_name: String,      // "SyntaxError" | "ReferenceError" | ...
}

pub fn parse_frontmatter(source: &str) -> Option<Test262Frontmatter> {
    let start = source.find("/*---")?;
    let after = &source[start + 5..];
    let end = after.find("---*/")?;
    let yaml = &after[..end];
    let mut fm = Test262Frontmatter::default();
    let mut current_key: Option<String> = None;
    let mut current_block: Option<&str> = None;
    let mut list_items: Vec<String> = Vec::new();
    for raw_line in yaml.lines() {
        let trimmed = raw_line.trim_start_matches(' ').trim_end();
        if trimmed.is_empty() { continue; }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            if let Some(_) = current_key {
                list_items.push(rest.trim_matches('\'').trim_matches('"').to_string());
            }
            continue;
        }
        // flush previous list
        if !list_items.is_empty() {
            if let Some(k) = current_key.as_deref() {
                assign_list(&mut fm, k, std::mem::take(&mut list_items));
            }
        }
        let _ = current_block;
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim().to_string();
            let v = v.trim().trim_matches('\'').trim_matches('"').to_string();
            current_key = Some(k.clone());
            if v.is_empty() {
                continue; // block follows (list or nested map)
            }
            assign_scalar(&mut fm, &k, &v);
        }
    }
    if !list_items.is_empty() {
        if let Some(k) = current_key.as_deref() {
            assign_list(&mut fm, k, list_items);
        }
    }
    Some(fm)
}

fn assign_scalar(fm: &mut Test262Frontmatter, key: &str, value: &str) {
    match key {
        "description" => fm.description = value.into(),
        "author" => fm.author = value.into(),
        "es5id" => fm.es5id = Some(value.into()),
        "es6id" => fm.es6id = Some(value.into()),
        "esid" => fm.esid = Some(value.into()),
        _ => {}
    }
}

fn assign_list(fm: &mut Test262Frontmatter, key: &str, items: Vec<String>) {
    match key {
        "features" => fm.features = items,
        "includes" => fm.includes = items,
        "flags" => fm.flags = items,
        "locale" => fm.locale = items,
        _ => {}
    }
}

/// Stats accumulator for a test262 run.
#[derive(Debug, Clone, Default)]
pub struct Test262Run {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub failures_by_feature: HashMap<String, u64>,
}

impl Test262Run {
    pub fn record_pass(&mut self) { self.total += 1; self.passed += 1; }
    pub fn record_fail(&mut self, features: &[String]) {
        self.total += 1;
        self.failed += 1;
        for f in features {
            *self.failures_by_feature.entry(f.clone()).or_insert(0) += 1;
        }
    }
    pub fn record_skip(&mut self) { self.total += 1; self.skipped += 1; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_frontmatter() {
        let src = "/*---\ndescription: testing\nflags:\n  - module\n---*/\n";
        let fm = parse_frontmatter(src).unwrap();
        assert_eq!(fm.description, "testing");
        assert_eq!(fm.flags, vec!["module"]);
    }

    #[test]
    fn parse_features_list() {
        let src = "/*---\nfeatures:\n  - Promise.any\n  - WeakRef\n---*/\n";
        let fm = parse_frontmatter(src).unwrap();
        assert_eq!(fm.features.len(), 2);
    }

    #[test]
    fn missing_frontmatter() {
        assert!(parse_frontmatter("// no frontmatter\n").is_none());
    }

    #[test]
    fn run_accumulates() {
        let mut r = Test262Run::default();
        r.record_pass();
        r.record_pass();
        r.record_fail(&["a".into()]);
        r.record_skip();
        assert_eq!(r.passed, 2);
        assert_eq!(r.failed, 1);
        assert_eq!(r.skipped, 1);
        assert_eq!(r.failures_by_feature.get("a"), Some(&1));
    }
}
