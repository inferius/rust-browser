/// 🔨 Parser - Převádí tokeny na AST (Abstract Syntax Tree)
///
/// Parser bere sekvenci tokenů z Lexeru a buduje z nich strom reprezentující
/// strukturu programu. Poté jej Evaluator interpretuje.

use crate::ast::*;
use crate::tokens::{Token, TokenKind, OperatorEnum, KeywordEnum};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub token_index: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        // Filtrujeme whitespace a komentáře
        let filtered: Vec<Token> = tokens
            .into_iter()
            .filter(|t| !matches!(
                t.kind,
                TokenKind::Whitespace | TokenKind::Newline | TokenKind::CommentLine(_) | TokenKind::CommentBlock(_)
            ))
            .collect();

        Parser {
            tokens: filtered,
            current: 0,
        }
    }

    /// Parsuje tokeny a vrací Program
    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program { statements })
    }

    // ═══════════════════════════════════════════════════════════════
    // STATEMENTS
    // ═══════════════════════════════════════════════════════════════

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek() {
            Some(token) => match &token.kind {
                TokenKind::Keyword(kw) => match kw {
                    KeywordEnum::Let | KeywordEnum::Const | KeywordEnum::Var => {
                        self.parse_variable_declaration()
                    }
                    KeywordEnum::Function => self.parse_function_declaration(),
                    KeywordEnum::If => self.parse_if_statement(),
                    KeywordEnum::While => self.parse_while_statement(),
                    KeywordEnum::For => self.parse_for_statement(),
                    KeywordEnum::Return => self.parse_return_statement(),
                    KeywordEnum::Break => {
                        self.advance();
                        self.consume_semicolon();
                        Ok(Statement::BreakStatement)
                    }
                    KeywordEnum::Continue => {
                        self.advance();
                        self.consume_semicolon();
                        Ok(Statement::ContinueStatement)
                    }
                    _ => self.parse_expression_statement(),
                },
                TokenKind::LeftBrace => self.parse_block_statement(),
                _ => self.parse_expression_statement(),
            },
            None => Err(self.error("Unexpected end of input")),
        }
    }

    fn parse_variable_declaration(&mut self) -> Result<Statement, ParseError> {
        let kind_token = self.advance().unwrap();
        let kind = match &kind_token.kind {
            TokenKind::Keyword(KeywordEnum::Let) => VarKind::Let,
            TokenKind::Keyword(KeywordEnum::Const) => VarKind::Const,
            TokenKind::Keyword(KeywordEnum::Var) => VarKind::Var,
            _ => return Err(self.error("Expected let, const, or var")),
        };

        let mut declarations = Vec::new();

        loop {
            let id = self.expect_identifier()?;

            let init = if self.check(&TokenKind::Operator(OperatorEnum::Assign)) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };

            declarations.push(Declarator { id, init });

            if !self.check(&TokenKind::Operator(OperatorEnum::Comma)) {
                break;
            }
            self.advance();
        }

        self.consume_semicolon();
        Ok(Statement::VariableDeclaration { kind, declarations })
    }

    fn parse_function_declaration(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(KeywordEnum::Function)?;

        let name = self.expect_identifier()?;
        self.expect_token(&TokenKind::LeftParen)?;

        let params = self.parse_parameter_list()?;

        self.expect_token(&TokenKind::RightParen)?;
        self.expect_token(&TokenKind::LeftBrace)?;

        let body = self.parse_statements_until(&TokenKind::RightBrace)?;

        self.expect_token(&TokenKind::RightBrace)?;

        Ok(Statement::FunctionDeclaration { name, params, body })
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(KeywordEnum::If)?;
        self.expect_token(&TokenKind::LeftParen)?;
        let test = self.parse_expression()?;
        self.expect_token(&TokenKind::RightParen)?;

        let consequent = vec![self.parse_statement()?];

        let alternate = if self.check(&TokenKind::Keyword(KeywordEnum::Else)) {
            self.advance();
            Some(vec![self.parse_statement()?])
        } else {
            None
        };

        Ok(Statement::IfStatement { test, consequent, alternate })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(KeywordEnum::While)?;
        self.expect_token(&TokenKind::LeftParen)?;
        let test = self.parse_expression()?;
        self.expect_token(&TokenKind::RightParen)?;

        let body = vec![self.parse_statement()?];

        Ok(Statement::WhileStatement { test, body })
    }

    fn parse_for_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(KeywordEnum::For)?;
        self.expect_token(&TokenKind::LeftParen)?;

        let init = if self.check(&TokenKind::Operator(OperatorEnum::Semicolon)) {
            None
        } else {
            Some(Box::new(self.parse_statement()?))
        };

        let test = if self.check(&TokenKind::Operator(OperatorEnum::Semicolon)) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_token(&TokenKind::Operator(OperatorEnum::Semicolon))?;

        let update = if self.check(&TokenKind::RightParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_token(&TokenKind::RightParen)?;

        let body = vec![self.parse_statement()?];

        Ok(Statement::ForStatement { init, test, update, body })
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(KeywordEnum::Return)?;

        let value = if self.check(&TokenKind::Operator(OperatorEnum::Semicolon)) || self.is_at_end() {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.consume_semicolon();
        Ok(Statement::ReturnStatement(value))
    }

    fn parse_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        Ok(Statement::ExpressionStatement(expr))
    }

    fn parse_block_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect_token(&TokenKind::LeftBrace)?;
        let statements = self.parse_statements_until(&TokenKind::RightBrace)?;
        self.expect_token(&TokenKind::RightBrace)?;
        Ok(Statement::BlockStatement(statements))
    }

    // ═══════════════════════════════════════════════════════════════
    // EXPRESSIONS
    // ═══════════════════════════════════════════════════════════════

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_ternary()?;

        if self.check(&TokenKind::Operator(OperatorEnum::Assign)) {
            self.advance();
            let right = Box::new(self.parse_assignment()?);
            return Ok(Expression::AssignmentExpression {
                left: Box::new(expr),
                right,
            });
        }

        Ok(expr)
    }

    fn parse_ternary(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_logical_or()?;

        if self.check(&TokenKind::Operator(OperatorEnum::Question)) {
            self.advance();
            let consequent = Box::new(self.parse_expression()?);
            self.expect_token(&TokenKind::Operator(OperatorEnum::Colon))?;
            let alternate = Box::new(self.parse_expression()?);

            return Ok(Expression::ConditionalExpression {
                test: Box::new(expr),
                consequent,
                alternate,
            });
        }

        Ok(expr)
    }

    fn parse_logical_or(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_logical_and()?;

        while let Some(token) = self.peek() {
            if self.check(&TokenKind::Operator(OperatorEnum::DoublePipe)) {
                self.advance();
                let right = Box::new(self.parse_logical_and()?);
                expr = Expression::LogicalExpression {
                    left: Box::new(expr),
                    operator: LogicalOperator::Or,
                    right,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_equality()?;

        while let Some(_) = self.peek() {
            if self.check(&TokenKind::Operator(OperatorEnum::DoubleAmpersand)) {
                self.advance();
                let right = Box::new(self.parse_equality()?);
                expr = Expression::LogicalExpression {
                    left: Box::new(expr),
                    operator: LogicalOperator::And,
                    right,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_comparison()?;

        while let Some(token) = self.peek() {
            let op = match &token.kind {
                TokenKind::Operator(OperatorEnum::DoubleEqual) => BinaryOperator::Equal,
                TokenKind::Operator(OperatorEnum::NotEqual) => BinaryOperator::NotEqual,
                TokenKind::Operator(OperatorEnum::TripleEqual) => BinaryOperator::StrictEqual,
                TokenKind::Operator(OperatorEnum::NotStrictEqual) => BinaryOperator::StrictNotEqual,
                _ => break,
            };
            self.advance();
            let right = Box::new(self.parse_comparison()?);
            expr = Expression::BinaryExpression {
                left: Box::new(expr),
                operator: op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_additive()?;

        while let Some(token) = self.peek() {
            let op = match &token.kind {
                TokenKind::Operator(OperatorEnum::Less) => BinaryOperator::Less,
                TokenKind::Operator(OperatorEnum::LessEqual) => BinaryOperator::LessEqual,
                TokenKind::Operator(OperatorEnum::Greater) => BinaryOperator::Greater,
                TokenKind::Operator(OperatorEnum::GreaterEqual) => BinaryOperator::GreaterEqual,
                _ => break,
            };
            self.advance();
            let right = Box::new(self.parse_additive()?);
            expr = Expression::BinaryExpression {
                left: Box::new(expr),
                operator: op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_multiplicative()?;

        while let Some(token) = self.peek() {
            let op = match &token.kind {
                TokenKind::Operator(OperatorEnum::Plus) => BinaryOperator::Add,
                TokenKind::Operator(OperatorEnum::Minus) => BinaryOperator::Subtract,
                _ => break,
            };
            self.advance();
            let right = Box::new(self.parse_multiplicative()?);
            expr = Expression::BinaryExpression {
                left: Box::new(expr),
                operator: op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_unary()?;

        while let Some(token) = self.peek() {
            let op = match &token.kind {
                TokenKind::Operator(OperatorEnum::Asterisk) => BinaryOperator::Multiply,
                TokenKind::Operator(OperatorEnum::Slash) => BinaryOperator::Divide,
                TokenKind::Operator(OperatorEnum::Percent) => BinaryOperator::Modulo,
                _ => break,
            };
            self.advance();
            let right = Box::new(self.parse_unary()?);
            expr = Expression::BinaryExpression {
                left: Box::new(expr),
                operator: op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        if let Some(token) = self.peek() {
            let op = match &token.kind {
                TokenKind::Operator(OperatorEnum::Minus) => Some(UnaryOperator::Minus),
                TokenKind::Operator(OperatorEnum::Plus) => Some(UnaryOperator::Plus),
                TokenKind::Operator(OperatorEnum::Exclamation) => Some(UnaryOperator::Not),
                TokenKind::Keyword(KeywordEnum::Typeof) => Some(UnaryOperator::Typeof),
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let argument = Box::new(self.parse_unary()?);
                return Ok(Expression::UnaryExpression { operator: op, argument });
            }
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_member()?;

        loop {
            if self.check(&TokenKind::LeftParen) {
                self.advance();
                let arguments = self.parse_argument_list()?;
                self.expect_token(&TokenKind::RightParen)?;
                expr = Expression::CallExpression {
                    callee: Box::new(expr),
                    arguments,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_member(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(&TokenKind::LeftBracket) {
                self.advance();
                let property = Box::new(self.parse_expression()?);
                self.expect_token(&TokenKind::RightBracket)?;
                expr = Expression::MemberExpression {
                    object: Box::new(expr),
                    property,
                    computed: true,
                };
            } else if self.check(&TokenKind::Operator(OperatorEnum::Dot)) {
                self.advance();
                let prop_name = self.expect_identifier()?;
                expr = Expression::MemberExpression {
                    object: Box::new(expr),
                    property: Box::new(Expression::Literal(Literal::String(prop_name))),
                    computed: false,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        match self.peek() {
            Some(token) => match &token.kind {
                TokenKind::NumericLiteral { value, .. } => {
                    let num = *value;
                    self.advance();
                    Ok(Expression::Literal(Literal::Number(num)))
                }
                TokenKind::StringLiteral { value, .. } => {
                    let str = value.clone();
                    self.advance();
                    Ok(Expression::Literal(Literal::String(str)))
                }
                TokenKind::Keyword(KeywordEnum::True) => {
                    self.advance();
                    Ok(Expression::Literal(Literal::Boolean(true)))
                }
                TokenKind::Keyword(KeywordEnum::False) => {
                    self.advance();
                    Ok(Expression::Literal(Literal::Boolean(false)))
                }
                TokenKind::Keyword(KeywordEnum::Null) => {
                    self.advance();
                    Ok(Expression::Literal(Literal::Null))
                }
                TokenKind::Keyword(KeywordEnum::Undefined) => {
                    self.advance();
                    Ok(Expression::Literal(Literal::Undefined))
                }
                TokenKind::Identifier(name) => {
                    let id = name.clone();
                    self.advance();
                    Ok(Expression::Identifier(id))
                }
                TokenKind::LeftParen => {
                    self.advance();
                    let expr = self.parse_expression()?;
                    self.expect_token(&TokenKind::RightParen)?;
                    Ok(expr)
                }
                TokenKind::LeftBrace => self.parse_object_literal(),
                TokenKind::LeftBracket => self.parse_array_literal(),
                _ => Err(self.error(&format!("Unexpected token: {:?}", token.kind))),
            },
            None => Err(self.error("Unexpected end of input")),
        }
    }

    fn parse_object_literal(&mut self) -> Result<Expression, ParseError> {
        self.expect_token(&TokenKind::LeftBrace)?;
        let mut properties = HashMap::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_identifier()?;
            self.expect_token(&TokenKind::Operator(OperatorEnum::Colon))?;
            let value = self.parse_expression()?;
            properties.insert(key, value);

            if !self.check(&TokenKind::RightBrace) {
                if self.check(&TokenKind::Operator(OperatorEnum::Comma)) {
                    self.advance();
                }
            }
        }

        self.expect_token(&TokenKind::RightBrace)?;
        Ok(Expression::ObjectExpression(properties))
    }

    fn parse_array_literal(&mut self) -> Result<Expression, ParseError> {
        self.expect_token(&TokenKind::LeftBracket)?;
        let mut elements = Vec::new();

        while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
            elements.push(self.parse_expression()?);
            if !self.check(&TokenKind::RightBracket) {
                self.expect_token(&TokenKind::Operator(OperatorEnum::Comma))?;
            }
        }

        self.expect_token(&TokenKind::RightBracket)?;
        Ok(Expression::ArrayExpression(elements))
    }

    // ═══════════════════════════════════════════════════════════════
    // HELPER FUNCTIONS
    // ═══════════════════════════════════════════════════════════════

    fn parse_parameter_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();

        while !self.check(&TokenKind::RightParen) && !self.is_at_end() {
            params.push(self.expect_identifier()?);
            if !self.check(&TokenKind::RightParen) {
                self.expect_token(&TokenKind::Operator(OperatorEnum::Comma))?;
            }
        }

        Ok(params)
    }

    fn parse_argument_list(&mut self) -> Result<Vec<Expression>, ParseError> {
        let mut args = Vec::new();

        while !self.check(&TokenKind::RightParen) && !self.is_at_end() {
            args.push(self.parse_expression()?);
            if !self.check(&TokenKind::RightParen) {
                self.expect_token(&TokenKind::Operator(OperatorEnum::Comma))?;
            }
        }

        Ok(args)
    }

    fn parse_statements_until(&mut self, until: &TokenKind) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        while !self.check(until) && !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(statements)
    }

    fn peek(&self) -> Option<&Token> {
        if self.current < self.tokens.len() {
            Some(&self.tokens[self.current])
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<&Token> {
        if self.current < self.tokens.len() {
            let token = &self.tokens[self.current];
            self.current += 1;
            Some(token)
        } else {
            None
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        match self.peek() {
            Some(token) => std::mem::discriminant(&token.kind) == std::mem::discriminant(kind),
            None => false,
        }
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len() ||
            matches!(self.peek(), Some(t) if matches!(t.kind, TokenKind::Eof))
    }

    fn expect_token(&mut self, kind: &TokenKind) -> Result<(), ParseError> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(&format!("Expected {:?}", kind)))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(token) => match &token.kind {
                TokenKind::Identifier(name) => {
                    let id = name.clone();
                    self.advance();
                    Ok(id)
                }
                _ => Err(self.error("Expected identifier")),
            },
            None => Err(self.error("Expected identifier, got EOF")),
        }
    }

    fn expect_keyword(&mut self, kw: KeywordEnum) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) => match &token.kind {
                TokenKind::Keyword(k) if k == &kw => {
                    self.advance();
                    Ok(())
                }
                _ => Err(self.error(&format!("Expected keyword {:?}", kw))),
            },
            None => Err(self.error("Expected keyword, got EOF")),
        }
    }

    fn consume_semicolon(&mut self) {
        if let Some(token) = self.peek() {
            if matches!(token.kind, TokenKind::Operator(OperatorEnum::Semicolon)) {
                self.advance();
            }
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            token_index: self.current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_variable_declaration() {
        // Budeme testovat až budeme mít tokeny z lexeru
    }
}
