/// Parser JS/ESNext - prevadi token stream na AST.
///
/// # Algoritmus
///
/// Parser pouziva **Pratt parsing** (top-down operator precedence) pro vyrazy.
/// Kazdy operator ma prirazenu `binding_power` (levou a pravou vazebnou silu),
/// ktera urcuje prioritu a asociativitu.
///
/// Pro prikazy pouziva rekurzivni sestup (recursive descent).
///
/// # Pouziti
/// ```rust
/// let mut parser = Parser::new(tokens);
/// let program = parser.parse()?;
/// ```
///
/// # Pred parsovanim je potreba odstranit trivia
/// Parser ocekava ze token stream neobsahuje `Whitespace`, `Newline`
/// ani komentare - ty je nutne odfiltrovat pred predanim.

use crate::ast::*;
use crate::tokens::{KeywordEnum, OperatorEnum, Token, TokenKind};

// ─── Chyby parseru ────────────────────────────────────────────────────────────

/// Chyba parsovani s pozici ve zdrojovem kodu.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Popis chyby
    pub msg: String,
    /// Radek kde chyba nastala (od 1)
    pub line: usize,
    /// Sloupec kde chyba nastala (od 0)
    pub column: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chyba parseru [{}:{}]: {}", self.line, self.column, self.msg)
    }
}

// ─── Pomocne funkce ───────────────────────────────────────────────────────────

/// Konvertuje destrukturovaci Pattern na odpovidajici Expr.
///
/// Pouziva se v `for...of` / `for...in` kde AST uklada target jako `Expr`,
/// ale parser parsuje leve-strana jako Pattern.
///
/// Mapovani:
/// - `Pattern::Ident(x)`   -> `Expr::Ident(x)`
/// - `Pattern::Array(...)` -> `Expr::Array(...)` (holes zachovany)
/// - `Pattern::Object(...)`-> `Expr::Object(...)` (shorthand zachovan)
fn pattern_to_expr(pattern: Pattern) -> Expr {
    match pattern {
        Pattern::Ident(name) => Expr::Ident(name),
        Pattern::Array(elems) => Expr::Array(
            elems.into_iter().map(|e| {
                e.pattern.map(|p| {
                    let inner = pattern_to_expr(p);
                    Box::new(inner)
                })
            }).collect()
        ),
        Pattern::Object(props) => Expr::Object(
            props.into_iter().map(|p| ObjectProp {
                key: p.key,
                value: Box::new(pattern_to_expr(p.pattern)),
                shorthand: p.shorthand,
                computed: false,
            }).collect()
        ),
    }
}

// ─── Parser ───────────────────────────────────────────────────────────────────

/// Rekurzivne sestupny parser s Pratt parsovanim pro vyrazy.
///
/// Drzi token stream a aktualni pozici. Metody `parse_*` konzumují
/// tokeny a vracejí AST uzly nebo `ParseError`.
pub struct Parser {
    /// Filtrovany token stream (bez whitespace a komentaru)
    tokens: Vec<Token>,
    /// Aktualni pozice v token streamu
    pos: usize,
}

impl Parser {
    /// Vytvori parser pro dany token stream.
    ///
    /// Token stream by mel byt jiz filtrovan - bez `Whitespace`,
    /// `Newline` a komentaru. Parser si trivia sam preskakuje,
    /// ale efektivnejsi je prefiltrovat predem.
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    /// Parsuje cely program a vraci koren AST.
    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let body = self.parse_stmts_until_eof()?;
        Ok(Program { body, strict: false })
    }

    // ─── Pohyb v tokenovém poli ───────────────────────────────────────────────

    fn cur(&self) -> &Token {
        let idx = self.pos.min(self.tokens.len().saturating_sub(1));
        &self.tokens[idx]
    }

    fn kind(&self) -> &TokenKind { &self.cur().kind }

    fn peek_kind_ahead(&self, n: usize) -> &TokenKind {
        let idx = (self.pos + n).min(self.tokens.len().saturating_sub(1));
        &self.tokens[idx].kind
    }

    fn advance(&mut self) -> Token {
        let t = self.cur().clone();
        if self.pos + 1 < self.tokens.len() { self.pos += 1; }
        t
    }

    fn skip_trivia(&mut self) {
        while matches!(self.kind(),
            TokenKind::Whitespace | TokenKind::Newline
            | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_))
        {
            self.advance();
        }
    }

    fn expect_op(&mut self, op: OperatorEnum) -> Result<(), ParseError> {
        self.skip_trivia();
        if self.kind() == &TokenKind::Operator(op.clone()) {
            self.advance(); Ok(())
        } else {
            Err(self.err(format!("Očekáváno '{}', nalezeno {:?}", op.as_str(), self.kind())))
        }
    }

    fn expect_kw(&mut self, kw: KeywordEnum) -> Result<(), ParseError> {
        self.skip_trivia();
        if self.kind() == &TokenKind::Keyword(kw.clone()) {
            self.advance(); Ok(())
        } else {
            Err(self.err(format!("Očekáváno klíčové slovo '{}'", kw.as_str())))
        }
    }

    fn eat_op(&mut self, op: OperatorEnum) -> bool {
        self.skip_trivia();
        if self.kind() == &TokenKind::Operator(op) { self.advance(); true } else { false }
    }

    fn eat_semi(&mut self) {
        self.skip_trivia();
        if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Semi)) { self.advance(); }
    }

    /// Precte nepovinny label za `break`/`continue` (identifikator).
    /// ECMAScript: label musi byt na stejnem radku - ale protoze trivia
    /// uz jsou odfiltrována, jen zkontrolujeme jestli nasleduje identifikator.
    fn eat_label(&mut self) -> Option<String> {
        // Preskocime pouze whitespace (ne newlines), ale protoze mame prefiltrovany
        // stream bez trivia, zkusime jednoduchy heuristiku: identifikator hned za.
        // V praxi to funguje pro vsechny bezne pripady.
        if let TokenKind::Identifier(name) = self.kind().clone() {
            self.advance();
            Some(name)
        } else {
            None
        }
    }

    fn err(&self, msg: impl Into<String>) -> ParseError {
        ParseError { msg: msg.into(), line: self.cur().line, column: self.cur().column }
    }

    fn at_eof(&mut self) -> bool {
        self.skip_trivia();
        matches!(self.kind(), TokenKind::Eof)
    }

    // ─── Příkazy ──────────────────────────────────────────────────────────────

    fn parse_stmts_until_eof(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_trivia();
            if matches!(self.kind(), TokenKind::Eof) { break; }
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_block_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_trivia();
            if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RBrace) | TokenKind::Eof) { break; }
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.skip_trivia();
        match self.kind().clone() {
            TokenKind::Operator(OperatorEnum::LBrace) => {
                self.advance();
                let body = self.parse_block_body()?;
                self.expect_op(OperatorEnum::RBrace)?;
                Ok(Stmt::Block(body))
            }
            TokenKind::Operator(OperatorEnum::Semi) => { self.advance(); Ok(Stmt::Empty) }

            TokenKind::Keyword(KeywordEnum::Let)
            | TokenKind::Keyword(KeywordEnum::Const)
            | TokenKind::Keyword(KeywordEnum::Var) => self.parse_var_decl(),

            TokenKind::Keyword(KeywordEnum::Function) => self.parse_fn_decl(),

            TokenKind::Keyword(KeywordEnum::Return) => {
                self.advance();
                self.skip_trivia();
                let has_val = !matches!(self.kind(),
                    TokenKind::Operator(OperatorEnum::Semi)
                    | TokenKind::Operator(OperatorEnum::RBrace)
                    | TokenKind::Eof
                );
                let val = if has_val { Some(self.parse_expr()?) } else { None };
                self.eat_semi();
                Ok(Stmt::Return(val))
            }

            TokenKind::Keyword(KeywordEnum::Throw) => {
                self.advance();
                let val = self.parse_expr()?;
                self.eat_semi();
                Ok(Stmt::Throw(val))
            }

            TokenKind::Keyword(KeywordEnum::Break) => {
                self.advance();
                // `break label;` - nepovinny identifikator na stejnem radku
                let label = self.eat_label();
                self.eat_semi();
                Ok(Stmt::Break(label))
            }

            TokenKind::Keyword(KeywordEnum::Continue) => {
                self.advance();
                let label = self.eat_label();
                self.eat_semi();
                Ok(Stmt::Continue(label))
            }

            TokenKind::Keyword(KeywordEnum::If)     => self.parse_if(),
            TokenKind::Keyword(KeywordEnum::While)  => self.parse_while(),
            TokenKind::Keyword(KeywordEnum::Do)     => self.parse_do_while(),
            TokenKind::Keyword(KeywordEnum::For)    => self.parse_for(),
            TokenKind::Keyword(KeywordEnum::Try)    => self.parse_try(),
            TokenKind::Keyword(KeywordEnum::Switch) => self.parse_switch(),
            TokenKind::Keyword(KeywordEnum::Class)  => self.parse_class_decl(),

            _ => {
                let expr = self.parse_expr()?;
                // labeled statement: label:
                if let Expr::Ident(ref name) = expr {
                    self.skip_trivia();
                    if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Colon)) {
                        self.advance();
                        let body = self.parse_stmt()?;
                        return Ok(Stmt::Labeled { label: name.clone(), body: Box::new(body) });
                    }
                }
                self.eat_semi();
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let kind = match self.kind().clone() {
            TokenKind::Keyword(KeywordEnum::Let)   => { self.advance(); VarKind::Let }
            TokenKind::Keyword(KeywordEnum::Const) => { self.advance(); VarKind::Const }
            _                                       => { self.advance(); VarKind::Var }
        };
        let mut decls = Vec::new();
        loop {
            self.skip_trivia();
            let pattern = self.parse_pattern()?;
            self.skip_trivia();
            let init = if self.eat_op(OperatorEnum::Assign) {
                Some(self.parse_assign_expr()?)
            } else { None };
            decls.push(VarDecl { pattern, init });
            self.skip_trivia();
            if !self.eat_op(OperatorEnum::Comma) { break; }
        }
        self.eat_semi();
        Ok(Stmt::Var { kind, decls })
    }

    fn parse_fn_decl(&mut self) -> Result<Stmt, ParseError> {
        self.expect_kw(KeywordEnum::Function)?;
        self.skip_trivia();
        let name = self.parse_ident()?;
        let params = self.parse_params()?;
        let body = self.parse_fn_body()?;
        Ok(Stmt::Function { name, params, body })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect_op(OperatorEnum::LParen)?;
        let mut params = Vec::new();
        loop {
            self.skip_trivia();
            if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RParen)) { break; }
            let rest = self.eat_op(OperatorEnum::Ellipsis);
            let pattern = self.parse_pattern()?;
            self.skip_trivia();
            let default = if !rest && matches!(self.kind(), TokenKind::Operator(OperatorEnum::Assign)) {
                self.advance();
                Some(Box::new(self.parse_assign_expr()?))
            } else { None };
            params.push(Param { pattern, default, rest });
            self.skip_trivia();
            if rest { break; }  // rest musi byt posledni
            if !self.eat_op(OperatorEnum::Comma) { break; }
        }
        self.expect_op(OperatorEnum::RParen)?;
        Ok(params)
    }

    // ─── Třídy ───────────────────────────────────────────────────────────────

    /// Parsuje `class` deklaraci na urovni prikazu.
    fn parse_class_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'class'
        self.skip_trivia();
        let name = self.parse_ident()?;
        self.skip_trivia();
        let super_class = self.parse_class_extends()?;
        self.expect_op(OperatorEnum::LBrace)?;
        let body = self.parse_class_body()?;
        self.expect_op(OperatorEnum::RBrace)?;
        Ok(Stmt::Class { name, super_class, body })
    }

    /// Parsuje `(extends Expr)?` - volitelny rodic tridy.
    fn parse_class_extends(&mut self) -> Result<Option<Box<Expr>>, ParseError> {
        self.skip_trivia();
        if !matches!(self.kind(), TokenKind::Keyword(KeywordEnum::Extends)) {
            return Ok(None);
        }
        self.advance();
        Ok(Some(Box::new(self.parse_assign_expr()?)))
    }

    /// Parsuje telo tridy `{ member* }` (bez svorek).
    ///
    /// Kazdy clen je: `static? (get|set)? name(params) { body }`
    fn parse_class_body(&mut self) -> Result<Vec<ClassMember>, ParseError> {
        let mut members = Vec::new();
        loop {
            self.skip_trivia();
            // Preskoc prazdne prikazy v tele tridy
            while self.eat_op(OperatorEnum::Semi) { self.skip_trivia(); }
            if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RBrace) | TokenKind::Eof) {
                break;
            }

            // static keyword - pouze kdyz nasledujici token neni `(`
            // (jinak je to metoda pojmenovana "static")
            let is_static = if matches!(self.kind(), TokenKind::Keyword(KeywordEnum::Static))
                && !matches!(self.peek_kind_ahead(1), TokenKind::Operator(OperatorEnum::LParen))
            {
                self.advance(); self.skip_trivia(); true
            } else { false };

            // getter / setter - get/set keyword kde nasleduje jmeno (ne "(")
            let (is_getter, is_setter) = match self.kind() {
                TokenKind::Keyword(KeywordEnum::Get)
                    if !matches!(self.peek_kind_ahead(1), TokenKind::Operator(OperatorEnum::LParen)) =>
                {
                    self.advance(); self.skip_trivia(); (true, false)
                }
                TokenKind::Keyword(KeywordEnum::Set)
                    if !matches!(self.peek_kind_ahead(1), TokenKind::Operator(OperatorEnum::LParen)) =>
                {
                    self.advance(); self.skip_trivia(); (false, true)
                }
                _ => (false, false),
            };

            // Jmeno metody
            let name = match self.kind().clone() {
                TokenKind::Identifier(s) => { self.advance(); s }
                TokenKind::Keyword(kw)   => { let s = kw.as_str().to_string(); self.advance(); s }
                TokenKind::StringLiteral { value, .. } => { self.advance(); value }
                TokenKind::NumericLiteral { value, .. } => {
                    let n = value; self.advance();
                    format!("{}", n as i64)
                }
                _ => return Err(self.err("Ocekavano jmeno metody v tele tridy")),
            };

            let params = self.parse_params()?;
            let body   = self.parse_fn_body()?;
            members.push(ClassMember { name, params, body, is_static, is_getter, is_setter });
        }
        Ok(members)
    }

    /// Parsuje destrukturovaci vzor (pattern).
    ///
    /// Pouziva se v deklaracich promennych (`const [a, b] = ...`),
    /// parametrech funkci (`function f({ x, y }) {}`),
    /// a for-of/for-in (`for (const [k, v] of ...)`).
    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.skip_trivia();
        match self.kind().clone() {

            // Array pattern: [a, b, ...rest]
            TokenKind::Operator(OperatorEnum::LBracket) => {
                self.advance();
                let mut elems = Vec::new();
                loop {
                    self.skip_trivia();
                    if self.eat_op(OperatorEnum::RBracket) { break; }
                    // Hole: [a, , b]
                    if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Comma)) {
                        elems.push(ArrayPatternElem { pattern: None, default: None, rest: false });
                        self.advance();
                        continue;
                    }
                    let rest = self.eat_op(OperatorEnum::Ellipsis);
                    let pat = self.parse_pattern()?;
                    self.skip_trivia();
                    let default = if !rest && self.eat_op(OperatorEnum::Assign) {
                        Some(Box::new(self.parse_assign_expr()?))
                    } else { None };
                    elems.push(ArrayPatternElem { pattern: Some(pat), default, rest });
                    self.skip_trivia();
                    if rest { self.eat_op(OperatorEnum::RBracket); break; }
                    if !self.eat_op(OperatorEnum::Comma) {
                        self.expect_op(OperatorEnum::RBracket)?; break;
                    }
                }
                Ok(Pattern::Array(elems))
            }

            // Object pattern: { x, y: renamed, z = 10, ...rest }
            TokenKind::Operator(OperatorEnum::LBrace) => {
                self.advance();
                let mut props = Vec::new();
                loop {
                    self.skip_trivia();
                    if self.eat_op(OperatorEnum::RBrace) { break; }
                    // Rest prop: ...rest
                    if self.eat_op(OperatorEnum::Ellipsis) {
                        let name = self.parse_ident()?;
                        props.push(ObjectPatternProp {
                            key: PropKey::Ident(name.clone()),
                            pattern: Pattern::Ident(name),
                            default: None,
                            shorthand: false,
                        });
                        self.eat_op(OperatorEnum::Comma);
                        self.expect_op(OperatorEnum::RBrace)?;
                        break;
                    }
                    // Klic: muze byt ident nebo retezec nebo cislo
                    let key = self.parse_prop_key_pattern()?;
                    self.skip_trivia();
                    let (final_key, pattern, shorthand) = if self.eat_op(OperatorEnum::Colon) {
                        // { key: pattern }
                        let pat = self.parse_pattern()?;
                        (key, pat, false)
                    } else {
                        // { x } nebo { x = default } - klic == nazev promenne
                        let name = match &key {
                            PropKey::Ident(s) => s.clone(),
                            _ => return Err(self.err("Zkracena forma vyzaduje identifikator")),
                        };
                        (key, Pattern::Ident(name), true)
                    };
                    self.skip_trivia();
                    let default = if self.eat_op(OperatorEnum::Assign) {
                        Some(Box::new(self.parse_assign_expr()?))
                    } else { None };
                    props.push(ObjectPatternProp { key: final_key, pattern, default, shorthand });
                    self.skip_trivia();
                    if !self.eat_op(OperatorEnum::Comma) {
                        self.expect_op(OperatorEnum::RBrace)?; break;
                    }
                }
                Ok(Pattern::Object(props))
            }

            // Jednoduchy identifikator
            _ => Ok(Pattern::Ident(self.parse_ident()?)),
        }
    }

    /// Parsuje klic vlastnosti v object patternu.
    fn parse_prop_key_pattern(&mut self) -> Result<PropKey, ParseError> {
        self.skip_trivia();
        match self.kind().clone() {
            TokenKind::Identifier(s) => { self.advance(); Ok(PropKey::Ident(s)) }
            TokenKind::Keyword(kw)   => { let s = kw.as_str().to_string(); self.advance(); Ok(PropKey::Ident(s)) }
            TokenKind::StringLiteral { value, .. } => { self.advance(); Ok(PropKey::Str(value)) }
            TokenKind::NumericLiteral { value, .. } => { self.advance(); Ok(PropKey::Num(value)) }
            _ => Err(self.err("Ocekavan klic vlastnosti v object patternu")),
        }
    }

    fn parse_fn_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect_op(OperatorEnum::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect_op(OperatorEnum::RBrace)?;
        Ok(body)
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        self.expect_op(OperatorEnum::LParen)?;
        let test = self.parse_expr()?;
        self.expect_op(OperatorEnum::RParen)?;
        let yes = self.parse_stmt()?;
        self.skip_trivia();
        let no = if matches!(self.kind(), TokenKind::Keyword(KeywordEnum::Else)) {
            self.advance();
            Some(Box::new(self.parse_stmt()?))
        } else { None };
        Ok(Stmt::If { test, yes: Box::new(yes), no })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        self.expect_op(OperatorEnum::LParen)?;
        let test = self.parse_expr()?;
        self.expect_op(OperatorEnum::RParen)?;
        let body = self.parse_stmt()?;
        Ok(Stmt::While { test, body: Box::new(body) })
    }

    fn parse_do_while(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let body = self.parse_stmt()?;
        self.expect_kw(KeywordEnum::While)?;
        self.expect_op(OperatorEnum::LParen)?;
        let test = self.parse_expr()?;
        self.expect_op(OperatorEnum::RParen)?;
        self.eat_semi();
        Ok(Stmt::DoWhile { body: Box::new(body), test })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        self.expect_op(OperatorEnum::LParen)?;
        self.skip_trivia();

        let is_var_kw = matches!(self.kind(),
            TokenKind::Keyword(KeywordEnum::Let)
            | TokenKind::Keyword(KeywordEnum::Const)
            | TokenKind::Keyword(KeywordEnum::Var));

        if is_var_kw {
            let kind = match self.kind().clone() {
                TokenKind::Keyword(KeywordEnum::Let)   => { self.advance(); VarKind::Let }
                TokenKind::Keyword(KeywordEnum::Const) => { self.advance(); VarKind::Const }
                _                                       => { self.advance(); VarKind::Var }
            };
            self.skip_trivia();
            let pattern = self.parse_pattern()?;
            self.skip_trivia();

            // for...of (vcetne destrukturovani: for (const [k, v] of ...))
            if matches!(self.kind(), TokenKind::Keyword(KeywordEnum::Of)) {
                self.advance();
                let iter = self.parse_assign_expr()?;
                self.expect_op(OperatorEnum::RParen)?;
                let target = pattern_to_expr(pattern);
                return Ok(Stmt::ForOf {
                    kind: Some(kind),
                    target: Box::new(target),
                    iter, body: Box::new(self.parse_stmt()?),
                });
            }
            // for...in
            if matches!(self.kind(), TokenKind::Keyword(KeywordEnum::In)) {
                self.advance();
                let iter = self.parse_expr()?;
                self.expect_op(OperatorEnum::RParen)?;
                let target = pattern_to_expr(pattern);
                return Ok(Stmt::ForIn {
                    kind: Some(kind),
                    target: Box::new(target),
                    iter, body: Box::new(self.parse_stmt()?),
                });
            }
            // for (let i = 0; i < n; i++) - pattern musi byt jednoduchy ident
            let name = match pattern {
                Pattern::Ident(n) => n,
                _ => return Err(self.err("Destrukturovani neni podporovano v klasickem for")),
            };
            let init_val = if self.eat_op(OperatorEnum::Assign) { Some(self.parse_assign_expr()?) } else { None };
            let init = Some(ForInit::Var { kind, decls: vec![VarDecl { pattern: Pattern::Ident(name), init: init_val }] });
            self.expect_op(OperatorEnum::Semi)?;
            let test = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Semi)) { None }
            else { Some(self.parse_expr()?) };
            self.expect_op(OperatorEnum::Semi)?;
            let update = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RParen)) { None }
            else { Some(self.parse_expr()?) };
            self.expect_op(OperatorEnum::RParen)?;
            return Ok(Stmt::For { init, test, update, body: Box::new(self.parse_stmt()?) });
        }

        // for (expr; ...)
        let init = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Semi)) { None }
        else { Some(ForInit::Expr(self.parse_expr()?)) };
        self.expect_op(OperatorEnum::Semi)?;
        let test = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Semi)) { None }
        else { Some(self.parse_expr()?) };
        self.expect_op(OperatorEnum::Semi)?;
        let update = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RParen)) { None }
        else { Some(self.parse_expr()?) };
        self.expect_op(OperatorEnum::RParen)?;
        Ok(Stmt::For { init, test, update, body: Box::new(self.parse_stmt()?) })
    }

    fn parse_try(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        self.expect_op(OperatorEnum::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect_op(OperatorEnum::RBrace)?;

        self.skip_trivia();
        let catch = if matches!(self.kind(), TokenKind::Keyword(KeywordEnum::Catch)) {
            self.advance();
            let param = if self.eat_op(OperatorEnum::LParen) {
                let p = self.parse_ident()?;
                self.expect_op(OperatorEnum::RParen)?;
                Some(p)
            } else { None };
            self.expect_op(OperatorEnum::LBrace)?;
            let cbody = self.parse_block_body()?;
            self.expect_op(OperatorEnum::RBrace)?;
            Some(CatchClause { param, body: cbody })
        } else { None };

        self.skip_trivia();
        let finally = if matches!(self.kind(), TokenKind::Keyword(KeywordEnum::Finally)) {
            self.advance();
            self.expect_op(OperatorEnum::LBrace)?;
            let fb = self.parse_block_body()?;
            self.expect_op(OperatorEnum::RBrace)?;
            Some(fb)
        } else { None };

        Ok(Stmt::Try { body, catch, finally })
    }

    fn parse_switch(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'switch'
        self.skip_trivia();
        self.expect_op(OperatorEnum::LParen)?;
        let discriminant = self.parse_expr()?;
        self.expect_op(OperatorEnum::RParen)?;
        self.skip_trivia();
        self.expect_op(OperatorEnum::LBrace)?;

        let mut cases = Vec::new();
        loop {
            self.skip_trivia();
            match self.kind().clone() {
                TokenKind::Operator(OperatorEnum::RBrace) | TokenKind::Eof => break,

                TokenKind::Keyword(KeywordEnum::Case) => {
                    self.advance();
                    // parse_assign_expr misto parse_expr: vyhneme se chyceni carky
                    let test = self.parse_assign_expr()?;
                    self.expect_op(OperatorEnum::Colon)?;
                    let body = self.parse_case_body()?;
                    cases.push(SwitchCase { test: Some(test), body });
                }

                TokenKind::Keyword(KeywordEnum::Default) => {
                    self.advance();
                    self.expect_op(OperatorEnum::Colon)?;
                    let body = self.parse_case_body()?;
                    cases.push(SwitchCase { test: None, body });
                }

                _ => return Err(self.err("Ocekavano 'case' nebo 'default'")),
            }
        }

        self.expect_op(OperatorEnum::RBrace)?;
        Ok(Stmt::Switch { discriminant, cases })
    }

    /// Parsuje prikazy tela jedne case/default vetve.
    /// Zastavi se pred dalsim `case`, `default`, `}` nebo EOF.
    fn parse_case_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_trivia();
            match self.kind() {
                TokenKind::Keyword(KeywordEnum::Case)
                | TokenKind::Keyword(KeywordEnum::Default)
                | TokenKind::Operator(OperatorEnum::RBrace)
                | TokenKind::Eof => break,
                _ => stmts.push(self.parse_stmt()?),
            }
        }
        Ok(stmts)
    }

    // ─── Výrazy ───────────────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_trivia();
        let mut exprs = vec![self.parse_assign_expr()?];
        while self.eat_op(OperatorEnum::Comma) {
            exprs.push(self.parse_assign_expr()?);
        }
        if exprs.len() == 1 { Ok(exprs.remove(0)) } else { Ok(Expr::Sequence(exprs)) }
    }

    fn parse_assign_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_trivia();

        // Detekce arrow funkce: ident => nebo () =>
        if self.is_arrow() {
            return self.parse_arrow();
        }

        let left = self.parse_ternary()?;
        self.skip_trivia();

        let op = match self.kind() {
            TokenKind::Operator(OperatorEnum::Assign)         => Some(AssignOp::Assign),
            TokenKind::Operator(OperatorEnum::AddAssign)      => Some(AssignOp::Add),
            TokenKind::Operator(OperatorEnum::SubAssign)      => Some(AssignOp::Sub),
            TokenKind::Operator(OperatorEnum::MulAssign)      => Some(AssignOp::Mul),
            TokenKind::Operator(OperatorEnum::DivAssign)      => Some(AssignOp::Div),
            TokenKind::Operator(OperatorEnum::ModAssign)      => Some(AssignOp::Mod),
            TokenKind::Operator(OperatorEnum::AssignExp)      => Some(AssignOp::Exp),
            TokenKind::Operator(OperatorEnum::AndAssign)      => Some(AssignOp::BitAnd),
            TokenKind::Operator(OperatorEnum::OrAssign)       => Some(AssignOp::BitOr),
            TokenKind::Operator(OperatorEnum::XorAssign)      => Some(AssignOp::BitXor),
            TokenKind::Operator(OperatorEnum::AssignShl)      => Some(AssignOp::Shl),
            TokenKind::Operator(OperatorEnum::AssignShr)      => Some(AssignOp::Shr),
            TokenKind::Operator(OperatorEnum::LogAndAssign)   => Some(AssignOp::LogicalAnd),
            TokenKind::Operator(OperatorEnum::LogOrAssign)    => Some(AssignOp::LogicalOr),
            TokenKind::Operator(OperatorEnum::NullCoalAssign) => Some(AssignOp::NullCoal),
            _ => None,
        };
        if let Some(aop) = op {
            self.advance();
            let right = self.parse_assign_expr()?;
            return Ok(Expr::Assign { op: aop, target: Box::new(left), value: Box::new(right) });
        }
        Ok(left)
    }

    /// Ternární výraz: expr ? yes : no
    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_pratt(0)?;
        self.skip_trivia();
        if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Question)) {
            self.advance();
            let yes = self.parse_assign_expr()?;
            self.expect_op(OperatorEnum::Colon)?;
            let no = self.parse_assign_expr()?;
            Ok(Expr::Ternary { test: Box::new(expr), yes: Box::new(yes), no: Box::new(no) })
        } else {
            Ok(expr)
        }
    }

    /// Pratt parser pro binární výrazy.
    fn parse_pratt(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        self.skip_trivia();
        let mut left = self.parse_unary()?;

        loop {
            self.skip_trivia();

            // (lbp, rbp) binding power. rbp > lbp = pravá asociativita.
            let (lbp, rbp): (u8, u8) = match self.kind() {
                TokenKind::Operator(OperatorEnum::Or)          => (6, 7),
                TokenKind::Operator(OperatorEnum::And)         => (8, 9),
                TokenKind::Operator(OperatorEnum::NullCoal)    => (6, 7),
                TokenKind::Operator(OperatorEnum::Pipe)        => (10, 11),
                TokenKind::Operator(OperatorEnum::Caret)       => (12, 13),
                TokenKind::Operator(OperatorEnum::Amp)         => (14, 15),
                TokenKind::Operator(OperatorEnum::EqEq)        => (16, 17),
                TokenKind::Operator(OperatorEnum::NotEq)       => (16, 17),
                TokenKind::Operator(OperatorEnum::StrictEqual)    => (16, 17),
                TokenKind::Operator(OperatorEnum::StrictNotEqual) => (16, 17),
                TokenKind::Operator(OperatorEnum::Lt)          => (18, 19),
                TokenKind::Operator(OperatorEnum::Gt)          => (18, 19),
                TokenKind::Operator(OperatorEnum::LtEq)        => (18, 19),
                TokenKind::Operator(OperatorEnum::GtEq)        => (18, 19),
                TokenKind::Keyword(KeywordEnum::In)            => (18, 19),
                TokenKind::Keyword(KeywordEnum::Instanceof)    => (18, 19),
                TokenKind::Operator(OperatorEnum::Shl)         => (20, 21),
                TokenKind::Operator(OperatorEnum::Shr)         => (20, 21),
                TokenKind::Operator(OperatorEnum::ShiftRightU) => (20, 21),
                TokenKind::Operator(OperatorEnum::Plus)        => (22, 23),
                TokenKind::Operator(OperatorEnum::Minus)       => (22, 23),
                TokenKind::Operator(OperatorEnum::Star)        => (24, 25),
                TokenKind::Operator(OperatorEnum::Slash)       => (24, 25),
                TokenKind::Operator(OperatorEnum::Percent)     => (24, 25),
                TokenKind::Operator(OperatorEnum::Exp)         => (27, 26), // pravá asociativita
                _ => break,
            };

            if lbp < min_bp { break; }

            let tok = self.advance();
            let right = self.parse_pratt(rbp)?;

            left = match &tok.kind {
                TokenKind::Operator(OperatorEnum::Or)       => Expr::Logical { op: LogicalOp::Or,       left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::And)      => Expr::Logical { op: LogicalOp::And,      left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::NullCoal) => Expr::Logical { op: LogicalOp::NullCoal, left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::EqEq)        => Expr::Binary { op: BinaryOp::Eq,          left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::NotEq)       => Expr::Binary { op: BinaryOp::NotEq,       left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::StrictEqual)    => Expr::Binary { op: BinaryOp::StrictEq,    left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::StrictNotEqual) => Expr::Binary { op: BinaryOp::StrictNotEq, left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Lt)          => Expr::Binary { op: BinaryOp::Lt,          left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Gt)          => Expr::Binary { op: BinaryOp::Gt,          left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::LtEq)        => Expr::Binary { op: BinaryOp::LtEq,        left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::GtEq)        => Expr::Binary { op: BinaryOp::GtEq,        left: Box::new(left), right: Box::new(right) },
                TokenKind::Keyword(KeywordEnum::In)            => Expr::Binary { op: BinaryOp::In,          left: Box::new(left), right: Box::new(right) },
                TokenKind::Keyword(KeywordEnum::Instanceof)    => Expr::Binary { op: BinaryOp::Instanceof,  left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Shl)         => Expr::Binary { op: BinaryOp::Shl,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Shr)         => Expr::Binary { op: BinaryOp::Shr,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::ShiftRightU) => Expr::Binary { op: BinaryOp::Ushr,        left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Pipe)        => Expr::Binary { op: BinaryOp::BitOr,       left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Caret)       => Expr::Binary { op: BinaryOp::BitXor,      left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Amp)         => Expr::Binary { op: BinaryOp::BitAnd,      left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Plus)        => Expr::Binary { op: BinaryOp::Add,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Minus)       => Expr::Binary { op: BinaryOp::Sub,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Star)        => Expr::Binary { op: BinaryOp::Mul,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Slash)       => Expr::Binary { op: BinaryOp::Div,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Percent)     => Expr::Binary { op: BinaryOp::Mod,         left: Box::new(left), right: Box::new(right) },
                TokenKind::Operator(OperatorEnum::Exp)         => Expr::Binary { op: BinaryOp::Exp,         left: Box::new(left), right: Box::new(right) },
                _ => unreachable!(),
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_trivia();
        match self.kind().clone() {
            TokenKind::Operator(OperatorEnum::Bang)       => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Not,    arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Operator(OperatorEnum::Minus)      => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Minus,  arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Operator(OperatorEnum::Plus)       => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Plus,   arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Operator(OperatorEnum::Tilde)      => { self.advance(); Ok(Expr::Unary { op: UnaryOp::BitNot, arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Keyword(KeywordEnum::Typeof)       => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Typeof, arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Keyword(KeywordEnum::Void)         => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Void,   arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Keyword(KeywordEnum::Delete)       => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Delete, arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Operator(OperatorEnum::PlusPlus)   => { self.advance(); Ok(Expr::Unary { op: UnaryOp::PreInc, arg: Box::new(self.parse_unary()?) }) }
            TokenKind::Operator(OperatorEnum::MinusMinus) => { self.advance(); Ok(Expr::Unary { op: UnaryOp::PreDec, arg: Box::new(self.parse_unary()?) }) }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            self.skip_trivia();
            match self.kind().clone() {
                TokenKind::Operator(OperatorEnum::Dot) => {
                    self.advance(); self.skip_trivia();
                    let name = match self.kind().clone() {
                        TokenKind::Identifier(s) => { self.advance(); s }
                        TokenKind::Keyword(kw)   => { let s = kw.as_str().to_string(); self.advance(); s }
                        _ => return Err(self.err("Ocekavano jmeno vlastnosti za teckou")),
                    };
                    expr = Expr::Member { object: Box::new(expr), prop: MemberProp::Ident(name), optional: false };
                }
                TokenKind::Operator(OperatorEnum::LBracket) => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect_op(OperatorEnum::RBracket)?;
                    expr = Expr::Member { object: Box::new(expr), prop: MemberProp::Computed(Box::new(idx)), optional: false };
                }
                TokenKind::Operator(OperatorEnum::LParen) => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect_op(OperatorEnum::RParen)?;
                    expr = Expr::Call { callee: Box::new(expr), args, optional: false };
                }
                // Optional chaining: obj?.prop  obj?.[expr]  obj?.()
                TokenKind::Operator(OperatorEnum::OptChain) => {
                    self.advance(); self.skip_trivia();
                    expr = match self.kind().clone() {
                        TokenKind::Operator(OperatorEnum::LBracket) => {
                            self.advance();
                            let idx = self.parse_expr()?;
                            self.expect_op(OperatorEnum::RBracket)?;
                            Expr::Member { object: Box::new(expr), prop: MemberProp::Computed(Box::new(idx)), optional: true }
                        }
                        TokenKind::Operator(OperatorEnum::LParen) => {
                            self.advance();
                            let args = self.parse_call_args()?;
                            self.expect_op(OperatorEnum::RParen)?;
                            Expr::Call { callee: Box::new(expr), args, optional: true }
                        }
                        _ => {
                            let name = match self.kind().clone() {
                                TokenKind::Identifier(s) => { self.advance(); s }
                                TokenKind::Keyword(kw)   => { let s = kw.as_str().to_string(); self.advance(); s }
                                _ => return Err(self.err("Ocekavano jmeno vlastnosti za ?.")),
                            };
                            Expr::Member { object: Box::new(expr), prop: MemberProp::Ident(name), optional: true }
                        }
                    };
                }
                TokenKind::Operator(OperatorEnum::PlusPlus) => {
                    self.advance();
                    expr = Expr::Binary { op: BinaryOp::PostInc, left: Box::new(expr), right: Box::new(Expr::Undefined) };
                }
                TokenKind::Operator(OperatorEnum::MinusMinus) => {
                    self.advance();
                    expr = Expr::Binary { op: BinaryOp::PostDec, left: Box::new(expr), right: Box::new(Expr::Undefined) };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_trivia();
        match self.kind().clone() {
            TokenKind::NumericLiteral { value, is_bigint, bigint_value, .. } => {
                self.advance();
                if is_bigint {
                    Ok(Expr::Number(bigint_value.map(|b| b.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0)))
                } else {
                    Ok(Expr::Number(value))
                }
            }

            TokenKind::StringLiteral { value, .. } => { let s = value.clone(); self.advance(); Ok(Expr::Str(s)) }

            TokenKind::NoSubstitutionTemplate(s) => { let st = s.clone(); self.advance(); Ok(Expr::Str(st)) }
            TokenKind::TemplateHead(_) => self.parse_template(),

            TokenKind::Keyword(KeywordEnum::True)  => { self.advance(); Ok(Expr::Bool(true)) }
            TokenKind::Keyword(KeywordEnum::False) => { self.advance(); Ok(Expr::Bool(false)) }
            TokenKind::Keyword(KeywordEnum::Null)  => { self.advance(); Ok(Expr::Null) }
            TokenKind::Keyword(KeywordEnum::This)  => { self.advance(); Ok(Expr::Ident("this".to_string())) }
            // `super` - jako identifikator, interpreter ho zpracuje specialne
            TokenKind::Keyword(KeywordEnum::Super) => { self.advance(); Ok(Expr::Ident("super".to_string())) }

            TokenKind::Identifier(s) => { let name = s.clone(); self.advance(); Ok(Expr::Ident(name)) }

            TokenKind::Operator(OperatorEnum::LParen) => {
                self.advance(); self.skip_trivia();
                // () => ...
                if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RParen)) {
                    self.advance(); self.skip_trivia();
                    self.expect_op(OperatorEnum::Arrow)?;
                    return self.parse_arrow_body(vec![]); // Vec<Param>
                }
                let expr = self.parse_expr()?;
                self.expect_op(OperatorEnum::RParen)?;
                Ok(expr)
            }

            TokenKind::Operator(OperatorEnum::LBracket) => {
                self.advance();
                let mut items: Vec<Option<Box<Expr>>> = Vec::new();
                loop {
                    self.skip_trivia();
                    if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RBracket)) { break; }
                    if matches!(self.kind(), TokenKind::Operator(OperatorEnum::Comma)) {
                        self.advance(); items.push(None); continue;
                    }
                    items.push(Some(Box::new(self.parse_assign_expr()?)));
                    self.skip_trivia();
                    if !self.eat_op(OperatorEnum::Comma) { break; }
                }
                self.expect_op(OperatorEnum::RBracket)?;
                Ok(Expr::Array(items))
            }

            TokenKind::Operator(OperatorEnum::LBrace) => {
                self.advance();
                let mut props = Vec::new();
                loop {
                    self.skip_trivia();
                    if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RBrace)) { break; }
                    props.push(self.parse_object_prop()?);
                    self.skip_trivia();
                    if !self.eat_op(OperatorEnum::Comma) { break; }
                }
                self.expect_op(OperatorEnum::RBrace)?;
                Ok(Expr::Object(props))
            }

            TokenKind::Keyword(KeywordEnum::New) => {
                self.advance(); self.skip_trivia();
                // Pro new: parsuj jen member access (tecka, [expr]), NE volani funkce.
                // "new Foo(args)" musi byt new(Foo)(args), ne new(Foo(args)).
                let mut callee = self.parse_primary()?;
                loop {
                    self.skip_trivia();
                    match self.kind().clone() {
                        TokenKind::Operator(OperatorEnum::Dot) => {
                            self.advance(); self.skip_trivia();
                            let name = match self.kind().clone() {
                                TokenKind::Identifier(s) => { self.advance(); s }
                                TokenKind::Keyword(kw)   => { let s = kw.as_str().to_string(); self.advance(); s }
                                _ => return Err(self.err("Ocekavano jmeno vlastnosti za teckou v new expr")),
                            };
                            callee = Expr::Member { object: Box::new(callee), prop: MemberProp::Ident(name), optional: false };
                        }
                        TokenKind::Operator(OperatorEnum::LBracket) => {
                            self.advance();
                            let idx = self.parse_expr()?;
                            self.expect_op(OperatorEnum::RBracket)?;
                            callee = Expr::Member { object: Box::new(callee), prop: MemberProp::Computed(Box::new(idx)), optional: false };
                        }
                        _ => break,
                    }
                }
                let args = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::LParen)) {
                    self.advance();
                    let a = self.parse_call_args()?;
                    self.expect_op(OperatorEnum::RParen)?;
                    a
                } else { vec![] };
                Ok(Expr::New { callee: Box::new(callee), args })
            }

            TokenKind::Keyword(KeywordEnum::Function) => {
                self.advance(); self.skip_trivia();
                let name = if matches!(self.kind(), TokenKind::Identifier(_) | TokenKind::Keyword(_)) {
                    Some(self.parse_ident()?)
                } else { None };
                let params = self.parse_params()?;
                let body = self.parse_fn_body()?;
                Ok(Expr::Function { name, params, body })
            }

            TokenKind::RegexLiteral { pattern, flags } => {
                let (p, f) = (pattern.clone(), flags.clone());
                self.advance();
                Ok(Expr::Regex(p, f))
            }

            // Vyrazova trida: `const Foo = class { ... }`
            TokenKind::Keyword(KeywordEnum::Class) => {
                self.advance(); self.skip_trivia();
                // Volitelne jmeno tridy v expressionu
                let name = if matches!(self.kind(), TokenKind::Identifier(_)) {
                    Some(self.parse_ident()?)
                } else { None };
                let super_class = self.parse_class_extends()?;
                self.expect_op(OperatorEnum::LBrace)?;
                let body = self.parse_class_body()?;
                self.expect_op(OperatorEnum::RBrace)?;
                Ok(Expr::ClassExpr { name, super_class, body })
            }

            _ => Err(self.err(format!("Neočekávaný token: {:?}", self.kind()))),
        }
    }

    fn parse_template(&mut self) -> Result<Expr, ParseError> {
        let mut quasis = Vec::new();
        let mut expressions: Vec<Box<Expr>> = Vec::new();

        if let TokenKind::TemplateHead(s) = self.kind().clone() {
            quasis.push(s); self.advance();
        }
        loop {
            expressions.push(Box::new(self.parse_assign_expr()?));
            self.skip_trivia();
            match self.kind().clone() {
                TokenKind::TemplateMiddle(s) => { quasis.push(s); self.advance(); }
                TokenKind::TemplateTail(s)   => { quasis.push(s); self.advance(); break; }
                _ => return Err(self.err("Neukončený template literál")),
            }
        }
        Ok(Expr::Template { quasis, expressions })
    }

    fn parse_object_prop(&mut self) -> Result<ObjectProp, ParseError> {
        self.skip_trivia();

        // [computed]:
        if matches!(self.kind(), TokenKind::Operator(OperatorEnum::LBracket)) {
            self.advance();
            let key_expr = self.parse_assign_expr()?;
            self.expect_op(OperatorEnum::RBracket)?;
            self.expect_op(OperatorEnum::Colon)?;
            let value = self.parse_assign_expr()?;
            return Ok(ObjectProp { key: PropKey::Computed(Box::new(key_expr)), value: Box::new(value), shorthand: false, computed: true });
        }

        let key = match self.kind().clone() {
            TokenKind::Identifier(s)           => { self.advance(); PropKey::Ident(s) }
            TokenKind::StringLiteral { value, .. } => { let s = value.clone(); self.advance(); PropKey::Str(s) }
            TokenKind::NumericLiteral { value, .. } => { let n = value; self.advance(); PropKey::Num(n) }
            TokenKind::Keyword(kw)             => { let s = kw.as_str().to_string(); self.advance(); PropKey::Ident(s) }
            _ => return Err(self.err("Očekáván klíč vlastnosti objektu")),
        };

        self.skip_trivia();
        // method shorthand: { foo(a, b) { ... } }
        if matches!(self.kind(), TokenKind::Operator(OperatorEnum::LParen)) {
            let fn_name = match &key { PropKey::Ident(s) | PropKey::Str(s) => Some(s.clone()), _ => None };
            let params = self.parse_params()?;
            let body = self.parse_fn_body()?;
            let func = Expr::Function { name: fn_name, params, body };
            return Ok(ObjectProp { key, value: Box::new(func), shorthand: false, computed: false });
        }
        // shorthand: { x }
        if !matches!(self.kind(), TokenKind::Operator(OperatorEnum::Colon)) {
            let name = match &key { PropKey::Ident(s) => s.clone(), _ => return Err(self.err("Shorthand klic musi byt identifikator")) };
            return Ok(ObjectProp { key, value: Box::new(Expr::Ident(name)), shorthand: true, computed: false });
        }
        self.expect_op(OperatorEnum::Colon)?;
        let value = self.parse_assign_expr()?;
        Ok(ObjectProp { key, value: Box::new(value), shorthand: false, computed: false })
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        loop {
            self.skip_trivia();
            if matches!(self.kind(), TokenKind::Operator(OperatorEnum::RParen)) { break; }
            let spread = self.eat_op(OperatorEnum::Ellipsis);
            let arg = self.parse_assign_expr()?;
            args.push(if spread { Expr::Spread(Box::new(arg)) } else { arg });
            self.skip_trivia();
            if !self.eat_op(OperatorEnum::Comma) { break; }
        }
        Ok(args)
    }

    // ─── Arrow funkce ─────────────────────────────────────────────────────────

    /// Vraci true pokud aktualni pozice vypada jako zacatek arrow funkce.
    /// Podporuje: `x =>` i `(params) =>`.
    fn is_arrow(&mut self) -> bool {
        match self.kind() {
            TokenKind::Identifier(_) => {
                let mut i = 1;
                loop {
                    match self.peek_kind_ahead(i) {
                        TokenKind::Whitespace | TokenKind::Newline => i += 1,
                        TokenKind::Operator(OperatorEnum::Arrow) => return true,
                        _ => return false,
                    }
                }
            }
            TokenKind::Operator(OperatorEnum::LParen) => {
                // Skenujeme dopredu pres vyrovnane () a hledame =>
                let mut i = 1;
                let mut depth = 1i32;
                loop {
                    match self.peek_kind_ahead(i) {
                        TokenKind::Whitespace | TokenKind::Newline => { i += 1; }
                        TokenKind::Operator(OperatorEnum::LParen) => { depth += 1; i += 1; }
                        TokenKind::Operator(OperatorEnum::RParen) => {
                            depth -= 1; i += 1;
                            if depth == 0 {
                                loop {
                                    match self.peek_kind_ahead(i) {
                                        TokenKind::Whitespace | TokenKind::Newline => { i += 1; }
                                        TokenKind::Operator(OperatorEnum::Arrow) => return true,
                                        _ => return false,
                                    }
                                }
                            }
                        }
                        TokenKind::Eof => return false,
                        _ => { i += 1; }
                    }
                }
            }
            _ => false,
        }
    }

    fn parse_arrow(&mut self) -> Result<Expr, ParseError> {
        let params = if matches!(self.kind(), TokenKind::Operator(OperatorEnum::LParen)) {
            self.parse_params()?
        } else {
            vec![Param::simple(self.parse_ident()?)]
        };
        self.skip_trivia();
        self.expect_op(OperatorEnum::Arrow)?;
        self.parse_arrow_body(params)
    }

    fn parse_arrow_body(&mut self, params: Vec<Param>) -> Result<Expr, ParseError> {
        self.skip_trivia();
        if matches!(self.kind(), TokenKind::Operator(OperatorEnum::LBrace)) {
            let body = self.parse_fn_body()?;
            Ok(Expr::Arrow { params, body: ArrowBody::Block(body) })
        } else {
            let expr = self.parse_assign_expr()?;
            Ok(Expr::Arrow { params, body: ArrowBody::Expr(Box::new(expr)) })
        }
    }

    // ─── Pomocné ──────────────────────────────────────────────────────────────

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        self.skip_trivia();
        match self.kind().clone() {
            TokenKind::Identifier(s) => { self.advance(); Ok(s) }
            TokenKind::Keyword(kw)   => { let s = kw.as_str().to_string(); self.advance(); Ok(s) }
            _ => Err(self.err(format!("Ocekavan identifikator, nalezeno {:?}", self.kind()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::lexer::base::Lexer;
    use crate::tokens::TokenKind;

    fn parse(src: &str) -> Program {
        let lexer = Lexer::parse_str(src, "<test>").unwrap();
        let tokens: Vec<_> = lexer.tokens.into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline
                | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)))
            .collect();
        Parser::new(tokens).parse().unwrap()
    }

    fn parse_expr(src: &str) -> Expr {
        let prog = parse(src);
        match prog.body.into_iter().next().unwrap() {
            Stmt::Expr(e) => e,
            other => panic!("Ocekavan ExprStmt, nalezeno {other:?}"),
        }
    }

    fn parse_stmt(src: &str) -> Stmt {
        parse(src).body.into_iter().next().unwrap()
    }

    // --- cisla a stringy ---

    #[test]
    fn number_literal() {
        assert!(matches!(parse_expr("42"), Expr::Number(n) if n == 42.0));
        assert!(matches!(parse_expr("3.14"), Expr::Number(n) if (n - 3.14).abs() < 1e-10));
        assert!(matches!(parse_expr("1e3"), Expr::Number(n) if n == 1000.0));
    }

    #[test]
    fn string_literal() {
        assert!(matches!(parse_expr(r#""hello""#), Expr::Str(s) if s == "hello"));
        assert!(matches!(parse_expr("'world'"), Expr::Str(s) if s == "world"));
    }

    #[test]
    fn bool_null_undefined() {
        assert!(matches!(parse_expr("true"), Expr::Bool(true)));
        assert!(matches!(parse_expr("false"), Expr::Bool(false)));
        assert!(matches!(parse_expr("null"), Expr::Null));
    }

    // --- binarne vyrazy a priorita ---

    #[test]
    fn binary_add() {
        match parse_expr("1 + 2") {
            Expr::Binary { op: BinaryOp::Add, .. } => {}
            other => panic!("Ocekavan Add, nalezeno {other:?}"),
        }
    }

    #[test]
    fn operator_precedence_mul_before_add() {
        // 1 + 2 * 3  =>  Add(1, Mul(2, 3))
        match parse_expr("1 + 2 * 3") {
            Expr::Binary { op: BinaryOp::Add, left, right } => {
                assert!(matches!(*left, Expr::Number(n) if n == 1.0));
                assert!(matches!(*right, Expr::Binary { op: BinaryOp::Mul, .. }));
            }
            other => panic!("Spatna struktura: {other:?}"),
        }
    }

    #[test]
    fn operator_precedence_grouping() {
        // (1 + 2) * 3  =>  Mul(Add(1,2), 3)
        match parse_expr("(1 + 2) * 3") {
            Expr::Binary { op: BinaryOp::Mul, left, .. } => {
                assert!(matches!(*left, Expr::Binary { op: BinaryOp::Add, .. }));
            }
            other => panic!("Spatna struktura: {other:?}"),
        }
    }

    #[test]
    fn exponentiation_right_assoc() {
        // 2 ** 3 ** 2  =>  2 ** (3 ** 2)  =>  Exp(2, Exp(3, 2))
        match parse_expr("2 ** 3 ** 2") {
            Expr::Binary { op: BinaryOp::Exp, right, .. } => {
                assert!(matches!(*right, Expr::Binary { op: BinaryOp::Exp, .. }));
            }
            other => panic!("Spatna struktura: {other:?}"),
        }
    }

    // --- unarne vyrazy ---

    #[test]
    fn unary_minus() {
        assert!(matches!(parse_expr("-1"), Expr::Unary { op: UnaryOp::Minus, .. }));
    }

    #[test]
    fn unary_not() {
        assert!(matches!(parse_expr("!true"), Expr::Unary { op: UnaryOp::Not, .. }));
    }

    #[test]
    fn unary_typeof() {
        assert!(matches!(parse_expr("typeof x"), Expr::Unary { op: UnaryOp::Typeof, .. }));
    }

    // --- ternary ---

    #[test]
    fn ternary_expr() {
        match parse_expr("a ? 1 : 2") {
            Expr::Ternary { test, yes, no } => {
                assert!(matches!(*test, Expr::Ident(s) if s == "a"));
                assert!(matches!(*yes, Expr::Number(n) if n == 1.0));
                assert!(matches!(*no, Expr::Number(n) if n == 2.0));
            }
            other => panic!("Ocekavan Ternary, nalezeno {other:?}"),
        }
    }

    // --- prirazeni ---

    #[test]
    fn assignment() {
        match parse_expr("x = 5") {
            Expr::Assign { op: AssignOp::Assign, target, value } => {
                assert!(matches!(*target, Expr::Ident(s) if s == "x"));
                assert!(matches!(*value, Expr::Number(n) if n == 5.0));
            }
            other => panic!("Ocekavano prirazeni, nalezeno {other:?}"),
        }
    }

    #[test]
    fn compound_assignment() {
        assert!(matches!(parse_expr("x += 1"), Expr::Assign { op: AssignOp::Add, .. }));
        assert!(matches!(parse_expr("x *= 2"), Expr::Assign { op: AssignOp::Mul, .. }));
    }

    // --- deklarace promennych ---

    #[test]
    fn var_decl_let() {
        match parse_stmt("let x = 42;") {
            Stmt::Var { kind: VarKind::Let, decls } => {
                assert_eq!(decls.len(), 1);
                assert!(matches!(&decls[0].pattern, Pattern::Ident(n) if n == "x"));
                assert!(matches!(decls[0].init, Some(Expr::Number(n)) if n == 42.0));
            }
            other => panic!("Ocekavan VarDecl(Let), nalezeno {other:?}"),
        }
    }

    #[test]
    fn var_decl_const() {
        match parse_stmt("const PI = 3.14;") {
            Stmt::Var { kind: VarKind::Const, decls } => {
                assert!(matches!(&decls[0].pattern, Pattern::Ident(n) if n == "PI"));
            }
            other => panic!("Ocekavan VarDecl(Const), nalezeno {other:?}"),
        }
    }

    #[test]
    fn var_decl_without_init() {
        match parse_stmt("let x;") {
            Stmt::Var { kind: VarKind::Let, decls } => {
                assert!(decls[0].init.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    // --- funkce ---

    #[test]
    fn function_declaration() {
        match parse_stmt("function add(a, b) { return a + b; }") {
            Stmt::Function { name, params, .. } => {
                assert_eq!(name, "add");
                assert_eq!(params.iter().map(|p| p.name_str()).collect::<Vec<_>>(), vec!["a", "b"]);
            }
            other => panic!("Ocekavan Function, nalezeno {other:?}"),
        }
    }

    #[test]
    fn arrow_simple_param() {
        match parse_expr("x => x * 2") {
            Expr::Arrow { params, body: ArrowBody::Expr(_) } => {
                assert_eq!(params.iter().map(|p| p.name_str()).collect::<Vec<_>>(), vec!["x"]);
            }
            other => panic!("Ocekavan Arrow, nalezeno {other:?}"),
        }
    }

    #[test]
    fn arrow_paren_params() {
        match parse_expr("(a, b) => a + b") {
            Expr::Arrow { params, body: ArrowBody::Expr(_) } => {
                assert_eq!(params.iter().map(|p| p.name_str()).collect::<Vec<_>>(), vec!["a", "b"]);
            }
            other => panic!("Ocekavan Arrow, nalezeno {other:?}"),
        }
    }

    #[test]
    fn arrow_no_params() {
        match parse_expr("() => 42") {
            Expr::Arrow { params, body: ArrowBody::Expr(_) } => {
                assert!(params.is_empty());
            }
            other => panic!("Ocekavan Arrow, nalezeno {other:?}"),
        }
    }

    #[test]
    fn arrow_block_body() {
        match parse_expr("(x) => { return x; }") {
            Expr::Arrow { body: ArrowBody::Block(_), .. } => {}
            other => panic!("Ocekavan Arrow s blokem, nalezeno {other:?}"),
        }
    }

    // --- volani funkci a member access ---

    #[test]
    fn function_call() {
        match parse_expr("foo(1, 2)") {
            Expr::Call { callee, args, .. } => {
                assert!(matches!(*callee, Expr::Ident(s) if s == "foo"));
                assert_eq!(args.len(), 2);
            }
            other => panic!("Ocekavan Call, nalezeno {other:?}"),
        }
    }

    #[test]
    fn member_dot() {
        match parse_expr("obj.prop") {
            Expr::Member { object, prop: MemberProp::Ident(name), .. } => {
                assert!(matches!(*object, Expr::Ident(s) if s == "obj"));
                assert_eq!(name, "prop");
            }
            other => panic!("Ocekavan Member, nalezeno {other:?}"),
        }
    }

    #[test]
    fn member_computed() {
        match parse_expr("arr[0]") {
            Expr::Member { object, prop: MemberProp::Computed(idx), .. } => {
                assert!(matches!(*object, Expr::Ident(s) if s == "arr"));
                assert!(matches!(*idx, Expr::Number(n) if n == 0.0));
            }
            other => panic!("Ocekavan Member(Computed), nalezeno {other:?}"),
        }
    }

    // --- objekty a pole ---

    #[test]
    fn array_literal() {
        match parse_expr("[1, 2, 3]") {
            Expr::Array(items) => {
                assert_eq!(items.len(), 3);
                match &items[0] {
                    Some(e) => assert!(matches!(**e, Expr::Number(n) if n == 1.0)),
                    None => panic!("Ocekavan prvni prvek"),
                }
            }
            other => panic!("Ocekavano Array, nalezeno {other:?}"),
        }
    }

    #[test]
    fn object_literal() {
        // { ... } jako expression statement je block - treba obalit do ()
        match parse_expr("({ a: 1, b: 2 })") {
            Expr::Object(props) => {
                assert_eq!(props.len(), 2);
            }
            other => panic!("Ocekavan Object, nalezeno {other:?}"),
        }
    }

    // --- ridici struktury ---

    #[test]
    fn if_else() {
        match parse_stmt("if (x) { 1; } else { 2; }") {
            Stmt::If { test, no: Some(_), .. } => {
                assert!(matches!(test, Expr::Ident(s) if s == "x"));
            }
            other => panic!("Ocekavan If, nalezeno {other:?}"),
        }
    }

    #[test]
    fn while_loop() {
        match parse_stmt("while (true) {}") {
            Stmt::While { test, .. } => {
                assert!(matches!(test, Expr::Bool(true)));
            }
            other => panic!("Ocekavan While, nalezeno {other:?}"),
        }
    }

    #[test]
    fn for_loop() {
        match parse_stmt("for (let i = 0; i < 10; i++) {}") {
            Stmt::For { init: Some(_), test: Some(_), update: Some(_), .. } => {}
            other => panic!("Ocekavan For, nalezeno {other:?}"),
        }
    }

    #[test]
    fn return_stmt() {
        match parse_stmt("return 42;") {
            Stmt::Return(Some(Expr::Number(n))) => assert_eq!(n, 42.0),
            other => panic!("Ocekavan Return(42), nalezeno {other:?}"),
        }
    }

    // --- destrukturovani ---

    #[test]
    fn array_destructuring_decl() {
        match parse_stmt("const [a, b] = arr;") {
            Stmt::Var { kind: VarKind::Const, decls } => {
                assert_eq!(decls.len(), 1);
                match &decls[0].pattern {
                    Pattern::Array(elems) => {
                        assert_eq!(elems.len(), 2);
                        assert!(matches!(&elems[0].pattern, Some(Pattern::Ident(n)) if n == "a"));
                        assert!(matches!(&elems[1].pattern, Some(Pattern::Ident(n)) if n == "b"));
                    }
                    other => panic!("Ocekavan Array pattern, nalezeno {other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn object_destructuring_decl() {
        match parse_stmt("const { x, y } = obj;") {
            Stmt::Var { kind: VarKind::Const, decls } => {
                match &decls[0].pattern {
                    Pattern::Object(props) => {
                        assert_eq!(props.len(), 2);
                        assert!(props[0].shorthand);
                        assert!(props[1].shorthand);
                    }
                    other => panic!("Ocekavan Object pattern, nalezeno {other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn array_destructuring_with_default() {
        match parse_stmt("const [a = 10] = arr;") {
            Stmt::Var { decls, .. } => {
                match &decls[0].pattern {
                    Pattern::Array(elems) => {
                        assert!(elems[0].default.is_some());
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn array_destructuring_rest() {
        match parse_stmt("const [a, ...rest] = arr;") {
            Stmt::Var { decls, .. } => {
                match &decls[0].pattern {
                    Pattern::Array(elems) => {
                        assert!(!elems[0].rest);
                        assert!(elems[1].rest);
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn object_destructuring_renamed() {
        // { x: a } - klic x, pattern Ident("a"), shorthand = false
        match parse_stmt("const { x: a } = obj;") {
            Stmt::Var { decls, .. } => {
                match &decls[0].pattern {
                    Pattern::Object(props) => {
                        assert!(!props[0].shorthand);
                        assert!(matches!(&props[0].key, PropKey::Ident(k) if k == "x"));
                        assert!(matches!(&props[0].pattern, Pattern::Ident(n) if n == "a"));
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn function_params_destructuring() {
        match parse_stmt("function f([a, b], { x }) {}") {
            Stmt::Function { params, .. } => {
                assert!(matches!(&params[0].pattern, Pattern::Array(_)));
                assert!(matches!(&params[1].pattern, Pattern::Object(_)));
            }
            other => panic!("{other:?}"),
        }
    }

    // --- tridy ---

    #[test]
    fn class_basic() {
        match parse_stmt("class Foo { constructor(x) { this.x = x; } greet() { return this.x; } }") {
            Stmt::Class { name, super_class, body } => {
                assert_eq!(name, "Foo");
                assert!(super_class.is_none());
                assert_eq!(body.len(), 2);
                assert_eq!(body[0].name, "constructor");
                assert_eq!(body[1].name, "greet");
                assert!(!body[1].is_static);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn class_extends() {
        match parse_stmt("class Dog extends Animal { }") {
            Stmt::Class { name, super_class, .. } => {
                assert_eq!(name, "Dog");
                assert!(super_class.is_some());
                // super_class je Expr::Ident("Animal")
                assert!(matches!(super_class.as_deref(), Some(Expr::Ident(n)) if n == "Animal"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn class_static_method() {
        match parse_stmt("class Foo { static create() {} }") {
            Stmt::Class { body, .. } => {
                assert!(body[0].is_static);
                assert_eq!(body[0].name, "create");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn class_getter_setter() {
        match parse_stmt("class Foo { get value() {} set value(v) {} }") {
            Stmt::Class { body, .. } => {
                assert!(body[0].is_getter);
                assert!(body[1].is_setter);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn class_expression() {
        // V expressionu (const X = class { ... }) class nema jmeno
        let prog = parse("const X = class { constructor() {} }");
        match prog.body.into_iter().next().unwrap() {
            Stmt::Var { decls, .. } => {
                let init = decls[0].init.as_ref().unwrap();
                match init {
                    Expr::ClassExpr { name: None, body, .. } => {
                        assert_eq!(body.len(), 1);
                        assert_eq!(body[0].name, "constructor");
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn class_expression_named() {
        // Pojmenovana class expression: const X = class Foo { }
        let prog = parse("const X = class Foo { }");
        match prog.body.into_iter().next().unwrap() {
            Stmt::Var { decls, .. } => {
                let init = decls[0].init.as_ref().unwrap();
                match init {
                    Expr::ClassExpr { name: Some(n), .. } => {
                        assert_eq!(n, "Foo");
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }
}
