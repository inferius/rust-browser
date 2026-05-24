//! ES Modules (ESM) - import/export, dynamic import(), import.meta.
//!
//! Spec: https://tc39.es/ecma262/#sec-modules
//!
//! Foundation: module registry + URL resolution. Real loading pres existing
//! interpreter (parse + eval s import bindings).

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModuleState {
    Unlinked,
    Linking,
    Linked,
    Evaluating,
    Evaluated,
    Errored,
}

#[derive(Debug, Clone)]
pub struct ImportSpecifier {
    pub source: String,           // raw "./util.js"
    pub resolved_url: String,     // absolute URL
    pub binding: ImportBinding,
}

#[derive(Debug, Clone)]
pub enum ImportBinding {
    Default(String),                              // import x from "..."
    Named(Vec<(String, String)>),                 // import { a, b as c } - (export, local)
    Namespace(String),                            // import * as ns from "..."
    SideEffect,                                   // import "..."
}

#[derive(Debug)]
pub struct EsmModule {
    pub url: String,
    pub source: String,
    pub state: ModuleState,
    pub imports: Vec<ImportSpecifier>,
    pub exports: Vec<String>,
    /// Bindings: export_name -> JS value placeholder.
    pub binding_values: HashMap<String, String>,
}

#[derive(Default)]
pub struct EsmRegistry {
    pub modules: HashMap<String, Rc<RefCell<EsmModule>>>,
}

impl EsmRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, url: &str, source: &str) -> Rc<RefCell<EsmModule>> {
        let m = Rc::new(RefCell::new(EsmModule {
            url: url.into(),
            source: source.into(),
            state: ModuleState::Unlinked,
            imports: Vec::new(),
            exports: Vec::new(),
            binding_values: HashMap::new(),
        }));
        self.modules.insert(url.into(), Rc::clone(&m));
        m
    }

    pub fn get(&self, url: &str) -> Option<Rc<RefCell<EsmModule>>> {
        self.modules.get(url).cloned()
    }

    /// Resolve import specifier proti base URL. "./x.js" -> base + x.js.
    pub fn resolve(specifier: &str, base_url: &str) -> String {
        if specifier.starts_with("http://") || specifier.starts_with("https://")
            || specifier.starts_with("file://")
        {
            return specifier.to_string();
        }
        if specifier.starts_with("/") {
            // Root-relative.
            if let Some(origin_end) = base_url.find("://").and_then(|i| base_url[i+3..].find('/').map(|j| i + 3 + j)) {
                return format!("{}{}", &base_url[..origin_end], specifier);
            }
        }
        // Relative: drop last segment of base.
        let base = if let Some(idx) = base_url.rfind('/') {
            &base_url[..=idx]
        } else { base_url };
        // Normalize "./" and "../".
        let mut combined = format!("{}{}", base, specifier);
        while combined.contains("/./") {
            combined = combined.replace("/./", "/");
        }
        while combined.contains("/../") {
            if let Some(idx) = combined.find("/../") {
                let before = &combined[..idx];
                let parent_end = before.rfind('/').unwrap_or(0);
                combined = format!("{}{}", &combined[..parent_end], &combined[idx + 3..]);
            }
        }
        combined
    }
}

/// Parse simple import/export statements (foundation, not full ESM grammar).
/// Real impl pres existing JS parser.
pub fn parse_imports(source: &str) -> Vec<ImportSpecifier> {
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if !line.starts_with("import") { continue; }
        // Naive parse: "import X from 'url'" / "import { a, b } from 'url'"
        let from_idx = line.find("from").map(|i| i);
        let (binding_part, url_part) = match from_idx {
            Some(i) => (&line[6..i], &line[i+4..]),
            None => {
                // "import 'url'" - side-effect.
                let raw = line[6..].trim().trim_matches(|c| c == '\'' || c == '"' || c == ';');
                out.push(ImportSpecifier {
                    source: raw.into(),
                    resolved_url: String::new(),
                    binding: ImportBinding::SideEffect,
                });
                continue;
            }
        };
        let url = url_part.trim().trim_matches(|c| c == '\'' || c == '"' || c == ';' || c == ' ');
        let b = binding_part.trim();
        let binding = if b.starts_with('{') {
            let inner = b.trim_matches(|c| c == '{' || c == '}');
            let names: Vec<(String, String)> = inner.split(',').map(|n| {
                let parts: Vec<&str> = n.split("as").map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    (parts[0].to_string(), parts[0].to_string())
                }
            }).collect();
            ImportBinding::Named(names)
        } else if b.starts_with("* as ") {
            ImportBinding::Namespace(b[5..].trim().to_string())
        } else {
            ImportBinding::Default(b.to_string())
        };
        out.push(ImportSpecifier {
            source: url.into(),
            resolved_url: String::new(),
            binding,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative() {
        let r = EsmRegistry::resolve("./util.js", "https://example.com/app/main.js");
        assert_eq!(r, "https://example.com/app/util.js");
    }

    #[test]
    fn resolve_parent() {
        let r = EsmRegistry::resolve("../lib/x.js", "https://example.com/app/main.js");
        assert_eq!(r, "https://example.com/lib/x.js");
    }

    #[test]
    fn resolve_absolute_url() {
        let r = EsmRegistry::resolve("https://cdn.com/lib.js", "https://example.com/app/");
        assert_eq!(r, "https://cdn.com/lib.js");
    }

    #[test]
    fn parse_default_import() {
        let imports = parse_imports("import foo from './foo.js';");
        assert_eq!(imports.len(), 1);
        assert!(matches!(imports[0].binding, ImportBinding::Default(ref n) if n == "foo"));
    }

    #[test]
    fn parse_named_imports() {
        let imports = parse_imports("import { a, b as c } from './lib.js';");
        match &imports[0].binding {
            ImportBinding::Named(names) => {
                assert_eq!(names.len(), 2);
                assert_eq!(names[1], ("b".into(), "c".into()));
            }
            _ => panic!("expected named"),
        }
    }

    #[test]
    fn parse_namespace_import() {
        let imports = parse_imports("import * as ns from './lib.js';");
        assert!(matches!(imports[0].binding, ImportBinding::Namespace(ref n) if n == "ns"));
    }

    #[test]
    fn parse_side_effect_import() {
        let imports = parse_imports("import './setup.js';");
        assert!(matches!(imports[0].binding, ImportBinding::SideEffect));
    }

    #[test]
    fn registry_register_and_get() {
        let mut r = EsmRegistry::new();
        r.register("https://x.com/a.js", "export const x = 1;");
        assert!(r.get("https://x.com/a.js").is_some());
    }
}
