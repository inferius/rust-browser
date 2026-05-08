use super::*;
use crate::tokens::TokenKind;

fn lex(src: &str) -> Vec<TokenKind> {
    Lexer::parse_str(src, "<test>").unwrap().tokens.into_iter()
        .map(|t| t.kind)
        .filter(|k| !matches!(k, TokenKind::Whitespace | TokenKind::Newline | TokenKind::Eof))
        .collect()
}

fn lex_kinds_str(src: &str) -> Vec<String> {
    lex(src).into_iter().map(|k| format!("{k}")).collect()
}

// --- keywords a identifikatory ---

#[test]
fn keywords_recognized() {
    let kinds = lex("let const var if else while for function return");
    for k in &kinds {
        assert!(matches!(k, TokenKind::Keyword(_)), "Ocekavan Keyword, nalezeno {k:?}");
    }
}

#[test]
fn identifier_vs_keyword() {
    let kinds = lex("letter letting");
    assert!(matches!(&kinds[0], TokenKind::Identifier(s) if s == "letter"));
    assert!(matches!(&kinds[1], TokenKind::Identifier(s) if s == "letting"));
}

#[test]
fn unicode_identifier() {
    let kinds = lex("_priv $scope hello123");
    assert!(matches!(&kinds[0], TokenKind::Identifier(s) if s == "_priv"));
    assert!(matches!(&kinds[1], TokenKind::Identifier(s) if s == "$scope"));
    assert!(matches!(&kinds[2], TokenKind::Identifier(s) if s == "hello123"));
}

// --- operatory (greedy matching) ---

#[test]
fn operator_greedy_4chars() {
    let kinds = lex(">>>=");
    assert!(matches!(&kinds[0], TokenKind::Operator(OperatorEnum::UnsignedRightShiftAssign)));
}

#[test]
fn operator_greedy_3chars() {
    let kinds = lex("===");
    assert!(matches!(&kinds[0], TokenKind::Operator(OperatorEnum::StrictEqual)));
}

#[test]
fn operator_greedy_2chars() {
    let kinds = lex("++ -- && ||");
    assert!(matches!(&kinds[0], TokenKind::Operator(OperatorEnum::PlusPlus)));
    assert!(matches!(&kinds[1], TokenKind::Operator(OperatorEnum::MinusMinus)));
    assert!(matches!(&kinds[2], TokenKind::Operator(OperatorEnum::And)));
    assert!(matches!(&kinds[3], TokenKind::Operator(OperatorEnum::Or)));
}

#[test]
fn operator_arrow_and_ellipsis() {
    let kinds = lex("=> ...");
    assert!(matches!(&kinds[0], TokenKind::Operator(OperatorEnum::Arrow)));
    assert!(matches!(&kinds[1], TokenKind::Operator(OperatorEnum::Ellipsis)));
}

// --- komentare ---

#[test]
fn line_comment_skipped() {
    let all = Lexer::parse_str("x // komentar\ny", "<test>").unwrap().tokens;
    let comments: Vec<_> = all.iter().filter(|t| matches!(t.kind, TokenKind::CommentLine(_))).collect();
    assert_eq!(comments.len(), 1);
    if let TokenKind::CommentLine(text) = &comments[0].kind {
        assert!(text.contains("komentar"));
    }
}

#[test]
fn block_comment() {
    let all = Lexer::parse_str("a /* block */ b", "<test>").unwrap().tokens;
    let blocks: Vec<_> = all.iter().filter(|t| matches!(t.kind, TokenKind::CommentBlock(_))).collect();
    assert_eq!(blocks.len(), 1);
}

// --- template literaly ---

#[test]
fn no_substitution_template() {
    let kinds = lex("`hello world`");
    assert!(matches!(&kinds[0], TokenKind::NoSubstitutionTemplate(s) if s == "hello world"));
}

#[test]
fn template_with_one_expr() {
    let kinds = lex("`Hello ${name}!`");
    assert!(matches!(&kinds[0], TokenKind::TemplateHead(s) if s == "Hello "));
    assert!(matches!(&kinds[1], TokenKind::Identifier(s) if s == "name"));
    assert!(matches!(&kinds[2], TokenKind::TemplateTail(s) if s == "!"));
}

#[test]
fn template_two_exprs() {
    let kinds = lex("`${a}+${b}`");
    assert!(matches!(&kinds[0], TokenKind::TemplateHead(s) if s == ""));
    assert!(matches!(&kinds[1], TokenKind::Identifier(s) if s == "a"));
    assert!(matches!(&kinds[2], TokenKind::TemplateMiddle(s) if s == "+"));
    assert!(matches!(&kinds[3], TokenKind::Identifier(s) if s == "b"));
    assert!(matches!(&kinds[4], TokenKind::TemplateTail(s) if s == ""));
}

// --- chyby ---

#[test]
fn unterminated_block_comment_error() {
    let res = Lexer::parse_str("/* neni uzavren", "<test>");
    assert!(res.is_err());
}

#[test]
fn unterminated_template_error() {
    let res = Lexer::parse_str("`neni uzavren", "<test>");
    assert!(res.is_err());
}

// --- numeric literals ---

#[test]
fn numeric_decimal() {
    let kinds = lex("42");
    assert!(matches!(&kinds[0], TokenKind::NumericLiteral { .. }));
}

#[test]
fn numeric_float() {
    let kinds = lex("3.14");
    assert!(matches!(&kinds[0], TokenKind::NumericLiteral { .. }));
}

#[test]
fn numeric_hex() {
    let kinds = lex("0xFF");
    assert!(matches!(&kinds[0], TokenKind::NumericLiteral { .. }));
}

#[test]
fn numeric_binary() {
    let kinds = lex("0b1010");
    assert!(matches!(&kinds[0], TokenKind::NumericLiteral { .. }));
}

#[test]
fn numeric_octal() {
    let kinds = lex("0o755");
    assert!(matches!(&kinds[0], TokenKind::NumericLiteral { .. }));
}

#[test]
fn numeric_scientific() {
    let kinds = lex("1.5e10");
    assert!(matches!(&kinds[0], TokenKind::NumericLiteral { .. }));
}

// --- string literals ---

#[test]
fn string_double_quote() {
    let kinds = lex(r#""hello""#);
    assert!(matches!(&kinds[0], TokenKind::StringLiteral { value, .. } if value == "hello"));
}

#[test]
fn string_single_quote() {
    let kinds = lex("'world'");
    assert!(matches!(&kinds[0], TokenKind::StringLiteral { value, .. } if value == "world"));
}

#[test]
fn string_escape_newline() {
    let kinds = lex(r#""line1\nline2""#);
    if let TokenKind::StringLiteral { value, .. } = &kinds[0] {
        assert!(value.contains('\n'), "escape \\n -> newline");
    } else {
        panic!("expected StringLiteral");
    }
}

#[test]
fn string_escape_tab() {
    let kinds = lex(r#""a\tb""#);
    if let TokenKind::StringLiteral { value, .. } = &kinds[0] {
        assert!(value.contains('\t'));
    }
}

#[test]
fn string_escape_quote() {
    let kinds = lex(r#""a\"b""#);
    if let TokenKind::StringLiteral { value, .. } = &kinds[0] {
        assert!(value.contains('"'));
    }
}

#[test]
fn template_literal_basic() {
    let kinds = lex("`hello`");
    assert!(!kinds.is_empty(), "lex template");
}

#[test]
fn template_literal_substitution() {
    let kinds = lex(r#"`x=${y}`"#);
    assert!(!kinds.is_empty());
}

// --- comments ---

#[test]
fn block_comment_skipped() {
    let all = Lexer::parse_str("a /* multi\nline */ b", "<test>").unwrap().tokens;
    let comments: Vec<_> = all.iter().filter(|t| matches!(t.kind, TokenKind::CommentBlock(_))).collect();
    assert_eq!(comments.len(), 1);
}

#[test]
fn empty_input_no_tokens() {
    let kinds = lex("");
    assert_eq!(kinds.len(), 0);
}

#[test]
fn whitespace_only_no_tokens() {
    let kinds = lex("   \t  \n  ");
    assert_eq!(kinds.len(), 0);
}

// --- regex disambiguation ---

#[test]
fn regex_literal_after_assignment() {
    let kinds = lex("let r = /abc/g;");
    assert!(kinds.iter().any(|k| matches!(k, TokenKind::RegexLiteral { .. })));
}

#[test]
fn division_after_identifier_no_regex() {
    let kinds = lex("a / b");
    let has_regex = kinds.iter().any(|k| matches!(k, TokenKind::RegexLiteral { .. }));
    assert!(!has_regex, "po identifier `/` neni regex");
}

#[test]
fn lex_full_program_no_panic() {
    // Smoke test - kompletni JS program lex bez crash.
    let _ = lex(r#"
        const obj = {
            name: "test",
            value: 42,
            arr: [1, 2, 3],
            nested: { a: true, b: null }
        };
        function fact(n) {
            if (n <= 1) return 1;
            return n * fact(n - 1);
        }
        const result = fact(10);
        console.log(`result = ${result}`);
    "#);
}
