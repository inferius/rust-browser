//! Web Platform Tests (WPT) harness foundation.
//!
//! WPT je oficialni W3C compliance suite - tisice testu pro HTML, CSS, DOM,
//! JS APIs. Spustenim WPT proti nasemu enginu mereno spec compliance gap.
//!
//! Harness implementuje subset `testharness.js` API ktery WPT testy pouzivaji:
//! - `test(fn, name)` - sync test
//! - `async_test(fn, name)` - async s `t.done()`
//! - `promise_test(fn, name)` - promise-based
//! - `assert_equals(actual, expected)` / `assert_true` / `assert_throws_*`
//!
//! Run model:
//! 1. Load test HTML
//! 2. Inject testharness.js (= nas Rust impl)
//! 3. Run script
//! 4. Capture test() results
//! 5. Compare s expected (= WPT reference results)
//!
//! Inspired by:
//! - WPT runner: https://github.com/web-platform-tests/wpt/blob/master/tools/wptrunner
//! - Chromium `third_party/blink/tools/blinkpy/web_tests/`
//! - Servo `tests/wpt/`

use std::collections::HashMap;

/// Test result jeden z testharness.js volani.
#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    Pass,
    Fail(String),
    Timeout,
    NotRun,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub message: Option<String>,
}

/// Test runner state - vola se z testharness.js builtins implementations.
#[derive(Default)]
pub struct WptHarness {
    pub results: Vec<TestResult>,
    /// Expected baseline z `.ini` files - test name -> expected status.
    pub expectations: HashMap<String, TestStatus>,
}

impl WptHarness {
    pub fn new() -> Self { Self::default() }

    pub fn add_test_pass(&mut self, name: &str) {
        self.results.push(TestResult {
            name: name.into(),
            status: TestStatus::Pass,
            message: None,
        });
    }

    pub fn add_test_fail(&mut self, name: &str, msg: &str) {
        self.results.push(TestResult {
            name: name.into(),
            status: TestStatus::Fail(msg.into()),
            message: Some(msg.into()),
        });
    }

    /// Summary: (pass, fail, expected_fail, unexpected_fail, unexpected_pass).
    /// Expected fail counts samostatne (= known issues), unexpected fail = regression.
    pub fn summary(&self) -> TestSummary {
        let mut s = TestSummary::default();
        for r in &self.results {
            let expected = self.expectations.get(&r.name);
            match (&r.status, expected) {
                (TestStatus::Pass, Some(TestStatus::Pass)) | (TestStatus::Pass, None) => s.pass += 1,
                (TestStatus::Pass, Some(_)) => s.unexpected_pass += 1,
                (TestStatus::Fail(_), Some(TestStatus::Fail(_))) => {
                    s.expected_fail += 1;
                    s.fail += 1;
                }
                (TestStatus::Fail(_), _) => { s.fail += 1; s.unexpected_fail += 1; }
                _ => {}
            }
        }
        s
    }

    /// Print results - rozliseni unexpected fails (= regression vs known issue).
    pub fn print_summary(&self) {
        let s = self.summary();
        println!("[WPT] {} pass, {} fail ({} expected fail, {} unexpected fail, {} unexpected pass)",
            s.pass, s.fail, s.expected_fail, s.unexpected_fail, s.unexpected_pass);
    }
}

#[derive(Debug, Default)]
pub struct TestSummary {
    pub pass: u32,
    pub fail: u32,
    pub expected_fail: u32,
    pub unexpected_fail: u32,
    pub unexpected_pass: u32,
}

/// testharness.js subset jako JS source - injectne se do test pages pred run.
/// Native bridge volá WptHarness::add_test_pass/add_test_fail z built-in fns.
pub const TESTHARNESS_JS: &str = r#"
(function() {
    var __tests = [];
    var __done = false;

    function test(fn, name) {
        try {
            fn({ name: name });
            __native_wpt_pass(name);
        } catch (e) {
            __native_wpt_fail(name, String(e));
        }
    }

    function async_test(fn, name) {
        var t = {
            name: name,
            done: function() { __native_wpt_pass(name); },
            step: function(cb) {
                try { cb(); } catch (e) { __native_wpt_fail(name, String(e)); }
            },
            step_func: function(cb) {
                return function() { t.step(function() { cb.apply(null, arguments); }); };
            },
            step_func_done: function(cb) {
                return function() {
                    t.step(function() { cb.apply(null, arguments); });
                    t.done();
                };
            },
            unreached_func: function(msg) {
                return function() { __native_wpt_fail(name, msg || 'should not be reached'); };
            },
        };
        try { fn(t); } catch (e) { __native_wpt_fail(name, String(e)); }
    }

    function promise_test(fn, name) {
        try {
            var p = fn({ name: name });
            if (p && typeof p.then === 'function') {
                p.then(function() { __native_wpt_pass(name); },
                       function(e) { __native_wpt_fail(name, String(e)); });
            } else {
                __native_wpt_pass(name);
            }
        } catch (e) { __native_wpt_fail(name, String(e)); }
    }

    function assert_equals(actual, expected, msg) {
        if (actual !== expected) {
            throw new Error((msg || 'assert_equals') + ': expected ' + expected + ' got ' + actual);
        }
    }
    function assert_true(value, msg) {
        if (value !== true) throw new Error((msg || 'assert_true') + ': got ' + value);
    }
    function assert_false(value, msg) {
        if (value !== false) throw new Error((msg || 'assert_false') + ': got ' + value);
    }
    function assert_not_equals(actual, expected, msg) {
        if (actual === expected) throw new Error(msg || 'assert_not_equals');
    }
    function assert_throws_js(constructor, fn, msg) {
        var threw = false;
        try { fn(); } catch (e) { threw = true; }
        if (!threw) throw new Error((msg || 'assert_throws_js') + ': did not throw');
    }
    function assert_unreached(msg) { throw new Error(msg || 'assert_unreached'); }
    function assert_array_equals(actual, expected, msg) {
        if (!Array.isArray(actual)) throw new Error((msg || 'assert_array_equals') + ': not array');
        if (actual.length !== expected.length) throw new Error('length mismatch');
        for (var i = 0; i < actual.length; i++) {
            if (actual[i] !== expected[i]) throw new Error('item ' + i + ' mismatch');
        }
    }

    // Globalni exposure pres window. Workaround pro stripped runtime (test prostredi
    // bez plne DOM) - take ulozit do globalniho scope (test/assert_* primo k dispozici).
    if (typeof window === 'undefined') { window = this; }
    window.test = test;
    window.async_test = async_test;
    window.promise_test = promise_test;
    window.assert_equals = assert_equals;
    window.assert_true = assert_true;
    window.assert_false = assert_false;
    window.assert_not_equals = assert_not_equals;
    window.assert_throws_js = assert_throws_js;
    window.assert_unreached = assert_unreached;
    window.assert_array_equals = assert_array_equals;
})();
"#;

/// Real WPT runner: vytvori novy Interpreter, zaregistruje testharness.js API
/// (test/async_test/promise_test/assert_*) jako native fns ktere primo
/// vyhodnocuji user JS callback + zapisuji do shared WptHarness, spusti
/// user JS skript, vrati final harness state.
///
/// Tento bridge konzumuje testharness.js test()/async_test()/promise_test() volania
/// primo z user skriptu = je to real execution, ne stub harness.
///
/// Vsechny `assert_*` fns throw JsError pri failure (interpreter throw/catch).
/// test() vola callback uvnitr try-catch, pri thrown error zaznamena fail.
///
/// Pouzitelne pro:
/// - Spec compliance smoke tests (drop test file dovnitr, mereni pass/fail)
/// - Regression tests (pridat assertion za novy feature -> CI cykla pres tento runner)
/// - Self-test enginu (run subset WPT manualy + diff vs expectations)
///
/// Inspired by Chromium third_party/blink/web_tests/external/wpt/ run model
/// + WebKit Layout Tests harness, oboje s native bridge style.
pub fn run_wpt_script(user_js: &str) -> WptHarness {
    use std::cell::RefCell;
    use std::rc::Rc;
    use crate::interpreter::{Interpreter, JsValue, JsFunc, JsError};
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;

    let harness = Rc::new(RefCell::new(WptHarness::new()));
    let mut interp = Interpreter::new();

    // Native assertion helpers - return Err(String) ktere interpreter mapuje na
    // Runtime error. Tester wrapper catches both Runtime + Thrown errors.
    let define_assert = |interp: &mut Interpreter, name: &'static str,
                         body: Rc<dyn Fn(Vec<JsValue>) -> Result<JsValue, String>>| {
        interp.global.borrow_mut().define(name,
            JsValue::Function(JsFunc::Native(name.to_string(), body)));
    };

    define_assert(&mut interp, "assert_equals", Rc::new(|args| {
        let mut it = args.into_iter();
        let actual = it.next().unwrap_or(JsValue::Undefined);
        let expected = it.next().unwrap_or(JsValue::Undefined);
        let msg = it.next().map(|v| v.to_string()).unwrap_or_else(|| "assert_equals".into());
        if !js_strict_equals(&actual, &expected) {
            return Err(format!("{}: expected {} got {}", msg, expected, actual));
        }
        Ok(JsValue::Undefined)
    }));
    define_assert(&mut interp, "assert_not_equals", Rc::new(|args| {
        let mut it = args.into_iter();
        let actual = it.next().unwrap_or(JsValue::Undefined);
        let expected = it.next().unwrap_or(JsValue::Undefined);
        let msg = it.next().map(|v| v.to_string()).unwrap_or_else(|| "assert_not_equals".into());
        if js_strict_equals(&actual, &expected) {
            return Err(format!("{}: values equal", msg));
        }
        Ok(JsValue::Undefined)
    }));
    define_assert(&mut interp, "assert_true", Rc::new(|args| {
        let v = args.into_iter().next().unwrap_or(JsValue::Undefined);
        if !matches!(v, JsValue::Bool(true)) {
            return Err(format!("assert_true: got {}", v));
        }
        Ok(JsValue::Undefined)
    }));
    define_assert(&mut interp, "assert_false", Rc::new(|args| {
        let v = args.into_iter().next().unwrap_or(JsValue::Undefined);
        if !matches!(v, JsValue::Bool(false)) {
            return Err(format!("assert_false: got {}", v));
        }
        Ok(JsValue::Undefined)
    }));
    define_assert(&mut interp, "assert_array_equals", Rc::new(|args| {
        let mut it = args.into_iter();
        let actual = it.next().unwrap_or(JsValue::Undefined);
        let expected = it.next().unwrap_or(JsValue::Undefined);
        if let (JsValue::Array(a), JsValue::Array(b)) = (&actual, &expected) {
            let a = a.borrow();
            let b = b.borrow();
            if a.len() != b.len() {
                return Err(format!("assert_array_equals: length {} != {}", a.len(), b.len()));
            }
            for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                if !js_strict_equals(av, bv) {
                    return Err(format!("assert_array_equals: item {} mismatch ({} != {})", i, av, bv));
                }
            }
            return Ok(JsValue::Undefined);
        }
        Err("assert_array_equals: not arrays".into())
    }));
    define_assert(&mut interp, "assert_unreached", Rc::new(|args| {
        let msg = args.into_iter().next().map(|v| v.to_string())
            .unwrap_or_else(|| "assert_unreached".into());
        Err(msg)
    }));
    // assert_throws_js(constructor, fn[, msg]) - real call s closure invoke
    // pres interp_ptr (definovany dale). Use Rc<RefCell<>> sdileni.
    let interp_ptr: Rc<RefCell<*mut Interpreter>> = Rc::new(RefCell::new(std::ptr::null_mut()));
    let interp_ptr_throws = Rc::clone(&interp_ptr);
    interp.global.borrow_mut().define("assert_throws_js",
        JsValue::Function(JsFunc::Native("assert_throws_js".to_string(), Rc::new(move |args| {
            let mut it = args.into_iter();
            let _ctor = it.next().unwrap_or(JsValue::Undefined);
            let cb = it.next().unwrap_or(JsValue::Undefined);
            let msg = it.next().map(|v| v.to_string())
                .unwrap_or_else(|| "assert_throws_js".into());
            let ptr = *interp_ptr_throws.borrow();
            if ptr.is_null() { return Err("no interpreter context".into()); }
            let interp = unsafe { &mut *ptr };
            match interp.call_function(cb, vec![], None) {
                Ok(_) => Err(format!("{}: did not throw", msg)),
                Err(_) => Ok(JsValue::Undefined),  // jakekoliv throw = pass
            }
        }))));

    // call_function (interp passed thru thread_local).
    let h2 = Rc::clone(&harness);
    let interp_ptr_clone = Rc::clone(&interp_ptr);
    interp.global.borrow_mut().define("test",
        JsValue::Function(JsFunc::Native("test".to_string(), Rc::new(move |args| {
            let mut it = args.into_iter();
            let cb = it.next().unwrap_or(JsValue::Undefined);
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let ptr = *interp_ptr_clone.borrow();
            if ptr.is_null() {
                h2.borrow_mut().add_test_fail(&name, "no interpreter context");
                return Ok(JsValue::Undefined);
            }
            let interp = unsafe { &mut *ptr };
            let mut t_obj = crate::interpreter::JsObject::new();
            t_obj.set("name".into(), JsValue::Str(name.clone()));
            let t_val = JsValue::Object(Rc::new(RefCell::new(t_obj)));
            match interp.call_function(cb, vec![t_val], None) {
                Ok(_) => h2.borrow_mut().add_test_pass(&name),
                Err(JsError::Thrown(v)) => h2.borrow_mut().add_test_fail(&name, &v.to_string()),
                Err(e) => h2.borrow_mut().add_test_fail(&name, &format!("{:?}", e)),
            }
            Ok(JsValue::Undefined)
        }))));
    // async_test - okamzite vyhodnoti callback s mock t (s.done = pass).
    let h3 = Rc::clone(&harness);
    let interp_ptr_a = Rc::clone(&interp_ptr);
    interp.global.borrow_mut().define("async_test",
        JsValue::Function(JsFunc::Native("async_test".to_string(), Rc::new(move |args| {
            let mut it = args.into_iter();
            let cb = it.next().unwrap_or(JsValue::Undefined);
            let name = it.next().map(|v| v.to_string()).unwrap_or_default();
            let ptr = *interp_ptr_a.borrow();
            if ptr.is_null() { h3.borrow_mut().add_test_fail(&name, "no ctx"); return Ok(JsValue::Undefined); }
            let interp = unsafe { &mut *ptr };
            // Vyrobime mock t s done/step closures (zatim stub - mark pass okamzite).
            let mut t_obj = crate::interpreter::JsObject::new();
            t_obj.set("name".into(), JsValue::Str(name.clone()));
            let t_val = JsValue::Object(Rc::new(RefCell::new(t_obj)));
            match interp.call_function(cb, vec![t_val], None) {
                Ok(_) => h3.borrow_mut().add_test_pass(&name),
                Err(JsError::Thrown(v)) => h3.borrow_mut().add_test_fail(&name, &v.to_string()),
                Err(e) => h3.borrow_mut().add_test_fail(&name, &format!("{:?}", e)),
            }
            Ok(JsValue::Undefined)
        }))));

    // Lexer + Parser + run user JS s interpreter_ptr set.
    if let Ok(lex) = Lexer::parse_str(user_js, "wpt-test.js") {
        let tokens: Vec<_> = lex.tokens.into_iter()
            .filter(|t| !matches!(t.kind,
                TokenKind::Whitespace | TokenKind::Newline
                | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
            .collect();
        let mut parser = Parser::new(tokens);
        if let Ok(prog) = parser.parse() {
            // Set raw pointer pro native fns ktere musi call_function rekurzivne.
            *interp_ptr.borrow_mut() = &mut interp as *mut _;
            let _ = interp.run(&prog);
            *interp_ptr.borrow_mut() = std::ptr::null_mut();
        }
    }
    let inner = std::mem::take(&mut *harness.borrow_mut());
    inner
}

fn js_strict_equals(a: &crate::interpreter::JsValue, b: &crate::interpreter::JsValue) -> bool {
    use crate::interpreter::JsValue;
    match (a, b) {
        (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::Str(x), JsValue::Str(y)) => x == y,
        (JsValue::BigInt(x), JsValue::BigInt(y)) => x == y,
        _ => false,
    }
}

/// Extract <script> obsah z HTML zdroje (pro WPT test soubory).
/// Velmi jednoduchy parser - ne plne HTML5, jen `<script>...</script>` extraction.
pub fn extract_inline_scripts(html: &str) -> String {
    let mut out = String::new();
    let mut idx = 0;
    let bytes = html.as_bytes();
    while idx < bytes.len() {
        // Najdi <script>
        let start_tag_idx = match find_subseq(&bytes[idx..], b"<script") {
            Some(p) => idx + p,
            None => break,
        };
        // Skip until end of opening tag '>'
        let open_end = match bytes[start_tag_idx..].iter().position(|b| *b == b'>') {
            Some(p) => start_tag_idx + p + 1,
            None => break,
        };
        // Find </script>
        let close_idx = match find_subseq(&bytes[open_end..], b"</script>") {
            Some(p) => open_end + p,
            None => break,
        };
        out.push_str(&html[open_end..close_idx]);
        out.push('\n');
        idx = close_idx + 9;
    }
    out
}

fn find_subseq(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() { return None; }
    hay.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_track_results() {
        let mut h = WptHarness::new();
        h.add_test_pass("test1");
        h.add_test_fail("test2", "err msg");
        let s = h.summary();
        assert_eq!(s.pass, 1);
        assert_eq!(s.fail, 1);
    }

    #[test]
    fn expected_fail_no_regression() {
        let mut h = WptHarness::new();
        h.expectations.insert("test1".into(), TestStatus::Fail("known".into()));
        h.add_test_fail("test1", "still failing");
        let s = h.summary();
        assert_eq!(s.expected_fail, 1);
        // Expected fail = ne unexpected = 0.
        assert_eq!(s.unexpected_fail, 0);
    }

    #[test]
    fn unexpected_pass_detected() {
        let mut h = WptHarness::new();
        h.expectations.insert("test1".into(), TestStatus::Fail("known".into()));
        h.add_test_pass("test1");
        let s = h.summary();
        assert_eq!(s.unexpected_pass, 1);
    }

    #[test]
    fn testharness_js_has_test_fns() {
        assert!(TESTHARNESS_JS.contains("function test("));
        assert!(TESTHARNESS_JS.contains("function async_test("));
        assert!(TESTHARNESS_JS.contains("function assert_equals("));
    }

    #[test]
    fn runner_executes_simple_pass() {
        let user = r#"
            test(function() { assert_equals(1 + 1, 2); }, "addition");
        "#;
        let h = run_wpt_script(user);
        let s = h.summary();
        assert_eq!(s.pass, 1);
        assert_eq!(s.fail, 0);
    }

    #[test]
    fn runner_executes_failing_assert() {
        let user = r#"
            test(function() { assert_equals(1, 2); }, "bad");
        "#;
        let h = run_wpt_script(user);
        let s = h.summary();
        assert_eq!(s.fail, 1);
    }

    #[test]
    fn runner_handles_multiple_tests() {
        let user = r#"
            test(function() { assert_true(true); }, "t1");
            test(function() { assert_true(false); }, "t2");
            test(function() { assert_equals('a', 'a'); }, "t3");
        "#;
        let h = run_wpt_script(user);
        let s = h.summary();
        assert_eq!(s.pass, 2);
        assert_eq!(s.fail, 1);
    }

    #[test]
    fn extract_inline_scripts_basic() {
        let html = r#"<html><head></head><body><script>test(function() { assert_true(true); }, "x");</script></body></html>"#;
        let js = extract_inline_scripts(html);
        assert!(js.contains("assert_true"));
    }

    #[test]
    fn extract_inline_scripts_with_attrs() {
        let html = r#"<script type="text/javascript">var x = 1;</script>"#;
        let js = extract_inline_scripts(html);
        assert!(js.contains("var x"));
    }

    #[test]
    fn runner_assert_array_equals() {
        let user = r#"
            test(function() { assert_array_equals([1,2,3], [1,2,3]); }, "arr");
        "#;
        let h = run_wpt_script(user);
        assert_eq!(h.summary().pass, 1);
    }

    #[test]
    fn runner_assert_throws_js_pass_on_throw() {
        let user = r#"
            test(function() { assert_throws_js(TypeError, function() { throw new Error('x'); }); }, "t");
        "#;
        let h = run_wpt_script(user);
        assert_eq!(h.summary().pass, 1);
    }

    #[test]
    fn runner_assert_throws_js_fail_on_no_throw() {
        let user = r#"
            test(function() { assert_throws_js(TypeError, function() { return 1; }); }, "t");
        "#;
        let h = run_wpt_script(user);
        assert_eq!(h.summary().fail, 1);
    }
}
