//! Uhura 0.4 concrete syntax frontend.
//!
//! Project loading supplies source identity and module role; this module lexes
//! and parses the headerless 0.4 language used by core, UI, and evidence
//! modules.

pub mod ast;
mod format;
pub mod lexer;
mod parser;
mod ui;

use serde::{Deserialize, Serialize};
use uhura_base::{Diagnostic, Edit, Severity};

pub use ast::{Module, SourceIdentity, Span};
pub use format::{FormatError, UnsupportedComment, format};
pub use lexer::{Keyword, LexDiagnosticKind, Token, TokenKind, Trivia, TriviaKind, lex};

use lexer::LexDiagnostic;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ParseDiagnosticKind {
    Lexical,
    UnexpectedToken,
    MissingToken,
    InvalidName,
    InvalidDeclaration,
    InvalidMember,
    InvalidType,
    InvalidPattern,
    InvalidExpression,
    InvalidStatement,
    InvalidUi,
    InvalidEvidence,
    ComparisonChain,
}

impl ParseDiagnosticKind {
    pub const fn diagnostic_identity(self) -> uhura_base::codes::Code {
        use uhura_base::codes::v04_parse;

        match self {
            Self::Lexical => v04_parse::LEXICAL,
            Self::UnexpectedToken => v04_parse::UNEXPECTED_TOKEN,
            Self::MissingToken => v04_parse::MISSING_TOKEN,
            Self::InvalidName => v04_parse::INVALID_NAME,
            Self::InvalidDeclaration => v04_parse::INVALID_DECLARATION,
            Self::InvalidMember => v04_parse::INVALID_MEMBER,
            Self::InvalidType => v04_parse::INVALID_TYPE,
            Self::InvalidPattern => v04_parse::INVALID_PATTERN,
            Self::InvalidExpression => v04_parse::INVALID_EXPRESSION,
            Self::InvalidStatement => v04_parse::INVALID_STATEMENT,
            Self::InvalidUi => v04_parse::INVALID_UI,
            Self::InvalidEvidence => v04_parse::INVALID_EVIDENCE,
            Self::ComparisonChain => v04_parse::COMPARISON_CHAIN,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ParseFix {
    pub title: String,
    pub span: Span,
    pub insert: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub kind: ParseDiagnosticKind,
    pub message: String,
    pub span: Span,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<ParseFix>,
}

impl ParseDiagnostic {
    pub(crate) fn new(kind: ParseDiagnosticKind, message: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
            fix: None,
        }
    }

    pub(crate) fn with_fix(
        mut self,
        title: impl Into<String>,
        span: Span,
        insert: impl Into<String>,
    ) -> Self {
        self.fix = Some(ParseFix {
            title: title.into(),
            span,
            insert: insert.into(),
        });
        self
    }

    /// Convert syntax output into the one public diagnostic shape consumed by
    /// the CLI, host, editor, and downstream tools.
    pub fn into_public_diagnostic(self) -> Diagnostic {
        let (code, rule) = self.kind.diagnostic_identity();
        let mut diagnostic = Diagnostic::new(
            code,
            rule,
            Severity::Error,
            self.message,
            uhura_base::Span::new(
                uhura_base::FileId(self.span.file),
                self.span.start,
                self.span.end,
            ),
        );
        if self.kind == ParseDiagnosticKind::ComparisonChain {
            diagnostic = diagnostic
                .with_note("split the relation into complete comparisons joined by `&&` or `||`");
        } else if self.kind == ParseDiagnosticKind::InvalidName {
            diagnostic = diagnostic.with_note(
                "Uhura 0.4 symbolic names are ASCII and their case shape is part of the grammar",
            );
        }
        if let Some(fix) = self.fix {
            diagnostic = diagnostic.with_fix(
                fix.title,
                vec![Edit {
                    span: uhura_base::Span::new(
                        uhura_base::FileId(fix.span.file),
                        fix.span.start,
                        fix.span.end,
                    ),
                    insert: fix.insert,
                }],
            );
        }
        diagnostic
    }
}

impl From<LexDiagnostic> for ParseDiagnostic {
    fn from(value: LexDiagnostic) -> Self {
        Self::new(ParseDiagnosticKind::Lexical, value.message, value.span)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Parse {
    pub module: Module,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub tokens: Vec<Token>,
}

impl Parse {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Reconstruct the exact input from the lossless token/trivia stream.
    pub fn source_from_tokens(&self) -> String {
        let mut source = String::new();
        for token in &self.tokens {
            for trivia in &token.leading {
                source.push_str(&trivia.text);
            }
            source.push_str(&token.lexeme);
        }
        source
    }
}

/// Parse one manifest-resolved Uhura 0.4 logical module.
pub fn parse(identity: SourceIdentity, source: &str) -> Parse {
    let lexical = lex(&identity, source);
    let tokens = lexical.tokens;
    let (uses, declarations, mut diagnostics) =
        parser::Parser::new(identity.clone(), &tokens).parse_module();
    diagnostics.extend(lexical.diagnostics.into_iter().map(Into::into));
    diagnostics.sort_by(|left, right| {
        (
            left.span.start,
            left.span.end,
            left.kind,
            left.message.as_str(),
        )
            .cmp(&(
                right.span.start,
                right.span.end,
                right.kind,
                right.message.as_str(),
            ))
    });

    let module = Module {
        span: Span::new(identity.file, 0, source.len().min(u32::MAX as usize) as u32),
        identity,
        uses,
        declarations,
        source: source.to_string(),
        tokens: tokens.clone(),
    };
    Parse {
        module,
        diagnostics,
        tokens,
    }
}
