pub trait AdvancedStringMethods {
    fn substring(&self, start: usize, end: usize) -> String;
    fn substring_start(&self, count: usize) -> String;
    fn substring_end(&self, count: usize) -> String;
    fn pad_left(&self, length: usize, pad_char: char) -> String;
    fn pad_right(&self, length: usize, pad_char: char) -> String;
}

impl AdvancedStringMethods for str {
    fn substring(&self, start: usize, end: usize) -> String {
        self.chars().skip(start).take(end - start).collect()
    }

    fn substring_start(&self, count: usize) -> String {
        self.chars().skip(count).collect()
    }

    fn substring_end(&self, count: usize) -> String {
        let total_chars = self.chars().count();
        if count >= total_chars {
            return String::new(); // Pokud count >= délka, vrátí prázdný řetězec.
        }
        self.chars().take(total_chars - count).collect()
    }

    fn pad_left(&self, length: usize, pad_char: char) -> String {
        let current_length = self.chars().count();
        if current_length >= length {
            return self.to_string(); // Pokud je řetězec delší, než požadovaná délka, nedoplní nic.
        }
        let padding = std::iter::repeat(pad_char).take(length - current_length).collect::<String>();
        format!("{}{}", padding, self) // Pad vlevo, původní řetězec doprava.
    }

    fn pad_right(&self, length: usize, pad_char: char) -> String {
        let current_length = self.chars().count();
        if current_length >= length {
            return self.to_string(); // Pokud je řetězec delší, než požadovaná délka, nedoplní nic.
        }
        let padding = std::iter::repeat(pad_char).take(length - current_length).collect::<String>();
        format!("{}{}", self, padding) // Původní řetězec vlevo, padovaný obsah doprava.
    }
}

impl AdvancedStringMethods for String {
    fn substring(&self, start: usize, end: usize) -> String {
        self.as_str().substring(start, end)
    }

    fn substring_start(&self, count: usize) -> String {
        self.as_str().substring_start(count)
    }

    fn substring_end(&self, count: usize) -> String {
        self.as_str().substring_end(count)
    }

    fn pad_left(&self, length: usize, pad_char: char) -> String {
        self.as_str().pad_left(length, pad_char)
    }

    fn pad_right(&self, length: usize, pad_char: char) -> String {
        self.as_str().pad_right(length, pad_char)
    }
}
