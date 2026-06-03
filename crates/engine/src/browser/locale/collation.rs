//! Intl.Collator - locale-aware string comparison.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollationSensitivity {
    Base,           // a == A, a != b
    Accent,         // a != á, a == A
    Case,           // a != A
    Variant,        // a != á, a != A
}

#[derive(Debug, Clone)]
pub struct CollatorOptions {
    pub locale: String,
    pub sensitivity: CollationSensitivity,
    pub ignore_punctuation: bool,
    pub numeric: bool,
    pub case_first: CaseFirst,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaseFirst {
    Upper,
    Lower,
    False,
}

impl Default for CollatorOptions {
    fn default() -> Self {
        Self {
            locale: "en-US".into(),
            sensitivity: CollationSensitivity::Variant,
            ignore_punctuation: false,
            numeric: false,
            case_first: CaseFirst::False,
        }
    }
}

pub fn compare(a: &str, b: &str, opts: &CollatorOptions) -> std::cmp::Ordering {
    let na = normalize(a, opts);
    let nb = normalize(b, opts);
    if opts.numeric {
        return natural_compare(&na, &nb);
    }
    na.cmp(&nb)
}

fn normalize(s: &str, opts: &CollatorOptions) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if opts.ignore_punctuation && c.is_ascii_punctuation() { continue; }
        let mut c2 = c;
        match opts.sensitivity {
            CollationSensitivity::Base => {
                // Drop case and diacritics (basic: strip combining marks; lowercase).
                c2 = strip_diacritic(c2).to_ascii_lowercase();
            }
            CollationSensitivity::Accent => {
                c2 = c2.to_ascii_lowercase();
            }
            CollationSensitivity::Case => {
                c2 = strip_diacritic(c2);
            }
            CollationSensitivity::Variant => {}
        }
        out.push(c2);
    }
    out
}

fn strip_diacritic(c: char) -> char {
    match c {
        '\u{00E1}' | '\u{00E0}' | '\u{00E2}' | '\u{00E4}' | '\u{00E3}' | '\u{00E5}' => 'a',
        '\u{00C1}' | '\u{00C0}' | '\u{00C2}' | '\u{00C4}' | '\u{00C3}' | '\u{00C5}' => 'A',
        '\u{00E9}' | '\u{00E8}' | '\u{00EA}' | '\u{00EB}' => 'e',
        '\u{00C9}' | '\u{00C8}' | '\u{00CA}' | '\u{00CB}' => 'E',
        '\u{00ED}' | '\u{00EC}' | '\u{00EE}' | '\u{00EF}' => 'i',
        '\u{00F3}' | '\u{00F2}' | '\u{00F4}' | '\u{00F6}' | '\u{00F5}' => 'o',
        '\u{00FA}' | '\u{00F9}' | '\u{00FB}' | '\u{00FC}' => 'u',
        '\u{0161}' => 's',
        '\u{017E}' => 'z',
        '\u{010D}' => 'c',
        '\u{0159}' => 'r',
        '\u{011B}' => 'e',
        '\u{016F}' => 'u',
        _ => c,
    }
}

/// Natural numeric compare: "a10" > "a9".
pub fn natural_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut i = 0; let mut j = 0;
    while i < a_chars.len() && j < b_chars.len() {
        let ca = a_chars[i]; let cb = b_chars[j];
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let mut num_a = String::new();
            while i < a_chars.len() && a_chars[i].is_ascii_digit() { num_a.push(a_chars[i]); i += 1; }
            let mut num_b = String::new();
            while j < b_chars.len() && b_chars[j].is_ascii_digit() { num_b.push(b_chars[j]); j += 1; }
            let na: u64 = num_a.parse().unwrap_or(0);
            let nb: u64 = num_b.parse().unwrap_or(0);
            match na.cmp(&nb) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }
        } else {
            match ca.cmp(&cb) {
                std::cmp::Ordering::Equal => { i += 1; j += 1; }
                ord => return ord,
            }
        }
    }
    a_chars.len().cmp(&b_chars.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn variant_distinguishes_case() {
        let opts = CollatorOptions::default();
        assert_eq!(compare("a", "A", &opts), Ordering::Greater);
    }

    #[test]
    fn base_ignores_case_and_accents() {
        let opts = CollatorOptions {
            sensitivity: CollationSensitivity::Base,
            ..Default::default()
        };
        assert_eq!(compare("\u{00E1}", "a", &opts), Ordering::Equal);
        assert_eq!(compare("A", "a", &opts), Ordering::Equal);
    }

    #[test]
    fn natural_orders_numbers() {
        assert_eq!(natural_compare("file2", "file10"), Ordering::Less);
        assert_eq!(natural_compare("file20", "file3"), Ordering::Greater);
    }

    #[test]
    fn ignore_punctuation() {
        let opts = CollatorOptions { ignore_punctuation: true, ..Default::default() };
        assert_eq!(compare("a.b", "ab", &opts), Ordering::Equal);
    }

    #[test]
    fn numeric_compare_via_options() {
        let opts = CollatorOptions { numeric: true, ..Default::default() };
        assert_eq!(compare("a10", "a9", &opts), Ordering::Greater);
    }
}
