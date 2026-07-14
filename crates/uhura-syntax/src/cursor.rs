//! The character cursor shared by every surface parser, plus the DSL
//! tokenizer. Mode ownership is structural: parsers call the tokenizer
//! function for the surface they are in (design §4, plan risk #1), so a
//! token can never be lexed in the wrong mode.

use uhura_base::{Diagnostic, FileId, Span, codes};

use crate::token::{Comment, CommentKind, Token, TokenKind};

pub struct Cursor<'src> {
    pub file: FileId,
    text: &'src str,
    pos: u32,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'src> Cursor<'src> {
    pub fn new(file: FileId, text: &'src str) -> Self {
        Cursor {
            file,
            text,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn pos(&self) -> u32 {
        self.pos
    }

    /// Rewind/seek — used by parsers to resync after speculative reads.
    pub fn set_pos(&mut self, pos: u32) {
        debug_assert!(pos as usize <= self.text.len());
        self.pos = pos;
    }

    pub fn is_eof(&self) -> bool {
        self.pos as usize >= self.text.len()
    }

    pub fn rest(&self) -> &'src str {
        &self.text[self.pos as usize..]
    }

    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub fn peek2(&self) -> Option<char> {
        let mut it = self.rest().chars();
        it.next();
        it.next()
    }

    pub fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8() as u32;
        Some(c)
    }

    pub fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn eat_str(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.pos += s.len() as u32;
            true
        } else {
            false
        }
    }

    pub fn span_from(&self, start: u32) -> Span {
        Span::new(self.file, start, self.pos)
    }

    /// The text consumed since `start`, as an owned string.
    pub fn rest_from(&self, start: u32) -> String {
        self.text[start as usize..self.pos as usize].to_string()
    }

    pub fn error(&mut self, code: codes::Code, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code.0, code.1, message, span));
    }

    /// Skips whitespace and `//` comments, returning the comments in order.
    pub fn skip_trivia(&mut self) -> Vec<Comment> {
        let mut comments = Vec::new();
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek2() == Some('/') => {
                    let start = self.pos;
                    self.bump();
                    self.bump();
                    let kind = if self.peek() == Some('!') {
                        self.bump();
                        CommentKind::InnerDoc
                    } else if self.peek() == Some('/') {
                        self.bump();
                        if self.peek() == Some('/') {
                            // Four or more slashes are ordinary. Put the
                            // third slash back into the body logically.
                            self.pos -= 1;
                            CommentKind::Ordinary
                        } else {
                            CommentKind::OuterDoc
                        }
                    } else {
                        CommentKind::Ordinary
                    };
                    let text_start = self.pos as usize;
                    while let Some(c) = self.peek() {
                        if c == '\n' || c == '\r' {
                            break;
                        }
                        self.bump();
                    }
                    comments.push(Comment {
                        span: self.span_from(start),
                        kind,
                        text: self.text[text_start..self.pos as usize].to_string(),
                    });
                }
                _ => break,
            }
        }
        comments
    }

    // ── DSL tokenizer ──────────────────────────────────────────────────────

    /// Lexes one DSL token (header / store / expression surfaces).
    pub fn dsl_token(&mut self) -> Token {
        self.dsl_token_mode(false)
    }

    /// Module-level DSL lexing stops before an XML-shaped markup comment so
    /// the file driver can perform the DSL-to-markup transition first.
    pub(crate) fn module_dsl_token(&mut self) -> Token {
        self.dsl_token_mode(true)
    }

    fn dsl_token_mode(&mut self, allow_markup_transition: bool) -> Token {
        let leading = self.skip_trivia();
        let start = self.pos;
        let kind = if allow_markup_transition && self.rest().starts_with("<!--") {
            // Consume only for lexer progress. DslStream::finish rewinds to
            // this token's start before the markup parser takes ownership.
            self.bump();
            TokenKind::Lt
        } else {
            self.dsl_token_kind(start)
        };
        Token {
            kind,
            span: self.span_from(start),
            leading,
        }
    }

    fn dsl_token_kind(&mut self, start: u32) -> TokenKind {
        if self.rest().starts_with("<!--") {
            return self.lex_disallowed_markup_comment(start);
        }
        let Some(c) = self.bump() else {
            return TokenKind::Eof;
        };
        match c {
            'a'..='z' => {
                // Kebab identifier: `-` joins iff immediately between
                // ident-continue characters (micro-decision #1).
                while let Some(n) = self.peek() {
                    match n {
                        'a'..='z' | '0'..='9' => {
                            self.bump();
                        }
                        '-' if matches!(self.peek2(), Some('a'..='z' | '0'..='9')) => {
                            self.bump();
                        }
                        _ => break,
                    }
                }
                let text = &self.text[start as usize..self.pos as usize];
                if text.len() > 64 {
                    self.error(
                        codes::INVALID_IDENT,
                        format!("identifier `{text}` exceeds 64 characters"),
                        self.span_from(start),
                    );
                }
                TokenKind::Ident(text.to_string())
            }
            '0'..='9' => {
                while matches!(self.peek(), Some('0'..='9')) {
                    self.bump();
                }
                let text = &self.text[start as usize..self.pos as usize];
                match text.parse::<i64>() {
                    Ok(i) => TokenKind::Int(i),
                    Err(_) => {
                        self.error(
                            codes::UNEXPECTED_TOKEN,
                            format!("integer literal `{text}` overflows i64"),
                            self.span_from(start),
                        );
                        TokenKind::Int(i64::MAX)
                    }
                }
            }
            '"' => self.lex_string(start),
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,
            '=' => {
                if self.eat('=') {
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.eat('=') {
                    TokenKind::NotEq
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.eat('=') {
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.eat('=') {
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }
            '+' => {
                if self.eat('+') {
                    TokenKind::PlusPlus
                } else {
                    TokenKind::Plus
                }
            }
            '-' => TokenKind::Minus,
            '&' => {
                if self.eat('&') {
                    TokenKind::AndAnd
                } else {
                    self.error(
                        codes::UNEXPECTED_TOKEN,
                        "single `&` — did you mean `&&`?",
                        self.span_from(start),
                    );
                    TokenKind::Error
                }
            }
            '|' => {
                if self.eat('|') {
                    TokenKind::OrOr
                } else {
                    self.error(
                        codes::UNEXPECTED_TOKEN,
                        "single `|` — did you mean `||`?",
                        self.span_from(start),
                    );
                    TokenKind::Error
                }
            }
            '?' => {
                if self.eat('?') {
                    TokenKind::Coalesce
                } else {
                    TokenKind::Question
                }
            }
            other => {
                self.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("unexpected character `{other}`"),
                    self.span_from(start),
                );
                TokenKind::Error
            }
        }
    }

    /// XML-shaped comments are recognized as one recovery token even in DSL
    /// mode so a well-formed carrier gets UH0001 while malformed XML/markers
    /// retain UH0016 precedence.
    fn lex_disallowed_markup_comment(&mut self, start: u32) -> TokenKind {
        self.eat_str("<!--");
        let body_start = self.pos;
        let Some(close) = self.rest().find("-->") else {
            let recovery = self.rest().find('}').unwrap_or(self.rest().len());
            self.set_pos(body_start + recovery as u32);
            self.error(
                codes::MALFORMED_MARKUP_COMMENT,
                "unterminated markup comment",
                self.span_from(start),
            );
            return TokenKind::Error;
        };
        let body = self.rest()[..close].to_string();
        self.set_pos(body_start + close as u32);
        self.eat_str("-->");
        let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
        let malformed_xml = body.contains("--") || body.ends_with('-');
        let malformed_marker = malformed_annotation_marker(&normalized);
        if malformed_xml || malformed_marker {
            self.error(
                codes::MALFORMED_MARKUP_COMMENT,
                "malformed XML-shaped comment or annotation marker",
                self.span_from(start),
            );
        } else {
            self.error(
                codes::UNEXPECTED_TOKEN,
                "XML-shaped comments are only legal at markup sibling positions",
                self.span_from(start),
            );
        }
        TokenKind::Error
    }

    fn lex_string(&mut self, start: u32) -> TokenKind {
        let mut out = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    self.error(
                        codes::UNTERMINATED_STRING,
                        "unterminated string literal (no raw newlines in strings)",
                        self.span_from(start),
                    );
                    return TokenKind::Str(out);
                }
                Some('"') => {
                    self.bump();
                    return TokenKind::Str(out);
                }
                Some('\\') => {
                    self.bump();
                    match self.bump() {
                        Some('"') => out.push('"'),
                        Some('\\') => out.push('\\'),
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('u') => {
                            if self.eat('{') {
                                let hex_start = self.pos as usize;
                                while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                                    self.bump();
                                }
                                let hex = &self.text[hex_start..self.pos as usize];
                                let ok = self.eat('}');
                                match (
                                    ok,
                                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32),
                                ) {
                                    (true, Some(c)) => out.push(c),
                                    _ => self.error(
                                        codes::UNTERMINATED_STRING,
                                        "invalid `\\u{…}` escape",
                                        self.span_from(start),
                                    ),
                                }
                            } else {
                                self.error(
                                    codes::UNTERMINATED_STRING,
                                    "`\\u` escape requires `{hex}`",
                                    self.span_from(start),
                                );
                            }
                        }
                        other => {
                            self.error(
                                codes::UNTERMINATED_STRING,
                                format!(
                                    "unknown escape `\\{}`",
                                    other.map(String::from).unwrap_or_default()
                                ),
                                self.span_from(start),
                            );
                        }
                    }
                }
                Some(_) => {
                    out.push(self.bump().unwrap());
                }
            }
        }
    }
}

fn malformed_annotation_marker(body: &str) -> bool {
    let body = body.trim_start_matches([' ', '\t', '\n']);
    let Some(marker) = body.strip_prefix('@') else {
        return false;
    };
    let Some((kind_end, separator)) = marker
        .char_indices()
        .find(|(_, ch)| matches!(ch, ' ' | '\t' | '\n'))
    else {
        return true;
    };
    let kind = &marker[..kind_end];
    let payload = &marker[kind_end + separator.len_utf8()..];
    !valid_annotation_kind(kind) || payload.trim_matches([' ', '\t', '\n']).is_empty()
}

fn valid_annotation_kind(kind: &str) -> bool {
    if kind.is_empty() || kind.len() > 64 || !kind.is_ascii() {
        return false;
    }
    let bytes = kind.as_bytes();
    if !bytes[0].is_ascii_lowercase() || bytes.last() == Some(&b'-') {
        return false;
    }
    let mut previous_dash = false;
    for byte in bytes {
        if *byte == b'-' {
            if previous_dash {
                return false;
            }
            previous_dash = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_dash = false;
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind as T;

    fn lex_all(src: &str) -> Vec<TokenKind> {
        let mut c = Cursor::new(FileId(0), src);
        let mut out = Vec::new();
        loop {
            let t = c.dsl_token();
            let eof = t.kind == T::Eof;
            out.push(t.kind);
            if eof {
                break;
            }
        }
        out.pop(); // drop Eof
        out
    }

    #[test]
    fn kebab_vs_minus() {
        assert_eq!(
            lex_all("like-pending"),
            vec![T::Ident("like-pending".into())]
        );
        assert_eq!(lex_all("0 - 1"), vec![T::Int(0), T::Minus, T::Int(1)]);
        // `a -b` is subtraction: `-` starts a fresh token after whitespace.
        assert_eq!(
            lex_all("a -b"),
            vec![T::Ident("a".into()), T::Minus, T::Ident("b".into())]
        );
        // `a- b`: the dash is not followed by an ident char, so it detaches.
        assert_eq!(
            lex_all("a- b"),
            vec![T::Ident("a".into()), T::Minus, T::Ident("b".into())]
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            lex_all("a ?? b != c ++ \"x\" && !d"),
            vec![
                T::Ident("a".into()),
                T::Coalesce,
                T::Ident("b".into()),
                T::NotEq,
                T::Ident("c".into()),
                T::PlusPlus,
                T::Str("x".into()),
                T::AndAnd,
                T::Bang,
                T::Ident("d".into()),
            ]
        );
    }

    #[test]
    fn string_escapes() {
        assert_eq!(
            lex_all(r#""a\n\"b\" \u{e9}""#),
            vec![T::Str("a\n\"b\" é".into())]
        );
    }

    #[test]
    fn comments_are_leading_trivia() {
        let mut c = Cursor::new(FileId(0), "// hi\n// there\nset");
        let t = c.dsl_token();
        assert_eq!(t.kind, T::Ident("set".into()));
        assert_eq!(t.leading.len(), 2);
        assert_eq!(t.leading[0].text, " hi");
    }

    #[test]
    fn unterminated_string_diagnoses() {
        let mut c = Cursor::new(FileId(0), "\"abc\nx");
        let t = c.dsl_token();
        assert!(matches!(t.kind, T::Str(_)));
        assert_eq!(c.diagnostics.len(), 1);
        assert_eq!(c.diagnostics[0].code, "UH0002");
    }
}
