use super::*;

#[test]
fn generates_html_for_simple_source() {
    let src = "let x = 42;";
    let html = generate_debug_html(src, "test");
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Tokens"));
    assert!(html.contains("AST"));
    assert!(html.contains("token tk-keyword")); // let
    assert!(html.contains("token tk-ident"));   // x
    assert!(html.contains("token tk-number"));  // 42
}

#[test]
fn html_escape_special_chars() {
    let src = r#"const s = "<script>";"#;
    let html = generate_debug_html(src, "test");
    // Source must be escaped (vyznam: nesmi zadat <script>)
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains(r#""<script>""#));
}

#[test]
fn ast_render_includes_program() {
    let src = "function foo() { return 1; }";
    let html = generate_debug_html(src, "test");
    assert!(html.contains("Program"));
    assert!(html.contains("FunctionDeclaration"));
    assert!(html.contains("ReturnStatement"));
}

#[test]
fn ast_render_class() {
    let src = "class A { foo() {} }";
    let html = generate_debug_html(src, "test");
    assert!(html.contains("ClassDeclaration"));
}

#[test]
fn lexer_error_displayed() {
    // Neuzavreny string -> lexer error
    let src = r#"const x = "unterminated"#;
    let html = generate_debug_html(src, "test");
    assert!(html.contains("error") || html.contains("Lexer error"));
}

#[test]
fn token_stats_present() {
    let src = "let x = 1; let y = 2;";
    let html = generate_debug_html(src, "test");
    assert!(html.contains("Statistika"));
    assert!(html.contains("Keyword(let)"));
}

#[test]
fn legend_visible() {
    let html = generate_debug_html("x", "test");
    assert!(html.contains("legend"));
    assert!(html.contains("token tk-keyword"));
}

#[test]
fn ast_nested_expressions() {
    let src = "1 + 2 * 3";
    let html = generate_debug_html(src, "test");
    assert!(html.contains("Binary"));
    assert!(html.contains("Number: 1"));
    assert!(html.contains("Number: 2"));
    assert!(html.contains("Number: 3"));
}
