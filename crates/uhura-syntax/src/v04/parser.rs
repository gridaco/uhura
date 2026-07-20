//! Recursive-descent parser for the Uhura 0.4 core grammar.

use std::mem::discriminant;

use super::ast::*;
use super::lexer::{Keyword, Token, TokenKind, Trivia, TriviaKind, lex_fragment};
use super::ui::parse_ui_body;
use super::{ParseDiagnostic, ParseDiagnosticKind};

#[derive(Clone, Copy)]
enum NameCategory {
    Lower,
    Upper,
    Constant,
    Declaration,
}

pub(super) struct Parser<'a> {
    identity: SourceIdentity,
    tokens: &'a [Token],
    cursor: usize,
    diagnostics: Vec<ParseDiagnostic>,
}

impl<'a> Parser<'a> {
    pub(super) fn new(identity: SourceIdentity, tokens: &'a [Token]) -> Self {
        Self {
            identity,
            tokens,
            cursor: 0,
            diagnostics: Vec::new(),
        }
    }

    pub(super) fn parse_module(
        mut self,
    ) -> (Vec<UseDeclaration>, Vec<Declaration>, Vec<ParseDiagnostic>) {
        let mut uses = Vec::new();
        while self.at_keyword(Keyword::Use)
            || (self.at_keyword(Keyword::Pub) && self.nth_keyword(1, Keyword::Use))
        {
            uses.push(self.parse_use());
        }

        let mut declarations = Vec::new();
        while !self.at_simple(TokenKind::Eof) {
            let start = self.cursor;
            if let Some(declaration) = self.parse_declaration() {
                declarations.push(declaration);
            } else {
                self.error_here(
                    ParseDiagnosticKind::InvalidDeclaration,
                    "expected `machine`, `part`, `ui`, `struct`, `enum`, `key`, `const`, or `fn`",
                );
                self.synchronize_module();
            }
            if self.cursor == start {
                self.bump();
            }
        }

        (uses, declarations, self.diagnostics)
    }

    fn parse_use(&mut self) -> UseDeclaration {
        let start = self.current().span.start;
        let visibility = if self.eat_keyword(Keyword::Pub).is_some() {
            Visibility::Public
        } else {
            Visibility::Private
        };
        self.expect_keyword(Keyword::Use, "use declaration");

        let root = self.parse_import_root();
        let mut segments = Vec::new();
        self.expect_simple(TokenKind::ColonColon, "import path");
        let first = self.expect_name(NameCategory::Declaration, "imported name");
        segments.push(first);
        while self.at_simple(TokenKind::ColonColon) && !self.nth_is_simple(1, TokenKind::LBrace) {
            self.bump();
            segments.push(self.expect_name(NameCategory::Declaration, "imported path segment"));
        }

        let tree = if self.eat_simple(TokenKind::ColonColon).is_some() {
            let prefix_end = segments
                .last()
                .map_or(root.span().end, |name| name.span.end);
            let prefix = ImportPrefix {
                root,
                segments,
                span: Span::new(self.identity.file, start, prefix_end),
            };
            self.expect_simple(TokenKind::LBrace, "grouped import");
            let mut items = Vec::new();
            if self.at_simple(TokenKind::RBrace) {
                self.error_here(
                    ParseDiagnosticKind::InvalidDeclaration,
                    "a grouped import must name at least one declaration",
                );
            }
            while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
                let item_start = self.current().span.start;
                let name = self.expect_name(NameCategory::Declaration, "imported name");
                let alias = if self.eat_keyword(Keyword::As).is_some() {
                    Some(self.expect_name(NameCategory::Declaration, "import alias"))
                } else {
                    None
                };
                let end = alias.as_ref().map_or(name.span.end, |alias| alias.span.end);
                items.push(ImportItem {
                    name,
                    alias,
                    span: Span::new(self.identity.file, item_start, end),
                });
                if self.eat_simple(TokenKind::Comma).is_none() {
                    if !self.at_simple(TokenKind::RBrace) {
                        self.error_here(
                            ParseDiagnosticKind::MissingToken,
                            "expected `,` between grouped imports",
                        );
                    }
                    break;
                }
            }
            self.expect_simple(TokenKind::RBrace, "grouped import");
            ImportTree::Group { prefix, items }
        } else {
            let end = segments
                .last()
                .map_or(root.span().end, |name| name.span.end);
            let path = ImportPath {
                root,
                segments,
                span: Span::new(self.identity.file, start, end),
            };
            let alias = if self.eat_keyword(Keyword::As).is_some() {
                if visibility == Visibility::Public {
                    self.error_here(
                        ParseDiagnosticKind::InvalidDeclaration,
                        "`pub use` is singular and cannot be aliased",
                    );
                }
                Some(self.expect_name(NameCategory::Declaration, "import alias"))
            } else {
                None
            };
            ImportTree::Single { path, alias }
        };
        let semicolon = self.expect_simple(TokenKind::Semicolon, "use declaration");
        UseDeclaration {
            visibility,
            tree,
            span: Span::new(self.identity.file, start, semicolon.end),
            semicolon,
        }
    }

    fn parse_import_root(&mut self) -> ImportRoot {
        if let Some(span) = self.eat_keyword(Keyword::Crate) {
            ImportRoot::Crate(span)
        } else {
            ImportRoot::Package(self.expect_name(NameCategory::Lower, "import root"))
        }
    }

    fn parse_declaration(&mut self) -> Option<Declaration> {
        let start = self.current().span.start;
        let visibility = if self.eat_keyword(Keyword::Pub).is_some() {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let kind = if self.at_keyword(Keyword::Machine) {
            DeclarationKind::Machine(self.parse_machine(visibility))
        } else if self.at_keyword(Keyword::Part) {
            DeclarationKind::Part(self.parse_part(visibility))
        } else if self.at_contextual("ui") {
            DeclarationKind::Ui(self.parse_ui(visibility))
        } else if self.at_keyword(Keyword::Struct) {
            DeclarationKind::Struct(self.parse_struct(visibility))
        } else if self.at_keyword(Keyword::Enum) {
            DeclarationKind::Enum(self.parse_enum(visibility))
        } else if self.at_keyword(Keyword::Key) {
            DeclarationKind::Key(self.parse_key(visibility))
        } else if self.at_keyword(Keyword::Const) {
            DeclarationKind::Const(self.parse_const(visibility))
        } else if self.at_keyword(Keyword::Fn) {
            DeclarationKind::Function(self.parse_function(visibility))
        } else {
            if visibility == Visibility::Public {
                self.error_here(
                    ParseDiagnosticKind::InvalidDeclaration,
                    "`pub` must precede a module declaration or `use`",
                );
            }
            return None;
        };
        Some(Node::new(
            kind,
            Span::new(self.identity.file, start, self.previous_end()),
        ))
    }

    fn parse_ui(&mut self, visibility: Visibility) -> UiDeclaration {
        self.expect_contextual("ui", "UI declaration");
        let name = self.expect_name(NameCategory::Upper, "UI declaration name");
        self.expect_contextual("for", "UI declaration");
        let machine = self.parse_type_path();
        self.expect_simple(TokenKind::LParen, "UI observation binding");
        let observation = self.expect_name(NameCategory::Lower, "UI observation binding");
        self.expect_simple(TokenKind::RParen, "UI observation binding");
        let open = self.expect_simple(TokenKind::LBrace, "UI declaration body");

        let (nodes, embedded_core_comments, body_span) = if self.at_simple(TokenKind::UiBody) {
            let body = self.bump().clone();
            let parsed = parse_ui_body(&self.identity, &body.lexeme, body.span.start);
            self.diagnostics.extend(parsed.diagnostics);
            (parsed.nodes, parsed.embedded_core_comments, body.span)
        } else {
            self.error_here(
                ParseDiagnosticKind::InvalidUi,
                "expected a lossless UI body after the declaration header",
            );
            while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
                self.bump();
            }
            (
                Vec::new(),
                Vec::new(),
                Span::empty(self.identity.file, open.end),
            )
        };
        self.expect_simple(TokenKind::RBrace, "UI declaration body");

        UiDeclaration {
            visibility,
            name,
            machine,
            observation,
            body: UiBody {
                nodes,
                embedded_core_comments,
                span: body_span,
            },
        }
    }

    fn parse_struct(&mut self, visibility: Visibility) -> StructDeclaration {
        self.expect_keyword(Keyword::Struct, "struct declaration");
        let name = self.expect_name(NameCategory::Upper, "struct name");
        let fields = self.parse_typed_field_body("struct fields");
        StructDeclaration {
            visibility,
            name,
            fields,
        }
    }

    fn parse_enum(&mut self, visibility: Visibility) -> EnumDeclaration {
        self.expect_keyword(Keyword::Enum, "enum declaration");
        let name = self.expect_name(NameCategory::Upper, "enum name");
        self.expect_simple(TokenKind::LBrace, "enum declaration");
        let mut variants = Vec::new();
        if self.at_simple(TokenKind::RBrace) {
            self.error_here(
                ParseDiagnosticKind::InvalidDeclaration,
                "an enum must declare at least one variant",
            );
        }
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start = self.current().span.start;
            let variant_name = self.expect_name(NameCategory::Upper, "enum variant");
            let fields = if self.at_simple(TokenKind::LBrace) {
                self.parse_typed_field_body("enum variant fields")
            } else {
                Vec::new()
            };
            variants.push(EnumVariant {
                name: variant_name,
                fields,
                span: Span::new(self.identity.file, start, self.previous_end()),
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RBrace) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between enum variants",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RBrace, "enum declaration");
        EnumDeclaration {
            visibility,
            name,
            variants,
        }
    }

    fn parse_key(&mut self, visibility: Visibility) -> KeyDeclaration {
        self.expect_keyword(Keyword::Key, "key declaration");
        let name = self.expect_name(NameCategory::Upper, "key name");
        self.expect_simple(TokenKind::LParen, "key declaration");
        let value = self.parse_type();
        self.expect_simple(TokenKind::RParen, "key declaration");
        let semicolon = self.expect_simple(TokenKind::Semicolon, "key declaration");
        KeyDeclaration {
            visibility,
            name,
            value,
            semicolon,
        }
    }

    fn parse_const(&mut self, visibility: Visibility) -> ConstDeclaration {
        self.expect_keyword(Keyword::Const, "const declaration");
        let name = self.expect_name(NameCategory::Constant, "constant name");
        self.expect_simple(TokenKind::Colon, "const declaration");
        let ty = self.parse_type();
        self.expect_simple(TokenKind::Eq, "const declaration");
        let value = self.parse_expression();
        let semicolon = self.expect_simple(TokenKind::Semicolon, "const declaration");
        ConstDeclaration {
            visibility,
            name,
            ty,
            value,
            semicolon,
        }
    }

    fn parse_function(&mut self, visibility: Visibility) -> FunctionDeclaration {
        self.expect_keyword(Keyword::Fn, "function declaration");
        let name = self.expect_name(NameCategory::Lower, "function name");
        let parameters = self.parse_parameter_list();
        self.expect_simple(TokenKind::Arrow, "function result");
        let result = self.parse_type();
        let body = self.parse_block();
        FunctionDeclaration {
            visibility,
            name,
            parameters,
            result,
            body,
        }
    }

    fn parse_parameter_list(&mut self) -> Vec<Parameter> {
        self.expect_simple(TokenKind::LParen, "parameter list");
        let mut parameters = Vec::new();
        while !self.at_simple(TokenKind::RParen) && !self.at_simple(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_name(NameCategory::Lower, "parameter name");
            self.expect_simple(TokenKind::Colon, "parameter");
            let ty = self.parse_type();
            parameters.push(Parameter {
                name,
                span: Span::new(self.identity.file, start, ty.span.end),
                ty,
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RParen) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between parameters",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RParen, "parameter list");
        parameters
    }

    fn parse_typed_field_body(&mut self, context: &str) -> Vec<TypedField> {
        self.expect_simple(TokenKind::LBrace, context);
        let fields = self.parse_typed_fields_until(TokenKind::RBrace, context);
        self.expect_simple(TokenKind::RBrace, context);
        fields
    }

    fn parse_typed_fields_until(&mut self, end: TokenKind, context: &str) -> Vec<TypedField> {
        let mut fields = Vec::new();
        while !self.at_simple(end.clone()) && !self.at_simple(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_name(NameCategory::Lower, "field name");
            self.expect_simple(TokenKind::Colon, context);
            let ty = self.parse_type();
            fields.push(TypedField {
                name,
                span: Span::new(self.identity.file, start, ty.span.end),
                ty,
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(end.clone()) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        format!("expected `,` between {context}"),
                    );
                }
                break;
            }
        }
        fields
    }

    fn parse_type(&mut self) -> TypeExpression {
        let start = self.current().span.start;
        if self.eat_simple(TokenKind::LParen).is_some() {
            if let Some(close) = self.eat_simple(TokenKind::RParen) {
                return Node::new(
                    TypeExpressionKind::Unit,
                    Span::new(self.identity.file, start, close.end),
                );
            }
            let first = self.parse_type();
            if self.eat_simple(TokenKind::Comma).is_none() {
                self.error_here(
                    ParseDiagnosticKind::InvalidType,
                    "a parenthesized type must be `()` or a tuple with at least two elements",
                );
                let close = self.expect_simple(TokenKind::RParen, "tuple type");
                return Node::new(first.kind, Span::new(self.identity.file, start, close.end));
            }
            let mut values = vec![first];
            if self.at_simple(TokenKind::RParen) {
                self.error_here(
                    ParseDiagnosticKind::InvalidType,
                    "a tuple type requires at least two elements",
                );
            } else {
                values.push(self.parse_type());
                while self.eat_simple(TokenKind::Comma).is_some()
                    && !self.at_simple(TokenKind::RParen)
                {
                    values.push(self.parse_type());
                }
            }
            let close = self.expect_simple(TokenKind::RParen, "tuple type");
            return Node::new(
                TypeExpressionKind::Tuple(values),
                Span::new(self.identity.file, start, close.end),
            );
        }

        let path = self.parse_type_path();
        Node::new(TypeExpressionKind::Path(path.clone()), path.span)
    }

    fn parse_type_path(&mut self) -> TypePath {
        let start = self.current().span.start;
        let mut segments = Vec::new();
        loop {
            let segment_start = self.current().span.start;
            let name = self.expect_name(NameCategory::Upper, "type name");
            let arguments = if self.eat_simple(TokenKind::Less).is_some() {
                let mut arguments = Vec::new();
                if self.at_simple(TokenKind::Greater) {
                    self.error_here(
                        ParseDiagnosticKind::InvalidType,
                        "type arguments cannot be empty",
                    );
                } else {
                    arguments.push(self.parse_type());
                    while self.eat_simple(TokenKind::Comma).is_some()
                        && !self.at_simple(TokenKind::Greater)
                    {
                        arguments.push(self.parse_type());
                    }
                }
                self.expect_simple(TokenKind::Greater, "type arguments");
                arguments
            } else {
                Vec::new()
            };
            segments.push(TypePathSegment {
                name,
                arguments,
                span: Span::new(self.identity.file, segment_start, self.previous_end()),
            });
            if self.eat_simple(TokenKind::ColonColon).is_none() {
                break;
            }
        }
        TypePath {
            segments,
            span: Span::new(self.identity.file, start, self.previous_end()),
        }
    }

    fn parse_machine(&mut self, visibility: Visibility) -> MachineDeclaration {
        self.expect_keyword(Keyword::Machine, "machine declaration");
        let name = self.expect_name(NameCategory::Upper, "machine name");
        self.expect_simple(TokenKind::LBrace, "machine declaration");
        let mut members = Vec::new();
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start_cursor = self.cursor;
            if let Some(member) = self.parse_machine_member() {
                members.push(member);
            } else {
                self.error_here(
                    ParseDiagnosticKind::InvalidMember,
                    "unexpected machine member",
                );
                self.synchronize_member();
            }
            if self.cursor == start_cursor {
                self.bump();
            }
        }
        self.expect_simple(TokenKind::RBrace, "machine declaration");
        MachineDeclaration {
            visibility,
            name,
            members,
        }
    }

    fn parse_part(&mut self, visibility: Visibility) -> PartDeclaration {
        self.expect_keyword(Keyword::Part, "part declaration");
        let name = self.expect_name(NameCategory::Upper, "part name");
        let parameters = if self.at_simple(TokenKind::LParen) {
            self.parse_parameter_list()
        } else {
            Vec::new()
        };
        self.expect_simple(TokenKind::LBrace, "part declaration");
        let mut members = Vec::new();
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start_cursor = self.cursor;
            if let Some(member) = self.parse_part_member() {
                members.push(member);
            } else {
                self.error_here(ParseDiagnosticKind::InvalidMember, "unexpected part member");
                self.synchronize_member();
            }
            if self.cursor == start_cursor {
                self.bump();
            }
        }
        self.expect_simple(TokenKind::RBrace, "part declaration");
        PartDeclaration {
            visibility,
            name,
            parameters,
            members,
        }
    }

    fn parse_machine_member(&mut self) -> Option<MachineMember> {
        let start = self.current().span.start;
        let kind = if self.at_keyword(Keyword::Config) {
            MachineMemberKind::Config(self.parse_config())
        } else if self.at_keyword(Keyword::Require) {
            MachineMemberKind::Require(self.parse_require())
        } else if self.at_keyword(Keyword::Const) {
            MachineMemberKind::Const(self.parse_const(Visibility::Private))
        } else if self.at_keyword(Keyword::Fn) {
            MachineMemberKind::Function(self.parse_function(Visibility::Private))
        } else if self.at_keyword(Keyword::Part) {
            MachineMemberKind::Part(self.parse_part_instance())
        } else if self.at_keyword(Keyword::Events) {
            MachineMemberKind::Events(self.parse_protocol_section(Keyword::Events))
        } else if self.at_keyword(Keyword::Commands) {
            MachineMemberKind::Commands(self.parse_protocol_section(Keyword::Commands))
        } else if self.at_keyword(Keyword::Port) {
            MachineMemberKind::Port(self.parse_port())
        } else if self.at_keyword(Keyword::Outcomes) {
            MachineMemberKind::Outcomes(self.parse_outcomes(false))
        } else if self.at_keyword(Keyword::State) {
            MachineMemberKind::State(self.parse_state())
        } else if self.at_keyword(Keyword::Computed) {
            MachineMemberKind::Computed(self.parse_computed(Visibility::Private))
        } else if self.at_keyword(Keyword::Invariant) {
            MachineMemberKind::Invariant(self.parse_invariant())
        } else if self.at_keyword(Keyword::Observe) {
            MachineMemberKind::Observe(self.parse_observe())
        } else if self.at_keyword(Keyword::On) {
            MachineMemberKind::Handler(self.parse_handler())
        } else if self.at_keyword(Keyword::Update) {
            MachineMemberKind::Update(self.parse_update(Visibility::Private))
        } else if self.at_keyword(Keyword::Before) {
            MachineMemberKind::BeforeCommit(self.parse_before_commit())
        } else {
            return None;
        };
        Some(Node::new(
            kind,
            Span::new(self.identity.file, start, self.previous_end()),
        ))
    }

    fn parse_part_member(&mut self) -> Option<PartMember> {
        let start = self.current().span.start;
        let visibility = if self.eat_keyword(Keyword::Pub).is_some() {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let kind = if self.at_keyword(Keyword::Require) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Require(self.parse_require())
        } else if self.at_keyword(Keyword::Requires) {
            self.reject_part_visibility(visibility);
            PartMemberKind::RequiresOutcomes(self.parse_outcomes(true))
        } else if self.at_keyword(Keyword::Const) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Const(self.parse_const(Visibility::Private))
        } else if self.at_keyword(Keyword::Fn) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Function(self.parse_function(Visibility::Private))
        } else if self.at_keyword(Keyword::Events) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Events(self.parse_protocol_section(Keyword::Events))
        } else if self.at_keyword(Keyword::Commands) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Commands(self.parse_protocol_section(Keyword::Commands))
        } else if self.at_keyword(Keyword::Port) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Port(self.parse_port())
        } else if self.at_keyword(Keyword::State) {
            self.reject_part_visibility(visibility);
            PartMemberKind::State(self.parse_state())
        } else if self.at_keyword(Keyword::Computed) {
            PartMemberKind::Computed(self.parse_computed(visibility))
        } else if self.at_keyword(Keyword::Invariant) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Invariant(self.parse_invariant())
        } else if self.at_keyword(Keyword::Observe) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Observe(self.parse_observe())
        } else if self.at_keyword(Keyword::On) {
            self.reject_part_visibility(visibility);
            PartMemberKind::Handler(self.parse_handler())
        } else if self.at_keyword(Keyword::Update) {
            PartMemberKind::Update(self.parse_update(visibility))
        } else {
            if visibility == Visibility::Public {
                self.error_here(
                    ParseDiagnosticKind::InvalidMember,
                    "only `computed` and `update` may be public inside a part",
                );
            }
            return None;
        };
        Some(Node::new(
            kind,
            Span::new(self.identity.file, start, self.previous_end()),
        ))
    }

    fn reject_part_visibility(&mut self, visibility: Visibility) {
        if visibility == Visibility::Public {
            self.error_here(
                ParseDiagnosticKind::InvalidMember,
                "only `computed` and `update` may be public inside a part",
            );
        }
    }

    fn parse_config(&mut self) -> ConfigSection {
        self.expect_keyword(Keyword::Config, "config section");
        ConfigSection {
            fields: self.parse_typed_field_body("config fields"),
        }
    }

    fn parse_require(&mut self) -> RequireDeclaration {
        self.expect_keyword(Keyword::Require, "require declaration");
        let condition = self.parse_expression();
        let semicolon = self.expect_simple(TokenKind::Semicolon, "require declaration");
        RequireDeclaration {
            condition,
            semicolon,
        }
    }

    fn parse_part_instance(&mut self) -> PartInstance {
        self.expect_keyword(Keyword::Part, "part instance");
        let name = self.expect_name(NameCategory::Lower, "part instance name");
        self.expect_simple(TokenKind::Eq, "part instance");
        let part = self.parse_type_path();
        let arguments = self.parse_argument_list();
        let semicolon = self.expect_simple(TokenKind::Semicolon, "part instance");
        PartInstance {
            name,
            part,
            arguments,
            semicolon,
        }
    }

    fn parse_protocol_section(&mut self, keyword: Keyword) -> ProtocolSection {
        self.expect_keyword(keyword, "protocol section");
        self.expect_simple(TokenKind::LBrace, "protocol section");
        let mut variants = Vec::new();
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            variants.push(self.parse_protocol_variant());
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RBrace) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between protocol variants",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RBrace, "protocol section");
        ProtocolSection { variants }
    }

    fn parse_protocol_variant(&mut self) -> ProtocolVariant {
        let start = self.current().span.start;
        let name = self.expect_name(NameCategory::Upper, "protocol variant");
        let parameters = if self.at_simple(TokenKind::LParen) {
            self.parse_parameter_list()
        } else {
            Vec::new()
        };
        ProtocolVariant {
            name,
            parameters,
            span: Span::new(self.identity.file, start, self.previous_end()),
        }
    }

    fn parse_port(&mut self) -> PortDeclaration {
        self.expect_keyword(Keyword::Port, "port declaration");
        let name = self.expect_name(NameCategory::Lower, "port name");
        self.expect_simple(TokenKind::Eq, "port declaration");
        let contract = self.parse_type_path();
        self.expect_simple(TokenKind::LBrace, "port binding");
        let fields = self.parse_field_initializers(TokenKind::RBrace, false);
        self.expect_simple(TokenKind::RBrace, "port binding");
        let semicolon = self.expect_simple(TokenKind::Semicolon, "port declaration");
        PortDeclaration {
            name,
            contract,
            fields,
            semicolon,
        }
    }

    fn parse_outcomes(&mut self, required: bool) -> OutcomeSection {
        if required {
            self.expect_keyword(Keyword::Requires, "required outcomes");
        }
        self.expect_keyword(Keyword::Outcomes, "outcomes section");
        self.expect_simple(TokenKind::LBrace, "outcomes section");
        let mut entries = Vec::new();
        if self.at_simple(TokenKind::RBrace) {
            self.error_here(
                ParseDiagnosticKind::InvalidMember,
                "an outcomes section must declare at least one outcome",
            );
        }
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start = self.current().span.start;
            let policy = if self.eat_keyword(Keyword::Commit).is_some() {
                OutcomePolicy::Commit
            } else if self.eat_keyword(Keyword::Abort).is_some() {
                OutcomePolicy::Abort
            } else {
                self.error_here(
                    ParseDiagnosticKind::InvalidMember,
                    "expected `commit` or `abort` outcome policy",
                );
                OutcomePolicy::Abort
            };
            let variant = self.parse_protocol_variant();
            entries.push(OutcomeEntry {
                policy,
                span: Span::new(self.identity.file, start, variant.span.end),
                variant,
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RBrace) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between outcomes",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RBrace, "outcomes section");
        OutcomeSection { entries }
    }

    fn parse_state(&mut self) -> StateSection {
        self.expect_keyword(Keyword::State, "state section");
        self.expect_simple(TokenKind::LBrace, "state section");
        let mut fields = Vec::new();
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_name(NameCategory::Lower, "state field");
            self.expect_simple(TokenKind::Colon, "state field");
            let ty = self.parse_type();
            self.expect_simple(TokenKind::Eq, "state field");
            let initial = self.parse_expression();
            fields.push(StateField {
                name,
                ty,
                span: Span::new(self.identity.file, start, initial.span.end),
                initial,
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RBrace) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between state fields",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RBrace, "state section");
        StateSection { fields }
    }

    fn parse_computed(&mut self, visibility: Visibility) -> ComputedDeclaration {
        self.expect_keyword(Keyword::Computed, "computed declaration");
        let name = self.expect_name(NameCategory::Lower, "computed name");
        let ty = if self.eat_simple(TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect_simple(TokenKind::Eq, "computed declaration");
        let value = self.parse_expression();
        let semicolon = self.expect_simple(TokenKind::Semicolon, "computed declaration");
        ComputedDeclaration {
            visibility,
            name,
            ty,
            value,
            semicolon,
        }
    }

    fn parse_invariant(&mut self) -> InvariantDeclaration {
        self.expect_keyword(Keyword::Invariant, "invariant declaration");
        if self.eat_simple(TokenKind::LBrace).is_some() {
            let mut conditions = Vec::new();
            if self.at_simple(TokenKind::RBrace) {
                self.error_here(
                    ParseDiagnosticKind::InvalidMember,
                    "a grouped invariant must contain at least one expression",
                );
            }
            while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
                conditions.push(self.parse_expression());
                if self.eat_simple(TokenKind::Comma).is_none() {
                    if !self.at_simple(TokenKind::RBrace) {
                        self.error_here(
                            ParseDiagnosticKind::MissingToken,
                            "expected `,` between invariant expressions",
                        );
                    }
                    break;
                }
            }
            self.expect_simple(TokenKind::RBrace, "grouped invariant");
            InvariantDeclaration {
                conditions,
                grouped: true,
                semicolon: None,
            }
        } else {
            let condition = self.parse_expression();
            let semicolon = self.expect_simple(TokenKind::Semicolon, "invariant declaration");
            InvariantDeclaration {
                conditions: vec![condition],
                grouped: false,
                semicolon: Some(semicolon),
            }
        }
    }

    fn parse_observe(&mut self) -> ObserveSection {
        self.expect_keyword(Keyword::Observe, "observe section");
        self.expect_simple(TokenKind::LBrace, "observe section");
        let mut fields = Vec::new();
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_name(NameCategory::Lower, "observation field");
            let value = if self.eat_simple(TokenKind::Colon).is_some() {
                Some(self.parse_expression())
            } else {
                None
            };
            let end = value.as_ref().map_or(name.span.end, |value| value.span.end);
            fields.push(ObserveField {
                name,
                value,
                span: Span::new(self.identity.file, start, end),
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RBrace) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between observation fields",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RBrace, "observe section");
        ObserveSection { fields }
    }

    fn parse_handler(&mut self) -> HandlerDeclaration {
        self.expect_keyword(Keyword::On, "handler declaration");
        let input = self.parse_protocol_selector();
        let parameters = if self.eat_simple(TokenKind::LParen).is_some() {
            let values = self.parse_pattern_list(TokenKind::RParen);
            self.expect_simple(TokenKind::RParen, "handler input pattern");
            values
        } else {
            Vec::new()
        };
        let body = self.parse_block();
        HandlerDeclaration {
            input,
            parameters,
            body,
        }
    }

    fn parse_update(&mut self, visibility: Visibility) -> UpdateDeclaration {
        self.expect_keyword(Keyword::Update, "update declaration");
        let name = self.expect_name(NameCategory::Lower, "update name");
        let parameters = self.parse_parameter_list();
        let result = if self.eat_simple(TokenKind::Arrow).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block();
        UpdateDeclaration {
            visibility,
            name,
            parameters,
            result,
            body,
        }
    }

    fn parse_before_commit(&mut self) -> BeforeCommitDeclaration {
        self.expect_keyword(Keyword::Before, "before commit declaration");
        self.expect_keyword(Keyword::Commit, "before commit declaration");
        BeforeCommitDeclaration {
            body: self.parse_block(),
        }
    }

    fn parse_protocol_selector(&mut self) -> ProtocolSelector {
        let start = self.current().span.start;
        let first = self.expect_name(NameCategory::Declaration, "protocol selector");
        if self.eat_simple(TokenKind::Dot).is_some() {
            self.validate_identifier(&first, NameCategory::Lower, "protocol owner");
            let variant = self.expect_name(NameCategory::Upper, "protocol variant");
            ProtocolSelector {
                owner: Some(first),
                span: Span::new(self.identity.file, start, variant.span.end),
                variant,
            }
        } else {
            self.validate_identifier(&first, NameCategory::Upper, "protocol variant");
            ProtocolSelector {
                owner: None,
                span: Span::new(self.identity.file, start, first.span.end),
                variant: first,
            }
        }
    }

    fn parse_argument_list(&mut self) -> Vec<Expression> {
        self.expect_simple(TokenKind::LParen, "argument list");
        let mut arguments = Vec::new();
        while !self.at_simple(TokenKind::RParen) && !self.at_simple(TokenKind::Eof) {
            arguments.push(self.parse_expression());
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RParen) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between arguments",
                    );
                }
                break;
            }
        }
        self.expect_simple(TokenKind::RParen, "argument list");
        arguments
    }

    fn parse_block(&mut self) -> Block {
        let open = self.expect_simple(TokenKind::LBrace, "block");
        let mut statements = Vec::new();
        let mut tail = None;

        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let start_cursor = self.cursor;
            if self.at_keyword(Keyword::Let) {
                statements.push(self.parse_let_statement());
            } else if self.at_keyword(Keyword::Emit) {
                statements.push(self.parse_emit_statement());
            } else if self.at_keyword(Keyword::While) {
                statements.push(self.parse_while_statement());
            } else if self.at_keyword(Keyword::Unreachable) {
                statements.push(self.parse_unreachable_statement());
            } else if self.at_identifier() && self.nth_is_simple(1, TokenKind::Eq) {
                statements.push(self.parse_assignment_statement());
            } else {
                let expression = self.parse_expression();
                if let Some(semicolon) = self.eat_simple(TokenKind::Semicolon) {
                    let span = Span::new(self.identity.file, expression.span.start, semicolon.end);
                    statements.push(Node::new(
                        StatementKind::Expression {
                            expression,
                            semicolon,
                        },
                        span,
                    ));
                } else if self.at_simple(TokenKind::RBrace) {
                    tail = Some(Box::new(expression));
                    break;
                } else if matches!(
                    expression.kind,
                    ExpressionKind::If(_) | ExpressionKind::Match(_)
                ) {
                    let span = expression.span;
                    statements.push(Node::new(StatementKind::BlockExpression(expression), span));
                } else {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `;` after non-final expression",
                    );
                    let semicolon = Span::empty(self.identity.file, expression.span.end);
                    let span = expression.span;
                    statements.push(Node::new(
                        StatementKind::Expression {
                            expression,
                            semicolon,
                        },
                        span,
                    ));
                }
            }
            if self.cursor == start_cursor {
                self.error_here(
                    ParseDiagnosticKind::InvalidStatement,
                    "unable to parse statement",
                );
                self.bump();
            }
        }

        let close = self.expect_simple(TokenKind::RBrace, "block");
        Block {
            statements,
            tail,
            span: Span::new(self.identity.file, open.start, close.end),
        }
    }

    fn parse_let_statement(&mut self) -> Statement {
        let start = self.expect_keyword(Keyword::Let, "let statement").start;
        let name = self.expect_name(NameCategory::Lower, "let binding");
        let ty = if self.eat_simple(TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect_simple(TokenKind::Eq, "let statement");
        let value = self.parse_expression();
        let semicolon = self.expect_simple(TokenKind::Semicolon, "let statement");
        Node::new(
            StatementKind::Let {
                name,
                ty,
                value,
                semicolon,
            },
            Span::new(self.identity.file, start, semicolon.end),
        )
    }

    fn parse_assignment_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        let target = self.expect_name(NameCategory::Lower, "state assignment target");
        self.expect_simple(TokenKind::Eq, "state assignment");
        let value = self.parse_expression();
        let semicolon = self.expect_simple(TokenKind::Semicolon, "state assignment");
        Node::new(
            StatementKind::Assign {
                target,
                value,
                semicolon,
            },
            Span::new(self.identity.file, start, semicolon.end),
        )
    }

    fn parse_emit_statement(&mut self) -> Statement {
        let start = self.expect_keyword(Keyword::Emit, "emit statement").start;
        let selector = self.parse_protocol_selector();
        let arguments = if self.at_simple(TokenKind::LParen) {
            self.parse_argument_list()
        } else {
            Vec::new()
        };
        let output = OutputConstructor {
            span: Span::new(self.identity.file, selector.span.start, self.previous_end()),
            selector,
            arguments,
        };
        let semicolon = self.expect_simple(TokenKind::Semicolon, "emit statement");
        Node::new(
            StatementKind::Emit { output, semicolon },
            Span::new(self.identity.file, start, semicolon.end),
        )
    }

    fn parse_while_statement(&mut self) -> Statement {
        let start = self.expect_keyword(Keyword::While, "while statement").start;
        let condition = self.parse_expression();
        self.expect_keyword(Keyword::Decreases, "while statement");
        self.expect_simple(TokenKind::LParen, "decreases measure");
        let decreases = self.parse_expression();
        self.expect_simple(TokenKind::RParen, "decreases measure");
        let body = self.parse_block();
        let end = body.span.end;
        Node::new(
            StatementKind::While {
                condition,
                decreases,
                body,
            },
            Span::new(self.identity.file, start, end),
        )
    }

    fn parse_unreachable_statement(&mut self) -> Statement {
        let start = self
            .expect_keyword(Keyword::Unreachable, "unreachable statement")
            .start;
        let semicolon = self.expect_simple(TokenKind::Semicolon, "unreachable statement");
        Node::new(
            StatementKind::Unreachable { semicolon },
            Span::new(self.identity.file, start, semicolon.end),
        )
    }

    fn parse_expression(&mut self) -> Expression {
        if self.at_keyword(Keyword::Return) {
            self.parse_return_expression()
        } else {
            self.parse_logical_or()
        }
    }

    fn parse_return_expression(&mut self) -> Expression {
        let start = self
            .expect_keyword(Keyword::Return, "return expression")
            .start;
        let value = if self.expression_terminator() {
            None
        } else {
            Some(Box::new(self.parse_expression()))
        };
        let end = value
            .as_ref()
            .map_or(self.previous_end(), |value| value.span.end);
        Node::new(
            ExpressionKind::Return(value),
            Span::new(self.identity.file, start, end),
        )
    }

    fn parse_logical_or(&mut self) -> Expression {
        let mut expression = self.parse_logical_and();
        while self.eat_simple(TokenKind::PipePipe).is_some() {
            let right = self.parse_logical_and();
            let span = expression.span.through(right.span);
            expression = Node::new(
                ExpressionKind::Binary {
                    operator: BinaryOperator::Or,
                    left: Box::new(expression),
                    right: Box::new(right),
                },
                span,
            );
        }
        expression
    }

    fn parse_logical_and(&mut self) -> Expression {
        let mut expression = self.parse_comparison();
        while self.eat_simple(TokenKind::AmpAmp).is_some() {
            let right = self.parse_comparison();
            let span = expression.span.through(right.span);
            expression = Node::new(
                ExpressionKind::Binary {
                    operator: BinaryOperator::And,
                    left: Box::new(expression),
                    right: Box::new(right),
                },
                span,
            );
        }
        expression
    }

    fn parse_comparison(&mut self) -> Expression {
        let mut expression = self.parse_additive();
        let mut compared = false;
        loop {
            if self.eat_keyword(Keyword::Is).is_some() {
                if compared {
                    self.error_previous(
                        ParseDiagnosticKind::ComparisonChain,
                        "comparison chains are not part of Uhura; combine comparisons with `&&`",
                    );
                }
                let pattern = self.parse_pattern();
                let span = expression.span.through(pattern.span);
                expression = Node::new(
                    ExpressionKind::Is {
                        value: Box::new(expression),
                        pattern,
                    },
                    span,
                );
                compared = true;
                continue;
            }
            let operator = if self.eat_simple(TokenKind::EqEq).is_some() {
                Some(ComparisonOperator::Equal)
            } else if self.eat_simple(TokenKind::NotEqual).is_some() {
                Some(ComparisonOperator::NotEqual)
            } else if self.eat_simple(TokenKind::Less).is_some() {
                Some(ComparisonOperator::Less)
            } else if self.eat_simple(TokenKind::LessEqual).is_some() {
                Some(ComparisonOperator::LessEqual)
            } else if self.eat_simple(TokenKind::Greater).is_some() {
                Some(ComparisonOperator::Greater)
            } else if self.eat_simple(TokenKind::GreaterEqual).is_some() {
                Some(ComparisonOperator::GreaterEqual)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            if compared {
                self.error_previous(
                    ParseDiagnosticKind::ComparisonChain,
                    "comparison chains are not part of Uhura; combine comparisons with `&&`",
                );
            }
            let right = self.parse_additive();
            let span = expression.span.through(right.span);
            expression = Node::new(
                ExpressionKind::Compare {
                    operator,
                    left: Box::new(expression),
                    right: Box::new(right),
                },
                span,
            );
            compared = true;
        }
        expression
    }

    fn parse_additive(&mut self) -> Expression {
        let mut expression = self.parse_multiplicative();
        loop {
            let operator = if self.eat_simple(TokenKind::Plus).is_some() {
                Some(BinaryOperator::Add)
            } else if self.eat_simple(TokenKind::Minus).is_some() {
                Some(BinaryOperator::Subtract)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.parse_multiplicative();
            let span = expression.span.through(right.span);
            expression = Node::new(
                ExpressionKind::Binary {
                    operator,
                    left: Box::new(expression),
                    right: Box::new(right),
                },
                span,
            );
        }
        expression
    }

    fn parse_multiplicative(&mut self) -> Expression {
        let mut expression = self.parse_unary();
        while self.eat_simple(TokenKind::Star).is_some() {
            let right = self.parse_unary();
            let span = expression.span.through(right.span);
            expression = Node::new(
                ExpressionKind::Binary {
                    operator: BinaryOperator::Multiply,
                    left: Box::new(expression),
                    right: Box::new(right),
                },
                span,
            );
        }
        expression
    }

    fn parse_unary(&mut self) -> Expression {
        if let Some(operator) = if self.eat_simple(TokenKind::Bang).is_some() {
            Some(UnaryOperator::Not)
        } else if self.eat_simple(TokenKind::Minus).is_some() {
            Some(UnaryOperator::Negate)
        } else {
            None
        } {
            let start = self.previous().span.start;
            let value = self.parse_unary();
            let end = value.span.end;
            Node::new(
                ExpressionKind::Unary {
                    operator,
                    value: Box::new(value),
                },
                Span::new(self.identity.file, start, end),
            )
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Expression {
        let mut expression = self.parse_primary();
        loop {
            if self.eat_simple(TokenKind::LParen).is_some() {
                let mut arguments = Vec::new();
                while !self.at_simple(TokenKind::RParen) && !self.at_simple(TokenKind::Eof) {
                    if self.at_simple(TokenKind::Pipe) {
                        arguments.push(CallArgument::Binder(self.parse_binder()));
                    } else {
                        arguments.push(CallArgument::Expression(self.parse_expression()));
                    }
                    if self.eat_simple(TokenKind::Comma).is_none() {
                        if !self.at_simple(TokenKind::RParen) {
                            self.error_here(
                                ParseDiagnosticKind::MissingToken,
                                "expected `,` between call arguments",
                            );
                        }
                        break;
                    }
                }
                let close = self.expect_simple(TokenKind::RParen, "call expression");
                let start = expression.span.start;
                expression = Node::new(
                    ExpressionKind::Call {
                        callee: Box::new(expression),
                        arguments,
                    },
                    Span::new(self.identity.file, start, close.end),
                );
            } else if self.eat_simple(TokenKind::Dot).is_some() {
                let member = self.expect_member_name();
                let start = expression.span.start;
                let end = member.span.end;
                expression = Node::new(
                    ExpressionKind::Member {
                        value: Box::new(expression),
                        member,
                    },
                    Span::new(self.identity.file, start, end),
                );
            } else if self.eat_simple(TokenKind::LBracket).is_some() {
                let index = self.parse_expression();
                let close = self.expect_simple(TokenKind::RBracket, "index expression");
                let start = expression.span.start;
                expression = Node::new(
                    ExpressionKind::Index {
                        value: Box::new(expression),
                        index: Box::new(index),
                    },
                    Span::new(self.identity.file, start, close.end),
                );
            } else {
                break;
            }
        }
        expression
    }

    fn parse_primary(&mut self) -> Expression {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Keyword(Keyword::True) => {
                self.bump();
                Node::new(ExpressionKind::Literal(Literal::Bool(true)), token.span)
            }
            TokenKind::Keyword(Keyword::False) => {
                self.bump();
                Node::new(ExpressionKind::Literal(Literal::Bool(false)), token.span)
            }
            TokenKind::Integer(raw) => {
                let raw = raw.clone();
                self.bump();
                Node::new(
                    ExpressionKind::Literal(Literal::Integer { raw }),
                    token.span,
                )
            }
            TokenKind::Decimal(raw) => {
                let raw = raw.clone();
                self.bump();
                Node::new(
                    ExpressionKind::Literal(Literal::Decimal { raw }),
                    token.span,
                )
            }
            TokenKind::Text(value) => {
                let value = value.clone();
                let raw = token.lexeme.clone();
                self.bump();
                Node::new(
                    ExpressionKind::Literal(Literal::Text { raw, value }),
                    token.span,
                )
            }
            TokenKind::LParen => self.parse_parenthesized_expression(),
            TokenKind::LBracket => self.parse_sequence_expression(),
            TokenKind::LBrace => {
                let block = self.parse_block();
                let span = block.span;
                Node::new(ExpressionKind::Block(block), span)
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_expression(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expression(),
            TokenKind::Identifier(_) => self.parse_name_or_record_expression(),
            _ => {
                self.error_here(
                    ParseDiagnosticKind::InvalidExpression,
                    format!("expected expression, found {}", token.kind.describe()),
                );
                if !self.at_simple(TokenKind::Eof) {
                    self.bump();
                }
                let placeholder = Identifier::new("error", token.span);
                Node::new(
                    ExpressionKind::Name(QualifiedName {
                        segments: vec![placeholder],
                        span: token.span,
                    }),
                    token.span,
                )
            }
        }
    }

    fn parse_parenthesized_expression(&mut self) -> Expression {
        let open = self.expect_simple(TokenKind::LParen, "parenthesized expression");
        if let Some(close) = self.eat_simple(TokenKind::RParen) {
            return Node::new(
                ExpressionKind::Unit,
                Span::new(self.identity.file, open.start, close.end),
            );
        }

        let first = self.parse_expression();
        if self.eat_simple(TokenKind::Comma).is_none() {
            let close = self.expect_simple(TokenKind::RParen, "grouped expression");
            return Node::new(
                ExpressionKind::Group(Box::new(first)),
                Span::new(self.identity.file, open.start, close.end),
            );
        }

        let mut values = vec![first];
        if self.at_simple(TokenKind::RParen) {
            self.error_here(
                ParseDiagnosticKind::InvalidExpression,
                "a tuple expression requires at least two elements",
            );
        } else {
            values.push(self.parse_expression());
            while self.eat_simple(TokenKind::Comma).is_some() && !self.at_simple(TokenKind::RParen)
            {
                values.push(self.parse_expression());
            }
        }
        let close = self.expect_simple(TokenKind::RParen, "tuple expression");
        Node::new(
            ExpressionKind::Tuple(values),
            Span::new(self.identity.file, open.start, close.end),
        )
    }

    fn parse_sequence_expression(&mut self) -> Expression {
        let open = self.expect_simple(TokenKind::LBracket, "sequence expression");
        let mut values = Vec::new();
        while !self.at_simple(TokenKind::RBracket) && !self.at_simple(TokenKind::Eof) {
            values.push(self.parse_expression());
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(TokenKind::RBracket) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between sequence elements",
                    );
                }
                break;
            }
        }
        let close = self.expect_simple(TokenKind::RBracket, "sequence expression");
        Node::new(
            ExpressionKind::Sequence(values),
            Span::new(self.identity.file, open.start, close.end),
        )
    }

    fn parse_name_or_record_expression(&mut self) -> Expression {
        let name = self.parse_qualified_name();
        if self.at_simple(TokenKind::LBrace) && self.looks_like_record(&name) {
            let start = name.span.start;
            self.bump();
            let mut fields = Vec::new();
            let mut base = None;
            while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
                if self.eat_simple(TokenKind::DotDot).is_some() {
                    let value = self.parse_expression();
                    base = Some(Box::new(value));
                    if self.eat_simple(TokenKind::Comma).is_some()
                        && !self.at_simple(TokenKind::RBrace)
                    {
                        self.error_here(
                            ParseDiagnosticKind::InvalidExpression,
                            "record base must be the final record entry",
                        );
                    }
                    break;
                }
                fields.push(self.parse_field_initializer());
                if self.eat_simple(TokenKind::Comma).is_none() {
                    if !self.at_simple(TokenKind::RBrace) {
                        self.error_here(
                            ParseDiagnosticKind::MissingToken,
                            "expected `,` between record fields",
                        );
                    }
                    break;
                }
            }
            let close = self.expect_simple(TokenKind::RBrace, "record expression");
            Node::new(
                ExpressionKind::Record(RecordExpression {
                    constructor: name,
                    fields,
                    base,
                }),
                Span::new(self.identity.file, start, close.end),
            )
        } else {
            let span = name.span;
            Node::new(ExpressionKind::Name(name), span)
        }
    }

    fn parse_qualified_name(&mut self) -> QualifiedName {
        let start = self.current().span.start;
        let mut segments = vec![self.expect_name(NameCategory::Declaration, "name")];
        while self.eat_simple(TokenKind::ColonColon).is_some() {
            segments.push(self.expect_name(NameCategory::Declaration, "qualified name"));
        }
        QualifiedName {
            span: Span::new(self.identity.file, start, self.previous_end()),
            segments,
        }
    }

    fn parse_field_initializer(&mut self) -> FieldInitializer {
        let start = self.current().span.start;
        let name = self.expect_record_label("field initializer");
        let value = if self.eat_simple(TokenKind::Colon).is_some() {
            Some(self.parse_expression())
        } else {
            None
        };
        let end = value.as_ref().map_or(name.span.end, |value| value.span.end);
        FieldInitializer {
            name,
            value,
            span: Span::new(self.identity.file, start, end),
        }
    }

    fn parse_field_initializers(
        &mut self,
        end: TokenKind,
        allow_base: bool,
    ) -> Vec<FieldInitializer> {
        let mut fields = Vec::new();
        while !self.at_simple(end.clone()) && !self.at_simple(TokenKind::Eof) {
            if self.at_simple(TokenKind::DotDot) {
                self.error_here(
                    ParseDiagnosticKind::InvalidExpression,
                    if allow_base {
                        "record base must be parsed by the record expression"
                    } else {
                        "a port binding does not admit a record base"
                    },
                );
                self.bump();
                self.parse_expression();
            } else {
                fields.push(self.parse_field_initializer());
            }
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(end.clone()) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between field initializers",
                    );
                }
                break;
            }
        }
        fields
    }

    fn parse_binder(&mut self) -> BinderExpression {
        let start = self
            .expect_simple(TokenKind::Pipe, "binder expression")
            .start;
        let parameter = self.expect_name(NameCategory::Lower, "binder parameter");
        self.expect_simple(TokenKind::Pipe, "binder expression");
        let body = self.parse_expression();
        BinderExpression {
            parameter,
            span: Span::new(self.identity.file, start, body.span.end),
            body,
        }
    }

    fn parse_if_expression(&mut self) -> Expression {
        let start = self.expect_keyword(Keyword::If, "if expression").start;
        let condition = self.parse_expression();
        let then_branch = self.parse_block();
        let else_branch = if self.eat_keyword(Keyword::Else).is_some() {
            if self.at_keyword(Keyword::If) {
                Some(ElseBranch::If(Box::new(self.parse_if_expression())))
            } else {
                Some(ElseBranch::Block(self.parse_block()))
            }
        } else {
            None
        };
        let end = match &else_branch {
            Some(ElseBranch::Block(block)) => block.span.end,
            Some(ElseBranch::If(expression)) => expression.span.end,
            None => then_branch.span.end,
        };
        Node::new(
            ExpressionKind::If(IfExpression {
                condition: Box::new(condition),
                then_branch,
                else_branch,
            }),
            Span::new(self.identity.file, start, end),
        )
    }

    fn parse_match_expression(&mut self) -> Expression {
        let start = self
            .expect_keyword(Keyword::Match, "match expression")
            .start;
        let value = self.parse_expression();
        self.expect_simple(TokenKind::LBrace, "match expression");
        let mut arms = Vec::new();
        if self.at_simple(TokenKind::RBrace) {
            self.error_here(
                ParseDiagnosticKind::InvalidExpression,
                "a match expression must contain at least one arm",
            );
        }
        while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
            let arm_start = self.current().span.start;
            let pattern = self.parse_pattern();
            self.expect_simple(TokenKind::FatArrow, "match arm");
            let arm_value = self.parse_expression();
            let arm_end = arm_value.span.end;
            arms.push(MatchArm {
                pattern,
                value: arm_value,
                span: Span::new(self.identity.file, arm_start, arm_end),
            });
            if self.eat_simple(TokenKind::Comma).is_none() {
                self.error_here(
                    ParseDiagnosticKind::MissingToken,
                    "every match arm must end with `,`",
                );
                if !self.at_simple(TokenKind::RBrace) {
                    self.synchronize_match_arm();
                }
            }
        }
        let close = self.expect_simple(TokenKind::RBrace, "match expression");
        Node::new(
            ExpressionKind::Match(MatchExpression {
                value: Box::new(value),
                arms,
            }),
            Span::new(self.identity.file, start, close.end),
        )
    }

    fn parse_pattern_list(&mut self, end: TokenKind) -> Vec<Pattern> {
        let mut patterns = Vec::new();
        while !self.at_simple(end.clone()) && !self.at_simple(TokenKind::Eof) {
            patterns.push(self.parse_pattern());
            if self.eat_simple(TokenKind::Comma).is_none() {
                if !self.at_simple(end.clone()) {
                    self.error_here(
                        ParseDiagnosticKind::MissingToken,
                        "expected `,` between patterns",
                    );
                }
                break;
            }
        }
        patterns
    }

    fn parse_pattern(&mut self) -> Pattern {
        let first = self.parse_atomic_pattern();
        if !self.at_simple(TokenKind::Pipe) {
            return first;
        }
        let start = first.span.start;
        let mut alternatives = vec![first];
        while self.eat_simple(TokenKind::Pipe).is_some() {
            alternatives.push(self.parse_atomic_pattern());
        }
        let end = alternatives
            .last()
            .map_or(start, |alternative| alternative.span.end);
        Node::new(
            PatternKind::Alternative(alternatives),
            Span::new(self.identity.file, start, end),
        )
    }

    fn parse_atomic_pattern(&mut self) -> Pattern {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Underscore => {
                self.bump();
                Node::new(PatternKind::Wildcard, token.span)
            }
            TokenKind::Keyword(Keyword::True) => {
                self.bump();
                Node::new(PatternKind::Literal(PatternLiteral::Bool(true)), token.span)
            }
            TokenKind::Keyword(Keyword::False) => {
                self.bump();
                Node::new(
                    PatternKind::Literal(PatternLiteral::Bool(false)),
                    token.span,
                )
            }
            TokenKind::Minus
                if matches!(
                    self.nth(1).kind,
                    TokenKind::Integer(_) | TokenKind::Decimal(_)
                ) =>
            {
                let start = token.span.start;
                self.bump();
                let numeric = self.bump().clone();
                let literal = match numeric.kind {
                    TokenKind::Integer(raw) => PatternLiteral::Integer {
                        raw,
                        negative: true,
                    },
                    TokenKind::Decimal(raw) => PatternLiteral::Decimal {
                        raw,
                        negative: true,
                    },
                    _ => unreachable!("guarded numeric pattern"),
                };
                Node::new(
                    PatternKind::Literal(literal),
                    Span::new(self.identity.file, start, numeric.span.end),
                )
            }
            TokenKind::Integer(raw) => {
                let raw = raw.clone();
                self.bump();
                Node::new(
                    PatternKind::Literal(PatternLiteral::Integer {
                        raw,
                        negative: false,
                    }),
                    token.span,
                )
            }
            TokenKind::Decimal(raw) => {
                let raw = raw.clone();
                self.bump();
                Node::new(
                    PatternKind::Literal(PatternLiteral::Decimal {
                        raw,
                        negative: false,
                    }),
                    token.span,
                )
            }
            TokenKind::Text(value) => {
                let value = value.clone();
                let raw = token.lexeme.clone();
                self.bump();
                Node::new(
                    PatternKind::Literal(PatternLiteral::Text { raw, value }),
                    token.span,
                )
            }
            TokenKind::LParen => self.parse_parenthesized_pattern(),
            TokenKind::Identifier(_) => self.parse_named_pattern(),
            _ => {
                self.error_here(
                    ParseDiagnosticKind::InvalidPattern,
                    format!("expected pattern, found {}", token.kind.describe()),
                );
                if !self.at_simple(TokenKind::Eof) {
                    self.bump();
                }
                Node::new(PatternKind::Wildcard, token.span)
            }
        }
    }

    fn parse_parenthesized_pattern(&mut self) -> Pattern {
        let open = self.expect_simple(TokenKind::LParen, "pattern");
        if let Some(close) = self.eat_simple(TokenKind::RParen) {
            return Node::new(
                PatternKind::Literal(PatternLiteral::Unit),
                Span::new(self.identity.file, open.start, close.end),
            );
        }
        let first = self.parse_pattern();
        if self.eat_simple(TokenKind::Comma).is_none() {
            let close = self.expect_simple(TokenKind::RParen, "grouped pattern");
            return Node::new(
                PatternKind::Group(Box::new(first)),
                Span::new(self.identity.file, open.start, close.end),
            );
        }
        let mut patterns = vec![first];
        if self.at_simple(TokenKind::RParen) {
            self.error_here(
                ParseDiagnosticKind::InvalidPattern,
                "a tuple pattern requires at least two elements",
            );
        } else {
            patterns.push(self.parse_pattern());
            while self.eat_simple(TokenKind::Comma).is_some() && !self.at_simple(TokenKind::RParen)
            {
                patterns.push(self.parse_pattern());
            }
        }
        let close = self.expect_simple(TokenKind::RParen, "tuple pattern");
        Node::new(
            PatternKind::Tuple(patterns),
            Span::new(self.identity.file, open.start, close.end),
        )
    }

    fn parse_named_pattern(&mut self) -> Pattern {
        let name = self.parse_qualified_name();
        let start = name.span.start;
        let is_unqualified_lower = name.segments.len() == 1
            && is_lower_name(&name.segments[0].text)
            && !matches!(name.segments[0].text.as_str(), "None" | "Some");
        if is_unqualified_lower
            && !self.at_simple(TokenKind::LParen)
            && !self.at_simple(TokenKind::LBrace)
        {
            let binder = name.segments[0].clone();
            return Node::new(PatternKind::Binder(binder), name.span);
        }

        if self.eat_simple(TokenKind::LParen).is_some() {
            if name.segments.len() != 1 || name.segments[0].text != "Some" {
                self.error(
                    ParseDiagnosticKind::InvalidPattern,
                    "only the prelude `Some(pattern)` constructor is positional in core patterns",
                    name.span,
                );
            }
            let arguments = self.parse_pattern_list(TokenKind::RParen);
            let close = self.expect_simple(TokenKind::RParen, "constructor pattern");
            return Node::new(
                PatternKind::TupleConstructor {
                    constructor: name,
                    arguments,
                },
                Span::new(self.identity.file, start, close.end),
            );
        }

        if self.at_simple(TokenKind::LBrace) && self.looks_like_record(&name) {
            self.bump();
            let mut fields = Vec::new();
            let mut rest = false;
            while !self.at_simple(TokenKind::RBrace) && !self.at_simple(TokenKind::Eof) {
                if self.eat_simple(TokenKind::DotDot).is_some() {
                    rest = true;
                    if self.eat_simple(TokenKind::Comma).is_some()
                        && !self.at_simple(TokenKind::RBrace)
                    {
                        self.error_here(
                            ParseDiagnosticKind::InvalidPattern,
                            "pattern rest must be the final record-pattern entry",
                        );
                    }
                    break;
                }
                let field_start = self.current().span.start;
                let field_name = self.expect_record_label("record-pattern field");
                let pattern = if self.eat_simple(TokenKind::Colon).is_some() {
                    Some(self.parse_pattern())
                } else {
                    None
                };
                let end = pattern
                    .as_ref()
                    .map_or(field_name.span.end, |pattern| pattern.span.end);
                fields.push(FieldPattern {
                    name: field_name,
                    pattern,
                    span: Span::new(self.identity.file, field_start, end),
                });
                if self.eat_simple(TokenKind::Comma).is_none() {
                    if !self.at_simple(TokenKind::RBrace) {
                        self.error_here(
                            ParseDiagnosticKind::MissingToken,
                            "expected `,` between record-pattern fields",
                        );
                    }
                    break;
                }
            }
            let close = self.expect_simple(TokenKind::RBrace, "record pattern");
            return Node::new(
                PatternKind::Record {
                    constructor: name,
                    fields,
                    rest,
                },
                Span::new(self.identity.file, start, close.end),
            );
        }

        let span = name.span;
        Node::new(PatternKind::Constructor(name), span)
    }

    fn looks_like_record(&self, name: &QualifiedName) -> bool {
        let starts_upper = name
            .segments
            .first()
            .and_then(|segment| segment.text.chars().next())
            .is_some_and(|first| first.is_ascii_uppercase());
        if !starts_upper || !self.at_simple(TokenKind::LBrace) {
            return false;
        }
        if self.nth_is_simple(1, TokenKind::RBrace) || self.nth_is_simple(1, TokenKind::DotDot) {
            return true;
        }
        match self.tokens.get(self.cursor + 1) {
            Some(Token {
                kind: TokenKind::Identifier(field),
                ..
            }) if is_lower_name(field) => matches!(
                self.tokens.get(self.cursor + 2).map(|token| &token.kind),
                Some(TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace)
            ),
            Some(Token {
                kind: TokenKind::Keyword(_),
                ..
            }) => matches!(
                self.tokens.get(self.cursor + 2).map(|token| &token.kind),
                Some(TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace)
            ),
            _ => false,
        }
    }

    fn expression_terminator(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Semicolon
                | TokenKind::Comma
                | TokenKind::RBrace
                | TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::FatArrow
                | TokenKind::Eof
        )
    }

    fn synchronize_module(&mut self) {
        let mut depth = 0_u32;
        while !self.at_simple(TokenKind::Eof) {
            if depth == 0 && self.is_module_declaration_start() {
                return;
            }
            match self.current().kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1)
                }
                TokenKind::Semicolon if depth == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn synchronize_member(&mut self) {
        let mut depth = 0_u32;
        while !self.at_simple(TokenKind::Eof) {
            if depth == 0 && (self.at_simple(TokenKind::RBrace) || self.is_member_start()) {
                return;
            }
            match self.current().kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                TokenKind::Semicolon if depth == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn synchronize_match_arm(&mut self) {
        let mut depth = 0_u32;
        while !self.at_simple(TokenKind::Eof) {
            match self.current().kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace if depth == 0 => return,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1)
                }
                TokenKind::Comma if depth == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn is_module_declaration_start(&self) -> bool {
        if self.at_contextual("ui")
            || (self.at_keyword(Keyword::Pub)
                && matches!(
                    self.nth(1).kind,
                    TokenKind::Identifier(ref value) if value == "ui"
                ))
        {
            return true;
        }
        matches!(
            self.current().kind,
            TokenKind::Keyword(
                Keyword::Pub
                    | Keyword::Machine
                    | Keyword::Part
                    | Keyword::Struct
                    | Keyword::Enum
                    | Keyword::Key
                    | Keyword::Const
                    | Keyword::Fn
            )
        )
    }

    fn is_member_start(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Keyword(
                Keyword::Pub
                    | Keyword::Config
                    | Keyword::Require
                    | Keyword::Requires
                    | Keyword::Const
                    | Keyword::Fn
                    | Keyword::Part
                    | Keyword::Events
                    | Keyword::Commands
                    | Keyword::Port
                    | Keyword::Outcomes
                    | Keyword::State
                    | Keyword::Computed
                    | Keyword::Invariant
                    | Keyword::Observe
                    | Keyword::On
                    | Keyword::Update
                    | Keyword::Before
            )
        )
    }

    fn validate_identifier(
        &mut self,
        identifier: &Identifier,
        category: NameCategory,
        context: &str,
    ) {
        if !valid_name(&identifier.text, category) {
            self.error(
                ParseDiagnosticKind::InvalidName,
                format!(
                    "invalid {context} `{}`; expected {}",
                    identifier.text,
                    category_description(category)
                ),
                identifier.span,
            );
        }
    }

    fn expect_name(&mut self, category: NameCategory, context: &str) -> Identifier {
        let token = self.current().clone();
        if let TokenKind::Identifier(value) = token.kind {
            self.bump();
            let identifier = Identifier::new(value, token.span);
            self.validate_identifier(&identifier, category, context);
            identifier
        } else {
            self.error_here(
                ParseDiagnosticKind::MissingToken,
                format!("expected {context} ({})", category_description(category)),
            );
            if !self.at_simple(TokenKind::Eof) {
                self.bump();
            }
            Identifier::new("error", token.span)
        }
    }

    fn expect_member_name(&mut self) -> Identifier {
        if let TokenKind::Keyword(keyword) = self.current().kind {
            let token = self.bump().clone();
            return Identifier::new(keyword.spelling(), token.span);
        }
        self.expect_name(NameCategory::Declaration, "member name")
    }

    fn expect_record_label(&mut self, context: &str) -> Identifier {
        if let TokenKind::Keyword(keyword) = self.current().kind {
            let token = self.current().clone();
            if self.nth_is_simple(1, TokenKind::Colon) {
                self.bump();
                return Identifier::new(keyword.spelling(), token.span);
            }
            self.error_here(
                ParseDiagnosticKind::InvalidName,
                format!(
                    "keyword `{}` is only a contextual {context} when followed by `:`; keyword shorthand is not admitted",
                    keyword.spelling()
                ),
            );
            self.bump();
            return Identifier::new(keyword.spelling(), token.span);
        }
        self.expect_name(NameCategory::Lower, context)
    }

    fn at_identifier(&self) -> bool {
        matches!(self.current().kind, TokenKind::Identifier(_))
    }

    fn at_contextual(&self, expected: &str) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Identifier(ref value) if value == expected
        )
    }

    fn expect_contextual(&mut self, expected: &str, context: &str) -> Span {
        if self.at_contextual(expected) {
            self.bump().span
        } else {
            self.error_here(
                ParseDiagnosticKind::MissingToken,
                format!("expected contextual `{expected}` in {context}"),
            );
            Span::empty(self.identity.file, self.current().span.start)
        }
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        self.current().kind == TokenKind::Keyword(keyword)
    }

    fn nth_keyword(&self, offset: usize, keyword: Keyword) -> bool {
        self.nth(offset).kind == TokenKind::Keyword(keyword)
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> Option<Span> {
        if self.at_keyword(keyword) {
            Some(self.bump().span)
        } else {
            None
        }
    }

    fn expect_keyword(&mut self, keyword: Keyword, context: &str) -> Span {
        if let Some(span) = self.eat_keyword(keyword) {
            span
        } else {
            self.error_here(
                ParseDiagnosticKind::MissingToken,
                format!("expected `{}` in {context}", keyword.spelling()),
            );
            Span::empty(self.identity.file, self.current().span.start)
        }
    }

    fn at_simple(&self, expected: TokenKind) -> bool {
        discriminant(&self.current().kind) == discriminant(&expected)
    }

    fn nth_is_simple(&self, offset: usize, expected: TokenKind) -> bool {
        discriminant(&self.nth(offset).kind) == discriminant(&expected)
    }

    fn eat_simple(&mut self, expected: TokenKind) -> Option<Span> {
        if self.at_simple(expected) {
            Some(self.bump().span)
        } else {
            None
        }
    }

    fn expect_simple(&mut self, expected: TokenKind, context: &str) -> Span {
        if let Some(span) = self.eat_simple(expected.clone()) {
            span
        } else {
            self.error_here(
                ParseDiagnosticKind::MissingToken,
                format!("expected {} in {context}", expected.describe()),
            );
            Span::empty(self.identity.file, self.current().span.start)
        }
    }

    fn current(&self) -> &Token {
        self.nth(0)
    }

    fn nth(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.cursor + offset)
            .unwrap_or_else(|| self.tokens.last().expect("lexer always emits EOF"))
    }

    fn previous(&self) -> &Token {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .unwrap_or_else(|| self.tokens.first().expect("lexer always emits EOF"))
    }

    fn previous_end(&self) -> u32 {
        self.previous().span.end
    }

    fn bump(&mut self) -> &Token {
        let index = self.cursor;
        if !self.at_simple(TokenKind::Eof) {
            self.cursor += 1;
        }
        &self.tokens[index]
    }

    fn error_here(&mut self, kind: ParseDiagnosticKind, message: impl Into<String>) {
        self.error(kind, message, self.current().span);
    }

    fn error_previous(&mut self, kind: ParseDiagnosticKind, message: impl Into<String>) {
        self.error(kind, message, self.previous().span);
    }

    fn error(&mut self, kind: ParseDiagnosticKind, message: impl Into<String>, span: Span) {
        self.diagnostics.push(ParseDiagnostic {
            kind,
            message: message.into(),
            span,
        });
    }
}

pub(super) struct FragmentParse<T> {
    pub value: T,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub comments: Vec<Trivia>,
    /// Absolute UTF-8 byte immediately after the parsed value.
    pub consumed: u32,
}

pub(super) fn parse_expression_fragment(
    identity: &SourceIdentity,
    source: &str,
    base: u32,
) -> FragmentParse<Expression> {
    parse_expression_fragment_inner(identity, source, base, true)
}

pub(super) fn parse_expression_prefix(
    identity: &SourceIdentity,
    source: &str,
    base: u32,
) -> FragmentParse<Expression> {
    parse_expression_fragment_inner(identity, source, base, false)
}

fn parse_expression_fragment_inner(
    identity: &SourceIdentity,
    source: &str,
    base: u32,
    require_complete: bool,
) -> FragmentParse<Expression> {
    let lexical = lex_fragment(identity.file, source, base);
    let comments = fragment_comments(&lexical.tokens);
    let mut parser = Parser::new(identity.clone(), &lexical.tokens);
    let value = parser.parse_expression();
    let consumed = value.span.end;
    if require_complete && !parser.at_simple(TokenKind::Eof) {
        parser.error_here(
            ParseDiagnosticKind::UnexpectedToken,
            "unexpected token after complete UI expression",
        );
    }
    let mut diagnostics = parser.diagnostics;
    diagnostics.extend(
        lexical
            .diagnostics
            .into_iter()
            .filter(|diagnostic| require_complete || diagnostic.span.start < consumed)
            .map(Into::into),
    );
    FragmentParse {
        value,
        diagnostics,
        // Even a comment after the parsed event-input prefix must survive as
        // author-visible trivia. Retaining all comments makes formatting
        // refuse malformed mixed input/attribute source rather than erase it.
        comments,
        consumed,
    }
}

pub(super) fn parse_pattern_fragment(
    identity: &SourceIdentity,
    source: &str,
    base: u32,
) -> FragmentParse<Pattern> {
    let lexical = lex_fragment(identity.file, source, base);
    let comments = fragment_comments(&lexical.tokens);
    let mut parser = Parser::new(identity.clone(), &lexical.tokens);
    let value = parser.parse_pattern();
    let consumed = value.span.end;
    if !parser.at_simple(TokenKind::Eof) {
        parser.error_here(
            ParseDiagnosticKind::UnexpectedToken,
            "unexpected token after complete UI pattern",
        );
    }
    let mut diagnostics = parser.diagnostics;
    diagnostics.extend(lexical.diagnostics.into_iter().map(Into::into));
    FragmentParse {
        value,
        diagnostics,
        comments,
        consumed,
    }
}

fn fragment_comments(tokens: &[Token]) -> Vec<Trivia> {
    tokens
        .iter()
        .flat_map(|token| token.leading.iter())
        .filter(|trivia| {
            matches!(
                trivia.kind,
                TriviaKind::OrdinaryComment | TriviaKind::OuterDoc | TriviaKind::InnerDoc
            )
        })
        .cloned()
        .collect()
}

fn is_lower_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|next| next.is_ascii_lowercase() || next.is_ascii_digit() || next == '_')
}

fn is_upper_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|next| next.is_ascii_alphanumeric())
}

fn is_constant_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|next| next.is_ascii_uppercase() || next.is_ascii_digit() || next == '_')
}

fn valid_name(value: &str, category: NameCategory) -> bool {
    match category {
        NameCategory::Lower => is_lower_name(value),
        NameCategory::Upper => is_upper_name(value),
        NameCategory::Constant => is_constant_name(value),
        NameCategory::Declaration => {
            is_lower_name(value) || is_upper_name(value) || is_constant_name(value)
        }
    }
}

fn category_description(category: NameCategory) -> &'static str {
    match category {
        NameCategory::Lower => "a snake_case name",
        NameCategory::Upper => "an UpperCamelCase name",
        NameCategory::Constant => "a SCREAMING_SNAKE_CASE name",
        NameCategory::Declaration => "a declared name",
    }
}
