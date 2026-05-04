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
use std::str::FromStr;
use crate::ast::{self, *};
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use bigdecimal::Zero;
use num_bigint::BigInt;
use num_bigint::Sign;
use num_traits::Zero as NumZero;
use num_traits::ToPrimitive as NumToPrimitive;
use num_traits::Pow;

// ─── Submoduly ────────────────────────────────────────────────────────────────

mod helpers;
mod builtins;
mod string_methods;
use helpers::*;
use builtins::setup_builtins;
use string_methods::call_string_method;

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
    /// BigNumber - arbitrary precision decimal cislo
    /// Sdilene pres Rc pro levne klonovani (BigDecimal je immutable).
    BigNumber(Rc<BigDecimal>),
    /// BigInt - arbitrary precision celociselny typ (nativni `42n` syntaxe)
    /// Sdileny pres Rc pro levne klonovani.
    BigInt(Rc<BigInt>),
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
    /// Async funkce (`async function`). Vraci vzdycky Promise.
    /// Vyjimka uvnitr = rejected Promise, return value = fulfilled Promise.
    Async {
        name: Option<String>,
        params: Vec<Param>,
        body: FuncBody,
        env: Rc<RefCell<Env>>,
    },
    /// Bound funkce - vysledek fn.bind(thisArg, ...args).
    /// Pri volani prepoji bound_this a prida bound_args pred call args.
    Bound {
        func: Box<JsValue>,
        bound_this: Box<JsValue>,
        bound_args: Vec<JsValue>,
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
            JsFunc::Async { name, .. }     => write!(f, "[AsyncFunction: {}]", name.as_deref().unwrap_or("anonymous")),
            JsFunc::Bound { .. }           => write!(f, "[BoundFunction]"),
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
            JsValue::BigNumber(n) => write!(f, "{n}"),
            JsValue::BigInt(n) => write!(f, "{n}"),
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
            JsValue::BigInt(n) => !NumZero::is_zero(n.as_ref()),
            JsValue::BigNumber(n) => !n.is_zero(),
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
            JsValue::BigNumber(n) => n.to_f64().unwrap_or(f64::NAN),
            JsValue::BigInt(n)    => n.to_f64().unwrap_or(f64::NAN),
            _                     => f64::NAN,
        }
    }

    pub fn type_of(&self) -> &'static str {
        match self {
            JsValue::Undefined    => "undefined",
            JsValue::Null         => "object",
            JsValue::Bool(_)      => "boolean",
            JsValue::Number(_)    => "number",
            JsValue::Str(_)       => "string",
            JsValue::Object(_)    => "object",
            JsValue::Array(_)     => "object",
            JsValue::Function(_)  => "function",
            JsValue::Map(_)       => "object",
            JsValue::Set(_)       => "object",
            JsValue::BigNumber(_) => "bignumber",
            JsValue::BigInt(_)    => "bigint",
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
            (JsValue::BigNumber(a), JsValue::BigNumber(b)) => *a == *b,
            (JsValue::BigInt(a),    JsValue::BigInt(b))    => *a == *b,
            _ => false,
        }
    }

    /// Vrati JsValue jako BigDecimal (pro BigNumber operace).
    /// Number -> BigDecimal, String -> parse, BigNumber -> klon, BigInt -> konverze
    pub fn to_bigdecimal(&self) -> Option<BigDecimal> {
        match self {
            JsValue::BigNumber(n) => Some((**n).clone()),
            JsValue::BigInt(n)    => Some(BigDecimal::from(n.as_ref().clone())),
            JsValue::Number(n) if n.is_finite() => {
                BigDecimal::from_str(&n.to_string()).ok()
            }
            JsValue::Str(s) => BigDecimal::from_str(s.trim()).ok(),
            JsValue::Bool(true)  => Some(BigDecimal::from(1)),
            JsValue::Bool(false) => Some(BigDecimal::from(0)),
            _ => None,
        }
    }

    /// Vrati JsValue jako BigInt (truncate pro Number, parse pro Str).
    /// Number -> BigInt (truncate na celou cast), BigInt -> klon, BigNumber -> truncate
    pub fn to_bigint(&self) -> Option<BigInt> {
        match self {
            JsValue::BigInt(n)    => Some((**n).clone()),
            JsValue::BigNumber(n) => {
                // BigDecimal::with_scale(0) zkopiruje, ale ceast neprijde - pouzij round/to_bigint pres string
                let s = n.with_scale(0).to_string();
                // Po with_scale(0) je to ".000" -> jen integer cast
                let int_str = s.split('.').next().unwrap_or("0");
                BigInt::from_str(int_str).ok()
            }
            JsValue::Number(n) if n.is_finite() => {
                // Truncate na cele cislo
                BigInt::from_str(&format!("{}", *n as i128)).ok()
            }
            JsValue::Str(s) => BigInt::from_str(s.trim()).ok(),
            JsValue::Bool(true)  => Some(BigInt::from(1)),
            JsValue::Bool(false) => Some(BigInt::from(0)),
            _ => None,
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
    /// Fronta timeru pro setTimeout/setInterval (id, callback, args)
    task_queue: Rc<RefCell<Vec<(u32, JsValue, Vec<JsValue>)>>>,
    /// Pocitadlo ID pro setTimeout/setInterval
    next_timer_id: Rc<RefCell<u32>>,
    /// Cache nactenych modulu: cesta -> namespace objekt s exporty
    /// Sdileny pres Rc, aby ho videly cizi/dynamicky importy.
    module_cache: Rc<RefCell<HashMap<String, JsValue>>>,
    /// Vetev pro testy / virtualni FS: source -> obsah
    /// Pokud je naplneno, importy se hledaji nejdrive zde.
    pub virtual_modules: Rc<RefCell<HashMap<String, String>>>,
    /// Aktualni "export" mapa - aktivni jen behem nacitani modulu.
    /// Stmt::Export prida do tohoto pole; po skonceni se konstruuje namespace.
    current_exports: Option<Rc<RefCell<HashMap<String, JsValue>>>>,
    /// Zakladni adresar pro resolve relativnich modulu (current dir nebo file dir).
    pub base_dir: Rc<RefCell<String>>,
}

// ─── Pomocne funkce ──────────────────────────────────────────────────────────


impl Interpreter {
    /// Vytvori novy interpreter s inicializovanymi vestavenymi objekty.
    pub fn new() -> Self {
        let global = Environment::new_global();
        let task_queue: Rc<RefCell<Vec<(u32, JsValue, Vec<JsValue>)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let next_timer_id: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
        setup_builtins(&global, &task_queue, &next_timer_id);
        Interpreter {
            global, yield_buffer: None, task_queue, next_timer_id,
            module_cache:    Rc::new(RefCell::new(HashMap::new())),
            virtual_modules: Rc::new(RefCell::new(HashMap::new())),
            current_exports: None,
            base_dir:        Rc::new(RefCell::new(".".to_string())),
        }
    }

    /// Prida virtualni modul (pro testy / sandboxing).
    /// `source` je klic, kterym se modul importuje.
    pub fn add_virtual_module(&self, source: &str, content: &str) {
        self.virtual_modules.borrow_mut().insert(source.to_string(), content.to_string());
    }

    /// Resolve & nacti modul podle source. Vraci namespace objekt (cachovany).
    fn load_module(&mut self, source: &str) -> EvalResult {
        // 1. Cache hit
        if let Some(ns) = self.module_cache.borrow().get(source).cloned() {
            return Ok(ns);
        }
        // 2. Nacti obsah - virtual modules priorita, pak FS
        let content = if let Some(c) = self.virtual_modules.borrow().get(source).cloned() {
            c
        } else {
            // FS: relativni cesty resolve proti base_dir
            let path = if source.starts_with("./") || source.starts_with("../") || source.starts_with('/') {
                let base = self.base_dir.borrow().clone();
                format!("{}/{}", base, source)
            } else {
                source.to_string()
            };
            std::fs::read_to_string(&path)
                .map_err(|e| JsError::Runtime(format!(
                    "ModuleError: nelze nacist modul '{source}' (cesta: {path}): {e}"
                )))?
        };

        // 3. Parse
        use crate::lexer::base::Lexer;
        use crate::parser::Parser;
        let lexer = Lexer::parse_str(&content, source)
            .map_err(|e| JsError::Runtime(format!("SyntaxError v modulu '{source}': {e}")))?;
        let tokens: Vec<_> = lexer.tokens.into_iter()
            .filter(|t| !matches!(t.kind,
                crate::tokens::TokenKind::Whitespace
                | crate::tokens::TokenKind::Newline
                | crate::tokens::TokenKind::CommentLine(_)
                | crate::tokens::TokenKind::CommentBlock(_)))
            .collect();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse()
            .map_err(|e| JsError::Runtime(format!("SyntaxError v modulu '{source}': {e}")))?;

        // 4. Spust v izolovanem env (s pristupem ke globalnim builtinum)
        let module_env = Environment::new_child(&self.global);
        let exports: Rc<RefCell<HashMap<String, JsValue>>> = Rc::new(RefCell::new(HashMap::new()));

        // Zaregistruj exports rezimu
        let prev_exports = self.current_exports.take();
        self.current_exports = Some(Rc::clone(&exports));

        // Pre-cache namespace placeholder (pro cyklicke importy)
        let ns_obj_rc = Rc::new(RefCell::new(JsObject::new()));
        let ns_value = JsValue::Object(Rc::clone(&ns_obj_rc));
        self.module_cache.borrow_mut().insert(source.to_string(), ns_value.clone());

        let exec_result = self.exec_stmts(&prog.body, &module_env);

        // Obnov predchozi exports
        self.current_exports = prev_exports;

        exec_result?;

        // 5. Naplni namespace objekt z exports mapy
        for (k, v) in exports.borrow().iter() {
            ns_obj_rc.borrow_mut().set(k.clone(), v.clone());
        }
        Ok(ns_value)
    }

    /// Spusti cely program (AST) a vrati posledni `return` hodnotu.
    ///
    /// Kdyz program neobsahuje `return`, vraci `JsValue::Undefined`.
    pub fn run(&mut self, program: &Program) -> EvalResult {
        let env = Rc::clone(&self.global);
        let result = match self.exec_stmts(&program.body, &env)? {
            Some(Signal::Return(v)) => v,
            _ => JsValue::Undefined,
        };
        // Drain timer queue - spust vsechny setTimeout callbacky
        self.drain_timers()?;
        Ok(result)
    }

    /// Spusti vsechny cekajici timer callbacky.
    fn drain_timers(&mut self) -> Result<(), JsError> {
        loop {
            let next = { self.task_queue.borrow().first().cloned() };
            match next {
                None => break,
                Some((_, cb, args)) => {
                    self.task_queue.borrow_mut().remove(0);
                    self.call_function(cb, args, None)?;
                }
            }
        }
        Ok(())
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

            // Async funkce: `async function name(params) { body }`
            Stmt::AsyncFunc { name, params, body } => {
                let func = JsValue::Function(JsFunc::Async {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: FuncBody::Stmts(body.clone()),
                    env: Rc::clone(env),
                });
                env.borrow_mut().define(name, func);
                Ok(None)
            }

            // Import: nacti modul a binduj specifiers do scope
            Stmt::Import { source, specifiers } => {
                let ns = self.load_module(source)?;
                let ns_obj = match &ns {
                    JsValue::Object(o) => Rc::clone(o),
                    _ => return Err(JsError::Runtime(
                        format!("ModuleError: modul '{source}' nevratil objekt")
                    )),
                };
                for spec in specifiers {
                    match spec {
                        ImportSpecifier::Default(local) => {
                            let v = ns_obj.borrow().props.get("default").cloned()
                                .unwrap_or(JsValue::Undefined);
                            env.borrow_mut().define(local, v);
                        }
                        ImportSpecifier::Named { imported, local } => {
                            let v = ns_obj.borrow().props.get(imported).cloned()
                                .unwrap_or(JsValue::Undefined);
                            env.borrow_mut().define(local, v);
                        }
                        ImportSpecifier::Namespace(local) => {
                            env.borrow_mut().define(local, JsValue::Object(Rc::clone(&ns_obj)));
                        }
                    }
                }
                Ok(None)
            }

            // Export: zaregistruje hodnotu do current_exports mapy.
            // Funguje jen pri nacitani modulu (current_exports = Some).
            Stmt::Export(kind) => {
                match kind {
                    ExportKind::Decl(decl) => {
                        // Spust deklaraci a pak najdi v env nove definovane jmeno(a)
                        let pre_keys: Vec<String> = env.borrow().vars.keys().cloned().collect();
                        self.exec_stmt(decl, env)?;
                        let post_keys: Vec<String> = env.borrow().vars.keys().cloned().collect();
                        if let Some(exports) = &self.current_exports {
                            for k in post_keys {
                                if !pre_keys.contains(&k) {
                                    let v = env.borrow().get(&k).unwrap_or(JsValue::Undefined);
                                    exports.borrow_mut().insert(k, v);
                                }
                            }
                        }
                    }
                    ExportKind::Default(expr) => {
                        let v = self.eval(expr, env)?;
                        if let Some(exports) = &self.current_exports {
                            exports.borrow_mut().insert("default".to_string(), v);
                        }
                    }
                    ExportKind::Named(pairs) => {
                        if let Some(exports) = &self.current_exports {
                            for (local, exported) in pairs {
                                let v = env.borrow().get(local).unwrap_or(JsValue::Undefined);
                                exports.borrow_mut().insert(exported.clone(), v);
                            }
                        }
                    }
                }
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
            Expr::BigInt(s)    => {
                let n = BigInt::from_str(s)
                    .map_err(|_| JsError::Runtime(format!("SyntaxError: invalid BigInt '{s}'")))?;
                Ok(JsValue::BigInt(Rc::new(n)))
            }
            Expr::DynamicImport(arg) => {
                // Vyhodnot specifier, nacti modul, vrat Promise.fulfilled(namespace).
                // V chybovem pripade vrat Promise.rejected(error).
                let spec = self.eval(arg, env)?;
                let source = spec.to_string();
                match self.load_module(&source) {
                    Ok(ns) => Ok(make_settled_promise("fulfilled", ns)),
                    Err(JsError::Runtime(msg)) => {
                        let mut err = JsObject::new();
                        err.set("name".into(),    JsValue::Str("Error".into()));
                        err.set("message".into(), JsValue::Str(msg));
                        Ok(make_settled_promise("rejected",
                            JsValue::Object(Rc::new(RefCell::new(err)))))
                    }
                    Err(e) => Err(e),
                }
            }
            Expr::Str(s)       => Ok(JsValue::Str(s.clone())),
            Expr::Bool(b)      => Ok(JsValue::Bool(*b)),
            Expr::Null         => Ok(JsValue::Null),
            Expr::Undefined    => Ok(JsValue::Undefined),
            Expr::Regex(p, f)  => Ok(make_regex_object(p, f)),

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

            // Async funkcni vyraz: `const f = async function() {}` nebo `async () => {}`
            Expr::AsyncFunc { name, params, body } => Ok(JsValue::Function(JsFunc::Async {
                name: name.clone(),
                params: params.clone(),
                body: FuncBody::Stmts(body.clone()),
                env: Rc::clone(env),
            })),

            // Await vyraz: `await promise` - synchronne rozbaluje Promise
            Expr::Await { value } => {
                let val = self.eval(value, env)?;
                // Rozbal promise pokud to je Promise
                match unwrap_promise_result(val) {
                    Ok(v) => Ok(v),
                    Err(reason) => Err(JsError::Thrown(reason)),
                }
            }

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
            UnaryOp::Minus  => {
                let v = self.eval(arg, env)?;
                match v {
                    JsValue::BigInt(n)    => Ok(JsValue::BigInt(Rc::new(-n.as_ref().clone()))),
                    JsValue::BigNumber(n) => Ok(JsValue::BigNumber(Rc::new(-n.as_ref().clone()))),
                    other => Ok(JsValue::Number(-other.to_number())),
                }
            }
            UnaryOp::Plus   => {
                // +bigint je TypeError v JS (nelze koercovat). Zde permisivni: vrat BigInt jako BigInt.
                let v = self.eval(arg, env)?;
                match v {
                    JsValue::BigInt(_) | JsValue::BigNumber(_) => Ok(v),
                    other => Ok(JsValue::Number(other.to_number())),
                }
            }
            UnaryOp::BitNot => {
                let v = self.eval(arg, env)?;
                match v {
                    JsValue::BigInt(n) => Ok(JsValue::BigInt(Rc::new(!n.as_ref().clone()))),
                    other => Ok(JsValue::Number(!(other.to_number() as i32) as f64)),
                }
            }
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

        // BigInt aritmetika: pokud aspon jeden operand je BigInt a ZADNY neni BigNumber,
        // proved operaci v BigInt presnosti. BigInt+BigNumber spada do BigNumber vetve nize.
        let has_bigint    = matches!(&l, JsValue::BigInt(_)) || matches!(&r, JsValue::BigInt(_));
        let has_bignumber = matches!(&l, JsValue::BigNumber(_)) || matches!(&r, JsValue::BigNumber(_));
        if has_bigint && !has_bignumber {
            if let (Some(la), Some(ra)) = (l.to_bigint(), r.to_bigint()) {
                match op {
                    BinaryOp::Add  => return Ok(JsValue::BigInt(Rc::new(la + ra))),
                    BinaryOp::Sub  => return Ok(JsValue::BigInt(Rc::new(la - ra))),
                    BinaryOp::Mul  => return Ok(JsValue::BigInt(Rc::new(la * ra))),
                    BinaryOp::Div  => {
                        if NumZero::is_zero(&ra) { return Err(JsError::Runtime("RangeError: deleni nulou v BigInt".into())); }
                        return Ok(JsValue::BigInt(Rc::new(la / ra)));
                    }
                    BinaryOp::Mod  => {
                        if NumZero::is_zero(&ra) { return Err(JsError::Runtime("RangeError: modulo nulou v BigInt".into())); }
                        return Ok(JsValue::BigInt(Rc::new(la % ra)));
                    }
                    BinaryOp::Exp  => {
                        // BigInt umocneni: exponent musi byt nezaporny
                        let exp = ra.to_u32().unwrap_or(0);
                        return Ok(JsValue::BigInt(Rc::new(la.pow(exp))));
                    }
                    BinaryOp::Lt   => return Ok(JsValue::Bool(la < ra)),
                    BinaryOp::Gt   => return Ok(JsValue::Bool(la > ra)),
                    BinaryOp::LtEq => return Ok(JsValue::Bool(la <= ra)),
                    BinaryOp::GtEq => return Ok(JsValue::Bool(la >= ra)),
                    BinaryOp::StrictEq    => {
                        // Strict eq vyzaduje stejny typ - BigInt vs Number neni striktne stejne
                        let same_type = matches!(&l, JsValue::BigInt(_)) && matches!(&r, JsValue::BigInt(_));
                        return Ok(JsValue::Bool(same_type && la == ra));
                    }
                    BinaryOp::StrictNotEq => {
                        let same_type = matches!(&l, JsValue::BigInt(_)) && matches!(&r, JsValue::BigInt(_));
                        return Ok(JsValue::Bool(!(same_type && la == ra)));
                    }
                    BinaryOp::Eq    => return Ok(JsValue::Bool(la == ra)),
                    BinaryOp::NotEq => return Ok(JsValue::Bool(la != ra)),
                    BinaryOp::BitAnd => return Ok(JsValue::BigInt(Rc::new(la & ra))),
                    BinaryOp::BitOr  => return Ok(JsValue::BigInt(Rc::new(la | ra))),
                    BinaryOp::BitXor => return Ok(JsValue::BigInt(Rc::new(la ^ ra))),
                    BinaryOp::Shl  => {
                        let shift = ra.to_i64().unwrap_or(0);
                        if shift >= 0 {
                            return Ok(JsValue::BigInt(Rc::new(la << shift as u32)));
                        } else {
                            return Ok(JsValue::BigInt(Rc::new(la >> (-shift) as u32)));
                        }
                    }
                    BinaryOp::Shr => {
                        let shift = ra.to_i64().unwrap_or(0);
                        if shift >= 0 {
                            return Ok(JsValue::BigInt(Rc::new(la >> shift as u32)));
                        } else {
                            return Ok(JsValue::BigInt(Rc::new(la << (-shift) as u32)));
                        }
                    }
                    _ => {} // Ostatni - pust dal
                }
            }
        }

        // BigNumber aritmetika: pokud aspon jeden operand je BigNumber,
        // preved oba na BigDecimal a proved operaci
        if matches!((&l, &r), (JsValue::BigNumber(_), _) | (_, JsValue::BigNumber(_))) {
            if let (Some(la), Some(ra)) = (l.to_bigdecimal(), r.to_bigdecimal()) {
                match op {
                    BinaryOp::Add  => return Ok(JsValue::BigNumber(Rc::new(la + ra))),
                    BinaryOp::Sub  => return Ok(JsValue::BigNumber(Rc::new(la - ra))),
                    BinaryOp::Mul  => return Ok(JsValue::BigNumber(Rc::new(la * ra))),
                    BinaryOp::Div  => {
                        if ra.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                        return Ok(JsValue::BigNumber(Rc::new(la / ra)));
                    }
                    BinaryOp::Mod  => {
                        if ra.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                        return Ok(JsValue::BigNumber(Rc::new(la % ra)));
                    }
                    BinaryOp::Exp  => {
                        let exp = ra.to_u64().unwrap_or(0);
                        return Ok(JsValue::BigNumber(Rc::new(bigdecimal_pow(la, exp))));
                    }
                    BinaryOp::Lt   => return Ok(JsValue::Bool(la < ra)),
                    BinaryOp::Gt   => return Ok(JsValue::Bool(la > ra)),
                    BinaryOp::LtEq => return Ok(JsValue::Bool(la <= ra)),
                    BinaryOp::GtEq => return Ok(JsValue::Bool(la >= ra)),
                    BinaryOp::StrictEq    => return Ok(JsValue::Bool(la == ra)),
                    BinaryOp::StrictNotEq => return Ok(JsValue::Bool(la != ra)),
                    BinaryOp::Eq    => return Ok(JsValue::Bool(la == ra)),
                    BinaryOp::NotEq => return Ok(JsValue::Bool(la != ra)),
                    _ => {} // Ostatni operace - pust dal jako cislo
                }
            }
        }

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
                    JsValue::Function(JsFunc::Native(name, _)) => name.clone(),
                    _ => return Ok(JsValue::Bool(false)),
                };
                if class_name.is_empty() { return Ok(JsValue::Bool(false)); }
                // Zkontroluj retezec trid ulozeny na instanci (pro tridy)
                // nebo typ intern. vlastnosti (pro vestavene typy)
                match &l {
                    JsValue::Object(o) => {
                        let obj = o.borrow();
                        // Tridy: __class_chain__
                        if let Some(JsValue::Str(chain)) = obj.props.get("__class_chain__") {
                            if chain.split(',').any(|n| n == class_name) {
                                return Ok(JsValue::Bool(true));
                            }
                        }
                        // Vestavene typy podle vnitrnich klicu
                        let result = match class_name.as_str() {
                            "Error" | "TypeError" | "RangeError" | "SyntaxError"
                            | "ReferenceError" | "URIError" | "EvalError" => {
                                // Error instance ma property "name"
                                if let Some(JsValue::Str(name)) = obj.props.get("name") {
                                    class_name == "Error"
                                        || name == &class_name
                                        || name.ends_with("Error")
                                } else { false }
                            }
                            "Date"    => obj.props.contains_key("__date_ms__"),
                            "RegExp"  => obj.props.contains_key("__regex_pattern__"),
                            "Promise" => obj.props.contains_key("__promise_state__"),
                            _ => false,
                        };
                        JsValue::Bool(result)
                    }
                    JsValue::Map(_) => JsValue::Bool(class_name == "Map"),
                    JsValue::Set(_) => JsValue::Bool(class_name == "Set"),
                    JsValue::Array(_) => JsValue::Bool(class_name == "Array"),
                    JsValue::Function(_) => JsValue::Bool(class_name == "Function"),
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
            // BigNumber vlastnosti (read-only)
            JsValue::BigNumber(bn) => {
                match key {
                    "s" | "sign" => return Ok(JsValue::Number(if bn.as_ref() < &BigDecimal::from(0) { -1.0 } else { 1.0 })),
                    _ => {}
                }
                Ok(JsValue::Undefined)
            }
            // BigInt vlastnosti (read-only)
            JsValue::BigInt(bn) => {
                match key {
                    "sign" => return Ok(JsValue::Number(match bn.sign() {
                        Sign::Minus => -1.0,
                        Sign::NoSign => 0.0,
                        Sign::Plus => 1.0,
                    })),
                    _ => {}
                }
                Ok(JsValue::Undefined)
            }
            // Native funkce: Number.XXX konstanty + Array.isArray atd.
            JsValue::Function(JsFunc::Native(fname, _)) => {
                match (fname.as_str(), key) {
                    ("Number", "MAX_VALUE")         => return Ok(JsValue::Number(f64::MAX)),
                    ("Number", "MIN_VALUE")         => return Ok(JsValue::Number(f64::MIN_POSITIVE)),
                    ("Number", "MAX_SAFE_INTEGER")  => return Ok(JsValue::Number(9007199254740991.0)),
                    ("Number", "MIN_SAFE_INTEGER")  => return Ok(JsValue::Number(-9007199254740991.0)),
                    ("Number", "POSITIVE_INFINITY") => return Ok(JsValue::Number(f64::INFINITY)),
                    ("Number", "NEGATIVE_INFINITY") => return Ok(JsValue::Number(f64::NEG_INFINITY)),
                    ("Number", "NaN")               => return Ok(JsValue::Number(f64::NAN)),
                    ("Number", "EPSILON")           => return Ok(JsValue::Number(f64::EPSILON)),
                    _ => {}
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

        // eval(src) - special case: potrebuje pristup k interpreteru a aktualnimu env
        if matches!(callee, Expr::Ident(n) if n == "eval") {
            let arg_vals = self.eval_args(args, env)?;
            let src = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
            return match src {
                JsValue::Str(s) => {
                    use crate::lexer::base::Lexer;
                    use crate::parser::Parser;
                    use crate::tokens::TokenKind;
                    let lexer = Lexer::parse_str(&s, "<eval>")
                        .map_err(|e| JsError::Runtime(format!("eval SyntaxError: {e}")))?;
                    let tokens: Vec<_> = lexer.tokens.into_iter()
                        .filter(|t| !matches!(t.kind,
                            TokenKind::Whitespace | TokenKind::Newline
                            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
                        .collect();
                    let mut parser = Parser::new(tokens);
                    let prog = parser.parse()
                        .map_err(|e| JsError::Runtime(format!("eval SyntaxError: {e}")))?;
                    // eval vraci completion value: posledni vyraz
                    // Pokud je program prazdny, vrat undefined
                    if prog.body.is_empty() {
                        return Ok(JsValue::Undefined);
                    }
                    // Spust vsechny prikazy krome posledniho
                    let last_idx = prog.body.len() - 1;
                    for stmt in &prog.body[..last_idx] {
                        match self.exec_stmt(stmt, env)? {
                            Some(Signal::Return(v)) => return Ok(v),
                            Some(sig) => return Err(JsError::Runtime(format!("eval: neocekavany signal {:?}", sig))),
                            None => {}
                        }
                    }
                    // Posledni prikaz: vyraz vraci hodnotu, jinak undefined
                    let last = &prog.body[last_idx];
                    match last {
                        crate::ast::Stmt::Expr(e) => self.eval(e, env),
                        other => {
                            match self.exec_stmt(other, env)? {
                                Some(Signal::Return(v)) => Ok(v),
                                _ => Ok(JsValue::Undefined),
                            }
                        }
                    }
                }
                other => Ok(other), // non-string preda as-is
            };
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

            // ─── Object.groupBy / Map.groupBy (ES2024) ───────────────────────
            // Detekce DRIVE nez specificke arms, protoze Map/Native have early return
            if key.as_str() == "groupBy" {
                if let Expr::Ident(name) = object.as_ref() {
                    if name == "Object" || name == "Map" {
                        let arg_vals = self.eval_args(args, env)?;
                        let mut iter = arg_vals.into_iter();
                        let items_val = iter.next().unwrap_or(JsValue::Undefined);
                        let cb = iter.next().unwrap_or(JsValue::Undefined);
                        let items = collect_iterable_values(&items_val);
                        if name == "Object" {
                            let groups_obj = JsObject::new();
                            let groups_rc = Rc::new(RefCell::new(groups_obj));
                            for (i, item) in items.into_iter().enumerate() {
                                let k = self.call_function(
                                    cb.clone(),
                                    vec![item.clone(), JsValue::Number(i as f64)],
                                    None,
                                )?;
                                let key_str = k.to_string();
                                let mut g = groups_rc.borrow_mut();
                                let bucket = g.props.entry(key_str)
                                    .or_insert_with(|| JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
                                if let JsValue::Array(a) = bucket {
                                    a.borrow_mut().push(item);
                                }
                            }
                            return Ok(JsValue::Object(groups_rc));
                        } else {
                            // Map.groupBy
                            let mut m = JsMap::new();
                            for (i, item) in items.into_iter().enumerate() {
                                let k = self.call_function(
                                    cb.clone(),
                                    vec![item.clone(), JsValue::Number(i as f64)],
                                    None,
                                )?;
                                let existing = m.get(&k);
                                let bucket = match existing {
                                    JsValue::Array(a) => a,
                                    _ => {
                                        let new_arr = Rc::new(RefCell::new(Vec::new()));
                                        m.set(k.clone(), JsValue::Array(Rc::clone(&new_arr)));
                                        new_arr
                                    }
                                };
                                bucket.borrow_mut().push(item);
                            }
                            return Ok(JsValue::Map(Rc::new(RefCell::new(m))));
                        }
                    }
                }
            }

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
                        // ─── ES2025 Set operace ─────────────────────────────
                        "union" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() { result.add(v); }
                            for v in other_vals { result.add(v); }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "intersection" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() {
                                if other_vals.iter().any(|x| JsMap::key_eq(x, &v)) {
                                    result.add(v);
                                }
                            }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "difference" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() {
                                if !other_vals.iter().any(|x| JsMap::key_eq(x, &v)) {
                                    result.add(v);
                                }
                            }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "symmetricDifference" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let mut result = JsSet::new();
                            for v in set_rc2.borrow().values.clone() {
                                if !other_vals.iter().any(|x| JsMap::key_eq(x, &v)) {
                                    result.add(v);
                                }
                            }
                            for v in other_vals {
                                if !set_rc2.borrow().has(&v) {
                                    result.add(v);
                                }
                            }
                            return Ok(JsValue::Set(Rc::new(RefCell::new(result))));
                        }
                        "isSubsetOf" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let result = set_rc2.borrow().values.iter().all(|v| {
                                other_vals.iter().any(|x| JsMap::key_eq(x, v))
                            });
                            return Ok(JsValue::Bool(result));
                        }
                        "isSupersetOf" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let result = other_vals.iter().all(|v| set_rc2.borrow().has(v));
                            return Ok(JsValue::Bool(result));
                        }
                        "isDisjointFrom" => {
                            let other = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            let other_vals = collect_iterable_values(&other);
                            let result = !set_rc2.borrow().values.iter().any(|v| {
                                other_vals.iter().any(|x| JsMap::key_eq(x, v))
                            });
                            return Ok(JsValue::Bool(result));
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
                    // ─── RegExp instance metody ───────────────────────────
                    if let Some((pat, flags)) = get_regex_parts(&JsValue::Object(Rc::clone(&obj_rc2))) {
                        match key.as_str() {
                            "test" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let text = arg_vals.into_iter().next()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                return Ok(JsValue::Bool(regex_test(&pat, &flags, &text)));
                            }
                            "exec" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let text = arg_vals.into_iter().next()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default();
                                match regex_exec(&pat, &flags, &text) {
                                    None => return Ok(JsValue::Null),
                                    Some(groups) => {
                                        let arr: Vec<JsValue> = groups.into_iter()
                                            .map(|g| g.map(JsValue::Str).unwrap_or(JsValue::Undefined))
                                            .collect();
                                        return Ok(JsValue::Array(Rc::new(RefCell::new(arr))));
                                    }
                                }
                            }
                            "toString" => {
                                return Ok(JsValue::Str(format!("/{pat}/{flags}")));
                            }
                            _ => {} // Pust dal
                        }
                    }
                    // ─── Promise instance metody ───────────────────────────
                    if let Some((state, pval)) = {
                        let b = obj_rc2.borrow();
                        b.props.get("__promise_state__").and_then(|s| {
                            if let JsValue::Str(st) = s {
                                let v = b.props.get("__promise_value__").cloned().unwrap_or(JsValue::Undefined);
                                Some((st.clone(), v))
                            } else { None }
                        })
                    } {
                        match key.as_str() {
                            "then" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let on_fulfilled = arg_vals.get(0).cloned().unwrap_or(JsValue::Undefined);
                                let on_rejected  = arg_vals.get(1).cloned().unwrap_or(JsValue::Undefined);
                                return match state.as_str() {
                                    "fulfilled" => {
                                        if matches!(on_fulfilled, JsValue::Function(_)) {
                                            match self.call_function(on_fulfilled, vec![pval], None) {
                                                Ok(r) => Ok(make_settled_promise("fulfilled",
                                                    unwrap_promise_result(r).unwrap_or_else(|v| v))),
                                                Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                                                Err(e) => Err(e),
                                            }
                                        } else {
                                            Ok(JsValue::Object(Rc::clone(&obj_rc2)))
                                        }
                                    }
                                    "rejected" => {
                                        if matches!(on_rejected, JsValue::Function(_)) {
                                            match self.call_function(on_rejected, vec![pval], None) {
                                                Ok(r) => Ok(make_settled_promise("fulfilled",
                                                    unwrap_promise_result(r).unwrap_or_else(|v| v))),
                                                Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                                                Err(e) => Err(e),
                                            }
                                        } else {
                                            Ok(JsValue::Object(Rc::clone(&obj_rc2)))
                                        }
                                    }
                                    _ => Ok(JsValue::Object(Rc::clone(&obj_rc2))),
                                };
                            }
                            "catch" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let on_rejected = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                return if state == "rejected" && matches!(on_rejected, JsValue::Function(_)) {
                                    match self.call_function(on_rejected, vec![pval], None) {
                                        Ok(r) => Ok(make_settled_promise("fulfilled",
                                            unwrap_promise_result(r).unwrap_or_else(|v| v))),
                                        Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                                        Err(e) => Err(e),
                                    }
                                } else {
                                    Ok(JsValue::Object(Rc::clone(&obj_rc2)))
                                };
                            }
                            "finally" => {
                                let arg_vals = self.eval_args(args, env)?;
                                let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                                if matches!(cb, JsValue::Function(_)) {
                                    match self.call_function(cb, vec![], None) {
                                        Err(JsError::Thrown(v)) => return Ok(make_settled_promise("rejected", v)),
                                        Err(e) => return Err(e),
                                        Ok(_) => {}
                                    }
                                }
                                return Ok(JsValue::Object(Rc::clone(&obj_rc2)));
                            }
                            _ => {} // Pust dal na normalni object dispatch
                        }
                        let _ = pval; // suppress unused warning
                        let _ = state;
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
                // ─── Number instance metody ────────────────────────────────
                JsValue::Number(n) => {
                    let n = *n;
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "toFixed" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            return Ok(JsValue::Str(format!("{:.prec$}", n, prec = digits)));
                        }
                        "toPrecision" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(1);
                            let s = format!("{:.prec$e}", n, prec = if digits > 0 { digits - 1 } else { 0 });
                            // Preved z Rust vedecke notace do JS formatu
                            return Ok(JsValue::Str(format!("{:.prec$}", n, prec = if digits > 0 { digits - 1 } else { 0 })));
                        }
                        "toExponential" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(6);
                            // Rust `{:e}` = "1.23e5", JS chce "1.23e+5"
                            let s = format!("{:.prec$e}", n, prec = digits);
                            // Pridame '+' pred kladny exponent
                            let s = if let Some(e_pos) = s.find('e') {
                                let (mantissa, exp_part) = s.split_at(e_pos);
                                let exp_str = &exp_part[1..]; // bez 'e'
                                if exp_str.starts_with('-') {
                                    format!("{}e{}", mantissa, exp_str)
                                } else {
                                    format!("{}e+{}", mantissa, exp_str)
                                }
                            } else { s };
                            return Ok(JsValue::Str(s));
                        }
                        "toString" => {
                            let radix = arg_vals.first().map(|v| v.to_number() as u32).unwrap_or(10);
                            if radix == 10 || radix == 0 {
                                return Ok(JsValue::Str(JsValue::Number(n).to_string()));
                            }
                            let radix = radix.min(36).max(2);
                            if n.fract() == 0.0 && n.is_finite() {
                                let i = n as i64;
                                return Ok(JsValue::Str(if i < 0 {
                                    format!("-{}", radix_string(-i as u64, radix))
                                } else {
                                    radix_string(i as u64, radix)
                                }));
                            }
                            return Ok(JsValue::Str(n.to_string()));
                        }
                        "valueOf" => return Ok(JsValue::Number(n)),
                        "toLocaleString" => {
                            return Ok(JsValue::Str(format_number_locale(n)));
                        }
                        _ => {}
                    }
                }
                // ─── BigNumber instance metody ────────────────────────────
                JsValue::BigNumber(bn) => {
                    let bn = Rc::clone(bn);
                    let arg_vals = self.eval_args(args, env)?;
                    let other_bd = arg_vals.first().and_then(|v| v.to_bigdecimal());
                    return match key.as_str() {
                        "plus"      => Ok(JsValue::BigNumber(Rc::new((*bn).clone() + other_bd.unwrap_or(BigDecimal::from(0))))),
                        "minus"     => Ok(JsValue::BigNumber(Rc::new((*bn).clone() - other_bd.unwrap_or(BigDecimal::from(0))))),
                        "times"     => Ok(JsValue::BigNumber(Rc::new((*bn).clone() * other_bd.unwrap_or(BigDecimal::from(1))))),
                        "multipliedBy" => Ok(JsValue::BigNumber(Rc::new((*bn).clone() * other_bd.unwrap_or(BigDecimal::from(1))))),
                        "div" | "dividedBy" => {
                            let d = other_bd.unwrap_or(BigDecimal::from(1));
                            if d.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                            Ok(JsValue::BigNumber(Rc::new(bn.as_ref().clone() / d)))
                        }
                        "mod" | "modulo" => {
                            let d = other_bd.unwrap_or(BigDecimal::from(1));
                            if d.is_zero() { return Ok(JsValue::Number(f64::NAN)); }
                            Ok(JsValue::BigNumber(Rc::new(bn.as_ref().clone() % d)))
                        }
                        "pow" | "exponentiatedBy" => {
                            let exp = other_bd.and_then(|d| d.to_u64()).unwrap_or(0);
                            Ok(JsValue::BigNumber(Rc::new(bigdecimal_pow(bn.as_ref().clone(), exp))))
                        }
                        "abs"       => Ok(JsValue::BigNumber(Rc::new(bn.abs()))),
                        "negated"   => Ok(JsValue::BigNumber(Rc::new(-bn.as_ref().clone()))),
                        "sqrt"      => Ok(JsValue::BigNumber(Rc::new(bn.sqrt().unwrap_or(BigDecimal::from(0))))),
                        "toNumber"  => Ok(JsValue::Number(bn.to_f64().unwrap_or(f64::NAN))),
                        "toString"  => Ok(JsValue::Str(bn.to_string())),
                        "toFixed"   => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            Ok(JsValue::Str(bn.round(digits as i64).to_string()))
                        }
                        "toPrecision" => {
                            let digits = arg_vals.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            Ok(JsValue::Str(bn.round(digits as i64).to_string()))
                        }
                        "isZero"     => Ok(JsValue::Bool(bn.is_zero())),
                        "isPositive" => Ok(JsValue::Bool(*bn > BigDecimal::from(0))),
                        "isNegative" => Ok(JsValue::Bool(*bn < BigDecimal::from(0))),
                        "isFinite"   => Ok(JsValue::Bool(true)),
                        "isNaN"      => Ok(JsValue::Bool(false)),
                        "isInteger"  => Ok(JsValue::Bool(bn.is_integer())),
                        "gt" | "isGreaterThan"           => Ok(JsValue::Bool((*bn) > other_bd.unwrap_or(BigDecimal::from(0)))),
                        "gte" | "isGreaterThanOrEqualTo" => Ok(JsValue::Bool((*bn) >= other_bd.unwrap_or(BigDecimal::from(0)))),
                        "lt" | "isLessThan"              => Ok(JsValue::Bool((*bn) < other_bd.unwrap_or(BigDecimal::from(0)))),
                        "lte" | "isLessThanOrEqualTo"    => Ok(JsValue::Bool((*bn) <= other_bd.unwrap_or(BigDecimal::from(0)))),
                        "eq" | "isEqualTo"               => Ok(JsValue::Bool((*bn) == other_bd.unwrap_or(BigDecimal::from(0)))),
                        "comparedTo" => {
                            let other = other_bd.unwrap_or(BigDecimal::from(0));
                            let cmp = if *bn < other { -1.0 } else if *bn > other { 1.0 } else { 0.0 };
                            Ok(JsValue::Number(cmp))
                        }
                        "decimalPlaces" | "dp" => {
                            let s = bn.to_string();
                            let dp = s.find('.').map(|i| s.len() - i - 1).unwrap_or(0);
                            Ok(JsValue::Number(dp as f64))
                        }
                        "integerValue" => Ok(JsValue::BigNumber(Rc::new(bn.round(0)))),
                        "shiftedBy" => {
                            let n = arg_vals.first().map(|v| v.to_number() as i64).unwrap_or(0);
                            let factor = bigdecimal_pow(BigDecimal::from(10i64), n.unsigned_abs());
                            let result = if n >= 0 { bn.as_ref().clone() * factor } else { bn.as_ref().clone() / factor };
                            Ok(JsValue::BigNumber(Rc::new(result)))
                        }
                        "valueOf" => Ok(JsValue::Number(bn.to_f64().unwrap_or(f64::NAN))),
                        _ => Ok(JsValue::Undefined),
                    };
                }
                // ─── BigInt instance metody ────────────────────────────────
                JsValue::BigInt(bn) => {
                    let bn = Rc::clone(bn);
                    let arg_vals = self.eval_args(args, env)?;
                    return match key.as_str() {
                        "toString" => {
                            let radix = arg_vals.first().map(|v| v.to_number() as u32).unwrap_or(10);
                            let radix = radix.clamp(2, 36);
                            Ok(JsValue::Str(bn.to_str_radix(radix)))
                        }
                        "toLocaleString" => Ok(JsValue::Str(bn.to_string())),
                        "valueOf" => Ok(JsValue::BigInt(bn)),
                        _ => Ok(JsValue::Undefined),
                    };
                }
                JsValue::Str(s) => {
                    let s = s.clone();
                    let arg_vals = self.eval_args(args, env)?;
                    if let Some(result) = call_string_method(&s, &key, arg_vals)? {
                        return Ok(result);
                    }
                }
                // Function.prototype.call / apply / bind
                JsValue::Function(_) if matches!(key.as_str(), "call" | "apply" | "bind") => {
                    let arg_vals = self.eval_args(args, env)?;
                    match key.as_str() {
                        "call" => {
                            // fn.call(thisArg, arg1, arg2, ...)
                            let this_arg = arg_vals.first().cloned();
                            let call_args = arg_vals.into_iter().skip(1).collect();
                            return self.call_function(this.clone(), call_args, this_arg);
                        }
                        "apply" => {
                            // fn.apply(thisArg, [arg1, arg2, ...])
                            let this_arg = arg_vals.first().cloned();
                            let call_args = match arg_vals.get(1) {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            return self.call_function(this.clone(), call_args, this_arg);
                        }
                        "bind" => {
                            // fn.bind(thisArg, ...boundArgs) -> nova JsFunc::Bound
                            let bound_this = arg_vals.first().cloned().unwrap_or(JsValue::Undefined);
                            let bound_args: Vec<JsValue> = arg_vals.into_iter().skip(1).collect();
                            return Ok(JsValue::Function(JsFunc::Bound {
                                func: Box::new(this.clone()),
                                bound_this: Box::new(bound_this),
                                bound_args,
                            }));
                        }
                        _ => unreachable!()
                    }
                }
                // Array/Number/Date/Promise staticke metody
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
                                JsValue::Set(s) => JsValue::Array(Rc::new(RefCell::new(s.borrow().values.clone()))),
                                JsValue::Map(m) => {
                                    let entries: Vec<JsValue> = m.borrow().entries.iter()
                                        .map(|(k, v)| JsValue::Array(Rc::new(RefCell::new(vec![k.clone(), v.clone()]))))
                                        .collect();
                                    JsValue::Array(Rc::new(RefCell::new(entries)))
                                }
                                _ => JsValue::Array(Rc::new(RefCell::new(vec![]))),
                            };
                            return Ok(result);
                        }
                        ("Array", "of") => {
                            return Ok(JsValue::Array(Rc::new(RefCell::new(arg_vals))));
                        }
                        // Number staticke metody
                        ("Number", "isInteger") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.fract() == 0.0 && n.is_finite())));
                        }
                        ("Number", "isFinite") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.is_finite())));
                        }
                        ("Number", "isNaN") => {
                            return Ok(JsValue::Bool(matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.is_nan())));
                        }
                        ("Number", "isSafeInteger") => {
                            let ok = matches!(arg_vals.first(),
                                Some(JsValue::Number(n)) if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0);
                            return Ok(JsValue::Bool(ok));
                        }
                        ("Number", "parseInt") => {
                            let s = arg_vals.first().map(|v| v.to_string()).unwrap_or_default();
                            let radix = arg_vals.get(1).map(|v| v.to_number() as u32).unwrap_or(10);
                            let radix = if radix == 0 { 10 } else { radix.min(36) };
                            return Ok(JsValue::Number(
                                i64::from_str_radix(s.trim(), radix)
                                    .map(|n| n as f64).unwrap_or(f64::NAN)
                            ));
                        }
                        ("Number", "parseFloat") => {
                            let s = arg_vals.first().map(|v| v.to_string()).unwrap_or_default();
                            return Ok(JsValue::Number(s.trim().parse::<f64>().unwrap_or(f64::NAN)));
                        }
                        // Date staticke metody
                        ("Date", "now") => return Ok(JsValue::Number(now_ms())),
                        ("Date", "parse") => {
                            // Stub - vratime NaN pro neznamy format
                            return Ok(JsValue::Number(f64::NAN));
                        }
                        // Promise staticke metody
                        ("Promise", "resolve") => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(make_settled_promise("fulfilled", v));
                        }
                        ("Promise", "reject") => {
                            let v = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            return Ok(make_settled_promise("rejected", v));
                        }
                        ("Promise", "all") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            let mut results = Vec::new();
                            for item in arr {
                                match get_promise_state(&item) {
                                    Some((s, v)) if s == "rejected" => {
                                        return Ok(make_settled_promise("rejected", v));
                                    }
                                    Some((_, v)) => results.push(v),
                                    None => results.push(item),
                                }
                            }
                            return Ok(make_settled_promise("fulfilled",
                                JsValue::Array(Rc::new(RefCell::new(results)))));
                        }
                        ("Promise", "allSettled") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            let results: Vec<JsValue> = arr.into_iter().map(|item| {
                                let (status, value) = match get_promise_state(&item) {
                                    Some((s, v)) => (s, v),
                                    None => ("fulfilled".into(), item),
                                };
                                let mut o = JsObject::new();
                                o.set("status".into(), JsValue::Str(status.clone()));
                                if status == "fulfilled" {
                                    o.set("value".into(), value);
                                } else {
                                    o.set("reason".into(), value);
                                }
                                JsValue::Object(Rc::new(RefCell::new(o)))
                            }).collect();
                            return Ok(make_settled_promise("fulfilled",
                                JsValue::Array(Rc::new(RefCell::new(results)))));
                        }
                        ("Promise", "race") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            for item in arr {
                                match get_promise_state(&item) {
                                    Some((s, v)) if s == "fulfilled" || s == "rejected" => {
                                        return Ok(make_settled_promise(&s, v));
                                    }
                                    _ => {}
                                }
                            }
                            return Ok(make_settled_promise("pending", JsValue::Undefined));
                        }
                        // String staticke metody
                        ("String", "fromCharCode") => {
                            let s: String = arg_vals.iter()
                                .map(|v| {
                                    let code = v.to_number() as u32;
                                    char::from_u32(code).unwrap_or('\u{FFFD}')
                                })
                                .collect();
                            return Ok(JsValue::Str(s));
                        }
                        ("String", "fromCodePoint") => {
                            let s: String = arg_vals.iter()
                                .map(|v| {
                                    let code = v.to_number() as u32;
                                    char::from_u32(code).unwrap_or('\u{FFFD}')
                                })
                                .collect();
                            return Ok(JsValue::Str(s));
                        }
                        ("Promise", "withResolvers") => {
                            // ES2024: { promise, resolve, reject }
                            // V nasi sync implementaci pouzivame stav v sdilenem RefCell
                            let state: Rc<RefCell<(String, JsValue)>> =
                                Rc::new(RefCell::new(("pending".to_string(), JsValue::Undefined)));
                            let promise_obj_rc = Rc::new(RefCell::new(JsObject::new()));
                            promise_obj_rc.borrow_mut().set("__promise_state__".into(), JsValue::Str("pending".into()));
                            promise_obj_rc.borrow_mut().set("__promise_value__".into(), JsValue::Undefined);
                            let promise_val = JsValue::Object(Rc::clone(&promise_obj_rc));

                            let p1 = Rc::clone(&promise_obj_rc);
                            let s1 = Rc::clone(&state);
                            let resolve_fn = native("resolve", move |a| {
                                let v = a.into_iter().next().unwrap_or(JsValue::Undefined);
                                if s1.borrow().0 == "pending" {
                                    *s1.borrow_mut() = ("fulfilled".into(), v.clone());
                                    p1.borrow_mut().set("__promise_state__".into(), JsValue::Str("fulfilled".into()));
                                    p1.borrow_mut().set("__promise_value__".into(), v);
                                }
                                Ok(JsValue::Undefined)
                            });
                            let p2 = Rc::clone(&promise_obj_rc);
                            let s2 = Rc::clone(&state);
                            let reject_fn = native("reject", move |a| {
                                let v = a.into_iter().next().unwrap_or(JsValue::Undefined);
                                if s2.borrow().0 == "pending" {
                                    *s2.borrow_mut() = ("rejected".into(), v.clone());
                                    p2.borrow_mut().set("__promise_state__".into(), JsValue::Str("rejected".into()));
                                    p2.borrow_mut().set("__promise_value__".into(), v);
                                }
                                Ok(JsValue::Undefined)
                            });

                            let mut result = JsObject::new();
                            result.set("promise".into(), promise_val);
                            result.set("resolve".into(), resolve_fn);
                            result.set("reject".into(),  reject_fn);
                            return Ok(JsValue::Object(Rc::new(RefCell::new(result))));
                        }
                        ("Promise", "try") => {
                            // ES2025: zavola callback synchronne, zabali vysledek do Promise
                            let cb = arg_vals.into_iter().next().unwrap_or(JsValue::Undefined);
                            match self.call_function(cb, vec![], None) {
                                Ok(v) => {
                                    if get_promise_state(&v).is_some() {
                                        return Ok(v);
                                    }
                                    return Ok(make_settled_promise("fulfilled", v));
                                }
                                Err(JsError::Thrown(v)) => return Ok(make_settled_promise("rejected", v)),
                                Err(e) => return Err(e),
                            }
                        }
                        ("Promise", "any") => {
                            let arr = match arg_vals.into_iter().next() {
                                Some(JsValue::Array(a)) => a.borrow().clone(),
                                _ => vec![],
                            };
                            let mut errors = Vec::new();
                            for item in arr {
                                match get_promise_state(&item) {
                                    Some((s, v)) if s == "fulfilled" => {
                                        return Ok(make_settled_promise("fulfilled", v));
                                    }
                                    Some((_, v)) => errors.push(v),
                                    None => return Ok(make_settled_promise("fulfilled", item)),
                                }
                            }
                            let mut agg = JsObject::new();
                            agg.set("name".into(), JsValue::Str("AggregateError".into()));
                            agg.set("message".into(), JsValue::Str("All promises were rejected".into()));
                            agg.set("errors".into(), JsValue::Array(Rc::new(RefCell::new(errors))));
                            return Ok(make_settled_promise("rejected", JsValue::Object(Rc::new(RefCell::new(agg)))));
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
            // Async funkce: spust synchronne, zabal vysledek do Promise
            JsValue::Function(JsFunc::Async { params, body, env, .. }) => {
                let call_env = Environment::new_child(&env);
                self.bind_params(&params, args.clone(), &call_env)?;
                if let Some(t) = this { call_env.borrow_mut().define("this", t); }
                let args_arr = JsValue::Array(Rc::new(RefCell::new(args)));
                call_env.borrow_mut().define("arguments", args_arr);
                match match &body {
                    FuncBody::Stmts(stmts) => {
                        let stmts = stmts.clone();
                        self.exec_stmts(&stmts, &call_env)
                            .map(|s| match s { Some(Signal::Return(v)) => v, _ => JsValue::Undefined })
                    }
                    FuncBody::Expr(e) => {
                        let e = e.clone();
                        self.eval(&e, &call_env)
                    }
                } {
                    Ok(v) => {
                        // Pokud return value je uz Promise, vrat ho primo
                        if get_promise_state(&v).is_some() {
                            Ok(v)
                        } else {
                            Ok(make_settled_promise("fulfilled", v))
                        }
                    }
                    Err(JsError::Thrown(v)) => Ok(make_settled_promise("rejected", v)),
                    Err(e) => Err(e),
                }
            }
            // Bound funkce: prepend bound_args, pouzij bound_this
            JsValue::Function(JsFunc::Bound { func, bound_this, bound_args }) => {
                let mut all_args = bound_args.clone();
                all_args.extend(args);
                let effective_this = this.or(Some(*bound_this));
                self.call_function(*func, all_args, effective_this)
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
                "Promise"         => return self.construct_promise(args),
                "RegExp"          => {
                    let pat = args.get(0).map(|v| v.to_string()).unwrap_or_default();
                    let flags = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                    return Ok(make_regex_object(&pat, &flags));
                }
                "BigNumber"       => {
                    let s = args.into_iter().next().map(|v| match v {
                        JsValue::BigNumber(n) => n.to_string(),
                        other => other.to_string(),
                    }).unwrap_or_else(|| "0".into());
                    return BigDecimal::from_str(s.trim())
                        .map(|bd| JsValue::BigNumber(Rc::new(bd)))
                        .map_err(|_| JsError::Runtime(format!("BigNumber: neplatna hodnota '{s}'")));
                }
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
    /// ES2022: druhy argument je options objekt s `cause`.
    fn construct_error(&mut self, name: String, args: Vec<JsValue>) -> EvalResult {
        let mut iter = args.into_iter();
        let msg = iter.next().map(|v| v.to_string()).unwrap_or_default();
        let options = iter.next();
        let mut obj = JsObject::new();
        obj.set("name".into(),    JsValue::Str(name.clone()));
        obj.set("message".into(), JsValue::Str(msg.clone()));
        obj.set("stack".into(),   JsValue::Str(format!("{name}: {msg}")));
        // ES2022 Error.cause: pokud options.cause existuje, uloz
        if let Some(JsValue::Object(opts)) = options {
            let cause = opts.borrow().props.get("cause").cloned();
            if let Some(c) = cause {
                obj.set("cause".into(), c);
            }
        }
        Ok(JsValue::Object(Rc::new(RefCell::new(obj))))
    }

    /// Konstruktor `new Promise(executor)` - synchronni rozliseni.
    ///
    /// Executor je volan okamzite se dvema argumenty:
    /// - `resolve(value)` - splni promise
    /// - `reject(reason)` - odmitne promise
    fn construct_promise(&mut self, args: Vec<JsValue>) -> EvalResult {
        let mut obj = JsObject::new();
        obj.set("__promise_state__".into(), JsValue::Str("pending".into()));
        obj.set("__promise_value__".into(), JsValue::Undefined);
        let obj_rc = Rc::new(RefCell::new(obj));

        let executor = args.into_iter().next().unwrap_or(JsValue::Undefined);
        if matches!(executor, JsValue::Function(_)) {
            // Vytvor resolve/reject closures ktere zachyti Rc<RefCell<JsObject>>
            let obj_rc_r = Rc::clone(&obj_rc);
            let resolve = native("resolve", move |a| {
                let val = a.into_iter().next().unwrap_or(JsValue::Undefined);
                let mut o = obj_rc_r.borrow_mut();
                if matches!(o.props.get("__promise_state__"), Some(JsValue::Str(s)) if s == "pending") {
                    o.set("__promise_state__".into(), JsValue::Str("fulfilled".into()));
                    o.set("__promise_value__".into(), val);
                }
                Ok(JsValue::Undefined)
            });
            let obj_rc_j = Rc::clone(&obj_rc);
            let reject = native("reject", move |a| {
                let val = a.into_iter().next().unwrap_or(JsValue::Undefined);
                let mut o = obj_rc_j.borrow_mut();
                if matches!(o.props.get("__promise_state__"), Some(JsValue::Str(s)) if s == "pending") {
                    o.set("__promise_state__".into(), JsValue::Str("rejected".into()));
                    o.set("__promise_value__".into(), val);
                }
                Ok(JsValue::Undefined)
            });

            // Spust executor - chyba = reject
            match self.call_function(executor, vec![resolve, reject], None) {
                Ok(_) => {}
                Err(JsError::Thrown(v)) => {
                    let mut o = obj_rc.borrow_mut();
                    if matches!(o.props.get("__promise_state__"), Some(JsValue::Str(s)) if s == "pending") {
                        o.set("__promise_state__".into(), JsValue::Str("rejected".into()));
                        o.set("__promise_value__".into(), v);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(JsValue::Object(obj_rc))
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
            "toLocaleString" => {
                // Kazdy prvek prevede pres toLocaleString (nebo toString)
                let s = arr.borrow().iter().map(|v| match v {
                    JsValue::Number(n) => format_number_locale(*n),
                    JsValue::Undefined | JsValue::Null => String::new(),
                    other => other.to_string(),
                }).collect::<Vec<_>>().join(",");
                Ok(Some(JsValue::Str(s)))
            }
            // ─── ES2023 immutable varianty ─────────────────────────────────
            "toSorted" => {
                // Vraci NOVE pole, neupravuje original
                let mut copy: Vec<JsValue> = arr.borrow().clone();
                if args.is_empty() {
                    copy.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                } else {
                    let cb = args.into_iter().next().unwrap();
                    let n = copy.len();
                    for i in 0..n {
                        for j in 0..n-1-i {
                            match self.call_function(cb.clone(), vec![copy[j].clone(), copy[j+1].clone()], None) {
                                Ok(v) if v.to_number() > 0.0 => copy.swap(j, j+1),
                                Err(e) => return Err(e),
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(copy)))))
            }
            "toReversed" => {
                let mut copy: Vec<JsValue> = arr.borrow().clone();
                copy.reverse();
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(copy)))))
            }
            "toSpliced" => {
                // toSpliced(start, deleteCount, ...items) - immutable splice
                let len = arr.borrow().len() as i64;
                let start = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                let s = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
                let delete_count = args.get(1).map(|v| v.to_number() as usize)
                    .unwrap_or(arr.borrow().len() - s);
                let mut copy: Vec<JsValue> = arr.borrow().clone();
                let end = (s + delete_count).min(copy.len());
                copy.drain(s..end);
                let inserts: Vec<JsValue> = if args.len() > 2 { args[2..].to_vec() } else { vec![] };
                for (i, v) in inserts.into_iter().enumerate() { copy.insert(s + i, v); }
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(copy)))))
            }
            "with" => {
                // with(index, value) - immutable [index] = value
                let len = arr.borrow().len() as i64;
                let idx = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                let real = if idx < 0 { len + idx } else { idx };
                if real < 0 || real >= len {
                    return Err(JsError::Runtime(format!("RangeError: index {idx} mimo rozsah")));
                }
                let val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut copy: Vec<JsValue> = arr.borrow().clone();
                copy[real as usize] = val;
                Ok(Some(JsValue::Array(Rc::new(RefCell::new(copy)))))
            }
            // ─── ES2023 findLast / findLastIndex ──────────────────────────
            "findLast" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.iter().enumerate().rev() {
                    if self.call_function(cb.clone(), vec![v.clone(), JsValue::Number(i as f64)], None)?.is_truthy() {
                        return Ok(Some(v.clone()));
                    }
                }
                Ok(Some(JsValue::Undefined))
            }
            "findLastIndex" => {
                let cb = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let items: Vec<JsValue> = arr.borrow().clone();
                for (i, v) in items.iter().enumerate().rev() {
                    if self.call_function(cb.clone(), vec![v.clone(), JsValue::Number(i as f64)], None)?.is_truthy() {
                        return Ok(Some(JsValue::Number(i as f64)));
                    }
                }
                Ok(Some(JsValue::Number(-1.0)))
            }
            // ES2023 copyWithin (mutating)
            "copyWithin" => {
                let len = arr.borrow().len() as i64;
                let target = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                let start  = args.get(1).map(|v| v.to_number() as i64).unwrap_or(0);
                let end    = args.get(2).map(|v| v.to_number() as i64).unwrap_or(len);
                let t = if target < 0 { (len + target).max(0) } else { target.min(len) } as usize;
                let s = if start  < 0 { (len + start ).max(0) } else { start .min(len) } as usize;
                let e = if end    < 0 { (len + end   ).max(0) } else { end   .min(len) } as usize;
                let count = (e.saturating_sub(s)).min(arr.borrow().len().saturating_sub(t));
                let segment: Vec<JsValue> = arr.borrow()[s..s+count].to_vec();
                for i in 0..count {
                    arr.borrow_mut()[t + i] = segment[i].clone();
                }
                Ok(Some(JsValue::Array(arr)))
            }
            _ => Ok(None), // neni znama array metoda -> zkus get_prop
        }
    }
}



#[cfg(test)]
mod tests;
