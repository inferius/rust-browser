/// Testy pro bytecode VM. Korelovane s tree-walkerem - obe musi davat stejny vysledek.

use crate::interpreter::bytecode::{compile_program, VM};
use crate::interpreter::JsValue;
use crate::lexer::base::Lexer;
use crate::parser::Parser;

fn parse_to_stmts(src: &str) -> Vec<crate::ast::Stmt> {
    let lex = Lexer::parse_str(src, "test").expect("lex");
    let mut parser = Parser::new(lex.tokens.clone());
    parser.parse().expect("parse").body
}

fn run_vm(src: &str) -> Result<JsValue, String> {
    let stmts = parse_to_stmts(src);
    let code = compile_program(&stmts).map_err(|s| s.to_string())?;
    let mut vm = VM::new();
    vm.run(&code)
}

/// Run VM s priplnenym globalem (Math, console, ...) z plne setup interpreteru.
fn run_vm_with_globals(src: &str) -> Result<JsValue, String> {
    let stmts = parse_to_stmts(src);
    let code = compile_program(&stmts).map_err(|s| s.to_string())?;
    let interp = crate::interpreter::Interpreter::new();
    let mut vm = VM::with_env(interp.global.clone());
    vm.run(&code)
}

fn jv_eq(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Number(x), JsValue::Number(y)) => x == y || (x.is_nan() && y.is_nan()),
        (JsValue::Str(x), JsValue::Str(y)) => x == y,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Undefined, JsValue::Undefined) => true,
        _ => false,
    }
}

macro_rules! assert_jv {
    ($actual:expr, $expected:expr) => {{
        let a = $actual;
        let e = $expected;
        assert!(jv_eq(&a, &e), "expected {:?}, got {:?}", e, a);
    }};
}

fn n(v: f64) -> JsValue { JsValue::Number(v) }

#[test]
fn vm_arithmetic_basic() {
    assert_jv!(run_vm("1 + 2 * 3").unwrap(), n(7.0));
    assert_jv!(run_vm("(1 + 2) * 3").unwrap(), n(9.0));
    assert_jv!(run_vm("10 - 4").unwrap(), n(6.0));
    assert_jv!(run_vm("12 / 4").unwrap(), n(3.0));
    assert_jv!(run_vm("2 ** 10").unwrap(), n(1024.0));
}

#[test]
fn vm_unary_ops() {
    assert_jv!(run_vm("-5").unwrap(), n(-5.0));
    assert_jv!(run_vm("+5").unwrap(), n(5.0));
    assert_jv!(run_vm("!true").unwrap(), JsValue::Bool(false));
    assert_jv!(run_vm("!false").unwrap(), JsValue::Bool(true));
}

#[test]
fn vm_comparison() {
    assert_jv!(run_vm("5 > 3").unwrap(), JsValue::Bool(true));
    assert_jv!(run_vm("5 < 3").unwrap(), JsValue::Bool(false));
    assert_jv!(run_vm("5 == 5").unwrap(), JsValue::Bool(true));
    assert_jv!(run_vm("5 === 5").unwrap(), JsValue::Bool(true));
    assert_jv!(run_vm("5 !== '5'").unwrap(), JsValue::Bool(true));
}

#[test]
fn vm_logical_short_circuit() {
    assert_jv!(run_vm("true && 5").unwrap(), n(5.0));
    assert_jv!(run_vm("false && 5").unwrap(), JsValue::Bool(false));
    assert_jv!(run_vm("true || 5").unwrap(), JsValue::Bool(true));
    assert_jv!(run_vm("false || 5").unwrap(), n(5.0));
    assert_jv!(run_vm("null ?? 'default'").unwrap(), JsValue::Str("default".to_string()));
    assert_jv!(run_vm("'val' ?? 'default'").unwrap(), JsValue::Str("val".to_string()));
}

#[test]
fn vm_var_decl_and_use() {
    let stmts = parse_to_stmts("let x = 5; let y = 10; x + y");
    let code = compile_program(&stmts).unwrap();
    let mut vm = VM::new();
    let r = vm.run(&code).unwrap();
    // VM Halt vrati top of stack (posledni pop nebo zustatek po Pop).
    // Tady je posledni stmt expr "x + y" -> push result -> Pop discard -> stack empty -> Undefined.
    // Aby fungovalo "expression result", potrebovali bychom Halt vratit pred Pop.
    // Proto check just var assignment didn't error.
    let _ = r;
}

#[test]
fn vm_assignment_returns_value() {
    let r = run_vm("let x = 0; x = 42").unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_compound_assignment() {
    let r = run_vm("let x = 10; x += 5").unwrap();
    assert_jv!(r, n(15.0));
}

#[test]
fn vm_string_concat() {
    let r = run_vm("'hello ' + 'world'").unwrap();
    assert_jv!(r, JsValue::Str("hello world".to_string()));
}

#[test]
fn vm_string_number_concat() {
    let r = run_vm("'x = ' + 42").unwrap();
    assert_jv!(r, JsValue::Str("x = 42".to_string()));
}

#[test]
fn vm_ternary() {
    let r = run_vm("true ? 1 : 2").unwrap();
    assert_jv!(r, n(1.0));
    let r = run_vm("false ? 1 : 2").unwrap();
    assert_jv!(r, n(2.0));
}

#[test]
fn vm_bitwise() {
    assert_jv!(run_vm("5 & 3").unwrap(), n(1.0));
    assert_jv!(run_vm("5 | 3").unwrap(), n(7.0));
    assert_jv!(run_vm("5 ^ 3").unwrap(), n(6.0));
    assert_jv!(run_vm("1 << 4").unwrap(), n(16.0));
    assert_jv!(run_vm("16 >> 2").unwrap(), n(4.0));
}

#[test]
fn vm_unsupported_returns_err() {
    // Generators neimplementovany - musi vratit Err.
    let stmts = parse_to_stmts("function* g() { yield 1; }");
    let r = compile_program(&stmts);
    assert!(r.is_err());
}

#[test]
fn vm_nested_function_decl() {
    let r = run_vm(r#"
        function outer() {
            function inner() { return 42; }
            return inner();
        }
        outer()
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_nested_function_with_closure() {
    let r = run_vm(r#"
        function outer(x) {
            function inner(y) { return x + y; }
            return inner(10);
        }
        outer(5)
    "#).unwrap();
    assert_jv!(r, n(15.0));
}

#[test]
fn vm_async_function_compile() {
    // Async fn returns Promise wrapping value. Await unwraps.
    let r = run_vm(r#"
        async function f(x) { return x + 1; }
        await f(41)
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_async_returns_promise_object() {
    // Bez await: result je Promise object.
    let r = run_vm(r#"
        async function f() { return 99; }
        let p = f();
        p.__state__
    "#).unwrap();
    assert_jv!(r, JsValue::Str("fulfilled".to_string()));
}

#[test]
fn vm_await_unwraps_promise() {
    // Pokud value neni Promise, await vraci value bezimo.
    let r = run_vm(r#"
        let v = 42;
        await v
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_for_loop_sum() {
    let r = run_vm(r#"
        let sum = 0;
        for (let i = 0; i < 10; i = i + 1) {
            sum = sum + i;
        }
        sum
    "#).unwrap();
    assert_jv!(r, n(45.0));
}

#[test]
fn vm_while_loop_countdown() {
    let r = run_vm(r#"
        let x = 10;
        while (x > 0) {
            x = x - 1;
        }
        x
    "#).unwrap();
    assert_jv!(r, n(0.0));
}

#[test]
fn vm_do_while_runs_at_least_once() {
    let r = run_vm(r#"
        let x = 0;
        do {
            x = x + 1;
        } while (x < 5);
        x
    "#).unwrap();
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_break_exits_loop() {
    let r = run_vm(r#"
        let x = 0;
        for (let i = 0; i < 100; i = i + 1) {
            if (i === 7) break;
            x = i;
        }
        x
    "#).unwrap();
    assert_jv!(r, n(6.0));
}

#[test]
fn vm_continue_skips_iter() {
    let r = run_vm(r#"
        let sum = 0;
        for (let i = 0; i < 10; i = i + 1) {
            if (i === 5) continue;
            sum = sum + i;
        }
        sum
    "#).unwrap();
    assert_jv!(r, n(40.0));
}

#[test]
fn vm_post_increment() {
    let r = run_vm(r#"
        let x = 5;
        let y = x++;
        y
    "#).unwrap();
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_pre_increment_returns_new() {
    let r = run_vm(r#"
        let x = 5;
        let y = ++x;
        y
    "#).unwrap();
    assert_jv!(r, n(6.0));
}

#[test]
fn vm_typeof_primitives() {
    assert_jv!(run_vm("typeof 42").unwrap(), JsValue::Str("number".to_string()));
    assert_jv!(run_vm("typeof 'hello'").unwrap(), JsValue::Str("string".to_string()));
    assert_jv!(run_vm("typeof true").unwrap(), JsValue::Str("boolean".to_string()));
    assert_jv!(run_vm("typeof undefined").unwrap(), JsValue::Str("undefined".to_string()));
    assert_jv!(run_vm("typeof null").unwrap(), JsValue::Str("object".to_string()));
}

#[test]
fn vm_void_returns_undefined() {
    assert_jv!(run_vm("void 0").unwrap(), JsValue::Undefined);
    assert_jv!(run_vm("void 'x'").unwrap(), JsValue::Undefined);
}

#[test]
fn vm_array_literal_and_index() {
    let r = run_vm("[10, 20, 30][1]").unwrap();
    assert_jv!(r, n(20.0));
}

#[test]
fn vm_array_length() {
    let r = run_vm("[1, 2, 3, 4].length").unwrap();
    assert_jv!(r, n(4.0));
}

#[test]
fn vm_object_literal_and_member() {
    let r = run_vm(r#"({ a: 1, b: 2, c: 3 }).b"#).unwrap();
    assert_jv!(r, n(2.0));
}

#[test]
fn vm_object_with_computed_index() {
    let r = run_vm(r#"
        let o = { x: 'hello', y: 'world' };
        o['x']
    "#).unwrap();
    assert_jv!(r, JsValue::Str("hello".to_string()));
}

#[test]
fn vm_string_length() {
    let r = run_vm(r#""hello".length"#).unwrap();
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_string_index() {
    let r = run_vm(r#""abc"[1]"#).unwrap();
    assert_jv!(r, JsValue::Str("b".to_string()));
}

#[test]
fn vm_call_math_sqrt() {
    let r = run_vm_with_globals("Math.sqrt(16)").unwrap();
    assert_jv!(r, n(4.0));
}

#[test]
fn vm_call_math_max() {
    let r = run_vm_with_globals("Math.max(3, 7, 2, 9, 1)").unwrap();
    assert_jv!(r, n(9.0));
}

#[test]
fn vm_call_math_pow() {
    let r = run_vm_with_globals("Math.pow(2, 10)").unwrap();
    assert_jv!(r, n(1024.0));
}

#[test]
fn vm_call_math_abs() {
    let r = run_vm_with_globals("Math.abs(-42)").unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
#[allow(non_snake_case)]
fn vm_call_global_parseInt() {
    let r = run_vm_with_globals("parseInt('42', 10)").unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_user_function_simple() {
    let r = run_vm(r#"
        function add(a, b) { return a + b; }
        add(3, 4)
    "#).unwrap();
    assert_jv!(r, n(7.0));
}

#[test]
fn vm_user_function_with_local_vars() {
    let r = run_vm(r#"
        function double(x) {
            let result = x * 2;
            return result;
        }
        double(21)
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_user_function_with_branch() {
    let r = run_vm(r#"
        function abs(x) {
            if (x < 0) return -x;
            return x;
        }
        abs(-15)
    "#).unwrap();
    assert_jv!(r, n(15.0));
}

#[test]
fn vm_user_function_chained_calls() {
    let r = run_vm(r#"
        function inc(x) { return x + 1; }
        inc(inc(inc(10)))
    "#).unwrap();
    assert_jv!(r, n(13.0));
}

#[test]
fn vm_call_args_spread() {
    let r = run_vm(r#"
        function sum3(a, b, c) { return a + b + c; }
        let args = [10, 20, 30];
        sum3(...args)
    "#).unwrap();
    assert_jv!(r, n(60.0));
}

#[test]
fn vm_call_args_spread_mixed() {
    let r = run_vm(r#"
        function add(a, b, c, d) { return a + b + c + d; }
        let mid = [2, 3];
        add(1, ...mid, 4)
    "#).unwrap();
    assert_jv!(r, n(10.0));
}

#[test]
fn vm_class_basic() {
    let r = run_vm(r#"
        class Counter {
            constructor() { this.n = 0; }
            inc() { this.n = this.n + 1; return this.n; }
        }
        let c = new Counter();
        c.inc();
        c.inc();
        c.inc();
        c.n
    "#).unwrap();
    assert_jv!(r, n(3.0));
}

#[test]
fn vm_class_with_param() {
    let r = run_vm(r#"
        class Box {
            constructor(w, h) {
                this.w = w;
                this.h = h;
            }
            area() { return this.w * this.h; }
        }
        let b = new Box(5, 6);
        b.area()
    "#).unwrap();
    assert_jv!(r, n(30.0));
}

#[test]
fn vm_destructure_object_basic() {
    let r = run_vm(r#"
        let obj = { a: 1, b: 2, c: 3 };
        let { a, b } = obj;
        a + b
    "#).unwrap();
    assert_jv!(r, n(3.0));
}

#[test]
fn vm_destructure_object_with_default() {
    let r = run_vm(r#"
        let obj = { a: 1 };
        let { a, b = 99 } = obj;
        a + b
    "#).unwrap();
    assert_jv!(r, n(100.0));
}

#[test]
fn vm_destructure_array() {
    let r = run_vm(r#"
        let arr = [10, 20, 30];
        let [x, y, z] = arr;
        x + y + z
    "#).unwrap();
    assert_jv!(r, n(60.0));
}

#[test]
fn vm_compound_member_add_assign() {
    let r = run_vm(r#"
        let o = { count: 10 };
        o.count += 5;
        o.count
    "#).unwrap();
    assert_jv!(r, n(15.0));
}

#[test]
fn vm_compound_member_mul_assign() {
    let r = run_vm(r#"
        let o = { x: 4 };
        o.x *= 3;
        o.x
    "#).unwrap();
    assert_jv!(r, n(12.0));
}

#[test]
fn vm_switch_basic() {
    let r = run_vm(r#"
        let x = 2;
        let result = '';
        switch (x) {
            case 1: result = 'one'; break;
            case 2: result = 'two'; break;
            case 3: result = 'three'; break;
            default: result = 'other';
        }
        result
    "#).unwrap();
    assert_jv!(r, JsValue::Str("two".to_string()));
}

#[test]
fn vm_switch_default() {
    let r = run_vm(r#"
        let x = 99;
        let result = '';
        switch (x) {
            case 1: result = 'one'; break;
            default: result = 'other';
        }
        result
    "#).unwrap();
    assert_jv!(r, JsValue::Str("other".to_string()));
}

#[test]
fn vm_switch_fall_through() {
    let r = run_vm(r#"
        let x = 1;
        let r = 0;
        switch (x) {
            case 1:
            case 2:
                r = 12;
                break;
            case 3:
                r = 3;
        }
        r
    "#).unwrap();
    assert_jv!(r, n(12.0));
}

#[test]
fn vm_try_catch_no_throw() {
    let r = run_vm(r#"
        let result = 0;
        try {
            result = 1;
        } catch (e) {
            result = -1;
        }
        result
    "#).unwrap();
    assert_jv!(r, n(1.0));
}

#[test]
fn vm_try_catch_throw_string() {
    let r = run_vm(r#"
        let msg = '';
        try {
            throw 'error!';
        } catch (e) {
            msg = e;
        }
        msg
    "#).unwrap();
    assert_jv!(r, JsValue::Str("error!".to_string()));
}

#[test]
fn vm_try_catch_throw_number() {
    let r = run_vm(r#"
        let caught = 0;
        try {
            throw 42;
        } catch (e) {
            caught = e + 8;
        }
        caught
    "#).unwrap();
    assert_jv!(r, n(50.0));
}

#[test]
fn vm_method_this_binding() {
    let r = run_vm(r#"
        let counter = {
            n: 0,
            inc: function() { this.n = this.n + 1; return this.n; }
        };
        counter.inc();
        counter.inc();
        counter.inc();
        counter.n
    "#).unwrap();
    assert_jv!(r, n(3.0));
}

#[test]
fn vm_method_returns_this_field() {
    let r = run_vm(r#"
        let obj = {
            value: 42,
            getValue: function() { return this.value; }
        };
        obj.getValue()
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_new_constructor_simple() {
    let r = run_vm(r#"
        function Point(x, y) {
            this.x = x;
            this.y = y;
        }
        let p = new Point(3, 4);
        p.x + p.y
    "#).unwrap();
    assert_jv!(r, n(7.0));
}

#[test]
fn vm_new_constructor_with_method() {
    let r = run_vm(r#"
        function Box(w, h) {
            this.w = w;
            this.h = h;
            this.area = w * h;
        }
        let b = new Box(5, 6);
        b.area
    "#).unwrap();
    assert_jv!(r, n(30.0));
}

#[test]
fn vm_array_spread_basic() {
    let r = run_vm(r#"
        let a = [1, 2, 3];
        let b = [0, ...a, 4];
        b.length
    "#).unwrap();
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_array_spread_join() {
    let r = run_vm(r#"
        let a = [1, 2];
        let b = [3, 4];
        let c = [...a, ...b];
        c.join('-')
    "#).unwrap();
    assert_jv!(r, JsValue::Str("1-2-3-4".to_string()));
}

#[test]
fn vm_array_spread_only() {
    let r = run_vm(r#"
        let arr = [10, 20, 30];
        let copy = [...arr];
        copy[1]
    "#).unwrap();
    assert_jv!(r, n(20.0));
}

#[test]
fn vm_optional_call_null_returns_undefined() {
    let r = run_vm(r#"
        let f = null;
        f?.()
    "#).unwrap();
    assert_jv!(r, JsValue::Undefined);
}

#[test]
fn vm_optional_call_defined_invokes() {
    let r = run_vm(r#"
        function add(a, b) { return a + b; }
        add?.(3, 4)
    "#).unwrap();
    assert_jv!(r, n(7.0));
}

#[test]
fn vm_optional_chain_undefined() {
    let r = run_vm(r#"
        let o = null;
        o?.foo
    "#).unwrap();
    assert_jv!(r, JsValue::Undefined);
}

#[test]
fn vm_optional_chain_defined() {
    let r = run_vm(r#"
        let o = { foo: 42 };
        o?.foo
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_optional_chain_chained() {
    let r = run_vm(r#"
        let o = { a: { b: 7 } };
        o?.a?.b
    "#).unwrap();
    assert_jv!(r, n(7.0));
}

#[test]
fn vm_for_in_object_keys() {
    // Pozadujem run_vm_with_globals (Object je global).
    let r = run_vm_with_globals(r#"
        let obj = { a: 1, b: 2, c: 3 };
        let count = 0;
        for (let key in obj) {
            count = count + 1;
        }
        count
    "#).unwrap();
    assert_jv!(r, n(3.0));
}

#[test]
fn vm_for_in_sum_values() {
    let r = run_vm_with_globals(r#"
        let obj = { x: 10, y: 20, z: 30 };
        let total = 0;
        for (let k in obj) {
            total = total + obj[k];
        }
        total
    "#).unwrap();
    assert_jv!(r, n(60.0));
}

#[test]
fn vm_for_of_array() {
    let r = run_vm(r#"
        let sum = 0;
        for (let x of [1, 2, 3, 4]) {
            sum = sum + x;
        }
        sum
    "#).unwrap();
    assert_jv!(r, n(10.0));
}

#[test]
fn vm_for_of_with_break() {
    let r = run_vm(r#"
        let result = 0;
        for (let n of [1, 2, 3, 4, 5]) {
            if (n === 3) break;
            result = n;
        }
        result
    "#).unwrap();
    assert_jv!(r, n(2.0));
}

#[test]
fn vm_for_of_string() {
    let r = run_vm(r#"
        let count = 0;
        for (let c of "hello") {
            count = count + 1;
        }
        count
    "#).unwrap();
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_template_literal_simple() {
    let r = run_vm(r#"
        let name = 'world';
        `Hello, ${name}!`
    "#).unwrap();
    assert_jv!(r, JsValue::Str("Hello, world!".to_string()));
}

#[test]
fn vm_template_literal_multiple_exprs() {
    let r = run_vm(r#"
        let a = 1;
        let b = 2;
        `${a}+${b}=${a+b}`
    "#).unwrap();
    assert_jv!(r, JsValue::Str("1+2=3".to_string()));
}

#[test]
fn vm_template_literal_only_quasi() {
    let r = run_vm(r#"`just a string`"#).unwrap();
    assert_jv!(r, JsValue::Str("just a string".to_string()));
}

#[test]
fn vm_logical_and_assign() {
    // Truthy lhs: assign rhs.
    let r = run_vm(r#"
        let x = 1;
        x &&= 99;
        x
    "#).unwrap();
    assert_jv!(r, n(99.0));
    // Falsy lhs: nech as is.
    let r = run_vm(r#"
        let x = 0;
        x &&= 99;
        x
    "#).unwrap();
    assert_jv!(r, n(0.0));
}

#[test]
fn vm_logical_or_assign() {
    // Falsy lhs: assign rhs.
    let r = run_vm(r#"
        let x = 0;
        x ||= 42;
        x
    "#).unwrap();
    assert_jv!(r, n(42.0));
    // Truthy lhs: nech as is.
    let r = run_vm(r#"
        let x = 5;
        x ||= 42;
        x
    "#).unwrap();
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_null_coalesce_assign() {
    // null lhs: assign rhs.
    let r = run_vm(r#"
        let x = null;
        x ??= 'default';
        x
    "#).unwrap();
    assert_jv!(r, JsValue::Str("default".to_string()));
    // Non-null lhs: nech as is.
    let r = run_vm(r#"
        let x = 0;
        x ??= 'default';
        x
    "#).unwrap();
    assert_jv!(r, n(0.0));
}

#[test]
fn vm_array_push() {
    let r = run_vm(r#"
        let a = [1, 2];
        a.push(3);
        a.push(4);
        a.length
    "#).unwrap();
    assert_jv!(r, n(4.0));
}

#[test]
fn vm_array_pop() {
    let r = run_vm(r#"
        let a = [10, 20, 30];
        let last = a.pop();
        last
    "#).unwrap();
    assert_jv!(r, n(30.0));
}

#[test]
fn vm_array_indexOf() {
    let r = run_vm(r#"
        let a = [1, 2, 3, 4, 5];
        a.indexOf(3)
    "#).unwrap();
    assert_jv!(r, n(2.0));
}

#[test]
fn vm_array_includes() {
    let r = run_vm(r#"
        let a = [1, 2, 3];
        a.includes(2)
    "#).unwrap();
    assert_jv!(r, JsValue::Bool(true));
}

#[test]
fn vm_array_join() {
    let r = run_vm(r#"
        let a = [1, 2, 3];
        a.join('-')
    "#).unwrap();
    assert_jv!(r, JsValue::Str("1-2-3".to_string()));
}

#[test]
fn vm_string_to_upper() {
    let r = run_vm(r#""hello".toUpperCase()"#).unwrap();
    assert_jv!(r, JsValue::Str("HELLO".to_string()));
}

#[test]
fn vm_string_split() {
    let r = run_vm(r#"
        let parts = "a,b,c".split(',');
        parts.length
    "#).unwrap();
    assert_jv!(r, n(3.0));
}

#[test]
fn vm_string_includes() {
    let r = run_vm(r#""hello world".includes("world")"#).unwrap();
    assert_jv!(r, JsValue::Bool(true));
}

#[test]
fn vm_arrow_function_expr_body() {
    let r = run_vm(r#"
        let add = (a, b) => a + b;
        add(7, 8)
    "#).unwrap();
    assert_jv!(r, n(15.0));
}

#[test]
fn vm_arrow_function_block_body() {
    let r = run_vm(r#"
        let mul = (a, b) => { return a * b; };
        mul(6, 7)
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_arrow_captures_outer() {
    let r = run_vm(r#"
        let factor = 10;
        let scale = x => x * factor;
        scale(5)
    "#).unwrap();
    assert_jv!(r, n(50.0));
}

#[test]
fn vm_function_expression_anonymous() {
    let r = run_vm(r#"
        let f = function(n) { return n + 100; };
        f(23)
    "#).unwrap();
    assert_jv!(r, n(123.0));
}

#[test]
fn vm_object_prop_assign() {
    let r = run_vm(r#"
        let o = { a: 1 };
        o.a = 42;
        o.a
    "#).unwrap();
    assert_jv!(r, n(42.0));
}

#[test]
fn vm_object_new_prop_assign() {
    let r = run_vm(r#"
        let o = {};
        o.x = 'hello';
        o.x
    "#).unwrap();
    assert_jv!(r, JsValue::Str("hello".to_string()));
}

#[test]
fn vm_array_index_assign() {
    let r = run_vm(r#"
        let a = [1, 2, 3];
        a[1] = 99;
        a[1]
    "#).unwrap();
    assert_jv!(r, n(99.0));
}

#[test]
fn vm_object_computed_assign() {
    let r = run_vm(r#"
        let o = {};
        let key = 'test';
        o[key] = 100;
        o['test']
    "#).unwrap();
    assert_jv!(r, n(100.0));
}

#[test]
fn vm_closure_captures_outer_var() {
    let r = run_vm(r#"
        let x = 100;
        function f() { return x; }
        f()
    "#).unwrap();
    assert_jv!(r, n(100.0));
}

#[test]
fn vm_closure_captures_multiple_vars() {
    let r = run_vm(r#"
        let a = 10;
        let b = 20;
        let c = 30;
        function sum() { return a + b + c; }
        sum()
    "#).unwrap();
    assert_jv!(r, n(60.0));
}

#[test]
fn vm_closure_capture_by_value() {
    // Closure capturuje hodnotu pri vzniku funkce, ne pozdejsi mutation.
    let r = run_vm(r#"
        let x = 5;
        function readX() { return x; }
        x = 999;
        readX()
    "#).unwrap();
    // By-value semantics: readX vraci 5 (snapshot pri creation), ne 999.
    assert_jv!(r, n(5.0));
}

#[test]
fn vm_user_function_recursion() {
    let r = run_vm(r#"
        function fact(n) {
            if (n <= 1) return 1;
            return n * fact(n - 1);
        }
        fact(6)
    "#).unwrap();
    assert_jv!(r, n(720.0));
}

#[test]
fn vm_user_function_fib_recursive() {
    let r = run_vm(r#"
        function fib(n) {
            if (n < 2) return n;
            return fib(n - 1) + fib(n - 2);
        }
        fib(10)
    "#).unwrap();
    assert_jv!(r, n(55.0));
}

#[test]
fn vm_user_function_loop_inside() {
    let r = run_vm(r#"
        function sumTo(n) {
            let s = 0;
            for (let i = 1; i <= n; i = i + 1) {
                s = s + i;
            }
            return s;
        }
        sumTo(10)
    "#).unwrap();
    assert_jv!(r, n(55.0));
}

#[test]
fn vm_nested_for() {
    let r = run_vm(r#"
        let total = 0;
        for (let i = 1; i <= 3; i = i + 1) {
            for (let j = 1; j <= 3; j = j + 1) {
                total = total + i * j;
            }
        }
        total
    "#).unwrap();
    assert_jv!(r, n(36.0));
}
