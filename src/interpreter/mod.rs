use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::ast::*;

// ─── JS hodnoty ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Object(Rc<RefCell<JsObject>>),
    Array(Rc<RefCell<Vec<JsValue>>>),
    Function(JsFunc),
}

#[derive(Debug, Clone)]
pub struct JsObject {
    pub props: HashMap<String, JsValue>,
}

impl JsObject {
    fn new() -> Self { JsObject { props: HashMap::new() } }
    fn get(&self, k: &str) -> JsValue { self.props.get(k).cloned().unwrap_or(JsValue::Undefined) }
    fn set(&mut self, k: String, v: JsValue) { self.props.insert(k, v); }
}

/// Nativní funkce vrací Result<JsValue, String> (chybová zpráva jako String).
type NativeFn = Rc<dyn Fn(Vec<JsValue>) -> Result<JsValue, String>>;

#[derive(Clone)]
pub enum JsFunc {
    User { name: Option<String>, params: Vec<String>, body: FuncBody, env: Rc<RefCell<Env>> },
    Native(String, NativeFn),
}

impl std::fmt::Debug for JsFunc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunc::User { name, .. } => write!(f, "[Function: {}]", name.as_deref().unwrap_or("anonymous")),
            JsFunc::Native(name, _)   => write!(f, "[NativeFunction: {name}]"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FuncBody {
    Stmts(Vec<Stmt>),
    Expr(Box<Expr>),
}

impl std::fmt::Display for JsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsValue::Undefined    => write!(f, "undefined"),
            JsValue::Null         => write!(f, "null"),
            JsValue::Bool(b)      => write!(f, "{b}"),
            JsValue::Number(n)    => {
                if n.is_nan()           { write!(f, "NaN") }
                else if n.is_infinite() { write!(f, "{}Infinity", if *n > 0.0 { "" } else { "-" }) }
                else if *n == n.trunc() && n.abs() < 1e15 { write!(f, "{}", *n as i64) }
                else                    { write!(f, "{n}") }
            }
            JsValue::Str(s)       => write!(f, "{s}"),
            JsValue::Object(o)    => {
                let pairs: Vec<String> = o.borrow().props.iter().map(|(k,v)| format!("{k}: {v}")).collect();
                write!(f, "{{ {} }}", pairs.join(", "))
            }
            JsValue::Array(a)     => {
                let items: Vec<String> = a.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            JsValue::Function(fn_) => write!(f, "{fn_:?}"),
        }
    }
}

impl JsValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Undefined | JsValue::Null => false,
            JsValue::Bool(b)   => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::Str(s)    => !s.is_empty(),
            _                  => true,
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            JsValue::Number(n)    => *n,
            JsValue::Bool(true)   => 1.0,
            JsValue::Bool(false)  => 0.0,
            JsValue::Null         => 0.0,
            JsValue::Undefined    => f64::NAN,
            JsValue::Str(s)       => s.trim().parse().unwrap_or(f64::NAN),
            _                     => f64::NAN,
        }
    }

    pub fn type_of(&self) -> &'static str {
        match self {
            JsValue::Undefined   => "undefined",
            JsValue::Null        => "object",
            JsValue::Bool(_)     => "boolean",
            JsValue::Number(_)   => "number",
            JsValue::Str(_)      => "string",
            JsValue::Object(_)   => "object",
            JsValue::Array(_)    => "object",
            JsValue::Function(_) => "function",
        }
    }

    fn loose_eq(&self, other: &JsValue) -> bool {
        match (self, other) {
            (JsValue::Null | JsValue::Undefined, JsValue::Null | JsValue::Undefined) => true,
            (JsValue::Number(a), JsValue::Number(b)) => a == b,
            (JsValue::Str(a), JsValue::Str(b))       => a == b,
            (JsValue::Bool(a), JsValue::Bool(b))     => a == b,
            (JsValue::Number(n), JsValue::Str(s)) | (JsValue::Str(s), JsValue::Number(n)) =>
                s.trim().parse::<f64>().ok().as_ref() == Some(n),
            _ => false,
        }
    }

    fn strict_eq(&self, other: &JsValue) -> bool {
        match (self, other) {
            (JsValue::Undefined, JsValue::Undefined) => true,
            (JsValue::Null, JsValue::Null)           => true,
            (JsValue::Bool(a), JsValue::Bool(b))     => a == b,
            (JsValue::Number(a), JsValue::Number(b)) => a == b,
            (JsValue::Str(a), JsValue::Str(b))       => a == b,
            (JsValue::Object(a), JsValue::Object(b)) => Rc::ptr_eq(a, b),
            (JsValue::Array(a), JsValue::Array(b))   => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

// ─── Environment (scope) ──────────────────────────────────────────────────────

type Env = Environment;

#[derive(Debug, Clone)]
pub struct Environment {
    vars: HashMap<String, JsValue>,
    parent: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    pub fn new_global() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment { vars: HashMap::new(), parent: None }))
    }

    pub fn new_child(parent: &Rc<RefCell<Environment>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment { vars: HashMap::new(), parent: Some(Rc::clone(parent)) }))
    }

    pub fn define(&mut self, name: &str, val: JsValue) {
        self.vars.insert(name.to_string(), val);
    }

    pub fn get(&self, name: &str) -> Option<JsValue> {
        self.vars.get(name).cloned()
            .or_else(|| self.parent.as_ref()?.borrow().get(name))
    }

    pub fn set(&mut self, name: &str, val: JsValue) -> bool {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), val);
            true
        } else {
            self.parent.as_ref().map(|p| p.borrow_mut().set(name, val)).unwrap_or(false)
        }
    }
}

// ─── Chyby a signály ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum JsError {
    Runtime(String),
    Thrown(JsValue),
}

impl std::fmt::Display for JsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsError::Runtime(s) => write!(f, "RuntimeError: {s}"),
            JsError::Thrown(v)  => write!(f, "Uncaught: {v}"),
        }
    }
}

#[derive(Debug)]
enum Signal {
    Return(JsValue),
    Break(Option<String>),
    Continue(Option<String>),
}

type EvalResult = Result<JsValue, JsError>;
type StmtResult = Result<Option<Signal>, JsError>;

// ─── Interpreter ─────────────────────────────────────────────────────────────

pub struct Interpreter {
    pub global: Rc<RefCell<Environment>>,
}

impl Interpreter {
    pub fn new() -> Self {
        let global = Environment::new_global();
        setup_builtins(&global);
        Interpreter { global }
    }

    pub fn run(&mut self, program: &Program) -> EvalResult {
        let env = Rc::clone(&self.global);
        match self.exec_stmts(&program.body, &env)? {
            Some(Signal::Return(v)) => Ok(v),
            _ => Ok(JsValue::Undefined),
        }
    }

    // ─── Příkazy ──────────────────────────────────────────────────────────────

    fn exec_stmts(&mut self, stmts: &[Stmt], env: &Rc<RefCell<Environment>>) -> StmtResult {
        for s in stmts {
            if let Some(sig) = self.exec_stmt(s, env)? { return Ok(Some(sig)); }
        }
        Ok(None)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &Rc<RefCell<Environment>>) -> StmtResult {
        match stmt {
            Stmt::Empty => Ok(None),

            Stmt::Expr(e) => { self.eval(e, env)?; Ok(None) }

            Stmt::Block(body) => {
                let child = Environment::new_child(env);
                self.exec_stmts(body, &child)
            }

            Stmt::Var { kind, decls } => {
                for d in decls {
                    let val = match &d.init { Some(e) => self.eval(e, env)?, None => JsValue::Undefined };
                    if *kind == VarKind::Var {
                        self.global.borrow_mut().define(&d.name, val);
                    } else {
                        env.borrow_mut().define(&d.name, val);
                    }
                }
                Ok(None)
            }

            Stmt::Function { name, params, body } => {
                let func = JsValue::Function(JsFunc::User {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: FuncBody::Stmts(body.clone()),
                    env: Rc::clone(env),
                });
                env.borrow_mut().define(name, func);
                Ok(None)
            }

            Stmt::Return(val) => {
                let v = match val { Some(e) => self.eval(e, env)?, None => JsValue::Undefined };
                Ok(Some(Signal::Return(v)))
            }

            Stmt::Throw(e) => Err(JsError::Thrown(self.eval(e, env)?)),

            Stmt::Break(label)    => Ok(Some(Signal::Break(label.clone()))),
            Stmt::Continue(label) => Ok(Some(Signal::Continue(label.clone()))),

            Stmt::If { test, yes, no } => {
                if self.eval(test, env)?.is_truthy() {
                    self.exec_stmt(yes, env)
                } else if let Some(alt) = no {
                    self.exec_stmt(alt, env)
                } else { Ok(None) }
            }

            Stmt::While { test, body } => {
                loop {
                    if !self.eval(test, env)?.is_truthy() { break; }
                    match self.exec_stmt(body, env)? {
                        Some(Signal::Break(_))    => break,
                        Some(Signal::Continue(_)) => continue,
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::DoWhile { body, test } => {
                loop {
                    match self.exec_stmt(body, env)? {
                        Some(Signal::Break(_))    => break,
                        Some(Signal::Continue(_)) => {}
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                    if !self.eval(test, env)?.is_truthy() { break; }
                }
                Ok(None)
            }

            Stmt::For { init, test, update, body } => {
                let for_env = Environment::new_child(env);
                if let Some(init) = init {
                    match init {
                        ForInit::Var { kind, decls } => {
                            for d in decls {
                                let v = match &d.init { Some(e) => self.eval(e, &for_env)?, None => JsValue::Undefined };
                                for_env.borrow_mut().define(&d.name, v);
                            }
                        }
                        ForInit::Expr(e) => { self.eval(e, &for_env)?; }
                    }
                }
                loop {
                    if let Some(cond) = test {
                        if !self.eval(cond, &for_env)?.is_truthy() { break; }
                    }
                    match self.exec_stmt(body, &for_env)? {
                        Some(Signal::Break(_))    => break,
                        Some(Signal::Continue(_)) => {}
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                    if let Some(upd) = update { self.eval(upd, &for_env)?; }
                }
                Ok(None)
            }

            Stmt::ForOf { kind: _, target, iter, body } => {
                let arr_val = self.eval(iter, env)?;
                let items = match arr_val {
                    JsValue::Array(ref a) => a.borrow().clone(),
                    JsValue::Str(ref s) => s.chars().map(|c| JsValue::Str(c.to_string())).collect(),
                    _ => return Err(JsError::Runtime("for...of: nenaiterabilní hodnota".into())),
                };
                for item in items {
                    let loop_env = Environment::new_child(env);
                    if let Expr::Ident(name) = target.as_ref() {
                        loop_env.borrow_mut().define(name, item);
                    }
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(_))    => break,
                        Some(Signal::Continue(_)) => continue,
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::ForIn { kind: _, target, iter, body } => {
                let obj_val = self.eval(iter, env)?;
                let keys = match &obj_val {
                    JsValue::Object(o) => o.borrow().props.keys().cloned().collect::<Vec<_>>(),
                    _ => vec![],
                };
                for key in keys {
                    let loop_env = Environment::new_child(env);
                    if let Expr::Ident(name) = target.as_ref() {
                        loop_env.borrow_mut().define(name, JsValue::Str(key));
                    }
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(_))    => break,
                        Some(Signal::Continue(_)) => continue,
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::Try { body, catch, finally } => {
                let try_env = Environment::new_child(env);
                let result = self.exec_stmts(body, &try_env);
                let sig = match result {
                    Ok(s) => s,
                    Err(e) => {
                        if let Some(c) = catch {
                            let catch_env = Environment::new_child(env);
                            if let Some(param) = &c.param {
                                let err_val = match e {
                                    JsError::Thrown(v) => v,
                                    JsError::Runtime(s) => JsValue::Str(s),
                                };
                                catch_env.borrow_mut().define(param, err_val);
                            }
                            self.exec_stmts(&c.body, &catch_env)?
                        } else { return Err(e); }
                    }
                };
                if let Some(fin) = finally {
                    let fin_env = Environment::new_child(env);
                    self.exec_stmts(fin, &fin_env)?;
                }
                Ok(sig)
            }

            Stmt::Labeled { label: _, body } => self.exec_stmt(body, env),
        }
    }

    // ─── Výrazy ───────────────────────────────────────────────────────────────

    pub fn eval(&mut self, expr: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        match expr {
            Expr::Number(n)    => Ok(JsValue::Number(*n)),
            Expr::Str(s)       => Ok(JsValue::Str(s.clone())),
            Expr::Bool(b)      => Ok(JsValue::Bool(*b)),
            Expr::Null         => Ok(JsValue::Null),
            Expr::Undefined    => Ok(JsValue::Undefined),
            Expr::Regex(p, f)  => Ok(JsValue::Str(format!("/{p}/{f}"))),

            Expr::Ident(name)  => {
                env.borrow().get(name)
                    .ok_or_else(|| JsError::Runtime(format!("ReferenceError: '{name}' není definováno")))
            }

            Expr::Template { quasis, expressions } => {
                let mut s = quasis[0].clone();
                for (i, e) in expressions.iter().enumerate() {
                    s.push_str(&self.eval(e, env)?.to_string());
                    if let Some(q) = quasis.get(i + 1) { s.push_str(q); }
                }
                Ok(JsValue::Str(s))
            }

            Expr::Array(items) => {
                let mut arr = Vec::new();
                for item in items {
                    arr.push(match item { Some(e) => self.eval(e, env)?, None => JsValue::Undefined });
                }
                Ok(JsValue::Array(Rc::new(RefCell::new(arr))))
            }

            Expr::Object(props) => {
                let mut obj = JsObject::new();
                for p in props {
                    let key = match &p.key {
                        PropKey::Ident(s) | PropKey::Str(s) => s.clone(),
                        PropKey::Num(n) => n.to_string(),
                        PropKey::Computed(e) => self.eval(e, env)?.to_string(),
                    };
                    let val = self.eval(&p.value, env)?;
                    obj.set(key, val);
                }
                Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
            }

            Expr::Function { name, params, body } => Ok(JsValue::Function(JsFunc::User {
                name: name.clone(), params: params.clone(),
                body: FuncBody::Stmts(body.clone()), env: Rc::clone(env),
            })),

            Expr::Arrow { params, body } => Ok(JsValue::Function(JsFunc::User {
                name: None, params: params.clone(),
                body: match body {
                    ArrowBody::Block(b) => FuncBody::Stmts(b.clone()),
                    ArrowBody::Expr(e)  => FuncBody::Expr(e.clone()),
                },
                env: Rc::clone(env),
            })),

            Expr::Unary  { op, arg }          => self.eval_unary(op, arg, env),
            Expr::Binary { op, left, right }   => self.eval_binary(op, left, right, env),
            Expr::Logical { op, left, right }  => self.eval_logical(op, left, right, env),

            Expr::Ternary { test, yes, no } => {
                if self.eval(test, env)?.is_truthy() { self.eval(yes, env) } else { self.eval(no, env) }
            }

            Expr::Assign { op, target, value } => self.eval_assign(op, target, value, env),

            Expr::Call   { callee, args }     => self.eval_call(callee, args, env),

            Expr::New { callee, args } => {
                let func = self.eval(callee, env)?;
                let mut arg_vals = Vec::new();
                for a in args { arg_vals.push(self.eval(a, env)?); }
                self.call_new(func, arg_vals)
            }

            Expr::Member { object, prop }     => self.eval_member(object, prop, env),

            Expr::Spread(e)   => self.eval(e, env),

            Expr::Sequence(exprs) => {
                let mut last = JsValue::Undefined;
                for e in exprs { last = self.eval(e, env)?; }
                Ok(last)
            }
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, arg: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        match op {
            UnaryOp::Typeof => {
                let t = if let Expr::Ident(name) = arg {
                    env.borrow().get(name).unwrap_or(JsValue::Undefined).type_of()
                } else {
                    self.eval(arg, env)?.type_of()
                };
                Ok(JsValue::Str(t.to_string()))
            }
            UnaryOp::Void   => { self.eval(arg, env)?; Ok(JsValue::Undefined) }
            UnaryOp::Not    => Ok(JsValue::Bool(!self.eval(arg, env)?.is_truthy())),
            UnaryOp::Minus  => Ok(JsValue::Number(-self.eval(arg, env)?.to_number())),
            UnaryOp::Plus   => Ok(JsValue::Number(self.eval(arg, env)?.to_number())),
            UnaryOp::BitNot => Ok(JsValue::Number(!(self.eval(arg, env)?.to_number() as i32) as f64)),
            UnaryOp::Delete => {
                if let Expr::Member { object, prop } = arg {
                    let obj = self.eval(object, env)?;
                    let key = self.resolve_prop_key(prop, env)?;
                    if let JsValue::Object(o) = &obj { o.borrow_mut().props.remove(&key); }
                }
                Ok(JsValue::Bool(true))
            }
            UnaryOp::PreInc => {
                let v = self.eval(arg, env)?.to_number() + 1.0;
                self.assign_to(arg, JsValue::Number(v), env)?;
                Ok(JsValue::Number(v))
            }
            UnaryOp::PreDec => {
                let v = self.eval(arg, env)?.to_number() - 1.0;
                self.assign_to(arg, JsValue::Number(v), env)?;
                Ok(JsValue::Number(v))
            }
        }
    }

    fn eval_binary(&mut self, op: &BinaryOp, left: &Expr, right: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        if *op == BinaryOp::PostInc {
            let old = self.eval(left, env)?.to_number();
            self.assign_to(left, JsValue::Number(old + 1.0), env)?;
            return Ok(JsValue::Number(old));
        }
        if *op == BinaryOp::PostDec {
            let old = self.eval(left, env)?.to_number();
            self.assign_to(left, JsValue::Number(old - 1.0), env)?;
            return Ok(JsValue::Number(old));
        }

        let l = self.eval(left, env)?;
        let r = self.eval(right, env)?;

        Ok(match op {
            BinaryOp::Add => match (&l, &r) {
                (JsValue::Str(a), _) => JsValue::Str(format!("{a}{r}")),
                (_, JsValue::Str(b)) => JsValue::Str(format!("{l}{b}")),
                _ => JsValue::Number(l.to_number() + r.to_number()),
            },
            BinaryOp::Sub  => JsValue::Number(l.to_number() - r.to_number()),
            BinaryOp::Mul  => JsValue::Number(l.to_number() * r.to_number()),
            BinaryOp::Div  => JsValue::Number(l.to_number() / r.to_number()),
            BinaryOp::Mod  => JsValue::Number(l.to_number() % r.to_number()),
            BinaryOp::Exp  => JsValue::Number(l.to_number().powf(r.to_number())),
            BinaryOp::Eq        => JsValue::Bool(l.loose_eq(&r)),
            BinaryOp::NotEq     => JsValue::Bool(!l.loose_eq(&r)),
            BinaryOp::StrictEq  => JsValue::Bool(l.strict_eq(&r)),
            BinaryOp::StrictNotEq => JsValue::Bool(!l.strict_eq(&r)),
            BinaryOp::Lt   => JsValue::Bool(l.to_number() < r.to_number()),
            BinaryOp::Gt   => JsValue::Bool(l.to_number() > r.to_number()),
            BinaryOp::LtEq => JsValue::Bool(l.to_number() <= r.to_number()),
            BinaryOp::GtEq => JsValue::Bool(l.to_number() >= r.to_number()),
            BinaryOp::BitAnd => JsValue::Number((l.to_number() as i32 & r.to_number() as i32) as f64),
            BinaryOp::BitOr  => JsValue::Number((l.to_number() as i32 | r.to_number() as i32) as f64),
            BinaryOp::BitXor => JsValue::Number((l.to_number() as i32 ^ r.to_number() as i32) as f64),
            BinaryOp::Shl    => JsValue::Number(((l.to_number() as i32) << (r.to_number() as u32 & 31)) as f64),
            BinaryOp::Shr    => JsValue::Number(((l.to_number() as i32) >> (r.to_number() as u32 & 31)) as f64),
            BinaryOp::Ushr   => JsValue::Number(((l.to_number() as u32) >> (r.to_number() as u32 & 31)) as f64),
            BinaryOp::In => {
                let key = l.to_string();
                let found = matches!(&r, JsValue::Object(o) if o.borrow().props.contains_key(&key));
                JsValue::Bool(found)
            }
            BinaryOp::Instanceof => JsValue::Bool(false),
            BinaryOp::PostInc | BinaryOp::PostDec => unreachable!(),
        })
    }

    fn eval_logical(&mut self, op: &LogicalOp, left: &Expr, right: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let l = self.eval(left, env)?;
        match op {
            LogicalOp::And      => if !l.is_truthy() { Ok(l) } else { self.eval(right, env) },
            LogicalOp::Or       => if l.is_truthy()  { Ok(l) } else { self.eval(right, env) },
            LogicalOp::NullCoal => if matches!(l, JsValue::Null | JsValue::Undefined) { self.eval(right, env) } else { Ok(l) },
        }
    }

    fn eval_assign(&mut self, op: &AssignOp, target: &Expr, value: &Expr, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let new_val = if *op == AssignOp::Assign {
            self.eval(value, env)?
        } else {
            let old = self.eval(target, env)?;
            let rhs = self.eval(value, env)?;
            match op {
                AssignOp::Add    => match (&old, &rhs) {
                    (JsValue::Str(a), _) => JsValue::Str(format!("{a}{rhs}")),
                    _ => JsValue::Number(old.to_number() + rhs.to_number()),
                },
                AssignOp::Sub    => JsValue::Number(old.to_number() - rhs.to_number()),
                AssignOp::Mul    => JsValue::Number(old.to_number() * rhs.to_number()),
                AssignOp::Div    => JsValue::Number(old.to_number() / rhs.to_number()),
                AssignOp::Mod    => JsValue::Number(old.to_number() % rhs.to_number()),
                AssignOp::Exp    => JsValue::Number(old.to_number().powf(rhs.to_number())),
                AssignOp::BitAnd => JsValue::Number((old.to_number() as i32 & rhs.to_number() as i32) as f64),
                AssignOp::BitOr  => JsValue::Number((old.to_number() as i32 | rhs.to_number() as i32) as f64),
                AssignOp::BitXor => JsValue::Number((old.to_number() as i32 ^ rhs.to_number() as i32) as f64),
                AssignOp::Shl    => JsValue::Number(((old.to_number() as i32) << (rhs.to_number() as u32 & 31)) as f64),
                AssignOp::Shr    => JsValue::Number(((old.to_number() as i32) >> (rhs.to_number() as u32 & 31)) as f64),
                AssignOp::Ushr   => JsValue::Number(((old.to_number() as u32) >> (rhs.to_number() as u32 & 31)) as f64),
                AssignOp::Assign => unreachable!(),
            }
        };
        self.assign_to(target, new_val.clone(), env)?;
        Ok(new_val)
    }

    fn assign_to(&mut self, target: &Expr, val: JsValue, env: &Rc<RefCell<Environment>>) -> Result<(), JsError> {
        match target {
            Expr::Ident(name) => {
                if !env.borrow_mut().set(name, val.clone()) {
                    env.borrow_mut().define(name, val);
                }
                Ok(())
            }
            Expr::Member { object, prop } => {
                let obj = self.eval(object, env)?;
                let key = self.resolve_prop_key(prop, env)?;
                match &obj {
                    JsValue::Object(o) => { o.borrow_mut().set(key, val); Ok(()) }
                    JsValue::Array(a) => {
                        if let Ok(idx) = key.parse::<usize>() {
                            let mut arr = a.borrow_mut();
                            while arr.len() <= idx { arr.push(JsValue::Undefined); }
                            arr[idx] = val;
                        }
                        Ok(())
                    }
                    _ => Err(JsError::Runtime(format!("Nelze přiřadit do vlastnosti '{key}'")))
                }
            }
            _ => Err(JsError::Runtime("Neplatný cíl přiřazení".into())),
        }
    }

    fn eval_member(&mut self, object: &Expr, prop: &MemberProp, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let obj = self.eval(object, env)?;
        let key = self.resolve_prop_key(prop, env)?;
        self.get_prop(&obj, &key)
    }

    fn get_prop(&self, obj: &JsValue, key: &str) -> EvalResult {
        match obj {
            JsValue::Object(o) => Ok(o.borrow().get(key)),
            JsValue::Array(a)  => {
                if key == "length" { return Ok(JsValue::Number(a.borrow().len() as f64)); }
                if let Ok(i) = key.parse::<usize>() {
                    return Ok(a.borrow().get(i).cloned().unwrap_or(JsValue::Undefined));
                }
                Ok(JsValue::Undefined)
            }
            JsValue::Str(s) => {
                if key == "length" { return Ok(JsValue::Number(s.chars().count() as f64)); }
                if let Ok(i) = key.parse::<usize>() {
                    return Ok(s.chars().nth(i).map(|c| JsValue::Str(c.to_string())).unwrap_or(JsValue::Undefined));
                }
                Ok(JsValue::Undefined)
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    fn resolve_prop_key(&mut self, prop: &MemberProp, env: &Rc<RefCell<Environment>>) -> Result<String, JsError> {
        match prop {
            MemberProp::Ident(s) => Ok(s.clone()),
            MemberProp::Computed(e) => Ok(self.eval(e, env)?.to_string()),
        }
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], env: &Rc<RefCell<Environment>>) -> EvalResult {
        let (func_val, this_val) = match callee {
            Expr::Member { object, prop } => {
                let this = self.eval(object, env)?;
                let key = self.resolve_prop_key(prop, env)?;
                let func = self.get_prop(&this, &key)?;
                (func, Some(this))
            }
            _ => (self.eval(callee, env)?, None),
        };

        let mut arg_vals: Vec<JsValue> = Vec::new();
        for a in args {
            if let Expr::Spread(e) = a {
                if let JsValue::Array(arr) = self.eval(e, env)? {
                    arg_vals.extend(arr.borrow().clone());
                }
            } else {
                arg_vals.push(self.eval(a, env)?);
            }
        }

        self.call_function(func_val, arg_vals, this_val)
    }

    pub fn call_function(&mut self, func: JsValue, args: Vec<JsValue>, this: Option<JsValue>) -> EvalResult {
        match func {
            JsValue::Function(JsFunc::Native(_, f)) => {
                f(args).map_err(JsError::Runtime)
            }
            JsValue::Function(JsFunc::User { params, body, env, .. }) => {
                let call_env = Environment::new_child(&env);
                for (i, p) in params.iter().enumerate() {
                    call_env.borrow_mut().define(p, args.get(i).cloned().unwrap_or(JsValue::Undefined));
                }
                if let Some(t) = this { call_env.borrow_mut().define("this", t); }
                let args_arr = JsValue::Array(Rc::new(RefCell::new(args)));
                call_env.borrow_mut().define("arguments", args_arr);

                match &body {
                    FuncBody::Stmts(stmts) => {
                        let stmts = stmts.clone();
                        Ok(match self.exec_stmts(&stmts, &call_env)? {
                            Some(Signal::Return(v)) => v,
                            _ => JsValue::Undefined,
                        })
                    }
                    FuncBody::Expr(e) => {
                        let e = e.clone();
                        self.eval(&e, &call_env)
                    }
                }
            }
            _ => Err(JsError::Runtime(format!("{func} není funkce"))),
        }
    }

    fn call_new(&mut self, func: JsValue, args: Vec<JsValue>) -> EvalResult {
        let obj = JsValue::Object(Rc::new(RefCell::new(JsObject::new())));
        self.call_function(func, args, Some(obj.clone()))?;
        Ok(obj)
    }
}

// ─── Built-in funkce ─────────────────────────────────────────────────────────

fn native(name: &str, f: impl Fn(Vec<JsValue>) -> Result<JsValue, String> + 'static) -> JsValue {
    JsValue::Function(JsFunc::Native(name.to_string(), Rc::new(f)))
}

fn setup_builtins(env: &Rc<RefCell<Environment>>) {
    let mut e = env.borrow_mut();

    // console
    let mut console = JsObject::new();
    console.set("log".into(), native("log", |args| {
        println!("{}", args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        Ok(JsValue::Undefined)
    }));
    console.set("error".into(), native("error", |args| {
        eprintln!("[error] {}", args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        Ok(JsValue::Undefined)
    }));
    console.set("warn".into(), native("warn", |args| {
        eprintln!("[warn] {}", args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        Ok(JsValue::Undefined)
    }));
    e.define("console", JsValue::Object(Rc::new(RefCell::new(console))));

    // Math
    let mut math = JsObject::new();
    math.set("PI".into(), JsValue::Number(std::f64::consts::PI));
    math.set("E".into(),  JsValue::Number(std::f64::consts::E));
    math.set("sqrt".into(),  native("sqrt",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).sqrt()))));
    math.set("abs".into(),   native("abs",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).abs()))));
    math.set("floor".into(), native("floor", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).floor()))));
    math.set("ceil".into(),  native("ceil",  |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).ceil()))));
    math.set("round".into(), native("round", |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).round()))));
    math.set("sin".into(),   native("sin",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).sin()))));
    math.set("cos".into(),   native("cos",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).cos()))));
    math.set("log".into(),   native("log",   |a| Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(f64::NAN).ln()))));
    math.set("max".into(),   native("max",   |a| Ok(JsValue::Number(a.iter().fold(f64::NEG_INFINITY, |acc, v| acc.max(v.to_number()))))));
    math.set("min".into(),   native("min",   |a| Ok(JsValue::Number(a.iter().fold(f64::INFINITY,     |acc, v| acc.min(v.to_number()))))));
    math.set("pow".into(),   native("pow",   |a| {
        let base = a.get(0).map(|v| v.to_number()).unwrap_or(f64::NAN);
        let exp  = a.get(1).map(|v| v.to_number()).unwrap_or(f64::NAN);
        Ok(JsValue::Number(base.powf(exp)))
    }));
    math.set("random".into(), native("random", |_| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static S: AtomicU64 = AtomicU64::new(12345678901234567);
        let s = S.fetch_add(6364136223846793005, Ordering::Relaxed);
        Ok(JsValue::Number((s >> 11) as f64 / (1u64 << 53) as f64))
    }));
    e.define("Math", JsValue::Object(Rc::new(RefCell::new(math))));

    // Globální funkce
    e.define("parseInt", native("parseInt", |a| {
        let s = a.first().map(|v| v.to_string()).unwrap_or_default();
        let radix = a.get(1).map(|v| v.to_number() as u32).unwrap_or(10).max(2).min(36);
        Ok(JsValue::Number(i64::from_str_radix(s.trim(), radix).map(|n| n as f64).unwrap_or(f64::NAN)))
    }));
    e.define("parseFloat", native("parseFloat", |a| {
        Ok(JsValue::Number(a.first().map(|v| v.to_string()).unwrap_or_default().trim().parse().unwrap_or(f64::NAN)))
    }));
    e.define("isNaN", native("isNaN", |a| {
        Ok(JsValue::Bool(a.first().map(|v| v.to_number().is_nan()).unwrap_or(true)))
    }));
    e.define("isFinite", native("isFinite", |a| {
        Ok(JsValue::Bool(a.first().map(|v| v.to_number().is_finite()).unwrap_or(false)))
    }));
    e.define("String", native("String", |a| {
        Ok(JsValue::Str(a.first().map(|v| v.to_string()).unwrap_or_default()))
    }));
    e.define("Number", native("Number", |a| {
        Ok(JsValue::Number(a.first().map(|v| v.to_number()).unwrap_or(0.0)))
    }));
    e.define("Boolean", native("Boolean", |a| {
        Ok(JsValue::Bool(a.first().map(|v| v.is_truthy()).unwrap_or(false)))
    }));
    e.define("Array", native("Array", |a| {
        if let (1, Some(JsValue::Number(n))) = (a.len(), a.first()) {
            return Ok(JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Undefined; *n as usize]))));
        }
        Ok(JsValue::Array(Rc::new(RefCell::new(a))))
    }));

    e.define("Infinity",  JsValue::Number(f64::INFINITY));
    e.define("NaN",       JsValue::Number(f64::NAN));
    e.define("undefined", JsValue::Undefined);
}
