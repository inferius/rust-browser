/// Kurzor pro čtení UTF-8 řetězce znak po znaku.
///
/// # Opravená chyba oproti originálu
/// `undo()` nyní sleduje počet bajtů posledního `advance()`,
/// takže funguje správně i pro vícebajtové UTF-8 znaky (ě, č, š, 😀, …).
pub struct Utf8Cursor {
    input: Vec<u8>,
    pos: usize,
    /// Počet bajtů které spotřeboval poslední `advance()`.
    last_len: usize,
}

impl Utf8Cursor {
    pub fn new(s: &str) -> Self {
        Self { input: s.as_bytes().to_vec(), pos: 0, last_len: 0 }
    }

    pub fn from_string(s: String) -> Self {
        Self { input: s.into_bytes(), pos: 0, last_len: 0 }
    }

    /// Konec vstupu?
    pub fn eof(&self) -> bool { self.pos >= self.input.len() }

    /// Aktuální znak (bez posunutí).
    pub fn peek(&self) -> Option<char> {
        self.char_at(self.pos).map(|(ch, _)| ch)
    }

    /// N-tý znak od aktuální pozice (0 = peek).
    pub fn peek_n(&self, n: usize) -> Option<char> {
        let mut i = self.pos;
        for _ in 0..n {
            let (_, len) = self.char_at(i)?;
            i += len;
        }
        self.char_at(i).map(|(ch, _)| ch)
    }

    /// Přečte a vrátí aktuální znak, posune kurzor.
    pub fn advance(&mut self) -> Option<char> {
        let (ch, len) = self.char_at(self.pos)?;
        self.last_len = len;
        self.pos += len;
        Some(ch)
    }

    /// Vrátí se zpět o jeden znak (přesně o počet bajtů posledního advance).
    pub fn undo(&mut self) {
        debug_assert!(self.last_len > 0, "undo() bez předchozího advance()");
        self.pos -= self.last_len;
        self.last_len = 0;
    }

    pub fn pos(&self) -> usize { self.pos }

    pub fn reset_to(&mut self, pos: usize) {
        self.pos = pos;
        self.last_len = 0;
    }

    // ── Interní UTF-8 dekodér ────────────────────────────────────────────────

    fn char_at(&self, pos: usize) -> Option<(char, usize)> {
        let b0 = *self.input.get(pos)?;
        let (code, len) = if b0 < 0x80 {
            (b0 as u32, 1usize)
        } else if b0 & 0xE0 == 0xC0 {
            let b1 = *self.input.get(pos + 1)?;
            (((b0 & 0x1F) as u32) << 6 | (b1 & 0x3F) as u32, 2)
        } else if b0 & 0xF0 == 0xE0 {
            let b1 = *self.input.get(pos + 1)?;
            let b2 = *self.input.get(pos + 2)?;
            (((b0 & 0x0F) as u32) << 12 | ((b1 & 0x3F) as u32) << 6 | (b2 & 0x3F) as u32, 3)
        } else if b0 & 0xF8 == 0xF0 {
            let b1 = *self.input.get(pos + 1)?;
            let b2 = *self.input.get(pos + 2)?;
            let b3 = *self.input.get(pos + 3)?;
            (((b0 & 0x07) as u32) << 18 | ((b1 & 0x3F) as u32) << 12
                 | ((b2 & 0x3F) as u32) << 6 | (b3 & 0x3F) as u32, 4)
        } else {
            return None;
        };
        Some((char::from_u32(code)?, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrip() {
        let mut c = Utf8Cursor::new("abc");
        assert_eq!(c.advance(), Some('a'));
        assert_eq!(c.advance(), Some('b'));
        c.undo();
        assert_eq!(c.advance(), Some('b'));
        assert_eq!(c.advance(), Some('c'));
        assert!(c.eof());
    }

    #[test]
    fn multibyte_undo() {
        let mut c = Utf8Cursor::new("aě");
        c.advance(); // 'a' (1 byte)
        let ch = c.advance(); // 'ě' (2 bytes)
        assert_eq!(ch, Some('ě'));
        c.undo(); // must go back 2 bytes, not 1
        assert_eq!(c.peek(), Some('ě'));
    }
}
