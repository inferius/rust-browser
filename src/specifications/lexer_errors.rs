use crate::tokens::NumericBase;

#[derive(Debug, Clone, PartialEq)]
pub enum LexerErrorKind {
    InvalidDigit {
        base: NumericBase,
        found: char,
    },
    UnexpectedCharacter {
        found: char,
    },
    UnterminatedString,
    UnterminatedTemplate,
    UnterminatedComment,
    InvalidEscapeSequence,
    UnexpectedEOF,
    InvalidBigInt {
        reason: String,
    },
    LegacyOctalInStrictMode,
    UnexpectedNumber,
    UnexpectedToken,
}

#[derive(Debug, Clone)]
pub struct LexerError {
    pub kind: LexerErrorKind,
    pub span: Span, // e.g. start + end index, or line/col
}


#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl std::fmt::Display for LexerErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use LexerErrorKind::*;
        match self {
            InvalidDigit { base, found } => write!(f, "invalid digit '{}' for base {:?}", found, base),
            UnexpectedNumber { } => write!(f, "unexpected number"),
            UnexpectedCharacter { found } => write!(f, "unexpected character '{}'", found),
            UnterminatedString => write!(f, "unterminated string literal"),
            UnterminatedTemplate => write!(f, "unterminated template literal"),
            UnterminatedComment => write!(f, "unterminated comment"),
            InvalidEscapeSequence => write!(f, "invalid escape sequence"),
            UnexpectedEOF => write!(f, "unexpected end of input"),
            InvalidBigInt { reason } => write!(f, "invalid BigInt literal: {}", reason),
            LegacyOctalInStrictMode => write!(f, "legacy octal literal not allowed in strict mode"),
            UnexpectedToken => write!(f, "unexpected token"),
        }
    }
}
