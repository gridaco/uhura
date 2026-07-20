//! The canonical Uhura source layer: a UTF-8 lexer, source-spanned AST,
//! recursive-descent parser, checked UI parser, and deterministic formatter.

pub mod ast;
mod format;
mod lexer;
mod parser;
mod ui;
pub mod v04;

pub use ast::SourceId;
pub use format::format;
pub use parser::{
    Parse, ParseDiagnostic, ParseDiagnosticKind, ProjectParse, SourceFile, parse, parse_project,
};

#[cfg(test)]
mod tests;
