use std::fmt;

use crate::lexer::{Token, TokenKind};
use crate::span::{SourcePosition, Span};

use super::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutput {
    pub program: Option<Program>,
    pub diagnostics: Vec<ParserDiagnostic>,
}

impl ParseOutput {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty() && self.program.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserDiagnostic {
    pub source_name: String,
    pub kind: ParserDiagnosticKind,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserDiagnosticKind {
    UnexpectedToken,
    ExpectedToken,
    InvalidImport,
    InvalidTypeSyntax,
    InvalidDeclaration,
    InvalidExpression,
    InvalidStatement,
    UnterminatedBlock,
    UnterminatedInterpolation,
    UnexpectedEof,
}

impl fmt::Display for ParserDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.source_name, self.span.start.line, self.span.start.column, self.message
        )
    }
}

pub fn parse(source_name: &str, tokens: &[Token]) -> ParseOutput {
    if tokens.is_empty() {
        let span = Span {
            start: SourcePosition {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: SourcePosition {
                offset: 0,
                line: 1,
                column: 1,
            },
        };
        return ParseOutput {
            program: None,
            diagnostics: vec![ParserDiagnostic {
                source_name: source_name.to_string(),
                kind: ParserDiagnosticKind::UnexpectedEof,
                message: "expected a token stream".to_string(),
                span,
            }],
        };
    }

    let mut parser = Parser {
        source_name: source_name.to_string(),
        tokens,
        index: 0,
        diagnostics: Vec::new(),
    };
    let program = parser.parse_program();
    ParseOutput {
        program: Some(program),
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    source_name: String,
    tokens: &'a [Token],
    index: usize,
    diagnostics: Vec<ParserDiagnostic>,
}

impl<'a> Parser<'a> {
    fn parse_program(&mut self) -> Program {
        let start = self.current().span.start;
        let mut imports = Vec::new();
        while self.check(TokenKind::KwImport) {
            if let Some(import) = self.parse_import() {
                imports.push(import);
            }
        }

        let mut functions = Vec::new();
        while !self.is_at_end() {
            let before = self.index;
            if self.check(TokenKind::KwConst) {
                let token = self.bump();
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    token.span,
                    "a constant must be declared inside a function",
                );
                self.synchronize_statement();
            } else if self.check(TokenKind::KwImport) {
                self.error(
                    ParserDiagnosticKind::InvalidImport,
                    self.current().span,
                    "imports must appear before declarations",
                );
                let _ = self.parse_import();
            } else if self.starts_type() {
                if let Some(function) = self.parse_function() {
                    functions.push(function);
                }
            } else {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    self.current().span,
                    format!(
                        "expected a function declaration, found {}",
                        self.describe_current()
                    ),
                );
                self.bump_unless_eof();
                self.synchronize_top_level();
            }
            if self.index == before && !self.is_at_end() {
                self.bump_unless_eof();
            }
        }

        let end = self.previous_end().unwrap_or(start);
        Program {
            imports,
            functions,
            span: Span { start, end },
        }
    }

    fn parse_import(&mut self) -> Option<ImportDecl> {
        let start = self.bump().span;
        let domain = self.expect_identifier("expected a domain name")?;
        if !self.consume(TokenKind::Dot, "expected '.' in import path") {
            self.synchronize_statement();
            return None;
        }
        let library = self.expect_identifier("expected a library name")?;
        if self.check(TokenKind::Dot) {
            self.error(
                ParserDiagnosticKind::InvalidImport,
                self.current().span,
                "an import path must be domain.library",
            );
            while self.check(TokenKind::Dot) || self.check(TokenKind::Identifier) {
                self.bump();
            }
        }
        let end = if self.consume(TokenKind::Semicolon, "expected ';' after import") {
            self.previous_end().unwrap_or(library.span.end)
        } else {
            library.span.end
        };
        Some(ImportDecl {
            domain,
            library,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_function(&mut self) -> Option<FunctionDecl> {
        let return_type = self.parse_type()?;
        let name = self.expect_identifier("expected a function name")?;
        if !self.consume(TokenKind::LeftParen, "expected '(' after function name") {
            self.synchronize_top_level();
            return None;
        }
        let parameters = self.parse_parameter_list();
        let _ = self.consume(TokenKind::RightParen, "expected ')' after parameters");
        let body = self.parse_block();
        let span = Span {
            start: return_type.span.start,
            end: body.span.end,
        };
        Some(FunctionDecl {
            return_type,
            name,
            parameters,
            body,
            span,
        })
    }

    fn parse_parameter_list(&mut self) -> Vec<Parameter> {
        let mut parameters = Vec::new();
        if self.check(TokenKind::RightParen) || self.check(TokenKind::LeftBrace) {
            return parameters;
        }
        loop {
            if self.check(TokenKind::RightParen) || self.check(TokenKind::LeftBrace) {
                break;
            }
            if !self.starts_type() && !self.check(TokenKind::KwAuto) {
                self.error(
                    ParserDiagnosticKind::ExpectedToken,
                    self.current().span,
                    "expected a parameter or ')'",
                );
                break;
            }
            let Some(parameter) = self.parse_parameter() else {
                break;
            };
            parameters.push(parameter);
            if self.check(TokenKind::Comma) {
                self.bump();
                if self.check(TokenKind::RightParen) {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        self.current().span,
                        "a trailing comma is not allowed in a parameter list",
                    );
                    break;
                }
                continue;
            }
            break;
        }
        parameters
    }

    fn parse_parameter(&mut self) -> Option<Parameter> {
        let start = self.current().span.start;
        let conversion = self.parse_conversion();
        let ty = self.parse_type()?;
        let name = self.expect_identifier("expected a parameter name")?;
        let span = Span {
            start,
            end: name.span.end,
        };
        Some(Parameter {
            name,
            ty,
            conversion,
            span,
        })
    }

    fn parse_block(&mut self) -> BlockStmt {
        if !self.check(TokenKind::LeftBrace) {
            self.error(
                ParserDiagnosticKind::UnterminatedBlock,
                self.current().span,
                "expected '{'",
            );
            let span = self.current().span;
            return BlockStmt {
                statements: Vec::new(),
                span,
            };
        }
        let start = self.bump().span.start;
        let mut statements = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            let before = self.index;
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            }
            if self.index == before && !self.is_at_end() && !self.check(TokenKind::RightBrace) {
                self.bump_unless_eof();
            }
        }
        let end = if self.check(TokenKind::RightBrace) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::UnterminatedBlock,
                self.current().span,
                "expected '}' to close block",
            );
            self.current().span.end
        };
        BlockStmt {
            statements,
            span: Span { start, end },
        }
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        if self.check(TokenKind::LeftBrace) {
            return Some(Stmt::Block(self.parse_block()));
        }
        if self.check(TokenKind::KwReturn) {
            return Some(self.parse_return());
        }
        if self.check(TokenKind::KwIf) {
            return Some(self.parse_if());
        }
        if self.check(TokenKind::KwLoop) {
            return Some(self.parse_loop());
        }
        if self.check(TokenKind::KwBreak) {
            return Some(self.parse_break_or_continue(true));
        }
        if self.check(TokenKind::KwContinue) {
            return Some(self.parse_break_or_continue(false));
        }
        if self.starts_declaration() {
            return self.parse_declaration_statement();
        }
        let expression = self.parse_expression();
        let start = expression.span().start;
        let end = if self.consume(TokenKind::Semicolon, "expected ';' after expression") {
            self.previous_end().unwrap_or(expression.span().end)
        } else {
            expression.span().end
        };
        Some(Stmt::Expression(ExpressionStmt {
            expression,
            span: Span { start, end },
        }))
    }

    fn parse_declaration_statement(&mut self) -> Option<Stmt> {
        let head = self.parse_binding_head()?;
        if self.check(TokenKind::Comma) {
            return self.finish_destructure(head).map(Stmt::Destructure);
        }
        self.finish_variable(head, true).map(Stmt::VariableDecl)
    }

    fn parse_binding_head(&mut self) -> Option<BindingHead> {
        let start = self.current().span.start;
        let mutable = !self.check(TokenKind::KwConst);
        if !mutable {
            self.bump();
        }
        let conversion = self.parse_conversion();
        let ty = self.parse_type()?;
        let name = self.expect_identifier("expected a variable name")?;
        if self.check(TokenKind::LeftParen) {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "nested functions are not supported",
            );
            self.skip_nested_function();
            return None;
        }
        let end = name.span.end;
        Some(BindingHead {
            name,
            ty,
            conversion,
            mutable,
            span: Span { start, end },
        })
    }

    fn finish_variable(
        &mut self,
        head: BindingHead,
        require_semicolon: bool,
    ) -> Option<VariableDecl> {
        let start = head.span.start;
        if !self.consume(TokenKind::Equal, "expected '=' in variable declaration") {
            if require_semicolon {
                self.synchronize_statement();
            }
            return None;
        }
        let initializer = self.parse_expression();
        let mut end = initializer.span().end;
        if require_semicolon
            && self.consume(
                TokenKind::Semicolon,
                "expected ';' after variable declaration",
            )
        {
            end = self.previous_end().unwrap_or(end);
        }
        Some(VariableDecl {
            name: head.name,
            ty: head.ty,
            conversion: head.conversion,
            initializer,
            mutable: head.mutable,
            span: Span { start, end },
        })
    }

    fn finish_destructure(&mut self, first: BindingHead) -> Option<DestructureDecl> {
        let start = first.span.start;
        let mut bindings = vec![first];
        while self.check(TokenKind::Comma) {
            self.bump();
            if self.check(TokenKind::Equal) || self.check(TokenKind::Semicolon) {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    self.current().span,
                    "a trailing comma is not allowed in a destructuring declaration",
                );
                break;
            }
            let Some(binding) = self.parse_binding_head() else {
                break;
            };
            if binding.conversion == ConversionMode::Auto {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    binding.span,
                    "auto is not allowed in a destructuring declaration",
                );
            }
            bindings.push(binding);
        }
        if bindings[0].conversion == ConversionMode::Auto {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                bindings[0].span,
                "auto is not allowed in a destructuring declaration",
            );
        }
        if !self.consume(
            TokenKind::Equal,
            "expected '=' in destructuring declaration",
        ) {
            self.synchronize_statement();
            return None;
        }
        let initializer = self.parse_expression();
        let mut end = initializer.span().end;
        if self.consume(
            TokenKind::Semicolon,
            "expected ';' after destructuring declaration",
        ) {
            end = self.previous_end().unwrap_or(end);
        }
        Some(DestructureDecl {
            bindings,
            initializer,
            span: Span { start, end },
        })
    }

    fn parse_return(&mut self) -> Stmt {
        let start = self.bump().span.start;
        let value = if self.can_start_expression() {
            Some(self.parse_expression())
        } else {
            None
        };
        let end = if self.consume(TokenKind::Semicolon, "expected ';' after return") {
            self.previous_end().unwrap_or(self.current().span.end)
        } else {
            value
                .as_ref()
                .map(Expr::span)
                .map(|span| span.end)
                .unwrap_or(self.current().span.end)
        };
        Stmt::Return(ReturnStmt {
            value,
            span: Span { start, end },
        })
    }

    fn parse_if(&mut self) -> Stmt {
        let start = self.bump().span.start;
        let _ = self.consume(TokenKind::LeftParen, "expected '(' after if");
        let condition = self.parse_expression();
        let _ = self.consume(TokenKind::RightParen, "expected ')' after if condition");
        let then_branch = self.parse_block();
        let else_branch = if self.check(TokenKind::KwElse) {
            self.bump();
            if self.check(TokenKind::KwIf) {
                Some(Box::new(self.parse_if()))
            } else {
                Some(Box::new(Stmt::Block(self.parse_block())))
            }
        } else {
            None
        };
        let end = else_branch
            .as_ref()
            .map(|statement| statement_span(statement).end)
            .unwrap_or(then_branch.span.end);
        Stmt::If(IfStmt {
            condition,
            then_branch,
            else_branch,
            span: Span { start, end },
        })
    }

    fn parse_loop(&mut self) -> Stmt {
        let start = self.bump().span.start;
        let _ = self.consume(TokenKind::LeftParen, "expected '(' after loop");
        let kind = if self.check(TokenKind::RightParen) {
            LoopKind::Infinite
        } else if self.check(TokenKind::Semicolon) {
            self.bump();
            self.parse_cstyle_tail(None)
        } else if self.starts_declaration() {
            if let Some(head) = self.parse_binding_head() {
                if self.check(TokenKind::Comma) {
                    self.parse_iteration(vec![head])
                } else if self.consume(TokenKind::Equal, "expected '=' in loop binding") {
                    let initializer = self.parse_expression();
                    if self.check(TokenKind::Semicolon) {
                        self.bump();
                        let declaration = VariableDecl {
                            name: head.name,
                            ty: head.ty,
                            conversion: head.conversion,
                            initializer,
                            mutable: head.mutable,
                            span: head.span,
                        };
                        self.parse_cstyle_tail(Some(declaration))
                    } else {
                        LoopKind::Iteration {
                            bindings: vec![head],
                            iterable: initializer,
                        }
                    }
                } else {
                    self.parse_cstyle_tail(None)
                }
            } else {
                self.parse_cstyle_tail(None)
            }
        } else {
            let condition = self.parse_expression();
            if self.check(TokenKind::Semicolon) {
                self.error(
                    ParserDiagnosticKind::InvalidStatement,
                    condition.span(),
                    "a C-style loop initializer must be a variable declaration",
                );
                self.bump();
                self.parse_cstyle_tail(None)
            } else {
                LoopKind::Condition { condition }
            }
        };
        let _ = self.consume(TokenKind::RightParen, "expected ')' after loop header");
        let body = self.parse_block();
        let end = body.span.end;
        Stmt::Loop(LoopStmt {
            kind,
            body,
            span: Span { start, end },
        })
    }

    fn parse_cstyle_tail(&mut self, initializer: Option<VariableDecl>) -> LoopKind {
        let condition = if self.can_start_expression() {
            Some(self.parse_expression())
        } else {
            None
        };
        let saw_second_semicolon =
            self.consume(TokenKind::Semicolon, "expected ';' after loop condition");
        let update = if self.can_start_expression() {
            Some(self.parse_expression())
        } else {
            None
        };
        if initializer.is_none() && condition.is_none() && update.is_none() && saw_second_semicolon
        {
            self.error(
                ParserDiagnosticKind::InvalidStatement,
                self.current().span,
                "loop (;;) is not supported; use loop ()",
            );
        }
        LoopKind::CStyle {
            initializer,
            condition,
            update,
        }
    }

    fn parse_iteration(&mut self, mut bindings: Vec<BindingHead>) -> LoopKind {
        while self.check(TokenKind::Comma) {
            self.bump();
            if self.check(TokenKind::Equal) || self.check(TokenKind::RightParen) {
                self.error(
                    ParserDiagnosticKind::InvalidStatement,
                    self.current().span,
                    "a trailing comma is not allowed in a loop binding",
                );
                break;
            }
            let Some(binding) = self.parse_binding_head() else {
                break;
            };
            if binding.conversion == ConversionMode::Auto {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    binding.span,
                    "auto is not allowed in a loop binding",
                );
            }
            bindings.push(binding);
        }
        if bindings[0].conversion == ConversionMode::Auto {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                bindings[0].span,
                "auto is not allowed in a loop binding",
            );
        }
        let iterable = if self.consume(TokenKind::Equal, "expected '=' in loop binding") {
            self.parse_expression()
        } else {
            Expr::Literal(LiteralExpr::Integer {
                lexeme: "0".to_string(),
                span: self.current().span,
            })
        };
        LoopKind::Iteration { bindings, iterable }
    }

    fn parse_break_or_continue(&mut self, is_break: bool) -> Stmt {
        let start = self.bump().span.start;
        let end = if self.consume(
            TokenKind::Semicolon,
            if is_break {
                "expected ';' after break"
            } else {
                "expected ';' after continue"
            },
        ) {
            self.previous_end().unwrap_or(self.current().span.end)
        } else {
            self.current().span.end
        };
        let span = Span { start, end };
        if is_break {
            Stmt::Break(span)
        } else {
            Stmt::Continue(span)
        }
    }

    fn parse_expression(&mut self) -> Expr {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Expr {
        let left = self.parse_or();
        if self.check(TokenKind::Equal) {
            let _operator = self.bump();
            let value = self.parse_assignment();
            let span = covering(left.span(), value.span());
            return Expr::Assignment(AssignmentExpr {
                target: Box::new(left),
                value: Box::new(value),
                span,
            });
        }
        left
    }

    fn parse_or(&mut self) -> Expr {
        self.parse_binary(&[TokenKind::OrOr], Self::parse_and, BinaryOperator::Or)
    }

    fn parse_and(&mut self) -> Expr {
        self.parse_binary(
            &[TokenKind::AndAnd],
            Self::parse_equality,
            BinaryOperator::And,
        )
    }

    fn parse_equality(&mut self) -> Expr {
        let mut left = self.parse_comparison();
        while matches!(
            self.current().kind,
            TokenKind::EqualEqual | TokenKind::BangEqual
        ) {
            let operator = if self.current().kind == TokenKind::EqualEqual {
                BinaryOperator::Equal
            } else {
                BinaryOperator::NotEqual
            };
            self.bump();
            let right = self.parse_comparison();
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut left = self.parse_additive();
        while matches!(
            self.current().kind,
            TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual
        ) {
            let operator = match self.current().kind {
                TokenKind::Less => BinaryOperator::Less,
                TokenKind::LessEqual => BinaryOperator::LessEqual,
                TokenKind::Greater => BinaryOperator::Greater,
                TokenKind::GreaterEqual => BinaryOperator::GreaterEqual,
                _ => unreachable!("comparison operator was checked"),
            };
            self.bump();
            let right = self.parse_additive();
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_additive(&mut self) -> Expr {
        let mut left = self.parse_multiplicative();
        while matches!(self.current().kind, TokenKind::Plus | TokenKind::Minus) {
            if self.reject_compound_or_arrow() {
                let _discarded = self.parse_multiplicative();
                continue;
            }
            let operator = if self.current().kind == TokenKind::Plus {
                BinaryOperator::Add
            } else {
                BinaryOperator::Subtract
            };
            self.bump();
            let right = self.parse_multiplicative();
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_multiplicative(&mut self) -> Expr {
        let mut left = self.parse_unary();
        while matches!(
            self.current().kind,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent
        ) {
            if self.peek_kind() == TokenKind::Equal {
                let spelled = match self.current().kind {
                    TokenKind::Star => "*=",
                    TokenKind::Slash => "/=",
                    TokenKind::Percent => "%=",
                    _ => "compound assignment",
                };
                self.reject_sequence(spelled);
                let _discarded = self.parse_unary();
                continue;
            }
            let operator = match self.current().kind {
                TokenKind::Star => BinaryOperator::Multiply,
                TokenKind::Slash => BinaryOperator::Divide,
                TokenKind::Percent => BinaryOperator::Remainder,
                _ => unreachable!("multiplicative operator was checked"),
            };
            self.bump();
            let right = self.parse_unary();
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        if self.check(TokenKind::Plus) && self.peek_kind() == TokenKind::Equal {
            self.reject_sequence("+=");
            return self.parse_unary();
        }
        if self.check(TokenKind::Minus) && self.peek_kind() == TokenKind::Equal {
            self.reject_sequence("-=");
            return self.parse_unary();
        }
        if self.check(TokenKind::Minus) && self.peek_kind() == TokenKind::Greater {
            self.reject_sequence("->");
            return self.parse_unary();
        }
        if self.check(TokenKind::Plus) {
            self.error(
                ParserDiagnosticKind::InvalidExpression,
                self.current().span,
                "unary '+' is not supported",
            );
            self.bump();
            return self.parse_unary();
        }
        if self.check(TokenKind::Bang) || self.check(TokenKind::Minus) {
            let operator = if self.check(TokenKind::Bang) {
                UnaryOperator::Not
            } else {
                UnaryOperator::Negate
            };
            let start = self.bump().span.start;
            let operand = self.parse_unary();
            let span = Span {
                start,
                end: operand.span().end,
            };
            return Expr::Unary(UnaryExpr {
                operator,
                operand: Box::new(operand),
                span,
            });
        }
        if self.check(TokenKind::PlusPlus) || self.check(TokenKind::MinusMinus) {
            return self.finish_update(UpdatePosition::Prefix);
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expression = self.parse_primary();
        loop {
            if self.check(TokenKind::LeftParen) {
                expression = self.finish_call(expression);
            } else if self.check(TokenKind::LeftBracket) {
                expression = self.parse_subscript(expression);
            } else if self.check(TokenKind::Dot) {
                let _dot = self.bump();
                if let Some(member) = self.expect_identifier("expected a name after '.'") {
                    let span = Span {
                        start: expression.span().start,
                        end: member.span.end,
                    };
                    expression = Expr::Member(MemberExpr {
                        object: Box::new(expression),
                        member,
                        span,
                    });
                } else {
                    break;
                }
            } else if self.check(TokenKind::PlusPlus) || self.check(TokenKind::MinusMinus) {
                let operator = self.bump();
                let span = Span {
                    start: expression.span().start,
                    end: operator.span.end,
                };
                expression = Expr::Update(UpdateExpr {
                    operator: update_operator(operator.kind),
                    operand: Box::new(expression),
                    position: UpdatePosition::Postfix,
                    span,
                });
            } else {
                break;
            }
        }
        expression
    }

    fn finish_update(&mut self, position: UpdatePosition) -> Expr {
        let operator = self.bump();
        let operand = self.parse_postfix();
        let span = match position {
            UpdatePosition::Prefix => Span {
                start: operator.span.start,
                end: operand.span().end,
            },
            UpdatePosition::Postfix => Span {
                start: operand.span().start,
                end: operator.span.end,
            },
        };
        Expr::Update(UpdateExpr {
            operator: update_operator(operator.kind),
            operand: Box::new(operand),
            position,
            span,
        })
    }

    fn finish_call(&mut self, callee: Expr) -> Expr {
        let start = callee.span().start;
        self.bump();
        let mut arguments = Vec::new();
        if !self.check(TokenKind::RightParen) && !self.is_at_end() {
            loop {
                if self.check(TokenKind::RightParen) {
                    self.error(
                        ParserDiagnosticKind::InvalidExpression,
                        self.current().span,
                        "a trailing comma is not allowed in an argument list",
                    );
                    break;
                }
                arguments.push(self.parse_expression());
                if self.check(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        let end = if self.check(TokenKind::RightParen) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected ')' after arguments",
            );
            self.current().span.end
        };
        Expr::Call(CallExpr {
            callee: Box::new(callee),
            arguments,
            span: Span { start, end },
        })
    }

    fn parse_array_literal(&mut self) -> Expr {
        let start = self.bump().span.start;
        let mut elements = Vec::new();
        if !self.check(TokenKind::RightBracket) && !self.is_at_end() {
            loop {
                if self.check(TokenKind::RightBracket) {
                    self.error(
                        ParserDiagnosticKind::InvalidExpression,
                        self.current().span,
                        "a trailing comma is not allowed in an array literal",
                    );
                    break;
                }
                elements.push(self.parse_expression());
                if self.check(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        let end = if self.check(TokenKind::RightBracket) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected ']' after array literal",
            );
            self.current().span.end
        };
        Expr::Array(ArrayExpr {
            elements,
            span: Span { start, end },
        })
    }

    fn parse_subscript(&mut self, object: Expr) -> Expr {
        let start = object.span().start;
        self.bump();
        let mut parts = Vec::<Option<Expr>>::new();
        let mut colons = 0u8;
        if !self.check(TokenKind::RightBracket) && !self.is_at_end() {
            loop {
                if self.check(TokenKind::Colon) || self.check(TokenKind::RightBracket) {
                    parts.push(None);
                } else {
                    parts.push(Some(self.parse_expression()));
                }
                if self.check(TokenKind::Colon) {
                    let colon = self.bump();
                    colons += 1;
                    if colons > 2 {
                        self.error(
                            ParserDiagnosticKind::InvalidExpression,
                            colon.span,
                            "a slice has at most two ':' separators",
                        );
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        let end = if self.check(TokenKind::RightBracket) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected ']' after index",
            );
            self.current().span.end
        };
        let span = Span { start, end };
        if colons == 0 {
            if let Some(Some(index)) = parts.pop() {
                if parts.is_empty() {
                    return Expr::Index(IndexExpr {
                        object: Box::new(object),
                        index: Box::new(index),
                        span,
                    });
                }
            }
            self.error(
                ParserDiagnosticKind::InvalidExpression,
                span,
                "expected an index expression",
            );
            return Expr::Index(IndexExpr {
                object: Box::new(object),
                index: Box::new(Expr::Literal(LiteralExpr::Integer {
                    lexeme: "0".to_string(),
                    span,
                })),
                span,
            });
        }
        let take = |parts: &mut Vec<Option<Expr>>, index: usize| {
            parts.get_mut(index).and_then(Option::take).map(Box::new)
        };
        let mut parts = parts;
        Expr::Slice(SliceExpr {
            object: Box::new(object),
            start: take(&mut parts, 0),
            stop: take(&mut parts, 1),
            step: take(&mut parts, 2),
            span,
        })
    }

    fn parse_primary(&mut self) -> Expr {
        match self.current().kind {
            TokenKind::Identifier => {
                let token = self.bump();
                Expr::Identifier(Identifier {
                    name: token.lexeme.clone(),
                    span: token.span,
                })
            }
            TokenKind::IntegerLiteral => {
                let token = self.bump();
                Expr::Literal(LiteralExpr::Integer {
                    lexeme: token.lexeme.clone(),
                    span: token.span,
                })
            }
            TokenKind::FloatLiteral => {
                let token = self.bump();
                Expr::Literal(LiteralExpr::Float {
                    lexeme: token.lexeme.clone(),
                    span: token.span,
                })
            }
            TokenKind::StringLiteral => {
                let token = self.bump();
                Expr::Literal(LiteralExpr::String {
                    value: token.decoded.clone().unwrap_or_default(),
                    span: token.span,
                })
            }
            TokenKind::KwTrue | TokenKind::KwFalse => {
                let token = self.bump();
                Expr::Literal(LiteralExpr::Boolean {
                    value: token.kind == TokenKind::KwTrue,
                    span: token.span,
                })
            }
            TokenKind::InterpolatedStringStart => self.parse_interpolated_string(),
            kind if self.starts_conversion_at(kind) => self.parse_explicit_conversion(),
            TokenKind::LeftBracket => self.parse_array_literal(),
            TokenKind::LeftParen => {
                let open = self.bump();
                let expression = self.parse_expression();
                let end = if self.check(TokenKind::RightParen) {
                    self.bump().span.end
                } else {
                    self.error(
                        ParserDiagnosticKind::ExpectedToken,
                        self.current().span,
                        "expected ')' after expression",
                    );
                    expression.span().end
                };
                Expr::Parenthesized {
                    expression: Box::new(expression),
                    span: Span {
                        start: open.span.start,
                        end,
                    },
                }
            }
            _ => {
                self.error(
                    ParserDiagnosticKind::InvalidExpression,
                    self.current().span,
                    "expected an expression",
                );
                if !self.is_boundary() {
                    let token = self.bump();
                    return Expr::Identifier(Identifier {
                        name: token.lexeme.clone(),
                        span: token.span,
                    });
                }
                Expr::Identifier(Identifier {
                    name: String::new(),
                    span: self.current().span,
                })
            }
        }
    }

    fn parse_interpolated_string(&mut self) -> Expr {
        let start_token = self.bump();
        let mut parts = Vec::new();
        while !self.check(TokenKind::StringEnd) && !self.is_at_end() {
            if self.check(TokenKind::StringText) {
                let token = self.bump();
                parts.push(StringPart::Text {
                    value: token
                        .decoded
                        .clone()
                        .unwrap_or_else(|| token.lexeme.clone()),
                    span: token.span,
                });
            } else if self.check(TokenKind::InterpolationStart) {
                let open = self.bump();
                if self.check(TokenKind::InterpolationEnd) {
                    self.error(
                        ParserDiagnosticKind::UnterminatedInterpolation,
                        open.span,
                        "expected an expression inside interpolation",
                    );
                    let close = self.bump();
                    parts.push(StringPart::Expression {
                        expression: Expr::Identifier(Identifier {
                            name: String::new(),
                            span: Span {
                                start: open.span.start,
                                end: close.span.end,
                            },
                        }),
                        span: Span {
                            start: open.span.start,
                            end: close.span.end,
                        },
                    });
                    continue;
                }
                let expression = self.parse_expression();
                let end = if self.check(TokenKind::InterpolationEnd) {
                    self.bump().span.end
                } else {
                    self.error(
                        ParserDiagnosticKind::UnterminatedInterpolation,
                        self.current().span,
                        "expected '}' to close interpolation",
                    );
                    self.current().span.end
                };
                let span = Span {
                    start: open.span.start,
                    end,
                };
                parts.push(StringPart::Expression { expression, span });
            } else {
                self.error(
                    ParserDiagnosticKind::UnterminatedInterpolation,
                    self.current().span,
                    "expected the end of an interpolated string",
                );
                break;
            }
        }
        let end = if self.check(TokenKind::StringEnd) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::UnterminatedInterpolation,
                self.current().span,
                "unterminated interpolated string",
            );
            self.current().span.end
        };
        Expr::InterpolatedString(InterpolatedStringExpr {
            parts,
            span: Span {
                start: start_token.span.start,
                end,
            },
        })
    }

    fn parse_binary(
        &mut self,
        operators: &[TokenKind],
        mut parse_higher: impl FnMut(&mut Self) -> Expr,
        operator: BinaryOperator,
    ) -> Expr {
        let mut left = parse_higher(self);
        while operators.contains(&self.current().kind) {
            self.bump();
            let right = parse_higher(self);
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_conversion(&mut self) -> ConversionMode {
        if self.check(TokenKind::KwAuto) {
            self.bump();
            ConversionMode::Auto
        } else {
            ConversionMode::Exact
        }
    }

    fn parse_type(&mut self) -> Option<TypeSyntax> {
        let Some(kind) = type_kind(self.current().kind) else {
            self.error(
                ParserDiagnosticKind::InvalidTypeSyntax,
                self.current().span,
                "expected a type",
            );
            return None;
        };
        let token = self.bump();
        let mut bounds = Vec::new();
        let mut end = token.span.end;
        while self.check(TokenKind::LeftBracket) {
            if bounds.len() == MAX_ARRAY_RANK {
                self.error(
                    ParserDiagnosticKind::InvalidTypeSyntax,
                    self.current().span,
                    "array type is too deeply nested",
                );
                break;
            }
            self.bump();
            if self.check(TokenKind::RightBracket) {
                end = self.bump().span.end;
                bounds.push(None);
                continue;
            }
            if !self.check(TokenKind::IntegerLiteral) {
                self.error(
                    ParserDiagnosticKind::InvalidTypeSyntax,
                    self.current().span,
                    "an array size must be an integer literal",
                );
                break;
            }
            let literal = self.bump();
            let bound = match literal.lexeme.parse::<u64>() {
                Ok(value) => value,
                Err(_) => {
                    self.error(
                        ParserDiagnosticKind::InvalidTypeSyntax,
                        literal.span,
                        "array maximum does not fit in uint64",
                    );
                    0
                }
            };
            if self.check(TokenKind::RightBracket) {
                end = self.bump().span.end;
            } else {
                self.error(
                    ParserDiagnosticKind::ExpectedToken,
                    self.current().span,
                    "expected ']'",
                );
                end = literal.span.end;
            }
            bounds.push(Some(bound));
        }
        Some(TypeSyntax {
            kind,
            bounds,
            span: Span {
                start: token.span.start,
                end,
            },
        })
    }

    fn expect_identifier(&mut self, message: &str) -> Option<Identifier> {
        if self.check(TokenKind::Identifier) {
            let token = self.bump();
            return Some(Identifier {
                name: token.lexeme.clone(),
                span: token.span,
            });
        }
        self.error(
            ParserDiagnosticKind::ExpectedToken,
            self.current().span,
            message,
        );
        None
    }

    fn reject_compound_or_arrow(&mut self) -> bool {
        let kind = self.current().kind;
        let next = self.peek_kind();
        if kind == TokenKind::Plus && next == TokenKind::Equal {
            self.reject_sequence("+=");
            return true;
        }
        if kind == TokenKind::Minus && next == TokenKind::Equal {
            self.reject_sequence("-=");
            return true;
        }
        if kind == TokenKind::Minus && next == TokenKind::Greater {
            self.reject_sequence("->");
            return true;
        }
        false
    }

    fn reject_sequence(&mut self, spelled: &str) {
        let start = self.current().span;
        self.bump();
        let end = if !self.is_at_end() {
            self.bump().span.end
        } else {
            start.end
        };
        self.error(
            ParserDiagnosticKind::InvalidExpression,
            Span {
                start: start.start,
                end,
            },
            format!("'{spelled}' is not supported"),
        );
    }

    fn starts_type(&self) -> bool {
        type_kind(self.current().kind).is_some()
    }

    fn starts_declaration(&self) -> bool {
        if self.check(TokenKind::KwConst) || self.check(TokenKind::KwAuto) {
            return true;
        }
        self.starts_type() && self.peek_kind() != TokenKind::LeftParen
    }

    fn starts_conversion_at(&self, kind: TokenKind) -> bool {
        type_kind(kind).is_some() && self.peek_kind() == TokenKind::LeftParen
    }

    fn can_start_expression(&self) -> bool {
        self.starts_conversion_at(self.current().kind)
            || matches!(
                self.current().kind,
                TokenKind::Identifier
                    | TokenKind::IntegerLiteral
                    | TokenKind::FloatLiteral
                    | TokenKind::StringLiteral
                    | TokenKind::KwTrue
                    | TokenKind::KwFalse
                    | TokenKind::InterpolatedStringStart
                    | TokenKind::LeftParen
                    | TokenKind::LeftBracket
                    | TokenKind::Bang
                    | TokenKind::Minus
                    | TokenKind::Plus
                    | TokenKind::PlusPlus
                    | TokenKind::MinusMinus
            )
    }

    fn parse_explicit_conversion(&mut self) -> Expr {
        let token = self.bump();
        let target = TypeSyntax {
            kind: type_kind(token.kind).expect("conversion target was checked"),
            bounds: Vec::new(),
            span: token.span,
        };
        self.bump();
        let mut arguments = Vec::new();
        if !self.check(TokenKind::RightParen) && !self.is_at_end() {
            loop {
                if self.check(TokenKind::RightParen) {
                    self.error(
                        ParserDiagnosticKind::InvalidExpression,
                        self.current().span,
                        "a trailing comma is not allowed in an argument list",
                    );
                    break;
                }
                arguments.push(self.parse_expression());
                if self.check(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        let end = if self.check(TokenKind::RightParen) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected ')' after conversion argument",
            );
            self.current().span.end
        };
        Expr::Conversion(ConversionExpr {
            target,
            arguments,
            span: Span {
                start: token.span.start,
                end,
            },
        })
    }

    fn is_boundary(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Semicolon
                | TokenKind::RightParen
                | TokenKind::RightBrace
                | TokenKind::Comma
                | TokenKind::Eof
        )
    }

    fn skip_nested_function(&mut self) {
        while !self.is_at_end()
            && !self.check(TokenKind::LeftBrace)
            && !self.check(TokenKind::Semicolon)
            && !self.check(TokenKind::RightBrace)
        {
            self.bump();
        }
        if self.check(TokenKind::LeftBrace) {
            self.bump();
            let mut depth = 1u32;
            while !self.is_at_end() && depth > 0 {
                if self.check(TokenKind::LeftBrace) {
                    depth += 1;
                } else if self.check(TokenKind::RightBrace) {
                    depth -= 1;
                }
                self.bump();
            }
        } else if self.check(TokenKind::Semicolon) {
            self.bump();
        }
    }

    fn synchronize_statement(&mut self) {
        while !self.is_at_end()
            && !self.check(TokenKind::Semicolon)
            && !self.check(TokenKind::RightBrace)
        {
            self.bump();
        }
        if self.check(TokenKind::Semicolon) {
            self.bump();
        }
    }

    fn synchronize_top_level(&mut self) {
        while !self.is_at_end() && !self.starts_type() && !self.check(TokenKind::KwImport) {
            if self.check(TokenKind::RightBrace) {
                self.bump();
                continue;
            }
            self.bump();
        }
    }

    fn consume(&mut self, kind: TokenKind, message: &str) -> bool {
        if self.check(kind) {
            self.bump();
            true
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                message,
            );
            false
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn is_at_end(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn current(&self) -> &Token {
        self.tokens
            .get(self.index)
            .or_else(|| self.tokens.last())
            .expect("parser token stream is empty")
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens
            .get(self.index + 1)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        if token.kind != TokenKind::Eof && self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn bump_unless_eof(&mut self) {
        if !self.is_at_end() {
            self.bump();
        }
    }

    fn previous_end(&self) -> Option<SourcePosition> {
        if self.index == 0 {
            return None;
        }
        self.tokens.get(self.index - 1).map(|token| token.span.end)
    }

    fn describe_current(&self) -> String {
        let token = self.current();
        if token.lexeme.is_empty() {
            format!("{:?}", token.kind)
        } else {
            format!("{:?} '{}'", token.kind, token.lexeme)
        }
    }

    fn error(&mut self, kind: ParserDiagnosticKind, span: Span, message: impl Into<String>) {
        let message = message.into();
        if self.diagnostics.last().is_some_and(|diagnostic| {
            diagnostic.kind == kind
                && diagnostic.span.start.offset == span.start.offset
                && diagnostic.message == message
        }) {
            return;
        }
        self.diagnostics.push(ParserDiagnostic {
            source_name: self.source_name.clone(),
            kind,
            message,
            span,
        });
    }
}

fn type_kind(kind: TokenKind) -> Option<TypeKind> {
    Some(match kind {
        TokenKind::KwVoid => TypeKind::Void,
        TokenKind::KwInt => TypeKind::Int,
        TokenKind::KwUint => TypeKind::UInt,
        TokenKind::KwFlt => TypeKind::Flt,
        TokenKind::KwInt16 => TypeKind::Int16,
        TokenKind::KwInt32 => TypeKind::Int32,
        TokenKind::KwInt64 => TypeKind::Int64,
        TokenKind::KwUint16 => TypeKind::UInt16,
        TokenKind::KwUint32 => TypeKind::UInt32,
        TokenKind::KwUint64 => TypeKind::UInt64,
        TokenKind::KwFlt32 => TypeKind::Flt32,
        TokenKind::KwFlt64 => TypeKind::Flt64,
        TokenKind::KwBln => TypeKind::Bln,
        TokenKind::KwChr => TypeKind::Chr,
        TokenKind::KwStr => TypeKind::Str,
        _ => return None,
    })
}

fn update_operator(kind: TokenKind) -> UpdateOperator {
    if kind == TokenKind::MinusMinus {
        UpdateOperator::Decrement
    } else {
        UpdateOperator::Increment
    }
}

fn binary(left: Expr, operator: BinaryOperator, right: Expr) -> Expr {
    let span = covering(left.span(), right.span());
    Expr::Binary(BinaryExpr {
        left: Box::new(left),
        operator,
        right: Box::new(right),
        span,
    })
}

fn covering(left: Span, right: Span) -> Span {
    Span {
        start: left.start,
        end: right.end,
    }
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Block(block) => block.span,
        Stmt::VariableDecl(declaration) => declaration.span,
        Stmt::Destructure(declaration) => declaration.span,
        Stmt::Return(return_statement) => return_statement.span,
        Stmt::If(if_statement) => if_statement.span,
        Stmt::Loop(loop_statement) => loop_statement.span,
        Stmt::Break(span) | Stmt::Continue(span) => *span,
        Stmt::Expression(expression) => expression.span,
    }
}
