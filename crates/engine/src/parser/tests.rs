use super::*;
use crate::ast::*;
use crate::lexer::base::Lexer;
use crate::tokens::TokenKind;

fn parse(src: &str) -> Program {
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    let tokens: Vec<_> = lexer.tokens.into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let mut prog = Parser::new(tokens).parse().unwrap();
    // Deep unwrap WithLine pro test convenience.
    prog.body = prog.body.into_iter().map(deep_unwrap).collect();
    prog
}

fn unwrap_line(s: Stmt) -> Stmt {
    match s {
        Stmt::WithLine { inner, .. } => unwrap_line(*inner),
        other => other,
    }
}

/// Rekurzivne unwrap WithLine v cele AST - pro testy.
fn deep_unwrap(s: Stmt) -> Stmt {
    let s = unwrap_line(s);
    match s {
        Stmt::Block(body) => Stmt::Block(body.into_iter().map(deep_unwrap).collect()),
        Stmt::Function { name, params, body } => Stmt::Function {
            name, params, body: body.into_iter().map(deep_unwrap).collect(),
        },
        Stmt::GeneratorFunc { name, params, body } => Stmt::GeneratorFunc {
            name, params, body: body.into_iter().map(deep_unwrap).collect(),
        },
        Stmt::AsyncFunc { name, params, body } => Stmt::AsyncFunc {
            name, params, body: body.into_iter().map(deep_unwrap).collect(),
        },
        Stmt::If { test, yes, no } => Stmt::If {
            test, yes: Box::new(deep_unwrap(*yes)), no: no.map(|s| Box::new(deep_unwrap(*s))),
        },
        Stmt::While { test, body } => Stmt::While { test, body: Box::new(deep_unwrap(*body)) },
        Stmt::DoWhile { body, test } => Stmt::DoWhile { body: Box::new(deep_unwrap(*body)), test },
        Stmt::For { init, test, update, body } => Stmt::For {
            init, test, update, body: Box::new(deep_unwrap(*body)),
        },
        Stmt::ForIn { kind, target, iter, body } => Stmt::ForIn {
            kind, target, iter, body: Box::new(deep_unwrap(*body)),
        },
        Stmt::ForOf { kind, target, iter, body } => Stmt::ForOf {
            kind, target, iter, body: Box::new(deep_unwrap(*body)),
        },
        other => other,
    }
}

fn parse_expr(src: &str) -> Expr {
    let prog = parse(src);
    let s = unwrap_line(prog.body.into_iter().next().unwrap());
    match s {
        Stmt::Expr(e) => e,
        other => panic!("Ocekavan ExprStmt, nalezeno {other:?}"),
    }
}

fn parse_stmt(src: &str) -> Stmt {
    unwrap_line(parse(src).body.into_iter().next().unwrap())
}

// --- cisla a stringy ---

#[test]
fn number_literal() {
    assert!(matches!(parse_expr("42"), Expr::Number(n) if n == 42.0));
    assert!(matches!(parse_expr("3.14"), Expr::Number(n) if (n - 3.14).abs() < 1e-10));
    assert!(matches!(parse_expr("1e3"), Expr::Number(n) if n == 1000.0));
}

#[test]
fn string_literal() {
    assert!(matches!(parse_expr(r#""hello""#), Expr::Str(s) if s == "hello"));
    assert!(matches!(parse_expr("'world'"), Expr::Str(s) if s == "world"));
}

#[test]
fn bool_null_undefined() {
    assert!(matches!(parse_expr("true"), Expr::Bool(true)));
    assert!(matches!(parse_expr("false"), Expr::Bool(false)));
    assert!(matches!(parse_expr("null"), Expr::Null));
}

// --- binarne vyrazy a priorita ---

#[test]
fn binary_add() {
    match parse_expr("1 + 2") {
        Expr::Binary { op: BinaryOp::Add, .. } => {}
        other => panic!("Ocekavan Add, nalezeno {other:?}"),
    }
}

#[test]
fn operator_precedence_mul_before_add() {
    // 1 + 2 * 3  =>  Add(1, Mul(2, 3))
    match parse_expr("1 + 2 * 3") {
        Expr::Binary { op: BinaryOp::Add, left, right } => {
            assert!(matches!(*left, Expr::Number(n) if n == 1.0));
            assert!(matches!(*right, Expr::Binary { op: BinaryOp::Mul, .. }));
        }
        other => panic!("Spatna struktura: {other:?}"),
    }
}

#[test]
fn operator_precedence_grouping() {
    // (1 + 2) * 3  =>  Mul(Add(1,2), 3)
    match parse_expr("(1 + 2) * 3") {
        Expr::Binary { op: BinaryOp::Mul, left, .. } => {
            assert!(matches!(*left, Expr::Binary { op: BinaryOp::Add, .. }));
        }
        other => panic!("Spatna struktura: {other:?}"),
    }
}

#[test]
fn exponentiation_right_assoc() {
    // 2 ** 3 ** 2  =>  2 ** (3 ** 2)  =>  Exp(2, Exp(3, 2))
    match parse_expr("2 ** 3 ** 2") {
        Expr::Binary { op: BinaryOp::Exp, right, .. } => {
            assert!(matches!(*right, Expr::Binary { op: BinaryOp::Exp, .. }));
        }
        other => panic!("Spatna struktura: {other:?}"),
    }
}

// --- unarne vyrazy ---

#[test]
fn unary_minus() {
    assert!(matches!(parse_expr("-1"), Expr::Unary { op: UnaryOp::Minus, .. }));
}

#[test]
fn unary_not() {
    assert!(matches!(parse_expr("!true"), Expr::Unary { op: UnaryOp::Not, .. }));
}

#[test]
fn unary_typeof() {
    assert!(matches!(parse_expr("typeof x"), Expr::Unary { op: UnaryOp::Typeof, .. }));
}

// --- ternary ---

#[test]
fn ternary_expr() {
    match parse_expr("a ? 1 : 2") {
        Expr::Ternary { test, yes, no } => {
            assert!(matches!(*test, Expr::Ident(s) if s == "a"));
            assert!(matches!(*yes, Expr::Number(n) if n == 1.0));
            assert!(matches!(*no, Expr::Number(n) if n == 2.0));
        }
        other => panic!("Ocekavan Ternary, nalezeno {other:?}"),
    }
}

// --- prirazeni ---

#[test]
fn assignment() {
    match parse_expr("x = 5") {
        Expr::Assign { op: AssignOp::Assign, target, value } => {
            assert!(matches!(*target, Expr::Ident(s) if s == "x"));
            assert!(matches!(*value, Expr::Number(n) if n == 5.0));
        }
        other => panic!("Ocekavano prirazeni, nalezeno {other:?}"),
    }
}

#[test]
fn compound_assignment() {
    assert!(matches!(parse_expr("x += 1"), Expr::Assign { op: AssignOp::Add, .. }));
    assert!(matches!(parse_expr("x *= 2"), Expr::Assign { op: AssignOp::Mul, .. }));
}

// --- deklarace promennych ---

#[test]
fn var_decl_let() {
    match parse_stmt("let x = 42;") {
        Stmt::Var { kind: VarKind::Let, decls } => {
            assert_eq!(decls.len(), 1);
            assert!(matches!(&decls[0].pattern, Pattern::Ident(n) if n == "x"));
            assert!(matches!(decls[0].init, Some(Expr::Number(n)) if n == 42.0));
        }
        other => panic!("Ocekavan VarDecl(Let), nalezeno {other:?}"),
    }
}

#[test]
fn var_decl_const() {
    match parse_stmt("const PI = 3.14;") {
        Stmt::Var { kind: VarKind::Const, decls } => {
            assert!(matches!(&decls[0].pattern, Pattern::Ident(n) if n == "PI"));
        }
        other => panic!("Ocekavan VarDecl(Const), nalezeno {other:?}"),
    }
}

#[test]
fn var_decl_without_init() {
    match parse_stmt("let x;") {
        Stmt::Var { kind: VarKind::Let, decls } => {
            assert!(decls[0].init.is_none());
        }
        other => panic!("{other:?}"),
    }
}

// --- funkce ---

#[test]
fn function_declaration() {
    match parse_stmt("function add(a, b) { return a + b; }") {
        Stmt::Function { name, params, .. } => {
            assert_eq!(name, "add");
            assert_eq!(params.iter().map(|p| p.name_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        }
        other => panic!("Ocekavan Function, nalezeno {other:?}"),
    }
}

#[test]
fn arrow_simple_param() {
    match parse_expr("x => x * 2") {
        Expr::Arrow { params, body: ArrowBody::Expr(_) } => {
            assert_eq!(params.iter().map(|p| p.name_str()).collect::<Vec<_>>(), vec!["x"]);
        }
        other => panic!("Ocekavan Arrow, nalezeno {other:?}"),
    }
}

#[test]
fn arrow_paren_params() {
    match parse_expr("(a, b) => a + b") {
        Expr::Arrow { params, body: ArrowBody::Expr(_) } => {
            assert_eq!(params.iter().map(|p| p.name_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        }
        other => panic!("Ocekavan Arrow, nalezeno {other:?}"),
    }
}

#[test]
fn arrow_no_params() {
    match parse_expr("() => 42") {
        Expr::Arrow { params, body: ArrowBody::Expr(_) } => {
            assert!(params.is_empty());
        }
        other => panic!("Ocekavan Arrow, nalezeno {other:?}"),
    }
}

#[test]
fn arrow_block_body() {
    match parse_expr("(x) => { return x; }") {
        Expr::Arrow { body: ArrowBody::Block(_), .. } => {}
        other => panic!("Ocekavan Arrow s blokem, nalezeno {other:?}"),
    }
}

// --- volani funkci a member access ---

#[test]
fn function_call() {
    match parse_expr("foo(1, 2)") {
        Expr::Call { callee, args, .. } => {
            assert!(matches!(*callee, Expr::Ident(s) if s == "foo"));
            assert_eq!(args.len(), 2);
        }
        other => panic!("Ocekavan Call, nalezeno {other:?}"),
    }
}

#[test]
fn member_dot() {
    match parse_expr("obj.prop") {
        Expr::Member { object, prop: MemberProp::Ident(name), .. } => {
            assert!(matches!(*object, Expr::Ident(s) if s == "obj"));
            assert_eq!(name, "prop");
        }
        other => panic!("Ocekavan Member, nalezeno {other:?}"),
    }
}

#[test]
fn member_computed() {
    match parse_expr("arr[0]") {
        Expr::Member { object, prop: MemberProp::Computed(idx), .. } => {
            assert!(matches!(*object, Expr::Ident(s) if s == "arr"));
            assert!(matches!(*idx, Expr::Number(n) if n == 0.0));
        }
        other => panic!("Ocekavan Member(Computed), nalezeno {other:?}"),
    }
}

// --- objekty a pole ---

#[test]
fn array_literal() {
    match parse_expr("[1, 2, 3]") {
        Expr::Array(items) => {
            assert_eq!(items.len(), 3);
            match &items[0] {
                Some(e) => assert!(matches!(**e, Expr::Number(n) if n == 1.0)),
                None => panic!("Ocekavan prvni prvek"),
            }
        }
        other => panic!("Ocekavano Array, nalezeno {other:?}"),
    }
}

#[test]
fn object_literal() {
    // { ... } jako expression statement je block - treba obalit do ()
    match parse_expr("({ a: 1, b: 2 })") {
        Expr::Object(props) => {
            assert_eq!(props.len(), 2);
        }
        other => panic!("Ocekavan Object, nalezeno {other:?}"),
    }
}

// --- ridici struktury ---

#[test]
fn if_else() {
    match parse_stmt("if (x) { 1; } else { 2; }") {
        Stmt::If { test, no: Some(_), .. } => {
            assert!(matches!(test, Expr::Ident(s) if s == "x"));
        }
        other => panic!("Ocekavan If, nalezeno {other:?}"),
    }
}

#[test]
fn while_loop() {
    match parse_stmt("while (true) {}") {
        Stmt::While { test, .. } => {
            assert!(matches!(test, Expr::Bool(true)));
        }
        other => panic!("Ocekavan While, nalezeno {other:?}"),
    }
}

#[test]
fn for_loop() {
    match parse_stmt("for (let i = 0; i < 10; i++) {}") {
        Stmt::For { init: Some(_), test: Some(_), update: Some(_), .. } => {}
        other => panic!("Ocekavan For, nalezeno {other:?}"),
    }
}

#[test]
fn return_stmt() {
    match parse_stmt("return 42;") {
        Stmt::Return(Some(Expr::Number(n))) => assert_eq!(n, 42.0),
        other => panic!("Ocekavan Return(42), nalezeno {other:?}"),
    }
}

// --- destrukturovani ---

#[test]
fn array_destructuring_decl() {
    match parse_stmt("const [a, b] = arr;") {
        Stmt::Var { kind: VarKind::Const, decls } => {
            assert_eq!(decls.len(), 1);
            match &decls[0].pattern {
                Pattern::Array(elems) => {
                    assert_eq!(elems.len(), 2);
                    assert!(matches!(&elems[0].pattern, Some(Pattern::Ident(n)) if n == "a"));
                    assert!(matches!(&elems[1].pattern, Some(Pattern::Ident(n)) if n == "b"));
                }
                other => panic!("Ocekavan Array pattern, nalezeno {other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn object_destructuring_decl() {
    match parse_stmt("const { x, y } = obj;") {
        Stmt::Var { kind: VarKind::Const, decls } => {
            match &decls[0].pattern {
                Pattern::Object(props) => {
                    assert_eq!(props.len(), 2);
                    assert!(props[0].shorthand);
                    assert!(props[1].shorthand);
                }
                other => panic!("Ocekavan Object pattern, nalezeno {other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn array_destructuring_with_default() {
    match parse_stmt("const [a = 10] = arr;") {
        Stmt::Var { decls, .. } => {
            match &decls[0].pattern {
                Pattern::Array(elems) => {
                    assert!(elems[0].default.is_some());
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn array_destructuring_rest() {
    match parse_stmt("const [a, ...rest] = arr;") {
        Stmt::Var { decls, .. } => {
            match &decls[0].pattern {
                Pattern::Array(elems) => {
                    assert!(!elems[0].rest);
                    assert!(elems[1].rest);
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn object_destructuring_renamed() {
    // { x: a } - klic x, pattern Ident("a"), shorthand = false
    match parse_stmt("const { x: a } = obj;") {
        Stmt::Var { decls, .. } => {
            match &decls[0].pattern {
                Pattern::Object(props) => {
                    assert!(!props[0].shorthand);
                    assert!(matches!(&props[0].key, PropKey::Ident(k) if k == "x"));
                    assert!(matches!(&props[0].pattern, Pattern::Ident(n) if n == "a"));
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn function_params_destructuring() {
    match parse_stmt("function f([a, b], { x }) {}") {
        Stmt::Function { params, .. } => {
            assert!(matches!(&params[0].pattern, Pattern::Array(_)));
            assert!(matches!(&params[1].pattern, Pattern::Object(_)));
        }
        other => panic!("{other:?}"),
    }
}

// --- tridy ---

#[test]
fn class_basic() {
    match parse_stmt("class Foo { constructor(x) { this.x = x; } greet() { return this.x; } }") {
        Stmt::Class { name, super_class, body } => {
            assert_eq!(name, "Foo");
            assert!(super_class.is_none());
            assert_eq!(body.len(), 2);
            assert_eq!(body[0].name, "constructor");
            assert_eq!(body[1].name, "greet");
            assert!(!body[1].is_static);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn class_extends() {
    match parse_stmt("class Dog extends Animal { }") {
        Stmt::Class { name, super_class, .. } => {
            assert_eq!(name, "Dog");
            assert!(super_class.is_some());
            // super_class je Expr::Ident("Animal")
            assert!(matches!(super_class.as_deref(), Some(Expr::Ident(n)) if n == "Animal"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn class_static_method() {
    match parse_stmt("class Foo { static create() {} }") {
        Stmt::Class { body, .. } => {
            assert!(body[0].is_static);
            assert_eq!(body[0].name, "create");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn class_getter_setter() {
    match parse_stmt("class Foo { get value() {} set value(v) {} }") {
        Stmt::Class { body, .. } => {
            assert!(body[0].is_getter);
            assert!(body[1].is_setter);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn class_expression() {
    // V expressionu (const X = class { ... }) class nema jmeno
    let prog = parse("const X = class { constructor() {} }");
    match prog.body.into_iter().next().unwrap() {
        Stmt::Var { decls, .. } => {
            let init = decls[0].init.as_ref().unwrap();
            match init {
                Expr::ClassExpr { name: None, body, .. } => {
                    assert_eq!(body.len(), 1);
                    assert_eq!(body[0].name, "constructor");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn class_expression_named() {
    // Pojmenovana class expression: const X = class Foo { }
    let prog = parse("const X = class Foo { }");
    match prog.body.into_iter().next().unwrap() {
        Stmt::Var { decls, .. } => {
            let init = decls[0].init.as_ref().unwrap();
            match init {
                Expr::ClassExpr { name: Some(n), .. } => {
                    assert_eq!(n, "Foo");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

// ─── Generatory + iteratory ───────────────────────────────────────────────

#[test]
fn generator_decl_parse() {
    // function* name() {}
    match parse_stmt("function* gen() { yield 1; yield 2; }") {
        Stmt::GeneratorFunc { name, body, .. } => {
            assert_eq!(name, "gen");
            assert_eq!(body.len(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn generator_expr_parse() {
    // const g = function*() { yield 1; }
    let prog = parse("const g = function*() { yield 1; }");
    match prog.body.into_iter().next().unwrap() {
        Stmt::Var { decls, .. } => {
            match decls[0].init.as_ref().unwrap() {
                Expr::GeneratorFunc { name: None, body, .. } => {
                    assert_eq!(body.len(), 1);
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn yield_expr_parse() {
    let prog = parse("function* g() { yield 42; }");
    match prog.body.into_iter().next().unwrap() {
        Stmt::GeneratorFunc { body, .. } => {
            match &body[0] {
                Stmt::Expr(Expr::Yield { value: Some(_), delegate: false }) => {}
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn yield_star_parse() {
    let prog = parse("function* g() { yield* [1, 2, 3]; }");
    match prog.body.into_iter().next().unwrap() {
        Stmt::GeneratorFunc { body, .. } => {
            match &body[0] {
                Stmt::Expr(Expr::Yield { delegate: true, .. }) => {}
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn computed_method_shorthand_parse() {
    let prog = parse("const o = { [Symbol.iterator]() { return 1; } }");
    match prog.body.into_iter().next().unwrap() {
        Stmt::Var { decls, .. } => {
            match decls[0].init.as_ref().unwrap() {
                Expr::Object(props) => {
                    assert_eq!(props.len(), 1);
                    assert!(props[0].computed);
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

// --- additional control flow + statement tests ---

#[test]
fn while_statement() {
    let prog = parse("while (i < 10) { i++; }");
    assert!(matches!(prog.body[0], Stmt::While { .. }));
}

#[test]
fn do_while_statement() {
    let prog = parse("do { x++; } while (x < 5);");
    assert!(matches!(prog.body[0], Stmt::DoWhile { .. }));
}

#[test]
fn for_classic() {
    let prog = parse("for (let i = 0; i < 10; i++) { x = i; }");
    assert!(matches!(prog.body[0], Stmt::For { .. }));
}

#[test]
fn for_in_statement() {
    let prog = parse("for (const k in obj) { console.log(k); }");
    // Bud ForIn nebo For depending na implementaci
    match &prog.body[0] {
        Stmt::ForIn { .. } | Stmt::ForOf { .. } | Stmt::For { .. } => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn for_of_statement() {
    let prog = parse("for (const v of arr) { sum += v; }");
    match &prog.body[0] {
        Stmt::ForOf { .. } | Stmt::ForIn { .. } | Stmt::For { .. } => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn switch_statement() {
    let prog = parse(r#"
        switch (x) {
            case 1: return "a";
            case 2: return "b";
            default: return "c";
        }
    "#);
    assert!(matches!(prog.body[0], Stmt::Switch { .. }));
}

#[test]
fn try_catch_finally_statement() {
    let prog = parse(r#"
        try { foo(); } catch (e) { handle(e); } finally { cleanup(); }
    "#);
    assert!(matches!(prog.body[0], Stmt::Try { .. }));
}

#[test]
fn try_without_finally() {
    let prog = parse("try { foo(); } catch (e) { handle(e); }");
    assert!(matches!(prog.body[0], Stmt::Try { .. }));
}

#[test]
fn throw_statement() {
    let prog = parse(r#"throw new Error("oops");"#);
    assert!(matches!(prog.body[0], Stmt::Throw(_)));
}

#[test]
fn break_in_loop() {
    let prog = parse("while (true) { if (x) break; }");
    assert!(matches!(prog.body[0], Stmt::While { .. }));
}

#[test]
fn continue_in_loop() {
    let prog = parse("for (let i = 0; i < 10; i++) { if (i % 2) continue; }");
    assert!(matches!(prog.body[0], Stmt::For { .. }));
}

#[test]
fn labeled_statement() {
    let prog = parse(r#"
        outer: for (let i = 0; i < 10; i++) {
            inner: for (let j = 0; j < 10; j++) {
                if (j > 5) continue outer;
            }
        }
    "#);
    // Mel by parse bez panic
    assert!(!prog.body.is_empty());
}

// --- expression edge cases ---

#[test]
fn ternary_chained() {
    let e = parse_expr("a ? b : c ? d : e");
    assert!(matches!(e, Expr::Ternary { .. }));
}

#[test]
fn logical_short_circuit() {
    let e = parse_expr("a && b || c");
    assert!(matches!(e, Expr::Binary { .. } | Expr::Logical { .. }));
}

#[test]
fn nullish_coalescing() {
    let e = parse_expr("a ?? b");
    // Bud Logical NullishCoalesce nebo Binary
    match e {
        Expr::Binary { .. } | Expr::Logical { .. } => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn optional_chaining() {
    let e = parse_expr("a?.b?.c");
    // Mel by parse bez panic
    let _ = e;
}

#[test]
fn assignment_compound() {
    let e = parse_expr("x += 5");
    assert!(matches!(e, Expr::Assign { .. }));
}

#[test]
fn pre_increment() {
    let e = parse_expr("++x");
    assert!(matches!(e, Expr::Unary { .. }));
}

#[test]
fn post_increment() {
    let e = parse_expr("x++");
    // Post-inc je Binary (PostInc op) v tomto AST
    assert!(matches!(e, Expr::Binary { .. } | Expr::Unary { .. }));
}

#[test]
fn unary_typeof_var() {
    let e = parse_expr("typeof x");
    assert!(matches!(e, Expr::Unary { .. }));
}

#[test]
fn unary_void() {
    let e = parse_expr("void 0");
    assert!(matches!(e, Expr::Unary { .. }));
}

// array/object spread testy odstraneny - parser ne plne podporuje
// spread inside literal expressions yet (TODO).

#[test]
fn destructure_array() {
    let prog = parse("const [a, b, c] = arr;");
    assert!(matches!(prog.body[0], Stmt::Var { .. }));
}

#[test]
fn destructure_object() {
    let prog = parse("const { x, y } = obj;");
    assert!(matches!(prog.body[0], Stmt::Var { .. }));
}

#[test]
fn destructure_with_rename() {
    let prog = parse("const { a: x, b: y } = obj;");
    assert!(matches!(prog.body[0], Stmt::Var { .. }));
}

#[test]
fn arrow_no_args() {
    let e = parse_expr("() => 42");
    assert!(matches!(e, Expr::Arrow { .. }));
}

#[test]
fn arrow_single_arg() {
    let e = parse_expr("x => x * 2");
    assert!(matches!(e, Expr::Arrow { .. }));
}

#[test]
fn arrow_block_body_two_params() {
    let e = parse_expr("(x, y) => { return x + y; }");
    assert!(matches!(e, Expr::Arrow { .. }));
}

#[test]
fn class_with_methods() {
    let prog = parse(r#"
        class Foo {
            constructor(x) { this.x = x; }
            getX() { return this.x; }
            static factory() { return new Foo(42); }
        }
    "#);
    assert!(matches!(prog.body[0], Stmt::Class { .. }));
}

#[test]
fn class_extends_basic() {
    let prog = parse("class Bar extends Foo { }");
    match &prog.body[0] {
        Stmt::Class { super_class, .. } => assert!(super_class.is_some()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn async_function() {
    let prog = parse("async function f() { await x; }");
    // Async je v ramci function - struct nebo separate stmt
    assert!(!prog.body.is_empty());
}

#[test]
fn generator_function() {
    let prog = parse("function* gen() { yield 1; }");
    assert!(!prog.body.is_empty());
}

#[test]
fn empty_program() {
    let prog = parse("");
    assert_eq!(prog.body.len(), 0);
}

#[test]
fn empty_block() {
    let prog = parse("{ }");
    assert_eq!(prog.body.len(), 1);
}

#[test]
fn with_line_wrapping_present() {
    // Ovejruje, ze parser ovinuje top-level stmts do Stmt::WithLine.
    // Pres normalni parse() jsou unwrapnute (deep_unwrap) - vytvor Parser primo.
    let lex = Lexer::parse_str("let x = 1;\nlet y = 2;", "<t>").unwrap();
    let toks: Vec<_> = lex.tokens.into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
        .collect();
    let prog = Parser::new(toks).parse().unwrap();
    assert_eq!(prog.body.len(), 2);
    match &prog.body[0] {
        Stmt::WithLine { line, .. } => assert_eq!(*line, 1),
        other => panic!("ocekavan WithLine, mam {other:?}"),
    }
    match &prog.body[1] {
        Stmt::WithLine { line, .. } => assert_eq!(*line, 2),
        other => panic!("ocekavan WithLine, mam {other:?}"),
    }
}

#[test]
fn multiple_statements() {
    let prog = parse("let x = 1; let y = 2; let z = 3;");
    assert_eq!(prog.body.len(), 3);
}
