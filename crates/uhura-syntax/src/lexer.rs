//! UTF-8 lexer for Uhura's expression and declaration surfaces.
//!
//! UI bodies are parsed with a dedicated character cursor because text nodes
//! are intentionally not tokenised as language-generation identifiers. The
//! general lexer
//! still emits punctuation for the complete file so the declaration parser
//! can locate the balanced UI body without a second source scan.

use serde::{Deserialize, Serialize};
use unicode_ident::{is_xid_continue, is_xid_start};

use super::ast::{SourceId, SourceSpan};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace,
    Comment,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub text: String,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Integer(String),
    Decimal(String),
    Text(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Comma,
    Colon,
    ColonColon,
    Dot,
    Ellipsis,
    At,
    Eq,
    EqEq,
    Bang,
    NotEqual,
    Plus,
    Minus,
    Star,
    Pipe,
    Arrow,
    FatArrow,
    Slash,
    Hash,
    Other(char),
    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        match self {
            Self::Ident(value) => format!("identifier `{value}`"),
            Self::Integer(value) | Self::Decimal(value) => format!("number `{value}`"),
            Self::Text(_) => "text literal".into(),
            Self::LBrace => "`{`".into(),
            Self::RBrace => "`}`".into(),
            Self::LParen => "`(`".into(),
            Self::RParen => "`)`".into(),
            Self::LBracket => "`[`".into(),
            Self::RBracket => "`]`".into(),
            Self::Less => "`<`".into(),
            Self::LessEqual => "`<=`".into(),
            Self::Greater => "`>`".into(),
            Self::GreaterEqual => "`>=`".into(),
            Self::Comma => "`,`".into(),
            Self::Colon => "`:`".into(),
            Self::ColonColon => "`::`".into(),
            Self::Dot => "`.`".into(),
            Self::Ellipsis => "`...`".into(),
            Self::At => "`@`".into(),
            Self::Eq => "`=`".into(),
            Self::EqEq => "`==`".into(),
            Self::Bang => "`!`".into(),
            Self::NotEqual => "`!=`".into(),
            Self::Plus => "`+`".into(),
            Self::Minus => "`-`".into(),
            Self::Star => "`*`".into(),
            Self::Pipe => "`|`".into(),
            Self::Arrow => "`->`".into(),
            Self::FatArrow => "`=>`".into(),
            Self::Slash => "`/`".into(),
            Self::Hash => "`#`".into(),
            Self::Other(value) => format!("`{value}`"),
            Self::Eof => "end of file".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
    pub leading: Vec<Trivia>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexDiagnosticKind {
    InitialBom,
    ReservedComment,
    UnterminatedText,
    InvalidEscape,
    InvalidUnicodeEscape,
    SourceTooLarge,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LexDiagnostic {
    pub kind: LexDiagnosticKind,
    pub message: String,
    pub span: SourceSpan,
}

pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<LexDiagnostic>,
}

pub fn lex(source_id: &SourceId, source: &str) -> LexOutput {
    Lexer::new(source_id.file, source, 0).run()
}

pub(crate) fn lex_fragment(file: u32, source: &str, base: u32) -> LexOutput {
    Lexer::new(file, source, base).run()
}

struct Lexer<'a> {
    file: u32,
    source: &'a str,
    base: u32,
    pos: usize,
    diagnostics: Vec<LexDiagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(file: u32, source: &'a str, base: u32) -> Self {
        Self {
            file,
            source,
            base,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> LexOutput {
        if self.source.starts_with('\u{feff}') {
            let end = '\u{feff}'.len_utf8() as u32;
            self.diagnostics.push(LexDiagnostic {
                kind: LexDiagnosticKind::InitialBom,
                message: "Uhura source must not begin with a UTF-8 BOM".into(),
                span: SourceSpan::new(self.file, self.base, self.base + end),
            });
            self.pos += '\u{feff}'.len_utf8();
        }
        if self.source.len() > (u32::MAX - self.base) as usize {
            self.diagnostics.push(LexDiagnostic {
                kind: LexDiagnosticKind::SourceTooLarge,
                message: "Uhura source exceeds the supported 32-bit byte range".into(),
                span: SourceSpan::empty(self.file, self.base),
            });
        }

        let mut tokens = Vec::new();
        loop {
            let leading = self.trivia();
            let start = self.pos;
            let kind = self.token();
            let end = self.pos;
            let eof = kind == TokenKind::Eof;
            tokens.push(Token {
                kind,
                span: self.span(start, end),
                leading,
            });
            if eof {
                break;
            }
        }
        LexOutput {
            tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(
            self.file,
            self.base.saturating_add(start as u32),
            self.base.saturating_add(end as u32),
        )
    }

    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_n(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.pos += value.len_utf8();
        Some(value)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn trivia(&mut self) -> Vec<Trivia> {
        let mut values = Vec::new();
        loop {
            let start = self.pos;
            match self.peek() {
                Some(value) if value.is_whitespace() => {
                    while self.peek().is_some_and(char::is_whitespace) {
                        self.bump();
                    }
                    values.push(Trivia {
                        kind: TriviaKind::Whitespace,
                        text: self.source[start..self.pos].to_string(),
                        span: self.span(start, self.pos),
                    });
                }
                Some('/') if self.peek_n(1) == Some('/') => {
                    self.bump();
                    self.bump();
                    let reserved = matches!(self.peek(), Some('/' | '!'));
                    if reserved {
                        self.bump();
                    }
                    while !matches!(self.peek(), None | Some('\n' | '\r')) {
                        self.bump();
                    }
                    self.eat('\r');
                    self.eat('\n');
                    let span = self.span(start, self.pos);
                    if reserved {
                        self.diagnostics.push(LexDiagnostic {
                            kind: LexDiagnosticKind::ReservedComment,
                            message: "`///` and `//!` are reserved in Uhura 0.3".into(),
                            span,
                        });
                    }
                    values.push(Trivia {
                        kind: TriviaKind::Comment,
                        text: self.source[start..self.pos].to_string(),
                        span,
                    });
                }
                _ => break,
            }
        }
        values
    }

    fn token(&mut self) -> TokenKind {
        let start = self.pos;
        let Some(value) = self.bump() else {
            return TokenKind::Eof;
        };
        match value {
            value if value == '_' || is_xid_start(value) => {
                while self
                    .peek()
                    .is_some_and(|next| next == '_' || is_xid_continue(next))
                {
                    self.bump();
                }
                TokenKind::Ident(self.source[start..self.pos].to_string())
            }
            '0'..='9' => {
                while self.peek().is_some_and(|next| next.is_ascii_digit()) {
                    self.bump();
                }
                if self.peek() == Some('.')
                    && self.peek_n(1).is_some_and(|next| next.is_ascii_digit())
                {
                    self.bump();
                    while self.peek().is_some_and(|next| next.is_ascii_digit()) {
                        self.bump();
                    }
                    TokenKind::Decimal(self.source[start..self.pos].to_string())
                } else {
                    TokenKind::Integer(self.source[start..self.pos].to_string())
                }
            }
            '"' => self.text(start),
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '<' => {
                if self.eat('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }
            '>' => {
                if self.eat('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }
            ',' => TokenKind::Comma,
            ':' => {
                if self.eat(':') {
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            '.' => {
                if self.eat('.') && self.eat('.') {
                    TokenKind::Ellipsis
                } else {
                    TokenKind::Dot
                }
            }
            '@' => TokenKind::At,
            '=' => {
                if self.eat('=') {
                    TokenKind::EqEq
                } else if self.eat('>') {
                    TokenKind::FatArrow
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.eat('=') {
                    TokenKind::NotEqual
                } else {
                    TokenKind::Bang
                }
            }
            '+' => TokenKind::Plus,
            '-' => {
                if self.eat('>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }
            '*' => TokenKind::Star,
            '|' => TokenKind::Pipe,
            '/' => TokenKind::Slash,
            '#' => TokenKind::Hash,
            other => TokenKind::Other(other),
        }
    }

    fn text(&mut self, start: usize) -> TokenKind {
        let mut decoded = String::new();
        loop {
            let Some(value) = self.bump() else {
                self.diagnostics.push(LexDiagnostic {
                    kind: LexDiagnosticKind::UnterminatedText,
                    message: "unterminated Uhura text literal".into(),
                    span: self.span(start, self.pos),
                });
                return TokenKind::Text(decoded);
            };
            match value {
                '"' => return TokenKind::Text(decoded),
                '\n' | '\r' => {
                    self.diagnostics.push(LexDiagnostic {
                        kind: LexDiagnosticKind::UnterminatedText,
                        message: "Uhura text literals cannot contain raw line endings".into(),
                        span: self.span(start, self.pos),
                    });
                    return TokenKind::Text(decoded);
                }
                '\\' => {
                    let escape_start = self.pos.saturating_sub(1);
                    match self.bump() {
                        Some('"') => decoded.push('"'),
                        Some('\\') => decoded.push('\\'),
                        Some('n') => decoded.push('\n'),
                        Some('r') => decoded.push('\r'),
                        Some('t') => decoded.push('\t'),
                        Some('u') if self.eat('{') => {
                            let digits_start = self.pos;
                            while self.peek().is_some_and(|next| next.is_ascii_hexdigit()) {
                                self.bump();
                            }
                            let digits_end = self.pos;
                            let closed = self.eat('}');
                            let parsed =
                                u32::from_str_radix(&self.source[digits_start..digits_end], 16)
                                    .ok()
                                    .and_then(char::from_u32);
                            if !closed || digits_start == digits_end || parsed.is_none() {
                                self.diagnostics.push(LexDiagnostic {
                                    kind: LexDiagnosticKind::InvalidUnicodeEscape,
                                    message: "invalid Unicode scalar escape".into(),
                                    span: self.span(escape_start, self.pos),
                                });
                            } else if let Some(value) = parsed {
                                decoded.push(value);
                            }
                        }
                        Some(_) | None => {
                            self.diagnostics.push(LexDiagnostic {
                                kind: LexDiagnosticKind::InvalidEscape,
                                message: "unsupported Uhura text escape".into(),
                                span: self.span(escape_start, self.pos),
                            });
                        }
                    }
                }
                other => decoded.push(other),
            }
        }
    }
}
