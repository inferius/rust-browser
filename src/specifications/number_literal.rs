pub struct NumberLiteral;

const NON_ZERO_DIGITS: [char;9] = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];
const HEX_DIGITS: [char;22] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'A', 'B', 'C', 'D', 'E', 'F'];

impl NumberLiteral {
    pub fn is_non_zero_digit(c: char) -> bool {
        NON_ZERO_DIGITS.contains(&c)
    }

    pub fn is_decimal_digit(c: char) -> bool {
        NumberLiteral::is_non_zero_digit(c) || c == '0'
    }

    pub fn is_exponent_indicator(c: char) -> bool {
        c == 'e' || c == 'E'
    }

    pub fn is_binary_digit(c: char) -> bool {
        c == '0' || c == '1'
    }

    pub fn is_octal_digit(c: char) -> bool {
        c >= '0' && c <= '7'
    }

    pub fn is_non_octal_digit(c: char) -> bool {
        c == '8' || c == '7'
    }

    pub fn is_hex_digit(c: char) -> bool {
        HEX_DIGITS.contains(&c)
    }

    pub fn is_exponent_sign(c: char) -> bool {
        c == '+' || c == '-'
    }

    pub fn is_binary_start(c: char) -> bool {
        c == 'b' || c == 'B'
    }

    pub fn is_hex_start(c: char) -> bool {
        c == 'x' || c == 'X'
    }

    pub fn is_octal_start(c: char) -> bool {
        c == 'o' || c == 'O'
    }

    pub fn is_exponent_start(c: char) -> bool {
        c == 'e' || c == 'E'
    }

    pub fn is_decimal_separator(c: char) -> bool {
        c == '.'
    }

    pub fn is_numeric_literal_separator(c: char) -> bool {
        c == '_'
    }
    
    pub fn is_bigint_suffix(c: char) -> bool {
        c == 'n'
    }

}