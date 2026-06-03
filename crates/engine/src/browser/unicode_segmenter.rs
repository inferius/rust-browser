//! Unicode segmentation - grapheme/word/line/sentence breaks.
//!
//! Spec: UAX #14 (line break), UAX #29 (grapheme cluster, word break, sentence break).
//! Used pri text editing, selection, word-by-word cursor movement.

/// Grapheme cluster boundary detection - simplified GB rules.
/// Real impl would use the UCD property table; here we implement the common cases:
/// - CR LF stays together
/// - No break before NSM (combining mark)
/// - No break inside emoji ZWJ sequences
/// - No break around regional indicators (flags) in pairs
pub fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut prev_was_cr = false;
    let mut prev_was_ri = false;
    let mut ri_pair_open = false;
    for (i, c) in text.char_indices() {
        let cp = c as u32;
        let is_lf = cp == 0x000A;
        let is_cr = cp == 0x000D;
        let is_nsm = is_combining_mark(cp);
        let is_zwj = cp == 0x200D;
        let is_ri = (0x1F1E6..=0x1F1FF).contains(&cp);
        let is_extend = is_nsm || is_zwj;

        if i == 0 { prev_was_cr = is_cr; prev_was_ri = is_ri; ri_pair_open = is_ri; continue; }

        let mut do_break = true;
        if prev_was_cr && is_lf { do_break = false; }
        else if is_extend { do_break = false; }
        else if is_ri && prev_was_ri && !ri_pair_open { /* break between pairs */ }
        else if is_ri && prev_was_ri && ri_pair_open { do_break = false; ri_pair_open = false; }
        if do_break { boundaries.push(i); }
        prev_was_cr = is_cr;
        if is_ri && !prev_was_ri { ri_pair_open = true; }
        else if !is_ri { ri_pair_open = false; }
        prev_was_ri = is_ri;
    }
    boundaries.push(text.len());
    boundaries
}

pub fn is_combining_mark(cp: u32) -> bool {
    // Subset of Mark categories. Real impl walks UCD; we cover the common ranges.
    matches!(cp,
        0x0300..=0x036F |  // combining diacriticals
        0x0483..=0x0489 |  // cyrillic combining
        0x0591..=0x05BD |  // hebrew points
        0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7 |
        0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED |
        0x1AB0..=0x1AFF |  // combining extended
        0x1DC0..=0x1DFF |  // combining supplement
        0x20D0..=0x20FF |  // combining for symbols
        0xFE20..=0xFE2F    // combining half marks
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WordBoundaryKind {
    Word,
    Whitespace,
    Punctuation,
}

/// Find word boundary indices (start of each word run).
pub fn word_boundaries(text: &str) -> Vec<(usize, usize, WordBoundaryKind)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut current = WordBoundaryKind::Whitespace;
    let mut started = false;
    for (i, c) in text.char_indices() {
        let kind = classify_word_char(c);
        if !started { start = i; current = kind; started = true; continue; }
        if kind != current {
            out.push((start, i, current));
            start = i;
            current = kind;
        }
    }
    if started { out.push((start, text.len(), current)); }
    out
}

fn classify_word_char(c: char) -> WordBoundaryKind {
    if c.is_alphanumeric() || c == '_' || c == '\'' { WordBoundaryKind::Word }
    else if c.is_whitespace() { WordBoundaryKind::Whitespace }
    else { WordBoundaryKind::Punctuation }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineBreakOpportunity {
    Mandatory,    // \n
    Allowed,      // space, hyphen, between CJK ideographs
    Prohibited,
}

/// UAX #14 simplified line break detection.
pub fn line_break_opportunities(text: &str) -> Vec<(usize, LineBreakOpportunity)> {
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        let op = match c {
            '\n' | '\u{2028}' | '\u{2029}' => LineBreakOpportunity::Mandatory,
            ' ' | '\t' | '\u{00A0}' => LineBreakOpportunity::Allowed,
            '-' => LineBreakOpportunity::Allowed,
            c if (0x4E00..=0x9FFF).contains(&(c as u32)) => LineBreakOpportunity::Allowed, // CJK Unified
            _ => continue,
        };
        out.push((i, op));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grapheme_basic_ascii() {
        let b = grapheme_boundaries("hello");
        assert_eq!(b, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn grapheme_crlf_one_cluster() {
        let b = grapheme_boundaries("a\r\nb");
        // a, CRLF, b -> 3 clusters
        assert_eq!(b.len(), 4); // boundaries: 0, 1, 3, 4
    }

    #[test]
    fn grapheme_combining_attached() {
        // "a\u{0301}" = a + combining acute = single cluster
        let b = grapheme_boundaries("a\u{0301}b");
        assert_eq!(b.len(), 3); // 0, 3 (after a+combining), 4
    }

    #[test]
    fn words_split_on_whitespace() {
        let w = word_boundaries("hello world");
        assert_eq!(w.len(), 3); // hello, space, world
        assert_eq!(w[0].2, WordBoundaryKind::Word);
        assert_eq!(w[1].2, WordBoundaryKind::Whitespace);
    }

    #[test]
    fn words_with_punctuation() {
        let w = word_boundaries("Hi! Yo.");
        // Hi, !, space, Yo, .
        assert_eq!(w.len(), 5);
    }

    #[test]
    fn line_break_at_newline() {
        let b = line_break_opportunities("a\nb");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].1, LineBreakOpportunity::Mandatory);
    }

    #[test]
    fn line_break_at_space() {
        let b = line_break_opportunities("hello world");
        assert!(b.iter().any(|(_, o)| *o == LineBreakOpportunity::Allowed));
    }

    #[test]
    fn combining_marks_recognized() {
        assert!(is_combining_mark(0x0301));
        assert!(!is_combining_mark(b'A' as u32));
    }
}
