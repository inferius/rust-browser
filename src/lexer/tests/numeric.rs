use crate::lexer::*;
use crate::lexer::base::Lexer;
use crate::lexer::numeric::NumberResult;
use crate::utils::utf8_cursor::Utf8Cursor;

#[test]
fn test_binary_literal() {
    let mut lexer = Lexer::new();

    assert_eq!(Lexer::number_result_to_string(lexer.read_binary_literal(&mut Utf8Cursor::new("101010"), String::from("0b"), String::from(""), 0).unwrap()), "0b101010|101010|false");
    assert_eq!(Lexer::token_to_string(lexer.read_numeric_literal(&mut Utf8Cursor::new("0b101010")).unwrap()), "NumericLiteral(42)");
}

#[test]
fn test_hex_literal() {
    let mut lexer = Lexer::new();
    let data = [["0x123", "291"], ["0x123654", "1193556"], ["0x123654n", "1193556n"]];

    //assert_eq!(Lexer::number_result_to_string(lexer.read_hex_literal(&mut Utf8Cursor::new("101010"), String::from("0b"), String::from(""), 0).unwrap()), "0b101010|101010|false");
    //assert_eq!(Lexer::token_to_string(lexer.read_numeric_literal(&mut Utf8Cursor::new("101010")).unwrap()), "NumericLiteral(42)");
    for data in data {
        assert_eq!(Lexer::token_to_string(lexer.read_numeric_literal(&mut Utf8Cursor::new(data[0])).unwrap()), format!("NumericLiteral({})", data[1]));
    }
}

#[test]
fn test_octal_literal() {
    let mut lexer = Lexer::new();
    let data = [["0132", "90"], ["0o132", "90"]];

    //assert_eq!(Lexer::number_result_to_string(lexer.read_octal_literal(&mut Utf8Cursor::new("0132"), String::from("0b"), String::from(""), 0).unwrap()), "0b101010|101010|false");
    for data in data {
        assert_eq!(Lexer::token_to_string(lexer.read_numeric_literal(&mut Utf8Cursor::new(data[0])).unwrap()), format!("NumericLiteral({})", data[1]));
    }
}

#[test]
fn test_decimal_literal() {
    let mut lexer = Lexer::new();
    let data = [["1", "1"], ["1.2", "1.2"], [".1", "0.1"], ["1.2e3", "1200"], ["1.2e-3", "0.0012"], ["1.2e+3", "1200"]];

    //assert_eq!(Lexer::number_result_to_string(lexer.read_octal_literal(&mut Utf8Cursor::new("0132"), String::from("0b"), String::from(""), 0).unwrap()), "0b101010|101010|false");
    for data in data {
        assert_eq!(Lexer::token_to_string(lexer.read_numeric_literal(&mut Utf8Cursor::new(data[0])).unwrap()), format!("NumericLiteral({})", data[1]));
    }
}