//! Recursive-descent parser for Uhura declarations, machines, expressions,
//! ports, and evidence.

use std::mem::discriminant;

use unicode_ident::{is_xid_continue, is_xid_start};

use super::ast::*;
use super::lexer::{LexDiagnostic, Token, TokenKind, lex, lex_fragment};
use super::ui::parse_ui_body;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseDiagnosticKind {
    Lexical,
    UnexpectedToken,
    MissingToken,
    InvalidDeclaration,
    InvalidType,
    InvalidPattern,
    InvalidExpression,
    InvalidUi,
    InvalidEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub kind: ParseDiagnosticKind,
    pub message: String,
    pub span: SourceSpan,
}

impl From<LexDiagnostic> for ParseDiagnostic {
    fn from(value: LexDiagnostic) -> Self {
        Self {
            kind: ParseDiagnosticKind::Lexical,
            message: value.message,
            span: value.span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Parse {
    pub module: Option<Module>,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub tokens: Vec<Token>,
}

impl Parse {
    pub fn is_ok(&self) -> bool {
        self.module.is_some() && self.diagnostics.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile<'a> {
    pub id: SourceId,
    pub text: &'a str,
}

impl<'a> SourceFile<'a> {
    pub fn new(file: uhura_base::FileId, path: impl Into<String>, text: &'a str) -> Self {
        Self {
            id: SourceId::new(file, path),
            text,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProjectParse {
    pub project: Project,
    pub diagnostics: Vec<ParseDiagnostic>,
}

pub fn parse(source_id: SourceId, text: &str) -> Parse {
    let lexical = lex(&source_id, text);
    let mut parser = Parser {
        source_id,
        source: text,
        tokens: lexical.tokens,
        cursor: 0,
        diagnostics: lexical.diagnostics.into_iter().map(Into::into).collect(),
    };
    let module = parser.parse_module();
    Parse {
        module,
        diagnostics: parser.diagnostics,
        tokens: parser.tokens,
    }
}

pub fn parse_project<'a>(files: impl IntoIterator<Item = SourceFile<'a>>) -> ProjectParse {
    let mut modules = Vec::new();
    let mut diagnostics = Vec::new();
    for file in files {
        let result = parse(file.id, file.text);
        diagnostics.extend(result.diagnostics);
        if let Some(module) = result.module {
            modules.push(module);
        }
    }
    ProjectParse {
        project: Project { modules },
        diagnostics,
    }
}

pub(crate) fn parse_expression_fragment(
    file: u32,
    source: &str,
    base: u32,
) -> (Expr, Vec<ParseDiagnostic>) {
    let lexical = lex_fragment(file, source, base);
    let mut parser = Parser {
        source_id: SourceId {
            file,
            path: "<ui-fragment>".into(),
        },
        source,
        tokens: lexical.tokens,
        cursor: 0,
        diagnostics: lexical.diagnostics.into_iter().map(Into::into).collect(),
    };
    let expression = parser.parse_expr();
    if !parser.at(TokenKind::Eof) {
        parser.error_here(
            ParseDiagnosticKind::InvalidExpression,
            "unexpected token after UI expression",
        );
    }
    (expression, parser.diagnostics)
}

pub(crate) fn parse_pattern_fragment(
    file: u32,
    source: &str,
    base: u32,
) -> (Pattern, Vec<ParseDiagnostic>) {
    let lexical = lex_fragment(file, source, base);
    let mut parser = Parser {
        source_id: SourceId {
            file,
            path: "<ui-pattern>".into(),
        },
        source,
        tokens: lexical.tokens,
        cursor: 0,
        diagnostics: lexical.diagnostics.into_iter().map(Into::into).collect(),
    };
    let pattern = parser.parse_pattern();
    if !parser.at(TokenKind::Eof) {
        parser.error_here(
            ParseDiagnosticKind::InvalidPattern,
            "unexpected token after UI pattern",
        );
    }
    (pattern, parser.diagnostics)
}

struct Parser<'a> {
    source_id: SourceId,
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: Vec<ParseDiagnostic>,
}

impl Parser<'_> {
    fn parse_module(&mut self) -> Option<Module> {
        let start = self.current().span.start;
        if !self.eat_word("language") {
            self.error_here(
                ParseDiagnosticKind::MissingToken,
                "Uhura source must begin with `language uhura 0.3`",
            );
            return None;
        }
        let language_name = self.expect_word_name("uhura");
        let (version, version_span) = match self.bump().clone() {
            Token {
                kind: TokenKind::Decimal(value),
                span,
                ..
            } => (value, span),
            token => {
                self.error(
                    ParseDiagnosticKind::MissingToken,
                    "expected Uhura language version `0.3`",
                    token.span,
                );
                ("0.3".into(), token.span)
            }
        };
        let language_end = version_span.end;
        if version != "0.3" {
            self.error(
                ParseDiagnosticKind::UnexpectedToken,
                format!("unsupported Uhura language version `{version}`; expected `0.3`"),
                version_span,
            );
        }
        let language = LanguageHeader {
            name: language_name,
            version,
            span: SourceSpan::new(self.source_id.file, start, language_end),
        };

        let module_start = self.current().span.start;
        self.expect_word("module");
        let path = self.parse_dot_path();
        self.expect(TokenKind::At, "`@` before module major");
        let (major, major_span) = self.expect_integer("positive module major");
        if major.starts_with('0') {
            self.error(
                ParseDiagnosticKind::UnexpectedToken,
                "module major must be a positive integer without leading zeroes",
                major_span,
            );
        }
        let identity = ModuleIdentity {
            path,
            major,
            span: SourceSpan::new(self.source_id.file, module_start, major_span.end),
        };

        let mut uses = Vec::new();
        while self.at_word("use") {
            let use_start = self.bump().span.start;
            let feature = self.expect_feature_name();
            uses.push(UseDecl {
                span: SourceSpan::new(self.source_id.file, use_start, feature.span.end),
                feature,
            });
        }

        let mut imports = Vec::new();
        while self.at_word("import") {
            imports.push(self.parse_import());
        }

        let mut declarations = Vec::new();
        while !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(declaration) = self.parse_declaration() {
                declarations.push(declaration);
            }
            if self.cursor == before {
                self.error_here(
                    ParseDiagnosticKind::InvalidDeclaration,
                    "expected an Uhura top-level declaration",
                );
                self.bump();
            }
        }

        let end = self.current().span.end;
        Some(Module {
            source_id: self.source_id.clone(),
            span: SourceSpan::new(self.source_id.file, start, end),
            language,
            identity,
            uses,
            imports,
            declarations,
            source: self.source.to_string(),
        })
    }

    fn parse_import(&mut self) -> ImportDecl {
        let start = self.bump().span.start;
        self.expect(TokenKind::LBrace, "`{` after `import`");
        let mut names = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            names.push(self.expect_binding_name("imported symbol"));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "`}` after imported symbols");
        self.expect_word("from");
        let token = self.bump().clone();
        let (target, target_span) = match token.kind {
            TokenKind::Text(value) => (value, token.span),
            _ => {
                self.error(
                    ParseDiagnosticKind::MissingToken,
                    "expected quoted logical module identity after `from`",
                    token.span,
                );
                (String::new(), token.span)
            }
        };
        let identity = self.parse_import_identity(&target, target_span);
        ImportDecl {
            names,
            target,
            identity,
            target_span,
            span: SourceSpan::new(self.source_id.file, start, token.span.end),
        }
    }

    fn parse_import_identity(&mut self, target: &str, span: SourceSpan) -> ModuleIdentity {
        let Some((path_text, major)) = target.rsplit_once('@') else {
            self.error(
                ParseDiagnosticKind::UnexpectedToken,
                "import target must end in `@<positive-major>`",
                span,
            );
            return ModuleIdentity {
                path: vec![Spanned::new("<error>".into(), span)],
                major: "0".into(),
                span,
            };
        };
        if major.is_empty()
            || major.starts_with('0')
            || !major.bytes().all(|value| value.is_ascii_digit())
        {
            self.error(
                ParseDiagnosticKind::UnexpectedToken,
                "import target major must be a positive integer without leading zeroes",
                span,
            );
        }
        let mut path = Vec::new();
        let mut byte_offset = 0usize;
        for part in path_text.split('.') {
            let part_start = span.start.saturating_add(1 + byte_offset as u32);
            let part_span = SourceSpan::new(span.file, part_start, part_start + part.len() as u32);
            if !valid_identifier(part) {
                self.error(
                    ParseDiagnosticKind::UnexpectedToken,
                    format!("invalid logical module component `{part}`"),
                    part_span,
                );
            }
            path.push(Spanned::new(part.to_string(), part_span));
            byte_offset += part.len() + 1;
        }
        if path.is_empty() {
            path.push(Spanned::new("<error>".into(), span));
        }
        ModuleIdentity {
            path,
            major: major.to_string(),
            span,
        }
    }

    fn parse_declaration(&mut self) -> Option<Declaration> {
        let start = self.current().span.start;
        let kind = if self.at_word("const") {
            DeclarationKind::Const(self.parse_const())
        } else if self.at_word("key") {
            DeclarationKind::Key(self.parse_key())
        } else if self.at_word("type") {
            DeclarationKind::Type(self.parse_type_decl())
        } else if self.at_word("fn") {
            DeclarationKind::Function(self.parse_function())
        } else if self.at_word("machine") {
            DeclarationKind::Machine(self.parse_machine())
        } else if self.at_word("ui") {
            DeclarationKind::Ui(self.parse_ui())
        } else if self.at_word("scenario") {
            DeclarationKind::Scenario(self.parse_scenario())
        } else if self.at_word("example") {
            DeclarationKind::Example(self.parse_evidence_alias(true))
        } else if self.at_word("checkpoint") {
            DeclarationKind::Checkpoint(self.parse_evidence_alias(false))
        } else {
            return None;
        };
        let end = self.previous_end();
        Some(Spanned::new(
            kind,
            SourceSpan::new(self.source_id.file, start, end),
        ))
    }

    fn parse_const(&mut self) -> ConstDecl {
        self.expect_word("const");
        let name = self.expect_binding_name("constant name");
        self.expect(TokenKind::Colon, "`:` after constant name");
        let ty = self.parse_type();
        self.expect(TokenKind::Eq, "`=` after constant type");
        let value = self.parse_expr();
        ConstDecl { name, ty, value }
    }

    fn parse_key(&mut self) -> KeyDecl {
        self.expect_word("key");
        let name = self.expect_binding_name("key name");
        self.expect_word("over");
        let over = self.parse_type();
        KeyDecl { name, over }
    }

    fn parse_type_decl(&mut self) -> TypeDecl {
        self.expect_word("type");
        let name = self.expect_binding_name("type name");
        let parameters = if self.eat(TokenKind::Less) {
            let values = self.parse_name_list(TokenKind::Greater);
            self.expect(TokenKind::Greater, "`>` after type parameters");
            values
        } else {
            Vec::new()
        };
        self.expect(TokenKind::Eq, "`=` after type name");
        let body = if self.at(TokenKind::LBrace) || self.looks_like_type_alias() {
            TypeBody::Alias(self.parse_type())
        } else {
            TypeBody::Sum(self.parse_closed_sum())
        };
        TypeDecl {
            name,
            parameters,
            body,
        }
    }

    fn looks_like_type_alias(&self) -> bool {
        // Phase-1 authorable type bodies are records or sums.  Keep this
        // narrow alias hook for a future qualified/generic alias without
        // misclassifying `type Phase = queued | running(...)`.
        matches!(self.current().kind, TokenKind::LParen)
            && self
                .find_matching(self.cursor, TokenKind::LParen, TokenKind::RParen)
                .is_some_and(|end| {
                    !matches!(
                        self.tokens.get(end + 1).map(|token| &token.kind),
                        Some(TokenKind::Pipe)
                    )
                })
    }

    fn parse_function(&mut self) -> FunctionDecl {
        self.expect_word("fn");
        let name = self.expect_binding_name("function name");
        let parameters = self.parse_parameters();
        self.expect(TokenKind::Arrow, "`->` after function parameters");
        let result = self.parse_type();
        self.expect(TokenKind::Eq, "`=` before function body");
        let body = self.parse_expr();
        FunctionDecl {
            name,
            parameters,
            result,
            body,
        }
    }

    fn parse_machine(&mut self) -> MachineDecl {
        self.expect_word("machine");
        let name = self.expect_binding_name("machine name");
        self.expect(TokenKind::LBrace, "`{` after machine name");
        let mut members = Vec::new();
        let mut latest_section = None;
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let before = self.cursor;
            let kind = self.parse_machine_member();
            if let Some(kind) = kind {
                let span = SourceSpan::new(self.source_id.file, start, self.previous_end());
                let (section, label) = machine_member_section(&kind);
                if latest_section.is_some_and(|latest| section < latest) {
                    self.error(
                        ParseDiagnosticKind::InvalidDeclaration,
                        format!(
                            "machine member `{label}` is out of source order; Uhura machine sections cannot move backwards"
                        ),
                        span,
                    );
                } else {
                    latest_section = Some(section);
                }
                members.push(Spanned::new(kind, span));
            }
            if before == self.cursor {
                self.error_here(
                    ParseDiagnosticKind::InvalidDeclaration,
                    "expected a machine member",
                );
                self.bump();
            }
        }
        self.expect(TokenKind::RBrace, "`}` after machine");
        MachineDecl { name, members }
    }

    fn parse_machine_member(&mut self) -> Option<MachineMemberKind> {
        Some(if self.at_word("const") {
            MachineMemberKind::Const(self.parse_const())
        } else if self.at_word("key") {
            MachineMemberKind::Key(self.parse_key())
        } else if self.at_word("type") {
            MachineMemberKind::Type(self.parse_type_decl())
        } else if self.at_word("port") {
            MachineMemberKind::Port(self.parse_port())
        } else if self.at_word("config") {
            self.bump();
            MachineMemberKind::Config(self.parse_field_block())
        } else if self.at_word("require") {
            self.bump();
            MachineMemberKind::Require(self.parse_expr())
        } else if self.at_word("input") {
            self.bump();
            self.expect(TokenKind::Eq, "`=` after `input`");
            MachineMemberKind::Input(self.parse_sum_domain())
        } else if self.at_word("command") {
            self.bump();
            self.expect(TokenKind::Eq, "`=` after `command`");
            MachineMemberKind::Command(self.parse_sum_domain())
        } else if self.at_word("outcome") {
            MachineMemberKind::Outcome(self.parse_outcome())
        } else if self.at_word("state") {
            MachineMemberKind::State(self.parse_state())
        } else if self.at_word("fn") {
            MachineMemberKind::Function(self.parse_function())
        } else if self.at_word("derive") {
            MachineMemberKind::Derive(self.parse_derive())
        } else if self.at_word("invariant") {
            MachineMemberKind::Invariant(self.parse_invariant())
        } else if self.at_word("observe") {
            MachineMemberKind::Observe(self.parse_observe())
        } else if self.at_word("transition") {
            MachineMemberKind::Transition(self.parse_transition())
        } else if self.at_word("on") {
            MachineMemberKind::Handler(self.parse_handler())
        } else if self.at_word("before") {
            self.bump();
            self.expect_word("commit");
            MachineMemberKind::BeforeCommit(self.parse_block())
        } else {
            return None;
        })
    }

    fn parse_port(&mut self) -> PortDecl {
        self.expect_word("port");
        let name = self.expect_binding_name("port name");
        self.expect(TokenKind::Colon, "`:` after port name");
        let contract = self.parse_type();
        let configuration = if self.eat(TokenKind::LParen) {
            let values = self.parse_expr_list(TokenKind::RParen);
            self.expect(TokenKind::RParen, "`)` after port configuration");
            values
        } else {
            Vec::new()
        };
        PortDecl {
            name,
            contract,
            configuration,
        }
    }

    fn parse_field_block(&mut self) -> FieldBlock {
        self.expect(TokenKind::LBrace, "`{` before fields");
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_binding_name("configuration field name");
            self.expect(TokenKind::Colon, "`:` after field name");
            let ty = self.parse_type();
            fields.push(TypeField {
                name,
                ty,
                span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
            });
        }
        self.expect(TokenKind::RBrace, "`}` after fields");
        FieldBlock { fields }
    }

    fn parse_sum_domain(&mut self) -> SumDomain {
        if self.at_word("Never") {
            SumDomain::Never(self.expect_name("`Never`"))
        } else {
            SumDomain::Sum(self.parse_closed_sum())
        }
    }

    fn parse_outcome(&mut self) -> OutcomeDecl {
        self.expect_word("outcome");
        self.expect(TokenKind::Eq, "`=` after `outcome`");
        let leading_pipe = self.eat(TokenKind::Pipe);
        let mut variants = Vec::new();
        loop {
            let start = self.current().span.start;
            let variant = self.parse_variant();
            let policy_token = self.bump().clone();
            let policy = match &policy_token.kind {
                TokenKind::Ident(value) if value == "commit" => OutcomePolicy::Commit,
                TokenKind::Ident(value) if value == "abort" => OutcomePolicy::Abort,
                _ => {
                    self.error(
                        ParseDiagnosticKind::MissingToken,
                        "expected `commit` or `abort` after outcome variant",
                        policy_token.span,
                    );
                    OutcomePolicy::Abort
                }
            };
            variants.push(OutcomeVariant {
                variant,
                policy: Spanned::new(policy, policy_token.span),
                span: SourceSpan::new(self.source_id.file, start, policy_token.span.end),
            });
            if !self.eat(TokenKind::Pipe) {
                break;
            }
        }
        OutcomeDecl {
            variants,
            leading_pipe,
        }
    }

    fn parse_state(&mut self) -> StateDecl {
        self.expect_word("state");
        self.expect(TokenKind::LBrace, "`{` after `state`");
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_binding_name("state field name");
            self.expect(TokenKind::Colon, "`:` after state field");
            let ty = self.parse_type();
            self.expect(TokenKind::Eq, "`=` after state field type");
            let value = self.parse_expr();
            fields.push(InitializedField {
                name,
                ty,
                value,
                span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
            });
        }
        self.expect(TokenKind::RBrace, "`}` after state");
        StateDecl { fields }
    }

    fn parse_derive(&mut self) -> DeriveDecl {
        self.expect_word("derive");
        let name = self.expect_binding_name("derive name");
        self.expect(TokenKind::Colon, "`:` after derive name");
        let ty = self.parse_type();
        self.expect(TokenKind::Eq, "`=` after derive type");
        let value = self.parse_expr();
        DeriveDecl {
            name,
            ty: Some(ty),
            value,
        }
    }

    fn parse_invariant(&mut self) -> InvariantDecl {
        self.expect_word("invariant");
        if self.eat(TokenKind::LBrace) {
            let mut expressions = Vec::new();
            while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                expressions.push(self.parse_expr());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace, "`}` after invariant expressions");
            InvariantDecl {
                expressions,
                braced: true,
            }
        } else {
            InvariantDecl {
                expressions: vec![self.parse_expr()],
                braced: false,
            }
        }
    }

    fn parse_observe(&mut self) -> ObserveDecl {
        self.expect_word("observe");
        self.expect(TokenKind::LBrace, "`{` after `observe`");
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_binding_name("observation field name");
            let ty = if self.eat(TokenKind::Colon) {
                Some(self.parse_type())
            } else {
                None
            };
            self.expect(TokenKind::Eq, "`=` in observation field");
            let value = self.parse_expr();
            fields.push(ObserveField {
                name,
                ty,
                value,
                span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
            });
        }
        self.expect(TokenKind::RBrace, "`}` after `observe`");
        ObserveDecl { fields }
    }

    fn parse_transition(&mut self) -> TransitionDecl {
        self.expect_word("transition");
        let name = self.expect_binding_name("transition name");
        let parameters = self.parse_parameters();
        let body = self.parse_block();
        TransitionDecl {
            name,
            parameters,
            body,
        }
    }

    fn parse_handler(&mut self) -> HandlerDecl {
        self.expect_word("on");
        let input = self.parse_pattern();
        let body = if self.eat(TokenKind::Eq) {
            HandlerBody::Delegate(self.parse_expr())
        } else {
            HandlerBody::Block(self.parse_block())
        };
        HandlerDecl { input, body }
    }

    fn parse_parameters(&mut self) -> Vec<Parameter> {
        self.expect(TokenKind::LParen, "`(` before parameters");
        let mut values = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let name = self.expect_binding_name("parameter name");
            self.expect(TokenKind::Colon, "`:` after parameter name");
            let ty = self.parse_type();
            values.push(Parameter {
                name,
                ty,
                span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)` after parameters");
        values
    }

    fn parse_type(&mut self) -> TypeExpr {
        let start = self.current().span.start;
        let kind = match self.current().kind.clone() {
            TokenKind::LBrace => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                    let field_start = self.current().span.start;
                    let name = self.expect_binding_name("record type field");
                    self.expect(TokenKind::Colon, "`:` after record type field");
                    let ty = self.parse_type();
                    fields.push(TypeField {
                        name,
                        ty,
                        span: SourceSpan::new(
                            self.source_id.file,
                            field_start,
                            self.previous_end(),
                        ),
                    });
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace, "`}` after record type");
                TypeExprKind::Record(fields)
            }
            TokenKind::LParen => {
                self.bump();
                let mut values = Vec::new();
                values.push(self.parse_type());
                self.expect(TokenKind::Comma, "`,` in tuple type");
                while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                    values.push(self.parse_type());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "`)` after tuple type");
                TypeExprKind::Tuple(values)
            }
            TokenKind::Ident(_) => {
                let path = self.parse_dot_path();
                let arguments = if self.eat(TokenKind::Less) {
                    let values = self.parse_type_list(TokenKind::Greater);
                    self.expect(TokenKind::Greater, "`>` after type arguments");
                    values
                } else {
                    Vec::new()
                };
                TypeExprKind::Named { path, arguments }
            }
            _ => {
                let span = self.current().span;
                self.error(
                    ParseDiagnosticKind::InvalidType,
                    format!("expected a type, found {}", self.current().kind.describe()),
                    span,
                );
                self.bump();
                TypeExprKind::Named {
                    path: vec![Spanned::new("<error>".into(), span)],
                    arguments: Vec::new(),
                }
            }
        };
        Spanned::new(
            kind,
            SourceSpan::new(self.source_id.file, start, self.previous_end()),
        )
    }

    fn parse_type_list(&mut self, close: TokenKind) -> Vec<TypeExpr> {
        let mut values = Vec::new();
        while !self.at(close.clone()) && !self.at(TokenKind::Eof) {
            values.push(self.parse_type());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        values
    }

    fn parse_closed_sum(&mut self) -> ClosedSum {
        let start = self.current().span.start;
        let leading_pipe = self.eat(TokenKind::Pipe);
        let mut variants = Vec::new();
        loop {
            variants.push(self.parse_variant());
            if !self.eat(TokenKind::Pipe) {
                break;
            }
        }
        ClosedSum {
            variants,
            leading_pipe,
            span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
        }
    }

    fn parse_variant(&mut self) -> Variant {
        let start = self.current().span.start;
        let name = self.expect_binding_name("variant name");
        let payload = if self.eat(TokenKind::LParen) {
            if self.eat(TokenKind::RParen) {
                VariantPayload::Positional(Vec::new())
            } else if self.current_is_named_type_field() {
                let mut fields = Vec::new();
                loop {
                    let field_start = self.current().span.start;
                    let field_name = self.expect_binding_name("variant field name");
                    self.expect(TokenKind::Colon, "`:` after variant field");
                    let ty = self.parse_type();
                    fields.push(TypeField {
                        name: field_name,
                        ty,
                        span: SourceSpan::new(
                            self.source_id.file,
                            field_start,
                            self.previous_end(),
                        ),
                    });
                    if !self.eat(TokenKind::Comma) || self.at(TokenKind::RParen) {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "`)` after variant fields");
                VariantPayload::Named(fields)
            } else {
                let values = self.parse_type_list(TokenKind::RParen);
                self.expect(TokenKind::RParen, "`)` after variant payload");
                VariantPayload::Positional(values)
            }
        } else {
            VariantPayload::Unit
        };
        Variant {
            name,
            payload,
            span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
        }
    }

    fn current_is_named_type_field(&self) -> bool {
        matches!(self.current().kind, TokenKind::Ident(_))
            && matches!(
                self.tokens.get(self.cursor + 1).map(|token| &token.kind),
                Some(TokenKind::Colon)
            )
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_lambda()
    }

    fn parse_lambda(&mut self) -> Expr {
        if self.lambda_ahead() {
            let start = self.current().span.start;
            let parameters = if self.eat(TokenKind::LParen) {
                let open = self.tokens[self.cursor - 1].span;
                let mut values = Vec::new();
                while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                    values.push(self.parse_pattern());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.expect(TokenKind::RParen, "`)` after lambda parameters");
                if values.len() < 2 {
                    self.error(
                        ParseDiagnosticKind::InvalidPattern,
                        "parenthesized lambda parameters require at least two bindings",
                        open.to(close),
                    );
                }
                values
            } else {
                vec![self.parse_atomic_pattern()]
            };
            for parameter in &parameters {
                self.validate_lambda_parameter(parameter);
            }
            self.expect(TokenKind::FatArrow, "`=>` after lambda parameters");
            let body = self.parse_expr();
            let end = body.span.end;
            return Spanned::new(
                ExprKind::Lambda {
                    parameters,
                    body: Box::new(body),
                },
                SourceSpan::new(self.source_id.file, start, end),
            );
        }
        self.parse_binary(1)
    }

    fn lambda_ahead(&self) -> bool {
        if matches!(self.current().kind, TokenKind::Ident(_)) {
            return matches!(
                self.tokens.get(self.cursor + 1).map(|token| &token.kind),
                Some(TokenKind::FatArrow)
            );
        }
        if self.at(TokenKind::LParen) {
            return self
                .find_matching(self.cursor, TokenKind::LParen, TokenKind::RParen)
                .is_some_and(|end| {
                    matches!(
                        self.tokens.get(end + 1).map(|token| &token.kind),
                        Some(TokenKind::FatArrow)
                    )
                });
        }
        false
    }

    fn parse_binary(&mut self, minimum_precedence: u8) -> Expr {
        let mut left = self.parse_unary();
        let mut saw_equality = false;
        let mut saw_relational = false;
        loop {
            if self.at_word("is") && 3 >= minimum_precedence {
                let op = self.bump().span;
                if saw_equality {
                    self.error(
                        ParseDiagnosticKind::InvalidExpression,
                        "equality and `is` comparisons do not chain",
                        op,
                    );
                }
                saw_equality = true;
                let pattern = self.parse_pattern();
                let span = left.span.to(pattern.span).to(op);
                left = Spanned::new(
                    ExprKind::Is {
                        value: Box::new(left),
                        pattern,
                    },
                    span,
                );
                continue;
            }

            let Some((precedence, op)) = self.current_binary_op() else {
                break;
            };
            if precedence < minimum_precedence {
                break;
            }
            let op_span = self.bump().span;
            match precedence {
                3 => {
                    if saw_equality {
                        self.error(
                            ParseDiagnosticKind::InvalidExpression,
                            "equality and `is` comparisons do not chain",
                            op_span,
                        );
                    }
                    saw_equality = true;
                }
                4 => {
                    if saw_relational {
                        self.error(
                            ParseDiagnosticKind::InvalidExpression,
                            "relational comparisons do not chain",
                            op_span,
                        );
                    }
                    saw_relational = true;
                }
                _ => {}
            }
            let right = self.parse_binary(precedence + 1);
            let span = left.span.to(right.span);
            left = Spanned::new(
                ExprKind::Binary {
                    left: Box::new(left),
                    op: Spanned::new(op, op_span),
                    right: Box::new(right),
                },
                span,
            );
        }
        left
    }

    fn current_binary_op(&self) -> Option<(u8, BinaryOp)> {
        match &self.current().kind {
            TokenKind::Ident(value) if value == "or" => Some((1, BinaryOp::Or)),
            TokenKind::Ident(value) if value == "and" => Some((2, BinaryOp::And)),
            TokenKind::EqEq => Some((3, BinaryOp::Equal)),
            TokenKind::NotEqual => Some((3, BinaryOp::NotEqual)),
            TokenKind::Less => Some((4, BinaryOp::Less)),
            TokenKind::LessEqual => Some((4, BinaryOp::LessEqual)),
            TokenKind::Greater => Some((4, BinaryOp::Greater)),
            TokenKind::GreaterEqual => Some((4, BinaryOp::GreaterEqual)),
            TokenKind::Plus => Some((5, BinaryOp::Add)),
            TokenKind::Minus => Some((5, BinaryOp::Subtract)),
            TokenKind::Star => Some((6, BinaryOp::Multiply)),
            _ => None,
        }
    }

    fn parse_unary(&mut self) -> Expr {
        let start = self.current().span.start;
        let operation = if self.at_word("not") {
            Some((UnaryOp::Not, self.bump().span))
        } else if self.at(TokenKind::Minus) {
            Some((UnaryOp::Negate, self.bump().span))
        } else {
            None
        };
        if let Some((operation, operation_span)) = operation {
            let operand = self.parse_unary();
            let end = operand.span.end;
            Spanned::new(
                ExprKind::Unary {
                    op: Spanned::new(operation, operation_span),
                    operand: Box::new(operand),
                },
                SourceSpan::new(self.source_id.file, start, end),
            )
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut value = self.parse_primary();
        let mut saw_update = false;
        let mut diagnosed_post_update_suffix = false;
        loop {
            if self.eat(TokenKind::LParen) {
                if saw_update && !diagnosed_post_update_suffix {
                    self.error(
                        ParseDiagnosticKind::InvalidExpression,
                        "a record update must be parenthesized before applying a postfix suffix",
                        self.tokens[self.cursor - 1].span,
                    );
                    diagnosed_post_update_suffix = true;
                }
                // Match arms have no comma terminator.  In
                // `current == expected` followed by `(next, pattern) =>`,
                // the next arm's tuple would otherwise be consumed as a
                // call on `expected`.  A parenthesized group immediately
                // followed by `=>` is unambiguously the next arm pattern.
                let open_index = self.cursor.saturating_sub(1);
                if self
                    .find_matching(open_index, TokenKind::LParen, TokenKind::RParen)
                    .is_some_and(|end| {
                        matches!(
                            self.tokens.get(end + 1).map(|token| &token.kind),
                            Some(TokenKind::FatArrow)
                        )
                    })
                {
                    self.cursor = open_index;
                    break;
                }
                let arguments = self.parse_expr_list(TokenKind::RParen);
                let close = self.expect(TokenKind::RParen, "`)` after call arguments");
                let span = value.span.to(close);
                value = Spanned::new(
                    ExprKind::Call {
                        callee: Box::new(value),
                        arguments,
                    },
                    span,
                );
            } else if self.eat(TokenKind::Dot) {
                if saw_update && !diagnosed_post_update_suffix {
                    self.error(
                        ParseDiagnosticKind::InvalidExpression,
                        "a record update must be parenthesized before applying a postfix suffix",
                        self.tokens[self.cursor - 1].span,
                    );
                    diagnosed_post_update_suffix = true;
                }
                let member = self.expect_name("member name");
                let span = value.span.to(member.span);
                value = Spanned::new(
                    ExprKind::Member {
                        receiver: Box::new(value),
                        member,
                    },
                    span,
                );
            } else if self.eat(TokenKind::LBracket) {
                if saw_update && !diagnosed_post_update_suffix {
                    self.error(
                        ParseDiagnosticKind::InvalidExpression,
                        "a record update must be parenthesized before applying a postfix suffix",
                        self.tokens[self.cursor - 1].span,
                    );
                    diagnosed_post_update_suffix = true;
                }
                let index = self.parse_expr();
                let close = self.expect(TokenKind::RBracket, "`]` after index");
                let span = value.span.to(close);
                value = Spanned::new(
                    ExprKind::Index {
                        receiver: Box::new(value),
                        index: Box::new(index),
                    },
                    span,
                );
            } else if self.at_word("with") {
                let with_span = self.bump().span;
                if saw_update {
                    self.error(
                        ParseDiagnosticKind::InvalidExpression,
                        "record updates permit only one `with` clause",
                        with_span,
                    );
                }
                let fields = self.parse_record_entries();
                let span =
                    SourceSpan::new(self.source_id.file, value.span.start, self.previous_end());
                value = Spanned::new(
                    ExprKind::Update {
                        base: Box::new(value),
                        fields,
                    },
                    span,
                );
                saw_update = true;
            } else {
                break;
            }
        }
        value
    }

    fn parse_primary(&mut self) -> Expr {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Integer(value) => {
                self.bump();
                Spanned::new(ExprKind::Integer(value), token.span)
            }
            TokenKind::Decimal(value) => {
                self.bump();
                Spanned::new(ExprKind::Decimal(value), token.span)
            }
            TokenKind::Text(value) => {
                self.bump();
                Spanned::new(ExprKind::Text(value), token.span)
            }
            TokenKind::Ident(ref value) if value == "true" || value == "false" => {
                let value = value == "true";
                self.bump();
                Spanned::new(ExprKind::Bool(value), token.span)
            }
            TokenKind::Ident(ref value) if value == "if" => self.parse_if(),
            TokenKind::Ident(ref value) if value == "match" => self.parse_match(),
            TokenKind::Ident(ref value) if value == "collect" => self.parse_collect(),
            TokenKind::Ident(ref value)
                if value == "Set"
                    && matches!(
                        self.tokens.get(self.cursor + 1).map(|token| &token.kind),
                        Some(TokenKind::LBrace)
                    ) =>
            {
                self.parse_set_comprehension()
            }
            TokenKind::Ident(ref value) if value == "finish" => {
                let start = self.bump().span.start;
                let outcome = self.parse_expr();
                let end = outcome.span.end;
                Spanned::new(
                    ExprKind::Finish(Box::new(outcome)),
                    SourceSpan::new(self.source_id.file, start, end),
                )
            }
            TokenKind::Ident(ref value) if value == "unreachable" => {
                self.bump();
                Spanned::new(ExprKind::Unreachable, token.span)
            }
            TokenKind::Ident(_) => {
                let name = self.expect_name("name");
                let span = name.span;
                Spanned::new(ExprKind::Name(name), span)
            }
            TokenKind::LParen => self.parse_tuple_or_group(),
            TokenKind::LBracket => {
                let start = self.bump().span.start;
                let values = self.parse_expr_list(TokenKind::RBracket);
                let close = self.expect(TokenKind::RBracket, "`]` after sequence");
                Spanned::new(
                    ExprKind::Sequence(values),
                    SourceSpan::new(self.source_id.file, start, close.end),
                )
            }
            TokenKind::LBrace if self.looks_like_record_literal() => {
                let start = self.current().span.start;
                let entries = self.parse_record_entries();
                Spanned::new(
                    ExprKind::Record(entries),
                    SourceSpan::new(self.source_id.file, start, self.previous_end()),
                )
            }
            TokenKind::LBrace => {
                let block = self.parse_block();
                let span = block.span;
                Spanned::new(ExprKind::Block(block), span)
            }
            _ => {
                self.error(
                    ParseDiagnosticKind::InvalidExpression,
                    format!("expected an expression, found {}", token.kind.describe()),
                    token.span,
                );
                self.bump();
                Spanned::new(ExprKind::Error, token.span)
            }
        }
    }

    fn parse_tuple_or_group(&mut self) -> Expr {
        let open = self.bump().span;
        let start = open.start;
        if self.eat(TokenKind::RParen) {
            let close = self.tokens[self.cursor - 1].span;
            self.error(
                ParseDiagnosticKind::InvalidExpression,
                "Uhura has no unit tuple expression; use the selected `Unit` value form",
                open.to(close),
            );
            return Spanned::new(
                ExprKind::Tuple(Vec::new()),
                SourceSpan::new(self.source_id.file, start, self.previous_end()),
            );
        }
        let first = self.parse_expr();
        if !self.at(TokenKind::Comma) {
            let close = self.expect(TokenKind::RParen, "`)` after grouped expression");
            return Spanned::new(
                first.value,
                SourceSpan::new(self.source_id.file, start, close.end),
            );
        }
        let comma = self.bump().span;
        let mut values = vec![first];
        if self.at(TokenKind::RParen) {
            self.error(
                ParseDiagnosticKind::InvalidExpression,
                "Uhura tuple expressions require at least two elements",
                comma.to(self.current().span),
            );
        }
        while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
            values.push(self.parse_expr());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let close = self.expect(TokenKind::RParen, "`)` after tuple");
        Spanned::new(
            ExprKind::Tuple(values),
            SourceSpan::new(self.source_id.file, start, close.end),
        )
    }

    fn parse_if(&mut self) -> Expr {
        let start = self.bump().span.start;
        let condition = self.parse_expr();
        let then_branch = if self.eat_word("then") {
            self.parse_expr()
        } else {
            let block = self.parse_block();
            let span = block.span;
            Spanned::new(ExprKind::Block(block), span)
        };
        let else_branch = if self.eat_word("else") {
            Some(Box::new(if self.at_word("if") {
                self.parse_if()
            } else {
                self.parse_expr()
            }))
        } else {
            None
        };
        let end = else_branch
            .as_ref()
            .map_or(then_branch.span.end, |value| value.span.end);
        Spanned::new(
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            SourceSpan::new(self.source_id.file, start, end),
        )
    }

    fn parse_match(&mut self) -> Expr {
        let start = self.bump().span.start;
        let subject = self.parse_expr();
        self.expect(TokenKind::LBrace, "`{` after match subject");
        let mut arms = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let arm_start = self.current().span.start;
            let pattern = self.parse_pattern();
            self.expect(TokenKind::FatArrow, "`=>` after match pattern");
            let body = self.parse_expr();
            arms.push(MatchArm {
                pattern,
                body,
                span: SourceSpan::new(self.source_id.file, arm_start, self.previous_end()),
            });
        }
        let close = self.expect(TokenKind::RBrace, "`}` after match arms");
        Spanned::new(
            ExprKind::Match {
                subject: Box::new(subject),
                arms,
            },
            SourceSpan::new(self.source_id.file, start, close.end),
        )
    }

    fn parse_collect(&mut self) -> Expr {
        let start = self.bump().span.start;
        self.expect(TokenKind::LBracket, "`[` after `collect`");
        let mut clauses = Vec::new();
        while !self.at(TokenKind::RBracket) && !self.at(TokenKind::Eof) {
            let clause_start = self.current().span.start;
            self.expect_word("when");
            let condition = self.parse_expr();
            self.expect(TokenKind::FatArrow, "`=>` in collect clause");
            let value = self.parse_expr();
            clauses.push(CollectClause {
                condition,
                value,
                span: SourceSpan::new(self.source_id.file, clause_start, self.previous_end()),
            });
        }
        let close = self.expect(TokenKind::RBracket, "`]` after collect clauses");
        Spanned::new(
            ExprKind::Collect(clauses),
            SourceSpan::new(self.source_id.file, start, close.end),
        )
    }

    fn parse_set_comprehension(&mut self) -> Expr {
        let start = self.bump().span.start;
        self.expect(TokenKind::LBrace, "`{` after `Set`");
        self.expect_word("for");
        let binding = self.parse_pattern();
        self.expect_word("in");
        let source = self.parse_expr();
        let mut filters = Vec::new();
        while self.eat_word("when") {
            filters.push(self.parse_expr());
        }
        self.expect_word("yield");
        let value = self.parse_expr();
        let close = self.expect(TokenKind::RBrace, "`}` after set comprehension");
        Spanned::new(
            ExprKind::SetComprehension {
                binding,
                source: Box::new(source),
                filters,
                value: Box::new(value),
            },
            SourceSpan::new(self.source_id.file, start, close.end),
        )
    }

    fn parse_block(&mut self) -> Block {
        let open = self.expect(TokenKind::LBrace, "`{` before block");
        let mut statements = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            statements.push(self.parse_statement());
            if before == self.cursor {
                self.bump();
            }
        }
        let close = self.expect(TokenKind::RBrace, "`}` after block");
        Block {
            statements,
            span: open.to(close),
        }
    }

    fn parse_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        let value = if self.eat_word("let") {
            let name = self.expect_binding_name("local binding");
            let ty = if self.eat(TokenKind::Colon) {
                Some(self.parse_type())
            } else {
                None
            };
            self.expect(TokenKind::Eq, "`=` in local binding");
            StatementKind::Let {
                name,
                ty,
                value: self.parse_expr(),
            }
        } else if self.eat_word("set") {
            let target = self.expect_name("state field");
            self.expect(TokenKind::Eq, "`=` in state update");
            StatementKind::Set {
                target,
                value: self.parse_expr(),
            }
        } else if self.eat_word("emit") {
            StatementKind::Emit(self.parse_expr())
        } else if self.eat_word("while") {
            let condition = self.parse_expr();
            self.expect_word("decreases");
            let decreases = self.parse_expr();
            let body = self.parse_block();
            StatementKind::While {
                condition,
                decreases,
                body,
            }
        } else {
            StatementKind::Expr(self.parse_expr())
        };
        Spanned::new(
            value,
            SourceSpan::new(self.source_id.file, start, self.previous_end()),
        )
    }

    fn looks_like_record_literal(&self) -> bool {
        if !self.at(TokenKind::LBrace) {
            return false;
        }
        // Speculatively parse exactly the first braces item as an expression.
        // A following top-level `:` makes it a record/map entry.  This admits
        // every grammar-valid compile-time key expression without mistaking a
        // later `let name: Type` inside a value/reaction block for an entry.
        let mut probe = Parser {
            source_id: self.source_id.clone(),
            source: self.source,
            tokens: self.tokens.clone(),
            cursor: self.cursor + 1,
            diagnostics: Vec::new(),
        };
        probe.parse_expr();
        probe.diagnostics.is_empty() && probe.at(TokenKind::Colon)
    }

    fn parse_record_entries(&mut self) -> Vec<RecordEntry> {
        self.expect(TokenKind::LBrace, "`{` before record");
        let mut entries = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let key = self.parse_expr();
            self.expect(TokenKind::Colon, "`:` in record entry");
            let value = self.parse_expr();
            entries.push(RecordEntry {
                key,
                value,
                span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "`}` after record");
        entries
    }

    fn parse_expr_list(&mut self, close: TokenKind) -> Vec<Expr> {
        let mut values = Vec::new();
        while !self.at(close.clone()) && !self.at(TokenKind::Eof) {
            values.push(self.parse_expr());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        values
    }

    fn parse_pattern(&mut self) -> Pattern {
        let first = self.parse_atomic_pattern();
        if !self.eat(TokenKind::Pipe) {
            return first;
        }
        let start = first.span.start;
        let mut alternatives = vec![first];
        loop {
            alternatives.push(self.parse_atomic_pattern());
            if !self.eat(TokenKind::Pipe) {
                break;
            }
        }
        let end = alternatives.last().map_or(start, |value| value.span.end);
        Spanned::new(
            PatternKind::Alternative(alternatives),
            SourceSpan::new(self.source_id.file, start, end),
        )
    }

    fn parse_atomic_pattern(&mut self) -> Pattern {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Ident(ref value) if value == "_" => {
                self.bump();
                Spanned::new(PatternKind::Wildcard, token.span)
            }
            TokenKind::Ellipsis => {
                self.error(
                    ParseDiagnosticKind::InvalidPattern,
                    "`...` is valid only as the final openness marker inside a record pattern",
                    token.span,
                );
                self.bump();
                Spanned::new(PatternKind::Error, token.span)
            }
            TokenKind::Minus => {
                let start = self.bump().span.start;
                let numeric = self.bump().clone();
                let kind = match numeric.kind {
                    TokenKind::Integer(value) => PatternKind::Integer(format!("-{value}")),
                    TokenKind::Decimal(value) => PatternKind::Decimal(format!("-{value}")),
                    _ => {
                        self.error(
                            ParseDiagnosticKind::InvalidPattern,
                            "expected a number after `-` in pattern",
                            numeric.span,
                        );
                        PatternKind::Error
                    }
                };
                Spanned::new(
                    kind,
                    SourceSpan::new(self.source_id.file, start, numeric.span.end),
                )
            }
            TokenKind::Integer(value) => {
                self.bump();
                Spanned::new(PatternKind::Integer(value), token.span)
            }
            TokenKind::Decimal(value) => {
                self.bump();
                Spanned::new(PatternKind::Decimal(value), token.span)
            }
            TokenKind::Text(value) => {
                self.bump();
                Spanned::new(PatternKind::Text(value), token.span)
            }
            TokenKind::Ident(ref value) if value == "true" || value == "false" => {
                let value = value == "true";
                self.bump();
                Spanned::new(PatternKind::Bool(value), token.span)
            }
            TokenKind::Ident(ref value) if is_hard_reserved_word(value) => {
                self.error(
                    ParseDiagnosticKind::InvalidPattern,
                    format!("`{value}` is a reserved Uhura word, not a pattern identifier"),
                    token.span,
                );
                self.bump();
                Spanned::new(PatternKind::Error, token.span)
            }
            TokenKind::Ident(_) => {
                let start = token.span.start;
                let path = self.parse_dot_path();
                if self.eat(TokenKind::LParen) {
                    let mut arguments = Vec::new();
                    while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                        arguments.push(self.parse_pattern());
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    let close = self.expect(TokenKind::RParen, "`)` after constructor pattern");
                    Spanned::new(
                        PatternKind::Constructor { path, arguments },
                        SourceSpan::new(self.source_id.file, start, close.end),
                    )
                } else if path.len() > 1 {
                    let end = path.last().map_or(start, |value| value.span.end);
                    Spanned::new(
                        PatternKind::Constructor {
                            path,
                            arguments: Vec::new(),
                        },
                        SourceSpan::new(self.source_id.file, start, end),
                    )
                } else {
                    let name = path
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| Spanned::new("<error>".into(), token.span));
                    let span = name.span;
                    Spanned::new(PatternKind::Name(name), span)
                }
            }
            TokenKind::LParen => {
                let start = self.bump().span.start;
                let mut values = Vec::new();
                while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                    values.push(self.parse_pattern());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.expect(TokenKind::RParen, "`)` after tuple pattern");
                Spanned::new(
                    PatternKind::Tuple(values),
                    SourceSpan::new(self.source_id.file, start, close.end),
                )
            }
            TokenKind::LBrace => {
                let start = self.bump().span.start;
                let mut fields = Vec::new();
                let mut open = false;
                while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                    if self.eat(TokenKind::Ellipsis) {
                        open = true;
                        self.eat(TokenKind::Comma);
                        break;
                    }
                    let field_start = self.current().span.start;
                    let name = self.expect_name("record pattern field");
                    self.expect(TokenKind::Colon, "`:` after record pattern field");
                    let pattern = self.parse_pattern();
                    fields.push(RecordPatternField {
                        name,
                        pattern,
                        span: SourceSpan::new(
                            self.source_id.file,
                            field_start,
                            self.previous_end(),
                        ),
                    });
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.expect(TokenKind::RBrace, "`}` after record pattern");
                Spanned::new(
                    PatternKind::Record { fields, open },
                    SourceSpan::new(self.source_id.file, start, close.end),
                )
            }
            _ => {
                self.error(
                    ParseDiagnosticKind::InvalidPattern,
                    format!("expected a pattern, found {}", token.kind.describe()),
                    token.span,
                );
                self.bump();
                Spanned::new(PatternKind::Error, token.span)
            }
        }
    }

    fn parse_ui(&mut self) -> UiDecl {
        self.expect_word("ui");
        let name = self.expect_binding_name("UI declaration name");
        self.expect_word("for");
        let machine = self.expect_name("UI machine name");
        self.expect(TokenKind::LParen, "`(` before UI observation binding");
        let binding = self.expect_binding_name("UI observation binding");
        self.expect(TokenKind::RParen, "`)` after UI observation binding");

        let open_index = self.cursor;
        let open = self.expect(TokenKind::LBrace, "`{` before UI body");
        let close_index = self.find_matching(open_index, TokenKind::LBrace, TokenKind::RBrace);
        let nodes = if let Some(close_index) = close_index {
            let close = self.tokens[close_index].span;
            let body_start = open.end as usize;
            let body_end = close.start as usize;
            let body = self.source.get(body_start..body_end).unwrap_or_default();
            let output = parse_ui_body(self.source_id.file, body, open.end);
            self.diagnostics.extend(output.diagnostics);
            self.cursor = close_index + 1;
            output.nodes
        } else {
            self.error(
                ParseDiagnosticKind::InvalidUi,
                "unterminated UI declaration",
                open,
            );
            Vec::new()
        };
        UiDecl {
            name,
            machine,
            binding,
            nodes,
        }
    }

    fn parse_scenario(&mut self) -> ScenarioDecl {
        self.expect_word("scenario");
        let name = self.expect_binding_name("scenario name");
        let origin = if self.eat_word("for") {
            let machine = self.expect_name("scenario machine");
            let configuration = if self.eat(TokenKind::LParen) {
                let value = self.parse_expr();
                self.expect(TokenKind::RParen, "`)` after scenario configuration");
                Some(value)
            } else {
                None
            };
            ScenarioOrigin::Machine {
                machine,
                configuration,
            }
        } else if self.eat_word("from") {
            ScenarioOrigin::Snapshot(self.parse_evidence_ref())
        } else {
            let span = self.current().span;
            self.error(
                ParseDiagnosticKind::InvalidEvidence,
                "scenario requires `for Machine` or `from snapshot`",
                span,
            );
            ScenarioOrigin::Snapshot(EvidenceRef {
                path: vec![Spanned::new("<error>".into(), span)],
                span,
            })
        };
        self.expect(TokenKind::LBrace, "`{` before scenario steps");
        let mut steps = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.current().span.start;
            let before = self.cursor;
            if let Some(kind) = self.parse_evidence_step() {
                steps.push(Spanned::new(
                    kind,
                    SourceSpan::new(self.source_id.file, start, self.previous_end()),
                ));
            }
            if before == self.cursor {
                self.error_here(
                    ParseDiagnosticKind::InvalidEvidence,
                    "expected an evidence step",
                );
                self.bump();
            }
        }
        self.expect(TokenKind::RBrace, "`}` after scenario");
        ScenarioDecl {
            name,
            origin,
            steps,
        }
    }

    fn parse_evidence_alias(&mut self, is_example: bool) -> EvidenceAliasDecl {
        self.bump(); // `example` or `checkpoint`
        let name = self.expect_binding_name("evidence declaration name");
        let mut presentation = None;
        let mut kind = None;
        let mut is_default = false;
        let mut note = None;
        if is_example && self.eat_word("for") {
            presentation = Some(self.expect_name("example presentation"));
            self.expect_word("as");
            kind = Some(if self.eat_word("page") {
                EvidencePresentationKind::Page
            } else if self.eat_word("component") {
                EvidencePresentationKind::Component
            } else if self.eat_word("surface") {
                EvidencePresentationKind::Surface
            } else {
                self.error_here(
                    ParseDiagnosticKind::InvalidEvidence,
                    "expected example presentation kind `page`, `component`, or `surface`",
                );
                if !self.at(TokenKind::Eof) {
                    self.bump();
                }
                EvidencePresentationKind::Page
            });
            loop {
                if self.eat_word("default") {
                    if is_default {
                        self.error_here(
                            ParseDiagnosticKind::InvalidEvidence,
                            "`default` is repeated on this example",
                        );
                    }
                    is_default = true;
                } else if self.eat_word("note") {
                    let token = self.bump().clone();
                    match token.kind {
                        TokenKind::Text(value) if note.is_none() => note = Some(value),
                        TokenKind::Text(_) => self.error(
                            ParseDiagnosticKind::InvalidEvidence,
                            "`note` is repeated on this example",
                            token.span,
                        ),
                        _ => self.error(
                            ParseDiagnosticKind::MissingToken,
                            "expected quoted text after `note`",
                            token.span,
                        ),
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::Eq, "`=` in evidence declaration");
        let target = self.parse_evidence_ref();
        EvidenceAliasDecl {
            name,
            presentation,
            kind,
            is_default,
            note,
            target,
        }
    }

    fn parse_evidence_ref(&mut self) -> EvidenceRef {
        let start = self.current().span.start;
        let mut path = vec![self.expect_name("evidence reference")];
        while self.eat(TokenKind::ColonColon) {
            path.push(self.expect_name("evidence reference component"));
        }
        EvidenceRef {
            span: SourceSpan::new(self.source_id.file, start, self.previous_end()),
            path,
        }
    }

    fn parse_evidence_step(&mut self) -> Option<EvidenceStepKind> {
        Some(if self.eat_word("bind") {
            let port = self.expect_name("bound port name");
            self.expect(TokenKind::Eq, "`=` in fixture binding");
            EvidenceStepKind::Bind {
                port,
                fixture: self.parse_expr(),
            }
        } else if self.eat_word("start") {
            EvidenceStepKind::Start
        } else if self.eat_word("send") {
            EvidenceStepKind::Send(self.parse_expr())
        } else if self.eat_word("deliver") {
            EvidenceStepKind::Deliver(self.parse_expr())
        } else if self.eat_word("pin") {
            EvidenceStepKind::Pin(self.expect_binding_name("pin name"))
        } else if self.eat_word("expect") {
            self.parse_expect_step()
        } else {
            return None;
        })
    }

    fn parse_expect_step(&mut self) -> EvidenceStepKind {
        if self.eat_word("observation") {
            if self.eat_word("where") {
                EvidenceStepKind::ExpectObservationWhere(self.parse_expr())
            } else {
                EvidenceStepKind::ExpectObservationPattern(self.parse_pattern())
            }
        } else if self.eat_word("inspection") {
            EvidenceStepKind::ExpectInspectionPattern(self.parse_pattern())
        } else if self.eat_word("restore") {
            self.expect_word("commands");
            EvidenceStepKind::ExpectRestore {
                commands: self.parse_command_expectation(),
            }
        } else if self.eat_word("snapshot") {
            self.expect(TokenKind::EqEq, "`==` in snapshot expectation");
            EvidenceStepKind::ExpectSnapshot {
                target: self.parse_evidence_ref(),
            }
        } else {
            let outcome = self.parse_pattern();
            self.expect_word("commands");
            EvidenceStepKind::ExpectReaction {
                outcome,
                commands: self.parse_command_expectation(),
            }
        }
    }

    fn parse_command_expectation(&mut self) -> Vec<Expr> {
        self.expect(TokenKind::LBracket, "`[` before expected commands");
        let commands = self.parse_expr_list(TokenKind::RBracket);
        self.expect(TokenKind::RBracket, "`]` after expected commands");
        commands
    }

    fn parse_dot_path(&mut self) -> Vec<Name> {
        let mut values = vec![self.expect_name("name")];
        while self.eat(TokenKind::Dot) {
            values.push(self.expect_name("name after `.`"));
        }
        values
    }

    fn parse_name_list(&mut self, close: TokenKind) -> Vec<Name> {
        let mut values = Vec::new();
        while !self.at(close.clone()) && !self.at(TokenKind::Eof) {
            values.push(self.expect_binding_name("type parameter name"));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        values
    }

    fn expect_name(&mut self, context: &str) -> Name {
        let token = self.bump().clone();
        match token.kind {
            // Uhura's checked-in corpus requires contextual words: `start`
            // is both a command constructor and an evidence step, `from` is
            // both a total conversion member and an import/scenario word,
            // and `machine` appears in a logical module identity.  The
            // declaration parser therefore consumes grammar words
            // contextually instead of globally banning their spelling.
            TokenKind::Ident(value) => Spanned::new(value, token.span),
            _ => {
                self.error(
                    ParseDiagnosticKind::MissingToken,
                    format!("expected {context}, found {}", token.kind.describe()),
                    token.span,
                );
                Spanned::new("<error>".into(), token.span)
            }
        }
    }

    fn expect_binding_name(&mut self, context: &str) -> Name {
        let name = self.expect_name(context);
        self.validate_binding_name(&name, context);
        name
    }

    fn validate_binding_name(&mut self, name: &Name, context: &str) {
        if is_hard_reserved_word(&name.value) {
            self.error(
                ParseDiagnosticKind::UnexpectedToken,
                format!(
                    "`{}` is a reserved Uhura word and cannot be used as {context}",
                    name.value
                ),
                name.span,
            );
        } else if is_binding_reserved_builtin(&name.value) {
            self.error(
                ParseDiagnosticKind::UnexpectedToken,
                format!(
                    "`{}` is a binding-reserved Uhura builtin and cannot be used as {context}",
                    name.value
                ),
                name.span,
            );
        }
    }

    fn validate_lambda_parameter(&mut self, parameter: &Pattern) {
        if let PatternKind::Name(name) = &parameter.value {
            self.validate_binding_name(name, "lambda parameter");
        } else if matches!(&parameter.value, PatternKind::Wildcard) {
            // `_` is an identifier spelling in the lexical grammar and is the
            // selected ignored lambda binding.
        } else if matches!(&parameter.value, PatternKind::Error) {
            // The atomic-pattern parser already emitted the precise error.
        } else {
            self.error(
                ParseDiagnosticKind::InvalidPattern,
                "Uhura lambda parameters must be identifier bindings",
                parameter.span,
            );
        }
    }

    fn expect_word_name(&mut self, word: &str) -> Name {
        let token = self.bump().clone();
        match token.kind {
            TokenKind::Ident(value) if value == word => Spanned::new(value, token.span),
            _ => {
                self.error(
                    ParseDiagnosticKind::MissingToken,
                    format!("expected `{word}`, found {}", token.kind.describe()),
                    token.span,
                );
                Spanned::new(word.into(), token.span)
            }
        }
    }

    fn expect_feature_name(&mut self) -> Name {
        let token = self.bump().clone();
        match token.kind {
            TokenKind::Ident(value) if matches!(value.as_str(), "ui" | "evidence") => {
                Spanned::new(value, token.span)
            }
            _ => {
                self.error(
                    ParseDiagnosticKind::UnexpectedToken,
                    "Uhura 0.3 supports only `use ui` and `use evidence`",
                    token.span,
                );
                Spanned::new("<error>".into(), token.span)
            }
        }
    }

    fn expect_integer(&mut self, context: &str) -> (String, SourceSpan) {
        let token = self.bump().clone();
        match token.kind {
            TokenKind::Integer(value) => (value, token.span),
            _ => {
                self.error(
                    ParseDiagnosticKind::MissingToken,
                    format!("expected {context}, found {}", token.kind.describe()),
                    token.span,
                );
                ("0".into(), token.span)
            }
        }
    }

    fn at_word(&self, word: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Ident(value) if value == word)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.at_word(word) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, word: &str) -> SourceSpan {
        if self.eat_word(word) {
            self.tokens[self.cursor - 1].span
        } else {
            let span = self.current().span;
            self.error(
                ParseDiagnosticKind::MissingToken,
                format!(
                    "expected `{word}`, found {}",
                    self.current().kind.describe()
                ),
                span,
            );
            if !self.at(TokenKind::Eof) {
                self.bump();
            }
            span
        }
    }

    fn at(&self, expected: TokenKind) -> bool {
        discriminant(&self.current().kind) == discriminant(&expected)
    }

    fn eat(&mut self, expected: TokenKind) -> bool {
        if self.at(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: TokenKind, context: &str) -> SourceSpan {
        if self.at(expected.clone()) {
            self.bump().span
        } else {
            let span = self.current().span;
            self.error(
                ParseDiagnosticKind::MissingToken,
                format!(
                    "expected {context}, found {}",
                    self.current().kind.describe()
                ),
                span,
            );
            if !self.at(TokenKind::Eof) {
                self.bump();
            }
            span
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len().saturating_sub(1))]
    }

    fn bump(&mut self) -> &Token {
        let index = self.cursor.min(self.tokens.len().saturating_sub(1));
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
        &self.tokens[index]
    }

    fn previous_end(&self) -> u32 {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(self.current().span.start, |token| token.span.end)
    }

    fn find_matching(&self, open_index: usize, open: TokenKind, close: TokenKind) -> Option<usize> {
        let mut depth = 0usize;
        for (index, token) in self.tokens.iter().enumerate().skip(open_index) {
            if discriminant(&token.kind) == discriminant(&open) {
                depth += 1;
            } else if discriminant(&token.kind) == discriminant(&close) {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
        }
        None
    }

    fn error_here(&mut self, kind: ParseDiagnosticKind, message: impl Into<String>) {
        self.error(kind, message, self.current().span);
    }

    fn error(&mut self, kind: ParseDiagnosticKind, message: impl Into<String>, span: SourceSpan) {
        self.diagnostics.push(ParseDiagnostic {
            kind,
            message: message.into(),
            span,
        });
    }
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|value| value == '_' || is_xid_start(value))
        && characters.all(|value| value == '_' || is_xid_continue(value))
}

fn machine_member_section(member: &MachineMemberKind) -> (u8, &'static str) {
    match member {
        MachineMemberKind::Const(_) | MachineMemberKind::Key(_) | MachineMemberKind::Type(_) => {
            (0, "early declaration")
        }
        MachineMemberKind::Port(_) => (1, "port"),
        MachineMemberKind::Config(_) => (2, "config"),
        MachineMemberKind::Require(_) => (3, "require"),
        MachineMemberKind::Input(_) => (4, "input"),
        MachineMemberKind::Command(_) => (5, "command"),
        MachineMemberKind::Outcome(_) => (6, "outcome"),
        MachineMemberKind::State(_) => (7, "state"),
        MachineMemberKind::Function(_) => (8, "fn"),
        MachineMemberKind::Derive(_) => (9, "derive"),
        MachineMemberKind::Invariant(_) => (10, "invariant"),
        MachineMemberKind::Observe(_) => (11, "observe"),
        MachineMemberKind::Transition(_) => (12, "transition"),
        MachineMemberKind::Handler(_) => (13, "on"),
        MachineMemberKind::BeforeCommit(_) => (14, "before commit"),
    }
}

fn is_hard_reserved_word(value: &str) -> bool {
    matches!(
        value,
        "language"
            | "module"
            | "use"
            | "import"
            | "from"
            | "const"
            | "key"
            | "over"
            | "type"
            | "fn"
            | "machine"
            | "port"
            | "config"
            | "require"
            | "input"
            | "command"
            | "outcome"
            | "commit"
            | "abort"
            | "state"
            | "derive"
            | "invariant"
            | "observe"
            | "transition"
            | "on"
            | "before"
            | "let"
            | "set"
            | "emit"
            | "if"
            | "then"
            | "else"
            | "while"
            | "decreases"
            | "finish"
            | "unreachable"
            | "true"
            | "false"
            | "not"
            | "and"
            | "or"
            | "is"
            | "with"
            | "match"
            | "collect"
            | "when"
            | "for"
            | "in"
            | "yield"
            | "ui"
            | "scenario"
            | "example"
            | "checkpoint"
    )
}

fn is_binding_reserved_builtin(value: &str) -> bool {
    matches!(
        value,
        "Bool"
            | "Unit"
            | "Never"
            | "Int"
            | "Nat"
            | "PositiveInt"
            | "Decimal"
            | "BoundaryNumber"
            | "Ratio"
            | "Text"
            | "Option"
            | "Seq"
            | "NonEmpty"
            | "Set"
            | "Map"
            | "Table"
            | "FiniteView"
            | "min"
            | "max"
    )
}
