/// Tokeny lexeru - datove typy pro vystup lexikalni analyzy.
///
/// Lexer prevadi zdrojovy text na sekvenci `Token` hodnot.
/// Kazdy `Token` nosi svuj druh (`TokenKind`), surovy text (`lexeme`)
/// a pozici ve zdrojovem souboru.

use std::fmt;
use num_bigint::BigInt;

use unicode_ident::{is_xid_start, is_xid_continue};
use unicode_general_category::{get_general_category, GeneralCategory};
use crate::string_enum;

// ─── Konstanty pro zalomeni radku (dle ECMAScript spec) ───────────────────────

/// Line Feed (LF) - Unix konec radku
pub const LB_LF: char = '\u{000A}';
/// Carriage Return (CR) - Windows konec radku (parova s LF)
pub const LB_CR: char = '\u{000D}';
/// Line Separator (LS) - Unicode oddelovac radku
pub const LB_LS: char = '\u{2028}';
/// Paragraph Separator (PS) - Unicode oddelovac odstavcu
pub const LB_PS: char = '\u{2029}';
/// Vsechny znaky zalomeni radku dle ECMAScript specifikace.
pub const LINE_BREAK_CHARS: &[char] = &[LB_LF, LB_CR, LB_LS, LB_PS];

// ─── Druhy tokenu ─────────────────────────────────────────────────────────────

/// Druh (typ) tokenu z lexikalni analyzy.
///
/// Pokryva vsechny lexikalni prvky ECMAScriptu:
/// literaly, klicova slova, operatory, template literaly, komentare a trivia.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // --- Literaly ---

    /// Identifikator: `foo`, `myVar`, `$element`, `_private`
    Identifier(String),

    /// Ciselny literal s plnou metadatou.
    ///
    /// - `raw` - surovy text ze zdrojaku (`"0xFF"`, `"1_000"`)
    /// - `value` - vypocitana f64 hodnota
    /// - `bigint_value` - hodnota pro BigInt literaly (`42n`)
    /// - `base` - soustava (desetinna, hex, binarni, oktalova)
    /// - `legacy_octal` - `true` pro `0755` styl (zakazano v strict mode)
    /// - `is_bigint` - `true` pro `42n`
    /// - `has_exponent` - `true` pro `1e5`
    /// - `is_valid` - `false` pro syntakticky vadne literaly
    NumericLiteral {
        raw: String, value: f64, bigint_value: Option<BigInt>,
        base: NumericBase, legacy_octal: bool,
        is_bigint: bool, has_exponent: bool, is_valid: bool,
    },

    /// Retezec: `"hello"` nebo `'world'`
    ///
    /// - `value` - rozbalena hodnota (escape sekvence prevedeny)
    /// - `raw` - surovy text vcetne uvozovek
    /// - `escapes` - seznam rozpoznanych escape sekvenci pro analyzu
    StringLiteral { value: String, raw: String, escapes: Vec<EscapeInfo> },

    /// Regularni vyraz: `/pattern/flags`
    RegexLiteral { pattern: String, flags: String },

    // --- Template literaly (dle ECMAScript spec) ---
    //
    // Template literal je rozlozena na casti podle vyrazu uvnitr:
    //
    // `abc`           -> NoSubstitutionTemplate("abc")
    // `abc${          -> TemplateHead("abc")
    // }def${          -> TemplateMiddle("def")
    // }ghi`           -> TemplateTail("ghi")
    //
    // Hodnota v kazde variante je text quasis (mezi backticky/vyrazy),
    // s vyresenenymi escape sekvencemi.

    /// Cela template bez vyrazu: `` `hello world` ``
    NoSubstitutionTemplate(String),
    /// Zacatek template s vyrazem: `` `hello ${ ``
    TemplateHead(String),
    /// Prostredni cast template: `} world ${`
    TemplateMiddle(String),
    /// Konec template: `} !` ``
    TemplateTail(String),

    /// Klicove slovo: `if`, `function`, `return`, atd.
    Keyword(KeywordEnum),

    /// Operator nebo interpunkce: `+`, `===`, `=>`, `{`, atd.
    Operator(OperatorEnum),

    // --- Komentare ---

    /// Radkovy komentar: `// text`  (hodnota = text bez `//`)
    CommentLine(String),
    /// Blokovy komentar: `/* text */` (hodnota = text bez `/*` a `*/`)
    CommentBlock(String),
    /// Zacatek HTML komentare: `<!--` (pro compat rezim)
    HtmlCommentStart,

    // --- Trivia (ignorovana pri parsovani) ---

    /// Bile znaky (mezery, tabulatory) - dle ECMAScript bileznaky
    Whitespace,
    /// Zalomeni radku (LF, CR, LS, PS)
    Newline,
    /// Konec souboru
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

/// Druh escape sekvence v retezci.
#[derive(Debug, Clone, PartialEq)]
pub enum EscapeKind {
    /// Oktalova: `\077`
    Octal,
    /// Hexadecimalni: `\xFF`
    Hex,
    /// Unicode: `\uXXXX` nebo `\u{XXXXX}`
    Unicode,
    /// Jednoducha: `\n`, `\t`, `\\`, atd.
    Simple,
}

/// Informace o jedne escape sekvenci v retezci.
///
/// Pouziva se pro analyzu, validaci a source mapy.
#[derive(Debug, Clone, PartialEq)]
pub struct EscapeInfo {
    /// Druh escape sekvence
    pub kind: EscapeKind,
    /// Surovy text escape sekvence (vcetne `\`)
    pub raw: String,
    /// Prevedeny znak
    pub resolved_char: char,
    /// Pozice escape sekvence v surovem retezci
    pub position_in_raw: usize,
}

/// Ciselna soustava literalu.
#[derive(Debug, PartialEq, Clone)]
pub enum NumericBase {
    /// `42`, `3.14` - zakladni soustava
    Decimal,
    /// `0xFF` - sestnactkova
    Hex,
    /// `0b1010` - dvojkova
    Binary,
    /// `0o77` nebo legacy `077` - osmickova
    Octal,
}

/// Druh komentare (pouziva se pri lexikalni analyze, ne ve finalnich tokenech).
#[derive(Debug, PartialEq)]
pub enum CommentKind { SingleLine, MultiLine, None }

/// Druh uvozovek retezce (pouziva se pri lexikalni analyze).
#[derive(Debug, PartialEq)]
pub enum StringKind { DoubleQuote, SingleQuote, TemplateString, None }

// ─── Token ────────────────────────────────────────────────────────────────────

/// Jeden token ze zdrojoveho kodu.
///
/// Obsahuje druh tokenu, surovy text a pozici ve zdrojovem souboru.
/// Pozice jsou dulesita pro chybove hlasky a source mapy.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Druh tokenu s pridruzenou hodnotou
    pub kind: TokenKind,
    /// Surovy text tokenu ze zdrojoveho kodu
    pub lexeme: String,
    /// Byte-offset zacatku tokenu v souboru
    pub start: usize,
    /// Byte-offset konce tokenu v souboru (exkluzivni)
    pub end: usize,
    /// Cislo radku (od 1)
    pub line: usize,
    /// Cislo sloupce (od 0)
    pub column: usize,
}

// ─── Klasifikacni metody ──────────────────────────────────────────────────────

impl Token {
    /// Vraci `true` kdyz je znak zalomeni radku dle ECMAScript.
    pub fn is_line_break(ch: char) -> bool {
        LINE_BREAK_CHARS.contains(&ch)
    }

    /// Vraci `true` kdyz je sekvence CR+LF (Windows konec radku).
    pub fn is_crlf(ch: char, next: Option<char>) -> bool {
        ch == LB_CR && next == Some(LB_LF)
    }

    /// Vraci `true` kdyz je znak bily znak dle ECMAScript specifikace.
    ///
    /// Zahrnuje: TAB (`\t`), VT (`\v`), FF (`\f`), ZWNBSP (BOM `﻿`)
    /// a vsechny Unicode znaky kategorie SpaceSeparator (Zs).
    pub fn is_white_space(ch: char) -> bool {
        matches!(ch, '\u{0009}' | '\u{000B}' | '\u{000C}' | '\u{FEFF}')
            || get_general_category(ch) == GeneralCategory::SpaceSeparator
    }

    /// Rozezna druh uvozovek pro zahajeni retezce nebo template literalu.
    pub fn is_string_start(ch: char) -> StringKind {
        match ch {
            '"'  => StringKind::DoubleQuote,
            '\'' => StringKind::SingleQuote,
            '`'  => StringKind::TemplateString,
            _    => StringKind::None,
        }
    }

    /// Vraci `true` kdyz muze byt znak zacatkem ciselneho literalu.
    ///
    /// Zahrnuje: cislice 0-9 a tecka nasledovana cislici (`.5`).
    pub fn is_number_start(ch: char, next: Option<char>) -> bool {
        if ch.is_ascii_digit() { return true; }
        ch == '.' && next.map(|c| c.is_ascii_digit()).unwrap_or(false)
    }

    /// Rozezna druh komentare pri vstupu `//` nebo `/*`.
    pub fn is_comment_start(ch: char, next: Option<char>) -> CommentKind {
        match (ch, next) {
            ('/', Some('/')) => CommentKind::SingleLine,
            ('/', Some('*')) => CommentKind::MultiLine,
            _                => CommentKind::None,
        }
    }

    /// Vraci `true` kdyz je prvni radek hashbang (`#!/usr/bin/env node`).
    pub fn is_hashbang(ch: char, next: Option<char>) -> bool {
        ch == '#' && next == Some('!')
    }

    /// Vraci `true` kdyz muze byt znak platnym zacatkem identifikatoru dle Unicode.
    ///
    /// Zahrnuje: `$`, `_`, a vsechny znaky s Unicode vlastnosti XID_Start.
    pub fn is_valid_identifier_start(ch: char) -> bool {
        ch == '$' || ch == '_' || is_xid_start(ch)
    }

    /// Vraci `true` kdyz muze byt znak platnym pokracovanim identifikatoru.
    ///
    /// Zahrnuje: `$` a vsechny znaky s Unicode vlastnosti XID_Continue.
    pub fn is_valid_identifier_continue(ch: char) -> bool {
        ch == '$' || is_xid_continue(ch)
    }

    /// Vraci `true` kdyz je znak platnym znakem jednoduche escape sekvence.
    ///
    /// Platne: `'`, `"`, `\`, `` ` ``, `b`, `f`, `n`, `r`, `t`, `v`, `0`
    pub fn is_single_escape_char(ch: char) -> bool {
        matches!(ch, '\'' | '"' | '\\' | '`' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | '0')
    }

    /// Vraci `true` kdyz je retezec klicovym slovem JavaScriptu.
    pub fn is_keyword(s: &str) -> bool { KeywordEnum::from_str(s).is_some() }

    /// Vraci odpovidajici `KeywordEnum` pro retezec, nebo `None`.
    pub fn get_keyword(s: &str) -> Option<KeywordEnum> { KeywordEnum::from_str(s) }

    /// Vraci `true` kdyz muze byt znak zacatkem operatoru.
    pub fn is_operator_start(ch: char) -> bool {
        matches!(ch,
            '+' | '-' | '*' | '/' | '%' | '.' | ',' | ':' | ';' | '?' |
            '~' | '!' | '=' | '&' | '|' | '^' | '<' | '>' |
            '(' | ')' | '{' | '}' | '[' | ']' | '#'
        )
    }

    /// Vraci odpovidajici `OperatorEnum` pro retezec operatoru, nebo `None`.
    pub fn get_operator(s: &str) -> Option<OperatorEnum> { OperatorEnum::from_str(s) }
}

// ─── Klicova slova (lowercase - tak jak jsou ve zdrojaku JS) ──────────────────

// Vsechna klicova slova JavaScriptu/ESNext.
// Generovano makrem string_enum! -> prida as_str(), from_str(), Display.
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

// ─── Operatory (od nejdelsiho k nejkratsimu pro greedy matching) ──────────────

// Vsechny operatory a interpunkcni znaky JavaScriptu/ESNext.
// Poradi variant je dulezite: from_str zkusi delsi driv (greedy matching).
// Napr. "===" se spari pred "==", ">>>" pred ">>" pred ">".
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
