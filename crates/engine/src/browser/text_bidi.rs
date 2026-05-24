//! Unicode Bidirectional Algorithm (UAX #9) foundation.
//!
//! Spec: https://www.unicode.org/reports/tr9/
//! Used pri text shaping: RTL (Arabic/Hebrew) text inside LTR document and vice versa.
//!
//! Output: visual ordering of glyphs from logical character order.
//! Step 1: assign bidi classes (L/R/AL/EN/AN/ES/ET/CS/NSM/BN/B/S/WS/ON + isolates).
//! Step 2: paragraph base direction (P1, P2, P3).
//! Step 3: explicit + implicit reorder levels.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BidiClass {
    L,    // strong left-to-right
    R,    // strong right-to-left (Hebrew)
    AL,   // strong arabic letter
    EN,   // european number
    ES,   // european separator (- +)
    ET,   // european terminator ($ % ° etc)
    AN,   // arabic number
    CS,   // common separator (, .)
    NSM,  // non-spacing mark
    BN,   // boundary neutral
    B,    // paragraph separator
    S,    // segment separator (TAB)
    WS,   // whitespace
    ON,   // other neutral
    LRE, RLE, LRO, RLO, PDF,    // explicit
    LRI, RLI, FSI, PDI,         // isolates
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParagraphDirection {
    Ltr,
    Rtl,
    Auto,    // P3: scan for first strong
}

pub fn classify_char(c: char) -> BidiClass {
    let cp = c as u32;
    match cp {
        0x0030..=0x0039 => BidiClass::EN,
        0x0041..=0x005A | 0x0061..=0x007A => BidiClass::L,
        0x0009 => BidiClass::S,
        0x000A | 0x000D | 0x001C..=0x001E | 0x0085 | 0x2028 | 0x2029 => BidiClass::B,
        0x0020 | 0x00A0 => BidiClass::WS,
        0x002B | 0x002D => BidiClass::ES,
        0x0023 | 0x0024 | 0x00A2..=0x00A5 | 0x0025 | 0x00B0 | 0x00B1 => BidiClass::ET,
        0x002C | 0x002E | 0x002F | 0x003A => BidiClass::CS,
        0x05BE | 0x05C0 | 0x05C3 | 0x05C6 | 0x05D0..=0x05EA | 0x05F0..=0x05F4 => BidiClass::R, // Hebrew
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFC => BidiClass::AL,
        0x0660..=0x0669 | 0x06F0..=0x06F9 => BidiClass::AN,
        0x202A => BidiClass::LRE,
        0x202B => BidiClass::RLE,
        0x202C => BidiClass::PDF,
        0x202D => BidiClass::LRO,
        0x202E => BidiClass::RLO,
        0x2066 => BidiClass::LRI,
        0x2067 => BidiClass::RLI,
        0x2068 => BidiClass::FSI,
        0x2069 => BidiClass::PDI,
        _ => BidiClass::L,
    }
}

/// Determine paragraph base direction. P2/P3: first strong char.
pub fn detect_base_direction(text: &str) -> ParagraphDirection {
    for c in text.chars() {
        match classify_char(c) {
            BidiClass::L => return ParagraphDirection::Ltr,
            BidiClass::R | BidiClass::AL => return ParagraphDirection::Rtl,
            _ => {}
        }
    }
    ParagraphDirection::Ltr
}

/// Compute embedding levels per UAX #9 (simplified - rule-set N0+I1+I2 for the common case).
/// Returns per-char level (even = LTR, odd = RTL).
pub fn compute_levels(text: &str, base: ParagraphDirection) -> Vec<u8> {
    let base_level: u8 = match base {
        ParagraphDirection::Rtl => 1,
        _ => 0,
    };
    text.chars().map(|c| {
        match classify_char(c) {
            BidiClass::L => 0,
            BidiClass::R | BidiClass::AL => 1,
            BidiClass::AN | BidiClass::EN => base_level.max(2),
            _ => base_level,
        }
    }).collect()
}

/// Visual ordering: reverse runs of odd-level chars (UAX #9 L2).
pub fn reorder_visual(text: &str, levels: &[u8]) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != levels.len() { return text.to_string(); }
    let max_level = *levels.iter().max().unwrap_or(&0);
    let mut out: Vec<char> = chars.clone();
    let mut idx_seq: Vec<usize> = (0..chars.len()).collect();
    for level in (1..=max_level).rev() {
        let mut i = 0;
        while i < idx_seq.len() {
            let mut j = i;
            while j < idx_seq.len() && levels[idx_seq[j]] >= level { j += 1; }
            if j > i {
                idx_seq[i..j].reverse();
            }
            i = j.max(i + 1);
        }
    }
    for (n, &orig) in idx_seq.iter().enumerate() {
        out[n] = chars[orig];
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_latin() {
        assert_eq!(classify_char('A'), BidiClass::L);
        assert_eq!(classify_char('1'), BidiClass::EN);
        assert_eq!(classify_char(' '), BidiClass::WS);
    }

    #[test]
    fn classify_hebrew() {
        assert_eq!(classify_char('\u{05D0}'), BidiClass::R);
    }

    #[test]
    fn classify_arabic() {
        assert_eq!(classify_char('\u{0628}'), BidiClass::AL);
    }

    #[test]
    fn detect_ltr_default() {
        assert_eq!(detect_base_direction("Hello"), ParagraphDirection::Ltr);
    }

    #[test]
    fn detect_rtl_from_hebrew() {
        assert_eq!(detect_base_direction("\u{05D0}\u{05D1}\u{05D2}"), ParagraphDirection::Rtl);
    }

    #[test]
    fn detect_skips_neutrals() {
        assert_eq!(detect_base_direction("   123 \u{05D0}A"), ParagraphDirection::Rtl);
    }

    #[test]
    fn levels_ascii_zero() {
        let l = compute_levels("Hello", ParagraphDirection::Ltr);
        assert!(l.iter().all(|&v| v == 0));
    }

    #[test]
    fn levels_hebrew_one() {
        let l = compute_levels("\u{05D0}\u{05D1}", ParagraphDirection::Rtl);
        assert!(l.iter().all(|&v| v == 1));
    }

    #[test]
    fn reorder_pure_rtl_reversed() {
        let text = "\u{05D0}\u{05D1}\u{05D2}";
        let l = compute_levels(text, ParagraphDirection::Rtl);
        let v = reorder_visual(text, &l);
        // Reversed character order
        let chars: Vec<char> = v.chars().collect();
        assert_eq!(chars[0], '\u{05D2}');
        assert_eq!(chars[2], '\u{05D0}');
    }
}
