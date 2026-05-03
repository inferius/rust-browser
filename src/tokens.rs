use std::fmt;
use num_bigint::BigInt;

use unicode_ident::{is_xid_start, is_xid_continue};
use unicode_general_category::{get_general_category, GeneralCategory};
use crate::string_enum;

// ─── Konstanty (definovány jen jednou na úrovni modulu) ───────────────────────
pub const LB_LF: char = '\u{000A}';
pub const LB_CR: char = '\u{000D}';
pub const LB_LS: char = '\u{2028}';
pub const LB_PS: char = '\u{2029}';
pub const LINE_BREAK_CHARS: &[char] = &[LB_LF, LB_CR, LB_LS, LB_PS];

// ─── Typy tokenů ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literaly
    Identifier(String),
    NumericLiteral {
        raw: String, value: f64, bigint_value: Option<BigInt>,
        base: NumericBase, legacy_octal: bool,
        is_bigint: bool, has_exponent: bool, is_valid: bool,
    },
    StringLiteral { value: String, raw: String, escapes: Vec<EscapeInfo> },
    RegexLiteral { pattern: String, flags: String },

    // Template literal tokenizovany po castech (ECMAScript spec):
    // `abc`           -> NoSubstitutionTemplate("abc")
    // `abc${         -> TemplateHead("abc")
    // }def${         -> TemplateMiddle("def")
    // }ghi`          -> TemplateTail("ghi")
    NoSubstitutionTemplate(String),
    TemplateHead(String),
    TemplateMiddle(String),
    TemplateTail(String),

    Keyword(KeywordEnum),
    Operator(OperatorEnum),

    // Komentáře
    CommentLine(String),
    CommentBlock(String),
    HtmlCommentStart,

    // Interní
    Whitespace,
    Newline,
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Identifier(s)               => write!(f, "Identifier({s})"),
            TokenKind::Keyword(k)                  => write!(f, "Keyword({})", k.as_str()),
            TokenKind::Operator(o)                 => write!(f, "Operator({})", o.as_str()),
            TokenKind::NumericLiteral { value, .. } => write!(f, "Number({value})"),
            TokenKind::StringLiteral { value, .. }    => write!(f, "String(\"{value}\")"),
            TokenKind::NoSubstitutionTemplate(s)      => write!(f, "Template(`{s}`)"),
            TokenKind::TemplateHead(s)                => write!(f, "TemplateHead(`{s}${{`)"),
            TokenKind::TemplateMiddle(s)              => write!(f, "TemplateMiddle(`}}{s}${{`)"),
            TokenKind::TemplateTail(s)                => write!(f, "TemplateTail(`}}{s}`)"),
            _                                         => write!(f, "{self:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EscapeKind { Octal, Hex, Unicode, Simple }

#[derive(Debug, Clone, PartialEq)]
pub struct EscapeInfo {
    pub kind: EscapeKind,
    pub raw: String,
    pub resolved_char: char,
    pub position_in_raw: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub enum NumericBase { Decimal, Hex, Binary, Octal }

#[derive(Debug, PartialEq)]
pub enum CommentKind { SingleLine, MultiLine, None }

#[derive(Debug, PartialEq)]
pub enum StringKind { DoubleQuote, SingleQuote, TemplateString, None }

// ─── Token ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

// ─── Klasifikační metody (statické) ───────────────────────────────────────────

impl Token {
    pub fn is_line_break(ch: char) -> bool {
        LINE_BREAK_CHARS.contains(&ch)
    }

    pub fn is_crlf(ch: char, next: Option<char>) -> bool {
        ch == LB_CR && next == Some(LB_LF)
    }

    /// Bílé znaky dle ECMAScript:
    /// TAB, VT, FF, ZWNBSP → explicitní list
    /// Ostatní SpaceSeparatory → unicode kategorie
    ///
    /// BUG v originálu: používal && místo ||, takže Tab/VT/FF/BOM nebyly
    /// rozpoznány (nejsou SpaceSeparator).
    pub fn is_white_space(ch: char) -> bool {
        matches!(ch, '\u{0009}' | '\u{000B}' | '\u{000C}' | '\u{FEFF}')
            || get_general_category(ch) == GeneralCategory::SpaceSeparator
    }

    pub fn is_string_start(ch: char) -> StringKind {
        match ch {
            '"'  => StringKind::DoubleQuote,
            '\'' => StringKind::SingleQuote,
            '`'  => StringKind::TemplateString,
            _    => StringKind::None,
        }
    }

    pub fn is_number_start(ch: char, next: Option<char>) -> bool {
        if ch.is_ascii_digit() { return true; }
        ch == '.' && next.map(|c| c.is_ascii_digit()).unwrap_or(false)
    }

    pub fn is_comment_start(ch: char, next: Option<char>) -> CommentKind {
        match (ch, next) {
            ('/', Some('/')) => CommentKind::SingleLine,
            ('/', Some('*')) => CommentKind::MultiLine,
            _                => CommentKind::None,
        }
    }

    pub fn is_hashbang(ch: char, next: Option<char>) -> bool {
        ch == '#' && next == Some('!')
    }

    pub fn is_valid_identifier_start(ch: char) -> bool {
        ch == '$' || ch == '_' || is_xid_start(ch)
    }

    pub fn is_valid_identifier_continue(ch: char) -> bool {
        ch == '$' || is_xid_continue(ch)
    }

    pub fn is_single_escape_char(ch: char) -> bool {
        matches!(ch, '\'' | '"' | '\\' | '`' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | '0')
    }

    pub fn is_keyword(s: &str) -> bool { KeywordEnum::from_str(s).is_some() }
    pub fn get_keyword(s: &str) -> Option<KeywordEnum> { KeywordEnum::from_str(s) }

    /// Vrátí true pokud znak může být začátkem operátoru.
    pub fn is_operator_start(ch: char) -> bool {
        matches!(ch,
            '+' | '-' | '*' | '/' | '%' | '.' | ',' | ':' | ';' | '?' |
            '~' | '!' | '=' | '&' | '|' | '^' | '<' | '>' |
            '(' | ')' | '{' | '}' | '[' | ']' | '#'
        )
    }

    pub fn get_operator(s: &str) -> Option<OperatorEnum> { OperatorEnum::from_str(s) }
}

// ─── Klíčová slova (lowercase – tak jak jsou ve zdrojáku JS) ─────────────────

string_enum! {
    KeywordEnum,
    Async      => "async",
    Await      => "await",
    Break      => "break",
    Case       => "case",
    Catch      => "catch",
    Class      => "class",
    Const      => "const",
    Continue   => "continue",
    Debugger   => "debugger",
    Default    => "default",
    Delete     => "delete",
    Do         => "do",
    Else       => "else",
    Export     => "export",
    Extends    => "extends",
    False      => "false",
    Finally    => "finally",
    For        => "for",
    Function   => "function",
    Get        => "get",
    If         => "if",
    Implements => "implements",
    Import     => "import",
    In         => "in",
    Instanceof => "instanceof",
    Interface  => "interface",
    Let        => "let",
    New        => "new",
    Null       => "null",
    Of         => "of",
    Package    => "package",
    Private    => "private",
    Protected  => "protected",
    Public     => "public",
    Return     => "return",
    Set        => "set",
    Static     => "static",
    Super      => "super",
    Switch     => "switch",
    This       => "this",
    Throw      => "throw",
    True       => "true",
    Try        => "try",
    Typeof     => "typeof",
    Var        => "var",
    Void       => "void",
    While      => "while",
    With       => "with",
    Yield      => "yield"
}

// ─── Operátory (od nejdelšího k nejkratšímu pro greedy matching) ─────────────

string_enum! {
    OperatorEnum,
    // 4 znaky
    UnsignedRightShiftAssign => ">>>=",
    // 3 znaky
    StrictEqual     => "===",
    StrictNotEqual  => "!==",
    ShiftRightU     => ">>>",
    AssignExp       => "**=",
    AssignShl       => "<<=",
    AssignShr       => ">>=",
    Ellipsis        => "...",
    LogAndAssign    => "&&=",
    LogOrAssign     => "||=",
    NullCoalAssign  => "??=",
    // 2 znaky
    EqEq            => "==",
    NotEq           => "!=",
    LtEq            => "<=",
    GtEq            => ">=",
    And             => "&&",
    Or              => "||",
    NullCoal        => "??",
    PlusPlus        => "++",
    MinusMinus      => "--",
    Shl             => "<<",
    Shr             => ">>",
    AddAssign       => "+=",
    SubAssign       => "-=",
    MulAssign       => "*=",
    DivAssign       => "/=",
    ModAssign       => "%=",
    AndAssign       => "&=",
    OrAssign        => "|=",
    XorAssign       => "^=",
    Exp             => "**",
    Arrow           => "=>",
    OptChain        => "?.",
    // 1 znak
    Plus            => "+",
    Minus           => "-",
    Star            => "*",
    Slash           => "/",
    Percent         => "%",
    Dot             => ".",
    Comma           => ",",
    Colon           => ":",
    Semi            => ";",
    Question        => "?",
    Tilde           => "~",
    Bang            => "!",
    Assign          => "=",
    Amp             => "&",
    Pipe            => "|",
    Caret           => "^",
    Lt              => "<",
    Gt              => ">",
    LParen          => "(",
    RParen          => ")",
    LBrace          => "{",
    RBrace          => "}",
    LBracket        => "[",
    RBracket        => "]",
    Hash            => "#"
}
