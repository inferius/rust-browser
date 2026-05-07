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

use crate::ast::{Expr, BinaryOp, UnaryOp, Stmt, LogicalOp, AssignOp, Pattern, ForInit, MemberProp, PropKey};
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
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub bytecode: Vec<Opcode>,
    pub constants: Vec<JsValue>,
    pub var_names: Vec<String>,
    /// Per-loop break/continue jumps stack (transient pri compile).
    /// Push pri vstupu do loopu, pop pri vystupu. Vrstva = (break_jumps, cont_jumps, cont_target_idx).
    pub loop_stack: Vec<LoopFrame>,
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
        Self { bytecode: Vec::new(), constants: Vec::new(), var_names: Vec::new(), loop_stack: Vec::new() }
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
            let idx = code.push_var(name);
            code.emit(Opcode::LoadVar(idx));
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
            // Jen Ident target v MVP.
            if let Expr::Ident(name) = target.as_ref() {
                let var_idx = code.push_var(name);
                match op {
                    AssignOp::Assign => {
                        compile_expr(value, code)?;
                        code.emit(Opcode::Dup); // hodnota assignmentu = nova hodnota
                        code.emit(Opcode::StoreVar(var_idx));
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
        Expr::Member { object, prop, optional: _ } => {
            compile_expr(object, code)?;
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
            Ok(())
        }
        Expr::Array(items) => {
            // None hole = undefined.
            if items.len() > u16::MAX as usize { return Err("array > 65k items"); }
            for item in items {
                if let Some(e) = item {
                    if matches!(e.as_ref(), Expr::Spread(_)) {
                        return Err("array spread not supported");
                    }
                    compile_expr(e, code)?;
                } else {
                    code.emit(Opcode::LoadUndefined);
                }
            }
            code.emit(Opcode::NewArray(items.len() as u16));
            Ok(())
        }
        Expr::Call { callee, args, optional: _ } => {
            // Callee: bud Ident (globalni native fn = console.log? ne to je member),
            // nebo Member (obj.prop()). Push callee, args, CallNative.
            // Pro `Math.sqrt(x)`: callee=Member(Math, sqrt) -> GetProp + LoadGlobal Math first.
            // Bez `this` binding pro ted. Member callee: get_property na obj.
            match callee.as_ref() {
                Expr::Ident(name) => {
                    let g_idx = code.push_var(name);
                    code.emit(Opcode::LoadGlobal(g_idx));
                }
                Expr::Member { object, prop, optional: _ } => {
                    // Obj resolve: ident vs nested.
                    if let Expr::Ident(obj_name) = object.as_ref() {
                        let g_idx = code.push_var(obj_name);
                        code.emit(Opcode::LoadGlobal(g_idx));
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
            for arg in args {
                compile_expr(arg, code)?;
            }
            code.emit(Opcode::CallNative(args.len() as u16));
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
}

impl VM {
    pub fn new() -> Self {
        Self { stack: Vec::with_capacity(64), locals: Vec::new(), env: None }
    }
    pub fn with_env(env: std::rc::Rc<std::cell::RefCell<super::Environment>>) -> Self {
        Self { stack: Vec::with_capacity(64), locals: Vec::new(), env: Some(env) }
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

fn get_property(obj: &JsValue, key: &str) -> JsValue {
    match obj {
        JsValue::Object(o) => o.borrow().get(key),
        JsValue::Array(a) => {
            // Array.length
            if key == "length" { return JsValue::Number(a.borrow().len() as f64); }
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
