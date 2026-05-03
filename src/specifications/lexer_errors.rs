use crate::tokens::NumericBase;

#[derive(Debug, Clone, PartialEq)]
pub enum LexerErrorKind {
    InvalidDigit { base: NumericBase, found: char },
    UnexpectedCharacter { found: char },
    UnterminatedString,
    UnterminatedTemplate,
    UnterminatedComment,
    InvalidEscapeSequence,
    UnexpectedEOF,
    InvalidBigInt { reason: String },
    UnexpectedNumber,
}

#[derive(Debug, Clone)]
pub struct LexerError {
    pub kind: LexerErrorKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct Span { pub start: usize, pub end: usize }

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chyba lexeru [{}..{}]: {}", self.span.start, self.span.end, self.kind)
    }
}

impl std::fmt::Display for LexerErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use LexerErrorKind::*;
        match self {
            InvalidDigit { base, found }    => write!(f, "neplatná číslice '{}' pro {:?}", found, base),
            UnexpectedCharacter { found }   => write!(f, "neočekávaný znak '{}'", found),
            UnterminatedString              => write!(f, "neukončený řetězec"),
            UnterminatedTemplate            => write!(f, "neukončený template literál"),
            UnterminatedComment             => write!(f, "neukončený komentář"),
            InvalidEscapeSequence           => write!(f, "neplatná escape sekvence"),
            UnexpectedEOF                   => write!(f, "neočekávaný konec souboru"),
            InvalidBigInt { reason }        => write!(f, "neplatný BigInt: {}", reason),
            UnexpectedNumber                => write!(f, "neočekávané číslo"),
        }
    }
}
