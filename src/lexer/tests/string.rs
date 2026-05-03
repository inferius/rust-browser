use crate::lexer::base::Lexer;
use crate::lexer::string::EscapeResult;
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind};
use crate::tokens::{EscapeKind, TokenKind};
use crate::utils::utf8_cursor::Utf8Cursor;

// --- pomocne funkce ---

fn escape(src: &str) -> Result<EscapeResult, LexerError> {
    let mut lexer = Lexer::new();
    let mut cursor = Utf8Cursor::new(src);
    lexer.read_escape_sequence(&mut cursor, 0)
}

fn escape_char(src: &str) -> char {
    escape(src).expect("Ocekavana platna escape sekvence").character
}

fn escape_kind(src: &str) -> EscapeKind {
    escape(src).expect("Ocekavana platna escape sekvence").kind
}

// Precte string BEZ uvodni uvozovky (cursor uz je za ni), s uzaviraciuvozovkou na konci.
fn read_str_val(contents_with_closing_quote: &str, quote: char) -> String {
    let mut lexer = Lexer::new();
    let mut cursor = Utf8Cursor::new(contents_with_closing_quote);
    match lexer.read_string(&mut cursor, quote, 0).unwrap().kind {
        TokenKind::StringLiteral { value, .. } => value,
        other => panic!("Ocekavan StringLiteral, nalezeno {other:?}"),
    }
}

// Parsuje cely JS string literal vcetne uvodnich/zaviraci uvozovky.
fn parse_str(src: &str) -> String {
    let lexer = Lexer::parse_str(src, "<test>").unwrap();
    lexer.tokens.iter().find_map(|t| {
        if let TokenKind::StringLiteral { value, .. } = &t.kind {
            Some(value.clone())
        } else {
            None
        }
    }).expect("Token StringLiteral nenalezen")
}

// --- simple escape sekvence ---

#[test]
fn simple_escapes() {
    assert_eq!(escape_char("\\n"), '\n');
    assert_eq!(escape_char("\\r"), '\r');
    assert_eq!(escape_char("\\t"), '\t');
    assert_eq!(escape_char("\\b"), '\u{0008}');
    assert_eq!(escape_char("\\f"), '\u{000C}');
    assert_eq!(escape_char("\\v"), '\u{000B}');
    assert_eq!(escape_char("\\0"), '\0');
    assert_eq!(escape_char("\\'"), '\'');
    assert_eq!(escape_char("\\\""), '"');
    assert_eq!(escape_char("\\\\"), '\\');
    assert_eq!(escape_char("\\`"), '`');
}

#[test]
fn simple_escapes_kind() {
    assert_eq!(escape_kind("\\n"), EscapeKind::Simple);
    assert_eq!(escape_kind("\\'"), EscapeKind::Simple);
}

// --- hex escape (\xHH) ---

#[test]
fn hex_escape() {
    assert_eq!(escape_char("\\x41"), 'A');
    assert_eq!(escape_char("\\x61"), 'a');
    assert_eq!(escape_char("\\xFF"), '\u{00FF}');
    assert_eq!(escape_char("\\x00"), '\0');
    assert_eq!(escape_kind("\\x41"), EscapeKind::Hex);
}

#[test]
fn hex_escape_invalid() {
    let err = escape("\\xGG").unwrap_err();
    assert_eq!(err.kind, LexerErrorKind::InvalidEscapeSequence);
    let err2 = escape("\\x4").unwrap_err();
    assert_eq!(err2.kind, LexerErrorKind::InvalidEscapeSequence);
}

// --- unicode escape (\uHHHH a \u{H+}) ---

#[test]
fn unicode_4digit_escape() {
    assert_eq!(escape_char("\\u0041"), 'A');
    assert_eq!(escape_char("\\u1234"), '\u{1234}');
    assert_eq!(escape_char("\\uFFFF"), '\u{FFFF}');
    assert_eq!(escape_kind("\\u0041"), EscapeKind::Unicode);
}

#[test]
fn unicode_braced_escape() {
    assert_eq!(escape_char("\\u{41}"), 'A');
    assert_eq!(escape_char("\\u{1F600}"), '\u{1F600}');
    assert_eq!(escape_char("\\u{0}"), '\0');
    assert_eq!(escape_kind("\\u{41}"), EscapeKind::Unicode);
}

#[test]
fn unicode_braced_invalid() {
    let err = escape("\\u{1FFFFF}").unwrap_err();
    assert_eq!(err.kind, LexerErrorKind::InvalidEscapeSequence);
}

// --- octal escape (\1 az \377) ---

#[test]
fn octal_escape() {
    assert_eq!(escape_char("\\101"), 'A');  // 0o101 = 65
    assert_eq!(escape_char("\\123"), 'S');  // 0o123 = 83
    assert_eq!(escape_char("\\1"), '\u{0001}');
    assert_eq!(escape_kind("\\123"), EscapeKind::Octal);
}

#[test]
fn octal_escape_stops_at_3_digits() {
    // \1234 -> \123 = 83, '4' zustak v proudu
    let mut lexer = Lexer::new();
    let mut cursor = Utf8Cursor::new("\\1234");
    let res = lexer.read_escape_sequence(&mut cursor, 0).unwrap();
    assert_eq!(res.character, 'S');
    assert_eq!(cursor.peek(), Some('4')); // '4' nebylo spotrebovano
}

// --- read_string ---

#[test]
fn basic_double_quoted() {
    assert_eq!(read_str_val("hello world\"", '"'), "hello world");
    assert_eq!(read_str_val("\"", '"'), "");
}

#[test]
fn basic_single_quoted() {
    assert_eq!(read_str_val("hello'", '\''), "hello");
}

#[test]
fn string_with_escape_n() {
    assert_eq!(read_str_val("line1\\nline2\"", '"'), "line1\nline2");
}

#[test]
fn string_with_unicode_escape() {
    assert_eq!(read_str_val("\\u0041\"", '"'), "A");
}

#[test]
fn unterminated_string() {
    let mut lexer = Lexer::new();
    let mut cursor = Utf8Cursor::new("no closing quote");
    let err = lexer.read_string(&mut cursor, '"', 0).unwrap_err();
    assert_eq!(err.kind, LexerErrorKind::UnterminatedString);
}

#[test]
fn newline_in_string_is_error() {
    let mut lexer = Lexer::new();
    let mut cursor = Utf8Cursor::new("abc\nxyz\"");
    let err = lexer.read_string(&mut cursor, '"', 0).unwrap_err();
    assert_eq!(err.kind, LexerErrorKind::UnterminatedString);
}

// --- full lexer string parsing ---

#[test]
fn full_double_quoted() {
    assert_eq!(parse_str(r#""hello world""#), "hello world");
}

#[test]
fn full_single_quoted() {
    assert_eq!(parse_str("'hello'"), "hello");
}

#[test]
fn full_string_escapes() {
    assert_eq!(parse_str(r#""\n\t\\""#), "\n\t\\");
    assert_eq!(parse_str(r#""A""#), "A");
    assert_eq!(parse_str(r#""\x61""#), "a");
}
