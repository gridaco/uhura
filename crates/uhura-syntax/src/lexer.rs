//! Lossless UTF-8 lexer for the current Uhura core grammar.

use serde::{Deserialize, Serialize};
use unicode_ident::{is_xid_continue, is_xid_start};

use super::ast::{SourceIdentity, Span};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriviaKind {
    Whitespace,
    InvalidWhitespace,
    OrdinaryComment,
    OuterDoc,
    InnerDoc,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub text: String,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Keyword {
    Abort,
    As,
    Before,
    Commands,
    Commit,
    Computed,
    Config,
    Const,
    Crate,
    Decreases,
    Else,
    Emit,
    Enum,
    Events,
    False,
    Fn,
    If,
    Invariant,
    Is,
    Key,
    Let,
    Machine,
    Match,
    Observe,
    On,
    Outcomes,
    Part,
    Port,
    Pub,
    Require,
    Requires,
    Return,
    State,
    Struct,
    True,
    Unreachable,
    Update,
    Use,
    While,
}

impl Keyword {
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::As => "as",
            Self::Before => "before",
            Self::Commands => "commands",
            Self::Commit => "commit",
            Self::Computed => "computed",
            Self::Config => "config",
            Self::Const => "const",
            Self::Crate => "crate",
            Self::Decreases => "decreases",
            Self::Else => "else",
            Self::Emit => "emit",
            Self::Enum => "enum",
            Self::Events => "events",
            Self::False => "false",
            Self::Fn => "fn",
            Self::If => "if",
            Self::Invariant => "invariant",
            Self::Is => "is",
            Self::Key => "key",
            Self::Let => "let",
            Self::Machine => "machine",
            Self::Match => "match",
            Self::Observe => "observe",
            Self::On => "on",
            Self::Outcomes => "outcomes",
            Self::Part => "part",
            Self::Port => "port",
            Self::Pub => "pub",
            Self::Require => "require",
            Self::Requires => "requires",
            Self::Return => "return",
            Self::State => "state",
            Self::Struct => "struct",
            Self::True => "true",
            Self::Unreachable => "unreachable",
            Self::Update => "update",
            Self::Use => "use",
            Self::While => "while",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "abort" => Self::Abort,
            "as" => Self::As,
            "before" => Self::Before,
            "commands" => Self::Commands,
            "commit" => Self::Commit,
            "computed" => Self::Computed,
            "config" => Self::Config,
            "const" => Self::Const,
            "crate" => Self::Crate,
            "decreases" => Self::Decreases,
            "else" => Self::Else,
            "emit" => Self::Emit,
            "enum" => Self::Enum,
            "events" => Self::Events,
            "false" => Self::False,
            "fn" => Self::Fn,
            "if" => Self::If,
            "invariant" => Self::Invariant,
            "is" => Self::Is,
            "key" => Self::Key,
            "let" => Self::Let,
            "machine" => Self::Machine,
            "match" => Self::Match,
            "observe" => Self::Observe,
            "on" => Self::On,
            "outcomes" => Self::Outcomes,
            "part" => Self::Part,
            "port" => Self::Port,
            "pub" => Self::Pub,
            "require" => Self::Require,
            "requires" => Self::Requires,
            "return" => Self::Return,
            "state" => Self::State,
            "struct" => Self::Struct,
            "true" => Self::True,
            "unreachable" => Self::Unreachable,
            "update" => Self::Update,
            "use" => Self::Use,
            "while" => Self::While,
            _ => return None,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(Keyword),
    Identifier(String),
    Integer(String),
    Decimal(String),
    Text(String),
    Underscore,
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
    Semicolon,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    Eq,
    EqEq,
    Bang,
    NotEqual,
    Plus,
    Minus,
    Star,
    Pipe,
    PipePipe,
    AmpAmp,
    Arrow,
    FatArrow,
    /// Exact source between the outer braces of a contextual `ui`
    /// declaration. A dedicated character-level markup parser owns it.
    UiBody,
    Invalid(String),
    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        match self {
            Self::Keyword(keyword) => format!("`{}`", keyword.spelling()),
            Self::Identifier(value) => format!("identifier `{value}`"),
            Self::Integer(value) | Self::Decimal(value) => format!("number `{value}`"),
            Self::Text(_) => "text literal".into(),
            Self::Underscore => "`_`".into(),
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
            Self::Semicolon => "`;`".into(),
            Self::Colon => "`:`".into(),
            Self::ColonColon => "`::`".into(),
            Self::Dot => "`.`".into(),
            Self::DotDot => "`..`".into(),
            Self::Eq => "`=`".into(),
            Self::EqEq => "`==`".into(),
            Self::Bang => "`!`".into(),
            Self::NotEqual => "`!=`".into(),
            Self::Plus => "`+`".into(),
            Self::Minus => "`-`".into(),
            Self::Star => "`*`".into(),
            Self::Pipe => "`|`".into(),
            Self::PipePipe => "`||`".into(),
            Self::AmpAmp => "`&&`".into(),
            Self::Arrow => "`->`".into(),
            Self::FatArrow => "`=>`".into(),
            Self::UiBody => "UI body".into(),
            Self::Invalid(value) => format!("invalid token `{value}`"),
            Self::Eof => "end of file".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Exact token spelling. EOF has an empty lexeme.
    pub lexeme: String,
    pub span: Span,
    /// Exact trivia between the preceding token and this token.
    pub leading: Vec<Trivia>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LexDiagnosticKind {
    InitialBom,
    SourceTooLarge,
    NulCharacter,
    InvalidWhitespace,
    NonAsciiIdentifier,
    InvalidIdentifier,
    InvalidNumber,
    UnterminatedText,
    InvalidEscape,
    InvalidUnicodeEscape,
    InvalidSurrogatePair,
    UnexpectedCharacter,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LexDiagnostic {
    pub kind: LexDiagnosticKind,
    pub message: String,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<LexDiagnostic>,
}

pub fn lex(identity: &SourceIdentity, source: &str) -> LexOutput {
    isolate_ui_bodies(
        identity.file,
        source,
        Lexer::new(identity.file, source).run(),
    )
}

/// Lex one core fragment embedded in a UI brace at its absolute source offset.
///
/// This deliberately bypasses UI-body isolation: an embedded fragment is
/// always the Rust-shaped core expression or pattern language.
pub(super) fn lex_fragment(file: u32, source: &str, base: u32) -> LexOutput {
    let mut output = Lexer::new(file, source).run();
    for token in &mut output.tokens {
        shift_span(&mut token.span, base);
        for trivia in &mut token.leading {
            shift_span(&mut trivia.span, base);
        }
    }
    for diagnostic in &mut output.diagnostics {
        shift_span(&mut diagnostic.span, base);
    }
    output
}

fn shift_span(span: &mut Span, offset: u32) {
    span.start = span.start.saturating_add(offset);
    span.end = span.end.saturating_add(offset);
}

#[derive(Clone, Copy, Debug)]
struct UiBodyRange {
    open_token: usize,
    close_token: usize,
    body_start: usize,
    body_end: usize,
}

/// Replace the core tokens inside an exact contextual UI declaration with one
/// lossless body token. This is a lexical mode boundary, not a regex rewrite:
/// the declaration header is recognized from core tokens and the body close is
/// found with markup/comment/string-aware brace balancing.
fn isolate_ui_bodies(file: u32, source: &str, mut output: LexOutput) -> LexOutput {
    let mut ranges = Vec::new();
    let mut index = 0usize;
    let mut brace_depth = 0u32;

    while index < output.tokens.len() {
        if brace_depth == 0
            && let Some(open_token) = ui_header_open(&output.tokens, index)
        {
            let body_start = output.tokens[open_token].span.end as usize;
            if let Some(body_end) = find_ui_body_close(source, body_start)
                && let Some(close_token) = output
                    .tokens
                    .iter()
                    .enumerate()
                    .skip(open_token + 1)
                    .find_map(|(candidate, token)| {
                        (token.span.start as usize == body_end && token.kind == TokenKind::RBrace)
                            .then_some(candidate)
                    })
            {
                ranges.push(UiBodyRange {
                    open_token,
                    close_token,
                    body_start,
                    body_end,
                });
                index = close_token + 1;
                continue;
            }
        }

        match output.tokens[index].kind {
            TokenKind::LBrace => brace_depth += 1,
            TokenKind::RBrace => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }

    if ranges.is_empty() {
        return output;
    }

    output.diagnostics.retain(|diagnostic| {
        diagnostic.kind == LexDiagnosticKind::NulCharacter
            || !ranges.iter().any(|range| {
                diagnostic.span.start as usize >= range.body_start
                    && diagnostic.span.end as usize <= range.body_end
            })
    });

    let mut tokens = Vec::with_capacity(output.tokens.len());
    let mut cursor = 0usize;
    for range in ranges {
        tokens.extend(output.tokens[cursor..=range.open_token].iter().cloned());
        tokens.push(Token {
            kind: TokenKind::UiBody,
            lexeme: source[range.body_start..range.body_end].to_string(),
            span: Span::new(file, range.body_start as u32, range.body_end as u32),
            leading: Vec::new(),
        });
        let mut close = output.tokens[range.close_token].clone();
        // The UI-body token owns every byte before the close, including trivia
        // that the core lexer had attached to this token.
        close.leading.clear();
        tokens.push(close);
        cursor = range.close_token + 1;
    }
    tokens.extend(output.tokens[cursor..].iter().cloned());
    output.tokens = tokens;
    output
}

fn ui_header_open(tokens: &[Token], start: usize) -> Option<usize> {
    let mut index = start;
    if tokens.get(index)?.kind == TokenKind::Keyword(Keyword::Pub) {
        index += 1;
    }
    if !matches!(
        tokens.get(index)?.kind,
        TokenKind::Identifier(ref value) if value == "ui"
    ) {
        return None;
    }
    index += 1;
    if !matches!(tokens.get(index)?.kind, TokenKind::Identifier(_)) {
        return None;
    }
    index += 1;
    if !matches!(
        tokens.get(index)?.kind,
        TokenKind::Identifier(ref value) if value == "for"
    ) {
        return None;
    }
    index += 1;

    let machine_start = index;
    while !matches!(
        tokens.get(index)?.kind,
        TokenKind::LParen | TokenKind::LBrace | TokenKind::Eof
    ) {
        if !matches!(
            tokens[index].kind,
            TokenKind::Identifier(_)
                | TokenKind::ColonColon
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::Comma
        ) {
            return None;
        }
        index += 1;
    }
    if index == machine_start || tokens.get(index)?.kind != TokenKind::LParen {
        return None;
    }
    index += 1;
    if !matches!(tokens.get(index)?.kind, TokenKind::Identifier(_)) {
        return None;
    }
    index += 1;
    if tokens.get(index)?.kind != TokenKind::RParen {
        return None;
    }
    index += 1;
    (tokens.get(index)?.kind == TokenKind::LBrace).then_some(index)
}

fn find_ui_body_close(source: &str, mut position: usize) -> Option<usize> {
    let mut brace_depth = 1u32;
    let mut in_tag = false;
    let mut closing_tag = false;
    let mut tag_start = 0usize;
    let mut tag_paren_depth = 0u32;
    let mut tag_bracket_depth = 0u32;
    let mut element_depth = 0u32;
    let mut directive_depth = 0u32;
    let mut recovery_close = None;

    while position < source.len() {
        let rest = &source[position..];
        if let Some(comment_body) = rest.strip_prefix("<!--") {
            if let Some(close) = comment_body.find("-->") {
                position += 4 + close + 3;
            } else if let Some(recovery) = find_ui_comment_recovery(comment_body) {
                position += 4 + recovery;
            } else {
                return recovery_close;
            }
            continue;
        }
        if brace_depth > 1 && rest.starts_with("//") {
            position += rest.find(['\n', '\r']).unwrap_or(rest.len());
            continue;
        }

        let value = rest.chars().next()?;
        if value == '"' && (in_tag || brace_depth > 1) {
            position += value.len_utf8();
            let mut escaped = false;
            while position < source.len() {
                let next = source[position..].chars().next()?;
                position += next.len_utf8();
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == '"' {
                    break;
                }
            }
            continue;
        }

        match value {
            '<' if brace_depth == 1 && !in_tag => {
                in_tag = true;
                closing_tag = rest.starts_with("</");
                tag_start = position;
                tag_paren_depth = 0;
                tag_bracket_depth = 0;
            }
            '>' if in_tag
                && brace_depth == 1
                && tag_paren_depth == 0
                && tag_bracket_depth == 0
                && !source[..position].ends_with('-') =>
            {
                let self_closing = source[tag_start..position].trim_end().ends_with('/');
                if closing_tag {
                    element_depth = element_depth.saturating_sub(1);
                } else if !self_closing {
                    element_depth += 1;
                }
                in_tag = false;
            }
            '(' if in_tag => tag_paren_depth += 1,
            ')' if in_tag => tag_paren_depth = tag_paren_depth.saturating_sub(1),
            '[' if in_tag => tag_bracket_depth += 1,
            ']' if in_tag => tag_bracket_depth = tag_bracket_depth.saturating_sub(1),
            '{' => {
                if brace_depth == 1 && !in_tag {
                    if rest.starts_with("{#if") || rest.starts_with("{#each") {
                        directive_depth += 1;
                    } else if rest.starts_with("{/if}") || rest.starts_with("{/each}") {
                        directive_depth = directive_depth.saturating_sub(1);
                    }
                }
                brace_depth += 1;
            }
            '}' => {
                if brace_depth > 1 {
                    brace_depth -= 1;
                } else if !in_tag && element_depth == 0 && directive_depth == 0 {
                    return Some(position);
                } else if !in_tag {
                    // Keep the last plausible declaration close for malformed
                    // markup/directives. A later balanced close wins; if none
                    // exists, the UI parser still receives the body and can
                    // report the missing inner close precisely.
                    recovery_close = Some(position);
                }
            }
            _ => {}
        }
        position += value.len_utf8();
    }
    recovery_close
}

fn find_ui_comment_recovery(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut line = 0;
    while line < bytes.len() {
        let relative = rest[line..].find(['\n', '\r'])?;
        let mut next = line + relative + 1;
        if bytes.get(line + relative) == Some(&b'\r') && bytes.get(next) == Some(&b'\n') {
            next += 1;
        }
        let mut probe = next;
        while matches!(bytes.get(probe), Some(b' ' | b'\t')) {
            probe += 1;
        }
        if is_ui_comment_recovery_boundary(&rest[probe..]) {
            return Some(next);
        }
        line = next;
    }
    None
}

fn is_ui_comment_recovery_boundary(rest: &str) -> bool {
    rest.starts_with("<!--")
        || rest.starts_with("{#")
        || rest.starts_with("{:")
        || rest.starts_with("{/")
        || rest.starts_with("</")
        || rest.starts_with('}')
        || rest
            .strip_prefix('<')
            .and_then(|tail| tail.as_bytes().first())
            .is_some_and(u8::is_ascii_alphabetic)
}

struct Lexer<'a> {
    file: u32,
    source: &'a str,
    position: usize,
    diagnostics: Vec<LexDiagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(file: u32, source: &'a str) -> Self {
        Self {
            file,
            source,
            position: 0,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> LexOutput {
        if self.source.starts_with('\u{feff}') {
            self.diagnostic(
                LexDiagnosticKind::InitialBom,
                "Uhura source must not begin with a UTF-8 BOM",
                0,
                '\u{feff}'.len_utf8(),
            );
        }
        if self.source.len() > u32::MAX as usize {
            self.diagnostics.push(LexDiagnostic {
                kind: LexDiagnosticKind::SourceTooLarge,
                message: "Uhura source exceeds the supported 32-bit byte range".into(),
                span: Span::empty(self.file, 0),
            });
        }

        let mut tokens = Vec::new();
        loop {
            let leading = self.trivia();
            let start = self.position;
            let kind = self.token();
            let end = self.position;
            let eof = kind == TokenKind::Eof;
            tokens.push(Token {
                kind,
                lexeme: self.source[start..end].to_string(),
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

    fn trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::new();
        loop {
            let start = self.position;
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r') => {
                    while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
                        self.bump();
                    }
                    trivia.push(self.make_trivia(TriviaKind::Whitespace, start));
                }
                Some(value) if value.is_whitespace() => {
                    while self
                        .peek()
                        .is_some_and(|next| next.is_whitespace() && !next.is_ascii())
                    {
                        self.bump();
                    }
                    self.diagnostic(
                        LexDiagnosticKind::InvalidWhitespace,
                        "only ASCII space, tab, LF, CRLF, and CR are core whitespace",
                        start,
                        self.position,
                    );
                    trivia.push(self.make_trivia(TriviaKind::InvalidWhitespace, start));
                }
                Some('/') if self.peek_n(1) == Some('/') => {
                    self.bump();
                    self.bump();
                    let kind = if self.peek() == Some('!') {
                        self.bump();
                        TriviaKind::InnerDoc
                    } else if self.peek() == Some('/') && self.peek_n(1) != Some('/') {
                        self.bump();
                        TriviaKind::OuterDoc
                    } else {
                        while self.peek() == Some('/') {
                            self.bump();
                        }
                        TriviaKind::OrdinaryComment
                    };
                    while !matches!(self.peek(), None | Some('\n' | '\r')) {
                        self.bump();
                    }
                    trivia.push(self.make_trivia(kind, start));
                }
                _ => break,
            }
        }
        trivia
    }

    fn make_trivia(&self, kind: TriviaKind, start: usize) -> Trivia {
        Trivia {
            kind,
            text: self.source[start..self.position].to_string(),
            span: self.span(start, self.position),
        }
    }

    fn token(&mut self) -> TokenKind {
        let Some(value) = self.peek() else {
            return TokenKind::Eof;
        };

        if value.is_ascii_digit() {
            return self.number();
        }
        if value == '.' && self.peek_n(1).is_some_and(|next| next.is_ascii_digit()) {
            return self.leading_dot_number();
        }
        if value == '"' {
            return self.text();
        }
        if value == '_' || value.is_ascii_alphabetic() || is_xid_start(value) {
            return self.identifier();
        }

        let start = self.position;
        match value {
            '{' => self.single(TokenKind::LBrace),
            '}' => self.single(TokenKind::RBrace),
            '(' => self.single(TokenKind::LParen),
            ')' => self.single(TokenKind::RParen),
            '[' => self.single(TokenKind::LBracket),
            ']' => self.single(TokenKind::RBracket),
            ',' => self.single(TokenKind::Comma),
            ';' => self.single(TokenKind::Semicolon),
            ':' => {
                self.bump();
                if self.eat(':') {
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            '.' => {
                self.bump();
                if self.eat('.') {
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                }
            }
            '=' => {
                self.bump();
                if self.eat('=') {
                    TokenKind::EqEq
                } else if self.eat('>') {
                    TokenKind::FatArrow
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                self.bump();
                if self.eat('=') {
                    TokenKind::NotEqual
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                self.bump();
                if self.eat('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }
            '>' => {
                self.bump();
                if self.eat('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }
            '+' => self.single(TokenKind::Plus),
            '-' => {
                self.bump();
                if self.eat('>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }
            '*' => self.single(TokenKind::Star),
            '|' => {
                self.bump();
                if self.eat('|') {
                    TokenKind::PipePipe
                } else {
                    TokenKind::Pipe
                }
            }
            '&' => {
                self.bump();
                if self.eat('&') {
                    TokenKind::AmpAmp
                } else {
                    self.unexpected(start, "`&` is not a core token; use `&&`")
                }
            }
            '\0' => {
                self.bump();
                self.diagnostic(
                    LexDiagnosticKind::NulCharacter,
                    "U+0000 is forbidden in Uhura source",
                    start,
                    self.position,
                );
                TokenKind::Invalid("\\0".into())
            }
            '\u{feff}' if start == 0 => {
                self.bump();
                TokenKind::Invalid("UTF-8 BOM".into())
            }
            _ => {
                self.bump();
                self.unexpected(start, format!("unexpected character `{value}`"))
            }
        }
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.bump();
        kind
    }

    fn unexpected(&mut self, start: usize, message: impl Into<String>) -> TokenKind {
        self.diagnostic(
            LexDiagnosticKind::UnexpectedCharacter,
            message,
            start,
            self.position,
        );
        TokenKind::Invalid(self.source[start..self.position].to_string())
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.position;
        let first = self.bump().expect("identifier has a first character");
        while self
            .peek()
            .is_some_and(|value| value == '_' || is_xid_continue(value))
        {
            self.bump();
        }
        let word = &self.source[start..self.position];

        if word == "_" {
            return TokenKind::Underscore;
        }
        if !word.is_ascii() {
            self.diagnostic(
                LexDiagnosticKind::NonAsciiIdentifier,
                "Uhura 0.4 symbolic names are ASCII; use Unicode inside Text",
                start,
                self.position,
            );
            return TokenKind::Invalid(word.to_string());
        }
        if first == '_' {
            self.diagnostic(
                LexDiagnosticKind::InvalidIdentifier,
                "`_` is only a pattern wildcard and declared names cannot begin with `_`",
                start,
                self.position,
            );
            return TokenKind::Invalid(word.to_string());
        }
        if let Some(keyword) = Keyword::from_word(word) {
            TokenKind::Keyword(keyword)
        } else {
            TokenKind::Identifier(word.to_string())
        }
    }

    fn number(&mut self) -> TokenKind {
        let start = self.position;
        while self.peek().is_some_and(|value| value.is_ascii_digit()) {
            self.bump();
        }
        let integer_end = self.position;
        let mut decimal = false;
        let mut invalid = self.source[start..integer_end].len() > 1
            && self.source.as_bytes().get(start) == Some(&b'0');

        if self.peek() == Some('.') && self.peek_n(1) != Some('.') {
            self.bump();
            decimal = true;
            if !self.peek().is_some_and(|value| value.is_ascii_digit()) {
                invalid = true;
            }
            while self.peek().is_some_and(|value| value.is_ascii_digit()) {
                self.bump();
            }
        }

        if self
            .peek()
            .is_some_and(|value| value == '_' || is_xid_continue(value))
        {
            invalid = true;
            while self
                .peek()
                .is_some_and(|value| value == '_' || is_xid_continue(value))
            {
                self.bump();
            }
        }

        let raw = self.source[start..self.position].to_string();
        if invalid {
            self.diagnostic(
                LexDiagnosticKind::InvalidNumber,
                "numbers use unsigned decimal spelling without leading zeroes, separators, bases, or exponents",
                start,
                self.position,
            );
            return TokenKind::Invalid(raw);
        }
        if decimal {
            TokenKind::Decimal(raw)
        } else {
            TokenKind::Integer(raw)
        }
    }

    fn leading_dot_number(&mut self) -> TokenKind {
        let start = self.position;
        self.bump();
        while self.peek().is_some_and(|value| value.is_ascii_digit()) {
            self.bump();
        }
        self.diagnostic(
            LexDiagnosticKind::InvalidNumber,
            "decimal source requires digits on both sides of the dot",
            start,
            self.position,
        );
        TokenKind::Invalid(self.source[start..self.position].to_string())
    }

    fn text(&mut self) -> TokenKind {
        let start = self.position;
        self.bump();
        let mut decoded = String::new();
        let mut terminated = false;

        while let Some(value) = self.peek() {
            match value {
                '"' => {
                    self.bump();
                    terminated = true;
                    break;
                }
                '\\' => self.text_escape(&mut decoded),
                value if value <= '\u{001f}' => {
                    let invalid_start = self.position;
                    self.bump();
                    self.diagnostic(
                        LexDiagnosticKind::InvalidEscape,
                        "Text cannot contain an unescaped control character",
                        invalid_start,
                        self.position,
                    );
                }
                value => {
                    self.bump();
                    decoded.push(value);
                }
            }
        }

        if !terminated {
            self.diagnostic(
                LexDiagnosticKind::UnterminatedText,
                "unterminated Text literal",
                start,
                self.position,
            );
        }
        TokenKind::Text(decoded)
    }

    fn text_escape(&mut self, decoded: &mut String) {
        let start = self.position;
        self.bump();
        let Some(escape) = self.bump() else {
            self.diagnostic(
                LexDiagnosticKind::InvalidEscape,
                "incomplete Text escape",
                start,
                self.position,
            );
            return;
        };

        match escape {
            '"' => decoded.push('"'),
            '\\' => decoded.push('\\'),
            '/' => decoded.push('/'),
            'b' => decoded.push('\u{0008}'),
            'f' => decoded.push('\u{000c}'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            'u' => self.unicode_escape(start, decoded),
            _ => self.diagnostic(
                LexDiagnosticKind::InvalidEscape,
                "Text escapes follow JSON spelling",
                start,
                self.position,
            ),
        }
    }

    fn unicode_escape(&mut self, start: usize, decoded: &mut String) {
        let Some(first) = self.four_hex_digits() else {
            self.diagnostic(
                LexDiagnosticKind::InvalidUnicodeEscape,
                "Unicode Text escapes require exactly four hexadecimal digits",
                start,
                self.position,
            );
            return;
        };

        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if self.peek() != Some('\\') || self.peek_n(1) != Some('u') {
                self.diagnostic(
                    LexDiagnosticKind::InvalidSurrogatePair,
                    "a high-surrogate escape must be followed by an escaped low surrogate",
                    start,
                    self.position,
                );
                return;
            }
            self.bump();
            self.bump();
            let Some(second) = self.four_hex_digits() else {
                self.diagnostic(
                    LexDiagnosticKind::InvalidUnicodeEscape,
                    "Unicode Text escapes require exactly four hexadecimal digits",
                    start,
                    self.position,
                );
                return;
            };
            if !(0xdc00..=0xdfff).contains(&second) {
                self.diagnostic(
                    LexDiagnosticKind::InvalidSurrogatePair,
                    "a high-surrogate escape must be followed by an escaped low surrogate",
                    start,
                    self.position,
                );
                return;
            }
            0x1_0000 + (((first as u32 - 0xd800) << 10) | (second as u32 - 0xdc00))
        } else if (0xdc00..=0xdfff).contains(&first) {
            self.diagnostic(
                LexDiagnosticKind::InvalidSurrogatePair,
                "an isolated low-surrogate escape is invalid",
                start,
                self.position,
            );
            return;
        } else {
            first as u32
        };

        if let Some(value) = char::from_u32(scalar) {
            decoded.push(value);
        }
    }

    fn four_hex_digits(&mut self) -> Option<u16> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = self.peek()?.to_digit(16)? as u16;
            self.bump();
            value = (value << 4) | digit;
        }
        Some(value)
    }

    fn rest(&self) -> &'a str {
        &self.source[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_n(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.position += value.len_utf8();
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

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.file,
            start.min(u32::MAX as usize) as u32,
            end.min(u32::MAX as usize) as u32,
        )
    }

    fn diagnostic(
        &mut self,
        kind: LexDiagnosticKind,
        message: impl Into<String>,
        start: usize,
        end: usize,
    ) {
        self.diagnostics.push(LexDiagnostic {
            kind,
            message: message.into(),
            span: self.span(start, end),
        });
    }
}
