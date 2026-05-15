/// Interpreter JavaScriptu pro podmnozinu ESNext.
///
/// # Architektura
///
/// Interpreter prochazi AST (Abstract Syntax Tree) a vyhodnocuje jednotlive uzly.
/// Stav programu je udrzovan v retezci `Environment` scopes.
///
/// ## Pipeline
/// ```text
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
use std::rc::{Rc, Weak};
use std::str::FromStr;
use crate::ast::*;
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use bigdecimal::Zero;
use num_bigint::BigInt;
use num_bigint::Sign;
use num_traits::Zero as NumZero;
use num_traits::Pow;

// ─── Submoduly ────────────────────────────────────────────────────────────────

pub mod helpers;
mod builtins;
mod builtins_helpers;
mod string_methods;
pub(crate) mod webgl;
pub(crate) mod canvas;
pub(crate) mod serialize;
pub(crate) mod dom_props;
pub mod bytecode;
mod js_value_impl;
mod builtins_reflect;
mod builtins_atomics;
mod builtins_temporal;
mod eval_member;
mod eval_call;
mod eval_expr;
mod exec_stmt;
mod class;
mod call_machinery;
#[allow(unused_imports)] // WebGLProgram je expose jen pro testy (cargo build je nevidi)
pub(crate) use webgl::{WebGLState, WebGLProgram, WebGLDrawCmd, WebGLAttribSlot, WebGLUniformValue, UniformSlot, UniformSlotKind};
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
    /// DOM uzel - real reference do browser::dom tree.
    /// Sdileny pres Rc s rodicovskym/detskym tree.
    DomNode(Rc<crate::browser::dom::Node>),
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
    pub fn new() -> Self {
        JsObject { props: HashMap::new(), proto: None, frozen: false }
    }

    /// Vytvori objekt s danym prototypem (Object.create(proto)).
    pub fn new_with_proto(proto: Rc<RefCell<JsObject>>) -> Self {
        JsObject { props: HashMap::new(), proto: Some(proto), frozen: false }
    }

    /// Cte vlastnost - prochazi prototypovym retezcem (max 100 uroven).
    pub fn get(&self, k: &str) -> JsValue {
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
    pub fn has_own(&self, k: &str) -> bool {
        self.props.contains_key(k)
    }

    /// Nastavi vlastnost. Frozen objekt zmeny ignoruje.
    pub fn set(&mut self, k: String, v: JsValue) {
        if self.frozen { return; }
        self.props.insert(k, v);
    }

    /// Vrati serazeny seznam vlastnich klicu (bez internich `__key__` klicu).
    pub fn own_keys(&self) -> Vec<String> {
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
    },
    /// Bytecode-VM compiled funkce. Pri volani spousti VM s body.
    /// Closure scope sdileny do native fn pro outer var lookup.
    VmCompiled {
        name: Option<String>,
        compiled: Rc<bytecode::CompiledFunction>,
        /// Closure env z misto definice (pro outer var lookup z VM).
        env: Rc<RefCell<Env>>,
        /// Closure captures - hodnoty volnych promennych z outer scope at
        /// LoadFunction time. Indexovane podle CompiledFunction.captures_outer_indices.
        captures: Vec<JsValue>,
    },
}

#[derive(Debug, Clone)]
pub enum FuncBody {
    Stmts(Vec<Stmt>),
    Expr(Box<Expr>),
}

// Display + Debug + impl JsValue extracted to js_value_impl.rs.

// ─── Environment (scope) ──────────────────────────────────────────────────────

type Env = Environment;

/// Lexikalni scope (prostredi promennych).
///
/// Implementuje retezec scopes: kazdy scope ma volitelny `parent`.
/// Vyhledavani promenne jde od nejhlubsiho scope ke globalnimu.
///
/// # Priklad retezce
/// ```text
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
    /// Seznam definovanych jmen v tomto scopu (bez parent walk).
    pub fn names(&self) -> Vec<String> {
        self.vars.keys().cloned().collect()
    }

    /// Vraci parent scope (None u global).
    pub fn parent_chain(&self) -> Option<Rc<RefCell<Environment>>> {
        self.parent.clone()
    }

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
    /// Iterativni misto rekurze - vyhne se borrow chain overhead.
    pub fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        // Iterate parent chain bez rekurze.
        let mut cur = self.parent.clone();
        while let Some(env_rc) = cur {
            let env = env_rc.borrow();
            if let Some(v) = env.vars.get(name) {
                return Some(v.clone());
            }
            cur = env.parent.clone();
        }
        None
    }

    /// Prirazuje hodnotu existujici promenne (hleda ji v retezci scopu).
    ///
    /// Vraci `true` kdyz promennou nasla a zmenila,
    /// `false` kdyz promenna neexistuje (volajici pak muze rozhodnout co delat).
    pub fn set(&mut self, name: &str, val: JsValue) -> bool {
        // Self scope first.
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), val);
            return true;
        }
        // Iterativni walk parent chain.
        let mut cur = self.parent.clone();
        while let Some(env_rc) = cur {
            // Try contains_key first (cheap immutable borrow), then mutable.
            let has = env_rc.borrow().vars.contains_key(name);
            if has {
                env_rc.borrow_mut().vars.insert(name.to_string(), val);
                return true;
            }
            cur = env_rc.borrow().parent.clone();
        }
        false
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
    /// Debugger pause - propaguje az nahoru, run() vrati graceful Ok.
    /// Tree-walking interp je single-threaded UI, real freeze nelze (UI by zamrzla),
    /// proto pause = early abort skriptu. Continue v UI rerun skript s skip_first flag.
    Paused(u32),
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
/// ```ignore
/// let lexer = Lexer::parse_str("return 1 + 2;", "<script>").unwrap();
/// let tokens = /* filtrovat trivia */;
/// let program = Parser::new(tokens).parse().unwrap();
/// let mut interp = Interpreter::new();
/// let result = interp.run(&program).unwrap();
/// ```
/// Real Worker thread state.
/// Kazdy Worker bezi v separatnim threadu s vlastnim Interpreterem.
/// Komunikace pres mpsc kanaly, JSON-serializovane zpravy.
pub struct WorkerState {
    pub sender: std::sync::mpsc::Sender<String>,
    pub outgoing: std::sync::mpsc::Receiver<String>,
    pub handle: Option<std::thread::JoinHandle<()>>,
    /// onmessage callback registrovany z main threadu
    pub on_message: Option<JsValue>,
}

/// WebSocket state - background thread cte z connection + posila incoming pres
/// outgoing channel. Main interpreter posila send-message pres sender.
pub struct WebSocketState {
    pub sender: std::sync::mpsc::Sender<WebSocketCommand>,
    pub incoming: std::sync::mpsc::Receiver<WebSocketEvent>,
    pub handle: Option<std::thread::JoinHandle<()>>,
    /// readyState: 0=CONNECTING, 1=OPEN, 2=CLOSING, 3=CLOSED.
    pub ready_state: u8,
    /// Event handlers from JS (onopen/onmessage/onerror/onclose).
    pub on_open: Option<JsValue>,
    pub on_message: Option<JsValue>,
    pub on_error: Option<JsValue>,
    pub on_close: Option<JsValue>,
}

#[derive(Debug)]
pub enum WebSocketCommand {
    Send(String),
    Close,
}

/// Pending fetch task - drzi handle na pending Promise + receiver.
pub struct PendingFetch {
    pub promise_obj: Rc<RefCell<JsObject>>,
    pub url: String,
    pub receiver: std::sync::mpsc::Receiver<FetchOutcome>,
}

pub type FetchOutcome = Result<(u16, String, String, Vec<(String, String)>), String>;

#[derive(Debug)]
pub enum WebSocketEvent {
    Open,
    Message(String),
    Error(String),
    Closed,
}

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
    /// Worker state registry - Worker ID -> WorkerState.
    pub workers: Rc<RefCell<HashMap<u32, WorkerState>>>,
    /// Pocitadlo Worker ID
    pub next_worker_id: Rc<RefCell<u32>>,
    /// WebSocket state registry - WebSocket ID -> WebSocketState.
    pub websockets: Rc<RefCell<HashMap<u32, WebSocketState>>>,
    pub next_ws_id: Rc<RefCell<u32>>,
    /// DOM Document - sdileny mezi browser engine a JS interpreterem.
    /// Pri startu prazdny; muze byt nahrazen z parsed HTML.
    pub document: Rc<RefCell<crate::browser::dom::Document>>,
    /// Pending fetch tasks - non-blocking ureq spawn s mpsc Receiver.
    /// drain_fetches() vola z event loopu kazdy frame; pri try_recv Ok,
    /// pending promise se prepne na fulfilled/rejected.
    /// Format: (promise_obj, Receiver<perform_http_request output>).
    pub pending_fetches: Rc<RefCell<Vec<PendingFetch>>>,
    /// Event callback registry: ID -> JS callback funkce.
    /// Pouziva se pro addEventListener / dispatchEvent.
    pub event_callbacks: Rc<RefCell<HashMap<usize, JsValue>>>,
    /// Counter pro callback ID.
    pub next_callback_id: Rc<RefCell<usize>>,
    /// Console log capture pro DevTools: (level, message).
    pub console_log: Rc<RefCell<Vec<(String, String)>>>,
    /// Canvas 2D operations: canvas DOM node ptr -> ops sequence.
    pub canvas_ops: Rc<RefCell<std::collections::HashMap<usize, Vec<crate::browser::paint::CanvasOp>>>>,
    /// WebGL contexty per canvas DOM node ptr -> sdileny WebGLState.
    pub webgl_states: Rc<RefCell<std::collections::HashMap<usize, Rc<RefCell<WebGLState>>>>>,
    /// Network log capture: (url, status).
    pub network_log: Rc<RefCell<Vec<(String, u16)>>>,
    /// CustomElements registry: tag-name -> constructor JsValue.
    pub custom_elements: Rc<RefCell<HashMap<String, JsValue>>>,
    /// CustomElements instances: DomNode ptr -> JS instance JsValue.
    pub custom_element_instances: Rc<RefCell<HashMap<usize, JsValue>>>,
    /// MutationObserver registry: (target node ptr, callback JsValue, opts JsValue, subtree bool).
    /// Pri DOM mutaci se dispatchnou records.
    pub mutation_observers: Rc<RefCell<Vec<(usize, JsValue, JsValue, bool)>>>,
    /// Pending mutation records pro batched delivery (microtask queue).
    pub pending_mutation_records: Rc<RefCell<Vec<(usize, JsValue, JsValue)>>>,
    /// Pending XMLHttpRequest callbacks - po send() done event loop drainnnula
    /// + zavolal onload/onreadystatechange. Format: (callback, xhr_this_obj).
    pub pending_xhr_callbacks: Rc<RefCell<Vec<(JsValue, JsValue)>>>,
    /// requestAnimationFrame callbacks: (id, callback). Drain pri next frame
    /// (volat z renderer event loop). cancelAnimationFrame(id) -> remove.
    pub raf_callbacks: Rc<RefCell<Vec<(u32, JsValue)>>>,
    /// rAF id counter.
    pub next_raf_id: Rc<RefCell<u32>>,
    /// Aktualni line v exec - update z Stmt::WithLine. 0 pri rucne run.
    pub current_line: u32,
    /// Sdileny debugger state - breakpoints + pause indicator (single-thread Rc/RefCell).
    pub debugger: Rc<RefCell<DebuggerState>>,
    /// Volitelny mezi-thread sdileny debugger - kdyz nastaveny, worker thread
    /// pri pause volaa block_for_continue() namisto early-abort.
    pub shared_debugger: Option<SharedDebugger>,
    /// Continue signal pri pause v worker thread.
    pub continue_signal: Option<ContinueSignal>,
    /// Cache style objektu per DOM node (klic = Rc::as_ptr(node) as usize).
    /// Weak ref aby se uvolnili po posledni reference z JS. Pri setteru
    /// (eval_expr.rs assign_to) se z `__style_node__` propu vyzvedne Rc<Node>
    /// a sync zpet do attr "style".
    pub style_cache: Rc<RefCell<HashMap<usize, Weak<RefCell<JsObject>>>>>,
    /// Lookup callback z hosta - element rect (offsetLeft/Top/Width/Height,
    /// getBoundingClientRect). Vraci (x, y, w, h) v CSS px. None = no layout.
    pub layout_lookup: Option<Rc<dyn Fn(*const crate::browser::dom::NodeData) -> Option<(f32, f32, f32, f32)>>>,
    /// Lookup callback z hosta - cascade computed style pro getComputedStyle.
    /// Vraci HashMap<property, value>. None = vrat prazdne.
    pub cascade_lookup: Option<Rc<dyn Fn(*const crate::browser::dom::NodeData) -> HashMap<String, String>>>,
    /// window.addEventListener registry: event type -> Vec<callback>.
    pub window_listeners: Rc<RefCell<HashMap<String, Vec<JsValue>>>>,
    /// Aktualne fokusovany element (document.activeElement). Pri focus() -> Some,
    /// pri blur() -> None (a getter pak vraci document.body).
    pub focused_element: Rc<RefCell<Option<Rc<crate::browser::dom::Node>>>>,
    /// Scroll position pres `Rc<RefCell<(x, y)>>`. Host (WebView/shell) drzi
    /// stejny Rc a synchronizuje sve `scroll_x/scroll_y` z teto hodnoty po kazde
    /// JS dispatch. Default (0, 0). Pristup pres `window.pageXOffset/pageYOffset`,
    /// `window.scrollX/scrollY` a `document.documentElement.scrollTop`.
    pub scroll_pos: Rc<RefCell<(f32, f32)>>,
    /// Shadow DOM registry: host node ptr (Rc::as_ptr as usize) -> ShadowRoot obj.
    /// `attachShadow` create entry, `el.shadowRoot` getter lookup.
    /// Closed mode = lookup return Null (ale samotny objekt drzi mode="closed").
    pub shadow_roots: Rc<RefCell<HashMap<usize, Rc<RefCell<JsObject>>>>>,
}

/// Sdileny debugger state pres Arc<Mutex>. UI thread cte/zapisuje set
/// breakpoints + Continue signal, JS worker thread blocking wait na pause.
pub type SharedDebugger = std::sync::Arc<std::sync::Mutex<DebuggerState>>;

/// Continue signal pres Condvar - worker wait pri pause, UI notify pri klik.
pub type ContinueSignal = std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>;

/// Debugger state - sdileny mezi Interpreter a UI (Renderer/devtools_panel).
#[derive(Debug, Default)]
pub struct DebuggerState {
    /// Set lines kde je aktivni breakpoint (pro current source).
    pub breakpoints: std::collections::HashSet<u32>,
    /// Aktualne hit pause line (None = neni paused).
    pub paused_at: Option<u32>,
    /// Counter hit-u pres celou run pro UI feedback.
    pub hit_count: u32,
    /// Snapshot lokalnich promennych pri posledni pause (name -> stringified value).
    /// Plni se v exec_stmt pri breakpoint hit ze current env scope chain.
    pub locals: Vec<(String, String)>,
    /// Step mode - kdyz Some, dalsi exec_stmt pause bez ohledu na breakpoint.
    pub step: Option<StepKind>,
    /// Current call depth (incremented pri call_function, decremented na return).
    /// Pouziva se pro Step Over (pause kdyz depth <= snapshot) /
    /// Step Out (pause kdyz depth < snapshot).
    pub call_depth: u32,
    /// Snapshot call_depth pri Step start.
    pub step_depth_anchor: u32,
    /// Pri rerun po Continue: skip pause na teto line raz (aby script nezavartil
    /// hned znovu). Po consume se vynuluje.
    pub skip_once_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// Kazdy stmt pause - vstoupi do volane funkce.
    Into,
    /// Pause na stejne nebo niz urovni depth.
    Over,
    /// Pause az kdyz depth < anchor (vraceni z funkce).
    Out,
}

impl DebuggerState {
    pub fn is_breakpoint(&self, line: u32) -> bool {
        self.breakpoints.contains(&line)
    }
    pub fn pause_at(&mut self, line: u32) {
        self.paused_at = Some(line);
        self.hit_count = self.hit_count.saturating_add(1);
    }
    pub fn resume(&mut self) {
        // Konzumuj paused line do skip_once aby rerun preskoci stejny BP.
        self.skip_once_line = self.paused_at.take();
        self.step = None;
    }
    pub fn set_breakpoints(&mut self, lines: std::collections::HashSet<u32>) {
        self.breakpoints = lines;
    }

    /// Spustit Step kind - po next exec_stmt pause s prislusnym scope.
    pub fn start_step(&mut self, kind: StepKind) {
        self.step = Some(kind);
        self.step_depth_anchor = self.call_depth;
        self.paused_at = None;
    }

    /// True pokud step podminka pri current depth zpusobi pause.
    pub fn step_should_pause(&self) -> bool {
        let Some(kind) = self.step else { return false };
        match kind {
            StepKind::Into => true,
            StepKind::Over => self.call_depth <= self.step_depth_anchor,
            StepKind::Out => self.call_depth < self.step_depth_anchor,
        }
    }
}

// ─── Pomocne funkce ──────────────────────────────────────────────────────────


impl Interpreter {
    /// Vytvori novy interpreter s inicializovanymi vestavenymi objekty.
    pub fn new() -> Self {
        let global = Environment::new_global();
        let task_queue: Rc<RefCell<Vec<(u32, JsValue, Vec<JsValue>)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let next_timer_id: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
        let workers: Rc<RefCell<HashMap<u32, WorkerState>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let next_worker_id: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
        let websockets: Rc<RefCell<HashMap<u32, WebSocketState>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let next_ws_id: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
        let document = Rc::new(RefCell::new(
            crate::browser::dom::Document::new("about:blank".to_string())
        ));
        let console_log: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let network_log: Rc<RefCell<Vec<(String, u16)>>> = Rc::new(RefCell::new(Vec::new()));
        let custom_elements: Rc<RefCell<HashMap<String, JsValue>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let mutation_observers: Rc<RefCell<Vec<(usize, JsValue, JsValue, bool)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let pending_fetches: Rc<RefCell<Vec<PendingFetch>>> =
            Rc::new(RefCell::new(Vec::new()));
        let pending_xhr_callbacks: Rc<RefCell<Vec<(JsValue, JsValue)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let raf_callbacks: Rc<RefCell<Vec<(u32, JsValue)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let next_raf_id: Rc<RefCell<u32>> = Rc::new(RefCell::new(1));
        let scroll_pos: Rc<RefCell<(f32, f32)>> = Rc::new(RefCell::new((0.0, 0.0)));
        setup_builtins(
            &global, &task_queue, &next_timer_id, &workers, &next_worker_id,
            &document, &console_log, &network_log, &custom_elements,
            &mutation_observers, &websockets, &next_ws_id,
            &pending_fetches, &pending_xhr_callbacks,
            &raf_callbacks, &next_raf_id, &scroll_pos,
        );
        Interpreter {
            global, yield_buffer: None, task_queue, next_timer_id,
            module_cache:    Rc::new(RefCell::new(HashMap::new())),
            virtual_modules: Rc::new(RefCell::new(HashMap::new())),
            current_exports: None,
            base_dir:        Rc::new(RefCell::new(".".to_string())),
            workers, next_worker_id,
            websockets, next_ws_id,
            document,
            event_callbacks: Rc::new(RefCell::new(HashMap::new())),
            next_callback_id: Rc::new(RefCell::new(1)),
            console_log,
            network_log,
            canvas_ops: Rc::new(RefCell::new(std::collections::HashMap::new())),
            webgl_states: Rc::new(RefCell::new(std::collections::HashMap::new())),
            custom_elements,
            custom_element_instances: Rc::new(RefCell::new(std::collections::HashMap::new())),
            mutation_observers,
            pending_mutation_records: Rc::new(RefCell::new(Vec::new())),
            current_line: 0,
            debugger: Rc::new(RefCell::new(DebuggerState::default())),
            shared_debugger: None,
            continue_signal: None,
            pending_fetches,
            pending_xhr_callbacks,
            raf_callbacks,
            next_raf_id,
            style_cache: Rc::new(RefCell::new(HashMap::new())),
            layout_lookup: None,
            cascade_lookup: None,
            window_listeners: Rc::new(RefCell::new(HashMap::new())),
            focused_element: Rc::new(RefCell::new(None)),
            scroll_pos,
            shadow_roots: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Zaregistruje callback pro lookup layout boxu node-u.
    /// Volane z hosta (shell/WebView) po layout passe. JS API jako
    /// getBoundingClientRect / offsetWidth / offsetHeight / offsetLeft /
    /// offsetTop / clientWidth / clientHeight / scrollWidth / scrollHeight
    /// pak vraci skutecne rozmery.
    ///
    /// Argument: Box closure (node_ptr) -> Option<(x, y, w, h)> v CSS px.
    pub fn set_layout_lookup<F>(&mut self, f: F)
    where
        F: Fn(*const crate::browser::dom::NodeData) -> Option<(f32, f32, f32, f32)> + 'static,
    {
        self.layout_lookup = Some(Rc::new(f));
    }

    /// Zaregistruje callback pro lookup cascade-resolved CSS computed style.
    /// Volane z hosta po cascade passe. JS API window.getComputedStyle pak
    /// vrati realne hodnoty.
    pub fn set_cascade_lookup<F>(&mut self, f: F)
    where
        F: Fn(*const crate::browser::dom::NodeData) -> HashMap<String, String> + 'static,
    {
        self.cascade_lookup = Some(Rc::new(f));
    }

    /// Volane z hosta pri window-level eventu (load, resize, scroll, ...).
    /// Vsechny zaregistrovane callback-y se zavolaji s event objektem.
    pub fn dispatch_window_event(&mut self, evt_type: &str, event_obj: JsValue) {
        let listeners: Vec<JsValue> = self.window_listeners.borrow()
            .get(evt_type).cloned().unwrap_or_default();
        for cb in listeners {
            let _ = self.call_function(cb, vec![event_obj.clone()], None);
        }
    }

    /// Helper - vyhleda rect node-u pres layout_lookup.
    pub(crate) fn lookup_layout_rect(&self, node: &Rc<crate::browser::dom::NodeData>) -> Option<(f32, f32, f32, f32)> {
        let lookup = self.layout_lookup.as_ref()?;
        lookup(Rc::as_ptr(node))
    }

    /// Drain requestAnimationFrame callbacks - vsechny pending volat s
    /// timestamp ms (since interpreter start). Volat per frame z renderer.
    pub fn drain_raf_callbacks(&mut self, timestamp_ms: f64) -> Result<(), JsError> {
        // Snapshot vsechny pending + clear (kazda rAF callback je one-shot,
        // pokud si chce dalsi frame, znovu zavola requestAnimationFrame).
        let callbacks: Vec<(u32, JsValue)> = {
            let mut q = self.raf_callbacks.borrow_mut();
            std::mem::take(&mut *q)
        };
        for (_id, cb) in callbacks {
            if matches!(cb, JsValue::Undefined | JsValue::Null) { continue; }
            let _ = self.call_function(cb, vec![JsValue::Number(timestamp_ms)], None);
        }
        Ok(())
    }

    /// Drainuj dokoncene async fetch tasks - try_recv kazdy pending,
    /// pri Ok prepne pending Promise -> fulfilled/rejected. Volat z event
    /// loopu kazdy frame (App.render).
    /// Drain XMLHttpRequest pending callbacky (push pres XHR.send()). Volat z
    /// event loopu kazdy frame. Kazdy callback dostane this = xhr_obj.
    pub fn drain_xhr_callbacks(&mut self) -> Result<(), JsError> {
        loop {
            let next = {
                let mut q = self.pending_xhr_callbacks.borrow_mut();
                if q.is_empty() { None } else { Some(q.remove(0)) }
            };
            match next {
                Some((callback, this)) => {
                    if !matches!(callback, JsValue::Undefined | JsValue::Null) {
                        let _ = self.call_function(callback, vec![], Some(this));
                    }
                }
                None => break,
            }
        }
        Ok(())
    }

    pub fn drain_fetches(&mut self) {
        let mut completed: Vec<usize> = Vec::new();
        {
            let pending = self.pending_fetches.borrow();
            for (i, task) in pending.iter().enumerate() {
                if let Ok(outcome) = task.receiver.try_recv() {
                    completed.push(i);
                    let mut p = task.promise_obj.borrow_mut();
                    match outcome {
                        Ok((status, status_text, body, headers)) => {
                            let mut response = JsObject::new();
                            response.set("__response__".into(), JsValue::Bool(true));
                            response.set("__body__".into(), JsValue::Str(body));
                            response.set("url".into(), JsValue::Str(task.url.clone()));
                            response.set("status".into(), JsValue::Number(status as f64));
                            response.set("ok".into(), JsValue::Bool(status >= 200 && status < 300));
                            response.set("statusText".into(), JsValue::Str(status_text));
                            let mut hdr_obj = JsObject::new();
                            for (k, v) in headers {
                                hdr_obj.set(k.to_lowercase(), JsValue::Str(v));
                            }
                            hdr_obj.set("__headers__".into(), JsValue::Bool(true));
                            response.set("headers".into(), JsValue::Object(Rc::new(RefCell::new(hdr_obj))));
                            p.set("__promise_state__".into(), JsValue::Str("fulfilled".into()));
                            p.set("__promise_value__".into(),
                                JsValue::Object(Rc::new(RefCell::new(response))));
                        }
                        Err(msg) => {
                            let mut err = JsObject::new();
                            err.set("name".into(), JsValue::Str("TypeError".into()));
                            err.set("message".into(), JsValue::Str(format!("Failed to fetch: {msg}")));
                            p.set("__promise_state__".into(), JsValue::Str("rejected".into()));
                            p.set("__promise_value__".into(),
                                JsValue::Object(Rc::new(RefCell::new(err))));
                        }
                    }
                    // Network log update.
                    let status = match &task.receiver.try_recv() {
                        Ok(Ok((s, ..))) => *s,
                        _ => 0,
                    };
                    let _ = status;
                }
            }
        }
        // Remove completed (zpetne aby idx zustal valid).
        let mut pending = self.pending_fetches.borrow_mut();
        for i in completed.into_iter().rev() {
            if i < pending.len() { pending.remove(i); }
        }
    }

    /// Pripoj sdileny debugger state + Continue signal pro worker-thread pause.
    /// Po set: pri breakpoint hit volaa block_for_continue() namisto early-abort.
    pub fn attach_shared_debugger(&mut self, dbg: SharedDebugger, signal: ContinueSignal) {
        self.shared_debugger = Some(dbg);
        self.continue_signal = Some(signal);
    }

    /// Blocking wait na continue signal (volat z worker thread pri pause).
    /// Po notify: continued = true, vrat se a pokracuj v exec.
    pub fn block_for_continue(&self) {
        let Some(sig) = &self.continue_signal else { return };
        let (lock, cvar) = &**sig;
        let mut continued = lock.lock().unwrap();
        // Reset to false na vstup, ceka az UI nastavi true.
        *continued = false;
        while !*continued {
            continued = cvar.wait(continued).unwrap();
        }
    }

    /// Nahradi DOM document novym (po parsu HTML).
    pub fn set_document(&self, doc: crate::browser::dom::Document) {
        *self.document.borrow_mut() = doc;
    }

    /// Variant pro childList mutace s explicit added/removed nodes.
    /// Wrapper kolem dispatch_mutation_full pres added/removed lists.
    pub fn dispatch_mutation_childlist(
        &mut self,
        target: &Rc<crate::browser::dom::NodeData>,
        added: Vec<Rc<crate::browser::dom::NodeData>>,
        removed: Vec<Rc<crate::browser::dom::NodeData>>,
    ) {
        self.dispatch_mutation_full(target, "childList", None, None, added, removed);
    }

    /// Plna dispatch s vsemi MutationRecord poli. Volat pro vsechny mutace.
    /// `dispatch_mutation` (legacy) volej tuto s prazdnymi added/removed.
    pub fn dispatch_mutation_full(
        &mut self,
        target: &Rc<crate::browser::dom::NodeData>,
        record_type: &str,
        attribute_name: Option<String>,
        old_value: Option<String>,
        added: Vec<Rc<crate::browser::dom::NodeData>>,
        removed: Vec<Rc<crate::browser::dom::NodeData>>,
    ) {
        let target_ptr = Rc::as_ptr(target) as usize;
        let observers: Vec<(JsValue, JsValue)> = self.mutation_observers.borrow().iter()
            .filter(|(obs_ptr, _, _, subtree)| {
                if *obs_ptr == target_ptr { return true; }
                if !subtree { return false; }
                let mut current = target.parent.borrow().upgrade();
                while let Some(n) = current {
                    if Rc::as_ptr(&n) as usize == *obs_ptr { return true; }
                    current = n.parent.borrow().upgrade();
                }
                false
            })
            .map(|(_, cb, opts, _)| (cb.clone(), opts.clone()))
            .collect();

        if observers.is_empty() { return; }

        let added_arr: Vec<JsValue> = added.iter()
            .map(|n| JsValue::DomNode(Rc::clone(n))).collect();
        let removed_arr: Vec<JsValue> = removed.iter()
            .map(|n| JsValue::DomNode(Rc::clone(n))).collect();

        for (cb, _opts) in observers {
            let mut record = JsObject::new();
            record.set("type".into(), JsValue::Str(record_type.into()));
            record.set("target".into(), JsValue::DomNode(Rc::clone(target)));
            record.set("addedNodes".into(),
                JsValue::Array(Rc::new(RefCell::new(added_arr.clone()))));
            record.set("removedNodes".into(),
                JsValue::Array(Rc::new(RefCell::new(removed_arr.clone()))));
            if let Some(name) = &attribute_name {
                record.set("attributeName".into(), JsValue::Str(name.clone()));
                record.set("attributeNamespace".into(), JsValue::Null);
            } else {
                record.set("attributeName".into(), JsValue::Null);
            }
            if let Some(old) = &old_value {
                record.set("oldValue".into(), JsValue::Str(old.clone()));
            } else {
                record.set("oldValue".into(), JsValue::Null);
            }
            // Sibling links - real spec: previousSibling / nextSibling of removed child.
            record.set("previousSibling".into(), JsValue::Null);
            record.set("nextSibling".into(), JsValue::Null);
            let records = JsValue::Array(Rc::new(RefCell::new(vec![
                JsValue::Object(Rc::new(RefCell::new(record)))
            ])));
            let _ = self.call_function(cb, vec![records], None);
        }
    }

    /// Dispatch MutationObserver records pro mutation na danem nodu.
    /// Pro kazdeho observera s matching target (nebo ancestor pri subtree=true)
    /// zavolame callback se [{type, target, addedNodes, removedNodes, attributeName, oldValue}].
    pub fn dispatch_mutation(
        &mut self,
        target: &Rc<crate::browser::dom::NodeData>,
        record_type: &str,
        attribute_name: Option<String>,
        old_value: Option<String>,
    ) {
        let target_ptr = Rc::as_ptr(target) as usize;
        // Najit observers co matchuji target nebo (pri subtree) ancestor target.
        let observers: Vec<(JsValue, JsValue)> = self.mutation_observers.borrow().iter()
            .filter(|(obs_ptr, _, _, subtree)| {
                if *obs_ptr == target_ptr { return true; }
                if !subtree { return false; }
                // Subtree: kontroluj zda je obs_ptr ancestor target
                let mut current = target.parent.borrow().upgrade();
                while let Some(n) = current {
                    if Rc::as_ptr(&n) as usize == *obs_ptr { return true; }
                    current = n.parent.borrow().upgrade();
                }
                false
            })
            .map(|(_, cb, opts, _)| (cb.clone(), opts.clone()))
            .collect();

        for (cb, _opts) in observers {
            // Postav MutationRecord objekt
            let mut record = JsObject::new();
            record.set("type".into(), JsValue::Str(record_type.into()));
            record.set("target".into(), JsValue::DomNode(Rc::clone(target)));
            record.set("addedNodes".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
            record.set("removedNodes".into(), JsValue::Array(Rc::new(RefCell::new(Vec::new()))));
            if let Some(name) = &attribute_name {
                record.set("attributeName".into(), JsValue::Str(name.clone()));
            } else {
                record.set("attributeName".into(), JsValue::Null);
            }
            if let Some(old) = &old_value {
                record.set("oldValue".into(), JsValue::Str(old.clone()));
            } else {
                record.set("oldValue".into(), JsValue::Null);
            }
            let records = JsValue::Array(Rc::new(RefCell::new(vec![
                JsValue::Object(Rc::new(RefCell::new(record)))
            ])));
            // Callback s [records, observer]
            let _ = self.call_function(cb, vec![records], None);
        }
    }

    /// Pomocnik pro render - dispatch event na konkretni DOM node z external code.
    /// Volat addEventListener listenery pro `event_type`.
    pub fn dispatch_event(
        &mut self,
        node: &Rc<crate::browser::dom::NodeData>,
        event_type: &str,
        event_val: JsValue,
    ) -> Result<(), JsError> {
        // Walk parent chain target -> root pro bubble phase.
        // (Capture phase NOT implementovan: listeners drzi jen callback id bez
        // capture flag. Defaultni useCapture=false je 99% real-world use case,
        // bubble plne staci pro event delegation pattern.)
        let mut chain: Vec<Rc<crate::browser::dom::NodeData>> = Vec::new();
        chain.push(Rc::clone(node));
        {
            let mut cur = node.parent.borrow().upgrade();
            while let Some(p) = cur {
                let next = p.parent.borrow().upgrade();
                chain.push(p);
                cur = next;
            }
        }
        // Vyrob (nebo augment) event object s spravnym target / currentTarget /
        // stopPropagation / preventDefault. Pokud caller poslal hotovy event,
        // pridame chybejici pole; jinak vyrob novy.
        let event_obj = match &event_val {
            JsValue::Object(o) => Rc::clone(o),
            _ => {
                let mut o = JsObject::new();
                o.set("type".into(), JsValue::Str(event_type.to_string()));
                Rc::new(RefCell::new(o))
            }
        };
        {
            let mut e = event_obj.borrow_mut();
            // Ensure mandatory fields.
            if matches!(e.get("type"), JsValue::Undefined) {
                e.set("type".into(), JsValue::Str(event_type.to_string()));
            }
            e.set("target".into(), JsValue::DomNode(Rc::clone(node)));
            if matches!(e.get("bubbles"), JsValue::Undefined) {
                e.set("bubbles".into(), JsValue::Bool(true));
            }
            if matches!(e.get("cancelable"), JsValue::Undefined) {
                e.set("cancelable".into(), JsValue::Bool(true));
            }
            if matches!(e.get("defaultPrevented"), JsValue::Undefined) {
                e.set("defaultPrevented".into(), JsValue::Bool(false));
            }
            if matches!(e.get("__stopped__"), JsValue::Undefined) {
                e.set("__stopped__".into(), JsValue::Bool(false));
            }
            if matches!(e.get("__stopped_immediate__"), JsValue::Undefined) {
                e.set("__stopped_immediate__".into(), JsValue::Bool(false));
            }
            // stopPropagation / preventDefault native fns (capture event_obj
            // ref pres Rc).
            let ev_stop = Rc::clone(&event_obj);
            e.set("stopPropagation".into(), native("stopPropagation", move |_| {
                ev_stop.borrow_mut().set("__stopped__".into(), JsValue::Bool(true));
                Ok(JsValue::Undefined)
            }));
            let ev_stop_imm = Rc::clone(&event_obj);
            e.set("stopImmediatePropagation".into(),
                native("stopImmediatePropagation", move |_| {
                    let mut x = ev_stop_imm.borrow_mut();
                    x.set("__stopped__".into(), JsValue::Bool(true));
                    x.set("__stopped_immediate__".into(), JsValue::Bool(true));
                    Ok(JsValue::Undefined)
                }));
            let ev_prevent = Rc::clone(&event_obj);
            e.set("preventDefault".into(), native("preventDefault", move |_| {
                ev_prevent.borrow_mut().set("defaultPrevented".into(), JsValue::Bool(true));
                Ok(JsValue::Undefined)
            }));
        }
        // Bubble dispatch: target -> root. Pri __stopped__ break.
        let bubbles = matches!(event_obj.borrow().get("bubbles"), JsValue::Bool(true));
        for (depth, n) in chain.iter().enumerate() {
            // currentTarget update.
            event_obj.borrow_mut().set("currentTarget".into(),
                JsValue::DomNode(Rc::clone(n)));
            event_obj.borrow_mut().set("eventPhase".into(),
                JsValue::Number(if depth == 0 { 2.0 } else { 3.0 })); // AT_TARGET / BUBBLING_PHASE
            // Snapshot listenery (kopie usize). Pri call_function muze JS
            // mutate listeners - chranime se kopii.
            let ids: Vec<usize> = n.listeners.borrow().get(event_type)
                .cloned().unwrap_or_default();
            for id in ids {
                if matches!(event_obj.borrow().get("__stopped_immediate__"), JsValue::Bool(true)) {
                    return Ok(());
                }
                let cb = self.event_callbacks.borrow().get(&id).cloned();
                if let Some(cb) = cb {
                    let arg = JsValue::Object(Rc::clone(&event_obj));
                    self.call_function(cb, vec![arg], Some(JsValue::DomNode(Rc::clone(n))))?;
                }
            }
            // stopPropagation -> nepokracovat bubble do parenta.
            if matches!(event_obj.borrow().get("__stopped__"), JsValue::Bool(true)) {
                return Ok(());
            }
            // Pokud event nebubbluje, dispatch jen na target.
            if !bubbles && depth == 0 { break; }
        }
        Ok(())
    }

    /// Drainuje WebSocket events z bg threadu a vola registrovane callbacky.
    pub fn drain_websockets(&mut self) -> Result<(), JsError> {
        // Fast path - prazdny pool.
        if self.websockets.borrow().is_empty() { return Ok(()); }
        // Sber events z vsech sockets.
        let pending: Vec<(u32, WebSocketEvent)> = {
            let map = self.websockets.borrow();
            let mut out = Vec::new();
            for (id, state) in map.iter() {
                while let Ok(evt) = state.incoming.try_recv() {
                    out.push((*id, evt));
                }
            }
            out
        };
        for (id, evt) in pending {
            let (cb, ready_state_after) = {
                let map = self.websockets.borrow();
                let state = match map.get(&id) { Some(s) => s, None => continue };
                let cb = match &evt {
                    WebSocketEvent::Open => state.on_open.clone(),
                    WebSocketEvent::Message(_) => state.on_message.clone(),
                    WebSocketEvent::Error(_) => state.on_error.clone(),
                    WebSocketEvent::Closed => state.on_close.clone(),
                };
                let new_state = match &evt {
                    WebSocketEvent::Open => 1u8,
                    WebSocketEvent::Closed => 3u8,
                    _ => state.ready_state,
                };
                (cb, new_state)
            };
            // Update ready_state.
            if let Some(s) = self.websockets.borrow_mut().get_mut(&id) {
                s.ready_state = ready_state_after;
            }
            if let Some(cb) = cb {
                let mut event = JsObject::new();
                let evt_type = match &evt {
                    WebSocketEvent::Open => "open",
                    WebSocketEvent::Message(_) => "message",
                    WebSocketEvent::Error(_) => "error",
                    WebSocketEvent::Closed => "close",
                };
                event.set("type".into(), JsValue::Str(evt_type.to_string()));
                if let WebSocketEvent::Message(t) = &evt {
                    event.set("data".into(), JsValue::Str(t.clone()));
                }
                if let WebSocketEvent::Error(e) = &evt {
                    event.set("message".into(), JsValue::Str(e.clone()));
                }
                self.call_function(cb, vec![JsValue::Object(Rc::new(RefCell::new(event)))], None)?;
            }
        }
        Ok(())
    }

    /// Drainuje vsechny worker zpravy a zavola onmessage callbacky.
    fn drain_workers(&mut self) -> Result<(), JsError> {
        // Fast path - prazdny pool.
        if self.workers.borrow().is_empty() { return Ok(()); }
        // Sber zprav z vsech workeru (ID + msg)
        let pending: Vec<(u32, String)> = {
            let workers = self.workers.borrow();
            let mut out = Vec::new();
            for (id, state) in workers.iter() {
                while let Ok(msg) = state.outgoing.try_recv() {
                    out.push((*id, msg));
                }
            }
            out
        };
        // Vyvolat onmessage callback per zpravu
        for (id, msg) in pending {
            let cb = self.workers.borrow().get(&id).and_then(|s| s.on_message.clone());
            if let Some(cb) = cb {
                let parsed = json_parse(&msg).unwrap_or(JsValue::Str(msg));
                let mut event = JsObject::new();
                event.set("data".into(), parsed);
                self.call_function(cb, vec![JsValue::Object(Rc::new(RefCell::new(event)))], None)?;
            }
        }
        Ok(())
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
            Some(Signal::Paused(line)) => {
                // Tree-walking single-thread: pause = early abort. UI obsluha
                // ulozi `program` pro pripadny rerun po Continue.
                self.console_log.borrow_mut().push((
                    "info".into(),
                    format!("[debugger] script paused at line {} (early abort)", line),
                ));
                JsValue::Undefined
            }
            _ => JsValue::Undefined,
        };
        // Drain timer queue - spust vsechny setTimeout callbacky
        self.drain_timers()?;
        // Drain XHR onload/onreadystatechange callbacky.
        self.drain_xhr_callbacks()?;
        // Worker + websocket sync: 50ms sleep dava worker threadum cas dorucit
        // zpravy. Skip pri prazdnych pools - ciste sync programy nemusi cekat.
        let has_workers = !self.workers.borrow().is_empty();
        let has_websockets = !self.websockets.borrow().is_empty();
        if has_workers || has_websockets {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if has_workers { self.drain_workers()?; }
            if has_websockets { self.drain_websockets()?; }
            // Terminate workery (drop senderu -> threadu signal)
            let ids: Vec<u32> = self.workers.borrow().keys().cloned().collect();
            for id in ids {
                self.workers.borrow_mut().remove(&id);
            }
        }
        Ok(result)
    }

    /// Spusti vsechny cekajici timer callbacky.
    fn drain_timers(&mut self) -> Result<(), JsError> {
        // Fast path - prazdne queue, zadny borrow needed.
        if self.task_queue.borrow().is_empty() { return Ok(()); }
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

    // ─── Třídy ────────────────────────────────────────────────────────────────



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


    /// Iterator.prototype.toArray, map, filter, take, drop, reduce, forEach,
    /// some, every, find, flatMap. ES2025 Iterator helpers.
    /// Volane z call_method pres fast path pro iteratory.
    pub fn iterator_helper_method(
        &mut self,
        iter: JsValue,
        method: &str,
        args: Vec<JsValue>,
    ) -> Result<JsValue, JsError> {
        let values = self.collect_iterable(iter)?;
        match method {
            "toArray" => Ok(JsValue::Array(Rc::new(RefCell::new(values)))),
            "map" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let mut out = Vec::with_capacity(values.len());
                for v in values {
                    out.push(self.call_function(f.clone(), vec![v], None)?);
                }
                Ok(make_iterator_from_values(out))
            }
            "filter" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let mut out = Vec::new();
                for v in values {
                    let keep = self.call_function(f.clone(), vec![v.clone()], None)?;
                    if keep.is_truthy() { out.push(v); }
                }
                Ok(make_iterator_from_values(out))
            }
            "take" => {
                let n = args.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
                Ok(make_iterator_from_values(values.into_iter().take(n).collect()))
            }
            "drop" => {
                let n = args.into_iter().next().map(|v| v.to_number() as usize).unwrap_or(0);
                Ok(make_iterator_from_values(values.into_iter().skip(n).collect()))
            }
            "reduce" => {
                let mut it = args.into_iter();
                let f = it.next().unwrap_or(JsValue::Undefined);
                let init = it.next();
                let has_init = init.is_some();
                let mut acc = match init {
                    Some(v) => v,
                    None => {
                        if values.is_empty() {
                            return Err(JsError::Runtime("TypeError: Reduce of empty iterator with no initial value".into()));
                        }
                        values[0].clone()
                    }
                };
                let start = if has_init { 0 } else { 1 };
                for v in &values[start..] {
                    acc = self.call_function(f.clone(), vec![acc, v.clone()], None)?;
                }
                Ok(acc)
            }
            "forEach" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                for v in values {
                    self.call_function(f.clone(), vec![v], None)?;
                }
                Ok(JsValue::Undefined)
            }
            "some" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                for v in values {
                    let r = self.call_function(f.clone(), vec![v], None)?;
                    if r.is_truthy() { return Ok(JsValue::Bool(true)); }
                }
                Ok(JsValue::Bool(false))
            }
            "every" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                for v in values {
                    let r = self.call_function(f.clone(), vec![v], None)?;
                    if !r.is_truthy() { return Ok(JsValue::Bool(false)); }
                }
                Ok(JsValue::Bool(true))
            }
            "find" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                for v in values {
                    let r = self.call_function(f.clone(), vec![v.clone()], None)?;
                    if r.is_truthy() { return Ok(v); }
                }
                Ok(JsValue::Undefined)
            }
            "flatMap" => {
                let f = args.into_iter().next().unwrap_or(JsValue::Undefined);
                let mut out = Vec::new();
                for v in values {
                    let mapped = self.call_function(f.clone(), vec![v], None)?;
                    let inner = self.collect_iterable(mapped).unwrap_or_default();
                    out.extend(inner);
                }
                Ok(make_iterator_from_values(out))
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    /// Sbira vsechny hodnoty z iteratoru nebo iterovatelneho objektu (interni).
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
            // Iterator helper objekt - rovnou drainni
            let next_fn = o.borrow().get("next");
            if !matches!(next_fn, JsValue::Undefined) {
                return self.drain_iterator(val.clone());
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
