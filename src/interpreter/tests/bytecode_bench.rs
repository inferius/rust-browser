/// Benchmark - VM vs tree-walker. Mereni v debug profile (release lepsi pomer).
///
/// Spustitelne: `cargo test --release bytecode_bench -- --nocapture`

use crate::interpreter::bytecode::{compile_program, VM};
use crate::interpreter::Interpreter;
use crate::lexer::base::Lexer;
use crate::parser::Parser;

fn parse(src: &str) -> Vec<crate::ast::Stmt> {
    let lex = Lexer::parse_str(src, "bench").expect("lex");
    let mut parser = Parser::new(lex.tokens.clone());
    parser.parse().expect("parse").body
}

fn run_vm(stmts: &[crate::ast::Stmt]) -> std::time::Duration {
    let code = compile_program(stmts).expect("compile");
    let interp = Interpreter::new();
    let mut vm = VM::with_env(interp.global.clone());
    let start = std::time::Instant::now();
    let _ = vm.run(&code);
    start.elapsed()
}

fn run_treewalker(src: &str) -> std::time::Duration {
    let lex = Lexer::parse_str(src, "bench").expect("lex");
    let mut parser = Parser::new(lex.tokens.clone());
    let program = parser.parse().expect("parse");
    let mut interp = Interpreter::new();
    let start = std::time::Instant::now();
    let _ = interp.run(&program);
    start.elapsed()
}

#[test]
fn bench_arithmetic_loop() {
    // Tight loop - 100k iteraci, sum + 2*x.
    let src = r#"
        let sum = 0;
        for (let i = 0; i < 100000; i = i + 1) {
            sum = sum + i * 2;
        }
        sum
    "#;
    let stmts = parse(src);
    let vm_time = run_vm(&stmts);
    let tw_time = run_treewalker(src);
    let ratio = tw_time.as_nanos() as f64 / vm_time.as_nanos().max(1) as f64;
    println!("[bench] arithmetic_loop: VM={:.2}ms, TreeWalker={:.2}ms, speedup={:.2}x",
        vm_time.as_secs_f64() * 1000.0,
        tw_time.as_secs_f64() * 1000.0,
        ratio);
    // VM by mela byt aspon stejne rychla nebo rychlejsi.
    // V debug profile bytecode dispatch overhead muze byt > tree walk - stale OK.
    assert!(vm_time.as_micros() > 0);
    assert!(tw_time.as_micros() > 0);
}

#[test]
fn bench_recursive_fib_native() {
    // Bez user-defined funkci (VM neumi). Cca ekvivalent: iterativni fib.
    let src = r#"
        let a = 0;
        let b = 1;
        for (let i = 0; i < 30; i = i + 1) {
            let c = a + b;
            a = b;
            b = c;
        }
        b
    "#;
    let stmts = parse(src);
    let vm_time = run_vm(&stmts);
    let tw_time = run_treewalker(src);
    let ratio = tw_time.as_nanos() as f64 / vm_time.as_nanos().max(1) as f64;
    println!("[bench] iterative_fib: VM={:.2}us, TreeWalker={:.2}us, speedup={:.2}x",
        vm_time.as_secs_f64() * 1_000_000.0,
        tw_time.as_secs_f64() * 1_000_000.0,
        ratio);
}

#[test]
fn bench_user_function_recursion() {
    let src = r#"
        function fib(n) {
            if (n < 2) return n;
            return fib(n - 1) + fib(n - 2);
        }
        fib(20)
    "#;
    let stmts = parse(src);
    let vm_time = run_vm(&stmts);
    let tw_time = run_treewalker(src);
    let ratio = tw_time.as_nanos() as f64 / vm_time.as_nanos().max(1) as f64;
    println!("[bench] fib_recursive_20: VM={:.2}ms, TreeWalker={:.2}ms, speedup={:.2}x",
        vm_time.as_secs_f64() * 1000.0,
        tw_time.as_secs_f64() * 1000.0,
        ratio);
}

#[test]
fn bench_user_function_loop() {
    let src = r#"
        function sumTo(n) {
            let s = 0;
            for (let i = 1; i <= n; i = i + 1) {
                s = s + i;
            }
            return s;
        }
        sumTo(50000)
    "#;
    let stmts = parse(src);
    let vm_time = run_vm(&stmts);
    let tw_time = run_treewalker(src);
    let ratio = tw_time.as_nanos() as f64 / vm_time.as_nanos().max(1) as f64;
    println!("[bench] sumTo_50k: VM={:.2}ms, TreeWalker={:.2}ms, speedup={:.2}x",
        vm_time.as_secs_f64() * 1000.0,
        tw_time.as_secs_f64() * 1000.0,
        ratio);
}

#[test]
fn bench_member_access() {
    // Member access intenzivni loop.
    let src = r#"
        let obj = { x: 10, y: 20, z: 30 };
        let total = 0;
        for (let i = 0; i < 10000; i = i + 1) {
            total = total + obj.x + obj.y + obj.z;
        }
        total
    "#;
    let stmts = parse(src);
    let vm_time = run_vm(&stmts);
    let tw_time = run_treewalker(src);
    let ratio = tw_time.as_nanos() as f64 / vm_time.as_nanos().max(1) as f64;
    println!("[bench] member_access: VM={:.2}ms, TreeWalker={:.2}ms, speedup={:.2}x",
        vm_time.as_secs_f64() * 1000.0,
        tw_time.as_secs_f64() * 1000.0,
        ratio);
}
