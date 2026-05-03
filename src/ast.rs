/// AST pro podmnoZinu JavaScriptu.
/// Pokryva vse potrebne pro ESNext podmnozinu.

// ─── Výrazy ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    // Literály
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,
    Regex(String, String),   // (pattern, flags)

    // Template literál: `Hello ${name}!`
    // quasis[0] + exprs[0] + quasis[1] + exprs[1] + ... + quasis[n]
    Template { quasis: Vec<String>, expressions: Vec<Box<Expr>> },

    // Identifikátor
    Ident(String),

    // Kolekce
    Array(Vec<Option<Box<Expr>>>),  // None = prázná pozice (hole)
    Object(Vec<ObjectProp>),

    // Operátory
    Unary  { op: UnaryOp, arg: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },
    Ternary { test: Box<Expr>, yes: Box<Expr>, no: Box<Expr> },
    Assign  { op: AssignOp, target: Box<Expr>, value: Box<Expr> },

    // Volani a pristup k vlastnostem
    // optional=true -> ?. (vrati Undefined misto chyby kdyz object je null/undefined)
    Call   { callee: Box<Expr>, args: Vec<Expr>, optional: bool },
    New    { callee: Box<Expr>, args: Vec<Expr> },
    Member { object: Box<Expr>, prop: MemberProp, optional: bool },

    // Funkce
    Function { name: Option<String>, params: Vec<Param>, body: Vec<Stmt> },
    Arrow    { params: Vec<Param>, body: ArrowBody },

    // Pomocné
    Spread(Box<Expr>),
    Sequence(Vec<Expr>),
}

#[derive(Debug, Clone)]
pub struct ObjectProp {
    pub key: PropKey,
    pub value: Box<Expr>,
    pub shorthand: bool,      // { x } === { x: x }
    pub computed: bool,       // { [expr]: value }
}

#[derive(Debug, Clone)]
pub enum PropKey {
    Ident(String),
    Str(String),
    Num(f64),
    Computed(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum MemberProp {
    Ident(String),        // obj.name
    Computed(Box<Expr>),  // obj[expr]
}

#[derive(Debug, Clone)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

// ─── Operátory ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp { Minus, Plus, Not, BitNot, Typeof, Void, Delete, PreInc, PreDec }

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Exp,
    Eq, NotEq, StrictEq, StrictNotEq,
    Lt, Gt, LtEq, GtEq,
    BitAnd, BitOr, BitXor, Shl, Shr, Ushr,
    In, Instanceof,
    PostInc, PostDec,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOp { And, Or, NullCoal }

#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Assign,
    Add, Sub, Mul, Div, Mod, Exp,
    BitAnd, BitOr, BitXor, Shl, Shr, Ushr,
    LogicalAnd,   // &&=
    LogicalOr,    // ||=
    NullCoal,     // ??=
}

// ─── Parametry funkci ─────────────────────────────────────────────────────────

/// Jeden parametr funkce / arrow funkce.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub default: Option<Box<Expr>>,   // (x = 42) - vyhodnoceno pri volani
    pub rest: bool,                    // ...args   - sbira zbyvajici args do pole
}

impl Param {
    pub fn simple(name: String) -> Self {
        Param { name, default: None, rest: false }
    }
}

// ─── Prikazy ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Expr),
    Block(Vec<Stmt>),
    Empty,
    Return(Option<Expr>),
    Break(Option<String>),
    Continue(Option<String>),
    Throw(Expr),

    // Deklarace proměnných
    Var { kind: VarKind, decls: Vec<VarDecl> },

    // Deklarace funkce
    Function { name: String, params: Vec<Param>, body: Vec<Stmt> },

    // Větvení a cykly
    If     { test: Expr, yes: Box<Stmt>, no: Option<Box<Stmt>> },
    While  { test: Expr, body: Box<Stmt> },
    DoWhile { body: Box<Stmt>, test: Expr },
    For    { init: Option<ForInit>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    ForIn  { kind: Option<VarKind>, target: Box<Expr>, iter: Expr, body: Box<Stmt> },
    ForOf  { kind: Option<VarKind>, target: Box<Expr>, iter: Expr, body: Box<Stmt> },

    // Ošetření chyb
    Try { body: Vec<Stmt>, catch: Option<CatchClause>, finally: Option<Vec<Stmt>> },

    // Label
    Labeled { label: String, body: Box<Stmt> },
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,       // Zjednodušeno: bez destructuring
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VarKind { Var, Let, Const }

#[derive(Debug, Clone)]
pub enum ForInit {
    Var { kind: VarKind, decls: Vec<VarDecl> },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub param: Option<String>,
    pub body: Vec<Stmt>,
}

// ─── Program ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Program {
    pub body: Vec<Stmt>,
    pub strict: bool,
}
