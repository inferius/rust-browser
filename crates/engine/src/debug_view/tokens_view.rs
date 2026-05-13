/// Render tokenu jako HTML badge.
///
/// Per token vytvori <span class="token tk-X" data-info="..."> ...
/// Tooltip se zobrazi pres CSS hover.

use crate::tokens::{Token, TokenKind};
use super::html_escape;

/// Vrati CSS classu podle typu tokenu (pro barvu).
fn token_class(t: &TokenKind) -> &'static str {
    match t {
        TokenKind::Identifier(_)              => "tk-ident",
        TokenKind::NumericLiteral { .. }      => "tk-number",
        TokenKind::StringLiteral { .. }       => "tk-string",
        TokenKind::RegexLiteral { .. }        => "tk-regex",
        TokenKind::NoSubstitutionTemplate(_)
        | TokenKind::TemplateHead(_)
        | TokenKind::TemplateMiddle(_)
        | TokenKind::TemplateTail(_)          => "tk-template",
        TokenKind::Keyword(_)                 => "tk-keyword",
        TokenKind::Operator(_)                => "tk-operator",
        TokenKind::CommentLine(_)
        | TokenKind::CommentBlock(_)
        | TokenKind::HtmlCommentStart         => "tk-comment",
        TokenKind::Whitespace                 => "tk-whitespace",
        TokenKind::Newline                    => "tk-newline",
        TokenKind::Eof                        => "tk-eof",
    }
}

/// Vrati lidsky citelny popis typu tokenu.
fn token_type_label(t: &TokenKind) -> String {
    match t {
        TokenKind::Identifier(_)               => "Identifier".into(),
        TokenKind::NumericLiteral { .. }       => "NumericLiteral".into(),
        TokenKind::StringLiteral { .. }        => "StringLiteral".into(),
        TokenKind::RegexLiteral { .. }         => "RegexLiteral".into(),
        TokenKind::NoSubstitutionTemplate(_)   => "Template".into(),
        TokenKind::TemplateHead(_)             => "TemplateHead".into(),
        TokenKind::TemplateMiddle(_)           => "TemplateMiddle".into(),
        TokenKind::TemplateTail(_)             => "TemplateTail".into(),
        TokenKind::Keyword(k)                  => format!("Keyword({})", k.as_str()),
        TokenKind::Operator(o)                 => format!("Operator({})", o.as_str()),
        TokenKind::CommentLine(_)              => "CommentLine".into(),
        TokenKind::CommentBlock(_)             => "CommentBlock".into(),
        TokenKind::HtmlCommentStart            => "HtmlCommentStart".into(),
        TokenKind::Whitespace                  => "Whitespace".into(),
        TokenKind::Newline                     => "Newline".into(),
        TokenKind::Eof                         => "EOF".into(),
    }
}

/// Vytvori detail string pro tooltip.
fn token_details(t: &Token) -> String {
    let mut details = vec![
        format!("Typ: {}", token_type_label(&t.kind)),
        format!("Lexeme: {:?}", t.lexeme),
        format!("Line: {}, Col: {}", t.line, t.column),
        format!("Range: {}..{}", t.start, t.end),
    ];

    match &t.kind {
        TokenKind::NumericLiteral { value, raw, base, is_bigint, has_exponent, legacy_octal, bigint_value, .. } => {
            details.push(format!("Hodnota: {value}"));
            details.push(format!("Raw: {raw}"));
            details.push(format!("Base: {base:?}"));
            if *is_bigint {
                details.push(format!("BigInt: {}", bigint_value.as_ref().map(|b| b.to_string()).unwrap_or_default()));
            }
            if *has_exponent { details.push("Has exponent".into()); }
            if *legacy_octal { details.push("Legacy octal".into()); }
        }
        TokenKind::StringLiteral { value, raw, escapes } => {
            details.push(format!("Hodnota: {value:?}"));
            details.push(format!("Raw: {raw:?}"));
            if !escapes.is_empty() {
                details.push(format!("Escapes: {} sequences", escapes.len()));
            }
        }
        TokenKind::RegexLiteral { pattern, flags } => {
            details.push(format!("Pattern: {pattern}"));
            details.push(format!("Flags: {flags}"));
        }
        TokenKind::Keyword(k) => details.push(format!("Keyword: {}", k.as_str())),
        TokenKind::Operator(o) => details.push(format!("Operator: {}", o.as_str())),
        _ => {}
    }
    details.join("\n")
}

/// Vykresli token jako HTML span.
fn render_token(t: &Token) -> String {
    let class = token_class(&t.kind);
    let label = token_lexeme_label(t);
    let details = html_escape(&token_details(t));
    // newlines v badge: zobraz jako "\n"
    if matches!(t.kind, crate::tokens::TokenKind::Newline) {
        return format!(
            "<span class=\"token {class}\" data-tip=\"{details}\">↵</span><br/>"
        );
    }
    if matches!(t.kind, crate::tokens::TokenKind::Whitespace) {
        return format!(
            "<span class=\"token {class}\" data-tip=\"{details}\">·{}·</span>",
            t.lexeme.chars().count()
        );
    }
    format!(
        "<span class=\"token {class}\" data-tip=\"{details}\">{}</span>",
        html_escape(&label)
    )
}

/// Lexeme zobrazeny v badge - pro stringy zkraceny.
fn token_lexeme_label(t: &Token) -> String {
    let lex = &t.lexeme;
    if lex.len() > 60 {
        format!("{}...", &lex.chars().take(57).collect::<String>())
    } else {
        lex.clone()
    }
}

/// Render vsechny tokeny do HTML.
pub fn render_tokens(tokens: &[Token]) -> String {
    let mut out = String::from("<div class=\"tokens\">");
    for t in tokens {
        out.push_str(&render_token(t));
    }
    out.push_str("</div>");

    // Take statistika
    let stats = compute_stats(tokens);
    out.push_str("<div class=\"stats\"><h3>Statistika</h3><table>");
    out.push_str("<tr><th>Typ</th><th>Pocet</th></tr>");
    let mut entries: Vec<_> = stats.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (typ, count) in entries {
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            html_escape(typ), count
        ));
    }
    out.push_str("</table></div>");

    out
}

fn compute_stats(tokens: &[Token]) -> std::collections::HashMap<String, usize> {
    let mut map = std::collections::HashMap::new();
    for t in tokens {
        let label = token_type_label(&t.kind);
        *map.entry(label).or_insert(0) += 1;
    }
    map
}
