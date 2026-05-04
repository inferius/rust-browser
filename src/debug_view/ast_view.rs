/// AST tree viewer - rekurzivni rendering vetvi jako collapsible <details>.
///
/// Kazdy uzel je <details><summary>NodeName</summary>...children...</details>.
/// Hloubka neni omezena. Click na summary toggluje viditelnost.

use crate::ast::*;
use super::html_escape;

/// Render Program (top-level).
pub fn render_program(p: &Program) -> String {
    let mut out = String::from("<div class=\"ast-tree\">");
    out.push_str(&format!(
        "<details open><summary class=\"ast-root\">Program ({} stmts)</summary>",
        p.body.len()
    ));
    for stmt in &p.body {
        out.push_str(&render_stmt(stmt));
    }
    out.push_str("</details></div>");
    out
}

fn open_node(label: &str, kind: &str) -> String {
    format!(
        "<details open><summary class=\"ast-node ast-{kind}\">{}</summary><div class=\"ast-children\">",
        html_escape(label)
    )
}

fn open_node_collapsed(label: &str, kind: &str) -> String {
    format!(
        "<details><summary class=\"ast-node ast-{kind}\">{}</summary><div class=\"ast-children\">",
        html_escape(label)
    )
}

fn close_node() -> &'static str {
    "</div></details>"
}

fn leaf(label: &str, kind: &str) -> String {
    format!(
        "<div class=\"ast-leaf ast-{kind}\">{}</div>",
        html_escape(label)
    )
}

pub fn render_stmt(s: &Stmt) -> String {
    let mut out = String::new();
    match s {
        Stmt::Expr(e) => {
            out.push_str(&open_node("ExpressionStatement", "stmt"));
            out.push_str(&render_expr(e));
            out.push_str(close_node());
        }
        Stmt::Block(b) => {
            out.push_str(&open_node(&format!("Block ({} stmts)", b.len()), "stmt"));
            for s in b { out.push_str(&render_stmt(s)); }
            out.push_str(close_node());
        }
        Stmt::Empty => {
            out.push_str(&leaf("EmptyStatement", "stmt"));
        }
        Stmt::Return(v) => {
            out.push_str(&open_node("ReturnStatement", "stmt"));
            if let Some(e) = v { out.push_str(&render_expr(e)); }
            out.push_str(close_node());
        }
        Stmt::Break(l) => {
            out.push_str(&leaf(&format!("Break {}", l.as_deref().unwrap_or("")), "stmt"));
        }
        Stmt::Continue(l) => {
            out.push_str(&leaf(&format!("Continue {}", l.as_deref().unwrap_or("")), "stmt"));
        }
        Stmt::Throw(e) => {
            out.push_str(&open_node("ThrowStatement", "stmt"));
            out.push_str(&render_expr(e));
            out.push_str(close_node());
        }
        Stmt::Var { kind, decls } => {
            out.push_str(&open_node(&format!("VariableDeclaration ({:?}, {} decls)", kind, decls.len()), "stmt"));
            for d in decls {
                out.push_str(&open_node(&format!("VarDecl {}", pattern_label(&d.pattern)), "decl"));
                if let Some(init) = &d.init {
                    out.push_str(&open_node("init", "field"));
                    out.push_str(&render_expr(init));
                    out.push_str(close_node());
                }
                out.push_str(close_node());
            }
            out.push_str(close_node());
        }
        Stmt::Function { name, params, body } => {
            out.push_str(&open_node(&format!("FunctionDeclaration: {name} ({} params)", params.len()), "stmt"));
            out.push_str(&render_params(params));
            out.push_str(&open_node(&format!("body ({} stmts)", body.len()), "field"));
            for s in body { out.push_str(&render_stmt(s)); }
            out.push_str(close_node());
            out.push_str(close_node());
        }
        Stmt::If { test, yes, no } => {
            out.push_str(&open_node("IfStatement", "stmt"));
            out.push_str(&open_node("test", "field"));
            out.push_str(&render_expr(test));
            out.push_str(close_node());
            out.push_str(&open_node("then", "field"));
            out.push_str(&render_stmt(yes));
            out.push_str(close_node());
            if let Some(n) = no {
                out.push_str(&open_node("else", "field"));
                out.push_str(&render_stmt(n));
                out.push_str(close_node());
            }
            out.push_str(close_node());
        }
        Stmt::While { test, body } => {
            out.push_str(&open_node("WhileStatement", "stmt"));
            out.push_str(&open_node("test", "field"));
            out.push_str(&render_expr(test));
            out.push_str(close_node());
            out.push_str(&render_stmt(body));
            out.push_str(close_node());
        }
        Stmt::DoWhile { body, test } => {
            out.push_str(&open_node("DoWhileStatement", "stmt"));
            out.push_str(&render_stmt(body));
            out.push_str(&open_node("test", "field"));
            out.push_str(&render_expr(test));
            out.push_str(close_node());
            out.push_str(close_node());
        }
        Stmt::For { init, test, update, body } => {
            out.push_str(&open_node("ForStatement", "stmt"));
            if let Some(_) = init {
                out.push_str(&open_node("init", "field"));
                out.push_str(&leaf("(ForInit)", "field"));
                out.push_str(close_node());
            }
            if let Some(t) = test {
                out.push_str(&open_node("test", "field"));
                out.push_str(&render_expr(t));
                out.push_str(close_node());
            }
            if let Some(u) = update {
                out.push_str(&open_node("update", "field"));
                out.push_str(&render_expr(u));
                out.push_str(close_node());
            }
            out.push_str(&render_stmt(body));
            out.push_str(close_node());
        }
        Stmt::ForIn { target, iter, body, .. } => {
            out.push_str(&open_node("ForInStatement", "stmt"));
            out.push_str(&render_expr(target));
            out.push_str(&open_node("iter", "field"));
            out.push_str(&render_expr(iter));
            out.push_str(close_node());
            out.push_str(&render_stmt(body));
            out.push_str(close_node());
        }
        Stmt::ForOf { target, iter, body, .. } => {
            out.push_str(&open_node("ForOfStatement", "stmt"));
            out.push_str(&render_expr(target));
            out.push_str(&open_node("iter", "field"));
            out.push_str(&render_expr(iter));
            out.push_str(close_node());
            out.push_str(&render_stmt(body));
            out.push_str(close_node());
        }
        Stmt::ForAwaitOf { target, iter, body, .. } => {
            out.push_str(&open_node("ForAwaitOfStatement", "stmt"));
            out.push_str(&render_expr(target));
            out.push_str(&open_node("iter", "field"));
            out.push_str(&render_expr(iter));
            out.push_str(close_node());
            out.push_str(&render_stmt(body));
            out.push_str(close_node());
        }
        Stmt::Try { body, catch, finally } => {
            out.push_str(&open_node("TryStatement", "stmt"));
            out.push_str(&open_node(&format!("try ({} stmts)", body.len()), "field"));
            for s in body { out.push_str(&render_stmt(s)); }
            out.push_str(close_node());
            if let Some(c) = catch {
                out.push_str(&open_node(&format!("catch ({} stmts)", c.body.len()), "field"));
                for s in &c.body { out.push_str(&render_stmt(s)); }
                out.push_str(close_node());
            }
            if let Some(f) = finally {
                out.push_str(&open_node(&format!("finally ({} stmts)", f.len()), "field"));
                for s in f { out.push_str(&render_stmt(s)); }
                out.push_str(close_node());
            }
            out.push_str(close_node());
        }
        Stmt::Labeled { label, body } => {
            out.push_str(&open_node(&format!("LabeledStatement: {label}"), "stmt"));
            out.push_str(&render_stmt(body));
            out.push_str(close_node());
        }
        Stmt::Switch { discriminant, cases } => {
            out.push_str(&open_node(&format!("SwitchStatement ({} cases)", cases.len()), "stmt"));
            out.push_str(&open_node("discriminant", "field"));
            out.push_str(&render_expr(discriminant));
            out.push_str(close_node());
            for c in cases {
                let label = match &c.test {
                    Some(_) => "case",
                    None    => "default",
                };
                out.push_str(&open_node(&format!("{} ({} stmts)", label, c.body.len()), "field"));
                if let Some(t) = &c.test { out.push_str(&render_expr(t)); }
                for s in &c.body { out.push_str(&render_stmt(s)); }
                out.push_str(close_node());
            }
            out.push_str(close_node());
        }
        Stmt::Class { name, super_class, body } => {
            out.push_str(&open_node(&format!("ClassDeclaration: {name} ({} members)", body.len()), "stmt"));
            if let Some(sc) = super_class {
                out.push_str(&open_node("extends", "field"));
                out.push_str(&render_expr(sc));
                out.push_str(close_node());
            }
            for _m in body {
                out.push_str(&leaf("ClassMember", "field"));
            }
            out.push_str(close_node());
        }
        Stmt::GeneratorFunc { name, params, body } => {
            out.push_str(&open_node(&format!("GeneratorFunc: {name}* ({} params, {} stmts)", params.len(), body.len()), "stmt"));
            out.push_str(close_node());
        }
        Stmt::AsyncFunc { name, params, body } => {
            out.push_str(&open_node(&format!("AsyncFunc: async {name} ({} params, {} stmts)", params.len(), body.len()), "stmt"));
            out.push_str(close_node());
        }
        Stmt::AsyncGeneratorFunc { name, params, body } => {
            out.push_str(&open_node(&format!("AsyncGeneratorFunc: async {name}* ({} params, {} stmts)", params.len(), body.len()), "stmt"));
            out.push_str(close_node());
        }
        Stmt::Import { source, specifiers } => {
            out.push_str(&open_node(&format!("Import \"{source}\" ({} specs)", specifiers.len()), "stmt"));
            for sp in specifiers {
                let lbl = match sp {
                    ImportSpecifier::Default(n) => format!("Default: {n}"),
                    ImportSpecifier::Named { imported, local } => format!("Named: {imported} as {local}"),
                    ImportSpecifier::Namespace(n) => format!("Namespace: {n}"),
                };
                out.push_str(&leaf(&lbl, "field"));
            }
            out.push_str(close_node());
        }
        Stmt::Export(kind) => {
            let label = match kind {
                ExportKind::Decl(_)    => "Export Decl",
                ExportKind::Default(_) => "Export Default",
                ExportKind::Named(ns)  => &format!("Export Named ({} items)", ns.len()),
            };
            out.push_str(&leaf(label, "stmt"));
        }
    }
    out
}

fn pattern_label(p: &Pattern) -> String {
    match p {
        Pattern::Ident(s) => s.clone(),
        Pattern::Array(_) => "[Array pattern]".into(),
        Pattern::Object(_) => "{Object pattern}".into(),
    }
}

fn render_params(params: &[Param]) -> String {
    let mut out = open_node("params", "field");
    for p in params {
        let lbl = if p.rest {
            format!("...{}", pattern_label(&p.pattern))
        } else {
            pattern_label(&p.pattern)
        };
        out.push_str(&leaf(&lbl, "field"));
    }
    out.push_str(close_node());
    out
}

pub fn render_expr(e: &Expr) -> String {
    let mut out = String::new();
    match e {
        Expr::Number(n) => out.push_str(&leaf(&format!("Number: {n}"), "literal")),
        Expr::BigInt(s) => out.push_str(&leaf(&format!("BigInt: {s}n"), "literal")),
        Expr::Str(s)    => out.push_str(&leaf(&format!("String: {s:?}"), "literal")),
        Expr::Bool(b)   => out.push_str(&leaf(&format!("Bool: {b}"), "literal")),
        Expr::Null      => out.push_str(&leaf("Null", "literal")),
        Expr::Undefined => out.push_str(&leaf("Undefined", "literal")),
        Expr::Regex(p, f) => out.push_str(&leaf(&format!("Regex: /{p}/{f}"), "literal")),
        Expr::Ident(s)  => out.push_str(&leaf(&format!("Identifier: {s}"), "ident")),

        Expr::Template { quasis, expressions } => {
            out.push_str(&open_node(&format!("TemplateLiteral ({} quasis, {} exprs)", quasis.len(), expressions.len()), "expr"));
            for (i, q) in quasis.iter().enumerate() {
                out.push_str(&leaf(&format!("quasi[{i}]: {q:?}"), "field"));
                if let Some(e) = expressions.get(i) {
                    out.push_str(&render_expr(e));
                }
            }
            out.push_str(close_node());
        }

        Expr::Array(items) => {
            out.push_str(&open_node(&format!("ArrayLiteral ({} items)", items.len()), "expr"));
            for item in items {
                match item {
                    Some(e) => out.push_str(&render_expr(e)),
                    None    => out.push_str(&leaf("(hole)", "field")),
                }
            }
            out.push_str(close_node());
        }
        Expr::Object(props) => {
            out.push_str(&open_node(&format!("ObjectLiteral ({} props)", props.len()), "expr"));
            for _p in props {
                out.push_str(&leaf("ObjectProp", "field"));
            }
            out.push_str(close_node());
        }

        Expr::Unary { op, arg } => {
            out.push_str(&open_node(&format!("Unary {:?}", op), "expr"));
            out.push_str(&render_expr(arg));
            out.push_str(close_node());
        }
        Expr::Binary { op, left, right } => {
            out.push_str(&open_node(&format!("Binary {:?}", op), "expr"));
            out.push_str(&render_expr(left));
            out.push_str(&render_expr(right));
            out.push_str(close_node());
        }
        Expr::Logical { op, left, right } => {
            out.push_str(&open_node(&format!("Logical {:?}", op), "expr"));
            out.push_str(&render_expr(left));
            out.push_str(&render_expr(right));
            out.push_str(close_node());
        }
        Expr::Ternary { test, yes, no } => {
            out.push_str(&open_node("Ternary", "expr"));
            out.push_str(&render_expr(test));
            out.push_str(&render_expr(yes));
            out.push_str(&render_expr(no));
            out.push_str(close_node());
        }
        Expr::Assign { op, target, value } => {
            out.push_str(&open_node(&format!("Assign {:?}", op), "expr"));
            out.push_str(&render_expr(target));
            out.push_str(&render_expr(value));
            out.push_str(close_node());
        }
        Expr::Call { callee, args, optional } => {
            let opt = if *optional { "?." } else { "" };
            out.push_str(&open_node(&format!("Call{opt} ({} args)", args.len()), "expr"));
            out.push_str(&open_node("callee", "field"));
            out.push_str(&render_expr(callee));
            out.push_str(close_node());
            for (i, a) in args.iter().enumerate() {
                out.push_str(&open_node(&format!("arg[{i}]"), "field"));
                out.push_str(&render_expr(a));
                out.push_str(close_node());
            }
            out.push_str(close_node());
        }
        Expr::New { callee, args } => {
            out.push_str(&open_node(&format!("New ({} args)", args.len()), "expr"));
            out.push_str(&render_expr(callee));
            for a in args { out.push_str(&render_expr(a)); }
            out.push_str(close_node());
        }
        Expr::Member { object, prop, optional } => {
            let opt = if *optional { "?." } else { "." };
            out.push_str(&open_node(&format!("Member {opt}"), "expr"));
            out.push_str(&render_expr(object));
            match prop {
                MemberProp::Ident(s) => out.push_str(&leaf(&format!(".{s}"), "field")),
                MemberProp::Computed(e) => {
                    out.push_str(&open_node("[computed]", "field"));
                    out.push_str(&render_expr(e));
                    out.push_str(close_node());
                }
            }
            out.push_str(close_node());
        }

        Expr::Function { name, params, body } => {
            let lbl = format!("FunctionExpr: {} ({} params)",
                name.as_deref().unwrap_or("(anon)"), params.len());
            out.push_str(&open_node(&lbl, "expr"));
            out.push_str(&open_node_collapsed(&format!("body ({} stmts)", body.len()), "field"));
            for s in body { out.push_str(&render_stmt(s)); }
            out.push_str(close_node());
            out.push_str(close_node());
        }
        Expr::Arrow { params, body } => {
            let body_label = match body {
                ArrowBody::Expr(_) => "expr-body".to_string(),
                ArrowBody::Block(s) => format!("body ({} stmts)", s.len()),
            };
            out.push_str(&open_node(&format!("ArrowFunction ({} params)", params.len()), "expr"));
            out.push_str(&open_node_collapsed(&body_label, "field"));
            match body {
                ArrowBody::Expr(e) => out.push_str(&render_expr(e)),
                ArrowBody::Block(s) => for st in s { out.push_str(&render_stmt(st)); },
            }
            out.push_str(close_node());
            out.push_str(close_node());
        }
        Expr::ClassExpr { name, super_class, body } => {
            let lbl = format!("ClassExpr: {} ({} members)",
                name.as_deref().unwrap_or("(anon)"), body.len());
            out.push_str(&open_node(&lbl, "expr"));
            if let Some(sc) = super_class { out.push_str(&render_expr(sc)); }
            out.push_str(close_node());
        }
        Expr::Sequence(exprs) => {
            out.push_str(&open_node(&format!("Sequence ({} exprs)", exprs.len()), "expr"));
            for e in exprs { out.push_str(&render_expr(e)); }
            out.push_str(close_node());
        }
        Expr::Spread(e) => {
            out.push_str(&open_node("Spread", "expr"));
            out.push_str(&render_expr(e));
            out.push_str(close_node());
        }
        Expr::Yield { value, delegate } => {
            let lbl = if *delegate { "Yield*" } else { "Yield" };
            out.push_str(&open_node(lbl, "expr"));
            if let Some(v) = value { out.push_str(&render_expr(v)); }
            out.push_str(close_node());
        }
        Expr::Await { value } => {
            out.push_str(&open_node("Await", "expr"));
            out.push_str(&render_expr(value));
            out.push_str(close_node());
        }
        Expr::AsyncFunc { name, params, body } => {
            let lbl = format!("AsyncFunctionExpr: {} ({} params, {} stmts)",
                name.as_deref().unwrap_or("(anon)"), params.len(), body.len());
            out.push_str(&leaf(&lbl, "expr"));
        }
        Expr::GeneratorFunc { name, params, body } => {
            let lbl = format!("GeneratorFunctionExpr: {}* ({} params, {} stmts)",
                name.as_deref().unwrap_or("(anon)"), params.len(), body.len());
            out.push_str(&leaf(&lbl, "expr"));
        }
        Expr::DynamicImport(arg) => {
            out.push_str(&open_node("DynamicImport", "expr"));
            out.push_str(&render_expr(arg));
            out.push_str(close_node());
        }
    }
    out
}
