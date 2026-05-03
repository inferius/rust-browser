/// Testy pro tokenizaci regularniho vyrazu.
///
/// Klicova vec: `/` musi byt spravne rozpoznano jako regex nebo deleni
/// podle predchoziho tokenu (kontextova analyza).

use crate::lexer::base::Lexer;
use crate::tokens::TokenKind;

// Pomocnik: tokenizuj, filtruj trivia, vrat druhy tokenu.
fn lex(src: &str) -> Vec<TokenKind> {
    Lexer::parse_str(src, "<test>").unwrap().tokens.into_iter()
        .map(|t| t.kind)
        .filter(|k| !matches!(k, TokenKind::Whitespace | TokenKind::Newline | TokenKind::Eof))
        .collect()
}

// Pomocnik: ocekavame jeden RegexLiteral, vrat (pattern, flags).
fn single_regex(src: &str) -> (String, String) {
    let tokens = lex(src);
    assert_eq!(tokens.len(), 1, "Ocekavan 1 token, nalezeno {}: {:?}", tokens.len(), tokens);
    match &tokens[0] {
        TokenKind::RegexLiteral { pattern, flags } => (pattern.clone(), flags.clone()),
        other => panic!("Ocekavan RegexLiteral, nalezeno {:?}", other),
    }
}

// ─── Zakladni regex literaly ──────────────────────────────────────────────────

#[test]
fn simple_pattern() {
    let (pat, flags) = single_regex("/hello/");
    assert_eq!(pat, "hello");
    assert_eq!(flags, "");
}

#[test]
fn pattern_with_flags() {
    let (pat, flags) = single_regex("/foo/gi");
    assert_eq!(pat, "foo");
    assert_eq!(flags, "gi");
}

#[test]
fn all_common_flags() {
    let (pat, flags) = single_regex("/pattern/gimsuy");
    assert_eq!(pat, "pattern");
    assert_eq!(flags, "gimsuy");
}

#[test]
fn empty_pattern() {
    // // je komentar - prazdny regex neni validni, ale / je operator
    // Prazdny regex /(?:)/ je jediny zpusob - testujeme neco smysluplneho
    let (pat, _) = single_regex("/(?:)/");
    assert_eq!(pat, "(?:)");
}

// ─── Escape sekvence v regexu ─────────────────────────────────────────────────

#[test]
fn escaped_slash() {
    let (pat, flags) = single_regex(r"/foo\/bar/");
    assert_eq!(pat, r"foo\/bar");
    assert_eq!(flags, "");
}

#[test]
fn escaped_dot() {
    let (pat, _) = single_regex(r"/\d+\.\d+/");
    assert_eq!(pat, r"\d+\.\d+");
}

#[test]
fn backslash_sequences() {
    let (pat, _) = single_regex(r"/\w+\s+\d*/");
    assert_eq!(pat, r"\w+\s+\d*");
}

// ─── Znakove tridy ────────────────────────────────────────────────────────────

#[test]
fn character_class_with_slash() {
    // Lomitko uvnitr [...] nezakonci regex
    let (pat, _) = single_regex("/[a-z/]+/");
    assert_eq!(pat, "[a-z/]+");
}

#[test]
fn character_class_negated() {
    let (pat, _) = single_regex("/[^0-9]/");
    assert_eq!(pat, "[^0-9]");
}

#[test]
fn character_class_escaped_bracket() {
    let (pat, _) = single_regex(r"/[\]]/");
    assert_eq!(pat, r"[\]]");
}

// ─── Kontextova analyza: regex vs. deleni ────────────────────────────────────

#[test]
fn division_after_number() {
    // 10 / 2 -> cislo, deleni, cislo (NE regex)
    let tokens = lex("10 / 2");
    assert!(matches!(tokens[0], TokenKind::NumericLiteral { .. }));
    assert!(matches!(tokens[1], TokenKind::Operator(_)));   // /
    assert!(matches!(tokens[2], TokenKind::NumericLiteral { .. }));
}

#[test]
fn division_after_identifier() {
    // x / 2 -> identifikator, deleni, cislo
    let tokens = lex("x / 2");
    assert!(matches!(tokens[0], TokenKind::Identifier(_)));
    assert!(matches!(tokens[1], TokenKind::Operator(_)));   // /
    assert!(matches!(tokens[2], TokenKind::NumericLiteral { .. }));
}

#[test]
fn regex_after_assignment() {
    // x = /pattern/ -> prirazeni, regex
    let tokens = lex("x = /pattern/");
    assert!(matches!(tokens[0], TokenKind::Identifier(_)));
    assert!(matches!(tokens[1], TokenKind::Operator(_)));   // =
    assert!(matches!(tokens[2], TokenKind::RegexLiteral { .. }));
}

#[test]
fn regex_after_keyword_return() {
    let tokens = lex("return /abc/i");
    assert!(matches!(tokens[0], TokenKind::Keyword(_)));    // return
    assert!(matches!(tokens[1], TokenKind::RegexLiteral { .. }));
}

#[test]
fn regex_after_open_paren() {
    let tokens = lex("(/test/)");
    // (, regex, )
    assert!(matches!(tokens[0], TokenKind::Operator(_)));   // (
    assert!(matches!(tokens[1], TokenKind::RegexLiteral { .. }));
    assert!(matches!(tokens[2], TokenKind::Operator(_)));   // )
}

#[test]
fn regex_after_comma() {
    // foo(/a/, /b/) -> ident, (, regex, comma, regex, )
    let tokens = lex("foo(/a/, /b/)");
    assert!(matches!(tokens[0], TokenKind::Identifier(_)));
    assert!(matches!(tokens[1], TokenKind::Operator(_)));   // (
    assert!(matches!(tokens[2], TokenKind::RegexLiteral { .. }));
    assert!(matches!(tokens[3], TokenKind::Operator(_)));   // ,
    assert!(matches!(tokens[4], TokenKind::RegexLiteral { .. }));
}

#[test]
fn division_after_close_paren() {
    // (a) / b -> uzaviraci ), pak deleni (ne regex)
    let tokens = lex("(a) / b");
    let has_regex = tokens.iter().any(|t| matches!(t, TokenKind::RegexLiteral { .. }));
    assert!(!has_regex, "Neocekavan RegexLiteral za ): {:?}", tokens);
}

#[test]
fn regex_at_start_of_input() {
    // Na zacatku vstupu je / vzdy regex
    let (pat, _) = single_regex("/start/");
    assert_eq!(pat, "start");
}

// ─── Chybove stavy ───────────────────────────────────────────────────────────

#[test]
fn unterminated_regex_is_error() {
    let result = Lexer::parse_str("/unclosed", "<test>");
    assert!(result.is_err(), "Ocekavana chyba pro neuzavreny regex");
}

#[test]
fn regex_with_newline_is_error() {
    // Regex nemuze obsahovat newline (bez escape)
    let result = Lexer::parse_str("/foo\nbar/", "<test>");
    assert!(result.is_err(), "Ocekavana chyba pro regex s newline");
}
