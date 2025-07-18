use std::fmt;
use num_bigint::BigInt;
use crate::string_enum;
use unicode_ident::is_xid_start;
use unicode_ident::is_xid_continue;
use unicode_general_category::{get_general_category, GeneralCategory};
use crate::tokens::TokenKind::Operator;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // === Literály ===
    Identifier(String),          // např. `x`, `myVar`, `_$foo`
    NumericLiteral {
        raw: String,
        value: f64,
        bigint_value: Option<BigInt>,
        base: NumericBase,
        legacy_octal: bool, // pokud je definovany jako 0123 mist 0o123
        is_bigint: bool,
        has_exponent: bool,
        is_valid: bool,
    },       // např. `123`, `3.14`, `0xFF`, `0b1010`
    StringLiteral {
        value: String,
        raw: String,
        escapes: Vec<EscapeInfo>,
    },       // např. `"text"` nebo `'text'`
    TemplateLiteral(String),     // např. `` `template` ``
    RegexLiteral(String),        // např. `/abc/i`,
    Keyword(KeywordEnum),
    Operator(OperatorEnum),


    // === Template parsing specifika ===
    TemplateStart,           // "`" – začátek template literal
    TemplateMiddle,          // střední část s `${`
    TemplateEnd,             // "`" – konec template literal
    DollarCurlyOpen,         // "${"

    // === Speciální tokeny ===
    CommentLine(String),     // "// ..."
    CommentBlock(String),    // "/* ... */"
    HtmlCommentStart,        // "<!--"
    HtmlCommentEnd,          // "-->"

    // === Interní ===
    Whitespace,              // " ", "\t", atd. – můžeš ignorovat
    Newline,                 // "\n" nebo "\r\n"
    Eof,                     // konec souboru
    Error(String),           // chybová zpráva (neplatný token)
}

// Implementace pro `TokenKind` (aby byl použitelný ve `Token`)
impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenKind::Identifier(name) => write!(f, "Identifier({})", name),
            TokenKind::StringLiteral { value, .. } => write!(f, "StringLiteral({})", value),
            TokenKind::NumericLiteral { value, is_bigint, bigint_value, .. } => {
                let bigint_suffix = if *is_bigint { "n" } else { "" };
                let value = if *is_bigint { bigint_value.clone().unwrap().to_str_radix(10) } else { value.to_string() };

                write!(f, "NumericLiteral({}{})", value, bigint_suffix)
            },
            _ => write!(f, "{:?}", self) // Ladicí výpis pro ostatní typy
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum EscapeKind {
    Octal,
    Hex,
    Unicode,
    Simple
}

#[derive(Debug, Clone, PartialEq)]
pub struct EscapeInfo {
    pub kind: EscapeKind,
    pub raw: String,
    pub resolved_char: char,
    pub position_in_raw: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub enum NumericBase {
    Decimal,
    Hex,
    Binary,
    Octal,
}

#[derive(Debug, PartialEq)]
pub enum CommentKind {
    SingleLine,
    MultiLine,
    None
}

#[derive(Debug, PartialEq)]
pub enum StringKind {
    DoubleQuote,
    SingleQuote,
    TemplateString,
    None
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,       // přesný úsek ze vstupu
    pub start: usize,          // bajtová pozice ve vstupu
    pub end: usize,            // bajtová pozice konce (ne nutně inclusive)
    pub line: usize,           // řádek (pro chyby)
    pub column: usize,         // sloupec (pro chyby)
}

impl Token {
     pub fn is_line_break(ch: char) -> bool {
         if LINE_BREAKS_CHARS.contains(&ch) {
             return true;
         }
         false
     }

    const LB_LF: char = '\u{000A}'; /* \n <LF> */
    const LB_CR: char = '\u{000D}'; /* \r <CR> */
    const LB_LS: char = '\u{2028}'; /* <LS> */
    const LB_PS: char = '\u{2029}'; /* <PS> */

    pub fn is_multichar_line_break_sequence(ch: char, next_ch: Option<char>) -> bool {
        if ch == LB_CR {
            if next_ch.is_none() {
                return false;
            }
            if next_ch.unwrap() == LB_LF {
                return true;
            }
        }

        false
    }

    pub fn is_string_start(ch: char) -> StringKind {
        if ch == '"' {
            return StringKind::DoubleQuote;
        }
        else if ch == '\'' {
            return StringKind::SingleQuote;
        }
        else if ch == '`' {
            return StringKind::TemplateString;
        }

        StringKind::None
    }

    pub fn is_number_start(ch: char, next_ch: Option<char>) -> bool {
        if NUMBER_START_CHARS.contains(&ch) {
            if ch == '.' && (next_ch.is_none() || !ONLY_NUMBER_CHARS.contains(&next_ch.unwrap())) {
                return false;
            }
            return true;
        }
        false
    }

    pub fn is_number(ch: char) -> bool {
        if NUMBER_CHARS.contains(&ch) {
            return true;
        }
        false
    }

    pub fn is_white_space(ch: char) -> bool {
        if WHITESPACE_CHARS.contains(&ch) && get_general_category(ch) == GeneralCategory::SpaceSeparator {
            return true;
        }
        false
    }

    pub fn is_comment_start(ch: char, next_char: Option<char>) -> CommentKind {
        if next_char.is_none() {
            return CommentKind::None;
        }
        if ch == '/' && next_char.unwrap() == '/' {
            return CommentKind::SingleLine;
        }
        else if ch == '/' && next_char.unwrap() == '*' {
            return CommentKind::MultiLine;
        }
        CommentKind::None
    }

    pub fn is_hashbang(ch: char, next_char: Option<char>) -> bool {
        if ch == '#' && next_char.is_some() && next_char.unwrap() == '!' {
            return true;
        }
        false
    }

    pub fn is_comment_end(ch: char, next_char: Option<char>) -> CommentKind {
        if next_char.is_none() {
            return CommentKind::None;
        }
        if ch == '*' && next_char.unwrap() == '/' {
            CommentKind::MultiLine
        }
        else if LINE_BREAKS_CHARS.contains(&ch) {
            CommentKind::SingleLine
        }
        else {
            CommentKind::None
        }
    }

    pub fn is_valid_identifier_start(ch: char) -> bool {
        if is_xid_start(ch) || ch == '$' || ch == '_' {
            return true;
        }
        false
    }

    pub fn is_valid_identifier_continue(ch: char) -> bool {
        if is_xid_continue(ch) || ch == '$' {
            return true;
        }
        false
    }

    pub fn is_single_escape_char(ch: char) -> bool {
        if SINGLE_ESCAPE_CHARS.contains(&ch) {
            return true;
        }
        false
    }

    pub fn is_keyword(buffer: &String) -> bool {
        KeywordEnum::from_str(buffer).is_some()
    }

    pub fn get_keyword(buffer: &String) -> Option<KeywordEnum> {
        KeywordEnum::from_str(buffer)
    }

    pub fn is_operator_start(ch: char) -> bool {
        let str = ch.to_string();
        if OperatorEnum::from_str(&str).is_some() {
            return true;
        }
        false
    }

    pub fn is_operator(buffer: &String) -> bool {
        OperatorEnum::from_str(buffer).is_some()
    }

    pub fn get_operator(buffer: &String) -> Option<OperatorEnum> {
        OperatorEnum::from_str(buffer)
    }
}

const LB_LF: char = '\u{000A}'; /* \n <LF> */
const LB_CR: char = '\u{000D}'; /* \r <CR> */
const LB_LS: char = '\u{2028}'; /* <LS> */
const LB_PS: char = '\u{2029}'; /* <PS> */

const LINE_BREAKS_CHARS: &[char] = &[LB_LF, LB_CR, LB_LS, LB_PS];
const NUMBER_START_CHARS: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.'];
const NUMBER_CHARS: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', 'e', 'E'];
const ONLY_NUMBER_CHARS: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
const WHITESPACE_CHARS: &[char] = &['\u{0009}', '\u{000B}', '\u{000C}', '\u{FEFF}'];
const SINGLE_ESCAPE_CHARS: &[char] = &['\'', '"', '\\', 'b', 'f', 'n', 'r', 't', 'v'];


string_enum! {
    KeywordEnum,
    Break,
    Case,
    Catch,
    Get,
    Set,
    Class,
    Const,
    Continue,
    Debugger,
    Default,
    Delete,
    Do,
    Else,
    Export,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    In,
    Instanceof,
    New,
    Return,
    Super,
    Switch,
    This,
    Throw,
    Try,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
    Implements,
    Interface,
    Let,
    Package,
    Private,
    Protected,
    Public,
    Static,
    Await,
    Async,
    True,
    False,
    Null
}

string_enum! {
    OperatorEnum,

    // Jednoznakové operátory
    Plus             => "+",
    Minus            => "-",
    Star             => "*",
    Slash            => "/",
    Percent          => "%",
    Punctuator       => ".",
    Comma            => ",",
    Colon            => ":",
    Semicolon        => ";",
    Question         => "?",
    Tilde            => "~",
    Exclamation      => "!",
    Equal            => "=",
    Ampersand        => "&",
    Pipe             => "|",
    Caret            => "^",
    LessThan         => "<",
    GreaterThan      => ">",
    LeftParen        => "(",
    RightParen       => ")",
    LeftBrace        => "{",
    RightBrace       => "}",
    LeftBracket      => "[",
    RightBracket     => "]",
    Backtick         => "`",
    Dollar           => "$",
    Sharp            => "#",

    // Dvouzankové operátory
    EqualEqual              => "==",
    NotEqual                => "!=",
    LessThanEqual           => "<=",
    GreaterThanEqual        => ">=",
    LogicalAnd              => "&&",
    LogicalOr               => "||",
    PlusPlus                => "++",
    MinusMinus              => "--",
    ShiftLeft               => "<<",
    ShiftRight              => ">>",
    AssignAdd               => "+=",
    AssignSub               => "-=",
    AssignMul               => "*=",
    AssignDiv               => "/=",
    AssignMod               => "%=",
    AssignAnd               => "&=",
    AssignOr                => "|=",
    AssignXor               => "^=",
    AssignShiftLeft         => "<<=",
    AssignShiftRight        => ">>=",

    // Tříznakové operátory
    StrictEqual              => "===",
    StrictNotEqual           => "!==",
    ShiftRightUnsigned       => ">>>",
    Exponent                 => "**",
    AssignExponent           => "**=",
    Arrow                    => "=>",
    Ellipsis                 => "...",

    // Čtyřznakové operátory
    AssignShiftRightUnsigned => ">>>="

}