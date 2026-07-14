//! DSL token model. Markup and style surfaces are parsed char-wise by their
//! own parsers (design §4.4–§4.5); these tokens cover the header, the store
//! block, and every `{expr}` region.

use uhura_base::Span;

/// The lexical class of one DSL line comment.
///
/// The distinction is made by the lexer in every DSL region. Placement and
/// attachment are parser concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    Ordinary,
    /// Exactly `///` (not a run of four or more slashes).
    OuterDoc,
    /// `//!`.
    InnerDoc,
}

/// A DSL line comment, excluding its line terminator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub span: Span,
    pub kind: CommentKind,
    /// Text after the complete sigil (`//`, `///`, or `//!`), untrimmed.
    pub text: String,
}

impl Comment {
    /// The canonical text of one documentation line.
    pub fn normalized_doc_line(&self) -> String {
        debug_assert!(self.kind != CommentKind::Ordinary);
        let text = self.text.strip_prefix(' ').unwrap_or(&self.text);
        text.trim_end_matches([' ', '\t']).to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Kebab-case identifier or contextual keyword (`store`, `on`, `when`,
    /// `set`, `send`, `if`, `true`, …) — keywords are resolved by parsers,
    /// never by the lexer.
    Ident(String),
    /// Integer literal. Overflow is diagnosed at lex time and saturates.
    Int(i64),
    /// String literal (unescaped content).
    Str(String),

    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Question,
    Dot,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    PlusPlus,
    Minus,
    Bang,
    AndAnd,
    OrOr,
    /// `??`
    Coalesce,

    Eof,
    /// Lex error already diagnosed; parsers skip it.
    Error,
}

impl TokenKind {
    /// Human name for diagnostics ("expected `}`, found …").
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Ident(s) => format!("`{s}`"),
            TokenKind::Int(i) => format!("`{i}`"),
            TokenKind::Str(_) => "string literal".to_string(),
            TokenKind::LBrace => "`{`".into(),
            TokenKind::RBrace => "`}`".into(),
            TokenKind::LParen => "`(`".into(),
            TokenKind::RParen => "`)`".into(),
            TokenKind::LBracket => "`[`".into(),
            TokenKind::RBracket => "`]`".into(),
            TokenKind::Comma => "`,`".into(),
            TokenKind::Colon => "`:`".into(),
            TokenKind::Question => "`?`".into(),
            TokenKind::Dot => "`.`".into(),
            TokenKind::Eq => "`=`".into(),
            TokenKind::EqEq => "`==`".into(),
            TokenKind::NotEq => "`!=`".into(),
            TokenKind::Lt => "`<`".into(),
            TokenKind::Le => "`<=`".into(),
            TokenKind::Gt => "`>`".into(),
            TokenKind::Ge => "`>=`".into(),
            TokenKind::Plus => "`+`".into(),
            TokenKind::PlusPlus => "`++`".into(),
            TokenKind::Minus => "`-`".into(),
            TokenKind::Bang => "`!`".into(),
            TokenKind::AndAnd => "`&&`".into(),
            TokenKind::OrOr => "`||`".into(),
            TokenKind::Coalesce => "`??`".into(),
            TokenKind::Eof => "end of file".into(),
            TokenKind::Error => "invalid token".into(),
        }
    }

    pub fn is_ident(&self, text: &str) -> bool {
        matches!(self, TokenKind::Ident(s) if s == text)
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// Comments collected immediately before this token.
    pub leading: Vec<Comment>,
}
