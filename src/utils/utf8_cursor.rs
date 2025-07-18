/*pub struct Utf8Cursor<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Utf8Cursor<'a> {
    pub fn new(s: &'a str) -> Self {
        Utf8Cursor {
            input: s.as_bytes(),
            pos: 0,
        }
    }

    pub fn from_bytes(input: &'a [u8]) -> Self {
        Utf8Cursor {
            input,
            pos: 0,
        }
    }


    pub fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Peeks the current character without advancing.
    pub fn peek(&self) -> Option<char> {
        let (ch, _) = self.peek_char_manual()?;
        Some(ch)
    }

    /// Peeks the current character and its UTF-8 byte length.
    pub fn peek_char_manual(&self) -> Option<(char, usize)> {
        let b0 = *self.input.get(self.pos)?;

        let (ch, len) = if b0 < 128 {
            (b0 as char, 1)
        } else if b0 & 0xE0 == 0xC0 {
            let b1 = *self.input.get(self.pos + 1)?;
            let code = (((b0 & 0x1F) as u32) << 6) | ((b1 & 0x3F) as u32);
            (char::from_u32(code)?, 2)
        } else if b0 & 0xF0 == 0xE0 {
            let b1 = *self.input.get(self.pos + 1)?;
            let b2 = *self.input.get(self.pos + 2)?;
            let code = (((b0 & 0x0F) as u32) << 12)
                | (((b1 & 0x3F) as u32) << 6)
                | ((b2 & 0x3F) as u32);
            (char::from_u32(code)?, 3)
        } else if b0 & 0xF8 == 0xF0 {
            let b1 = *self.input.get(self.pos + 1)?;
            let b2 = *self.input.get(self.pos + 2)?;
            let b3 = *self.input.get(self.pos + 3)?;
            let code = (((b0 & 0x07) as u32) << 18)
                | (((b1 & 0x3F) as u32) << 12)
                | (((b2 & 0x3F) as u32) << 6)
                | ((b3 & 0x3F) as u32);
            (char::from_u32(code)?, 4)
        } else {
            return None;
        };

        Some((ch, len))
    }

    /// Advances and returns the next character.
    pub fn advance(&mut self) -> Option<char> {
        let (ch, len) = self.peek_char_manual()?;
        self.pos += len;
        Some(ch)
    }

    /// Peek n-th character ahead (0 = current).
    pub fn peek_n(&self, n: usize) -> Option<char> {
        let mut i = self.pos;
        let mut count = 0;

        while count < n {
            let (_, len) = Utf8Cursor {
                input: self.input,
                pos: i,
            }
                .peek_char_manual()?;
            i += len;
            count += 1;
        }

        Utf8Cursor {
            input: self.input,
            pos: i,
        }
            .peek()
    }

    /// Returns current byte offset
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Resets back to a previous position (manual rewind)
    pub fn reset_to(&mut self, pos: usize) {
        self.pos = pos;
    }
    
    pub fn undo(&mut self) {
        self.pos -= 1;
    }
}*/

pub struct Utf8Cursor {
    input: Vec<u8>, // Vlastnictví dat
    pos: usize,     // Aktuální pozice
}

impl Utf8Cursor {
    pub fn new(s: &str) -> Self {
        Utf8Cursor {
            input: s.as_bytes().to_vec(),
            pos: 0,
        }
    }
    
    pub fn from_string(s: String) -> Self {
        Utf8Cursor {
            input: s.into_bytes(),
            pos: 0,
        }
    }

    pub fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    pub fn peek(&self) -> Option<char> {
        self.peek_char_manual_at(self.pos).map(|(ch, _)| ch)
    }

    pub fn peek_n(&self, n: usize) -> Option<char> {
        let mut i = self.pos;
        let mut count = 0;

        while count < n {
            let (_, len) = self.peek_char_manual_at(i)?;
            i += len;
            count += 1;
        }

        self.peek_char_manual_at(i).map(|(ch, _)| ch)
    }

    /// Advances and returns the next character.
    pub fn advance(&mut self) -> Option<char> {
        let (ch, len) = self.peek_char_manual_at(self.pos)?;
        self.pos += len;
        Some(ch)
    }

    /// Returns current byte offset
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Resets back to a previous position (manual rewind)
    pub fn reset_to(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn undo(&mut self) {
        self.pos -= 1;
    }

    fn peek_char_manual_at(&self, pos: usize) -> Option<(char, usize)> {
        let b0 = *self.input.get(pos)?;

        let (ch, len) = if b0 < 128 {
            (b0 as char, 1)
        } else if b0 & 0b1110_0000 == 0b1100_0000 {
            // 2 bajty, 110xxxxx 10xxxxxx
            let b1 = *self.input.get(pos + 1)?;
            
            let code = (((b0 & 0b0001_1111) as u32) << 6) 
                | ((b1 & 0b0011_1111) as u32);
            
            (char::from_u32(code)?, 2)
            
        } else if b0 & 0b1111_0000 == 0b1110_0000 {
            // 3 bajty, 1110xxxx 10xxxxxx 10xxxxxx
            let b1 = *self.input.get(pos + 1)?;
            let b2 = *self.input.get(pos + 2)?;
            
            let code = (((b0 & 0b0000_1111) as u32) << 12)
                | (((b1 & 0b0011_1111) as u32) << 6)
                | ((b2 & 0b0011_1111) as u32);
            
            (char::from_u32(code)?, 3)
        } else if b0 & 0b1111_1000 == 0b1111_0000 {
            // 4 bajty, 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
            let b1 = *self.input.get(pos + 1)?;
            let b2 = *self.input.get(pos + 2)?;
            let b3 = *self.input.get(pos + 3)?;
            
            let code = (((b0 & 0b0000_0111) as u32) << 18)
                | (((b1 & 0b0011_1111) as u32) << 12)
                | (((b2 & 0b0011_1111) as u32) << 6)
                | ((b3 & 0b0011_1111) as u32);
            
            (char::from_u32(code)?, 4)
        } else {
            return None;
        };

        Some((ch, len))
    }
}
