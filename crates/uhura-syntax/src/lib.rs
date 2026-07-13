//! uhura-syntax: mode-switching lexer (Dsl / Markup / Expr / Style /
//! Examples), recursive-descent parsers with recovery, AST, and the one
//! canonical trivia-preserving formatter (design §4, §12.2).

pub mod ast;
pub mod css;
mod cursor;
mod format;
mod parser;
mod token;

pub use cursor::Cursor;
pub use format::{expr_str, format_examples, format_module, type_str};
pub use parser::{ParseOutput, Parsed, SourceKind, parse};
pub use token::{Comment, Token, TokenKind};
