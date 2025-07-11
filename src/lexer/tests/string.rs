use std::fmt::format;
use crate::lexer;
use crate::lexer::base::Lexer;
use crate::lexer::string::{EscapeSequenceKind, EscapeSequenceResult};
use crate::specifications::lexer_errors::{LexerError, LexerErrorKind};
use crate::tokens::TokenKind;
use crate::utils::utf8_cursor::Utf8Cursor;

fn escape_sequence_to_char(ch: Result<EscapeSequenceResult, LexerError>) -> char {
    if ch.is_err() {
        return '\0';
    }
    ch.unwrap().character
}

fn get_reader_from_string(input: &str, start_ch: char) -> Utf8Cursor {
    Utf8Cursor::from_string(format!("{1}{0}", start_ch, input))
}

#[test]
fn escape_sequence_unicode_test() {
    assert_eq!(escape_sequence_to_char(Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\u{1234}"), EscapeSequenceKind::Unicode)), 'ሴ');
    assert_eq!(escape_sequence_to_char(Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\u{1234}"), EscapeSequenceKind::All)), 'ሴ');
    assert_eq!(escape_sequence_to_char(Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\u1234"), EscapeSequenceKind::Unicode)), 'ሴ');

    assert_eq!(escape_sequence_to_char(Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\u12345"), EscapeSequenceKind::Unicode)), 'ሴ');

    let res = Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\u{1FFFFF}"), EscapeSequenceKind::Unicode);

    if let Err(e) = res {
        assert_eq!(e.kind, LexerErrorKind::InvalidEscapeSequence)
    }

}

#[test]
fn escape_sequence_hex_test() {
    let mut lexer = Lexer::new();


}

#[test]
fn escape_sequence_octal_test() {
    assert_eq!(escape_sequence_to_char(Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\123"), EscapeSequenceKind::Octal)), 'S');
    assert_eq!(escape_sequence_to_char(Lexer::read_escape_sequence(&mut Utf8Cursor::new("\\1234"), EscapeSequenceKind::Octal)), 'S');
}

#[test]
fn base_string_test() {
    let mut lexer = Lexer::new();

    let res = lexer.read_string(&mut get_reader_from_string("hello\\u{1234}\\1234 world", '"'), '"');
    if res.is_ok() {
        let unwrap = res.unwrap();
        assert_eq!(unwrap.len(), 1);
        
        if let Some(token) = unwrap.get(0) {
            if let TokenKind::StringLiteral { value,.. } = &token.kind {
                assert_eq!(value, "\"helloሴS4 world\"")
            }
        }
    }
    else {
        panic!("{:?}", res);
    }
}