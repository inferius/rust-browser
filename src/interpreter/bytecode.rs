//! Bytecode VM pro JS interpreter.
//!
//! MVP: stack-based VM, hot-loop fast-path. Tree-walking interpreter zustava
//! authoritative pro non-trivial features (try/catch, classes, generators, async).
//! VM je used jen kde compile() succeeds.
//!
//! Architektura:
//! - Opcode: enum opcodes
//! - CodeBlock: bytecode + constants pool + var_names + jmp_targets
//! - compile(expr/stmt) -> Option<CodeBlock>: vrati None pri non-supported AST
//! - VM::run(&CodeBlock, &mut Scope) -> JsValue
//!
//! Cilem v1: aritmetika, srovnavani, logika, var/let/const decls, assignments,
//! if/else, while/for, function call (existing JS funkce). Concretne cca 30 opcodes.

use crate::ast::{Expr, BinaryOp, UnaryOp, Stmt, LogicalOp, AssignOp, Pattern, ForInit, MemberProp, PropKey, ArrowBody};

// Thread-local compile-time scratch:
// - OUTER_VARS_STACK: stack of outer var_names (push pri vstupu do function body).
// - CAPTURES_STACK: stack of captures vec (paralelni s OUTER_VARS_STACK).
//   Pri free var detection v body, push outer_idx -> aktualni captures Vec.
thread_local! {
    static OUTER_VARS_STACK: std::cell::RefCell<Vec<Vec<String>>> = std::cell::RefCell::new(Vec::new());
    static CAPTURES_STACK: std::cell::RefCell<Vec<Vec<u16>>> = std::cell::RefCell::new(Vec::new());
}

/// Push outer scope info pred compile body.
fn push_outer_scope(outer_vars: Vec<String>) {
    OUTER_VARS_STACK.with(|s| s.borrow_mut().push(outer_vars));
    CAPTURES_STACK.with(|s| s.borrow_mut().push(Vec::new()));
}

/// Pop a vrat collected captures pro tuto level.
fn pop_outer_scope() -> Vec<u16> {
    OUTER_VARS_STACK.with(|s| { s.borrow_mut().pop(); });
    CAPTURES_STACK.with(|s| s.borrow_mut().pop().unwrap_or_default())
}

/// Pri Ident lookup, kdyz neni v locals, zkus capture z outer.
/// Vrati Some(captures_idx) pokud je capture, jinak None (= LoadGlobal).
fn try_capture(name: &str) -> Option<u16> {
    OUTER_VARS_STACK.with(|stack| {
        let outer_stack = stack.borrow();
        let outer = outer_stack.last()?;
        let outer_idx = outer.iter().position(|n| n == name)? as u16;
        // Pridej do captures (dedupe).
        CAPTURES_STACK.with(|cs| {
            let mut cap_stack = cs.borrow_mut();
            let cap_vec = cap_stack.last_mut()?;
            // Dedupe.
            if let Some(i) = cap_vec.iter().position(|&x| x == outer_idx) {
                return Some(i as u16);
            }
            let idx = cap_vec.len() as u16;
            cap_vec.push(outer_idx);
            Some(idx)
        })
    })
}
use super::{JsValue};

#[derive(Debug, Clone, Copy)]
pub enum Opcode {
    // Stack manipulation
    LoadConst(u16),       // push constants[u16]
    LoadVar(u16),         // push scope.get(var_names[u16])
    StoreVar(u16),        // scope.set(var_names[u16], pop())
    DeclareVar(u16),      // scope.declare(var_names[u16], pop())  - var/let/const
    Pop,                  // pop, discard
    Dup,                  // duplicate top
    LoadUndefined,
    LoadNull,
    LoadTrue,
    LoadFalse,
    LoadZero,             // push Number(0.0) - common
    LoadOne,               // push Number(1.0) - common

    // Arithmetic
    Add, Sub, Mul, Div, Mod, Exp,
    Neg, Pos, Not, BitNot,

    // Comparison
    Eq, NotEq, StrictEq, StrictNotEq,
    Lt, Gt, LtEq, GtEq,

    // Bitwise
    BitAnd, BitOr, BitXor, Shl, Shr, Ushr,

    // Control flow
    Jmp(i32),             // unconditional jump
    JmpIfFalse(i32),      // pop, if !truthy jump
    JmpIfTrue(i32),       // pop, if truthy jump
    JmpIfFalseKeep(i32),  // peek (don't pop), if !truthy jump - pro && short-circuit
    JmpIfTrueKeep(i32),   // peek, if truthy jump - pro || short-circuit
    JmpIfNotNullishKeep(i32), // peek, if not null/undef jump - pro ?? short-circuit

    // Increment/Decrement (in-place na lokalni var).
    Inc(u16),             // var_names[u16]++ (pre/post differs)
    Dec(u16),
    PostInc(u16),         // push original + locals[u16]+=1
    PostDec(u16),

    // Member access (obj.prop), pop=obj, push=obj[prop].
    GetProp(u16),         // var_names[u16] = property name (stored jako str)
    GetIndex,             // pop key, pop obj, push obj[key]
    /// Pop value, pop obj, set obj[var_names[u16]] = value. Push value (assignment vrati v).
    SetProp(u16),
    /// Pop value, pop key, pop obj, set obj[key] = value. Push value.
    SetIndex,

    // Array/Object literal construction.
    NewArray(u16),        // pop u16 hodnot ze stacku, push Array<top..bottom>
    NewObject(u16),       // pop 2*u16 (key/value pairs), push Object

    // Global lookup (env-bound) + native function call.
    // LoadGlobal: vyhleda globalni promennou v Environment (Math, console, ...).
    // CallNative(argc): pop argc args, pop callee, invoke (must be JsFunc::Native), push result.
    LoadGlobal(u16),      // var_names[u16]
    CallNative(u16),      // argc

    // typeof/void
    TypeOf,               // pop, push string typeof
    LoadStrConst(u16),    // alias LoadConst pro strings - same impl

    // Returns
    Return,               // return pop()
    Halt,                 // konec compiled bloku

    // User function support.
    LoadFunction(u16),    // index do CodeBlock.functions, push JsValue::Function(VmCompiled).
                          // Pri kompilaci si uchoví indexy outer-locals pro closure capture
                          // v compiled.captures_outer_indices. VM pri LoadFunction snapne
                          // outer locals[idx] do kapture vec a embedne do JsFunc::VmCompiled.
    LoadCapture(u16),     // push captures[u16] - cte z aktualne bezici closure frame.

    // Array spread support pri literal [a, ...b].
    /// Pop value, append do top-of-stack Array (Array zustane na stacku).
    AppendItem,
    /// Pop source Array, iterate jeho elements + append do top-of-stack Array.
    AppendSpread,
    /// Pop Array (args), pop callee, call s array elements jako args.
    CallNativeArgs,
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub bytecode: Vec<Opcode>,
    pub constants: Vec<JsValue>,
    pub var_names: Vec<String>,
    /// Per-loop break/continue jumps stack (transient pri compile).
    /// Push pri vstupu do loopu, pop pri vystupu. Vrstva = (break_jumps, cont_jumps, cont_target_idx).
    pub loop_stack: Vec<LoopFrame>,
    /// Vnoreni funkce - LoadFunction(idx) reference.
    pub functions: Vec<std::rc::Rc<CompiledFunction>>,
}

/// Compiled user-defined function.
#[derive(Debug, Clone)]
pub struct CompiledFunction {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub code: CodeBlock,
    /// Closure captures: pro kazdy free var v body si pamatujem index v outer
    /// var_names. Pri LoadFunction VM nacte hodnoty z outer locals[idx] a vlozi
    /// do JsFunc::VmCompiled.captures vec.
    pub captures_outer_indices: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct LoopFrame {
    /// Jump indices co cili na break-target (= konec loopu).
    pub break_jumps: Vec<usize>,
    /// Jump indices co cili na continue-target (= test/update v for/while).
    pub continue_jumps: Vec<usize>,
}

impl CodeBlock {
    pub fn new() -> Self {
        Self {
            bytecode: Vec::new(),
            constants: Vec::new(),
            var_names: Vec::new(),
            loop_stack: Vec::new(),
            functions: Vec::new(),
        }
    }
    fn push_const(&mut self, v: JsValue) -> u16 {
        // Try dedupe na primitivech.
        for (i, c) in self.constants.iter().enumerate() {
            if values_strict_eq(c, &v) {
                return i as u16;
            }
        }
        let idx = self.constants.len();
        self.constants.push(v);
        idx as u16
    }
    fn push_var(&mut self, name: &str) -> u16 {
        for (i, n) in self.var_names.iter().enumerate() {
            if n == name { return i as u16; }
        }
        let idx = self.var_names.len();
        self.var_names.push(name.to_string());
        idx as u16
    }
    fn emit(&mut self, op: Opcode) -> usize {
        let idx = self.bytecode.len();
        self.bytecode.push(op);
        idx
    }
    fn patch_jmp(&mut self, idx: usize, target: usize) {
        let offset = (target as i32) - (idx as i32) - 1;
        match &mut self.bytecode[idx] {
            Opcode::Jmp(o) | Opcode::JmpIfFalse(o) | Opcode::JmpIfTrue(o)
            | Opcode::JmpIfFalseKeep(o) | Opcode::JmpIfTrueKeep(o)
            | Opcode::JmpIfNotNullishKeep(o) => *o = offset,
            _ => panic!("patch_jmp na non-jump opcode"),
        }
    }
}

fn values_strict_eq(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::Str(x), JsValue::Str(y)) => x == y,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Undefined, JsValue::Undefined) => true,
        _ => false,
    }
}

/// Zkus zkompilovat vyraz do bytecode. Pri non-supported AST vrati None.
pub fn compile_expr(e: &Expr, code: &mut CodeBlock) -> Result<(), &'static str> {
    match e {
        Expr::Number(n) => {
            if *n == 0.0 { code.emit(Opcode::LoadZero); }
            else if *n == 1.0 { code.emit(Opcode::LoadOne); }
            else {
                let idx = code.push_const(JsValue::Number(*n));
                code.emit(Opcode::LoadConst(idx));
            }
            Ok(())
        }
        Expr::Str(s) => {
            let idx = code.push_const(JsValue::Str(s.clone()));
            code.emit(Opcode::LoadConst(idx));
            Ok(())
        }
        Expr::Bool(b) => {
            code.emit(if *b { Opcode::LoadTrue } else { Opcode::LoadFalse });
            Ok(())
        }
        Expr::Null => { code.emit(Opcode::LoadNull); Ok(()) }
        Expr::Undefined => { code.emit(Opcode::LoadUndefined); Ok(()) }
        Expr::Ident(name) => {
            // Lookup order: 1) lokalni var, 2) closure capture, 3) global.
            if let Some(idx) = code.var_names.iter().position(|n| n == name) {
                code.emit(Opcode::LoadVar(idx as u16));
            } else if let Some(cap_idx) = try_capture(name) {
                code.emit(Opcode::LoadCapture(cap_idx));
            } else {
                let idx = code.push_var(name);
                code.emit(Opcode::LoadVar(idx));
            }
            Ok(())
        }
        Expr::Unary { op, arg } => {
            // Pre-inc/dec na ident: in-place opcode.
            if matches!(op, UnaryOp::PreInc | UnaryOp::PreDec) {
                if let Expr::Ident(name) = arg.as_ref() {
                    let var_idx = code.push_var(name);
                    code.emit(if matches!(op, UnaryOp::PreInc) { Opcode::Inc(var_idx) } else { Opcode::Dec(var_idx) });
                    code.emit(Opcode::LoadVar(var_idx)); // push novou hodnotu
                    return Ok(());
                }
                return Err("pre-inc/dec na non-ident");
            }
            if matches!(op, UnaryOp::Typeof) {
                compile_expr(arg, code)?;
                code.emit(Opcode::TypeOf);
                return Ok(());
            }
            compile_expr(arg, code)?;
            match op {
                UnaryOp::Minus => code.emit(Opcode::Neg),
                UnaryOp::Plus => code.emit(Opcode::Pos),
                UnaryOp::Not => code.emit(Opcode::Not),
                UnaryOp::BitNot => code.emit(Opcode::BitNot),
                UnaryOp::Void => {
                    code.emit(Opcode::Pop);
                    code.emit(Opcode::LoadUndefined);
                    return Ok(());
                }
                _ => return Err("unsupported unary op"),
            };
            Ok(())
        }
        Expr::Binary { op, left, right } => {
            // Post-inc/dec: BinaryOp varianty PostInc/PostDec na ident.
            if matches!(op, BinaryOp::PostInc | BinaryOp::PostDec) {
                if let Expr::Ident(name) = left.as_ref() {
                    let var_idx = code.push_var(name);
                    code.emit(if matches!(op, BinaryOp::PostInc) {
                        Opcode::PostInc(var_idx)
                    } else {
                        Opcode::PostDec(var_idx)
                    });
                    return Ok(());
                }
                return Err("post-inc/dec na non-ident");
            }
            compile_expr(left, code)?;
            compile_expr(right, code)?;
            let opc = match op {
                BinaryOp::Add => Opcode::Add,
                BinaryOp::Sub => Opcode::Sub,
                BinaryOp::Mul => Opcode::Mul,
                BinaryOp::Div => Opcode::Div,
                BinaryOp::Mod => Opcode::Mod,
                BinaryOp::Exp => Opcode::Exp,
                BinaryOp::Eq => Opcode::Eq,
                BinaryOp::NotEq => Opcode::NotEq,
                BinaryOp::StrictEq => Opcode::StrictEq,
                BinaryOp::StrictNotEq => Opcode::StrictNotEq,
                BinaryOp::Lt => Opcode::Lt,
                BinaryOp::Gt => Opcode::Gt,
                BinaryOp::LtEq => Opcode::LtEq,
                BinaryOp::GtEq => Opcode::GtEq,
                BinaryOp::BitAnd => Opcode::BitAnd,
                BinaryOp::BitOr => Opcode::BitOr,
                BinaryOp::BitXor => Opcode::BitXor,
                BinaryOp::Shl => Opcode::Shl,
                BinaryOp::Shr => Opcode::Shr,
                BinaryOp::Ushr => Opcode::Ushr,
                _ => return Err("unsupported binary op"),
            };
            code.emit(opc);
            Ok(())
        }
        Expr::Logical { op, left, right } => {
            // Short-circuit: emit left, peek-test, jump if known result, pop+eval right.
            compile_expr(left, code)?;
            let jmp_idx = match op {
                LogicalOp::And => code.emit(Opcode::JmpIfFalseKeep(0)),
                LogicalOp::Or => code.emit(Opcode::JmpIfTrueKeep(0)),
                LogicalOp::NullCoal => code.emit(Opcode::JmpIfNotNullishKeep(0)),
            };
            // Po jumpu (kdyz neskoceno) discard left, vyhodnoceni right.
            code.emit(Opcode::Pop);
            compile_expr(right, code)?;
            let target = code.bytecode.len();
            code.patch_jmp(jmp_idx, target);
            Ok(())
        }
        Expr::Ternary { test, yes, no } => {
            compile_expr(test, code)?;
            let jmp_to_no = code.emit(Opcode::JmpIfFalse(0));
            compile_expr(yes, code)?;
            let jmp_to_end = code.emit(Opcode::Jmp(0));
            let no_target = code.bytecode.len();
            code.patch_jmp(jmp_to_no, no_target);
            compile_expr(no, code)?;
            let end = code.bytecode.len();
            code.patch_jmp(jmp_to_end, end);
            Ok(())
        }
        Expr::Assign { op, target, value } => {
            // Member target: obj.prop = v -> emit obj, value, SetProp.
            if let Expr::Member { object, prop, optional: _ } = target.as_ref() {
                if !matches!(op, AssignOp::Assign) {
                    return Err("compound assign na member not supported");
                }
                compile_expr(object, code)?;
                match prop {
                    MemberProp::Ident(name) => {
                        compile_expr(value, code)?;
                        let key_idx = code.push_var(name);
                        code.emit(Opcode::SetProp(key_idx));
                    }
                    MemberProp::Computed(e) => {
                        compile_expr(e, code)?;
                        compile_expr(value, code)?;
                        code.emit(Opcode::SetIndex);
                    }
                }
                return Ok(());
            }
            if let Expr::Ident(name) = target.as_ref() {
                let var_idx = code.push_var(name);
                match op {
                    AssignOp::Assign => {
                        compile_expr(value, code)?;
                        code.emit(Opcode::Dup); // hodnota assignmentu = nova hodnota
                        code.emit(Opcode::StoreVar(var_idx));
                    }
                    AssignOp::LogicalAnd => {
                        // lhs &&= rhs: pri lhs truthy, lhs = rhs (jinak nech).
                        code.emit(Opcode::LoadVar(var_idx));
                        let jmp_skip = code.emit(Opcode::JmpIfFalseKeep(0));
                        // truthy: pop lhs (uz pushedy), compile rhs, store, dup
                        code.emit(Opcode::Pop);
                        compile_expr(value, code)?;
                        code.emit(Opcode::Dup);
                        code.emit(Opcode::StoreVar(var_idx));
                        let target = code.bytecode.len();
                        code.patch_jmp(jmp_skip, target);
                        // Pri falsy: lhs (puvodne pushed) zustane na stacku jako result.
                    }
                    AssignOp::LogicalOr => {
                        code.emit(Opcode::LoadVar(var_idx));
                        let jmp_skip = code.emit(Opcode::JmpIfTrueKeep(0));
                        code.emit(Opcode::Pop);
                        compile_expr(value, code)?;
                        code.emit(Opcode::Dup);
                        code.emit(Opcode::StoreVar(var_idx));
                        let target = code.bytecode.len();
                        code.patch_jmp(jmp_skip, target);
                    }
                    AssignOp::NullCoal => {
                        code.emit(Opcode::LoadVar(var_idx));
                        let jmp_skip = code.emit(Opcode::JmpIfNotNullishKeep(0));
                        code.emit(Opcode::Pop);
                        compile_expr(value, code)?;
                        code.emit(Opcode::Dup);
                        code.emit(Opcode::StoreVar(var_idx));
                        let target = code.bytecode.len();
                        code.patch_jmp(jmp_skip, target);
                    }
                    _ => {
                        // Compound: lhs <op>= rhs => lhs = lhs <op> rhs
                        // Stack: load lhs, load rhs, op, dup, store
                        code.emit(Opcode::LoadVar(var_idx));
                        compile_expr(value, code)?;
                        let bin_op = match op {
                            AssignOp::Add => Opcode::Add,
                            AssignOp::Sub => Opcode::Sub,
                            AssignOp::Mul => Opcode::Mul,
                            AssignOp::Div => Opcode::Div,
                            AssignOp::Mod => Opcode::Mod,
                            AssignOp::Exp => Opcode::Exp,
                            AssignOp::BitAnd => Opcode::BitAnd,
                            AssignOp::BitOr => Opcode::BitOr,
                            AssignOp::BitXor => Opcode::BitXor,
                            AssignOp::Shl => Opcode::Shl,
                            AssignOp::Shr => Opcode::Shr,
                            AssignOp::Ushr => Opcode::Ushr,
                            _ => return Err("unsupported compound assign"),
                        };
                        code.emit(bin_op);
                        code.emit(Opcode::Dup);
                        code.emit(Opcode::StoreVar(var_idx));
                    }
                }
                Ok(())
            } else {
                Err("assign target not ident")
            }
        }
        Expr::Template { quasis, expressions } => {
            // Compile: quasi[0] + expr[0] + quasi[1] + expr[1] + ... + quasi[n].
            let first_idx = code.push_const(JsValue::Str(quasis.first().cloned().unwrap_or_default()));
            code.emit(Opcode::LoadConst(first_idx));
            for (i, expr) in expressions.iter().enumerate() {
                compile_expr(expr, code)?;
                code.emit(Opcode::Add);
                if let Some(quasi) = quasis.get(i + 1) {
                    if !quasi.is_empty() {
                        let q_idx = code.push_const(JsValue::Str(quasi.clone()));
                        code.emit(Opcode::LoadConst(q_idx));
                        code.emit(Opcode::Add);
                    }
                }
            }
            Ok(())
        }
        Expr::Member { object, prop, optional } => {
            compile_expr(object, code)?;
            if *optional {
                // obj?.prop: pri null/undef vrat undefined.
                let jmp_proceed = code.emit(Opcode::JmpIfNotNullishKeep(0));
                code.emit(Opcode::Pop);
                code.emit(Opcode::LoadUndefined);
                let jmp_end = code.emit(Opcode::Jmp(0));
                let proceed = code.bytecode.len();
                code.patch_jmp(jmp_proceed, proceed);
                match prop {
                    MemberProp::Ident(name) => {
                        let key_idx = code.push_var(name);
                        code.emit(Opcode::GetProp(key_idx));
                    }
                    MemberProp::Computed(e) => {
                        compile_expr(e, code)?;
                        code.emit(Opcode::GetIndex);
                    }
                }
                let end = code.bytecode.len();
                code.patch_jmp(jmp_end, end);
            } else {
                match prop {
                    MemberProp::Ident(name) => {
                        let key_idx = code.push_var(name);
                        code.emit(Opcode::GetProp(key_idx));
                    }
                    MemberProp::Computed(e) => {
                        compile_expr(e, code)?;
                        code.emit(Opcode::GetIndex);
                    }
                }
            }
            Ok(())
        }
        Expr::Array(items) => {
            // Pri pritomnem Spread: builduj inkrementalne pres NewArray(0) + Append*.
            // Jinak: rychly NewArray(N) z fixed slotu.
            let has_spread = items.iter().any(|i|
                matches!(i.as_ref().map(|e| e.as_ref()), Some(Expr::Spread(_))));
            if !has_spread {
                if items.len() > u16::MAX as usize { return Err("array > 65k items"); }
                for item in items {
                    if let Some(e) = item {
                        compile_expr(e, code)?;
                    } else {
                        code.emit(Opcode::LoadUndefined);
                    }
                }
                code.emit(Opcode::NewArray(items.len() as u16));
            } else {
                // Inkrementalni build.
                code.emit(Opcode::NewArray(0));
                for item in items {
                    if let Some(e) = item {
                        if let Expr::Spread(inner) = e.as_ref() {
                            compile_expr(inner, code)?;
                            code.emit(Opcode::AppendSpread);
                        } else {
                            compile_expr(e, code)?;
                            code.emit(Opcode::AppendItem);
                        }
                    } else {
                        code.emit(Opcode::LoadUndefined);
                        code.emit(Opcode::AppendItem);
                    }
                }
            }
            Ok(())
        }
        Expr::Call { callee, args, optional } => {
            let opt = *optional;
            // Callee resolution: local var vs global.
            // Pri Ident: pokud existuje v var_names UZ pred timto callem (function decl
            // appears earlier), pouzij LoadVar, jinak LoadGlobal.
            match callee.as_ref() {
                Expr::Ident(name) => {
                    if let Some(idx) = code.var_names.iter().position(|n| n == name) {
                        code.emit(Opcode::LoadVar(idx as u16));
                    } else {
                        let g_idx = code.push_var(name);
                        code.emit(Opcode::LoadGlobal(g_idx));
                    }
                }
                Expr::Member { object, prop, optional: _ } => {
                    if let Expr::Ident(obj_name) = object.as_ref() {
                        if let Some(idx) = code.var_names.iter().position(|n| n == obj_name) {
                            code.emit(Opcode::LoadVar(idx as u16));
                        } else {
                            let g_idx = code.push_var(obj_name);
                            code.emit(Opcode::LoadGlobal(g_idx));
                        }
                    } else {
                        compile_expr(object, code)?;
                    }
                    match prop {
                        MemberProp::Ident(name) => {
                            let key_idx = code.push_var(name);
                            code.emit(Opcode::GetProp(key_idx));
                        }
                        MemberProp::Computed(e) => {
                            compile_expr(e, code)?;
                            code.emit(Opcode::GetIndex);
                        }
                    }
                }
                _ => return Err("complex callee not supported"),
            }
            if args.len() > u16::MAX as usize { return Err("too many args"); }
            let has_spread_arg = args.iter().any(|a| matches!(a, Expr::Spread(_)));
            let emit_call = |code: &mut CodeBlock, args: &[Expr]| -> Result<(), &'static str> {
                if has_spread_arg {
                    // Build args Array via NewArray(0) + AppendItem/AppendSpread.
                    code.emit(Opcode::NewArray(0));
                    for arg in args {
                        if let Expr::Spread(inner) = arg {
                            compile_expr(inner, code)?;
                            code.emit(Opcode::AppendSpread);
                        } else {
                            compile_expr(arg, code)?;
                            code.emit(Opcode::AppendItem);
                        }
                    }
                    code.emit(Opcode::CallNativeArgs);
                } else {
                    for arg in args {
                        compile_expr(arg, code)?;
                    }
                    code.emit(Opcode::CallNative(args.len() as u16));
                }
                Ok(())
            };
            if opt {
                let jmp_proceed = code.emit(Opcode::JmpIfNotNullishKeep(0));
                code.emit(Opcode::Pop);
                code.emit(Opcode::LoadUndefined);
                let jmp_end = code.emit(Opcode::Jmp(0));
                let proceed = code.bytecode.len();
                code.patch_jmp(jmp_proceed, proceed);
                emit_call(code, args)?;
                let end = code.bytecode.len();
                code.patch_jmp(jmp_end, end);
            } else {
                emit_call(code, args)?;
            }
            Ok(())
        }
        Expr::Arrow { params, body } => {
            // Snapshot outer var_names PRED compile body.
            let outer_vars_snapshot = code.var_names.clone();
            let mut fn_code = CodeBlock::new();
            // Anonymous - no slot 0 self.
            let mut param_names: Vec<String> = Vec::new();
            for p in params {
                if let Pattern::Ident(pn) = &p.pattern {
                    param_names.push(pn.clone());
                    fn_code.push_var(pn);
                } else {
                    return Err("destructuring arrow param not supported");
                }
            }
            push_outer_scope(outer_vars_snapshot);
            let body_result = (|| -> Result<(), &'static str> {
                match body {
                    ArrowBody::Expr(e) => {
                        compile_expr(e, &mut fn_code)?;
                        fn_code.emit(Opcode::Return);
                    }
                    ArrowBody::Block(stmts) => {
                        for s in stmts {
                            compile_stmt(s, &mut fn_code)?;
                        }
                        fn_code.emit(Opcode::LoadUndefined);
                        fn_code.emit(Opcode::Return);
                    }
                }
                Ok(())
            })();
            let captures_outer_indices = pop_outer_scope();
            body_result?;
            let compiled = std::rc::Rc::new(CompiledFunction {
                name: None,
                params: param_names,
                code: fn_code,
                captures_outer_indices,
            });
            let fn_idx = code.functions.len() as u16;
            code.functions.push(compiled);
            code.emit(Opcode::LoadFunction(fn_idx));
            Ok(())
        }
        Expr::Function { name, params, body } => {
            // Anonymous nebo named function expression. Stejny postup jako Arrow.
            let outer_vars_snapshot = code.var_names.clone();
            let mut fn_code = CodeBlock::new();
            // Pri named: pre-register self na slot 0.
            if let Some(n) = name {
                fn_code.push_var(n);
            }
            let mut param_names: Vec<String> = Vec::new();
            for p in params {
                if let Pattern::Ident(pn) = &p.pattern {
                    param_names.push(pn.clone());
                    fn_code.push_var(pn);
                } else {
                    return Err("destructuring fn-expr param not supported");
                }
            }
            push_outer_scope(outer_vars_snapshot);
            let body_result = (|| -> Result<(), &'static str> {
                for s in body {
                    compile_stmt(s, &mut fn_code)?;
                }
                fn_code.emit(Opcode::LoadUndefined);
                fn_code.emit(Opcode::Return);
                Ok(())
            })();
            let captures_outer_indices = pop_outer_scope();
            body_result?;
            let compiled = std::rc::Rc::new(CompiledFunction {
                name: name.clone(),
                params: param_names,
                code: fn_code,
                captures_outer_indices,
            });
            let fn_idx = code.functions.len() as u16;
            code.functions.push(compiled);
            code.emit(Opcode::LoadFunction(fn_idx));
            Ok(())
        }
        Expr::Object(props) => {
            if props.len() > u16::MAX as usize { return Err("object > 65k props"); }
            for prop in props {
                if prop.computed { return Err("computed object key not supported"); }
                let key_str = match &prop.key {
                    PropKey::Ident(s) => s.clone(),
                    PropKey::Str(s) => s.clone(),
                    PropKey::Num(n) => format!("{}", n),
                    PropKey::Computed(_) => return Err("computed object key not supported"),
                };
                let key_idx = code.push_const(JsValue::Str(key_str));
                code.emit(Opcode::LoadConst(key_idx));
                compile_expr(&prop.value, code)?;
            }
            code.emit(Opcode::NewObject(props.len() as u16));
            Ok(())
        }
        _ => Err("unsupported expr"),
    }
}

/// Zkus zkompilovat statement. Vrati Err("...") pri non-supported.
pub fn compile_stmt(s: &Stmt, code: &mut CodeBlock) -> Result<(), &'static str> {
    match s {
        Stmt::Expr(e) => {
            compile_expr(e, code)?;
            code.emit(Opcode::Pop);
            Ok(())
        }
        Stmt::Block(stmts) => {
            for st in stmts {
                compile_stmt(st, code)?;
            }
            Ok(())
        }
        Stmt::If { test, yes, no } => {
            compile_expr(test, code)?;
            let jmp_to_else = code.emit(Opcode::JmpIfFalse(0));
            compile_stmt(yes, code)?;
            let jmp_to_end = code.emit(Opcode::Jmp(0));
            let else_target = code.bytecode.len();
            code.patch_jmp(jmp_to_else, else_target);
            if let Some(alt) = no {
                compile_stmt(alt, code)?;
            }
            let end = code.bytecode.len();
            code.patch_jmp(jmp_to_end, end);
            Ok(())
        }
        Stmt::While { test, body } => {
            code.loop_stack.push(LoopFrame { break_jumps: vec![], continue_jumps: vec![] });
            let loop_start = code.bytecode.len();
            compile_expr(test, code)?;
            let jmp_to_end = code.emit(Opcode::JmpIfFalse(0));
            compile_stmt(body, code)?;
            // Continue cili na loop_start (re-test).
            let frame = code.loop_stack.last().cloned().unwrap();
            for ci in &frame.continue_jumps {
                code.patch_jmp(*ci, loop_start);
            }
            // Skoc zpet na test.
            let back = code.emit(Opcode::Jmp(0));
            code.patch_jmp(back, loop_start);
            let end = code.bytecode.len();
            code.patch_jmp(jmp_to_end, end);
            // Break cili na end.
            let frame = code.loop_stack.pop().unwrap();
            for bj in &frame.break_jumps {
                code.patch_jmp(*bj, end);
            }
            Ok(())
        }
        Stmt::DoWhile { body, test } => {
            code.loop_stack.push(LoopFrame { break_jumps: vec![], continue_jumps: vec![] });
            let body_start = code.bytecode.len();
            compile_stmt(body, code)?;
            let test_target = code.bytecode.len();
            let frame = code.loop_stack.last().cloned().unwrap();
            for ci in &frame.continue_jumps {
                code.patch_jmp(*ci, test_target);
            }
            compile_expr(test, code)?;
            let back = code.emit(Opcode::JmpIfTrue(0));
            code.patch_jmp(back, body_start);
            let end = code.bytecode.len();
            let frame = code.loop_stack.pop().unwrap();
            for bj in &frame.break_jumps {
                code.patch_jmp(*bj, end);
            }
            Ok(())
        }
        Stmt::For { init, test, update, body } => {
            // init
            if let Some(init) = init {
                match init {
                    ForInit::Var { kind: _, decls } => {
                        for decl in decls {
                            if let Pattern::Ident(name) = &decl.pattern {
                                let var_idx = code.push_var(name);
                                if let Some(init_e) = &decl.init {
                                    compile_expr(init_e, code)?;
                                } else {
                                    code.emit(Opcode::LoadUndefined);
                                }
                                code.emit(Opcode::DeclareVar(var_idx));
                            } else {
                                return Err("for-init destructuring not supported");
                            }
                        }
                    }
                    ForInit::Expr(e) => {
                        compile_expr(e, code)?;
                        code.emit(Opcode::Pop);
                    }
                }
            }
            code.loop_stack.push(LoopFrame { break_jumps: vec![], continue_jumps: vec![] });
            let test_pos = code.bytecode.len();
            let mut jmp_to_end = None;
            if let Some(t) = test {
                compile_expr(t, code)?;
                jmp_to_end = Some(code.emit(Opcode::JmpIfFalse(0)));
            }
            // Body
            compile_stmt(body, code)?;
            // Continue target = update (or test if no update)
            let cont_target = code.bytecode.len();
            let frame = code.loop_stack.last().cloned().unwrap();
            for ci in &frame.continue_jumps {
                code.patch_jmp(*ci, cont_target);
            }
            // Update
            if let Some(u) = update {
                compile_expr(u, code)?;
                code.emit(Opcode::Pop);
            }
            // Jump back na test
            let back = code.emit(Opcode::Jmp(0));
            code.patch_jmp(back, test_pos);
            let end = code.bytecode.len();
            if let Some(je) = jmp_to_end {
                code.patch_jmp(je, end);
            }
            let frame = code.loop_stack.pop().unwrap();
            for bj in &frame.break_jumps {
                code.patch_jmp(*bj, end);
            }
            Ok(())
        }
        Stmt::Break(label) => {
            if label.is_some() { return Err("labeled break not supported"); }
            if code.loop_stack.is_empty() { return Err("break outside loop"); }
            let idx = code.emit(Opcode::Jmp(0));
            code.loop_stack.last_mut().unwrap().break_jumps.push(idx);
            Ok(())
        }
        Stmt::Continue(label) => {
            if label.is_some() { return Err("labeled continue not supported"); }
            if code.loop_stack.is_empty() { return Err("continue outside loop"); }
            let idx = code.emit(Opcode::Jmp(0));
            code.loop_stack.last_mut().unwrap().continue_jumps.push(idx);
            Ok(())
        }
        Stmt::Empty => Ok(()),
        Stmt::Var { kind: _, decls } => {
            for decl in decls {
                if let Pattern::Ident(name) = &decl.pattern {
                    let var_idx = code.push_var(name);
                    if let Some(init) = &decl.init {
                        compile_expr(init, code)?;
                    } else {
                        code.emit(Opcode::LoadUndefined);
                    }
                    code.emit(Opcode::DeclareVar(var_idx));
                } else {
                    return Err("destructuring var not supported");
                }
            }
            Ok(())
        }
        Stmt::Function { name, params, body } => {
            // Pre-register name v outer var_names pro pripadnou rekurzi.
            let var_idx = code.push_var(name);
            // Snapshot outer var_names PRED compile body (pro closure capture).
            let outer_vars_snapshot = code.var_names.clone();
            // Compile body do nove CompiledFunction.
            let mut fn_code = CodeBlock::new();
            // Pre-register function name in body's var_names PRVNI - rekurze cez
            // LoadVar(0). VM CallNative s VmCompiled inicializuje locals[0] = self.
            fn_code.push_var(name);
            let mut param_names: Vec<String> = Vec::new();
            for p in params {
                if let crate::ast::Pattern::Ident(pn) = &p.pattern {
                    param_names.push(pn.clone());
                    fn_code.push_var(pn);
                } else {
                    return Err("destructuring param not supported");
                }
            }
            // Push outer scope context pro closure capture detection.
            push_outer_scope(outer_vars_snapshot);
            // Compile body.
            let body_result = (|| -> Result<(), &'static str> {
                for s in body {
                    compile_stmt(s, &mut fn_code)?;
                }
                Ok(())
            })();
            let captures_outer_indices = pop_outer_scope();
            body_result?;
            // Implicit return undefined po konci body.
            fn_code.emit(Opcode::LoadUndefined);
            fn_code.emit(Opcode::Return);
            let compiled = std::rc::Rc::new(CompiledFunction {
                name: Some(name.clone()),
                params: param_names,
                code: fn_code,
                captures_outer_indices,
            });
            let fn_idx = code.functions.len() as u16;
            code.functions.push(compiled);
            code.emit(Opcode::LoadFunction(fn_idx));
            code.emit(Opcode::DeclareVar(var_idx));
            Ok(())
        }
        Stmt::ForIn { kind: _, target, iter, body } => {
            // Iterate Object keys / Array indexes pres Vec<String> snapshotted at iter eval.
            // Hidden var __keys_N + __keys_idx_N + iterable.
            let keys_name = format!("__forin_keys_{}", code.bytecode.len());
            let idx_name = format!("__forin_idx_{}", code.bytecode.len());
            let keys_var = code.push_var(&keys_name);
            let idx_var = code.push_var(&idx_name);
            // Vyhodnoceni iter -> push -> for each key emit pres specialni opcode?
            // Misto toho: pouzij Object.keys() pres LoadGlobal + Call.
            // Generuj: keys = Object.keys(iter); idx=0; while idx<keys.length: target=keys[idx]; body; idx++.
            compile_expr(iter, code)?; // push iter (object/array)
            // Object.keys(iter) - LoadGlobal Object, GetProp keys, CallNative 1 arg.
            // Reorganize: LoadGlobal Object first then arg. Currently iter is on stack.
            // Better: temp store iter, push Object.keys, push iter, call.
            let tmp_iter = code.push_var(&format!("__forin_tmpiter_{}", code.bytecode.len()));
            code.emit(Opcode::DeclareVar(tmp_iter));
            let object_idx = code.push_var("Object");
            code.emit(Opcode::LoadGlobal(object_idx));
            let keys_prop = code.push_var("keys");
            code.emit(Opcode::GetProp(keys_prop));
            code.emit(Opcode::LoadVar(tmp_iter));
            code.emit(Opcode::CallNative(1));
            code.emit(Opcode::DeclareVar(keys_var));
            // idx = 0
            code.emit(Opcode::LoadZero);
            code.emit(Opcode::DeclareVar(idx_var));
            let target_name = match target.as_ref() {
                Expr::Ident(n) => n.clone(),
                _ => return Err("for-in target must be ident"),
            };
            let target_var = code.push_var(&target_name);
            code.loop_stack.push(LoopFrame { break_jumps: vec![], continue_jumps: vec![] });
            let test_pos = code.bytecode.len();
            code.emit(Opcode::LoadVar(idx_var));
            code.emit(Opcode::LoadVar(keys_var));
            let length_idx = code.push_var("length");
            code.emit(Opcode::GetProp(length_idx));
            code.emit(Opcode::Lt);
            let jmp_to_end = code.emit(Opcode::JmpIfFalse(0));
            code.emit(Opcode::LoadVar(keys_var));
            code.emit(Opcode::LoadVar(idx_var));
            code.emit(Opcode::GetIndex);
            code.emit(Opcode::DeclareVar(target_var));
            compile_stmt(body, code)?;
            let cont_target = code.bytecode.len();
            let frame = code.loop_stack.last().cloned().unwrap();
            for ci in &frame.continue_jumps {
                code.patch_jmp(*ci, cont_target);
            }
            code.emit(Opcode::Inc(idx_var));
            let back = code.emit(Opcode::Jmp(0));
            code.patch_jmp(back, test_pos);
            let end = code.bytecode.len();
            code.patch_jmp(jmp_to_end, end);
            let frame = code.loop_stack.pop().unwrap();
            for bj in &frame.break_jumps {
                code.patch_jmp(*bj, end);
            }
            Ok(())
        }
        Stmt::ForOf { kind: _, target, iter, body } => {
            // Iterate Array/String pres index. Hidden vars __for_iter_N + __for_idx_N.
            let iter_name = format!("__for_iter_{}", code.bytecode.len());
            let idx_name = format!("__for_idx_{}", code.bytecode.len());
            let iter_var = code.push_var(&iter_name);
            let idx_var = code.push_var(&idx_name);
            // iter = expr
            compile_expr(iter, code)?;
            code.emit(Opcode::DeclareVar(iter_var));
            // idx = 0
            code.emit(Opcode::LoadZero);
            code.emit(Opcode::DeclareVar(idx_var));
            // Target var: jen Ident.
            let target_name = match target.as_ref() {
                Expr::Ident(n) => n.clone(),
                _ => return Err("for-of target must be ident"),
            };
            let target_var = code.push_var(&target_name);

            code.loop_stack.push(LoopFrame { break_jumps: vec![], continue_jumps: vec![] });
            let test_pos = code.bytecode.len();
            // test: idx < iter.length
            code.emit(Opcode::LoadVar(idx_var));
            code.emit(Opcode::LoadVar(iter_var));
            let length_idx = code.push_var("length");
            code.emit(Opcode::GetProp(length_idx));
            code.emit(Opcode::Lt);
            let jmp_to_end = code.emit(Opcode::JmpIfFalse(0));
            // target = iter[idx]
            code.emit(Opcode::LoadVar(iter_var));
            code.emit(Opcode::LoadVar(idx_var));
            code.emit(Opcode::GetIndex);
            code.emit(Opcode::DeclareVar(target_var));
            // body
            compile_stmt(body, code)?;
            // continue target = increment
            let cont_target = code.bytecode.len();
            let frame = code.loop_stack.last().cloned().unwrap();
            for ci in &frame.continue_jumps {
                code.patch_jmp(*ci, cont_target);
            }
            // idx++
            code.emit(Opcode::Inc(idx_var));
            // jump back na test
            let back = code.emit(Opcode::Jmp(0));
            code.patch_jmp(back, test_pos);
            let end = code.bytecode.len();
            code.patch_jmp(jmp_to_end, end);
            let frame = code.loop_stack.pop().unwrap();
            for bj in &frame.break_jumps {
                code.patch_jmp(*bj, end);
            }
            Ok(())
        }
        Stmt::Return(opt_expr) => {
            if let Some(e) = opt_expr {
                compile_expr(e, code)?;
            } else {
                code.emit(Opcode::LoadUndefined);
            }
            code.emit(Opcode::Return);
            Ok(())
        }
        _ => Err("unsupported stmt"),
    }
}

/// Try compile cely Program. Pri chybe vrati Err.
/// Posledni Stmt::Expr nedostane Pop - jeji hodnota zustane na stacku, Halt ji vrati.
pub fn compile_program(stmts: &[Stmt]) -> Result<CodeBlock, &'static str> {
    let mut code = CodeBlock::new();
    let last_idx = stmts.len().saturating_sub(1);
    for (i, s) in stmts.iter().enumerate() {
        if i == last_idx {
            // Last stmt: pri Stmt::Expr nemmtuj Pop, hodnota = vysledek programu.
            if let Stmt::Expr(e) = s {
                compile_expr(e, &mut code)?;
                continue;
            }
        }
        compile_stmt(s, &mut code)?;
    }
    code.emit(Opcode::Halt);
    Ok(code)
}

/// Stack-based VM.
pub struct VM {
    stack: Vec<JsValue>,
    /// Lokalni promenne (mapping var_name idx -> JsValue).
    /// Misto plnoho scope chain pouzivame plain Vec - var_idx je primy index.
    locals: Vec<JsValue>,
    /// Volitelny global env hook: pri LoadGlobal vyhleda jmeno v env.
    /// Bez hooku = vrati Undefined.
    pub env: Option<std::rc::Rc<std::cell::RefCell<super::Environment>>>,
    /// Closure captures - free var values copied at LoadFunction time.
    captures: Vec<JsValue>,
}

impl VM {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(64),
            locals: Vec::new(),
            env: None,
            captures: Vec::new(),
        }
    }
    pub fn with_env(env: std::rc::Rc<std::cell::RefCell<super::Environment>>) -> Self {
        Self {
            stack: Vec::with_capacity(64),
            locals: Vec::new(),
            env: Some(env),
            captures: Vec::new(),
        }
    }

    pub fn run(&mut self, code: &CodeBlock) -> Result<JsValue, String> {
        // Init locals na velikost var_names.
        if self.locals.len() < code.var_names.len() {
            self.locals.resize(code.var_names.len(), JsValue::Undefined);
        }
        let mut pc: i32 = 0;
        let bytecode = &code.bytecode;
        while (pc as usize) < bytecode.len() {
            let op = bytecode[pc as usize];
            pc += 1;
            match op {
                Opcode::LoadConst(i) => self.stack.push(code.constants[i as usize].clone()),
                Opcode::LoadVar(i) => {
                    let v = self.locals.get(i as usize).cloned().unwrap_or(JsValue::Undefined);
                    self.stack.push(v);
                }
                Opcode::StoreVar(i) => {
                    let v = self.pop()?;
                    if (i as usize) < self.locals.len() {
                        self.locals[i as usize] = v;
                    }
                }
                Opcode::DeclareVar(i) => {
                    let v = self.pop()?;
                    while self.locals.len() <= i as usize {
                        self.locals.push(JsValue::Undefined);
                    }
                    self.locals[i as usize] = v;
                }
                Opcode::Pop => { self.pop()?; }
                Opcode::Dup => {
                    let v = self.peek()?.clone();
                    self.stack.push(v);
                }
                Opcode::LoadUndefined => self.stack.push(JsValue::Undefined),
                Opcode::LoadNull => self.stack.push(JsValue::Null),
                Opcode::LoadTrue => self.stack.push(JsValue::Bool(true)),
                Opcode::LoadFalse => self.stack.push(JsValue::Bool(false)),
                Opcode::LoadZero => self.stack.push(JsValue::Number(0.0)),
                Opcode::LoadOne => self.stack.push(JsValue::Number(1.0)),
                Opcode::Add => { let b = self.pop()?; let a = self.pop()?; self.stack.push(op_add(a, b)); }
                Opcode::Sub => self.bin_num(|a, b| a - b)?,
                Opcode::Mul => self.bin_num(|a, b| a * b)?,
                Opcode::Div => self.bin_num(|a, b| a / b)?,
                Opcode::Mod => self.bin_num(|a, b| a.rem_euclid(b))?,
                Opcode::Exp => self.bin_num(|a, b| a.powf(b))?,
                Opcode::Neg => {
                    let a = self.pop()?;
                    self.stack.push(JsValue::Number(-to_number(&a)));
                }
                Opcode::Pos => {
                    let a = self.pop()?;
                    self.stack.push(JsValue::Number(to_number(&a)));
                }
                Opcode::Not => {
                    let a = self.pop()?;
                    self.stack.push(JsValue::Bool(!to_bool(&a)));
                }
                Opcode::BitNot => {
                    let a = self.pop()?;
                    let n = to_number(&a) as i32;
                    self.stack.push(JsValue::Number((!n) as f64));
                }
                Opcode::Eq => self.cmp(|a, b| loose_eq(&a, &b))?,
                Opcode::NotEq => self.cmp(|a, b| !loose_eq(&a, &b))?,
                Opcode::StrictEq => self.cmp(|a, b| values_strict_eq(&a, &b))?,
                Opcode::StrictNotEq => self.cmp(|a, b| !values_strict_eq(&a, &b))?,
                Opcode::Lt => self.bin_cmp_num(|a, b| a < b)?,
                Opcode::Gt => self.bin_cmp_num(|a, b| a > b)?,
                Opcode::LtEq => self.bin_cmp_num(|a, b| a <= b)?,
                Opcode::GtEq => self.bin_cmp_num(|a, b| a >= b)?,
                Opcode::BitAnd => self.bin_int(|a, b| a & b)?,
                Opcode::BitOr => self.bin_int(|a, b| a | b)?,
                Opcode::BitXor => self.bin_int(|a, b| a ^ b)?,
                Opcode::Shl => self.bin_int(|a, b| a.wrapping_shl(b as u32))?,
                Opcode::Shr => self.bin_int(|a, b| a.wrapping_shr(b as u32))?,
                Opcode::Ushr => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let ai = to_number(&a) as u32;
                    let bi = to_number(&b) as u32 & 31;
                    self.stack.push(JsValue::Number((ai >> bi) as f64));
                }
                Opcode::Jmp(o) => { pc += o; }
                Opcode::JmpIfFalse(o) => {
                    let v = self.pop()?;
                    if !to_bool(&v) { pc += o; }
                }
                Opcode::JmpIfTrue(o) => {
                    let v = self.pop()?;
                    if to_bool(&v) { pc += o; }
                }
                Opcode::JmpIfFalseKeep(o) => {
                    let v = self.peek()?.clone();
                    if !to_bool(&v) { pc += o; }
                }
                Opcode::JmpIfTrueKeep(o) => {
                    let v = self.peek()?.clone();
                    if to_bool(&v) { pc += o; }
                }
                Opcode::JmpIfNotNullishKeep(o) => {
                    let v = self.peek()?.clone();
                    if !matches!(v, JsValue::Null | JsValue::Undefined) { pc += o; }
                }
                Opcode::Inc(i) => {
                    let cur = self.locals.get(i as usize).cloned().unwrap_or(JsValue::Undefined);
                    let n = to_number(&cur) + 1.0;
                    if (i as usize) < self.locals.len() {
                        self.locals[i as usize] = JsValue::Number(n);
                    }
                }
                Opcode::Dec(i) => {
                    let cur = self.locals.get(i as usize).cloned().unwrap_or(JsValue::Undefined);
                    let n = to_number(&cur) - 1.0;
                    if (i as usize) < self.locals.len() {
                        self.locals[i as usize] = JsValue::Number(n);
                    }
                }
                Opcode::PostInc(i) => {
                    let cur = self.locals.get(i as usize).cloned().unwrap_or(JsValue::Undefined);
                    let orig = to_number(&cur);
                    if (i as usize) < self.locals.len() {
                        self.locals[i as usize] = JsValue::Number(orig + 1.0);
                    }
                    self.stack.push(JsValue::Number(orig));
                }
                Opcode::PostDec(i) => {
                    let cur = self.locals.get(i as usize).cloned().unwrap_or(JsValue::Undefined);
                    let orig = to_number(&cur);
                    if (i as usize) < self.locals.len() {
                        self.locals[i as usize] = JsValue::Number(orig - 1.0);
                    }
                    self.stack.push(JsValue::Number(orig));
                }
                Opcode::LoadGlobal(i) => {
                    let name = code.var_names.get(i as usize).cloned().unwrap_or_default();
                    let v = if let Some(env) = &self.env {
                        env.borrow().get(&name).unwrap_or(JsValue::Undefined)
                    } else {
                        JsValue::Undefined
                    };
                    self.stack.push(v);
                }
                Opcode::CallNative(argc) => {
                    let argc = argc as usize;
                    if self.stack.len() < argc + 1 { return Err("stack underflow CallNative".into()); }
                    let args: Vec<JsValue> = self.stack.drain(self.stack.len() - argc..).collect();
                    let callee = self.pop()?;
                    let result = match callee {
                        JsValue::Function(super::JsFunc::Native(_, f)) => {
                            f(args).map_err(|e| format!("{:?}", e))?
                        }
                        JsValue::Function(super::JsFunc::VmCompiled { compiled, env, name, captures }) => {
                            // Nested VM run pro user function.
                            let mut nested = VM::new();
                            nested.env = Some(env.clone());
                            nested.captures = captures.clone();
                            nested.locals.resize(compiled.code.var_names.len(), JsValue::Undefined);
                            // Self-reference pro rekurzi: locals[0] = function value.
                            if !compiled.code.var_names.is_empty()
                                && compiled.code.var_names[0] == name.clone().unwrap_or_default() {
                                nested.locals[0] = JsValue::Function(super::JsFunc::VmCompiled {
                                    name: name.clone(),
                                    compiled: compiled.clone(),
                                    env: env.clone(),
                                    captures: captures.clone(),
                                });
                            }
                            for (i, p) in compiled.params.iter().enumerate() {
                                if let Some(idx) = compiled.code.var_names.iter().position(|n| n == p) {
                                    nested.locals[idx] = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                                }
                            }
                            nested.run(&compiled.code)?
                        }
                        _ => return Err("callee not a native function".into()),
                    };
                    self.stack.push(result);
                }
                Opcode::GetProp(i) => {
                    let key = code.var_names.get(i as usize).cloned().unwrap_or_default();
                    let obj = self.pop()?;
                    let v = get_property(&obj, &key);
                    self.stack.push(v);
                }
                Opcode::SetProp(i) => {
                    let key = code.var_names.get(i as usize).cloned().unwrap_or_default();
                    let value = self.pop()?;
                    let obj = self.pop()?;
                    set_property(&obj, &key, value.clone());
                    self.stack.push(value);
                }
                Opcode::SetIndex => {
                    let value = self.pop()?;
                    let key = self.pop()?;
                    let obj = self.pop()?;
                    let key_str = match &key {
                        JsValue::Str(s) => s.clone(),
                        JsValue::Number(n) => {
                            if *n == n.trunc() && n.is_finite() { format!("{}", *n as i64) }
                            else { format!("{}", n) }
                        }
                        _ => key_to_str(&key),
                    };
                    if let (JsValue::Array(arr), JsValue::Number(n)) = (&obj, &key) {
                        let idx = *n as usize;
                        let mut a = arr.borrow_mut();
                        while a.len() <= idx { a.push(JsValue::Undefined); }
                        a[idx] = value.clone();
                    } else {
                        set_property(&obj, &key_str, value.clone());
                    }
                    self.stack.push(value);
                }
                Opcode::GetIndex => {
                    let key = self.pop()?;
                    let obj = self.pop()?;
                    let key_str = match &key {
                        JsValue::Str(s) => s.clone(),
                        JsValue::Number(n) => {
                            if *n == n.trunc() && n.is_finite() { format!("{}", *n as i64) }
                            else { format!("{}", n) }
                        }
                        _ => format!("{}", key_to_str(&key)),
                    };
                    // Pri Array + numeric key: indexed access.
                    if let (JsValue::Array(arr), JsValue::Number(n)) = (&obj, &key) {
                        let idx = *n as usize;
                        let v = arr.borrow().get(idx).cloned().unwrap_or(JsValue::Undefined);
                        self.stack.push(v);
                    } else {
                        self.stack.push(get_property(&obj, &key_str));
                    }
                }
                Opcode::CallNativeArgs => {
                    // Pop Array (args), pop callee.
                    let args_v = self.pop()?;
                    let args: Vec<JsValue> = match args_v {
                        JsValue::Array(a) => a.borrow().clone(),
                        _ => return Err("CallNativeArgs: args not Array".into()),
                    };
                    let callee = self.pop()?;
                    let result = match callee {
                        JsValue::Function(super::JsFunc::Native(_, f)) => {
                            f(args).map_err(|e| format!("{:?}", e))?
                        }
                        JsValue::Function(super::JsFunc::VmCompiled { compiled, env, name, captures }) => {
                            let mut nested = VM::new();
                            nested.env = Some(env.clone());
                            nested.captures = captures.clone();
                            nested.locals.resize(compiled.code.var_names.len(), JsValue::Undefined);
                            if !compiled.code.var_names.is_empty()
                                && compiled.code.var_names[0] == name.clone().unwrap_or_default() {
                                nested.locals[0] = JsValue::Function(super::JsFunc::VmCompiled {
                                    name: name.clone(),
                                    compiled: compiled.clone(),
                                    env: env.clone(),
                                    captures: captures.clone(),
                                });
                            }
                            for (i, p) in compiled.params.iter().enumerate() {
                                if let Some(idx) = compiled.code.var_names.iter().position(|n| n == p) {
                                    nested.locals[idx] = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                                }
                            }
                            nested.run(&compiled.code)?
                        }
                        _ => return Err("CallNativeArgs: callee not function".into()),
                    };
                    self.stack.push(result);
                }
                Opcode::AppendItem => {
                    let val = self.pop()?;
                    if let Some(JsValue::Array(arr)) = self.stack.last() {
                        arr.borrow_mut().push(val);
                    } else {
                        return Err("AppendItem: top of stack not Array".into());
                    }
                }
                Opcode::AppendSpread => {
                    let src = self.pop()?;
                    let items: Vec<JsValue> = match src {
                        JsValue::Array(a) => a.borrow().clone(),
                        _ => return Err("AppendSpread: source not Array".into()),
                    };
                    if let Some(JsValue::Array(arr)) = self.stack.last() {
                        let mut a = arr.borrow_mut();
                        for it in items { a.push(it); }
                    } else {
                        return Err("AppendSpread: top of stack not Array".into());
                    }
                }
                Opcode::NewArray(count) => {
                    let count = count as usize;
                    if self.stack.len() < count { return Err("stack underflow NewArray".into()); }
                    let items: Vec<JsValue> = self.stack.drain(self.stack.len() - count..).collect();
                    self.stack.push(JsValue::Array(std::rc::Rc::new(std::cell::RefCell::new(items))));
                }
                Opcode::NewObject(count) => {
                    let count = count as usize;
                    let need = count * 2;
                    if self.stack.len() < need { return Err("stack underflow NewObject".into()); }
                    let drained: Vec<JsValue> = self.stack.drain(self.stack.len() - need..).collect();
                    let obj = std::rc::Rc::new(std::cell::RefCell::new(super::JsObject::new()));
                    for chunk in drained.chunks(2) {
                        if chunk.len() == 2 {
                            let key_str = match &chunk[0] {
                                JsValue::Str(s) => s.clone(),
                                _ => format!("{}", key_to_str(&chunk[0])),
                            };
                            obj.borrow_mut().set(key_str, chunk[1].clone());
                        }
                    }
                    self.stack.push(JsValue::Object(obj));
                }
                Opcode::TypeOf => {
                    let v = self.pop()?;
                    let t = match v {
                        JsValue::Undefined => "undefined",
                        JsValue::Null => "object",
                        JsValue::Bool(_) => "boolean",
                        JsValue::Number(_) => "number",
                        JsValue::Str(_) => "string",
                        _ => "object",
                    };
                    self.stack.push(JsValue::Str(t.to_string()));
                }
                Opcode::LoadStrConst(i) => {
                    self.stack.push(code.constants[i as usize].clone());
                }
                Opcode::Return => {
                    return self.pop();
                }
                Opcode::Halt => {
                    // Vrat top stacku nebo Undefined.
                    return Ok(self.stack.pop().unwrap_or(JsValue::Undefined));
                }
                Opcode::LoadFunction(idx) => {
                    let compiled = code.functions.get(idx as usize)
                        .ok_or("LoadFunction idx out of range")?
                        .clone();
                    let env = self.env.clone().unwrap_or_else(|| super::Environment::new_global());
                    // Capture outer locals.
                    let mut captures: Vec<JsValue> = Vec::with_capacity(compiled.captures_outer_indices.len());
                    for &outer_idx in &compiled.captures_outer_indices {
                        let v = self.locals.get(outer_idx as usize).cloned().unwrap_or(JsValue::Undefined);
                        captures.push(v);
                    }
                    self.stack.push(JsValue::Function(super::JsFunc::VmCompiled {
                        name: compiled.name.clone(),
                        compiled,
                        env,
                        captures,
                    }));
                }
                Opcode::LoadCapture(i) => {
                    let v = self.captures.get(i as usize).cloned().unwrap_or(JsValue::Undefined);
                    self.stack.push(v);
                }
            }
        }
        Ok(self.stack.pop().unwrap_or(JsValue::Undefined))
    }

    fn pop(&mut self) -> Result<JsValue, String> {
        self.stack.pop().ok_or_else(|| "stack underflow".to_string())
    }
    fn peek(&self) -> Result<&JsValue, String> {
        self.stack.last().ok_or_else(|| "stack empty".to_string())
    }
    fn bin_num<F: Fn(f64, f64) -> f64>(&mut self, f: F) -> Result<(), String> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.stack.push(JsValue::Number(f(to_number(&a), to_number(&b))));
        Ok(())
    }
    fn bin_int<F: Fn(i32, i32) -> i32>(&mut self, f: F) -> Result<(), String> {
        let b = self.pop()?;
        let a = self.pop()?;
        let ai = to_number(&a) as i32;
        let bi = to_number(&b) as i32;
        self.stack.push(JsValue::Number(f(ai, bi) as f64));
        Ok(())
    }
    fn bin_cmp_num<F: Fn(f64, f64) -> bool>(&mut self, f: F) -> Result<(), String> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.stack.push(JsValue::Bool(f(to_number(&a), to_number(&b))));
        Ok(())
    }
    fn cmp<F: Fn(JsValue, JsValue) -> bool>(&mut self, f: F) -> Result<(), String> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.stack.push(JsValue::Bool(f(a, b)));
        Ok(())
    }
}

fn set_property(obj: &JsValue, key: &str, value: JsValue) {
    match obj {
        JsValue::Object(o) => { o.borrow_mut().set(key.to_string(), value); }
        JsValue::Array(a) => {
            if let Ok(idx) = key.parse::<usize>() {
                let mut arr = a.borrow_mut();
                while arr.len() <= idx { arr.push(JsValue::Undefined); }
                arr[idx] = value;
            }
            // length and other - skip for now
        }
        _ => {}
    }
}

fn get_property(obj: &JsValue, key: &str) -> JsValue {
    match obj {
        JsValue::Object(o) => o.borrow().get(key),
        JsValue::Array(a) => {
            // Array.length
            if key == "length" { return JsValue::Number(a.borrow().len() as f64); }
            // Built-in methods returning Native fn closures.
            match key {
                "push" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.push".into(),
                        std::rc::Rc::new(move |args| {
                            let mut v = arr_ref.borrow_mut();
                            for a in args { v.push(a); }
                            Ok(JsValue::Number(v.len() as f64))
                        }),
                    ));
                }
                "pop" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.pop".into(),
                        std::rc::Rc::new(move |_| {
                            Ok(arr_ref.borrow_mut().pop().unwrap_or(JsValue::Undefined))
                        }),
                    ));
                }
                "shift" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.shift".into(),
                        std::rc::Rc::new(move |_| {
                            let mut v = arr_ref.borrow_mut();
                            if v.is_empty() { Ok(JsValue::Undefined) } else { Ok(v.remove(0)) }
                        }),
                    ));
                }
                "unshift" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.unshift".into(),
                        std::rc::Rc::new(move |args| {
                            let mut v = arr_ref.borrow_mut();
                            for (i, a) in args.into_iter().enumerate() {
                                v.insert(i, a);
                            }
                            Ok(JsValue::Number(v.len() as f64))
                        }),
                    ));
                }
                "indexOf" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.indexOf".into(),
                        std::rc::Rc::new(move |args| {
                            let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                            let v = arr_ref.borrow();
                            for (i, item) in v.iter().enumerate() {
                                if values_strict_eq(item, &needle) {
                                    return Ok(JsValue::Number(i as f64));
                                }
                            }
                            Ok(JsValue::Number(-1.0))
                        }),
                    ));
                }
                "includes" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.includes".into(),
                        std::rc::Rc::new(move |args| {
                            let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                            let v = arr_ref.borrow();
                            Ok(JsValue::Bool(v.iter().any(|x| values_strict_eq(x, &needle))))
                        }),
                    ));
                }
                "join" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.join".into(),
                        std::rc::Rc::new(move |args| {
                            let sep = args.into_iter().next()
                                .map(|v| v.to_string()).unwrap_or_else(|| ",".to_string());
                            let v = arr_ref.borrow();
                            let parts: Vec<String> = v.iter().map(|x| x.to_string()).collect();
                            Ok(JsValue::Str(parts.join(&sep)))
                        }),
                    ));
                }
                "reverse" => {
                    let arr_ref = std::rc::Rc::clone(a);
                    return JsValue::Function(super::JsFunc::Native(
                        "Array.reverse".into(),
                        std::rc::Rc::new(move |_| {
                            arr_ref.borrow_mut().reverse();
                            Ok(JsValue::Array(std::rc::Rc::clone(&arr_ref)))
                        }),
                    ));
                }
                _ => {}
            }
            // Numericky index: parse
            if let Ok(idx) = key.parse::<usize>() {
                return a.borrow().get(idx).cloned().unwrap_or(JsValue::Undefined);
            }
            JsValue::Undefined
        }
        JsValue::Str(s) => {
            if key == "length" { return JsValue::Number(s.chars().count() as f64); }
            if let Ok(idx) = key.parse::<usize>() {
                return s.chars().nth(idx).map(|c| JsValue::Str(c.to_string()))
                    .unwrap_or(JsValue::Undefined);
            }
            // String methods.
            let s_clone = s.clone();
            match key {
                "toUpperCase" => return JsValue::Function(super::JsFunc::Native(
                    "String.toUpperCase".into(),
                    std::rc::Rc::new(move |_| Ok(JsValue::Str(s_clone.to_uppercase()))),
                )),
                "toLowerCase" => return JsValue::Function(super::JsFunc::Native(
                    "String.toLowerCase".into(),
                    std::rc::Rc::new(move |_| Ok(JsValue::Str(s_clone.to_lowercase()))),
                )),
                "trim" => return JsValue::Function(super::JsFunc::Native(
                    "String.trim".into(),
                    std::rc::Rc::new(move |_| Ok(JsValue::Str(s_clone.trim().to_string()))),
                )),
                "split" => return JsValue::Function(super::JsFunc::Native(
                    "String.split".into(),
                    std::rc::Rc::new(move |args| {
                        let sep = args.into_iter().next().map(|v| v.to_string());
                        let parts: Vec<JsValue> = match sep {
                            Some(s) if !s.is_empty() => s_clone.split(&s).map(|p| JsValue::Str(p.to_string())).collect(),
                            _ => s_clone.chars().map(|c| JsValue::Str(c.to_string())).collect(),
                        };
                        Ok(JsValue::Array(std::rc::Rc::new(std::cell::RefCell::new(parts))))
                    }),
                )),
                "indexOf" => return JsValue::Function(super::JsFunc::Native(
                    "String.indexOf".into(),
                    std::rc::Rc::new(move |args| {
                        let needle = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                        match s_clone.find(&needle) {
                            Some(idx) => Ok(JsValue::Number(idx as f64)),
                            None => Ok(JsValue::Number(-1.0)),
                        }
                    }),
                )),
                "includes" => return JsValue::Function(super::JsFunc::Native(
                    "String.includes".into(),
                    std::rc::Rc::new(move |args| {
                        let needle = args.into_iter().next().map(|v| v.to_string()).unwrap_or_default();
                        Ok(JsValue::Bool(s_clone.contains(&needle)))
                    }),
                )),
                _ => {}
            }
            JsValue::Undefined
        }
        _ => JsValue::Undefined,
    }
}

fn key_to_str(v: &JsValue) -> String {
    match v {
        JsValue::Str(s) => s.clone(),
        JsValue::Number(n) => {
            if *n == n.trunc() && n.is_finite() { format!("{}", *n as i64) }
            else { format!("{}", n) }
        }
        JsValue::Bool(true) => "true".to_string(),
        JsValue::Bool(false) => "false".to_string(),
        JsValue::Null => "null".to_string(),
        JsValue::Undefined => "undefined".to_string(),
        _ => String::new(),
    }
}

fn op_add(a: JsValue, b: JsValue) -> JsValue {
    // String concat pri jednom string operandu, jinak number.
    match (&a, &b) {
        (JsValue::Str(_), _) | (_, JsValue::Str(_)) => {
            JsValue::Str(format!("{}{}", to_string_loose(&a), to_string_loose(&b)))
        }
        _ => JsValue::Number(to_number(&a) + to_number(&b)),
    }
}

fn to_number(v: &JsValue) -> f64 {
    match v {
        JsValue::Number(n) => *n,
        JsValue::Bool(true) => 1.0,
        JsValue::Bool(false) => 0.0,
        JsValue::Null => 0.0,
        JsValue::Undefined => f64::NAN,
        JsValue::Str(s) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

fn to_bool(v: &JsValue) -> bool {
    match v {
        JsValue::Bool(b) => *b,
        JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JsValue::Str(s) => !s.is_empty(),
        JsValue::Null | JsValue::Undefined => false,
        _ => true,
    }
}

fn to_string_loose(v: &JsValue) -> String {
    match v {
        JsValue::Number(n) => {
            if n.is_nan() { "NaN".to_string() }
            else if *n == n.trunc() && n.is_finite() { format!("{}", *n as i64) }
            else { format!("{}", n) }
        }
        JsValue::Str(s) => s.clone(),
        JsValue::Bool(true) => "true".to_string(),
        JsValue::Bool(false) => "false".to_string(),
        JsValue::Null => "null".to_string(),
        JsValue::Undefined => "undefined".to_string(),
        _ => "[object]".to_string(),
    }
}

fn loose_eq(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Null, JsValue::Undefined) | (JsValue::Undefined, JsValue::Null) => true,
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::Str(x), JsValue::Str(y)) => x == y,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Null, JsValue::Null) | (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::Number(_), JsValue::Str(_)) | (JsValue::Str(_), JsValue::Number(_)) => {
            to_number(a) == to_number(b)
        }
        (JsValue::Bool(_), _) | (_, JsValue::Bool(_)) => {
            to_number(a) == to_number(b)
        }
        _ => false,
    }
}
