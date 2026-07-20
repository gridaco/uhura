//! Uhura 0.4 concrete syntax frontend.
//!
//! Project loading supplies source identity; this module lexes and parses the
//! headerless core language while retaining the 0.3 parser as an explicit
//! differential and compatibility frontend.

pub mod ast;
mod format;
pub mod lexer;
mod parser;
mod ui;

use serde::{Deserialize, Serialize};

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
    ComparisonChain,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub kind: ParseDiagnosticKind,
    pub message: String,
    pub span: Span,
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
