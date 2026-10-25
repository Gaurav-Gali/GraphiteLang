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

impl ParserDiagnostic {
    pub fn code(&self) -> &'static str {
        if self.message.contains("is a reserved keyword") {
            return "GRF-NAM-003";
        }
        if self.message.contains("explicit throw") {
            return "GRF-EXC-007";
        }
        if self.message.contains("catch must")
            || self.message.contains("only one finally")
            || self.message.contains("must follow a try")
        {
            return "GRF-EXC-004";
        }
        match self.kind {
            ParserDiagnosticKind::ExpectedToken => "GRF-PAR-001",
            ParserDiagnosticKind::UnexpectedToken => "GRF-PAR-002",
            ParserDiagnosticKind::InvalidImport => "GRF-PAR-003",
            ParserDiagnosticKind::InvalidTypeSyntax => "GRF-PAR-004",
            ParserDiagnosticKind::InvalidDeclaration => "GRF-PAR-005",
            ParserDiagnosticKind::InvalidExpression => "GRF-PAR-006",
            ParserDiagnosticKind::InvalidStatement => "GRF-PAR-007",
            ParserDiagnosticKind::UnterminatedBlock => "GRF-PAR-008",
            ParserDiagnosticKind::UnterminatedInterpolation => "GRF-PAR-009",
            ParserDiagnosticKind::UnexpectedEof => "GRF-PAR-010",
        }
    }
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
        soft_terminator: 0,
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
    /// Unbraced lambda bodies leave `,` `)` and `;` for the enclosing syntax.
    soft_terminator: u32,
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

        let mut types = Vec::new();
        let mut classes = Vec::new();
        let mut interfaces = Vec::new();
        let mut functions = Vec::new();
        while !self.is_at_end() {
            let before = self.index;
            if self.check(TokenKind::KwType) {
                if let Some(declaration) = self.parse_type_decl() {
                    types.push(declaration);
                }
            } else if self.check(TokenKind::At)
                || self.check(TokenKind::KwAbstract)
                || self.check(TokenKind::KwClass)
                || self.check(TokenKind::KwInterface)
            {
                let mods = self.parse_decl_mods();
                if self.check(TokenKind::KwInterface) {
                    if mods.is_static || mods.is_override || mods.final_class {
                        self.error(
                            ParserDiagnosticKind::InvalidDeclaration,
                            self.current().span,
                            "an interface cannot be static, final, or an override",
                        );
                    }
                    if let Some(declaration) = self.parse_interface() {
                        interfaces.push(declaration);
                    }
                } else if self.check(TokenKind::KwClass) {
                    if mods.is_static || mods.is_override {
                        self.error(
                            ParserDiagnosticKind::InvalidDeclaration,
                            self.current().span,
                            "a class cannot be static or an override",
                        );
                    }
                    if let Some(declaration) =
                        self.parse_class(mods.abstract_class, mods.final_class)
                    {
                        classes.push(declaration);
                    }
                } else {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        self.current().span,
                        "expected a class or interface declaration",
                    );
                    self.bump_unless_eof();
                }
            } else if self.check(TokenKind::KwConst) {
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
            } else if self.starts_function() {
                if let Some(function) = self.parse_function() {
                    functions.push(function);
                }
            } else {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    self.current().span,
                    format!(
                        "expected a function or class declaration, found {}",
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
            types,
            classes,
            interfaces,
            functions,
            span: Span { start, end },
        }
    }

    fn parse_decl_mods(&mut self) -> DeclMods {
        let mut mods = DeclMods::default();
        loop {
            if self.check(TokenKind::At) {
                let name = self.parse_annotation();
                match name.as_deref() {
                    Some("abstract") => mods.abstract_class = true,
                    Some("final") => mods.final_class = true,
                    Some("static") => mods.is_static = true,
                    Some("override") => mods.is_override = true,
                    Some("deprecated") => {}
                    Some(other) => self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        self.previous_span(),
                        format!("unknown annotation '{other}'"),
                    ),
                    None => {}
                }
            } else if self.check(TokenKind::KwAbstract) {
                mods.abstract_class = true;
                self.bump();
            } else {
                break;
            }
        }
        mods
    }

    fn parse_annotation(&mut self) -> Option<String> {
        self.bump();
        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            let token = self.bump();
            Some(token.lexeme.clone())
        } else {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "expected an annotation name",
            );
            None
        }
    }

    fn parse_member_mods(&mut self) -> MemberMods {
        let mut mods = MemberMods::default();
        loop {
            if self.check(TokenKind::At) {
                let name = self.parse_annotation();
                match name.as_deref() {
                    Some("static") => mods.is_static = true,
                    Some("override") => mods.is_override = true,
                    Some("abstract") => mods.is_abstract = true,
                    Some("final") => mods.is_final = true,
                    Some("deprecated") => mods.deprecated = true,
                    Some(other) => self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        self.previous_span(),
                        format!("unknown annotation '{other}'"),
                    ),
                    None => {}
                }
            } else if self.check(TokenKind::KwPublic)
                || self.check(TokenKind::KwPrivate)
                || self.check(TokenKind::KwProtected)
            {
                if mods.saw_visibility {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        self.current().span,
                        "a member can have only one visibility",
                    );
                }
                mods.public = self.check(TokenKind::KwPublic);
                mods.protected = self.check(TokenKind::KwProtected);
                mods.saw_visibility = true;
                self.bump();
            } else if self.check(TokenKind::KwStatic) {
                mods.is_static = true;
                self.bump();
            } else {
                break;
            }
        }
        mods
    }

    fn previous_span(&self) -> Span {
        self.tokens
            .get(self.index.saturating_sub(1))
            .map(|token| token.span)
            .unwrap_or_else(|| self.current().span)
    }

    fn parse_class(&mut self, abstract_class: bool, final_class: bool) -> Option<ClassDecl> {
        let start = self.bump().span.start;
        let name = self.expect_identifier("expected a class name")?;
        let type_params = self.parse_type_parameters();
        let mut parent = None;
        let mut interfaces = Vec::new();
        if self.check(TokenKind::KwExtends) {
            self.bump();
            parent = self.expect_identifier("expected a parent class name");
            self.reject_generic_inheritance();
        }
        if self.check(TokenKind::KwImplements) {
            self.bump();
            loop {
                let Some(interface) = self.expect_identifier("expected an interface name") else {
                    break;
                };
                self.reject_generic_inheritance();
                interfaces.push(interface);
                if !self.check(TokenKind::Comma) {
                    break;
                }
                self.bump();
                if !self.check(TokenKind::Identifier) && !self.current().kind.is_keyword() {
                    break;
                }
            }
        }
        if !self.consume(TokenKind::LeftBrace, "expected '{' after class name") {
            self.synchronize_top_level();
            return None;
        }
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut init = None;
        while !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            let before = self.index;
            self.parse_class_member(&name.name, &mut fields, &mut methods, &mut init);
            if self.index == before && !self.is_at_end() && !self.check(TokenKind::RightBrace) {
                self.bump_unless_eof();
            }
        }
        let end = if self.check(TokenKind::RightBrace) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected '}' after class members",
            );
            self.current().span.end
        };
        Some(ClassDecl {
            name,
            type_params,
            parent,
            interfaces,
            abstract_class,
            final_class,
            fields,
            init,
            methods,
            span: Span { start, end },
        })
    }

    fn parse_interface(&mut self) -> Option<InterfaceDecl> {
        let start = self.bump().span.start;
        let name = self.expect_identifier("expected an interface name")?;
        if !self.consume(TokenKind::LeftBrace, "expected '{' after interface name") {
            self.synchronize_top_level();
            return None;
        }
        let mut methods = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            let before = self.index;
            if let Some(method) = self.parse_interface_method() {
                methods.push(method);
            }
            if self.index == before && !self.is_at_end() && !self.check(TokenKind::RightBrace) {
                self.bump_unless_eof();
            }
        }
        let end = if self.check(TokenKind::RightBrace) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected '}' after interface members",
            );
            self.current().span.end
        };
        Some(InterfaceDecl {
            name,
            methods,
            span: Span { start, end },
        })
    }

    fn parse_interface_method(&mut self) -> Option<InterfaceMethod> {
        let mods = self.parse_member_mods();
        if mods.is_static
            || mods.is_override
            || mods.is_abstract
            || mods.is_final
            || mods.saw_visibility
        {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "an interface method is a public contract",
            );
        }
        let start = self.current().span.start;
        if !self.starts_type() {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "expected an interface method",
            );
            self.bump_unless_eof();
            return None;
        }
        let return_type = self.parse_type()?;
        let name = self.expect_identifier("expected a method name")?;
        if !self.consume(TokenKind::LeftParen, "expected '(' after method name") {
            return None;
        }
        let parameters = self.parse_parameter_list();
        let _ = self.consume(TokenKind::RightParen, "expected ')' after parameters");
        if self.check(TokenKind::LeftBrace) {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "an interface method cannot have a body",
            );
            let _ = self.parse_block();
        } else {
            let _ = self.consume(TokenKind::Semicolon, "expected ';' after interface method");
        }
        let end = self.previous_end().unwrap_or(name.span.end);
        Some(InterfaceMethod {
            return_type,
            name,
            parameters,
            deprecated: mods.deprecated,
            span: Span { start, end },
        })
    }

    fn parse_class_member(
        &mut self,
        class_name: &str,
        fields: &mut Vec<ClassField>,
        methods: &mut Vec<ClassMethod>,
        init: &mut Option<ClassInit>,
    ) {
        let member_start = self.current().span.start;
        let mods = self.parse_member_mods();
        let visible = !mods.saw_visibility || mods.public;
        if self.check(TokenKind::KwInit) {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                format!("the constructor is '{class_name}'"),
            );
            self.bump();
            if self.check(TokenKind::LeftParen) {
                self.bump();
                let _ = self.parse_parameter_list();
                let _ = self.consume(TokenKind::RightParen, "expected ')' after parameters");
            }
            if self.check(TokenKind::LeftBrace) {
                let _ = self.parse_block();
            }
            return;
        }
        if self.check(TokenKind::Identifier)
            && self.current().lexeme == class_name
            && self.peek_kind() == TokenKind::LeftParen
        {
            if mods.is_static || mods.is_override || mods.is_abstract || mods.is_final {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    self.current().span,
                    "the constructor cannot be static, abstract, final, or an override",
                );
            }
            self.finish_constructor(visible, mods.protected, mods.deprecated, init);
            return;
        }
        let const_field = if self.check(TokenKind::KwConst) {
            self.bump();
            true
        } else {
            false
        };
        if self.check(TokenKind::Identifier) && self.peek_kind() == TokenKind::Colon {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "class fields use Type name",
            );
            while !self.check(TokenKind::Semicolon)
                && !self.check(TokenKind::RightBrace)
                && !self.is_at_end()
            {
                self.bump();
            }
            let _ = self.consume(TokenKind::Semicolon, "expected ';' after field");
            return;
        }
        if self.starts_type() || self.check(TokenKind::KwAuto) {
            let conversion = self.parse_conversion();
            let Some(ty) = self.parse_type() else {
                return;
            };
            let Some(name) = self.expect_identifier("expected a name") else {
                return;
            };
            if self.check(TokenKind::Semicolon) || self.check(TokenKind::Equal) {
                let initializer = if self.check(TokenKind::Equal) {
                    self.bump();
                    Some(self.parse_expression())
                } else {
                    None
                };
                let end = if self.consume(TokenKind::Semicolon, "expected ';' after field") {
                    self.previous_end().unwrap_or(name.span.end)
                } else {
                    name.span.end
                };
                if mods.is_override || mods.is_abstract || mods.is_final {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        name.span,
                        "a field cannot be abstract, final, or an override",
                    );
                }
                fields.push(ClassField {
                    span: Span {
                        start: member_start,
                        end,
                    },
                    name,
                    ty,
                    conversion,
                    const_field,
                    public: visible,
                    protected: mods.protected,
                    is_static: mods.is_static,
                    deprecated: mods.deprecated,
                    initializer,
                });
                return;
            }
            if const_field {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    name.span,
                    "const is only valid on a field",
                );
            }
            if conversion == ConversionMode::Auto {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    name.span,
                    "auto is only valid on a field",
                );
            }
            let type_params = self.parse_type_parameters();
            if !self.consume(TokenKind::LeftParen, "expected '(' after method name") {
                return;
            }
            let parameters = self.parse_parameter_list();
            let _ = self.consume(TokenKind::RightParen, "expected ')' after parameters");
            let body = if self.check(TokenKind::Semicolon) {
                let end = self.bump().span.end;
                if !mods.is_abstract {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        name.span,
                        "expected a method body",
                    );
                }
                BlockStmt {
                    statements: Vec::new(),
                    span: Span { start: end, end },
                }
            } else {
                if mods.is_abstract && self.check(TokenKind::LeftBrace) {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        self.current().span,
                        "an abstract method cannot have a body",
                    );
                }
                self.parse_body()
            };
            methods.push(ClassMethod {
                public: visible,
                protected: mods.protected,
                is_static: mods.is_static,
                is_override: mods.is_override,
                is_abstract: mods.is_abstract,
                is_final: mods.is_final,
                deprecated: mods.deprecated,
                span: Span {
                    start: member_start,
                    end: body.span.end,
                },
                return_type: ty,
                name,
                type_params,
                parameters,
                body,
            });
            return;
        }
        self.error(
            ParserDiagnosticKind::InvalidDeclaration,
            self.current().span,
            "expected a field, method, or constructor",
        );
        self.bump_unless_eof();
    }

    fn finish_constructor(
        &mut self,
        public: bool,
        protected: bool,
        deprecated: bool,
        init: &mut Option<ClassInit>,
    ) {
        let start = self.current().span.start;
        self.bump();
        if !self.consume(TokenKind::LeftParen, "expected '(' after the constructor") {
            return;
        }
        let parameters = self.parse_parameter_list();
        let _ = self.consume(TokenKind::RightParen, "expected ')' after parameters");
        let body = self.parse_body();
        let constructor = ClassInit {
            public,
            protected,
            deprecated,
            parameters,
            span: Span {
                start,
                end: body.span.end,
            },
            body,
        };
        if init.is_some() {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                constructor.span,
                "duplicate constructor",
            );
        } else {
            *init = Some(constructor);
        }
    }

    fn parse_type_decl(&mut self) -> Option<TypeDecl> {
        let start = self.bump().span.start;
        let name = self.expect_identifier("expected a record type name")?;
        if !self.consume(TokenKind::LeftBrace, "expected '{' after record type name") {
            self.synchronize_top_level();
            return None;
        }
        let mut fields = Vec::new();
        if !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            loop {
                if self.check(TokenKind::RightBrace) {
                    break;
                }
                let Some(field_name) = self.expect_identifier("expected a field name") else {
                    break;
                };
                if !self.consume(TokenKind::Colon, "expected ':' after field name") {
                    break;
                }
                let conversion = if self.check(TokenKind::KwAuto) {
                    self.bump();
                    ConversionMode::Auto
                } else {
                    ConversionMode::Exact
                };
                let Some(ty) = self.parse_type() else {
                    break;
                };
                let span = Span {
                    start: field_name.span.start,
                    end: ty.span.end,
                };
                fields.push(FieldDecl {
                    name: field_name,
                    ty,
                    conversion,
                    span,
                });
                if self.check(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        let end = if self.check(TokenKind::RightBrace) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected '}' after record fields",
            );
            self.current().span.end
        };
        Some(TypeDecl {
            name,
            fields,
            span: Span { start, end },
        })
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
        let type_params = self.parse_type_parameters();
        if !self.consume(TokenKind::LeftParen, "expected '(' after function name") {
            self.synchronize_top_level();
            return None;
        }
        let parameters = self.parse_parameter_list();
        let _ = self.consume(TokenKind::RightParen, "expected ')' after parameters");
        let body = self.parse_body();
        let span = Span {
            start: return_type.span.start,
            end: body.span.end,
        };
        Some(FunctionDecl {
            return_type,
            name,
            type_params,
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
        let mut end = name.span.end;
        let callable = if self.check(TokenKind::LeftParen) {
            self.bump();
            let nested = self.parse_parameter_list();
            if self.check(TokenKind::RightParen) {
                end = self.bump().span.end;
            } else {
                self.error(
                    ParserDiagnosticKind::ExpectedToken,
                    self.current().span,
                    "expected ')' after callable parameters",
                );
            }
            Some(nested)
        } else {
            None
        };
        let default = if self.check(TokenKind::Equal) {
            if callable.is_some() {
                self.error(
                    ParserDiagnosticKind::InvalidDeclaration,
                    self.current().span,
                    "callable parameters cannot have default values",
                );
            }
            self.bump();
            let value = self.parse_expression();
            end = value.span().end;
            Some(value)
        } else {
            None
        };
        Some(Parameter {
            name,
            ty,
            conversion,
            callable,
            default,
            span: Span { start, end },
        })
    }

    fn parse_body(&mut self) -> BlockStmt {
        if self.check(TokenKind::LeftBrace) {
            return self.parse_block();
        }
        let fallback = self.current().span;
        let statement = self.parse_statement();
        let span = statement.as_ref().map(statement_span).unwrap_or(fallback);
        BlockStmt {
            statements: statement.into_iter().collect(),
            span,
        }
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
        if self.check(TokenKind::KwType) {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "a type declaration must be at the top level",
            );
            let _ = self.parse_type_decl();
            return None;
        }
        if self.check(TokenKind::LeftBrace) {
            return Some(Stmt::Block(self.parse_block()));
        }
        if self.check(TokenKind::KwSuper) && self.peek_kind() != TokenKind::Dot {
            return Some(self.parse_super());
        }
        if self.check(TokenKind::KwReturn) {
            return Some(self.parse_return());
        }
        if self.check(TokenKind::KwEmit) {
            return Some(self.parse_emit());
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
        if self.check(TokenKind::KwTry) {
            return Some(self.parse_try());
        }
        if self.check(TokenKind::KwThrow) {
            self.parse_throw();
            return None;
        }
        if self.check(TokenKind::KwCatch) {
            self.error(
                ParserDiagnosticKind::InvalidStatement,
                self.current().span,
                "catch must follow a try",
            );
            let _ = self.parse_catch();
            return None;
        }
        if self.check(TokenKind::KwFinally) {
            self.error(
                ParserDiagnosticKind::InvalidStatement,
                self.current().span,
                "finally must follow a try",
            );
            let _ = self.parse_finally();
            return None;
        }
        if self.starts_declaration() {
            return self.parse_declaration_statement();
        }
        if self.can_start_expression() {
            return Some(self.parse_expression_statement());
        }
        let expression = self.parse_expression();
        Some(self.finish_expression_statement(expression))
    }

    fn parse_expression_statement(&mut self) -> Stmt {
        let leading = self.parse_or();
        if self.check(TokenKind::Question) && !self.colon_matches_question(self.index + 1) {
            let start = leading.span().start;
            self.bump();
            let action = self.parse_statement().unwrap_or_else(|| {
                let span = self.current().span;
                Stmt::Block(BlockStmt {
                    statements: Vec::new(),
                    span,
                })
            });
            let end = statement_span(&action).end;
            return Stmt::Conditional(ConditionalStmt {
                condition: leading,
                action: Box::new(action),
                span: Span { start, end },
            });
        }
        let expression = self.continue_after_or(leading);
        self.finish_expression_statement(expression)
    }

    fn finish_statement_end(&mut self, fallback: SourcePosition, message: &str) -> SourcePosition {
        if self.soft_terminator > 0
            && matches!(
                self.current().kind,
                TokenKind::Comma | TokenKind::RightParen | TokenKind::Semicolon
            )
        {
            return fallback;
        }
        if self.consume(TokenKind::Semicolon, message) {
            self.previous_end().unwrap_or(fallback)
        } else {
            fallback
        }
    }

    fn finish_expression_statement(&mut self, expression: Expr) -> Stmt {
        let start = expression.span().start;
        let end = self.finish_statement_end(expression.span().end, "expected ';' after expression");
        Stmt::Expression(ExpressionStmt {
            expression,
            span: Span { start, end },
        })
    }

    fn colon_matches_question(&self, start: usize) -> bool {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut nested = 0i32;
        let mut index = start;
        while index < self.tokens.len() {
            let kind = self.tokens[index].kind;
            if kind == TokenKind::Eof {
                return false;
            }
            let depth = paren + bracket + brace;
            match kind {
                TokenKind::LeftParen => paren += 1,
                TokenKind::RightParen => {
                    if paren == 0 {
                        return false;
                    }
                    paren -= 1;
                }
                TokenKind::LeftBracket => bracket += 1,
                TokenKind::RightBracket => {
                    if bracket == 0 {
                        return false;
                    }
                    bracket -= 1;
                }
                TokenKind::LeftBrace => brace += 1,
                TokenKind::RightBrace => {
                    if brace == 0 {
                        return false;
                    }
                    brace -= 1;
                }
                TokenKind::Question if depth == 0 && !self.question_is_type_suffix(index) => {
                    nested += 1;
                }
                TokenKind::Colon if depth == 0 => {
                    if nested == 0 {
                        return true;
                    }
                    nested -= 1;
                }
                TokenKind::Semicolon if depth == 0 => return false,
                _ => {}
            }
            index += 1;
        }
        false
    }

    fn question_is_type_suffix(&self, index: usize) -> bool {
        index > 0 && type_kind(self.tokens[index - 1].kind).is_some()
    }

    fn parse_try(&mut self) -> Stmt {
        let start = self.bump().span.start;
        let body = self.parse_required_block("expected '{' to open a try block");
        let mut catches = Vec::new();
        let mut finally_block = None;
        loop {
            if self.check(TokenKind::KwCatch) {
                if finally_block.is_some() {
                    self.error(
                        ParserDiagnosticKind::InvalidStatement,
                        self.current().span,
                        "catch must come before finally",
                    );
                }
                if let Some(clause) = self.parse_catch() {
                    catches.push(clause);
                }
            } else if self.check(TokenKind::KwFinally) {
                if finally_block.is_some() {
                    self.error(
                        ParserDiagnosticKind::InvalidStatement,
                        self.current().span,
                        "a try statement can have only one finally",
                    );
                    let _ = self.parse_finally();
                } else {
                    finally_block = Some(self.parse_finally());
                }
            } else {
                break;
            }
        }
        let end = finally_block
            .as_ref()
            .map(|block| block.span.end)
            .or_else(|| catches.last().map(|clause| clause.span.end))
            .unwrap_or(body.span.end);
        Stmt::Try(TryStmt {
            body,
            catches,
            finally_block,
            span: Span { start, end },
        })
    }

    fn parse_catch(&mut self) -> Option<CatchClause> {
        let start = self.bump().span.start;
        self.consume(TokenKind::LeftParen, "expected '(' after catch");
        let error_type = self.expect_identifier("expected a catch type")?;
        let binding = self.expect_identifier("expected a catch binding")?;
        self.consume(
            TokenKind::RightParen,
            "expected ')' after the catch binding",
        );
        let block = self.parse_required_block("expected '{' to open a catch block");
        let end = block.span.end;
        Some(CatchClause {
            error_type,
            binding,
            block,
            span: Span { start, end },
        })
    }

    fn parse_finally(&mut self) -> BlockStmt {
        self.bump();
        self.parse_required_block("expected '{' to open a finally block")
    }

    fn parse_throw(&mut self) {
        let token = self.bump();
        self.error(
            ParserDiagnosticKind::InvalidStatement,
            token.span,
            "explicit throw is not supported in this version",
        );
        if !self.check(TokenKind::Semicolon)
            && !self.check(TokenKind::RightBrace)
            && !self.is_at_end()
        {
            let _ = self.parse_expression();
        }
        let _ = self.consume(TokenKind::Semicolon, "expected ';' after throw");
    }

    fn parse_required_block(&mut self, message: &str) -> BlockStmt {
        if self.check(TokenKind::LeftBrace) {
            self.parse_block()
        } else {
            self.error(
                ParserDiagnosticKind::InvalidStatement,
                self.current().span,
                message,
            );
            BlockStmt {
                statements: Vec::new(),
                span: self.current().span,
            }
        }
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
        let initializer = if self.check(TokenKind::Equal) {
            self.bump();
            Some(self.parse_expression())
        } else if head.mutable {
            None
        } else {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                head.name.span,
                "a constant requires a value",
            );
            if require_semicolon {
                self.synchronize_statement();
            }
            return None;
        };
        let mut end = initializer
            .as_ref()
            .map(|initializer| initializer.span().end)
            .unwrap_or(head.span.end);
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
            bindings.push(binding);
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

    fn parse_super(&mut self) -> Stmt {
        let start = self.bump().span.start;
        let mut arguments = Vec::new();
        if self.consume(TokenKind::LeftParen, "expected '(' after super")
            && !self.check(TokenKind::RightParen)
            && !self.is_at_end()
        {
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
            self.current().span.end
        };
        let end = self.finish_statement_end(end, "expected ';' after super");
        Stmt::Super(SuperStmt {
            arguments,
            span: Span { start, end },
        })
    }

    fn parse_emit(&mut self) -> Stmt {
        let start = self.bump().span.start;
        let value = self.parse_expression();
        let end = self.finish_statement_end(value.span().end, "expected ';' after emit");
        Stmt::Emit(EmitStmt {
            value,
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
        let fallback = value
            .as_ref()
            .map(Expr::span)
            .map(|span| span.end)
            .unwrap_or(self.current().span.end);
        let end = self.finish_statement_end(fallback, "expected ';' after return");
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
        let then_branch = self.parse_body();
        let else_branch = if self.check(TokenKind::KwElse) {
            self.bump();
            if self.check(TokenKind::KwIf) {
                Some(Box::new(self.parse_if()))
            } else {
                Some(Box::new(Stmt::Block(self.parse_body())))
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
                } else if self.check(TokenKind::Colon) {
                    self.bump();
                    let iterable = self.parse_expression();
                    LoopKind::Iteration {
                        bindings: vec![head],
                        iterable,
                    }
                } else if self.check(TokenKind::Equal) {
                    self.bump();
                    let initializer = self.parse_expression();
                    if self.check(TokenKind::Semicolon) {
                        self.bump();
                        let declaration = VariableDecl {
                            name: head.name,
                            ty: head.ty,
                            conversion: head.conversion,
                            initializer: Some(initializer),
                            mutable: head.mutable,
                            span: head.span,
                        };
                        self.parse_cstyle_tail(Some(declaration))
                    } else {
                        self.error(
                            ParserDiagnosticKind::ExpectedToken,
                            self.current().span,
                            "use ':' for iteration loops: loop (Type name : iterable)",
                        );
                        LoopKind::Iteration {
                            bindings: vec![head],
                            iterable: initializer,
                        }
                    }
                } else if self.check(TokenKind::Semicolon) && head.mutable {
                    self.bump();
                    let declaration = VariableDecl {
                        name: head.name,
                        ty: head.ty,
                        conversion: head.conversion,
                        initializer: None,
                        mutable: head.mutable,
                        span: head.span,
                    };
                    self.parse_cstyle_tail(Some(declaration))
                } else if !head.mutable {
                    self.error(
                        ParserDiagnosticKind::InvalidDeclaration,
                        head.name.span,
                        "a constant requires a value",
                    );
                    self.parse_cstyle_tail(None)
                } else {
                    self.error(
                        ParserDiagnosticKind::ExpectedToken,
                        self.current().span,
                        "expected ':' for iteration or '=' for a C-style loop",
                    );
                    self.parse_cstyle_tail(None)
                }
            } else {
                self.parse_cstyle_tail(None)
            }
        } else {
            let condition = self.parse_expression();
            if self.check(TokenKind::Semicolon) {
                self.bump();
                let mut kind = self.parse_cstyle_tail(None);
                if let LoopKind::CStyle { init_expr, .. } = &mut kind {
                    *init_expr = Some(condition);
                }
                kind
            } else {
                LoopKind::Condition { condition }
            }
        };
        let _ = self.consume(TokenKind::RightParen, "expected ')' after loop header");
        let body = self.parse_body();
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
            init_expr: None,
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
        let iterable = if self.check(TokenKind::Colon) {
            self.bump();
            self.parse_expression()
        } else if self.check(TokenKind::Equal) {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "use ':' for iteration loops: loop (Type name : iterable)",
            );
            self.bump();
            self.parse_expression()
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected ':' in iteration loop binding",
            );
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
        let left = self.parse_conditional();
        let Some(op) = self.assignment_operator() else {
            return left;
        };
        self.bump();
        let value = self.parse_assignment();
        let span = covering(left.span(), value.span());
        Expr::Assignment(AssignmentExpr {
            target: Box::new(left),
            value: Box::new(value),
            op,
            span,
        })
    }

    fn parse_conditional(&mut self) -> Expr {
        let condition = self.parse_or();
        if self.check(TokenKind::Question) {
            self.finish_value_ternary(condition)
        } else {
            condition
        }
    }

    fn continue_after_or(&mut self, leading: Expr) -> Expr {
        let expr = if self.check(TokenKind::Question) {
            self.finish_value_ternary(leading)
        } else {
            leading
        };
        let Some(op) = self.assignment_operator() else {
            return expr;
        };
        self.bump();
        let value = self.parse_assignment();
        let span = covering(expr.span(), value.span());
        Expr::Assignment(AssignmentExpr {
            target: Box::new(expr),
            value: Box::new(value),
            op,
            span,
        })
    }

    fn finish_value_ternary(&mut self, condition: Expr) -> Expr {
        let start = condition.span().start;
        self.bump();
        let then_branch = self.parse_expression();
        let else_branch = if self.check(TokenKind::Colon) {
            self.bump();
            self.parse_conditional()
        } else {
            let message = if self.check(TokenKind::Semicolon)
                || self.check(TokenKind::RightBrace)
                || self.is_at_end()
            {
                "a conditional action does not produce a value"
            } else {
                "expected ':' in a ternary expression"
            };
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                message,
            );
            then_branch.clone()
        };
        let end = else_branch.span().end;
        Expr::Ternary(TernaryExpr {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            span: Span { start, end },
        })
    }

    fn assignment_operator(&self) -> Option<Option<BinaryOperator>> {
        Some(match self.current().kind {
            TokenKind::Equal => None,
            TokenKind::PlusEqual => Some(BinaryOperator::Add),
            TokenKind::MinusEqual => Some(BinaryOperator::Subtract),
            TokenKind::StarEqual => Some(BinaryOperator::Multiply),
            TokenKind::SlashEqual => Some(BinaryOperator::Divide),
            TokenKind::PercentEqual => Some(BinaryOperator::Remainder),
            TokenKind::SlashSlashEqual => Some(BinaryOperator::FloorDiv),
            TokenKind::StarStarEqual => Some(BinaryOperator::Power),
            TokenKind::AmpEqual => Some(BinaryOperator::BitAnd),
            TokenKind::PipeEqual => Some(BinaryOperator::BitOr),
            TokenKind::CaretEqual => Some(BinaryOperator::BitXor),
            TokenKind::LessLessEqual => Some(BinaryOperator::Shl),
            TokenKind::GreaterGreaterEqual => Some(BinaryOperator::Shr),
            _ => return None,
        })
    }

    fn parse_or(&mut self) -> Expr {
        self.parse_binary(
            &[TokenKind::OrOr, TokenKind::KwOr],
            Self::parse_and,
            BinaryOperator::Or,
        )
    }

    fn parse_and(&mut self) -> Expr {
        self.parse_binary(
            &[TokenKind::AndAnd, TokenKind::KwAnd],
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
        let mut left = self.parse_bitwise_or();
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
            let right = self.parse_bitwise_or();
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_bitwise_or(&mut self) -> Expr {
        self.parse_binary(
            &[TokenKind::Pipe],
            Self::parse_bitwise_xor,
            BinaryOperator::BitOr,
        )
    }

    fn parse_bitwise_xor(&mut self) -> Expr {
        self.parse_binary(
            &[TokenKind::Caret],
            Self::parse_bitwise_and,
            BinaryOperator::BitXor,
        )
    }

    fn parse_bitwise_and(&mut self) -> Expr {
        self.parse_binary(&[TokenKind::Amp], Self::parse_shift, BinaryOperator::BitAnd)
    }

    fn parse_shift(&mut self) -> Expr {
        let mut left = self.parse_additive();
        while self.check(TokenKind::LessLess)
            || (self.check(TokenKind::Greater) && self.peek_kind() == TokenKind::Greater)
        {
            let operator = if self.check(TokenKind::LessLess) {
                BinaryOperator::Shl
            } else {
                BinaryOperator::Shr
            };
            self.bump();
            if operator == BinaryOperator::Shr {
                self.bump();
            }
            let right = self.parse_additive();
            left = binary(left, operator, right);
        }
        left
    }

    fn parse_additive(&mut self) -> Expr {
        let mut left = self.parse_multiplicative();
        while matches!(self.current().kind, TokenKind::Plus | TokenKind::Minus) {
            if self.reject_arrow() {
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
        if self.check(TokenKind::DotDotDot) {
            self.error(
                ParserDiagnosticKind::InvalidExpression,
                self.current().span,
                "spread is only allowed in a map literal",
            );
            self.bump();
            return self.parse_unary();
        }
        if self.check(TokenKind::Bang)
            || self.check(TokenKind::Minus)
            || self.check(TokenKind::Tilde)
        {
            let operator = if self.check(TokenKind::Bang) {
                UnaryOperator::Not
            } else if self.check(TokenKind::Tilde) {
                UnaryOperator::BitNot
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
        self.parse_power()
    }

    fn parse_power(&mut self) -> Expr {
        let left = self.parse_postfix();
        if !self.check(TokenKind::StarStar) {
            return left;
        }
        self.bump();
        let right = self.parse_unary();
        binary(left, BinaryOperator::Power, right)
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expression = self.parse_primary();
        loop {
            if self.check(TokenKind::LeftParen) {
                expression = self.finish_call(expression, Vec::new());
            } else if self.check(TokenKind::Less) {
                if let Some(type_arguments) = self.try_type_arguments() {
                    expression = self.finish_call(expression, type_arguments);
                } else {
                    break;
                }
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

    fn finish_call(&mut self, callee: Expr, type_arguments: Vec<TypeSyntax>) -> Expr {
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
            type_arguments,
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

    fn parse_record_literal(&mut self) -> Expr {
        let start = self.bump().span.start;
        let mut entries = Vec::new();
        if !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            loop {
                if self.check(TokenKind::RightBrace) {
                    break;
                }
                if self.check(TokenKind::DotDotDot) {
                    let spread = self.bump();
                    let value = self.parse_expression();
                    let span = Span {
                        start: spread.span.start,
                        end: value.span().end,
                    };
                    entries.push(BraceEntry::Spread { value, span });
                } else {
                    let key = self.parse_expression();
                    if !self.consume(TokenKind::Colon, "expected ':' in a brace literal") {
                        break;
                    }
                    let value = self.parse_expression();
                    let span = Span {
                        start: key.span().start,
                        end: value.span().end,
                    };
                    entries.push(BraceEntry::Pair { key, value, span });
                }
                if self.check(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        let end = if self.check(TokenKind::RightBrace) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected '}' after brace literal",
            );
            self.current().span.end
        };
        Expr::Record(RecordExpr {
            entries,
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

    fn starts_lambda(&self) -> bool {
        if self.peek_kind() != TokenKind::LeftParen {
            return false;
        }
        let keyword =
            type_kind(self.current().kind).is_some() && self.current().kind != TokenKind::KwMap;
        if !keyword && self.current().kind != TokenKind::Identifier {
            return false;
        }
        self.lambda_parameters_at(self.index + 2, keyword)
    }

    fn lambda_parameters_at(&self, index: usize, allow_empty: bool) -> bool {
        if self.kind_at(index) == TokenKind::RightParen {
            return allow_empty && self.can_start_lambda_body(self.kind_at(index + 1));
        }
        self.lambda_parameter_at(index)
    }

    fn lambda_parameter_at(&self, mut index: usize) -> bool {
        if self.kind_at(index) == TokenKind::KwAuto {
            index += 1;
        }
        let kind = self.kind_at(index);
        if kind == TokenKind::KwMap || type_kind(kind).is_none() && kind != TokenKind::Identifier {
            return false;
        }
        index += 1;
        let Some(index) = self.skip_type_suffix(index) else {
            return false;
        };
        self.kind_at(index) == TokenKind::Identifier
    }

    /// Skip `?` and array bounds that are part of a type (`[]` or `[4]`).
    /// `arr[i]` is an index expression, so it is not a lambda parameter.
    fn skip_type_suffix(&self, mut index: usize) -> Option<usize> {
        if self.kind_at(index) == TokenKind::Question {
            index += 1;
        }
        while self.kind_at(index) == TokenKind::LeftBracket {
            let inside = self.kind_at(index + 1);
            if inside == TokenKind::RightBracket {
                index += 2;
                continue;
            }
            if inside == TokenKind::IntegerLiteral
                && self.kind_at(index + 2) == TokenKind::RightBracket
            {
                index += 3;
                continue;
            }
            return None;
        }
        Some(index)
    }

    fn can_start_lambda_body(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::LeftBrace
                | TokenKind::KwReturn
                | TokenKind::KwIf
                | TokenKind::KwLoop
                | TokenKind::KwConst
                | TokenKind::KwAuto
                | TokenKind::KwTry
                | TokenKind::Identifier
                | TokenKind::KwThis
                | TokenKind::KwNew
                | TokenKind::KwSuper
                | TokenKind::KwTrue
                | TokenKind::KwFalse
                | TokenKind::KwNull
                | TokenKind::IntegerLiteral
                | TokenKind::FloatLiteral
                | TokenKind::StringLiteral
                | TokenKind::InterpolatedStringStart
                | TokenKind::Bang
                | TokenKind::Tilde
                | TokenKind::Minus
                | TokenKind::PlusPlus
                | TokenKind::MinusMinus
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
        ) || (type_kind(kind).is_some() && kind != TokenKind::KwMap)
    }

    fn parse_lambda(&mut self) -> Expr {
        let start = self.current().span.start;
        let Some(ret) = self.parse_type() else {
            return Expr::Identifier(Identifier {
                name: String::new(),
                span: self.current().span,
            });
        };
        let _ = self.consume(
            TokenKind::LeftParen,
            "expected '(' after lambda return type",
        );
        let params = self.parse_parameter_list();
        if self.check(TokenKind::RightParen) {
            self.bump();
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected ')' after lambda parameters",
            );
        }
        let (body, end) = if self.check(TokenKind::LeftBrace) {
            let block = self.parse_block();
            let end = block.span.end;
            (block, end)
        } else {
            self.soft_terminator += 1;
            let statement = self.parse_statement();
            self.soft_terminator -= 1;
            let span = statement
                .as_ref()
                .map(statement_span)
                .unwrap_or(self.current().span);
            let end = span.end;
            (
                BlockStmt {
                    statements: statement.into_iter().collect(),
                    span,
                },
                end,
            )
        };
        Expr::Lambda(LambdaExpr {
            ret,
            params,
            body,
            span: Span { start, end },
        })
    }

    fn parse_primary(&mut self) -> Expr {
        match self.current().kind {
            TokenKind::KwNew => self.parse_new(),
            TokenKind::KwSuper => {
                let token = self.bump();
                Expr::Super { span: token.span }
            }
            TokenKind::KwThis => {
                let token = self.bump();
                Expr::Identifier(Identifier {
                    name: "this".to_string(),
                    span: token.span,
                })
            }
            TokenKind::Identifier if self.starts_lambda() => self.parse_lambda(),
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
            TokenKind::KwNull => {
                let token = self.bump();
                Expr::Literal(LiteralExpr::Null { span: token.span })
            }
            TokenKind::InterpolatedStringStart => self.parse_interpolated_string(),
            _ if self.starts_lambda() => self.parse_lambda(),
            kind if self.starts_conversion_at(kind) => self.parse_explicit_conversion(),
            TokenKind::LeftBracket => self.parse_array_literal(),
            TokenKind::LeftBrace => self.parse_record_literal(),
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

    fn parse_type_parameters(&mut self) -> Vec<Identifier> {
        if !self.check(TokenKind::Less) {
            return Vec::new();
        }
        self.bump();
        let mut params = Vec::new();
        if !self.check(TokenKind::Greater) {
            loop {
                let Some(name) = self.expect_identifier("expected a type parameter") else {
                    break;
                };
                params.push(name);
                if !self.check(TokenKind::Comma) {
                    break;
                }
                self.bump();
                if self.check(TokenKind::Greater) {
                    self.error(
                        ParserDiagnosticKind::InvalidTypeSyntax,
                        self.current().span,
                        "a trailing comma is not allowed in a type parameter list",
                    );
                    break;
                }
            }
        }
        let _ = self.consume(TokenKind::Greater, "expected '>' after type parameters");
        params
    }

    fn reject_generic_inheritance(&mut self) {
        if self.check(TokenKind::Less) {
            self.error(
                ParserDiagnosticKind::InvalidDeclaration,
                self.current().span,
                "generic inheritance is not supported",
            );
            let _ = self.parse_required_type_arguments();
        }
    }

    fn parse_required_type_arguments(&mut self) -> (Vec<TypeSyntax>, crate::span::SourcePosition) {
        let fallback = self.current().span.end;
        if !self.consume(TokenKind::Less, "expected '<'") {
            return (Vec::new(), fallback);
        }
        let mut arguments = Vec::new();
        if !self.check(TokenKind::Greater) {
            loop {
                if let Some(ty) = self.parse_type() {
                    arguments.push(ty);
                } else {
                    break;
                }
                if !self.check(TokenKind::Comma) {
                    break;
                }
                self.bump();
                if self.check(TokenKind::Greater) {
                    self.error(
                        ParserDiagnosticKind::InvalidTypeSyntax,
                        self.current().span,
                        "a trailing comma is not allowed in a type argument list",
                    );
                    break;
                }
            }
        }
        let end = if self.check(TokenKind::Greater) {
            self.bump().span.end
        } else {
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                self.current().span,
                "expected '>' after type arguments",
            );
            self.current().span.end
        };
        (arguments, end)
    }

    /// `<int, str>` followed by `(`. Anything else is left for comparison.
    fn try_type_arguments(&mut self) -> Option<Vec<TypeSyntax>> {
        if !self.check(TokenKind::Less) {
            return None;
        }
        let index = self.index;
        let errors = self.diagnostics.len();
        self.bump();
        let mut arguments = Vec::new();
        if !self.check(TokenKind::Greater) {
            loop {
                let Some(ty) = self.parse_type() else {
                    self.index = index;
                    self.diagnostics.truncate(errors);
                    return None;
                };
                arguments.push(ty);
                if !self.check(TokenKind::Comma) {
                    break;
                }
                self.bump();
                if self.check(TokenKind::Greater) {
                    self.index = index;
                    self.diagnostics.truncate(errors);
                    return None;
                }
            }
        }
        let closed = self.check(TokenKind::Greater)
            && self
                .tokens
                .get(self.index + 1)
                .is_some_and(|token| token.kind == TokenKind::LeftParen);
        if !closed {
            self.index = index;
            self.diagnostics.truncate(errors);
            return None;
        }
        self.bump();
        Some(arguments)
    }

    fn parse_type(&mut self) -> Option<TypeSyntax> {
        let (kind, name, token) = if let Some(kind) = type_kind(self.current().kind) {
            let token = self.bump();
            (kind, String::new(), token)
        } else if self.check(TokenKind::Identifier) {
            let token = self.bump();
            (TypeKind::Named, token.lexeme.clone(), token)
        } else {
            self.error(
                ParserDiagnosticKind::InvalidTypeSyntax,
                self.current().span,
                "expected a type",
            );
            return None;
        };
        let mut arguments = Vec::new();
        let mut bounds = Vec::new();
        let mut end = token.span.end;
        if kind == TypeKind::Named && self.check(TokenKind::Less) {
            let (parsed, parsed_end) = self.parse_required_type_arguments();
            arguments = parsed;
            end = parsed_end;
        }
        if kind == TypeKind::Map {
            if self.consume(TokenKind::Less, "expected '<' after map") {
                if let Some(key) = self.parse_type() {
                    arguments.push(key);
                }
                let _ = self.consume(
                    TokenKind::Comma,
                    "map type requires a key type and a value type",
                );
                if let Some(value) = self.parse_type() {
                    arguments.push(value);
                }
                if self.check(TokenKind::Greater) {
                    end = self.bump().span.end;
                } else {
                    self.error(
                        ParserDiagnosticKind::ExpectedToken,
                        self.current().span,
                        "expected '>' after map type arguments",
                    );
                }
            }
        }
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
        let mut optional = false;
        if self.check(TokenKind::Question) {
            end = self.bump().span.end;
            optional = true;
        }
        Some(TypeSyntax {
            kind,
            name,
            bounds,
            arguments,
            optional,
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
        if self.current().kind.is_keyword() {
            let token = self.bump();
            self.error(
                ParserDiagnosticKind::ExpectedToken,
                token.span,
                format!("'{}' is a reserved keyword", token.lexeme),
            );
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

    fn reject_arrow(&mut self) -> bool {
        if self.current().kind == TokenKind::Minus && self.peek_kind() == TokenKind::Greater {
            self.reject_sequence("->");
            return true;
        }
        false
    }

    fn parse_new(&mut self) -> Expr {
        let start = self.bump().span.start;
        let Some(name) = self.expect_identifier("expected a class name after 'new'") else {
            return Expr::Literal(LiteralExpr::Integer {
                lexeme: "0".to_string(),
                span: self.current().span,
            });
        };
        let type_arguments = if self.check(TokenKind::Less) {
            self.parse_required_type_arguments().0
        } else {
            Vec::new()
        };
        if !self.consume(TokenKind::LeftParen, "expected '(' after the class name") {
            return Expr::New(NewExpr {
                span: Span {
                    start,
                    end: name.span.end,
                },
                name,
                type_arguments,
                arguments: Vec::new(),
            });
        }
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
        Expr::New(NewExpr {
            name,
            type_arguments,
            arguments,
            span: Span { start, end },
        })
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
        type_kind(self.current().kind).is_some() || self.check(TokenKind::Identifier)
    }

    fn starts_function(&self) -> bool {
        if type_kind(self.current().kind).is_some() {
            // `map` is a type keyword that always begins a typed declaration;
            // incomplete forms such as `map scores;` are diagnosed in type
            // parsing rather than rejected as unknown top-level tokens.
            if self.current().kind == TokenKind::KwMap {
                return true;
            }
            let mut index = self.index + 1;
            index = self.skip_type_suffix_at(index);
            if self.kind_at(index) != TokenKind::Identifier {
                return true;
            }
            index += 1;
            index = self.skip_type_arg_list_at(index);
            return self.kind_at(index) == TokenKind::LeftParen
                || self.kind_at(index) == TokenKind::Less
                || self.kind_at(index) == TokenKind::LeftBrace;
        }
        self.named_type_end()
            .is_some_and(|index| self.kind_at(index) == TokenKind::LeftParen)
    }

    fn starts_declaration(&self) -> bool {
        if self.check(TokenKind::KwConst) || self.check(TokenKind::KwAuto) {
            return true;
        }
        if type_kind(self.current().kind).is_some() && self.peek_kind() != TokenKind::LeftParen {
            return true;
        }
        self.looks_like_named_binding()
    }

    fn looks_like_named_binding(&self) -> bool {
        self.named_type_end().is_some_and(|index| {
            matches!(
                self.kind_at(index),
                TokenKind::Equal | TokenKind::Semicolon | TokenKind::Comma | TokenKind::RightParen
            )
        })
    }

    fn named_type_end(&self) -> Option<usize> {
        if self.current().kind != TokenKind::Identifier {
            return None;
        }
        let mut index = self.index + 1;
        index = self.skip_type_arg_list_at(index);
        index = self.skip_type_suffix_at(index);
        if self.kind_at(index) != TokenKind::Identifier {
            return None;
        }
        index += 1;
        index = self.skip_type_arg_list_at(index);
        Some(index)
    }

    /// Skip `<...>` type arguments. Nested `<` / `>` pairs are balanced.
    fn skip_type_arg_list_at(&self, mut index: usize) -> usize {
        if self.kind_at(index) != TokenKind::Less {
            return index;
        }
        index += 1;
        let mut depth = 1usize;
        while depth > 0 {
            match self.kind_at(index) {
                TokenKind::Eof => return index,
                TokenKind::Less => depth += 1,
                TokenKind::Greater => depth -= 1,
                _ => {}
            }
            index += 1;
        }
        index
    }

    fn skip_type_suffix_at(&self, mut index: usize) -> usize {
        loop {
            if self.kind_at(index) != TokenKind::LeftBracket {
                break;
            }
            index += 1;
            if self.kind_at(index) == TokenKind::IntegerLiteral {
                index += 1;
            }
            if self.kind_at(index) != TokenKind::RightBracket {
                return index;
            }
            index += 1;
        }
        if self.kind_at(index) == TokenKind::Question {
            index += 1;
        }
        index
    }

    fn kind_at(&self, index: usize) -> TokenKind {
        self.tokens
            .get(index)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn starts_conversion_at(&self, kind: TokenKind) -> bool {
        if type_kind(kind).is_none() || kind == TokenKind::KwMap {
            return false;
        }
        self.peek_kind() == TokenKind::LeftParen
            || (self.peek_kind() == TokenKind::Question
                && self.kind_at(self.index + 2) == TokenKind::LeftParen)
    }

    fn can_start_expression(&self) -> bool {
        self.starts_conversion_at(self.current().kind)
            || matches!(
                self.current().kind,
                TokenKind::Identifier
                    | TokenKind::KwThis
                    | TokenKind::KwNew
                    | TokenKind::KwSuper
                    | TokenKind::IntegerLiteral
                    | TokenKind::FloatLiteral
                    | TokenKind::StringLiteral
                    | TokenKind::KwTrue
                    | TokenKind::KwFalse
                    | TokenKind::KwNull
                    | TokenKind::InterpolatedStringStart
                    | TokenKind::LeftParen
                    | TokenKind::LeftBracket
                    | TokenKind::LeftBrace
                    | TokenKind::Bang
                    | TokenKind::Tilde
                    | TokenKind::Minus
                    | TokenKind::Plus
                    | TokenKind::PlusPlus
                    | TokenKind::MinusMinus
            )
    }

    fn parse_explicit_conversion(&mut self) -> Expr {
        let token = self.bump();
        let mut end = token.span.end;
        let optional = if self.check(TokenKind::Question) {
            end = self.bump().span.end;
            true
        } else {
            false
        };
        let target = TypeSyntax {
            kind: type_kind(token.kind).expect("conversion target was checked"),
            name: String::new(),
            bounds: Vec::new(),
            arguments: Vec::new(),
            optional,
            span: Span {
                start: token.span.start,
                end,
            },
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
        while !self.is_at_end()
            && !self.starts_type()
            && !self.check(TokenKind::KwImport)
            && !self.check(TokenKind::KwType)
            && !self.check(TokenKind::KwClass)
        {
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
        TokenKind::KwMap => TypeKind::Map,
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
        Stmt::Emit(emit_statement) => emit_statement.span,
        Stmt::If(if_statement) => if_statement.span,
        Stmt::Loop(loop_statement) => loop_statement.span,
        Stmt::Break(span) | Stmt::Continue(span) => *span,
        Stmt::Try(try_statement) => try_statement.span,
        Stmt::Expression(expression) => expression.span,
        Stmt::Conditional(conditional) => conditional.span,
        Stmt::Super(statement) => statement.span,
    }
}

#[derive(Default)]
struct DeclMods {
    abstract_class: bool,
    final_class: bool,
    is_static: bool,
    is_override: bool,
}

#[derive(Default)]
struct MemberMods {
    public: bool,
    protected: bool,
    saw_visibility: bool,
    is_static: bool,
    is_override: bool,
    is_abstract: bool,
    is_final: bool,
    deprecated: bool,
}
