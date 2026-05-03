/// AST (Abstract Syntax Tree) pro podmnozinu JavaScriptu/ESNext.
///
/// Hierarchie: `Program` -> `Stmt` -> `Expr`
///
/// Pouziti:
/// ```
/// // Zdrojovy kod se tokenizuje lexerem, parsuje parserem,
/// // a interpreter prochazi toto AST a vyhodnocuje ho.
/// ```

// ─── Vyrazy ───────────────────────────────────────────────────────────────────

/// Uzel reprezentujici vyraz (neco co vraci hodnotu).
///
/// Pokryva: literaly, identifikatory, operatory, volani funkci,
/// pristup k vlastnostem, definice funkci a pomocne konstrukty.
#[derive(Debug, Clone)]
pub enum Expr {
    // --- Literaly ---

    /// Ciselny literal: `42`, `3.14`, `0xFF`
    Number(f64),
    /// Retezec: `"hello"` nebo `'world'`
    Str(String),
    /// Boolean: `true` nebo `false`
    Bool(bool),
    /// Klic. slovo `null`
    Null,
    /// Klic. slovo `undefined`
    Undefined,
    /// Regularni vyraz: `/pattern/flags`
    Regex(String, String),   // (pattern, flags)

    /// Template literal: `` `Hello ${name}!` ``
    ///
    /// Struktura: `quasis[0] + expressions[0] + quasis[1] + ... + quasis[n]`
    /// Pocet quasis je vzdy o 1 vetsi nez pocet expressions.
    Template { quasis: Vec<String>, expressions: Vec<Box<Expr>> },

    // --- Identifikatory a kolekce ---

    /// Identifikator promenne nebo funkce: `foo`, `myVar`
    Ident(String),

    /// Pole: `[1, 2, 3]`
    /// `None` na pozici = hole (`[1, , 3]`)
    Array(Vec<Option<Box<Expr>>>),

    /// Objektovy literal: `{ a: 1, b: 2 }`
    Object(Vec<ObjectProp>),

    // --- Operatory ---

    /// Unarni operator: `-x`, `!x`, `typeof x`, `++x`, ...
    Unary  { op: UnaryOp, arg: Box<Expr> },

    /// Binarni operator: `a + b`, `a * b`, `a instanceof B`, ...
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },

    /// Logicky operator s short-circuit: `a && b`, `a || b`, `a ?? b`
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },

    /// Ternary (podmineny) vyraz: `test ? yes : no`
    Ternary { test: Box<Expr>, yes: Box<Expr>, no: Box<Expr> },

    /// Prirazovaci vyraz: `x = 5`, `x += 3`, `x &&= y`, ...
    Assign  { op: AssignOp, target: Box<Expr>, value: Box<Expr> },

    // --- Volani a pristup k vlastnostem ---

    /// Volani funkce: `foo(1, 2)` nebo `obj?.method()`
    ///
    /// `optional = true` znamena optional chaining `?.()` -
    /// vrati `undefined` misto chyby, kdyz je callee null/undefined.
    Call   { callee: Box<Expr>, args: Vec<Expr>, optional: bool },

    /// Konstruktorove volani: `new Foo(args)`
    New    { callee: Box<Expr>, args: Vec<Expr> },

    /// Pristup k vlastnosti objektu: `obj.prop` nebo `obj[expr]`
    ///
    /// `optional = true` znamena `?.` - vrati `undefined` misto chyby.
    Member { object: Box<Expr>, prop: MemberProp, optional: bool },

    // --- Definice funkci ---

    /// Vyrazova funkce: `function name(params) { body }`
    Function { name: Option<String>, params: Vec<Param>, body: Vec<Stmt> },

    /// Arrow funkce: `x => x * 2` nebo `(a, b) => { return a + b; }`
    Arrow    { params: Vec<Param>, body: ArrowBody },

    // --- Pomocne ---

    /// Spread operator: `...expr` (v poli nebo argumentech volani)
    Spread(Box<Expr>),

    /// Sekvence vyrazu oddelena carkami: `(a, b, c)`
    Sequence(Vec<Expr>),
}

/// Jedna vlastnost v objektovem literalu.
#[derive(Debug, Clone)]
pub struct ObjectProp {
    /// Klic vlastnosti
    pub key: PropKey,
    /// Hodnota vlastnosti
    pub value: Box<Expr>,
    /// `true` pro zkracenou syntaxi `{ x }` (ekvivalent `{ x: x }`)
    pub shorthand: bool,
    /// `true` pro vypocitany klic `{ [expr]: value }`
    pub computed: bool,
}

/// Klic vlastnosti v objektovem literalu.
#[derive(Debug, Clone)]
pub enum PropKey {
    /// Textovy klic: `{ foo: 1 }`
    Ident(String),
    /// Retezec jako klic: `{ "foo": 1 }`
    Str(String),
    /// Cislo jako klic: `{ 42: "val" }`
    Num(f64),
    /// Vypocitany klic: `{ [expr]: val }`
    Computed(Box<Expr>),
}

/// Zpusob pristupu k vlastnosti objektu.
#[derive(Debug, Clone)]
pub enum MemberProp {
    /// Teckova notace: `obj.name`
    Ident(String),
    /// Hranatobrakova notace: `obj[expr]`
    Computed(Box<Expr>),
}

/// Telo arrow funkce - bud jeden vyraz, nebo blok prikazu.
#[derive(Debug, Clone)]
pub enum ArrowBody {
    /// `x => x * 2` - implicitni return
    Expr(Box<Expr>),
    /// `x => { return x; }` - explicitni return
    Block(Vec<Stmt>),
}

// ─── Operatory ────────────────────────────────────────────────────────────────

/// Unarni operatory.
///
/// Zahrnuje prefix (`-x`, `!x`, `++x`, `--x`) i postfix (`x++`, `x--`)
/// verze operatoru inkrement/dekrement - ty jsou ulozeny jako binarni op `PostInc`/`PostDec`.
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Minus, Plus, Not, BitNot,
    Typeof, Void, Delete,
    /// Prefix inkrement: `++x`
    PreInc,
    /// Prefix dekrement: `--x`
    PreDec,
}

/// Binarni operatory vcetne postfix inkrement/dekrement.
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    // Aritmetika
    Add, Sub, Mul, Div, Mod, Exp,
    // Porovnani (volna a strikni rovnost)
    Eq, NotEq, StrictEq, StrictNotEq,
    Lt, Gt, LtEq, GtEq,
    // Bitove operace
    BitAnd, BitOr, BitXor, Shl, Shr, Ushr,
    // Ostatni
    In, Instanceof,
    /// Postfix inkrement: `x++`
    PostInc,
    /// Postfix dekrement: `x--`
    PostDec,
}

/// Logicke operatory s lazy (short-circuit) vyhodnocenim.
#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOp {
    /// `&&` - vraci levy operand pokud je falsy, jinak pravy
    And,
    /// `||` - vraci levy operand pokud je truthy, jinak pravy
    Or,
    /// `??` - vraci levy operand pokud neni null/undefined, jinak pravy
    NullCoal,
}

/// Prirazovaci operatory.
#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    /// `=` - jednoduche prirazeni
    Assign,
    // Aritmeticke prirazeni
    Add, Sub, Mul, Div, Mod, Exp,
    // Bitove prirazeni
    BitAnd, BitOr, BitXor, Shl, Shr, Ushr,
    /// `&&=` - prirad jen kdyz je levy operand truthy
    LogicalAnd,
    /// `||=` - prirad jen kdyz je levy operand falsy
    LogicalOr,
    /// `??=` - prirad jen kdyz je levy operand null nebo undefined
    NullCoal,
}

// ─── Parametry funkci ─────────────────────────────────────────────────────────

/// Jeden parametr funkce nebo arrow funkce.
///
/// # Priklady
/// ```javascript
/// function f(x)          // Param { name: "x", default: None, rest: false }
/// function f(x = 42)     // Param { name: "x", default: Some(42), rest: false }
/// function f(...args)    // Param { name: "args", default: None, rest: true }
/// ```
#[derive(Debug, Clone)]
pub struct Param {
    /// Nazev parametru
    pub name: String,
    /// Vychozi hodnota: `(x = 42)` - vyhodnocuje se az pri volani, pokud je arg `undefined`
    pub default: Option<Box<Expr>>,
    /// `true` pro rest parametr `...args` - sebere vsechny zbyvajici argumenty do pole
    pub rest: bool,
}

impl Param {
    /// Vytvori jednoduchy parametr bez defaultu a bez rest flagu.
    pub fn simple(name: String) -> Self {
        Param { name, default: None, rest: false }
    }
}

// ─── Prikazy ──────────────────────────────────────────────────────────────────

/// Prikaz (statement) - neco co se vykonava, ale primo nevraci hodnotu.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// Vyrazovy prikaz: `foo(); x = 5;`
    Expr(Expr),
    /// Blok prikazu: `{ stmt1; stmt2; }`
    Block(Vec<Stmt>),
    /// Prazdny prikaz: `;`
    Empty,
    /// Navrat z funkce: `return;` nebo `return expr;`
    Return(Option<Expr>),
    /// Preruseni cyklu nebo switch: `break;` nebo `break label;`
    Break(Option<String>),
    /// Preskoceni iterace cyklu: `continue;` nebo `continue label;`
    Continue(Option<String>),
    /// Vyhozeni vyjimky: `throw expr;`
    Throw(Expr),

    /// Deklarace promennych: `var x = 1;`, `let y;`, `const Z = 42;`
    Var { kind: VarKind, decls: Vec<VarDecl> },

    /// Deklarace pojmenovane funkce: `function name(params) { body }`
    Function { name: String, params: Vec<Param>, body: Vec<Stmt> },

    /// Podminene vetveni: `if (test) yes` nebo `if (test) yes else no`
    If     { test: Expr, yes: Box<Stmt>, no: Option<Box<Stmt>> },
    /// Cyklus while: `while (test) body`
    While  { test: Expr, body: Box<Stmt> },
    /// Cyklus do-while: `do body while (test);`
    DoWhile { body: Box<Stmt>, test: Expr },
    /// Klasicky for cyklus: `for (init; test; update) body`
    For    { init: Option<ForInit>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    /// For-in cyklus: `for (key in obj) body`
    ForIn  { kind: Option<VarKind>, target: Box<Expr>, iter: Expr, body: Box<Stmt> },
    /// For-of cyklus: `for (val of iterable) body`
    ForOf  { kind: Option<VarKind>, target: Box<Expr>, iter: Expr, body: Box<Stmt> },

    /// Try-catch-finally: `try { } catch (e) { } finally { }`
    Try { body: Vec<Stmt>, catch: Option<CatchClause>, finally: Option<Vec<Stmt>> },

    /// Oznaceny prikaz: `label: stmt` (pro break/continue s labelem)
    Labeled { label: String, body: Box<Stmt> },
}

/// Jedna polozka v deklaraci promennych.
#[derive(Debug, Clone)]
pub struct VarDecl {
    /// Nazev promenne (zatim bez destructuringu)
    pub name: String,
    /// Pocatecni hodnota: `let x = 5` vs. `let x;`
    pub init: Option<Expr>,
}

/// Druh deklarace promenne.
#[derive(Debug, Clone, PartialEq)]
pub enum VarKind {
    /// `var` - function-scoped, hoistovana
    Var,
    /// `let` - block-scoped, neni hoistovana
    Let,
    /// `const` - block-scoped, nelze priradit po inicializaci
    Const,
}

/// Inicializace pro klasicky for cyklus.
#[derive(Debug, Clone)]
pub enum ForInit {
    /// `for (let i = 0; ...)` - deklarace promenne
    Var { kind: VarKind, decls: Vec<VarDecl> },
    /// `for (i = 0; ...)` - vyraz
    Expr(Expr),
}

/// Klauzule catch v try-catch.
#[derive(Debug, Clone)]
pub struct CatchClause {
    /// Volitelny parametr: `catch (e)` vs. `catch { }` (bez parametru)
    pub param: Option<String>,
    pub body: Vec<Stmt>,
}

// ─── Program ──────────────────────────────────────────────────────────────────

/// Koren AST - cely parsovany JS soubor nebo skript.
#[derive(Debug, Clone)]
pub struct Program {
    /// Sekvence prikazu na nejvyssi urovni
    pub body: Vec<Stmt>,
    /// `true` kdyz soubor obsahuje `"use strict"` direktivu
    pub strict: bool,
}
