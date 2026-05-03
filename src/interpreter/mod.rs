/// Interpreter JavaScriptu pro podmnozinu ESNext.
///
/// # Architektura
///
/// Interpreter prochazi AST (Abstract Syntax Tree) a vyhodnocuje jednotlive uzly.
/// Stav programu je udrzovan v retezci `Environment` scopes.
///
/// ## Pipeline
/// ```
/// Zdrojovy text
///   -> Lexer (src/lexer/) -> Vec<Token>
///   -> Parser (src/parser/) -> Program (AST)
///   -> Interpreter (tento soubor) -> JsValue
/// ```
///
/// ## Implementovane vlastnosti ESNext
/// - Datove typy: number, string, bool, null, undefined, object, array, function
/// - Operatory: vsechny aritmeticke, porovnavaci, logicke, bitove, assignment vcetne `&&=`, `||=`, `??=`
/// - Rizeni toku: if/else, while, do-while, for, for-in, for-of, break, continue, return, throw, try-catch-finally
/// - Funkce: declaration, expression, arrow, closures, rekurze
/// - Parametry: simple, default (`x = 42`), rest (`...args`)
/// - Optional chaining: `obj?.prop`, `obj?.method()`
/// - Template literaly: `` `Hello ${name}!` ``
/// - Vestavene objekty: Math, console, parseInt, String, Number, Boolean, Array, Object
/// - Array metody: ~30 (push, map, filter, reduce, ...)
/// - String metody: ~20 (split, slice, includes, ...)
/// - Object staticke metody: keys, values, entries, assign, freeze, create, fromEntries

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::ast::{self, *};

// ─── JS hodnoty ───────────────────────────────────────────────────────────────

/// Runtime hodnota JavaScriptu.
///
/// Odpovida dynamickym typum JS - jedna hodnota muze byt behem
/// zivota programu libovolnym typem.
///
/// `Object` a `Array` jsou ulozeny za `Rc<RefCell<>>` pro sdilene
/// vlastnictvi (closure, vicenasobne reference na stejny objekt).
#[derive(Debug, Clone)]
pub enum JsValue {
    /// `undefined` - neinicializovana nebo chybejici hodnota
    Undefined,
    /// `null` - explicitni absence hodnoty
    Null,
    /// Boolean: `true` nebo `false`
    Bool(bool),
    /// Cislo: IEEE 754 double-precision float (jako v JS)
    Number(f64),
    /// Retezec
    Str(String),
    /// Objekt: mapa klic->hodnota sdilena pres Rc
    Object(Rc<RefCell<JsObject>>),
    /// Pole: sekvence hodnot sdilena pres Rc
    Array(Rc<RefCell<Vec<JsValue>>>),
    /// Funkce (uzivatelska nebo nativni)
    Function(JsFunc),
}

/// JS objekt - mapa retezec -> hodnota.
#[derive(Debug, Clone)]
pub struct JsObject {
    pub props: HashMap<String, JsValue>,
}

impl JsObject {
    fn new() -> Self { JsObject { props: HashMap::new() } }
    fn get(&self, k: &str) -> JsValue { self.props.get(k).cloned().unwrap_or(JsValue::Undefined) }
    fn set(&mut self, k: String, v: JsValue) { self.props.insert(k, v); }
}

/// Typ nativni (Rust) funkce: prijima Vec<JsValue>, vraci Result<JsValue, String>.
type NativeFn = Rc<dyn Fn(Vec<JsValue>) -> Result<JsValue, String>>;

/// Reprezentace funkce v runtime.
///
/// - `User` - funkce definovana v JS kodu, ulozena jako AST + uzavreny scope
/// - `Native` - funkce implementovana v Rustu (Math.sqrt, console.log, atd.)
#[derive(Clone)]
pub enum JsFunc {
    /// Uzivatelska JS funkce. Uchovava si uzavreny `env` (closure).
    User { name: Option<String>, params: Vec<Param>, body: FuncBody, env: Rc<RefCell<Env>> },
    /// Nativni Rust funkce. Prvni parametr je jmeno pro debugovani.
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

/// Lexikalni scope (prostredi promennych).
///
/// Implementuje retezec scopes: kazdy scope ma volitelny `parent`.
/// Vyhledavani promenne jde od nejhlubsiho scope ke globalnimu.
///
/// # Priklad retezce
/// ```
/// global: { console, Math, ... }
///   function scope: { x: 5 }
///     block scope: { y: 10 }  <- aktualni
/// ```
///
/// `Rc<RefCell<>>` umoznuje sdilet environment mezi closurami.
#[derive(Debug, Clone)]
pub struct Environment {
    /// Promenne deklarovane v tomto scopu
    vars: HashMap<String, JsValue>,
    /// Rodicovsky scope (None pouze pro globalni scope)
    parent: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    /// Vytvori novy globalni scope (bez rodice).
    pub fn new_global() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment { vars: HashMap::new(), parent: None }))
    }

    /// Vytvori novy child scope (blok, funkce, ...).
    pub fn new_child(parent: &Rc<RefCell<Environment>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment { vars: HashMap::new(), parent: Some(Rc::clone(parent)) }))
    }

    /// Deklaruje novou promennou v tomto scopu (let/const/var).
    pub fn define(&mut self, name: &str, val: JsValue) {
        self.vars.insert(name.to_string(), val);
    }

    /// Cte promennou - hleda od tohoto scopu az ke globalnimu.
    /// Vraci `None` kdyz promenna neexistuje (nikde v retezci).
    pub fn get(&self, name: &str) -> Option<JsValue> {
        self.vars.get(name).cloned()
            .or_else(|| self.parent.as_ref()?.borrow().get(name))
    }

    /// Prirazuje hodnotu existujici promenne (hleda ji v retezci scopu).
    ///
    /// Vraci `true` kdyz promennou nasla a zmenila,
    /// `false` kdyz promenna neexistuje (volajici pak muze rozhodnout co delat).
    pub fn set(&mut self, name: &str, val: JsValue) -> bool {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), val);
            true
        } else {
            self.parent.as_ref().map(|p| p.borrow_mut().set(name, val)).unwrap_or(false)
        }
    }
}

// ─── Chyby a signaly ─────────────────────────────────────────────────────────

/// Chyby ktere mohou nastat pri behu JS programu.
#[derive(Debug)]
pub enum JsError {
    /// Interna chyba interpretu (nezachytitelna v JS `catch`)
    Runtime(String),
    /// Hodnota vyhozena pomoci `throw` (zachytitelna v JS `catch`)
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

/// Interni signal pro rizeni toku programu (ne chyba).
///
/// Pouziva se pro `return`, `break`, `continue` - tyto prikazy
/// prerusuji normalni vykonani a musime je propagovat nahoru v AST.
#[derive(Debug)]
enum Signal {
    /// `return [value]` - navrat z funkce
    Return(JsValue),
    /// `break [label]` - preruseni cyklu
    Break(Option<String>),
    /// `continue [label]` - preskoceni iterace
    Continue(Option<String>),
}

/// Zkratka pro vysledek vyhodnoceni vyrazu.
type EvalResult = Result<JsValue, JsError>;
/// Zkratka pro vysledek vykonani prikazu (muze emit signal).
type StmtResult = Result<Option<Signal>, JsError>;

// ─── Interpreter ─────────────────────────────────────────────────────────────

/// Hlavni struktura interpretu.
///
/// Uchovava globalni scope se vsemi vestavennymi funkcemi a objekty.
/// Pro spusteni programu zavolej `Interpreter::new()` a pak `run(&program)`.
///
/// # Priklad
/// ```rust
/// let lexer = Lexer::parse_str("return 1 + 2;", "<script>").unwrap();
/// let tokens = /* filtrovat trivia */;
/// let program = Parser::new(tokens).parse().unwrap();
/// let mut interp = Interpreter::new();
/// let result = interp.run(&program).unwrap();
/// ```
pub struct Interpreter {
    /// Globalni scope - obsahuje vestavene funkce (Math, console, atd.)
    pub global: Rc<RefCell<Environment>>,
}

impl Interpreter {
    /// Vytvori novy interpreter s inicializovanymi vestavenymi objekty.
    pub fn new() -> Self {
        let global = Environment::new_global();
        setup_builtins(&global);
        Interpreter { global }
    }

    /// Spusti cely program (AST) a vrati posledni `return` hodnotu.
    ///
    /// Kdyz program neobsahuje `return`, vraci `JsValue::Undefined`.
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

            Expr::Call   { callee, args, optional }     => self.eval_call(callee, args, *optional, env),

            Expr::New { callee, args } => {
                let func = self.eval(callee, env)?;
                let mut arg_vals = Vec::new();
                for a in args { arg_vals.push(self.eval(a, env)?); }
                self.call_new(func, arg_vals)
            }

            Expr::Member { object, prop, optional }     => self.eval_member(object, prop, *optional, env),

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
                if let Expr::Member { object, prop, .. } = arg {
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
        // Logical assignment: short-circuit pred eval rhs
        match op {
            AssignOp::LogicalAnd => {
                let cur = self.eval(target, env)?;
                if !cur.is_truthy() { return Ok(cur); }
                let rhs = self.eval(value, env)?;
                self.assign_to(target, rhs.clone(), env)?;
                return Ok(rhs);
            }
            AssignOp::LogicalOr => {
                let cur = self.eval(target, env)?;
                if cur.is_truthy() { return Ok(cur); }
                let rhs = self.eval(value, env)?;
                self.assign_to(target, rhs.clone(), env)?;
                return Ok(rhs);
            }
            AssignOp::NullCoal => {
                let cur = self.eval(target, env)?;
                if !matches!(cur, JsValue::Null | JsValue::Undefined) { return Ok(cur); }
                let rhs = self.eval(value, env)?;
                self.assign_to(target, rhs.clone(), env)?;
                return Ok(rhs);
            }
            _ => {}
        }

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
                AssignOp::Assign | AssignOp::LogicalAnd | AssignOp::LogicalOr | AssignOp::NullCoal => unreachable!(),
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
            Expr::Member { object, prop, .. } => {
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
                    _ => Err(JsError::Runtime(format!("Nelze priradit do vlastnosti '{key}'")))
                }
            }
            _ => Err(JsError::Runtime("Neplatny cil prirazeni".into())),
        }
    }

    fn eval_member(&mut self, object: &Expr, prop: &MemberProp, optional: bool, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let obj = self.eval(object, env)?;
        if optional && matches!(obj, JsValue::Null | JsValue::Undefined) {
            return Ok(JsValue::Undefined);
        }
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

    fn eval_args(&mut self, args: &[Expr], env: &Rc<RefCell<Environment>>) -> Result<Vec<JsValue>, JsError> {
        let mut vals = Vec::new();
        for a in args {
            if let Expr::Spread(e) = a {
                if let JsValue::Array(arr) = self.eval(e, env)? {
                    vals.extend(arr.borrow().clone());
                }
            } else {
                vals.push(self.eval(a, env)?);
            }
        }
        Ok(vals)
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], optional: bool, env: &Rc<RefCell<Environment>>) -> EvalResult {
        if let Expr::Member { object, prop, optional: member_opt } = callee {
            let this = self.eval(object, env)?;
            // optional chaining: obj?.method() -> Undefined kdyz obj je null/undefined
            if (optional || *member_opt) && matches!(this, JsValue::Null | JsValue::Undefined) {
                return Ok(JsValue::Undefined);
            }
            let key = self.resolve_prop_key(prop, env)?;

            // Built-in Array/String metody -- dispatch pred call_function
            match &this {
                JsValue::Array(arr_rc) => {
                    let arr_rc = Rc::clone(arr_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    if let Some(result) = self.call_array_method(arr_rc, &key, arg_vals)? {
                        return Ok(result);
                    }
                }
                JsValue::Str(s) => {
                    let s = s.clone();
                    let arg_vals = self.eval_args(args, env)?;
                    if let Some(result) = call_string_method(&s, &key, arg_vals)? {
                        return Ok(result);
                    }
                }
                // Array staticke metody: Array.isArray(), Array.from()
                JsValue::Function(JsFunc::Native(fname, _)) => {
                    let fname = fname.clone();
                    let arg_vals = self.eval_args(args, env)?;
                    match (fname.as_str(), key.as_str()) {
                        ("Array", "isArray") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(), Some(JsValue::Array(_)))));
                        }
                        ("Array", "from") => {
                            let result = match arg_vals.into_iter().next().unwrap_or(JsValue::Undefined) {
                                JsValue::Array(a) => JsValue::Array(Rc::new(RefCell::new(a.borrow().clone()))),
                                JsValue::Str(s) => JsValue::Array(Rc::new(RefCell::new(
                                    s.chars().map(|c| JsValue::Str(c.to_string())).collect()
                                ))),
                                _ => JsValue::Array(Rc::new(RefCell::new(vec![]))),
                            };
                            return Ok(result);
                        }
                        _ => {}
                    }
                    // Neznama staticka metoda - zkusit get_prop + call_function
                    let func = self.get_prop(&this, &key)?;
                    return self.call_function(func, arg_vals, Some(this));
                }
                _ => {}
            }

            let func = self.get_prop(&this, &key)?;
            let arg_vals = self.eval_args(args, env)?;
            return self.call_function(func, arg_vals, Some(this));
        }

        // Bezny call: optional chaining foo?.()
        let func_val = self.eval(callee, env)?;
        if optional && matches!(func_val, JsValue::Null | JsValue::Undefined) {
            return Ok(JsValue::Undefined);
        }
        let arg_vals = self.eval_args(args, env)?;
        self.call_function(func_val, arg_vals, None)
    }

    pub fn call_function(&mut self, func: JsValue, args: Vec<JsValue>, this: Option<JsValue>) -> EvalResult {
        match func {
            JsValue::Function(JsFunc::Native(_, f)) => {
                f(args).map_err(JsError::Runtime)
            }
            JsValue::Function(JsFunc::User { params, body, env, .. }) => {
                let call_env = Environment::new_child(&env);
                let params = params.clone();
                let body = body.clone();
                let mut arg_idx = 0usize;
                for p in &params {
                    if p.rest {
                        let rest: Vec<JsValue> = args.get(arg_idx..).unwrap_or(&[]).to_vec();
                        call_env.borrow_mut().define(&p.name, JsValue::Array(Rc::new(RefCell::new(rest))));
                        break;
                    }
                    let val = args.get(arg_idx).cloned().unwrap_or(JsValue::Undefined);
                    let val = if matches!(val, JsValue::Undefined) {
                        if let Some(default_expr) = &p.default {
                            let de = *default_expr.clone();
                            self.eval(&de, &call_env)?
                        } else { val }
                    } else { val };
                    call_env.borrow_mut().define(&p.name, val);
                    arg_idx += 1;
                }
                if let Some(t) = this { call_env.borrow_mut().define("this", t); }
                let args_arr = JsValue::Array(Rc::new(RefCell::new(args)));
                call_env.borrow_mut().define("arguments", args_arr);
                let body = body;

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

    // ─── Array built-in metody ────────────────────────────────────────────────

    fn call_array_method(&mut self, arr: Rc<RefCell<Vec<JsValue>>>, method: &str, args: Vec<JsValue>) -> Result<Option<JsValue>, JsError> {
        match method {
            "push" => {
                let new_len = { let mut a = arr.borrow_mut(); for v in args { a.push(v); } a.len() as f64 };
                Ok(Some(JsValue::Number(new_len)))
            }
            "pop" => Ok(Some(arr.borrow_mut().pop().unwrap_or(JsValue::Undefined))),
            "shift" => {
                let v = if arr.borrow().is_empty() { JsValue::Undefined } else { arr.borrow_mut().remove(0) };
                Ok(Some(v))
            }
            "unshift" => {
                let mut a = arr.borrow_mut();
                for (i, v) in args.into_iter().enumerate() { a.insert(i, v); }
                Ok(Some(JsValue::Number(a.len() as f64)))
            }
            "reverse" => {
                arr.borrow_mut().reverse();
                Ok(Some(JsValue::Array(arr)))
            }
            "join" => {
                let sep = args.into_iter().next().map(|v| v.to_string()).unwrap_or_else(|| ",".into());
                let s = arr.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(&sep);
                Ok(Some(JsValue::Str(s)))
            }
            "includes" => {
                let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                Ok(Some(JsValue::Bool(arr.borrow().iter().any(|v| v.strict_eq(&needle)))))
            }
            "indexOf" => {
                let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let idx = arr.borrow().iter().position(|v| v.strict_eq(&needle));
                Ok(Some(JsValue::Number(idx.map(|i| i as f64).unwrap_or(-1.0))))
            }
            "lastIndexOf" => {
                let needle = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let a = arr.borrow();
                let idx = a.iter().rposition(|v| v.strict_eq(&needle));
                Ok(Some(JsValue::Number(idx.map(|i| i as f64).unwrap_or(-1.0))))
            }
            "slice" => {
                let a = arr.borrow();
                let len = a.len() as i64;
                let start = args.get(0).map(|v| v.to_number() as i64).unwrap_or(0);
                let end   = args.get(1).map(|v| v.to_number() as i64).unwrap_or(len);
                let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
                let e = if end   < 0 { (len + end  ).max(0) } else { end  .min(len) } as usize;
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(a.get(s..e).unwrap_or(&[]).to_vec())))))
            }
            "concat" => {
                let mut result = arr.borrow().clone();
                for v in args {
                    match v { JsValue::Array(o) => result.extend(o.borrow().clone()), other => result.push(other) }
                }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(result)))))
            }
            "flat" => {
                let depth = args.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(1);
                fn flatten(items: &[JsValue], d: usize) -> Vec<JsValue> {
                    if d == 0 { return items.to_vec(); }
                    let mut r = Vec::new();
                    for v in items { match v { JsValue::Array(a) => r.extend(flatten(&a.borrow(), d-1)), other => r.push(other.clone()) } }
                    r
                }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(flatten(&arr.borrow(), depth))))))
            }
            "sort" => {
                if args.is_empty() {
                    arr.borrow_mut().sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                    Ok(Some(JsValue::Array(arr)))
                } else {
                    // Callback sort -- potrebujeme self ale mame &mut self; pouzijeme simple String sort jako fallback
                    let cb = args.into_iter().next().unwrap();
                    let items: Vec<JsValue> = arr.borrow().clone();
                    let mut indexed: Vec<(usize, JsValue)> = items.into_iter().enumerate().collect();
                    let mut err: Option<JsError> = None;
                    // bubble sort kvuli borrow checker omezeniam (nelze sort_by s Result)
                    let n = indexed.len();
                    for i in 0..n {
                        for j in 0..n-1-i {
                            if err.is_some() { break; }
                            match self.call_function(cb.clone(), vec![indexed[j].1.clone(), indexed[j+1].1.clone()], None) {
                                Ok(v) if v.to_number() > 0.0 => indexed.swap(j, j+1),
                                Err(e) => { err = Some(e); }
                                _ => {}
                            }
                        }
                    }
                    if let Some(e) = err { return Err(e); }
                    *arr.borrow_mut() = indexed.into_iter().map(|(_, v)| v).collect();
                    Ok(Some(JsValue::Array(arr)))
                }
            }
            "forEach" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.into_iter().enumerate() {
                    self.call_function(cb.clone(), vec![v, JsValue::Number(i as f64)], None)?;
                }
                Ok(Some(JsValue::Undefined))
            }
            "map" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                let mut result = Vec::new();
                for (i, v) in items.into_iter().enumerate() {
                    result.push(self.call_function(cb.clone(), vec![v, JsValue::Number(i as f64)], None)?);
                }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(result)))))
            }
            "filter" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                let mut result = Vec::new();
                for (i, v) in items.into_iter().enumerate() {
                    if self.call_function(cb.clone(), vec![v.clone(), JsValue::Number(i as f64)], None)?.is_truthy() {
                        result.push(v);
                    }
                }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(result)))))
            }
            "reduce" => {
                let mut args_iter = args.into_iter();
                let cb = args_iter.next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                let (mut acc, start) = if let Some(init) = args_iter.next() {
                    (init, 0usize)
                } else {
                    if items.is_empty() { return Err(JsError::Runtime("reduce na prazdnem poli bez initialValue".into())); }
                    (items[0].clone(), 1usize)
                };
                for (i, v) in items[start..].iter().enumerate() {
                    acc = self.call_function(cb.clone(), vec![acc, v.clone(), JsValue::Number((start + i) as f64)], None)?;
                }
                Ok(Some(acc))
            }
            "reduceRight" => {
                let mut args_iter = args.into_iter();
                let cb = args_iter.next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                let (mut acc, end) = if let Some(init) = args_iter.next() {
                    (init, items.len())
                } else {
                    if items.is_empty() { return Err(JsError::Runtime("reduceRight na prazdnem poli bez initialValue".into())); }
                    let last = items.len() - 1;
                    (items[last].clone(), last)
                };
                for i in (0..end).rev() {
                    acc = self.call_function(cb.clone(), vec![acc, items[i].clone(), JsValue::Number(i as f64)], None)?;
                }
                Ok(Some(acc))
            }
            "find" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.into_iter().enumerate() {
                    if self.call_function(cb.clone(), vec![v.clone(), JsValue::Number(i as f64)], None)?.is_truthy() {
                        return Ok(Some(v));
                    }
                }
                Ok(Some(JsValue::Undefined))
            }
            "findIndex" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.into_iter().enumerate() {
                    if self.call_function(cb.clone(), vec![v, JsValue::Number(i as f64)], None)?.is_truthy() {
                        return Ok(Some(JsValue::Number(i as f64)));
                    }
                }
                Ok(Some(JsValue::Number(-1.0)))
            }
            "every" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.into_iter().enumerate() {
                    if !self.call_function(cb.clone(), vec![v, JsValue::Number(i as f64)], None)?.is_truthy() {
                        return Ok(Some(JsValue::Bool(false)));
                    }
                }
                Ok(Some(JsValue::Bool(true)))
            }
            "some" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.into_iter().enumerate() {
                    if self.call_function(cb.clone(), vec![v, JsValue::Number(i as f64)], None)?.is_truthy() {
                        return Ok(Some(JsValue::Bool(true)));
                    }
                }
                Ok(Some(JsValue::Bool(false)))
            }
            "flatMap" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                let mut result = Vec::new();
                for (i, v) in items.into_iter().enumerate() {
                    match self.call_function(cb.clone(), vec![v, JsValue::Number(i as f64)], None)? {
                        JsValue::Array(a) => result.extend(a.borrow().clone()),
                        other => result.push(other),
                    }
                }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(result)))))
            }
            "fill" => {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let len = arr.borrow().len() as i64;
                let start = args.get(1).map(|v| v.to_number() as i64).unwrap_or(0);
                let end   = args.get(2).map(|v| v.to_number() as i64).unwrap_or(len);
                let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
                let e = if end   < 0 { (len + end  ).max(0) } else { end  .min(len) } as usize;
                for i in s..e { arr.borrow_mut()[i] = val.clone(); }
                Ok(Some(JsValue::Array(arr)))
            }
            "splice" => {
                let start = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                let mut a = arr.borrow_mut();
                let len = a.len() as i64;
                let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
                let delete_count = args.get(1).map(|v| v.to_number() as usize).unwrap_or(a.len() - s);
                let end = (s + delete_count).min(a.len());
                let removed: Vec<JsValue> = a.drain(s..end).collect();
                let inserts = if args.len() > 2 { args[2..].to_vec() } else { vec![] };
                for (i, v) in inserts.into_iter().enumerate() { a.insert(s + i, v); }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(removed)))))
            }
            "at" => {
                let a = arr.borrow();
                let len = a.len() as i64;
                let idx = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                let real = if idx < 0 { len + idx } else { idx };
                Ok(Some(a.get(real as usize).cloned().unwrap_or(JsValue::Undefined)))
            }
            "toString" => {
                let s = arr.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
                Ok(Some(JsValue::Str(s)))
            }
            _ => Ok(None), // neni znama array metoda -> zkus get_prop
        }
    }
}

// ─── String built-in metody (bez &mut self) ───────────────────────────────────

fn call_string_method(s: &str, method: &str, args: Vec<JsValue>) -> Result<Option<JsValue>, JsError> {
    let chars: Vec<char> = s.chars().collect();
    match method {
        "split" => {
            let sep = args.first().map(|v| v.to_string());
            let parts: Vec<JsValue> = match sep.as_deref() {
                None | Some("undefined") => vec![JsValue::Str(s.to_string())],
                Some("") => chars.iter().map(|c| JsValue::Str(c.to_string())).collect(),
                Some(d) => s.split(d).map(|p| JsValue::Str(p.to_string())).collect(),
            };
            Ok(Some(JsValue::Array(Rc::new(RefCell::new(parts)))))
        }
        "slice" => {
            let len = chars.len() as i64;
            let start = args.get(0).map(|v| v.to_number() as i64).unwrap_or(0);
            let end   = args.get(1).map(|v| v.to_number() as i64).unwrap_or(len);
            let s2 = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
            let e2 = if end   < 0 { (len + end  ).max(0) } else { end  .min(len) } as usize;
            Ok(Some(JsValue::Str(chars[s2..e2.max(s2)].iter().collect())))
        }
        "substring" => {
            let len = chars.len();
            let a = args.get(0).map(|v| (v.to_number() as usize).min(len)).unwrap_or(0);
            let b = args.get(1).map(|v| (v.to_number() as usize).min(len)).unwrap_or(len);
            let (s2, e2) = if a <= b { (a, b) } else { (b, a) };
            Ok(Some(JsValue::Str(chars[s2..e2].iter().collect())))
        }
        "indexOf" => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Number(s.find(&*needle).map(|i| s[..i].chars().count() as f64).unwrap_or(-1.0))))
        }
        "lastIndexOf" => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Number(s.rfind(&*needle).map(|i| s[..i].chars().count() as f64).unwrap_or(-1.0))))
        }
        "includes"    => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Bool(s.contains(&*needle))))
        }
        "startsWith"  => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Bool(s.starts_with(&*needle))))
        }
        "endsWith"    => {
            let needle = args.first().map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Bool(s.ends_with(&*needle))))
        }
        "toLowerCase"  => Ok(Some(JsValue::Str(s.to_lowercase()))),
        "toUpperCase"  => Ok(Some(JsValue::Str(s.to_uppercase()))),
        "trim"         => Ok(Some(JsValue::Str(s.trim().to_string()))),
        "trimStart"    => Ok(Some(JsValue::Str(s.trim_start().to_string()))),
        "trimEnd"      => Ok(Some(JsValue::Str(s.trim_end().to_string()))),
        "charAt"       => {
            let i = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(JsValue::Str(chars.get(i).map(|c| c.to_string()).unwrap_or_default())))
        }
        "charCodeAt"   => {
            let i = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(JsValue::Number(chars.get(i).map(|c| *c as u32 as f64).unwrap_or(f64::NAN))))
        }
        "at"           => {
            let len = chars.len() as i64;
            let idx = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
            let real = if idx < 0 { len + idx } else { idx };
            Ok(Some(chars.get(real as usize).map(|c| JsValue::Str(c.to_string())).unwrap_or(JsValue::Undefined)))
        }
        "padStart"     => {
            let target = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            let pad = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| " ".into());
            if chars.len() >= target { return Ok(Some(JsValue::Str(s.to_string()))); }
            let needed = target - chars.len();
            let pad_chars: Vec<char> = pad.chars().collect();
            let padding: String = (0..needed).map(|i| pad_chars[i % pad_chars.len()]).collect();
            Ok(Some(JsValue::Str(format!("{padding}{s}"))))
        }
        "padEnd"       => {
            let target = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            let pad = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| " ".into());
            if chars.len() >= target { return Ok(Some(JsValue::Str(s.to_string()))); }
            let needed = target - chars.len();
            let pad_chars: Vec<char> = pad.chars().collect();
            let padding: String = (0..needed).map(|i| pad_chars[i % pad_chars.len()]).collect();
            Ok(Some(JsValue::Str(format!("{s}{padding}"))))
        }
        "repeat"       => {
            let n = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
            Ok(Some(JsValue::Str(s.repeat(n))))
        }
        "replace"      => {
            let from = args.first().map(|v| v.to_string()).unwrap_or_default();
            let to   = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Str(s.replacen(&*from, &to, 1))))
        }
        "replaceAll"   => {
            let from = args.first().map(|v| v.to_string()).unwrap_or_default();
            let to   = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            Ok(Some(JsValue::Str(s.replace(&*from, &to))))
        }
        "toString" | "valueOf" => Ok(Some(JsValue::Str(s.to_string()))),
        _ => Ok(None), // neni znama string metoda
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

    // Object staticke metody
    let mut obj_ctor = JsObject::new();
    obj_ctor.set("keys".into(), native("Object.keys", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let mut keys: Vec<JsValue> = o.borrow().props.keys().map(|k| JsValue::Str(k.clone())).collect();
                keys.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                Ok(JsValue::Array(Rc::new(RefCell::new(keys))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));
    obj_ctor.set("values".into(), native("Object.values", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let mut pairs: Vec<(String, JsValue)> = o.borrow().props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                let vals: Vec<JsValue> = pairs.into_iter().map(|(_, v)| v).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(vals))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));
    obj_ctor.set("entries".into(), native("Object.entries", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let mut pairs: Vec<(String, JsValue)> = o.borrow().props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                let entries: Vec<JsValue> = pairs.into_iter().map(|(k, v)| {
                    JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Str(k), v])))
                }).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(entries))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));
    obj_ctor.set("assign".into(), native("Object.assign", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(target_rc) = &target {
            for src in iter {
                if let JsValue::Object(src_rc) = src {
                    for (k, v) in src_rc.borrow().props.clone() {
                        target_rc.borrow_mut().set(k, v);
                    }
                }
            }
        }
        Ok(target)
    }));
    obj_ctor.set("freeze".into(), native("Object.freeze", |a| {
        // Implementace bez skutecneho freeze (immutability neresime)
        Ok(a.into_iter().next().unwrap_or(JsValue::Undefined))
    }));
    obj_ctor.set("create".into(), native("Object.create", |a| {
        // Object.create(proto) - ignorujeme proto, vratime prazdny objekt
        let _ = a;
        Ok(JsValue::Object(Rc::new(RefCell::new(JsObject::new()))))
    }));
    obj_ctor.set("fromEntries".into(), native("Object.fromEntries", |a| {
        let mut obj = JsObject::new();
        if let Some(JsValue::Array(entries)) = a.into_iter().next() {
            for entry in entries.borrow().iter() {
                if let JsValue::Array(pair) = entry {
                    let pair = pair.borrow();
                    let key = pair.get(0).map(|v| v.to_string()).unwrap_or_default();
                    let val = pair.get(1).cloned().unwrap_or(JsValue::Undefined);
                    obj.set(key, val);
                }
            }
        }
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));
    e.define("Object", JsValue::Object(Rc::new(RefCell::new(obj_ctor))));

    e.define("Infinity",  JsValue::Number(f64::INFINITY));
    e.define("NaN",       JsValue::Number(f64::NAN));
    e.define("undefined", JsValue::Undefined);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::base::Lexer;
    use crate::parser::Parser;
    use crate::tokens::TokenKind;

    // Spusti JS kod a vrati posledni return hodnotu (nebo Undefined).
    fn run(src: &str) -> JsValue {
        let lexer = Lexer::parse_str(src, "<test>").unwrap();
        let tokens: Vec<_> = lexer.tokens.into_iter()
            .filter(|t| !matches!(t.kind,
                TokenKind::Whitespace | TokenKind::Newline
                | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
            .collect();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        let mut interp = Interpreter::new();
        interp.run(&program).unwrap()
    }

    // Spusti JS vyraz a vrati vysledek.
    fn eval(expr: &str) -> JsValue {
        run(&format!("return {expr};"))
    }

    fn as_num(v: JsValue) -> f64 {
        match v { JsValue::Number(n) => n, other => panic!("Ocekavano Number, nalezeno {other:?}") }
    }

    fn as_str(v: JsValue) -> String {
        match v { JsValue::Str(s) => s, other => panic!("Ocekavan Str, nalezeno {other:?}") }
    }

    fn as_bool(v: JsValue) -> bool {
        match v { JsValue::Bool(b) => b, other => panic!("Ocekavan Bool, nalezeno {other:?}") }
    }

    // --- aritmetika ---

    #[test]
    fn arithmetic_basic() {
        assert_eq!(as_num(eval("1 + 2")), 3.0);
        assert_eq!(as_num(eval("10 - 3")), 7.0);
        assert_eq!(as_num(eval("3 * 4")), 12.0);
        assert_eq!(as_num(eval("10 / 4")), 2.5);
        assert_eq!(as_num(eval("10 % 3")), 1.0);
        assert_eq!(as_num(eval("2 ** 10")), 1024.0);
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(as_num(eval("2 + 3 * 4")), 14.0);
        assert_eq!(as_num(eval("(2 + 3) * 4")), 20.0);
    }

    #[test]
    fn unary_minus() {
        assert_eq!(as_num(eval("-5")), -5.0);
        assert_eq!(as_num(eval("-(3 + 2)")), -5.0);
    }

    // --- stringy ---

    #[test]
    fn string_concat() {
        assert_eq!(as_str(eval(r#""hello" + " " + "world""#)), "hello world");
    }

    #[test]
    fn string_coercion() {
        assert_eq!(as_str(eval(r#""val: " + 42"#)), "val: 42");
    }

    // --- porovnani ---

    #[test]
    fn comparisons() {
        assert!(as_bool(eval("1 < 2")));
        assert!(!as_bool(eval("2 < 1")));
        assert!(as_bool(eval("2 <= 2")));
        assert!(as_bool(eval("3 > 2")));
        assert!(as_bool(eval("1 === 1")));
        assert!(!as_bool(eval("1 === 2")));
        assert!(as_bool(eval("1 !== 2")));
    }

    #[test]
    fn loose_equality() {
        assert!(as_bool(eval("1 == 1")));
        assert!(as_bool(eval(r#"1 == "1""#)));
        assert!(!as_bool(eval("1 === \"1\"")));
    }

    // --- logicke operatory ---

    #[test]
    fn logical_and_or() {
        assert!(as_bool(eval("true && true")));
        assert!(!as_bool(eval("true && false")));
        assert!(as_bool(eval("false || true")));
        assert!(!as_bool(eval("false || false")));
    }

    #[test]
    fn nullish_coalescing() {
        assert_eq!(as_num(eval("null ?? 42")), 42.0);
        assert_eq!(as_num(eval("undefined ?? 7")), 7.0);
        assert_eq!(as_num(eval("5 ?? 42")), 5.0);
    }

    // --- promenne a scope ---

    #[test]
    fn let_declaration() {
        assert_eq!(as_num(run("let x = 10; return x;")), 10.0);
    }

    #[test]
    fn const_declaration() {
        assert_eq!(as_num(run("const PI = 3.14; return PI;")), 3.14);
    }

    #[test]
    fn var_hoisting() {
        assert_eq!(as_num(run("var x = 5; return x;")), 5.0);
    }

    #[test]
    fn block_scope() {
        assert_eq!(as_num(run(r#"
            let x = 1;
            { let x = 2; }
            return x;
        "#)), 1.0);
    }

    // --- ridici tok ---

    #[test]
    fn if_true_branch() {
        assert_eq!(as_num(run("if (true) { return 1; } return 2;")), 1.0);
    }

    #[test]
    fn if_false_branch() {
        assert_eq!(as_num(run("if (false) { return 1; } return 2;")), 2.0);
    }

    #[test]
    fn if_else_stmt() {
        assert_eq!(as_num(run("let x = 5; if (x > 3) { return 1; } else { return 0; }")), 1.0);
    }

    #[test]
    fn ternary_operator() {
        assert_eq!(as_num(eval("true ? 1 : 2")), 1.0);
        assert_eq!(as_num(eval("false ? 1 : 2")), 2.0);
    }

    #[test]
    fn while_loop() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            let i = 0;
            while (i < 5) { sum += i; i++; }
            return sum;
        "#)), 10.0);
    }

    #[test]
    fn for_loop() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            for (let i = 0; i < 5; i++) { sum += i; }
            return sum;
        "#)), 10.0);
    }

    #[test]
    fn for_break() {
        assert_eq!(as_num(run(r#"
            let x = 0;
            for (let i = 0; i < 10; i++) {
                if (i === 3) break;
                x = i;
            }
            return x;
        "#)), 2.0);
    }

    #[test]
    fn for_continue() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            for (let i = 0; i < 5; i++) {
                if (i === 2) continue;
                sum += i;
            }
            return sum;
        "#)), 8.0);  // 0+1+3+4
    }

    // --- funkce ---

    #[test]
    fn function_declaration_and_call() {
        assert_eq!(as_num(run(r#"
            function add(a, b) { return a + b; }
            return add(3, 4);
        "#)), 7.0);
    }

    #[test]
    fn function_recursion() {
        assert_eq!(as_num(run(r#"
            function fact(n) {
                if (n <= 1) return 1;
                return n * fact(n - 1);
            }
            return fact(5);
        "#)), 120.0);
    }

    #[test]
    fn arrow_function() {
        assert_eq!(as_num(run(r#"
            const square = x => x * x;
            return square(5);
        "#)), 25.0);
    }

    #[test]
    fn arrow_paren_params() {
        assert_eq!(as_num(run(r#"
            const add = (a, b) => a + b;
            return add(3, 4);
        "#)), 7.0);
    }

    #[test]
    fn closure() {
        assert_eq!(as_num(run(r#"
            function makeAdder(x) {
                return (y) => x + y;
            }
            const add5 = makeAdder(5);
            return add5(3);
        "#)), 8.0);
    }

    // --- pole ---

    #[test]
    fn array_literal_access() {
        assert_eq!(as_num(run(r#"
            let arr = [10, 20, 30];
            return arr[1];
        "#)), 20.0);
    }

    #[test]
    fn array_mutation() {
        assert_eq!(as_num(run(r#"
            let arr = [1, 2, 3];
            arr[0] = 99;
            return arr[0];
        "#)), 99.0);
    }

    #[test]
    fn array_length() {
        assert_eq!(as_num(run(r#"
            let arr = [1, 2, 3];
            return arr.length;
        "#)), 3.0);
    }

    // --- objekty ---

    #[test]
    fn object_property_access() {
        assert_eq!(as_num(run(r#"
            const obj = { x: 42 };
            return obj.x;
        "#)), 42.0);
    }

    #[test]
    fn object_computed_access() {
        assert_eq!(as_num(run(r#"
            const obj = { x: 99 };
            const key = "x";
            return obj[key];
        "#)), 99.0);
    }

    #[test]
    fn object_mutation() {
        assert_eq!(as_num(run(r#"
            let obj = { a: 1 };
            obj.a = 42;
            return obj.a;
        "#)), 42.0);
    }

    // --- template literaly ---

    #[test]
    fn template_no_substitution() {
        assert_eq!(as_str(run(r#"return `hello world`;"#)), "hello world");
    }

    #[test]
    fn template_with_expr() {
        assert_eq!(as_str(run(r#"
            let name = "World";
            return `Hello ${name}!`;
        "#)), "Hello World!");
    }

    #[test]
    fn template_arithmetic() {
        assert_eq!(as_str(run(r#"return `result: ${1 + 2}`;"#)), "result: 3");
    }

    // --- try-catch ---

    #[test]
    fn try_catch_basic() {
        assert_eq!(as_str(run(r#"
            try {
                throw "oops";
            } catch (e) {
                return e;
            }
        "#)), "oops");
    }

    #[test]
    fn try_catch_no_throw() {
        assert_eq!(as_num(run(r#"
            let x = 0;
            try { x = 5; } catch (e) { x = 99; }
            return x;
        "#)), 5.0);
    }

    // --- typeof ---

    #[test]
    fn typeof_values() {
        assert_eq!(as_str(eval("typeof 42")), "number");
        assert_eq!(as_str(eval(r#"typeof "hello""#)), "string");
        assert_eq!(as_str(eval("typeof true")), "boolean");
        assert_eq!(as_str(eval("typeof undefined")), "undefined");
        assert_eq!(as_str(eval("typeof null")), "object");
    }

    // --- ESNext: default params ---

    #[test]
    fn default_params_basic() {
        assert_eq!(as_num(run(r#"
            function greet(x, y = 10) { return x + y; }
            return greet(5);
        "#)), 15.0);
    }

    #[test]
    fn default_params_override() {
        assert_eq!(as_num(run(r#"
            function greet(x, y = 10) { return x + y; }
            return greet(5, 3);
        "#)), 8.0);
    }

    #[test]
    fn default_params_undefined_triggers_default() {
        assert_eq!(as_num(run(r#"
            function f(a = 42) { return a; }
            return f(undefined);
        "#)), 42.0);
    }

    // --- ESNext: rest params ---

    #[test]
    fn rest_params_collect() {
        assert_eq!(as_num(run(r#"
            function sum(...nums) {
                let total = 0;
                for (let n of nums) total += n;
                return total;
            }
            return sum(1, 2, 3, 4);
        "#)), 10.0);
    }

    #[test]
    fn rest_params_after_fixed() {
        assert_eq!(as_num(run(r#"
            function f(first, ...rest) { return rest.length; }
            return f(1, 2, 3, 4);
        "#)), 3.0);
    }

    // --- ESNext: spread operator ---

    #[test]
    fn spread_in_call() {
        assert_eq!(as_num(run(r#"
            function add(a, b, c) { return a + b + c; }
            const args = [1, 2, 3];
            return add(...args);
        "#)), 6.0);
    }

    // --- ESNext: optional chaining ---

    #[test]
    fn optional_chaining_null_prop() {
        assert!(matches!(run(r#"
            const obj = null;
            return obj?.foo;
        "#), JsValue::Undefined));
    }

    #[test]
    fn optional_chaining_null_call() {
        assert!(matches!(run(r#"
            const obj = null;
            return obj?.foo();
        "#), JsValue::Undefined));
    }

    #[test]
    fn optional_chaining_valid_prop() {
        assert_eq!(as_num(run(r#"
            const obj = { x: 42 };
            return obj?.x;
        "#)), 42.0);
    }

    #[test]
    fn optional_chaining_nested() {
        assert!(matches!(run(r#"
            const obj = { a: null };
            return obj?.a?.b;
        "#), JsValue::Undefined));
    }

    // --- ESNext: logical assignment ---

    #[test]
    fn logical_and_assign() {
        assert_eq!(as_num(run(r#"
            let x = 5;
            x &&= 10;
            return x;
        "#)), 10.0);
    }

    #[test]
    fn logical_and_assign_falsy() {
        assert_eq!(as_num(run(r#"
            let x = 0;
            x &&= 10;
            return x;
        "#)), 0.0);
    }

    #[test]
    fn logical_or_assign() {
        assert_eq!(as_num(run(r#"
            let x = 0;
            x ||= 42;
            return x;
        "#)), 42.0);
    }

    #[test]
    fn logical_or_assign_truthy() {
        assert_eq!(as_num(run(r#"
            let x = 5;
            x ||= 42;
            return x;
        "#)), 5.0);
    }

    #[test]
    fn nullish_assign() {
        assert_eq!(as_num(run(r#"
            let x = null;
            x ??= 99;
            return x;
        "#)), 99.0);
    }

    #[test]
    fn nullish_assign_non_null() {
        assert_eq!(as_num(run(r#"
            let x = 5;
            x ??= 99;
            return x;
        "#)), 5.0);
    }

    // --- ESNext: Array metody ---

    #[test]
    fn array_push_pop() {
        assert_eq!(as_num(run(r#"
            const a = [1, 2, 3];
            a.push(4);
            return a.pop();
        "#)), 4.0);
    }

    #[test]
    fn array_map() {
        assert_eq!(as_num(run(r#"
            const a = [1, 2, 3];
            const b = a.map(x => x * 2);
            return b[2];
        "#)), 6.0);
    }

    #[test]
    fn array_filter() {
        assert_eq!(as_num(run(r#"
            const a = [1, 2, 3, 4, 5];
            const b = a.filter(x => x % 2 === 0);
            return b.length;
        "#)), 2.0);
    }

    #[test]
    fn array_reduce() {
        assert_eq!(as_num(run(r#"
            const a = [1, 2, 3, 4];
            return a.reduce((acc, x) => acc + x, 0);
        "#)), 10.0);
    }

    #[test]
    fn array_find() {
        assert_eq!(as_num(run(r#"
            const a = [1, 2, 3, 4];
            return a.find(x => x > 2);
        "#)), 3.0);
    }

    #[test]
    fn array_includes() {
        assert!(as_bool(run(r#"
            return [1, 2, 3].includes(2);
        "#)));
        assert!(!as_bool(run(r#"
            return [1, 2, 3].includes(5);
        "#)));
    }

    #[test]
    fn array_join() {
        assert_eq!(as_str(run(r#"
            return [1, 2, 3].join("-");
        "#)), "1-2-3");
    }

    #[test]
    fn array_slice() {
        assert_eq!(as_num(run(r#"
            const a = [1, 2, 3, 4, 5];
            return a.slice(1, 3).length;
        "#)), 2.0);
    }

    #[test]
    fn array_every_some() {
        assert!(as_bool(run(r#"return [2, 4, 6].every(x => x % 2 === 0);"#)));
        assert!(!as_bool(run(r#"return [1, 2, 3].every(x => x % 2 === 0);"#)));
        assert!(as_bool(run(r#"return [1, 2, 3].some(x => x % 2 === 0);"#)));
        assert!(!as_bool(run(r#"return [1, 3, 5].some(x => x % 2 === 0);"#)));
    }

    #[test]
    fn array_flat() {
        assert_eq!(as_num(run(r#"
            return [[1, 2], [3, 4]].flat().length;
        "#)), 4.0);
    }

    #[test]
    fn array_isarray() {
        assert!(as_bool(run(r#"return Array.isArray([1, 2, 3]);"#)));
        assert!(!as_bool(run(r#"return Array.isArray("hello");"#)));
    }

    #[test]
    fn array_foreach() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            [1, 2, 3].forEach(x => { sum += x; });
            return sum;
        "#)), 6.0);
    }

    // --- ESNext: String metody ---

    #[test]
    fn string_includes() {
        assert!(as_bool(run(r#"return "hello world".includes("world");"#)));
        assert!(!as_bool(run(r#"return "hello world".includes("xyz");"#)));
    }

    #[test]
    fn string_starts_ends_with() {
        assert!(as_bool(run(r#"return "hello".startsWith("he");"#)));
        assert!(as_bool(run(r#"return "hello".endsWith("lo");"#)));
        assert!(!as_bool(run(r#"return "hello".startsWith("lo");"#)));
    }

    #[test]
    fn string_slice() {
        assert_eq!(as_str(run(r#"return "hello world".slice(6);"#)), "world");
        assert_eq!(as_str(run(r#"return "hello world".slice(0, 5);"#)), "hello");
    }

    #[test]
    fn string_split() {
        assert_eq!(as_num(run(r#"return "a,b,c".split(",").length;"#)), 3.0);
        assert_eq!(as_str(run(r#"return "a,b,c".split(",")[1];"#)), "b");
    }

    #[test]
    fn string_trim() {
        assert_eq!(as_str(run(r#"return "  hello  ".trim();"#)), "hello");
        assert_eq!(as_str(run(r#"return "  hello  ".trimStart();"#)), "hello  ");
        assert_eq!(as_str(run(r#"return "  hello  ".trimEnd();"#)), "  hello");
    }

    #[test]
    fn string_to_upper_lower() {
        assert_eq!(as_str(run(r#"return "Hello".toUpperCase();"#)), "HELLO");
        assert_eq!(as_str(run(r#"return "Hello".toLowerCase();"#)), "hello");
    }

    #[test]
    fn string_pad() {
        assert_eq!(as_str(run(r#"return "5".padStart(3, "0");"#)), "005");
        assert_eq!(as_str(run(r#"return "5".padEnd(3, "0");"#)), "500");
    }

    #[test]
    fn string_repeat() {
        assert_eq!(as_str(run(r#"return "ab".repeat(3);"#)), "ababab");
    }

    #[test]
    fn string_replace() {
        assert_eq!(as_str(run(r#"return "hello world".replace("world", "JS");"#)), "hello JS");
    }

    #[test]
    fn string_index_of() {
        assert_eq!(as_num(run(r#"return "hello".indexOf("l");"#)), 2.0);
        assert_eq!(as_num(run(r#"return "hello".indexOf("x");"#)), -1.0);
    }

    // --- ESNext: Object staticke metody ---

    #[test]
    fn object_keys() {
        assert_eq!(as_num(run(r#"
            return Object.keys({ a: 1, b: 2, c: 3 }).length;
        "#)), 3.0);
    }

    #[test]
    fn object_values() {
        assert_eq!(as_num(run(r#"
            const vals = Object.values({ a: 1, b: 2 });
            return vals[0] + vals[1];
        "#)), 3.0);
    }

    #[test]
    fn object_entries() {
        assert_eq!(as_num(run(r#"
            return Object.entries({ x: 10, y: 20 }).length;
        "#)), 2.0);
    }

    #[test]
    fn object_assign() {
        assert_eq!(as_num(run(r#"
            const target = { a: 1 };
            Object.assign(target, { b: 2, c: 3 });
            return target.b + target.c;
        "#)), 5.0);
    }

    #[test]
    fn object_from_entries() {
        assert_eq!(as_num(run(r#"
            const obj = Object.fromEntries([["a", 1], ["b", 2]]);
            return obj.a + obj.b;
        "#)), 3.0);
    }

    // --- ESNext: for...of pole ---

    #[test]
    fn for_of_array() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            for (const x of [1, 2, 3, 4]) { sum += x; }
            return sum;
        "#)), 10.0);
    }

    // --- ESNext: for...in objekt ---

    #[test]
    fn for_in_object() {
        assert_eq!(as_num(run(r#"
            const obj = { a: 1, b: 2, c: 3 };
            let count = 0;
            for (const k in obj) { count++; }
            return count;
        "#)), 3.0);
    }

    // --- ESNext: method shorthand v objektu ---

    #[test]
    fn object_method_shorthand() {
        assert_eq!(as_num(run(r#"
            const obj = {
                x: 10,
                getX() { return this.x; }
            };
            return obj.getX();
        "#)), 10.0);
    }
}
