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
    /// ES2015 Map - klicovana kolekce (klic muze byt libovolny JsValue)
    Map(Rc<RefCell<JsMap>>),
    /// ES2015 Set - kolekce unikatnich hodnot
    Set(Rc<RefCell<JsSet>>),
}

// ─── Map / Set datove struktury ──────────────────────────────────────────────

/// JS `Map` - kolekce klicovanych hodnot (klic muze byt libovolny JsValue).
///
/// Pouziva Vec<(key, value)> pro spravnou sematiku vsech klicu (vcetne NaN,
/// objektu - porovnavane pres SameValueZero / referencni rovnost).
#[derive(Debug, Clone)]
pub struct JsMap {
    /// Polozky v poradi vlozeni
    pub entries: Vec<(JsValue, JsValue)>,
}

impl JsMap {
    fn new() -> Self { JsMap { entries: Vec::new() } }

    /// Porovnani klicu: SameValueZero (NaN === NaN, objekty pres ptr_eq)
    fn key_eq(a: &JsValue, b: &JsValue) -> bool {
        match (a, b) {
            (JsValue::Number(x), JsValue::Number(y)) => {
                if x.is_nan() && y.is_nan() { return true; }
                x.to_bits() == y.to_bits()
            }
            (JsValue::Object(x), JsValue::Object(y))   => Rc::ptr_eq(x, y),
            (JsValue::Array(x),  JsValue::Array(y))    => Rc::ptr_eq(x, y),
            (JsValue::Map(x),    JsValue::Map(y))       => Rc::ptr_eq(x, y),
            (JsValue::Set(x),    JsValue::Set(y))       => Rc::ptr_eq(x, y),
            _ => a.strict_eq(b),
        }
    }

    fn set(&mut self, key: JsValue, val: JsValue) {
        for entry in &mut self.entries {
            if Self::key_eq(&entry.0, &key) { entry.1 = val; return; }
        }
        self.entries.push((key, val));
    }

    fn get(&self, key: &JsValue) -> JsValue {
        self.entries.iter()
            .find(|(k, _)| Self::key_eq(k, key))
            .map(|(_, v)| v.clone())
            .unwrap_or(JsValue::Undefined)
    }

    fn has(&self, key: &JsValue) -> bool {
        self.entries.iter().any(|(k, _)| Self::key_eq(k, key))
    }

    fn delete(&mut self, key: &JsValue) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(k, _)| !Self::key_eq(k, key));
        self.entries.len() < before
    }
}

/// JS `Set` - kolekce unikatnich hodnot.
#[derive(Debug, Clone)]
pub struct JsSet {
    pub values: Vec<JsValue>,
}

impl JsSet {
    fn new() -> Self { JsSet { values: Vec::new() } }

    fn has(&self, val: &JsValue) -> bool {
        self.values.iter().any(|v| JsMap::key_eq(v, val))
    }

    fn add(&mut self, val: JsValue) {
        if !self.has(&val) { self.values.push(val); }
    }

    fn delete(&mut self, val: &JsValue) -> bool {
        let before = self.values.len();
        self.values.retain(|v| !JsMap::key_eq(v, val));
        self.values.len() < before
    }
}

/// JS objekt - mapa retezec -> hodnota + prototypovy retezec.
#[derive(Debug, Clone)]
pub struct JsObject {
    pub props: HashMap<String, JsValue>,
    /// Prototypovy objekt (`obj.__proto__`). None = zadny prototype (Object.create(null)).
    pub proto: Option<Rc<RefCell<JsObject>>>,
    /// Object.freeze - po zavolani nelze menit/pridat vlastnosti.
    pub frozen: bool,
}

impl JsObject {
    fn new() -> Self {
        JsObject { props: HashMap::new(), proto: None, frozen: false }
    }

    /// Vytvori objekt s danym prototypem (Object.create(proto)).
    fn new_with_proto(proto: Rc<RefCell<JsObject>>) -> Self {
        JsObject { props: HashMap::new(), proto: Some(proto), frozen: false }
    }

    /// Cte vlastnost - prochazi prototypovym retezcem (max 100 uroven).
    fn get(&self, k: &str) -> JsValue {
        self.get_depth(k, 0)
    }

    fn get_depth(&self, k: &str, depth: usize) -> JsValue {
        if depth > 100 { return JsValue::Undefined; }
        if let Some(v) = self.props.get(k) {
            return v.clone();
        }
        if let Some(proto) = &self.proto {
            return proto.borrow().get_depth(k, depth + 1);
        }
        JsValue::Undefined
    }

    /// Kontroluje vlastni vlastnost (bez prochazeni prototypoveho retezce).
    fn has_own(&self, k: &str) -> bool {
        self.props.contains_key(k)
    }

    /// Nastavi vlastnost. Frozen objekt zmeny ignoruje.
    fn set(&mut self, k: String, v: JsValue) {
        if self.frozen { return; }
        self.props.insert(k, v);
    }

    /// Vrati serazeny seznam vlastnich klicu (bez internich `__key__` klicu).
    fn own_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.props.keys()
            .filter(|k| !is_internal_key(k))
            .cloned()
            .collect();
        keys.sort();
        keys
    }
}

/// Typ nativni (Rust) funkce: prijima Vec<JsValue>, vraci Result<JsValue, String>.
type NativeFn = Rc<dyn Fn(Vec<JsValue>) -> Result<JsValue, String>>;

/// Definice metody tridy ulozena v `JsFunc::Class`.
///
/// Sdilena pro instance metody i staticke metody.
#[derive(Debug, Clone)]
pub struct ClassMethodDef {
    /// Jmeno metody
    pub name: String,
    /// Parametry
    pub params: Vec<Param>,
    /// Telo
    pub body: Vec<Stmt>,
}

/// Reprezentace funkce v runtime.
///
/// - `User`      - funkce definovana v JS kodu, ulozena jako AST + uzavreny scope
/// - `Native`    - funkce implementovana v Rustu (Math.sqrt, console.log, atd.)
/// - `Class`     - JS trida: obsahuje konstruktor, instance metody, staticke metody
/// - `Generator` - generator funkce (`function*`): vraci iterator pres yielded hodnoty
#[derive(Clone)]
pub enum JsFunc {
    /// Uzivatelska JS funkce. Uchovava si uzavreny `env` (closure).
    User { name: Option<String>, params: Vec<Param>, body: FuncBody, env: Rc<RefCell<Env>> },
    /// Nativni Rust funkce. Prvni parametr je jmeno pro debugovani.
    Native(String, NativeFn),
    /// Generator funkce (`function*`). Pri zavolani vraci iterator objekt.
    Generator {
        name: Option<String>,
        params: Vec<Param>,
        body: Vec<Stmt>,
        env: Rc<RefCell<Env>>,
    },
    /// JS trida. `super_val` = vyhodnocena rodicovska trida.
    ///
    /// Konstruktor je ulozeny oddelene od ostatnich metod.
    /// Staticke metody jsou pristupne pres `get_prop` bez `new`.
    Class {
        /// Jmeno tridy (pro `instanceof` a debugovani)
        name: Option<String>,
        /// Vyhodnocena rodicovska trida nebo `None`
        super_val: Option<Box<JsValue>>,
        /// `true` kdyz trida obsahuje explicitni `constructor()`
        has_ctor: bool,
        /// Parametry konstruktoru
        ctor_params: Vec<Param>,
        /// Telo konstruktoru
        ctor_body: Vec<Stmt>,
        /// Instance metody (prideleny kazdemu novemu objektu pri `new`)
        methods: Vec<ClassMethodDef>,
        /// Staticke metody (pristupne pres jmeno tridy)
        statics: Vec<ClassMethodDef>,
        /// Getters (pri pristup k vlastnosti zavolat funkci)
        getters: Vec<ClassMethodDef>,
        /// Setters (pri prirazeni vlastnosti zavolat funkci)
        setters: Vec<ClassMethodDef>,
        /// Uzavreny scope kde byla trida definovana
        env: Rc<RefCell<Env>>,
    }
}

impl std::fmt::Debug for JsFunc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunc::User { name, .. }      => write!(f, "[Function: {}]", name.as_deref().unwrap_or("anonymous")),
            JsFunc::Native(name, _)        => write!(f, "[NativeFunction: {name}]"),
            JsFunc::Class { name, .. }     => write!(f, "[class {}]", name.as_deref().unwrap_or("(anonymous)")),
            JsFunc::Generator { name, .. } => write!(f, "[GeneratorFunction: {}]", name.as_deref().unwrap_or("anonymous")),
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
            JsValue::Map(m) => {
                let pairs: Vec<String> = m.borrow().entries.iter()
                    .map(|(k, v)| format!("{k} => {v}")).collect();
                write!(f, "Map {{ {} }}", pairs.join(", "))
            }
            JsValue::Set(s) => {
                let items: Vec<String> = s.borrow().values.iter().map(|v| v.to_string()).collect();
                write!(f, "Set {{ {} }}", items.join(", "))
            }
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
            JsValue::Map(_)      => "object",
            JsValue::Set(_)      => "object",
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
            (JsValue::Array(a),  JsValue::Array(b))  => Rc::ptr_eq(a, b),
            (JsValue::Map(a),    JsValue::Map(b))    => Rc::ptr_eq(a, b),
            (JsValue::Set(a),    JsValue::Set(b))    => Rc::ptr_eq(a, b),
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
    /// Generator mode: Some = shromazduji yield hodnoty misto preruseni
    /// None = normalni rezim
    yield_buffer: Option<Vec<JsValue>>,
}

// ─── Pomocne funkce ──────────────────────────────────────────────────────────

/// Vrati true kdyz klic je interni (`__key__` format - napr. `__class_chain__`).
fn is_internal_key(k: &str) -> bool {
    k.len() >= 4 && k.starts_with("__") && k.ends_with("__")
}

/// Zkontroluje jestli `proto` je v prototypovem retezci `target`.
/// Implementuje semantiku `proto.isPrototypeOf(target)`.
fn is_in_proto_chain(proto: &Rc<RefCell<JsObject>>, target: &JsValue) -> bool {
    let mut current = match target {
        JsValue::Object(o) => o.borrow().proto.clone(),
        _ => return false,
    };
    let mut depth = 0;
    while let Some(p) = current {
        if depth > 100 { break; }
        if Rc::ptr_eq(&p, proto) { return true; }
        current = p.borrow().proto.clone();
        depth += 1;
    }
    false
}

/// Vytvori iterator objekt (s `next()` a `Symbol.iterator`) z pole hodnot.
///
/// Pouziva se pro Map.keys(), Set.values(), atd. - vraci lazy-looking objekt
/// ktery lze pouzit v `for...of` nebo primo s `.next()`.
fn make_array_iterator(values: Vec<JsValue>) -> JsValue {
    let values = Rc::new(values);
    let index  = Rc::new(RefCell::new(0usize));

    let v1 = Rc::clone(&values);
    let i1 = Rc::clone(&index);
    let next_fn = native("(iterator).next", move |_| {
        let i = *i1.borrow();
        if i < v1.len() {
            *i1.borrow_mut() = i + 1;
            let mut r = JsObject::new();
            r.set("value".into(), v1[i].clone());
            r.set("done".into(),  JsValue::Bool(false));
            Ok(JsValue::Object(Rc::new(RefCell::new(r))))
        } else {
            let mut r = JsObject::new();
            r.set("value".into(), JsValue::Undefined);
            r.set("done".into(),  JsValue::Bool(true));
            Ok(JsValue::Object(Rc::new(RefCell::new(r))))
        }
    });

    // Symbol.iterator vraci sebe sama (iterator je zaroven iterable)
    let values2 = Rc::clone(&values);
    let index2  = Rc::new(RefCell::new(0usize));
    let self_iter = native("(iterator)[Symbol.iterator]", move |_| {
        let v = Rc::clone(&values2);
        let i = Rc::clone(&index2);
        Ok(make_array_iterator(v.as_ref().clone()))
        // Pro zjednoduseni vratime novy iterator od zacatku
        // (spravnejsi by bylo vratit `this`, ale bez this kontextu)
    });

    let mut obj = JsObject::new();
    obj.set("next".into(), next_fn);
    obj.set("Symbol.iterator".into(), self_iter);
    JsValue::Object(Rc::new(RefCell::new(obj)))
}

impl Interpreter {
    /// Vytvori novy interpreter s inicializovanymi vestavenymi objekty.
    pub fn new() -> Self {
        let global = Environment::new_global();
        setup_builtins(&global);
        Interpreter { global, yield_buffer: None }
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
                    // var = function-scoped (global), let/const = block-scoped
                    let target_env = if *kind == VarKind::Var {
                        Rc::clone(&self.global)
                    } else {
                        Rc::clone(env)
                    };
                    self.destructure_bind(&d.pattern, val, &target_env)?;
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

            // Generator funkce: `function* name(params) { body }`
            Stmt::GeneratorFunc { name, params, body } => {
                let func = JsValue::Function(JsFunc::Generator {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
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

            Stmt::Switch { discriminant, cases } => {
                let value = self.eval(discriminant, env)?;

                // Najdi odpovidajici case (strict ===) a pozici default
                let mut match_idx: Option<usize> = None;
                let mut default_idx: Option<usize> = None;

                for (i, case) in cases.iter().enumerate() {
                    match &case.test {
                        None => { default_idx = Some(i); }
                        Some(test_expr) => {
                            if match_idx.is_none() {
                                let test_val = self.eval(test_expr, env)?;
                                if value.strict_eq(&test_val) {
                                    match_idx = Some(i);
                                }
                            }
                        }
                    }
                }

                // Spust od prvniho odpovidajiciho case, nebo od default
                let start = match_idx.or(default_idx);

                if let Some(start_idx) = start {
                    let switch_env = Environment::new_child(env);
                    for case in &cases[start_idx..] {
                        for stmt in &case.body {
                            match self.exec_stmt(stmt, &switch_env)? {
                                // break (bez labelu) ukonci switch
                                Some(Signal::Break(None)) => return Ok(None),
                                // break s labelem - propaguj nahoru (zpracuje Labeled)
                                Some(s) => return Ok(Some(s)),
                                None => {}
                            }
                        }
                    }
                }

                Ok(None)
            }

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
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
                        Some(s) => return Ok(Some(s)),  // labeled/Return propaguj nahoru
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::DoWhile { body, test } => {
                loop {
                    match self.exec_stmt(body, env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => {}
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
                        ForInit::Var { kind: _, decls } => {
                            for d in decls {
                                let v = match &d.init { Some(e) => self.eval(e, &for_env)?, None => JsValue::Undefined };
                                // for init vzdy bindu do for_env (let/const scoped)
                                let target_env = Rc::clone(&for_env);
                                self.destructure_bind(&d.pattern, v, &target_env)?;
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
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => {}
                        Some(s) => return Ok(Some(s)),
                        None => {}
                    }
                    if let Some(upd) = update { self.eval(upd, &for_env)?; }
                }
                Ok(None)
            }

            Stmt::ForOf { kind: _, target, iter, body } => {
                let arr_val = self.eval(iter, env)?;
                // Podpora pro custom iterables pres Symbol.iterator
                let items = self.collect_iterable(arr_val)?;
                for item in items {
                    let loop_env = Environment::new_child(env);
                    self.bind_target_expr(target, item, &loop_env)?;
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
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
                    self.bind_target_expr(target, JsValue::Str(key), &loop_env)?;
                    match self.exec_stmt(body, &loop_env)? {
                        Some(Signal::Break(None))    => break,
                        Some(Signal::Continue(None)) => continue,
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

            Stmt::Labeled { label, body } => {
                match self.exec_stmt(body, env)? {
                    // break label / continue label odpovidajici tomuto labelu -> konzumuj signal
                    Some(Signal::Break(Some(l)))    if l == *label => Ok(None),
                    Some(Signal::Continue(Some(l))) if l == *label => Ok(None),
                    // vsechno ostatni propaguj (Return, Break(None), Break(jiny_label), ...)
                    other => Ok(other),
                }
            }

            Stmt::Class { name, super_class, body } => {
                let super_val = if let Some(sc) = super_class {
                    Some(Box::new(self.eval(sc, env)?))
                } else { None };
                let cls = self.make_class_func(Some(name.clone()), super_val, body, env);
                env.borrow_mut().define(name, cls);
                Ok(None)
            }
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

            // Generator funkcni vyraz: `const gen = function*() { yield 1; }`
            Expr::GeneratorFunc { name, params, body } => Ok(JsValue::Function(JsFunc::Generator {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                env: Rc::clone(env),
            })),

            // Yield vyraz: `yield value`
            Expr::Yield { value, delegate } => {
                let val = if let Some(e) = value {
                    self.eval(e, env)?
                } else {
                    JsValue::Undefined
                };
                if *delegate {
                    // yield* - delegace na iterable (rozlozi do yield_buffer)
                    let items = self.collect_iterable(val.clone())?;
                    if let Some(buf) = &mut self.yield_buffer {
                        buf.extend(items);
                    }
                    Ok(JsValue::Undefined)
                } else if let Some(buf) = &mut self.yield_buffer {
                    buf.push(val);
                    Ok(JsValue::Undefined)
                } else {
                    Err(JsError::Runtime("yield lze pouzit jen v generator funkci".into()))
                }
            }

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

            Expr::ClassExpr { name, super_class, body } => {
                let super_val = if let Some(sc) = super_class {
                    Some(Box::new(self.eval(sc, env)?))
                } else { None };
                Ok(self.make_class_func(name.clone(), super_val, body, env))
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
                let found = match &r {
                    JsValue::Object(o) => {
                        // Prochazi prototypovym retezcem (max 100 uroven)
                        let mut current: Option<Rc<RefCell<JsObject>>> = Some(Rc::clone(o));
                        let mut found = false;
                        let mut depth = 0;
                        while let Some(obj) = current {
                            if depth > 100 { break; }
                            if obj.borrow().props.contains_key(&key) { found = true; break; }
                            current = obj.borrow().proto.clone();
                            depth += 1;
                        }
                        found
                    }
                    _ => false,
                };
                JsValue::Bool(found)
            }
            BinaryOp::Instanceof => {
                // Ziskej jmeno tridy z praveho operandu
                let class_name = match &r {
                    JsValue::Function(JsFunc::Class { name, .. }) => {
                        name.as_deref().unwrap_or("").to_string()
                    }
                    _ => return Ok(JsValue::Bool(false)),
                };
                if class_name.is_empty() { return Ok(JsValue::Bool(false)); }
                // Zkontroluj retezec trid ulozeny na instanci
                match &l {
                    JsValue::Object(o) => {
                        let obj = o.borrow();
                        if let Some(JsValue::Str(chain)) = obj.props.get("__class_chain__") {
                            JsValue::Bool(chain.split(',').any(|n| n == class_name))
                        } else { JsValue::Bool(false) }
                    }
                    _ => JsValue::Bool(false),
                }
            }
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
                    JsValue::Object(o) => {
                        // Specialni klic __proto__: prirazeni meni prototyp
                        if key == "__proto__" {
                            match &val {
                                JsValue::Object(p) => { o.borrow_mut().proto = Some(Rc::clone(p)); }
                                JsValue::Null       => { o.borrow_mut().proto = None; }
                                _ => {}
                            }
                            return Ok(());
                        }
                        // Setter podpora: kdyz objekt ma `__set_key__`, zavolej setter
                        let setter_key = format!("__set_{key}__");
                        let setter_fn = o.borrow().props.get(&setter_key).cloned();
                        if let Some(setter) = setter_fn {
                            self.call_function(setter, vec![val], Some(obj.clone()))?;
                            return Ok(());
                        }
                        // Frozen objekt: zmeny se tisnich ignoruji (soulad s JS non-strict)
                        if o.borrow().frozen { return Ok(()); }
                        o.borrow_mut().props.insert(key, val);
                        Ok(())
                    }
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

    // ─── Destrukturovani ──────────────────────────────────────────────────────

    /// Binduje hodnotu `val` do promenne/promennych definovanych vzorem `pattern`.
    ///
    /// Pouziva se pri:
    /// - `const [a, b] = arr` (Stmt::Var s Array/Object pattern)
    /// - `function f({ x, y }) {}` (parametry funkci)
    /// - `for (const [k, v] of ...)` (ForOf/ForIn pres bind_target_expr)
    ///
    /// Vsechny deklarovane promenne jsou definovany v `env`.
    fn destructure_bind(&mut self, pattern: &Pattern, val: JsValue, env: &Rc<RefCell<Environment>>) -> Result<(), JsError> {
        match pattern {
            Pattern::Ident(name) => {
                env.borrow_mut().define(name, val);
                Ok(())
            }

            Pattern::Array(elems) => {
                let items: Vec<JsValue> = match &val {
                    JsValue::Array(a) => a.borrow().clone(),
                    // retezec lze destrukturovat jako pole znaku
                    JsValue::Str(s) => s.chars().map(|c| JsValue::Str(c.to_string())).collect(),
                    _ => vec![],
                };
                let mut i = 0usize;
                for elem in elems {
                    let Some(pat) = &elem.pattern else {
                        // hole: preskoc pozici
                        i += 1;
                        continue;
                    };
                    if elem.rest {
                        // ...rest = vsechny zbyvajici prvky
                        let rest = JsValue::Array(Rc::new(RefCell::new(
                            items.get(i..).unwrap_or(&[]).to_vec()
                        )));
                        self.destructure_bind(pat, rest, env)?;
                        break;
                    }
                    let item = items.get(i).cloned().unwrap_or(JsValue::Undefined);
                    let item = if matches!(item, JsValue::Undefined) {
                        if let Some(def) = &elem.default {
                            self.eval(def, env)?
                        } else { item }
                    } else { item };
                    self.destructure_bind(pat, item, env)?;
                    i += 1;
                }
                Ok(())
            }

            Pattern::Object(props) => {
                // Klice ktere uz byly spotrebovany (pro ...rest - zatim neni implementovan)
                for prop in props {
                    let key = match &prop.key {
                        PropKey::Ident(s) | PropKey::Str(s) => s.clone(),
                        PropKey::Num(n) => format!("{}", *n as i64),
                        PropKey::Computed(e) => self.eval(e, env)?.to_string(),
                    };
                    let item = match &val {
                        JsValue::Object(o) => o.borrow().get(&key),
                        _ => JsValue::Undefined,
                    };
                    let item = if matches!(item, JsValue::Undefined) {
                        if let Some(def) = &prop.default {
                            self.eval(def, env)?
                        } else { item }
                    } else { item };
                    self.destructure_bind(&prop.pattern, item, env)?;
                }
                Ok(())
            }
        }
    }

    /// Binduje hodnotu `val` do cile ulozeného jako Expr.
    ///
    /// Pouziva se pro ForOf/ForIn target, kde AST uklada target jako `Expr`
    /// (preveden z Pattern pres `pattern_to_expr` v parseru).
    ///
    /// Podporuje:
    /// - `Expr::Ident` - jednoducha promenna
    /// - `Expr::Array` - array destrukturovani `[a, b]`
    /// - `Expr::Object` - object destrukturovani `{ x, y }`
    fn bind_target_expr(&mut self, target: &Expr, val: JsValue, env: &Rc<RefCell<Environment>>) -> Result<(), JsError> {
        match target {
            Expr::Ident(name) => {
                env.borrow_mut().define(name, val);
                Ok(())
            }
            Expr::Array(items) => {
                let vals: Vec<JsValue> = match &val {
                    JsValue::Array(a) => a.borrow().clone(),
                    JsValue::Str(s) => s.chars().map(|c| JsValue::Str(c.to_string())).collect(),
                    _ => vec![],
                };
                let mut i = 0usize;
                for item in items {
                    let Some(expr) = item else {
                        // hole
                        i += 1;
                        continue;
                    };
                    // rest element je ulozen jako Spread(inner)
                    if let Expr::Spread(inner) = expr.as_ref() {
                        let rest = JsValue::Array(Rc::new(RefCell::new(
                            vals.get(i..).unwrap_or(&[]).to_vec()
                        )));
                        self.bind_target_expr(inner, rest, env)?;
                        break;
                    }
                    let v = vals.get(i).cloned().unwrap_or(JsValue::Undefined);
                    self.bind_target_expr(expr, v, env)?;
                    i += 1;
                }
                Ok(())
            }
            Expr::Object(props) => {
                for prop in props {
                    let key = match &prop.key {
                        PropKey::Ident(s) | PropKey::Str(s) => s.clone(),
                        PropKey::Num(n) => format!("{}", *n as i64),
                        PropKey::Computed(e) => {
                            let e = e.as_ref().clone();
                            self.eval(&e, env)?.to_string()
                        }
                    };
                    let v = match &val {
                        JsValue::Object(o) => o.borrow().get(&key),
                        _ => JsValue::Undefined,
                    };
                    self.bind_target_expr(&prop.value, v, env)?;
                }
                Ok(())
            }
            // Pro prirazeni (x = ...) pouzij assign_to
            other => {
                self.assign_to(other, val, env)
            }
        }
    }

    // ─── Třídy ────────────────────────────────────────────────────────────────

    /// Vytvori JsValue::Function(JsFunc::Class) z AST ClassMember listu.
    ///
    /// Rozdeli cleny na: konstruktor, instance metody, staticke metody, gettery, settery.
    fn make_class_func(
        &self,
        name: Option<String>,
        super_val: Option<Box<JsValue>>,
        body: &[ClassMember],
        env: &Rc<RefCell<Env>>,
    ) -> JsValue {
        let mut has_ctor = false;
        let mut ctor_params = Vec::new();
        let mut ctor_body   = Vec::new();
        let mut methods  = Vec::new();
        let mut statics  = Vec::new();
        let mut getters  = Vec::new();
        let mut setters  = Vec::new();

        for m in body {
            let def = ClassMethodDef {
                name: m.name.clone(),
                params: m.params.clone(),
                body: m.body.clone(),
            };
            if m.name == "constructor" && !m.is_static {
                has_ctor = true;
                ctor_params = m.params.clone();
                ctor_body   = m.body.clone();
            } else if m.is_static {
                statics.push(def);
            } else if m.is_getter {
                getters.push(def);
            } else if m.is_setter {
                setters.push(def);
            } else {
                methods.push(def);
            }
        }

        JsValue::Function(JsFunc::Class {
            name,
            super_val,
            has_ctor,
            ctor_params,
            ctor_body,
            methods,
            statics,
            getters,
            setters,
            env: Rc::clone(env),
        })
    }

    /// Konstruuje novou instanci tridy (`new Foo(args)`).
    fn construct_class(&mut self, class_val: JsValue, args: Vec<JsValue>) -> EvalResult {
        let JsValue::Function(JsFunc::Class {
            name,
            super_val,
            has_ctor,
            ctor_params,
            ctor_body,
            methods,
            statics: _,
            getters,
            setters,
            env,
        }) = class_val else {
            return Err(JsError::Runtime("construct_class: ocekavana trida".into()));
        };

        let this_obj = Rc::new(RefCell::new(JsObject::new()));
        let this_val = JsValue::Object(Rc::clone(&this_obj));

        // Uloz retezec trid pro `instanceof`
        {
            let chain = build_class_chain(name.as_deref().unwrap_or(""), super_val.as_deref());
            if !chain.is_empty() {
                this_obj.borrow_mut().set("__class_chain__".to_string(), JsValue::Str(chain));
            }
        }

        // Env pro metody obsahuje __super_class__ (pro super.method() uvnitr metod)
        let method_env = Environment::new_child(&env);
        if let Some(sv) = &super_val {
            method_env.borrow_mut().define("__super_class__", (**sv).clone());
        }

        // Prirad instance metody objektu
        for mdef in &methods {
            let mfunc = JsValue::Function(JsFunc::User {
                name: Some(mdef.name.clone()),
                params: mdef.params.clone(),
                body: FuncBody::Stmts(mdef.body.clone()),
                env: Rc::clone(&method_env),
            });
            this_obj.borrow_mut().set(mdef.name.clone(), mfunc);
        }

        // Prirad gettery (ulozeny jako __get_name__ pro speciální eval_member handling)
        for gdef in &getters {
            let gfunc = JsValue::Function(JsFunc::User {
                name: Some(gdef.name.clone()),
                params: gdef.params.clone(),
                body: FuncBody::Stmts(gdef.body.clone()),
                env: Rc::clone(&method_env),
            });
            this_obj.borrow_mut().set(format!("__get_{}__", gdef.name), gfunc);
        }

        // Prirad settery
        for sdef in &setters {
            let sfunc = JsValue::Function(JsFunc::User {
                name: Some(sdef.name.clone()),
                params: sdef.params.clone(),
                body: FuncBody::Stmts(sdef.body.clone()),
                env: Rc::clone(&method_env),
            });
            this_obj.borrow_mut().set(format!("__set_{}__", sdef.name), sfunc);
        }

        // Konstruktor env: this + __super_class__
        let ctor_env = Environment::new_child(&env);
        ctor_env.borrow_mut().define("this", this_val.clone());
        if let Some(sv) = &super_val {
            ctor_env.borrow_mut().define("__super_class__", (**sv).clone());
        }

        if has_ctor {
            // Explicitni konstruktor - svaz parametry a spust telo
            let ctor_params = ctor_params.clone();
            self.bind_params(&ctor_params, args, &ctor_env)?;
            self.exec_stmts(&ctor_body, &ctor_env)?;
        } else if let Some(sv) = super_val {
            // Zadny konstruktor + ma super -> auto-deleguj super(args)
            self.run_super_constructor(*sv, args, &this_obj, &ctor_env)?;
        }
        // Else: zadny konstruktor, zadny super -> objekt je prazdny (vlastnosti se priradi rucne)

        Ok(this_val)
    }

    /// Spusti konstruktor rodicovske tridy na existujicim `this` objektu.
    ///
    /// Pouziva se pri `super(args)` uvnitr konstruktoru podtridy.
    /// Mutuje `this_obj` - priradi vlastnosti a metody parenta.
    fn run_super_constructor(
        &mut self,
        super_class: JsValue,
        args: Vec<JsValue>,
        this_obj: &Rc<RefCell<JsObject>>,
        parent_env: &Rc<RefCell<Env>>,
    ) -> Result<(), JsError> {
        match super_class {
            JsValue::Function(JsFunc::Class {
                super_val,
                has_ctor,
                ctor_params,
                ctor_body,
                methods,
                getters,
                setters,
                env,
                ..
            }) => {
                // Env pro metody parenta: super_val jako __super_class__ (pro super.method() uvnitr parenta)
                let method_env = Environment::new_child(&env);
                if let Some(sv) = &super_val {
                    method_env.borrow_mut().define("__super_class__", (**sv).clone());
                }

                // Prirad metody parenta - jen pokud uz nejsou defibovany podtridou
                for mdef in &methods {
                    if !this_obj.borrow().props.contains_key(&mdef.name) {
                        let mfunc = JsValue::Function(JsFunc::User {
                            name: Some(mdef.name.clone()),
                            params: mdef.params.clone(),
                            body: FuncBody::Stmts(mdef.body.clone()),
                            env: Rc::clone(&method_env),
                        });
                        this_obj.borrow_mut().set(mdef.name.clone(), mfunc);
                    }
                }
                for gdef in &getters {
                    let key = format!("__get_{}__", gdef.name);
                    if !this_obj.borrow().props.contains_key(&key) {
                        let gfunc = JsValue::Function(JsFunc::User {
                            name: Some(gdef.name.clone()),
                            params: gdef.params.clone(),
                            body: FuncBody::Stmts(gdef.body.clone()),
                            env: Rc::clone(&method_env),
                        });
                        this_obj.borrow_mut().set(key, gfunc);
                    }
                }
                for sdef in &setters {
                    let key = format!("__set_{}__", sdef.name);
                    if !this_obj.borrow().props.contains_key(&key) {
                        let sfunc = JsValue::Function(JsFunc::User {
                            name: Some(sdef.name.clone()),
                            params: sdef.params.clone(),
                            body: FuncBody::Stmts(sdef.body.clone()),
                            env: Rc::clone(&method_env),
                        });
                        this_obj.borrow_mut().set(key, sfunc);
                    }
                }

                // Spust konstruktor parenta
                let ctor_env = Environment::new_child(&env);
                ctor_env.borrow_mut().define("this", JsValue::Object(Rc::clone(this_obj)));
                if let Some(sv) = &super_val {
                    ctor_env.borrow_mut().define("__super_class__", (**sv).clone());
                }

                if has_ctor {
                    self.bind_params(&ctor_params, args, &ctor_env)?;
                    self.exec_stmts(&ctor_body, &ctor_env)?;
                } else if let Some(sv) = super_val {
                    // Auto-deleguj na praprarodice
                    self.run_super_constructor(*sv, args, this_obj, &ctor_env)?;
                }

                Ok(())
            }
            // Parent je stara-style function constructor (ne class)
            JsValue::Function(JsFunc::User { params, body, env, .. }) => {
                let ctor_env = Environment::new_child(&env);
                ctor_env.borrow_mut().define("this", JsValue::Object(Rc::clone(this_obj)));
                self.bind_params(&params, args, &ctor_env)?;
                if let FuncBody::Stmts(stmts) = body {
                    self.exec_stmts(&stmts, &ctor_env)?;
                }
                Ok(())
            }
            _ => Err(JsError::Runtime("super(): rodicovska hodnota neni trida".into()))
        }
    }

    /// Ziska metodu z tridy pro `super.method()` volani.
    ///
    /// Prochazi hierarchii trid (super_val retezec) pokud metoda neni nalezena.
    /// Vraci `JsValue::Function` nebo `JsValue::Undefined`.
    fn get_class_method_func(&self, class_val: &JsValue, name: &str) -> EvalResult {
        match class_val {
            JsValue::Function(JsFunc::Class { super_val, methods, env, .. }) => {
                for mdef in methods {
                    if mdef.name == name {
                        // Env metody: obsahuje __super_class__ pro dalsi super.method() volani
                        let method_env = Environment::new_child(env);
                        if let Some(sv) = super_val {
                            method_env.borrow_mut().define("__super_class__", (**sv).clone());
                        }
                        return Ok(JsValue::Function(JsFunc::User {
                            name: Some(mdef.name.clone()),
                            params: mdef.params.clone(),
                            body: FuncBody::Stmts(mdef.body.clone()),
                            env: Rc::clone(&method_env),
                        }));
                    }
                }
                // Metoda nenalezena - zkus v super (pro vicenasobnou dedicnost)
                if let Some(sv) = super_val {
                    return self.get_class_method_func(sv, name);
                }
                Ok(JsValue::Undefined)
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    /// Svaze parametry funkce s argumenty do `env`.
    ///
    /// Refaktorovana spolecna logika pouzivana v `call_function`,
    /// `construct_class` i `run_super_constructor`.
    fn bind_params(
        &mut self,
        params: &[Param],
        args: Vec<JsValue>,
        env: &Rc<RefCell<Env>>,
    ) -> Result<(), JsError> {
        let mut arg_idx = 0usize;
        for p in params {
            if p.rest {
                let rest = JsValue::Array(Rc::new(RefCell::new(
                    args.get(arg_idx..).unwrap_or(&[]).to_vec()
                )));
                self.destructure_bind(&p.pattern, rest, env)?;
                break;
            }
            let val = args.get(arg_idx).cloned().unwrap_or(JsValue::Undefined);
            let val = if matches!(val, JsValue::Undefined) {
                if let Some(def) = &p.default {
                    let de = *def.clone();
                    self.eval(&de, env)?
                } else { val }
            } else { val };
            self.destructure_bind(&p.pattern, val, env)?;
            arg_idx += 1;
        }
        Ok(())
    }

    fn eval_member(&mut self, object: &Expr, prop: &MemberProp, optional: bool, env: &Rc<RefCell<Environment>>) -> EvalResult {
        let obj = self.eval(object, env)?;
        if optional && matches!(obj, JsValue::Null | JsValue::Undefined) {
            return Ok(JsValue::Undefined);
        }
        let key = self.resolve_prop_key(prop, env)?;

        // Getter podpora: kdyz objekt ma `__get_key__` vlastnost (funkci), zavolej ji
        if let JsValue::Object(ref o) = obj {
            let getter_key = format!("__get_{key}__");
            let getter_fn = o.borrow().props.get(&getter_key).cloned();
            if let Some(getter) = getter_fn {
                return self.call_function(getter, vec![], Some(obj.clone()));
            }
        }

        self.get_prop(&obj, &key)
    }

    fn get_prop(&self, obj: &JsValue, key: &str) -> EvalResult {
        match obj {
            // Staticke metody tridy: ClassName.staticMethod()
            JsValue::Function(JsFunc::Class { statics, getters, env, super_val, .. }) => {
                for s in statics {
                    if s.name == key {
                        let senv = Environment::new_child(env);
                        if let Some(sv) = super_val {
                            senv.borrow_mut().define("__super_class__", (**sv).clone());
                        }
                        return Ok(JsValue::Function(JsFunc::User {
                            name: Some(s.name.clone()),
                            params: s.params.clone(),
                            body: FuncBody::Stmts(s.body.clone()),
                            env: Rc::clone(&senv),
                        }));
                    }
                }
                // Getters jako vlastnosti tridy (ne bezne)
                for g in getters {
                    if g.name == key {
                        return Ok(JsValue::Function(JsFunc::User {
                            name: Some(g.name.clone()),
                            params: g.params.clone(),
                            body: FuncBody::Stmts(g.body.clone()),
                            env: Rc::clone(env),
                        }));
                    }
                }
                Ok(JsValue::Undefined)
            }
            JsValue::Object(o) => {
                // Specialni klic __proto__ vraci prototyp objektu
                if key == "__proto__" {
                    return Ok(match o.borrow().proto.clone() {
                        Some(p) => JsValue::Object(p),
                        None    => JsValue::Null,
                    });
                }
                Ok(o.borrow().get(key))
            }
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
            // Map vlastnosti: size (read-only)
            JsValue::Map(m) => {
                if key == "size" { return Ok(JsValue::Number(m.borrow().entries.len() as f64)); }
                Ok(JsValue::Undefined)
            }
            // Set vlastnosti: size (read-only)
            JsValue::Set(s) => {
                if key == "size" { return Ok(JsValue::Number(s.borrow().values.len() as f64)); }
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
        // super(args) - volani konstruktoru rodicovske tridy
        if matches!(callee, Expr::Ident(n) if n == "super") {
            let super_class = env.borrow().get("__super_class__")
                .ok_or_else(|| JsError::Runtime("super() lze volat jen uvnitr konstruktoru tridy".into()))?;
            let arg_vals = self.eval_args(args, env)?;
            let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
            if let JsValue::Object(ref this_obj) = this_val {
                let this_obj = Rc::clone(this_obj);
                self.run_super_constructor(super_class, arg_vals, &this_obj, env)?;
            }
            return Ok(this_val);
        }

        // super.method(args) - volani metody rodicovske tridy
        if let Expr::Member { object, prop, .. } = callee {
            if matches!(object.as_ref(), Expr::Ident(n) if n == "super") {
                let super_class = env.borrow().get("__super_class__")
                    .ok_or_else(|| JsError::Runtime("super.method() lze volat jen uvnitr tridy".into()))?;
                let key = self.resolve_prop_key(prop, env)?;
                let method = self.get_class_method_func(&super_class, &key)?;
                let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
                let arg_vals = self.eval_args(args, env)?;
                return self.call_function(method, arg_vals, Some(this_val));
            }
        }

        if let Expr::Member { object, prop, optional: member_opt } = callee {
            let this = self.eval(object, env)?;
            // optional chaining: obj?.method() -> Undefined kdyz obj je null/undefined
            if (optional || *member_opt) && matches!(this, JsValue::Null | JsValue::Undefined) {
                return Ok(JsValue::Undefined);
            }
            let key = self.resolve_prop_key(prop, env)?;

            // Built-in Array/String/Object/Map/Set instance metody -- dispatch pred call_function
            match &this {
                // ─── Map metody ────────────────────────────────────────────
                JsValue::Map(map_rc) => {
                    let map_rc2 = Rc::clone(map_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "set" => {
                            let mut iter = arg_vals.into_iter();
                            let k = iter.next().unwrap_or(JsValue::Undefined);
                            let v = iter.next().unwrap_or(JsValue::Undefined);
                            map_rc2.borrow_mut().set(k, v);
                            return Ok(JsValue::Map(map_rc2));
                        }
                        "get" => {
                            let k = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(map_rc2.borrow().get(&k));
                        }
                        "has" => {
                            let k = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(map_rc2.borrow().has(&k)));
                        }
                        "delete" => {
                            let k = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(map_rc2.borrow_mut().delete(&k)));
                        }
                        "clear" => { map_rc2.borrow_mut().entries.clear(); return Ok(JsValue::Undefined); }
                        "keys" => {
                            let keys: Vec<JsValue> = map_rc2.borrow().entries.iter().map(|(k,_)| k.clone()).collect();
                            return Ok(make_array_iterator(keys));
                        }
                        "values" => {
                            let vals: Vec<JsValue> = map_rc2.borrow().entries.iter().map(|(_,v)| v.clone()).collect();
                            return Ok(make_array_iterator(vals));
                        }
                        "entries" => {
                            let entries: Vec<JsValue> = map_rc2.borrow().entries.iter()
                                .map(|(k,v)| JsValue::Array(Rc::new(RefCell::new(vec![k.clone(), v.clone()]))))
                                .collect();
                            return Ok(make_array_iterator(entries));
                        }
                        "forEach" => {
                            let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let entries: Vec<(JsValue,JsValue)> = map_rc2.borrow().entries.clone();
                            for (k, v) in entries {
                                self.call_function(cb.clone(), vec![v, k, JsValue::Map(Rc::clone(&map_rc2))], None)?;
                            }
                            return Ok(JsValue::Undefined);
                        }
                        _ => return Ok(JsValue::Undefined),
                    }
                }
                // ─── Set metody ────────────────────────────────────────────
                JsValue::Set(set_rc) => {
                    let set_rc2 = Rc::clone(set_rc);
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "add" => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            set_rc2.borrow_mut().add(v);
                            return Ok(JsValue::Set(set_rc2));
                        }
                        "has" => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(set_rc2.borrow().has(&v)));
                        }
                        "delete" => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(set_rc2.borrow_mut().delete(&v)));
                        }
                        "clear" => { set_rc2.borrow_mut().values.clear(); return Ok(JsValue::Undefined); }
                        "keys" | "values" => {
                            let vals: Vec<JsValue> = set_rc2.borrow().values.clone();
                            return Ok(make_array_iterator(vals));
                        }
                        "entries" => {
                            let entries: Vec<JsValue> = set_rc2.borrow().values.iter()
                                .map(|v| JsValue::Array(Rc::new(RefCell::new(vec![v.clone(), v.clone()]))))
                                .collect();
                            return Ok(make_array_iterator(entries));
                        }
                        "forEach" => {
                            let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let vals: Vec<JsValue> = set_rc2.borrow().values.clone();
                            for v in vals {
                                self.call_function(cb.clone(), vec![v.clone(), v, JsValue::Set(Rc::clone(&set_rc2))], None)?;
                            }
                            return Ok(JsValue::Undefined);
                        }
                        _ => return Ok(JsValue::Undefined),
                    }
                }
                JsValue::Object(obj_rc) => {
                    let obj_rc2 = Rc::clone(obj_rc);
                    // ─── Date instance metody ──────────────────────────────
                    if let JsValue::Number(ms) = obj_rc2.borrow().props.get("__date_ms__").cloned().unwrap_or(JsValue::Undefined) {
                        let arg_vals = self.eval_args(args, env)?;
                        let (yr, mo, day, hr, min, sec, ms_part) = ms_to_parts(ms);
                        match key.as_str() {
                            "getTime"           => return Ok(JsValue::Number(ms)),
                            "getFullYear"       => return Ok(JsValue::Number(yr as f64)),
                            "getMonth"          => return Ok(JsValue::Number(mo as f64)),
                            "getDate"           => return Ok(JsValue::Number(day as f64)),
                            "getHours"          => return Ok(JsValue::Number(hr as f64)),
                            "getMinutes"        => return Ok(JsValue::Number(min as f64)),
                            "getSeconds"        => return Ok(JsValue::Number(sec as f64)),
                            "getMilliseconds"   => return Ok(JsValue::Number(ms_part as f64)),
                            "getDay"            => {
                                // Den tydne: 0=Sun,...,6=Sat
                                let days = (ms / 86_400_000.0) as i64;
                                return Ok(JsValue::Number(((days + 4) % 7).rem_euclid(7) as f64));
                            }
                            "valueOf" | "getTimezoneOffset" => return Ok(JsValue::Number(ms)),
                            "toISOString" => {
                                return Ok(JsValue::Str(format!(
                                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                                    yr, mo+1, day, hr, min, sec, ms_part
                                )));
                            }
                            "toLocaleDateString" => {
                                return Ok(JsValue::Str(format!("{}/{}/{}", mo+1, day, yr)));
                            }
                            "toLocaleTimeString" => {
                                return Ok(JsValue::Str(format!("{:02}:{:02}:{:02}", hr, min, sec)));
                            }
                            "toLocaleString" | "toString" => {
                                return Ok(JsValue::Str(format!(
                                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                                    yr, mo+1, day, hr, min, sec
                                )));
                            }
                            "toDateString" => {
                                return Ok(JsValue::Str(format!("{:04}-{:02}-{:02}", yr, mo+1, day)));
                            }
                            "setTime" => {
                                let new_ms = arg_vals.into_iter().next()
                                    .map(|v| v.to_number()).unwrap_or(f64::NAN);
                                obj_rc2.borrow_mut().props.insert("__date_ms__".into(), JsValue::Number(new_ms));
                                return Ok(JsValue::Number(new_ms));
                            }
                            _ => {}
                        }
                    }
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        // obj.hasOwnProperty("key") - kontrola vlastni vlastnosti
                        "hasOwnProperty" => {
                            let k = arg_vals.into_iter().next()
                                .map(|v| v.to_string()).unwrap_or_default();
                            return Ok(JsValue::Bool(obj_rc2.borrow().has_own(&k)));
                        }
                        // obj.isPrototypeOf(other) - je this v proto retezci other?
                        "isPrototypeOf" => {
                            let target = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(JsValue::Bool(is_in_proto_chain(&obj_rc2, &target)));
                        }
                        // obj.propertyIsEnumerable("key") - vlastni + ne-interni
                        "propertyIsEnumerable" => {
                            let k = arg_vals.into_iter().next()
                                .map(|v| v.to_string()).unwrap_or_default();
                            let is_enum = obj_rc2.borrow().has_own(&k) && !is_internal_key(&k);
                            return Ok(JsValue::Bool(is_enum));
                        }
                        "toString" => return Ok(JsValue::Str("[object Object]".into())),
                        "valueOf"  => return Ok(JsValue::Object(Rc::clone(&obj_rc2))),
                        _ => {
                            // Normalni method call
                            let func = self.get_prop(&this, &key)?;
                            return self.call_function(func, arg_vals, Some(this));
                        }
                    }
                }
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
                        // Date staticke metody
                        ("Date", "now") => return Ok(JsValue::Number(now_ms())),
                        ("Date", "parse") => {
                            // Stub - vratime NaN pro neznamy format
                            return Ok(JsValue::Number(f64::NAN));
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
            // Tridu nelze zavolat bez `new`
            JsValue::Function(JsFunc::Class { name, .. }) => {
                Err(JsError::Runtime(format!(
                    "TypeError: trida '{}' musi byt volana s 'new'",
                    name.as_deref().unwrap_or("(anonymous)")
                )))
            }
            JsValue::Function(JsFunc::Native(_, f)) => {
                f(args).map_err(JsError::Runtime)
            }
            JsValue::Function(JsFunc::User { params, body, env, .. }) => {
                let call_env = Environment::new_child(&env);
                let params = params.clone();
                let body = body.clone();
                self.bind_params(&params, args.clone(), &call_env)?;
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
            // Volani generator funkce vraci iterator objekt
            JsValue::Function(JsFunc::Generator { params, body, env, .. }) => {
                self.call_generator(params, body, args, env)
            }
            _ => Err(JsError::Runtime(format!("{func} není funkce"))),
        }
    }

    fn call_new(&mut self, func: JsValue, args: Vec<JsValue>) -> EvalResult {
        // `new ClassName()` pro tridy - speciálni logika
        if matches!(&func, JsValue::Function(JsFunc::Class { .. })) {
            return self.construct_class(func, args);
        }
        // Vestavene konstruktory: Map, Set, ...
        if let JsValue::Function(JsFunc::Native(name, _)) = &func {
            match name.as_str() {
                "Map" | "WeakMap" => return self.construct_map(args),
                "Set" | "WeakSet" => return self.construct_set(args),
                "Date"            => return self.construct_date(args),
                "Error" | "TypeError" | "RangeError" | "SyntaxError"
                | "ReferenceError" | "URIError" | "EvalError" => {
                    return self.construct_error(name.clone(), args);
                }
                _     => {}
            }
        }
        // `new FunctionConstructor()` - stary styl
        let obj = JsValue::Object(Rc::new(RefCell::new(JsObject::new())));
        self.call_function(func, args, Some(obj.clone()))?;
        Ok(obj)
    }

    /// Konstruktor `new Map([[k,v], ...])` nebo `new Map()`.
    fn construct_map(&mut self, args: Vec<JsValue>) -> EvalResult {
        let mut m = JsMap::new();
        if let Some(JsValue::Array(entries)) = args.into_iter().next() {
            for entry in entries.borrow().clone() {
                if let JsValue::Array(pair) = entry {
                    let pair = pair.borrow();
                    let k = pair.get(0).cloned().unwrap_or(JsValue::Undefined);
                    let v = pair.get(1).cloned().unwrap_or(JsValue::Undefined);
                    m.set(k, v);
                }
            }
        }
        Ok(JsValue::Map(Rc::new(RefCell::new(m))))
    }

    /// Konstruktor `new Set([val, ...])` nebo `new Set()`.
    fn construct_set(&mut self, args: Vec<JsValue>) -> EvalResult {
        let mut s = JsSet::new();
        if let Some(iterable) = args.into_iter().next() {
            let items = self.collect_iterable(iterable).unwrap_or_default();
            for v in items { s.add(v); }
        }
        Ok(JsValue::Set(Rc::new(RefCell::new(s))))
    }

    /// Konstruktor `new Date()`, `new Date(ms)`, `new Date("iso-string")`.
    fn construct_date(&mut self, args: Vec<JsValue>) -> EvalResult {
        let ms = match args.into_iter().next() {
            None                       => now_ms(),
            Some(JsValue::Number(n))   => n,
            Some(JsValue::Str(_s))     => now_ms(), // TODO: parse date string
            Some(JsValue::Undefined)   => now_ms(),
            _                          => f64::NAN,
        };
        Ok(make_date_object(ms))
    }

    /// Konstruktor `new Error("msg")`, `new TypeError("msg")`, atd.
    fn construct_error(&mut self, name: String, args: Vec<JsValue>) -> EvalResult {
        let msg = args.into_iter().next()
            .map(|v| v.to_string())
            .unwrap_or_default();
        let mut obj = JsObject::new();
        obj.set("name".into(),    JsValue::Str(name.clone()));
        obj.set("message".into(), JsValue::Str(msg.clone()));
        obj.set("stack".into(),   JsValue::Str(format!("{name}: {msg}")));
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }

    // ─── Generator + iterator protokol ───────────────────────────────────────

    /// Zavola generator funkci a vrati iterator objekt.
    ///
    /// Implementace: spusti cely body v generator rezimu (yield_buffer = Some(vec![])),
    /// sbira yield hodnoty, pak vrati iterator objekt s metodou `next()`.
    fn call_generator(
        &mut self,
        params: Vec<Param>,
        body: Vec<Stmt>,
        args: Vec<JsValue>,
        closure_env: Rc<RefCell<Env>>,
    ) -> EvalResult {
        // Nastav generator rezim
        let prev_buf = self.yield_buffer.take();
        self.yield_buffer = Some(Vec::new());

        // Spust telo generator funkce
        let gen_env = Environment::new_child(&closure_env);
        let params = params.clone();
        let body = body.clone();
        self.bind_params(&params, args, &gen_env)?;
        let _ = self.exec_stmts(&body, &gen_env);

        // Vezmi nahromadene hodnoty
        let yielded = self.yield_buffer.take().unwrap_or_default();
        self.yield_buffer = prev_buf;

        // Vytvor iterator objekt (sdileny refcell pro index)
        let values = Rc::new(yielded);
        let index  = Rc::new(RefCell::new(0usize));

        let values2 = Rc::clone(&values);
        let index2  = Rc::clone(&index);

        let mut iter_obj = JsObject::new();

        // next() metoda
        let next_fn = native("(generator).next", move |_args| {
            let i = *index2.borrow();
            if i < values2.len() {
                *index2.borrow_mut() = i + 1;
                let mut result = JsObject::new();
                result.set("value".into(), values2[i].clone());
                result.set("done".into(),  JsValue::Bool(false));
                Ok(JsValue::Object(Rc::new(RefCell::new(result))))
            } else {
                let mut result = JsObject::new();
                result.set("value".into(), JsValue::Undefined);
                result.set("done".into(),  JsValue::Bool(true));
                Ok(JsValue::Object(Rc::new(RefCell::new(result))))
            }
        });
        iter_obj.set("next".into(), next_fn);

        // [Symbol.iterator]() - vraci this (iterator je zaroven iterable)
        let values3 = Rc::clone(&values);
        let index3  = Rc::new(RefCell::new(0usize));
        let sym_iter_fn = native("(generator)[Symbol.iterator]", move |_| {
            // Vrat novy iterator od zacatku
            let values4 = Rc::clone(&values3);
            let index4  = Rc::clone(&index3);
            let mut obj = JsObject::new();
            let v4 = Rc::clone(&values4);
            let i4 = Rc::clone(&index4);
            obj.set("next".into(), native("(gen.iter).next", move |_| {
                let i = *i4.borrow();
                if i < v4.len() {
                    *i4.borrow_mut() = i + 1;
                    let mut r = JsObject::new();
                    r.set("value".into(), v4[i].clone());
                    r.set("done".into(),  JsValue::Bool(false));
                    Ok(JsValue::Object(Rc::new(RefCell::new(r))))
                } else {
                    let mut r = JsObject::new();
                    r.set("value".into(), JsValue::Undefined);
                    r.set("done".into(),  JsValue::Bool(true));
                    Ok(JsValue::Object(Rc::new(RefCell::new(r))))
                }
            }));
            Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
        });
        iter_obj.set("Symbol.iterator".into(), sym_iter_fn);

        Ok(JsValue::Object(Rc::new(RefCell::new(iter_obj))))
    }

    /// Sbira vsechny hodnoty z iteratoru nebo iterovatelneho objektu.
    ///
    /// Pouzivane v `for...of` pro custom iterables a `yield*`.
    fn collect_iterable(&mut self, val: JsValue) -> Result<Vec<JsValue>, JsError> {
        match &val {
            JsValue::Array(a) => return Ok(a.borrow().clone()),
            JsValue::Str(s)   => return Ok(s.chars().map(|c| JsValue::Str(c.to_string())).collect()),
            // for...of Map -> [key, value] pary
            JsValue::Map(m) => {
                return Ok(m.borrow().entries.iter()
                    .map(|(k, v)| JsValue::Array(Rc::new(RefCell::new(vec![k.clone(), v.clone()]))))
                    .collect());
            }
            // for...of Set -> hodnoty
            JsValue::Set(s) => return Ok(s.borrow().values.clone()),
            _ => {}
        }
        // Zkus Symbol.iterator protocol
        if let JsValue::Object(o) = &val {
            let sym_iter_fn = o.borrow().get("Symbol.iterator");
            if !matches!(sym_iter_fn, JsValue::Undefined) {
                let iterator = self.call_function(sym_iter_fn, vec![], Some(val.clone()))?;
                return self.drain_iterator(iterator);
            }
        }
        Err(JsError::Runtime("for...of: hodnota neni iterovatelna".into()))
    }

    /// Opakuje volani .next() na iterator dokud done == true.
    fn drain_iterator(&mut self, iterator: JsValue) -> Result<Vec<JsValue>, JsError> {
        let mut result = Vec::new();
        loop {
            let next_fn = self.get_prop(&iterator, "next")?;
            if matches!(next_fn, JsValue::Undefined) {
                break;
            }
            let step = self.call_function(next_fn, vec![], Some(iterator.clone()))?;
            // step = { value: x, done: bool }
            let done  = self.get_prop(&step, "done")?.is_truthy();
            let value = self.get_prop(&step, "value")?;
            if done { break; }
            result.push(value);
            if result.len() > 100_000 { // ochrana pred nekonecnou smyckou
                break;
            }
        }
        Ok(result)
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

/// Vybuduje retezec jmen trid pro `instanceof` kontrolu.
///
/// Prochazi hierarchii pres `super_val` a vraci jmena oddelena carkou:
/// `"Dog,Animal,Creature"` - od podtridy ke korenove tride.
fn build_class_chain(class_name: &str, super_val: Option<&JsValue>) -> String {
    let mut chain = class_name.to_string();
    let mut current = super_val;
    while let Some(JsValue::Function(JsFunc::Class { name, super_val: sv, .. })) = current {
        if let Some(n) = name {
            if !n.is_empty() {
                chain.push(',');
                chain.push_str(n);
            }
        }
        current = sv.as_deref();
    }
    chain
}

// ─── JSON serialization / deserialization ────────────────────────────────────

/// Serializuje JsValue do JSON retezce.
fn json_stringify(val: &JsValue, indent: usize, depth: usize) -> Option<String> {
    match val {
        JsValue::Null             => Some("null".into()),
        JsValue::Bool(b)          => Some(b.to_string()),
        JsValue::Number(n) if n.is_nan() || n.is_infinite() => Some("null".into()),
        JsValue::Number(n) => {
            if *n == n.trunc() && n.abs() < 1e15 { Some(format!("{}", *n as i64)) }
            else { Some(format!("{n}")) }
        }
        JsValue::Str(s) => Some(json_escape_str(s)),
        JsValue::Array(a) => {
            let items: Vec<String> = a.borrow().iter()
                .map(|v| json_stringify(v, indent, depth + 1).unwrap_or_else(|| "null".into()))
                .collect();
            if indent == 0 || items.is_empty() {
                Some(format!("[{}]", items.join(",")))
            } else {
                let pad = " ".repeat(indent * (depth + 1));
                let close_pad = " ".repeat(indent * depth);
                Some(format!("[\n{}{}\n{}]",
                    pad, items.join(&format!(",\n{pad}")), close_pad))
            }
        }
        JsValue::Object(o) => {
            let mut pairs: Vec<String> = Vec::new();
            let borrowed = o.borrow();
            let mut keys: Vec<&String> = borrowed.props.keys()
                .filter(|k| !is_internal_key(k)).collect();
            keys.sort();
            for k in keys {
                let v = borrowed.props.get(k).unwrap();
                if let Some(serialized) = json_stringify(v, indent, depth + 1) {
                    pairs.push(format!("{}:{}", json_escape_str(k), serialized));
                }
            }
            if indent == 0 || pairs.is_empty() {
                Some(format!("{{{}}}", pairs.join(",")))
            } else {
                let pad = " ".repeat(indent * (depth + 1));
                let close_pad = " ".repeat(indent * depth);
                Some(format!("{{\n{}{}\n{}}}", pad,
                    pairs.join(&format!(",\n{pad}")), close_pad))
            }
        }
        // undefined, funkce, symboly -> None (vynechano z JSON)
        _ => None,
    }
}

/// Escapuje retezec pro JSON (prida uvozovky, escapuje spec. znaky).
fn json_escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => { out.push_str(&format!("\\u{:04x}", c as u32)); }
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Parsuje JSON retezec na JsValue. Jednoduchy rekurzivni descend parser.
fn json_parse(s: &str) -> Result<JsValue, String> {
    let chars: Vec<char> = s.chars().collect();
    let (val, _) = json_parse_value(&chars, 0)?;
    Ok(val)
}

fn json_skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && matches!(chars[i], ' ' | '\t' | '\n' | '\r') { i += 1; }
    i
}

fn json_parse_value(chars: &[char], pos: usize) -> Result<(JsValue, usize), String> {
    let i = json_skip_ws(chars, pos);
    if i >= chars.len() { return Err("Neocekavany konec JSON".into()); }
    match chars[i] {
        '"' => {
            let (s, end) = json_parse_string(chars, i)?;
            Ok((JsValue::Str(s), end))
        }
        '[' => json_parse_array(chars, i),
        '{' => json_parse_object(chars, i),
        't' => {
            if chars.get(i..i+4) == Some(&['t','r','u','e']) { Ok((JsValue::Bool(true), i+4)) }
            else { Err(format!("Neplatny JSON token na pozici {i}")) }
        }
        'f' => {
            if chars.get(i..i+5) == Some(&['f','a','l','s','e']) { Ok((JsValue::Bool(false), i+5)) }
            else { Err(format!("Neplatny JSON token na pozici {i}")) }
        }
        'n' => {
            if chars.get(i..i+4) == Some(&['n','u','l','l']) { Ok((JsValue::Null, i+4)) }
            else { Err(format!("Neplatny JSON token na pozici {i}")) }
        }
        '-' | '0'..='9' => json_parse_number(chars, i),
        c => Err(format!("Neocekavany znak '{c}' na pozici {i}")),
    }
}

fn json_parse_string(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let mut s = String::new();
    let mut i = start + 1; // preskoc uvodni "
    while i < chars.len() {
        match chars[i] {
            '"' => return Ok((s, i + 1)),
            '\\' => {
                i += 1;
                if i >= chars.len() { break; }
                match chars[i] {
                    '"'  => s.push('"'),
                    '\\' => s.push('\\'),
                    '/'  => s.push('/'),
                    'n'  => s.push('\n'),
                    'r'  => s.push('\r'),
                    't'  => s.push('\t'),
                    'b'  => s.push('\x08'),
                    'f'  => s.push('\x0C'),
                    'u' if i + 4 < chars.len() => {
                        let hex: String = chars[i+1..=i+4].iter().collect();
                        if let Ok(n) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(n) { s.push(c); }
                        }
                        i += 4;
                    }
                    c => s.push(c),
                }
                i += 1;
            }
            c => { s.push(c); i += 1; }
        }
    }
    Err("Neuzavreny JSON retezec".into())
}

fn json_parse_number(chars: &[char], start: usize) -> Result<(JsValue, usize), String> {
    let mut i = start;
    if i < chars.len() && chars[i] == '-' { i += 1; }
    while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    }
    if i < chars.len() && matches!(chars[i], 'e' | 'E') {
        i += 1;
        if i < chars.len() && matches!(chars[i], '+' | '-') { i += 1; }
        while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
    }
    let num_str: String = chars[start..i].iter().collect();
    let n: f64 = num_str.parse().map_err(|_| format!("Neplatne cislo: {num_str}"))?;
    Ok((JsValue::Number(n), i))
}

fn json_parse_array(chars: &[char], start: usize) -> Result<(JsValue, usize), String> {
    let mut items = Vec::new();
    let mut i = json_skip_ws(chars, start + 1);
    if i < chars.len() && chars[i] == ']' { return Ok((JsValue::Array(Rc::new(RefCell::new(items))), i + 1)); }
    loop {
        let (val, end) = json_parse_value(chars, i)?;
        items.push(val);
        i = json_skip_ws(chars, end);
        match chars.get(i) {
            Some(',') => i += 1,
            Some(']') => return Ok((JsValue::Array(Rc::new(RefCell::new(items))), i + 1)),
            _         => return Err(format!("Ocekavano ',' nebo ']' na pozici {i}")),
        }
    }
}

fn json_parse_object(chars: &[char], start: usize) -> Result<(JsValue, usize), String> {
    let mut obj = JsObject::new();
    let mut i = json_skip_ws(chars, start + 1);
    if i < chars.len() && chars[i] == '}' { return Ok((JsValue::Object(Rc::new(RefCell::new(obj))), i + 1)); }
    loop {
        i = json_skip_ws(chars, i);
        if chars.get(i) != Some(&'"') { return Err(format!("Ocekavan klic na pozici {i}")); }
        let (key, end) = json_parse_string(chars, i)?;
        i = json_skip_ws(chars, end);
        if chars.get(i) != Some(&':') { return Err(format!("Ocekavano ':' na pozici {i}")); }
        i += 1;
        let (val, end2) = json_parse_value(chars, i)?;
        obj.set(key, val);
        i = json_skip_ws(chars, end2);
        match chars.get(i) {
            Some(',') => i += 1,
            Some('}') => return Ok((JsValue::Object(Rc::new(RefCell::new(obj))), i + 1)),
            _         => return Err(format!("Ocekavano ',' nebo '}}' na pozici {i}")),
        }
    }
}

// ─── Date pomocne funkce ──────────────────────────────────────────────────────

/// Aktualni cas v milisekundach od Unix epoch.
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Vytvori Date objekt z ms timestamp.
fn make_date_object(ms: f64) -> JsValue {
    let mut obj = JsObject::new();
    obj.set("__date_ms__".into(), JsValue::Number(ms));
    JsValue::Object(Rc::new(RefCell::new(obj)))
}

/// Extrahuje ms timestamp z Date objektu.
fn get_date_ms(val: &JsValue) -> Option<f64> {
    if let JsValue::Object(o) = val {
        if let JsValue::Number(ms) = o.borrow().props.get("__date_ms__")? {
            return Some(*ms);
        }
    }
    None
}

/// Rozlozi ms na (year, month[0-11], day[1-31], hour, min, sec, ms).
fn ms_to_parts(ms: f64) -> (i64, u32, u32, u32, u32, u32, u32) {
    // Jednoducha implementace bez casovych zon (UTC)
    let total_secs = (ms / 1000.0) as i64;
    let ms_part = (ms as i64 % 1000).unsigned_abs() as u32;
    let sec = (total_secs % 60).unsigned_abs() as u32;
    let total_min = total_secs / 60;
    let min = (total_min % 60).unsigned_abs() as u32;
    let total_hour = total_min / 60;
    let hour = (total_hour % 24).unsigned_abs() as u32;
    let total_days = total_hour / 24;
    // Datum z poctu dni od 1970-01-01
    let (year, month, day) = days_to_date(total_days);
    (year, month, day, hour, min, sec, ms_part)
}

fn days_to_date(mut days: i64) -> (i64, u32, u32) {
    // Zjednoduseny algoritmus (Julian day number style)
    let mut year = 1970i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let months = [31u32, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0u32;
    for &m in &months {
        if days < m as i64 { break; }
        days -= m as i64;
        month += 1;
    }
    (year, month, (days + 1) as u32)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
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

    // Object.keys(obj) - vlastni neinterne klic
    obj_ctor.set("keys".into(), native("Object.keys", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let keys: Vec<JsValue> = o.borrow().own_keys()
                    .into_iter().map(JsValue::Str).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(keys))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));

    // Object.values(obj) - hodnoty vlastnich neinternich klicu
    obj_ctor.set("values".into(), native("Object.values", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let obj = o.borrow();
                let vals: Vec<JsValue> = obj.own_keys()
                    .into_iter().map(|k| obj.props.get(&k).cloned().unwrap_or(JsValue::Undefined)).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(vals))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));

    // Object.entries(obj) - [klic, hodnota] pary vlastnich neinternich klicu
    obj_ctor.set("entries".into(), native("Object.entries", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => {
                let obj = o.borrow();
                let entries: Vec<JsValue> = obj.own_keys().into_iter().map(|k| {
                    let v = obj.props.get(&k).cloned().unwrap_or(JsValue::Undefined);
                    JsValue::Array(Rc::new(RefCell::new(vec![JsValue::Str(k), v])))
                }).collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(entries))))
            }
            _ => Ok(JsValue::Array(Rc::new(RefCell::new(vec![]))))
        }
    }));

    // Object.assign(target, ...sources) - kopiruje vlastnosti
    obj_ctor.set("assign".into(), native("Object.assign", |a| {
        let mut iter = a.into_iter();
        let target = iter.next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(target_rc) = &target {
            for src in iter {
                if let JsValue::Object(src_rc) = src {
                    for k in src_rc.borrow().own_keys() {
                        let v = src_rc.borrow().props.get(&k).cloned().unwrap_or(JsValue::Undefined);
                        target_rc.borrow_mut().props.insert(k, v);
                    }
                }
            }
        }
        Ok(target)
    }));

    // Object.freeze(obj) - zakazuje dalsi zmeny vlastnosti
    obj_ctor.set("freeze".into(), native("Object.freeze", |a| {
        let obj = a.into_iter().next().unwrap_or(JsValue::Undefined);
        if let JsValue::Object(o) = &obj {
            o.borrow_mut().frozen = true;
        }
        Ok(obj)
    }));

    // Object.isFrozen(obj)
    obj_ctor.set("isFrozen".into(), native("Object.isFrozen", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(JsValue::Bool(o.borrow().frozen)),
            _                        => Ok(JsValue::Bool(false)),
        }
    }));

    // Object.create(proto) - vytvori objekt s danym prototypem
    obj_ctor.set("create".into(), native("Object.create", |a| {
        let proto = a.into_iter().next().unwrap_or(JsValue::Null);
        let obj = match proto {
            JsValue::Object(p) => JsObject::new_with_proto(p),
            JsValue::Null      => JsObject::new(),
            _                  => return Err("Object.create: proto musi byt Object nebo null".into()),
        };
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }));

    // Object.getPrototypeOf(obj) - vrati prototyp
    obj_ctor.set("getPrototypeOf".into(), native("Object.getPrototypeOf", |a| {
        match a.into_iter().next() {
            Some(JsValue::Object(o)) => Ok(match o.borrow().proto.clone() {
                Some(p) => JsValue::Object(p),
                None    => JsValue::Null,
            }),
            _ => Err("Object.getPrototypeOf: argument musi byt objekt".into()),
        }
    }));

    // Object.setPrototypeOf(obj, proto) - nastavi prototyp
    obj_ctor.set("setPrototypeOf".into(), native("Object.setPrototypeOf", |a| {
        let mut iter = a.into_iter();
        let obj   = iter.next().unwrap_or(JsValue::Undefined);
        let proto = iter.next().unwrap_or(JsValue::Null);
        if let JsValue::Object(obj_rc) = &obj {
            match &proto {
                JsValue::Object(p) => { obj_rc.borrow_mut().proto = Some(Rc::clone(p)); }
                JsValue::Null      => { obj_rc.borrow_mut().proto = None; }
                _ => return Err("Object.setPrototypeOf: proto musi byt Object nebo null".into()),
            }
        }
        Ok(obj)
    }));

    // Object.hasOwn(obj, key) - ES2022, kontroluje vlastni vlastnost
    obj_ctor.set("hasOwn".into(), native("Object.hasOwn", |a| {
        let mut iter = a.into_iter();
        let obj = iter.next().unwrap_or(JsValue::Undefined);
        let key = iter.next().map(|v| v.to_string()).unwrap_or_default();
        match obj {
            JsValue::Object(o) => Ok(JsValue::Bool(o.borrow().has_own(&key))),
            _ => Ok(JsValue::Bool(false)),
        }
    }));

    // Object.is(a, b) - SameValue porovnani (NaN === NaN)
    obj_ctor.set("is".into(), native("Object.is", |a| {
        let mut iter = a.into_iter();
        let a = iter.next().unwrap_or(JsValue::Undefined);
        let b = iter.next().unwrap_or(JsValue::Undefined);
        let eq = match (&a, &b) {
            (JsValue::Number(x), JsValue::Number(y)) => {
                if x.is_nan() && y.is_nan() { true } else { x.to_bits() == y.to_bits() }
            }
            _ => a.strict_eq(&b),
        };
        Ok(JsValue::Bool(eq))
    }));

    // Object.defineProperty(obj, key, descriptor) - zakladni podpora
    obj_ctor.set("defineProperty".into(), native("Object.defineProperty", |a| {
        let mut iter = a.into_iter();
        let obj  = iter.next().unwrap_or(JsValue::Undefined);
        let key  = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let desc = iter.next().unwrap_or(JsValue::Undefined);
        if let (JsValue::Object(obj_rc), JsValue::Object(desc_rc)) = (&obj, &desc) {
            // Setter funkce z descriptoru
            let get_fn = desc_rc.borrow().props.get("get").cloned();
            let set_fn = desc_rc.borrow().props.get("set").cloned();
            if let Some(getter) = get_fn {
                obj_rc.borrow_mut().props.insert(format!("__get_{key}__"), getter);
            }
            if let Some(setter) = set_fn {
                obj_rc.borrow_mut().props.insert(format!("__set_{key}__"), setter);
            }
            // Hodnota z descriptoru
            let val = desc_rc.borrow().get("value");
            if !matches!(val, JsValue::Undefined) {
                obj_rc.borrow_mut().props.insert(key, val);
            }
        }
        Ok(obj)
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

    // Symbol - reprezentujeme jako objekt s "well-known symbols" jako string klice
    // Symbol.iterator = "Symbol.iterator" (pouziva se jako klic vlastnosti)
    let mut sym_obj = JsObject::new();
    sym_obj.set("iterator".into(), JsValue::Str("Symbol.iterator".into()));
    sym_obj.set("toPrimitive".into(), JsValue::Str("Symbol.toPrimitive".into()));
    sym_obj.set("hasInstance".into(), JsValue::Str("Symbol.hasInstance".into()));
    sym_obj.set("asyncIterator".into(), JsValue::Str("Symbol.asyncIterator".into()));
    e.define("Symbol", JsValue::Object(Rc::new(RefCell::new(sym_obj))));

    // Map konstruktor (new Map() / new Map([[k,v], ...]))
    e.define("Map", native("Map", |_| Ok(JsValue::Undefined))); // skutecna logika v call_new

    // Set konstruktor (new Set() / new Set([1,2,3]))
    e.define("Set", native("Set", |_| Ok(JsValue::Undefined))); // skutecna logika v call_new

    // WeakMap / WeakSet - stub (bez GC semantiky, chovaji se jako Map/Set)
    e.define("WeakMap", native("WeakMap", |_| Ok(JsValue::Undefined)));
    e.define("WeakSet", native("WeakSet", |_| Ok(JsValue::Undefined)));

    // ─── JSON ─────────────────────────────────────────────────────────────────
    let mut json_obj = JsObject::new();

    json_obj.set("stringify".into(), native("JSON.stringify", |a| {
        let mut iter = a.into_iter();
        let val   = iter.next().unwrap_or(JsValue::Undefined);
        let _repl = iter.next(); // replacer - ignorujeme
        let space = iter.next().unwrap_or(JsValue::Undefined);
        let indent = match space {
            JsValue::Number(n) if n > 0.0 => n as usize,
            JsValue::Str(s) if !s.is_empty() => s.len(), // " " -> 1
            _ => 0,
        };
        match json_stringify(&val, indent, 0) {
            Some(s) => Ok(JsValue::Str(s)),
            None    => Ok(JsValue::Undefined),
        }
    }));

    json_obj.set("parse".into(), native("JSON.parse", |a| {
        match a.into_iter().next() {
            Some(JsValue::Str(s)) => json_parse(&s).map_err(|e| e),
            _ => Err("JSON.parse: argument musi byt retezec".into()),
        }
    }));

    e.define("JSON", JsValue::Object(Rc::new(RefCell::new(json_obj))));

    // ─── Date ─────────────────────────────────────────────────────────────────
    // Date konstruktor registrujeme jako native - skutecna logika je v call_new
    e.define("Date", native("Date", |_| Ok(JsValue::Undefined)));

    // ─── Error typy ───────────────────────────────────────────────────────────
    // Vsechny Error konstruktory jsou zaregistrovany; skutecna logika je v call_new
    for name in &["Error", "TypeError", "RangeError", "SyntaxError",
                   "ReferenceError", "URIError", "EvalError"] {
        let n = name.to_string();
        e.define(name, native(name, move |_| {
            // Pri volani bez `new` stale vytvor Error objekt (jako v JS)
            let mut obj = JsObject::new();
            obj.set("name".into(), JsValue::Str(n.clone()));
            obj.set("message".into(), JsValue::Str(String::new()));
            obj.set("stack".into(), JsValue::Str(n.clone()));
            Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
        }));
    }

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

    // --- switch/case ---

    #[test]
    fn switch_basic_match() {
        assert_eq!(as_num(run(r#"
            let x = 2;
            switch (x) {
                case 1: return 10;
                case 2: return 20;
                case 3: return 30;
            }
            return 0;
        "#)), 20.0);
    }

    #[test]
    fn switch_default_only() {
        assert_eq!(as_num(run(r#"
            switch (99) {
                case 1: return 1;
                default: return 42;
            }
        "#)), 42.0);
    }

    #[test]
    fn switch_default_in_middle() {
        assert_eq!(as_num(run(r#"
            switch (5) {
                case 1: return 1;
                default: return 99;
                case 2: return 2;
            }
        "#)), 99.0);
    }

    #[test]
    fn switch_no_match_no_default() {
        assert_eq!(as_num(run(r#"
            switch (7) {
                case 1: return 1;
                case 2: return 2;
            }
            return 0;
        "#)), 0.0);
    }

    #[test]
    fn switch_fallthrough() {
        // bez break - probehne case 1 i case 2
        assert_eq!(as_num(run(r#"
            let result = 0;
            switch (1) {
                case 1: result += 10;
                case 2: result += 20;
                case 3: result += 30; break;
                case 4: result += 40;
            }
            return result;
        "#)), 60.0);  // 10 + 20 + 30, case 4 uz ne
    }

    #[test]
    fn switch_break_stops() {
        assert_eq!(as_num(run(r#"
            let result = 0;
            switch (2) {
                case 1: result = 1; break;
                case 2: result = 2; break;
                case 3: result = 3; break;
            }
            return result;
        "#)), 2.0);
    }

    #[test]
    fn switch_multiple_cases_same_body() {
        // case 1: case 2: - oba provedou stejne telo
        assert_eq!(as_str(run(r#"
            function grade(n) {
                switch (n) {
                    case 1:
                    case 2: return "low";
                    case 3: return "mid";
                    case 4:
                    case 5: return "high";
                    default: return "unknown";
                }
            }
            return grade(2);
        "#)), "low");
    }

    #[test]
    fn switch_strict_equality() {
        // switch pouziva === (strict) - "1" != 1
        assert_eq!(as_num(run(r#"
            switch ("1") {
                case 1:  return 10;  // cislo 1, neshoduje se
                case "1": return 20; // retezec "1", shoduje se
            }
            return 0;
        "#)), 20.0);
    }

    #[test]
    fn switch_string_discriminant() {
        assert_eq!(as_num(run(r#"
            const day = "mon";
            switch (day) {
                case "sat":
                case "sun": return 0;
                case "mon": return 1;
                default: return -1;
            }
        "#)), 1.0);
    }

    #[test]
    fn switch_with_block_scope() {
        assert_eq!(as_num(run(r#"
            switch (1) {
                case 1: {
                    let x = 42;
                    return x;
                }
            }
            return 0;
        "#)), 42.0);
    }

    // --- labeled break ---

    #[test]
    fn labeled_break_outer_loop() {
        assert_eq!(as_num(run(r#"
            let result = 0;
            outer: for (let i = 0; i < 3; i++) {
                for (let j = 0; j < 3; j++) {
                    if (i === 1 && j === 1) break outer;
                    result++;
                }
            }
            return result;
        "#)), 4.0);  // (0,0),(0,1),(0,2),(1,0) = 4 iterace
    }

    #[test]
    // ─── Třídy ────────────────────────────────────────────────────────────────

    #[test]
    fn class_basic_constructor_and_method() {
        assert_eq!(as_str(run(r#"
            class Animal {
                constructor(name) { this.name = name; }
                speak() { return this.name + " makes a noise."; }
            }
            const a = new Animal("Dog");
            return a.speak();
        "#)), "Dog makes a noise.");
    }

    #[test]
    fn class_properties_set_in_constructor() {
        assert_eq!(as_str(run(r#"
            class Person {
                constructor(name, age) {
                    this.name = name;
                    this.age = age;
                }
            }
            const p = new Person("Alice", 30);
            return p.name + " " + p.age;
        "#)), "Alice 30");
    }

    #[test]
    fn class_multiple_methods() {
        assert_eq!(as_num(run(r#"
            class Counter {
                constructor() { this.count = 0; }
                inc() { this.count += 1; }
                get_count() { return this.count; }
            }
            const c = new Counter();
            c.inc(); c.inc(); c.inc();
            return c.get_count();
        "#)), 3.0);
    }

    #[test]
    fn class_static_method() {
        assert_eq!(as_num(run(r#"
            class MathHelper {
                static add(a, b) { return a + b; }
                static multiply(a, b) { return a * b; }
            }
            return MathHelper.add(3, 4) + MathHelper.multiply(2, 5);
        "#)), 17.0);
    }

    #[test]
    fn class_inheritance_basic() {
        assert_eq!(as_str(run(r#"
            class Animal {
                constructor(name) { this.name = name; }
                speak() { return this.name + " makes a noise."; }
            }
            class Dog extends Animal {
                constructor(name, breed) {
                    super(name);
                    this.breed = breed;
                }
            }
            const d = new Dog("Rex", "Labrador");
            return d.name + "/" + d.breed + "/" + d.speak();
        "#)), "Rex/Labrador/Rex makes a noise.");
    }

    #[test]
    fn class_method_override() {
        assert_eq!(as_str(run(r#"
            class Animal {
                constructor(name) { this.name = name; }
                speak() { return this.name + " makes a noise."; }
            }
            class Dog extends Animal {
                constructor(name) { super(name); }
                speak() { return this.name + " barks."; }
            }
            const d = new Dog("Rex");
            return d.speak();
        "#)), "Rex barks.");
    }

    #[test]
    fn class_super_method_call() {
        assert_eq!(as_str(run(r#"
            class Animal {
                constructor(name) { this.name = name; }
                speak() { return this.name + " makes a noise."; }
            }
            class Dog extends Animal {
                constructor(name) { super(name); }
                speak() { return super.speak() + " Woof!"; }
            }
            const d = new Dog("Rex");
            return d.speak();
        "#)), "Rex makes a noise. Woof!");
    }

    #[test]
    fn class_no_constructor_auto_super() {
        assert_eq!(as_str(run(r#"
            class Animal {
                constructor(name) { this.name = name; }
                speak() { return this.name; }
            }
            class Cat extends Animal {
                // bez konstruktoru -> auto super(args)
                purr() { return this.name + " purrs."; }
            }
            const c = new Cat("Whiskers");
            return c.speak() + " / " + c.purr();
        "#)), "Whiskers / Whiskers purrs.");
    }

    #[test]
    fn class_instanceof() {
        assert!(as_bool(run(r#"
            class Animal {}
            class Dog extends Animal {}
            const d = new Dog();
            return d instanceof Dog;
        "#)));
    }

    #[test]
    fn class_instanceof_parent() {
        assert!(as_bool(run(r#"
            class Animal {}
            class Dog extends Animal {}
            const d = new Dog();
            return d instanceof Animal;
        "#)));
    }

    #[test]
    fn class_instanceof_false() {
        assert!(!as_bool(run(r#"
            class Animal {}
            class Dog extends Animal {}
            const a = new Animal();
            return a instanceof Dog;
        "#)));
    }

    #[test]
    fn class_expression() {
        assert_eq!(as_str(run(r#"
            const Cat = class {
                constructor(name) { this.name = name; }
            };
            return new Cat("Kitty").name;
        "#)), "Kitty");
    }

    #[test]
    fn class_getter_basic() {
        assert_eq!(as_num(run(r#"
            class Circle {
                constructor(r) { this.r = r; }
                get area() { return 3.14159 * this.r * this.r; }
            }
            const c = new Circle(5);
            return c.area;
        "#)), 3.14159 * 25.0);
    }

    #[test]
    fn class_setter_basic() {
        assert_eq!(as_str(run(r#"
            class Person {
                constructor(name) { this._name = name; }
                get name() { return this._name; }
                set name(v) { this._name = v.trim(); }
            }
            const p = new Person("Alice");
            p.name = "  Bob  ";
            return p.name;
        "#)), "Bob");
    }

    #[test]
    fn class_three_level_inheritance() {
        assert_eq!(as_str(run(r#"
            class A {
                constructor() { this.val = "A"; }
                who() { return "A"; }
            }
            class B extends A {
                constructor() { super(); this.val += "B"; }
                who() { return super.who() + "B"; }
            }
            class C extends B {
                constructor() { super(); this.val += "C"; }
                who() { return super.who() + "C"; }
            }
            const c = new C();
            return c.val + "/" + c.who();
        "#)), "ABC/ABC");
    }

    #[test]
    fn class_method_uses_this() {
        assert_eq!(as_num(run(r#"
            class Rect {
                constructor(w, h) { this.w = w; this.h = h; }
                area() { return this.w * this.h; }
                perimeter() { return 2 * (this.w + this.h); }
            }
            const r = new Rect(3, 4);
            return r.area() + r.perimeter();
        "#)), 26.0);  // 12 + 14
    }

    fn labeled_break_switch_in_loop() {
        // break v switch nema prerusit obalujici cyklus
        assert_eq!(as_num(run(r#"
            let sum = 0;
            for (let i = 0; i < 3; i++) {
                switch (i) {
                    case 1: sum += 10; break; // break ukonci switch, ne for
                    default: sum += 1;
                }
            }
            return sum;
        "#)), 12.0);  // i=0: +1, i=1: +10, i=2: +1
    }

    // ─── Destructuring ────────────────────────────────────────────────────────

    #[test]
    fn array_destructuring_basic() {
        assert_eq!(as_num(run("const [a, b] = [1, 2]; return a + b;")), 3.0);
    }

    #[test]
    fn array_destructuring_skip() {
        // hole preskoci prvek
        assert_eq!(as_num(run("const [a, , c] = [1, 2, 3]; return a + c;")), 4.0);
    }

    #[test]
    fn array_destructuring_default() {
        // default kdyz prvek je undefined
        assert_eq!(as_num(run("const [a, b = 99] = [1]; return b;")), 99.0);
        assert_eq!(as_num(run("const [a, b = 99] = [1, 5]; return b;")), 5.0);
    }

    #[test]
    fn array_destructuring_rest() {
        assert_eq!(as_num(run("const [a, ...rest] = [1, 2, 3]; return rest.length;")), 2.0);
        assert_eq!(as_num(run("const [a, ...rest] = [1, 2, 3]; return rest[0];")), 2.0);
    }

    #[test]
    fn object_destructuring_basic() {
        assert_eq!(as_num(run("const { x, y } = { x: 10, y: 20 }; return x + y;")), 30.0);
    }

    #[test]
    fn object_destructuring_rename() {
        // { key: newName } - prejmenovani
        assert_eq!(as_num(run("const { x: a, y: b } = { x: 3, y: 4 }; return a + b;")), 7.0);
    }

    #[test]
    fn object_destructuring_default() {
        assert_eq!(as_num(run("const { x = 42 } = {}; return x;")), 42.0);
        assert_eq!(as_num(run("const { x = 42 } = { x: 5 }; return x;")), 5.0);
    }

    #[test]
    fn nested_array_destructuring() {
        assert_eq!(as_num(run("const [[a, b], c] = [[1, 2], 3]; return a + b + c;")), 6.0);
    }

    #[test]
    fn nested_object_destructuring() {
        assert_eq!(as_num(run("const { a: { b } } = { a: { b: 99 } }; return b;")), 99.0);
    }

    #[test]
    fn function_param_array_destructuring() {
        assert_eq!(as_num(run(r#"
            function sum([a, b]) { return a + b; }
            return sum([10, 20]);
        "#)), 30.0);
    }

    #[test]
    fn function_param_object_destructuring() {
        assert_eq!(as_num(run(r#"
            function greet({ name, age = 0 }) { return age; }
            return greet({ name: "Alice", age: 25 });
        "#)), 25.0);
    }

    #[test]
    fn function_param_object_default() {
        assert_eq!(as_num(run(r#"
            function f({ x = 10 }) { return x; }
            return f({});
        "#)), 10.0);
    }

    #[test]
    fn for_of_array_destructuring() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            for (const [k, v] of [[1, 10], [2, 20]]) {
                sum += k + v;
            }
            return sum;
        "#)), 33.0);  // (1+10) + (2+20) = 33
    }

    #[test]
    fn for_of_object_destructuring() {
        assert_eq!(as_num(run(r#"
            let sum = 0;
            for (const { x, y } of [{ x: 1, y: 2 }, { x: 3, y: 4 }]) {
                sum += x + y;
            }
            return sum;
        "#)), 10.0);  // (1+2) + (3+4) = 10
    }

    #[test]
    fn destructuring_in_arrow_params() {
        assert_eq!(as_num(run(r#"
            const fn = ([a, b]) => a + b;
            return fn([5, 6]);
        "#)), 11.0);
    }

    #[test]
    fn array_destructuring_from_string() {
        // retezec lze destrukturovat jako pole znaku
        assert_eq!(as_str(run(r#"
            const [a, b] = "hi";
            return a + b;
        "#)), "hi");
    }

    // ─── Batch 4: Prototype chain ─────────────────────────────────────────────

    #[test]
    fn proto_chain_property_lookup() {
        // Vlastnost na proto je videt pres obj.prop
        assert_eq!(as_num(run(r#"
            const proto = { x: 42 };
            const obj = Object.create(proto);
            return obj.x;
        "#)), 42.0);
    }

    #[test]
    fn proto_own_overrides_inherited() {
        // Vlastni vlastnost ma prednost pred proto
        assert_eq!(as_num(run(r#"
            const proto = { x: 1 };
            const obj = Object.create(proto);
            obj.x = 99;
            return obj.x;
        "#)), 99.0);
    }

    #[test]
    fn object_create_null() {
        // Object.create(null) - zadny prototyp
        assert!(matches!(run(r#"
            const obj = Object.create(null);
            return obj.x;
        "#), JsValue::Undefined));
    }

    #[test]
    fn object_get_prototype_of() {
        // Object.getPrototypeOf vrati prototyp
        assert_eq!(as_bool(run(r#"
            const proto = { x: 1 };
            const obj = Object.create(proto);
            return Object.getPrototypeOf(obj) === proto;
        "#)), true);
    }

    #[test]
    fn object_set_prototype_of() {
        // Object.setPrototypeOf meni prototyp
        assert_eq!(as_num(run(r#"
            const proto = { y: 77 };
            const obj = {};
            Object.setPrototypeOf(obj, proto);
            return obj.y;
        "#)), 77.0);
    }

    #[test]
    fn has_own_property() {
        // hasOwnProperty vrati true jen pro vlastni vlastnosti
        assert_eq!(as_bool(run(r#"
            const proto = { inherited: 1 };
            const obj = Object.create(proto);
            obj.own = 2;
            return obj.hasOwnProperty("own");
        "#)), true);
        assert_eq!(as_bool(run(r#"
            const proto = { inherited: 1 };
            const obj = Object.create(proto);
            return obj.hasOwnProperty("inherited");
        "#)), false);
    }

    #[test]
    fn is_prototype_of() {
        // isPrototypeOf kontroluje proto retezec
        assert_eq!(as_bool(run(r#"
            const proto = {};
            const obj = Object.create(proto);
            return proto.isPrototypeOf(obj);
        "#)), true);
    }

    #[test]
    fn is_prototype_of_false() {
        assert_eq!(as_bool(run(r#"
            const a = {};
            const b = {};
            return a.isPrototypeOf(b);
        "#)), false);
    }

    #[test]
    fn property_is_enumerable() {
        // propertyIsEnumerable: vlastni ne-interni vlastnost = true
        assert_eq!(as_bool(run(r#"
            const obj = { x: 1 };
            return obj.propertyIsEnumerable("x");
        "#)), true);
        assert_eq!(as_bool(run(r#"
            const proto = { y: 2 };
            const obj = Object.create(proto);
            return obj.propertyIsEnumerable("y");
        "#)), false);
    }

    #[test]
    fn object_freeze_prevents_mutation() {
        // Object.freeze: zmeny se ignoruji
        assert_eq!(as_num(run(r#"
            const obj = { x: 5 };
            Object.freeze(obj);
            obj.x = 99;
            return obj.x;
        "#)), 5.0);
    }

    #[test]
    fn object_is_frozen() {
        assert_eq!(as_bool(run(r#"
            const obj = { x: 1 };
            Object.freeze(obj);
            return Object.isFrozen(obj);
        "#)), true);
        assert_eq!(as_bool(run(r#"
            const obj = { x: 1 };
            return Object.isFrozen(obj);
        "#)), false);
    }

    #[test]
    fn object_keys_skip_internal() {
        // Object.keys nevrati interni __key__ vlastnosti
        assert_eq!(as_num(run(r#"
            class Foo { constructor() { this.x = 1; this.y = 2; } }
            const obj = new Foo();
            return Object.keys(obj).length;
        "#)), 2.0);
    }

    #[test]
    fn object_has_own() {
        // Object.hasOwn (ES2022) - staticka verze hasOwnProperty
        assert_eq!(as_bool(run(r#"
            const obj = { a: 1 };
            return Object.hasOwn(obj, "a");
        "#)), true);
        assert_eq!(as_bool(run(r#"
            const obj = { a: 1 };
            return Object.hasOwn(obj, "b");
        "#)), false);
    }

    #[test]
    fn object_is_same_value() {
        // Object.is: NaN === NaN, +0 !== -0
        assert_eq!(as_bool(run(r#"return Object.is(NaN, NaN);"#)), true);
        assert_eq!(as_bool(run(r#"return Object.is(1, 1);"#)), true);
        assert_eq!(as_bool(run(r#"return Object.is(1, 2);"#)), false);
    }

    #[test]
    fn object_define_property_getter() {
        // Object.defineProperty s get/set
        assert_eq!(as_num(run(r#"
            const obj = { _x: 10 };
            Object.defineProperty(obj, "x", {
                get: function() { return this._x * 2; }
            });
            return obj.x;
        "#)), 20.0);
    }

    #[test]
    fn proto_chain_set_prototype_of_null() {
        // Object.setPrototypeOf(obj, null) odstrani prototyp
        assert!(matches!(run(r#"
            const proto = { y: 5 };
            const obj = Object.create(proto);
            Object.setPrototypeOf(obj, null);
            return obj.y;
        "#), JsValue::Undefined));
    }

    #[test]
    fn proto_chain_proto_assignment() {
        // obj.__proto__ = proto
        assert_eq!(as_num(run(r#"
            const proto = { z: 77 };
            const obj = {};
            obj.__proto__ = proto;
            return obj.z;
        "#)), 77.0);
    }

    #[test]
    fn in_operator_walks_proto_chain() {
        // `in` operator hleda i v proto retezci
        assert_eq!(as_bool(run(r#"
            const proto = { inherited: 1 };
            const obj = Object.create(proto);
            return "inherited" in obj;
        "#)), true);
    }

    #[test]
    fn object_values_skip_internal() {
        // Object.values nevrati interni vlastnosti
        assert_eq!(as_num(run(r#"
            const obj = { a: 1, b: 2, c: 3 };
            return Object.values(obj).length;
        "#)), 3.0);
    }

    // ─── Batch 5: Generatory + iterator protokol ──────────────────────────────

    #[test]
    fn generator_basic_yield() {
        // Zakladni generator: yield 1, yield 2, yield 3
        assert_eq!(as_num(run(r#"
            function* gen() {
                yield 1;
                yield 2;
                yield 3;
            }
            const it = gen();
            const a = it.next().value;
            const b = it.next().value;
            const c = it.next().value;
            return a + b + c;
        "#)), 6.0);
    }

    #[test]
    fn generator_done_flag() {
        // done = true po dokonceni
        assert_eq!(as_bool(run(r#"
            function* gen() { yield 1; }
            const it = gen();
            it.next();
            return it.next().done;
        "#)), true);
    }

    #[test]
    fn generator_for_of() {
        // Generator pouzity v for...of
        assert_eq!(as_num(run(r#"
            function* range(n) {
                for (let i = 0; i < n; i++) {
                    yield i;
                }
            }
            let sum = 0;
            for (const x of range(5)) {
                sum += x;
            }
            return sum;
        "#)), 10.0);
    }

    #[test]
    fn generator_expression() {
        // Generator funkcni vyraz
        assert_eq!(as_num(run(r#"
            const gen = function*() {
                yield 10;
                yield 20;
            };
            let sum = 0;
            for (const x of gen()) { sum += x; }
            return sum;
        "#)), 30.0);
    }

    #[test]
    fn generator_yield_star_array() {
        // yield* deleguje na pole
        assert_eq!(as_num(run(r#"
            function* gen() {
                yield* [1, 2, 3];
                yield 4;
            }
            let sum = 0;
            for (const x of gen()) { sum += x; }
            return sum;
        "#)), 10.0);
    }

    #[test]
    fn generator_yield_star_other_gen() {
        // yield* deleguje na jiny generator
        assert_eq!(as_num(run(r#"
            function* inner() { yield 1; yield 2; }
            function* outer() { yield* inner(); yield 3; }
            let sum = 0;
            for (const x of outer()) { sum += x; }
            return sum;
        "#)), 6.0);
    }

    #[test]
    fn symbol_iterator_custom_iterable() {
        // Custom iterable s Symbol.iterator
        assert_eq!(as_num(run(r#"
            const range = {
                from: 1,
                to: 5,
                [Symbol.iterator]() {
                    let i = this.from;
                    const to = this.to;
                    return {
                        next() {
                            if (i <= to) {
                                return { value: i++, done: false };
                            }
                            return { value: undefined, done: true };
                        }
                    };
                }
            };
            let sum = 0;
            for (const x of range) { sum += x; }
            return sum;
        "#)), 15.0);
    }

    #[test]
    fn symbol_iterator_string_concat_key() {
        // Symbol.iterator je dostupny jako Symbol.iterator property
        assert_eq!(as_str(run(r#"
            return Symbol.iterator;
        "#)), "Symbol.iterator");
    }

    #[test]
    fn generator_parser_function_star_decl() {
        // Parser test: function* decl
        assert_eq!(as_num(run(r#"
            function* nums() { yield 1; yield 2; yield 3; }
            const arr = [];
            for (const n of nums()) { arr.push(n); }
            return arr.length;
        "#)), 3.0);
    }

    #[test]
    fn generator_next_returns_object_with_value_and_done() {
        // next() vraci { value, done }
        assert_eq!(as_bool(run(r#"
            function* g() { yield 42; }
            const it = g();
            const step = it.next();
            return step.value === 42 && step.done === false;
        "#)), true);
    }

    #[test]
    fn generator_multiple_calls() {
        // Kazde volani gen() vraci novy iterator
        assert_eq!(as_num(run(r#"
            function* gen() { yield 1; yield 2; }
            const it1 = gen();
            const it2 = gen();
            it1.next();
            // it2 zacina znovu od zacatku
            return it2.next().value;
        "#)), 1.0);
    }

    #[test]
    fn for_of_with_string() {
        // for...of na retezci (string je iterable)
        assert_eq!(as_num(run(r#"
            let count = 0;
            for (const ch of "abc") { count++; }
            return count;
        "#)), 3.0);
    }

    #[test]
    fn generator_with_params() {
        // Generator prijima parametry
        assert_eq!(as_num(run(r#"
            function* take(arr, n) {
                for (let i = 0; i < n && i < arr.length; i++) {
                    yield arr[i];
                }
            }
            let sum = 0;
            for (const x of take([10, 20, 30, 40], 3)) { sum += x; }
            return sum;
        "#)), 60.0);
    }

    // ─── Batch A: Map ─────────────────────────────────────────────────────────

    #[test]
    fn map_basic_set_get() {
        assert_eq!(as_num(run(r#"
            const m = new Map();
            m.set("a", 1);
            m.set("b", 2);
            return m.get("a") + m.get("b");
        "#)), 3.0);
    }

    #[test]
    fn map_has_delete() {
        assert_eq!(as_bool(run(r#"
            const m = new Map();
            m.set("x", 10);
            const had = m.has("x");
            m.delete("x");
            return had && !m.has("x");
        "#)), true);
    }

    #[test]
    fn map_size() {
        assert_eq!(as_num(run(r#"
            const m = new Map();
            m.set(1, "a");
            m.set(2, "b");
            m.set(3, "c");
            return m.size;
        "#)), 3.0);
    }

    #[test]
    fn map_constructor_with_entries() {
        assert_eq!(as_num(run(r#"
            const m = new Map([["a", 1], ["b", 2], ["c", 3]]);
            return m.size;
        "#)), 3.0);
    }

    #[test]
    fn map_for_of() {
        assert_eq!(as_num(run(r#"
            const m = new Map([["x", 10], ["y", 20]]);
            let sum = 0;
            for (const [k, v] of m) { sum += v; }
            return sum;
        "#)), 30.0);
    }

    #[test]
    fn map_object_key() {
        // Objekt jako klic - referencni rovnost
        assert_eq!(as_num(run(r#"
            const m = new Map();
            const key = {};
            m.set(key, 99);
            return m.get(key);
        "#)), 99.0);
    }

    #[test]
    fn map_clear() {
        assert_eq!(as_num(run(r#"
            const m = new Map([["a", 1], ["b", 2]]);
            m.clear();
            return m.size;
        "#)), 0.0);
    }

    #[test]
    fn map_keys_values() {
        assert_eq!(as_num(run(r#"
            const m = new Map([["a", 1], ["b", 2]]);
            let keySum = 0;
            for (const k of m.keys()) { keySum++; }
            let valSum = 0;
            for (const v of m.values()) { valSum += v; }
            return keySum + valSum;
        "#)), 5.0);
    }

    #[test]
    fn map_foreach() {
        assert_eq!(as_num(run(r#"
            const m = new Map([["a", 1], ["b", 2], ["c", 3]]);
            let sum = 0;
            m.forEach((v, k) => { sum += v; });
            return sum;
        "#)), 6.0);
    }

    #[test]
    fn map_update_existing_key() {
        assert_eq!(as_num(run(r#"
            const m = new Map();
            m.set("k", 1);
            m.set("k", 2);
            return m.get("k");
        "#)), 2.0);
    }

    // ─── Batch A: Set ─────────────────────────────────────────────────────────

    #[test]
    fn set_basic_add_has() {
        assert_eq!(as_bool(run(r#"
            const s = new Set();
            s.add(1);
            s.add(2);
            s.add(2); // duplikat
            return s.has(1) && s.has(2) && s.size === 2;
        "#)), true);
    }

    #[test]
    fn set_delete() {
        assert_eq!(as_bool(run(r#"
            const s = new Set([1, 2, 3]);
            s.delete(2);
            return !s.has(2) && s.size === 2;
        "#)), true);
    }

    #[test]
    fn set_for_of() {
        assert_eq!(as_num(run(r#"
            const s = new Set([1, 2, 3, 4, 5]);
            let sum = 0;
            for (const v of s) { sum += v; }
            return sum;
        "#)), 15.0);
    }

    #[test]
    fn set_constructor_with_array() {
        assert_eq!(as_num(run(r#"
            const s = new Set([1, 2, 2, 3, 3, 3]);
            return s.size;
        "#)), 3.0);
    }

    #[test]
    fn set_clear() {
        assert_eq!(as_num(run(r#"
            const s = new Set([1, 2, 3]);
            s.clear();
            return s.size;
        "#)), 0.0);
    }

    #[test]
    fn set_foreach() {
        assert_eq!(as_num(run(r#"
            const s = new Set([10, 20, 30]);
            let sum = 0;
            s.forEach(v => { sum += v; });
            return sum;
        "#)), 60.0);
    }

    #[test]
    fn set_values_iterator() {
        assert_eq!(as_num(run(r#"
            const s = new Set([5, 10, 15]);
            let sum = 0;
            for (const v of s.values()) { sum += v; }
            return sum;
        "#)), 30.0);
    }

    // ─── Batch B: JSON ────────────────────────────────────────────────────────

    #[test]
    fn json_stringify_number() {
        assert_eq!(as_str(run(r#"return JSON.stringify(42);"#)), "42");
    }

    #[test]
    fn json_stringify_string() {
        assert_eq!(as_str(run(r#"return JSON.stringify("hello");"#)), "\"hello\"");
    }

    #[test]
    fn json_stringify_bool_null() {
        assert_eq!(as_str(run(r#"return JSON.stringify(true);"#)), "true");
        assert_eq!(as_str(run(r#"return JSON.stringify(null);"#)), "null");
    }

    #[test]
    fn json_stringify_array() {
        assert_eq!(as_str(run(r#"return JSON.stringify([1,2,3]);"#)), "[1,2,3]");
    }

    #[test]
    fn json_stringify_object() {
        assert_eq!(as_str(run(r#"return JSON.stringify({a:1,b:"x"});"#)), r#"{"a":1,"b":"x"}"#);
    }

    #[test]
    fn json_stringify_nested() {
        assert_eq!(as_str(run(r#"return JSON.stringify({a:[1,2],b:{c:3}});"#)),
            r#"{"a":[1,2],"b":{"c":3}}"#);
    }

    #[test]
    fn json_stringify_undefined_omitted() {
        // undefined a funkce se vynechavaji z objektu
        assert_eq!(as_str(run(r#"return JSON.stringify({a:1,b:undefined,c:2});"#)),
            r#"{"a":1,"c":2}"#);
    }

    #[test]
    fn json_parse_number() {
        assert_eq!(as_num(run(r#"return JSON.parse("42");"#)), 42.0);
    }

    #[test]
    fn json_parse_string() {
        assert_eq!(as_str(run(r#"return JSON.parse('"hello"');"#)), "hello");
    }

    #[test]
    fn json_parse_array() {
        assert_eq!(as_num(run(r#"
            const a = JSON.parse('[1,2,3]');
            return a[1];
        "#)), 2.0);
    }

    #[test]
    fn json_parse_object() {
        assert_eq!(as_num(run(r#"
            const o = JSON.parse('{"x":10,"y":20}');
            return o.x + o.y;
        "#)), 30.0);
    }

    #[test]
    fn json_roundtrip() {
        // Testujeme ze parse(stringify(x)) zachova hodnoty (ne nutne poradi klicu)
        assert_eq!(as_num(run(r#"
            const obj = {x:42, y:99};
            const s = JSON.stringify(obj);
            const o2 = JSON.parse(s);
            return o2.x + o2.y;
        "#)), 141.0);
        assert_eq!(as_num(run(r#"
            const arr = [1, 2, 3, 4, 5];
            const s = JSON.stringify(arr);
            const a2 = JSON.parse(s);
            return a2[0] + a2[4];
        "#)), 6.0);
    }

    // ─── Batch B: Date ────────────────────────────────────────────────────────

    #[test]
    fn date_now_is_number() {
        assert!(matches!(run(r#"return Date.now();"#), JsValue::Number(_)));
    }

    #[test]
    fn date_constructor_epoch() {
        // new Date(0) -> 1970-01-01T00:00:00.000Z
        assert_eq!(as_str(run(r#"return new Date(0).toISOString();"#)),
            "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn date_get_time() {
        assert_eq!(as_num(run(r#"return new Date(1000).getTime();"#)), 1000.0);
    }

    #[test]
    fn date_get_full_year() {
        // 2000-01-01 = 946684800000 ms
        assert_eq!(as_num(run(r#"return new Date(946684800000).getFullYear();"#)), 2000.0);
    }

    #[test]
    fn date_get_month() {
        // 2000-03-15 = mesic je 2 (0-indexed)
        assert_eq!(as_num(run(r#"return new Date(946684800000).getMonth();"#)), 0.0);
    }

    #[test]
    fn date_get_date() {
        assert_eq!(as_num(run(r#"return new Date(946684800000).getDate();"#)), 1.0);
    }

    #[test]
    fn date_to_iso_string_known() {
        // 2024-06-15T12:30:45.500Z = 1718454645500 ms
        assert_eq!(as_str(run(r#"return new Date(1718454645500).toISOString();"#)),
            "2024-06-15T12:30:45.500Z");
    }

    // ─── Batch B: Error types ─────────────────────────────────────────────────

    #[test]
    fn error_basic() {
        assert_eq!(as_str(run(r#"
            const e = new Error("oops");
            return e.message;
        "#)), "oops");
    }

    #[test]
    fn error_name() {
        assert_eq!(as_str(run(r#"
            const e = new Error("x");
            return e.name;
        "#)), "Error");
    }

    #[test]
    fn error_type_error() {
        assert_eq!(as_str(run(r#"
            const e = new TypeError("bad type");
            return e.name + ": " + e.message;
        "#)), "TypeError: bad type");
    }

    #[test]
    fn error_range_error() {
        assert_eq!(as_str(run(r#"
            const e = new RangeError("out of range");
            return e.name;
        "#)), "RangeError");
    }

    #[test]
    fn error_throw_catch() {
        assert_eq!(as_str(run(r#"
            let msg = "";
            try {
                throw new TypeError("caught me");
            } catch (e) {
                msg = e.message;
            }
            return msg;
        "#)), "caught me");
    }

    #[test]
    fn error_instanceof_check() {
        // instanceof neni implementovano, ale muzes overit name property
        assert_eq!(as_str(run(r#"
            try {
                throw new RangeError("r");
            } catch (e) {
                return e.name;
            }
        "#)), "RangeError");
    }

    #[test]
    fn error_no_message() {
        assert_eq!(as_str(run(r#"
            const e = new Error();
            return e.message;
        "#)), "");
    }
}
