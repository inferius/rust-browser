pub struct NumberLiteral;

impl NumberLiteral {
    pub fn is_decimal_digit(c: char) -> bool { c.is_ascii_digit() }
    pub fn is_hex_digit(c: char) -> bool { c.is_ascii_hexdigit() }
    pub fn is_octal_digit(c: char) -> bool { matches!(c, '0'..='7') }
    pub fn is_binary_digit(c: char) -> bool { c == '0' || c == '1' }
    pub fn is_non_octal_digit(c: char) -> bool { matches!(c, '8' | '9') }

    pub fn is_hex_start(c: char) -> bool { c == 'x' || c == 'X' }
    pub fn is_binary_start(c: char) -> bool { c == 'b' || c == 'B' }
    pub fn is_octal_start(c: char) -> bool { c == 'o' || c == 'O' }

    pub fn is_decimal_separator(c: char) -> bool { c == '.' }
    pub fn is_bigint_suffix(c: char) -> bool { c == 'n' }
    pub fn is_separator(c: char) -> bool { c == '_' }
    pub fn is_exponent(c: char) -> bool { c == 'e' || c == 'E' }
    pub fn is_exponent_sign(c: char) -> bool { c == '+' || c == '-' }
}
