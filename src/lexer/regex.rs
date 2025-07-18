use crate::lexer::base::Lexer;

impl Lexer {
    pub fn is_valid_unicode_codepoint(s: &str) -> bool {
        if let Some(hex) = s.strip_prefix("0x") {
            if let Ok(value) = u32::from_str_radix(hex, 16) {
                return char::from_u32(value).is_some();
            }
        }
        false
    }

    pub fn get_unicode_codepoint(s: &str) -> Result<char, ()> {
        if let Some(hex) = s.strip_prefix("0x") {
            if let Ok(value) = u32::from_str_radix(hex, 16) {
                let ch = char::from_u32(value);
                if ch.is_some() {
                    return Ok(ch.unwrap());
                }
            }
        }
        Err(())
    }
}


#[cfg(test)]
#[path = "tests/regex.rs"]
mod regex;