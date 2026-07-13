//! A DSL token stream over the shared cursor, with bounded lookahead and
//! position resync — so a parser can leave DSL mode (say, at the `}` closing
//! an interpolation) without ever having lexed markup text as DSL tokens.

use uhura_base::{Span, codes};

use crate::cursor::Cursor;
use crate::token::{Comment, Token, TokenKind};

pub struct DslStream<'a, 'src> {
    pub cur: &'a mut Cursor<'src>,
    buf: Vec<Token>,
    /// End offset of the last consumed token (resync point).
    last_end: u32,
}

impl<'a, 'src> DslStream<'a, 'src> {
    pub fn new(cur: &'a mut Cursor<'src>) -> Self {
        let last_end = cur.pos();
        DslStream {
            cur,
            buf: Vec::new(),
            last_end,
        }
    }

    fn fill(&mut self, n: usize) {
        while self.buf.len() < n {
            let t = self.cur.dsl_token();
            self.buf.push(t);
        }
    }

    pub fn peek(&mut self) -> &TokenKind {
        self.fill(1);
        &self.buf[0].kind
    }

    /// Two-token lookahead (kept for later parser phases).
    #[allow(dead_code)]
    pub fn peek2(&mut self) -> &TokenKind {
        self.fill(2);
        &self.buf[1].kind
    }

    pub fn peek_token(&mut self) -> &Token {
        self.fill(1);
        &self.buf[0]
    }

    pub fn bump(&mut self) -> Token {
        self.fill(1);
        let t = self.buf.remove(0);
        self.last_end = t.span.end;
        t
    }

    /// Consumes the next token if it is exactly `kind`.
    pub fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consumes the next token if it is the ident `text`.
    pub fn eat_ident(&mut self, text: &str) -> bool {
        if self.peek().is_ident(text) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Expects `kind`; on mismatch diagnoses and returns `None` without
    /// consuming (the caller decides how to recover).
    pub fn expect(&mut self, kind: &TokenKind, context: &str) -> Option<Token> {
        if self.peek() == kind {
            Some(self.bump())
        } else {
            let found = self.peek_token();
            let (span, desc) = (found.span, found.kind.describe());
            self.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("expected {} {context}, found {desc}", kind.describe()),
                span,
            );
            None
        }
    }

    /// Expects any identifier; returns its text.
    pub fn expect_ident(&mut self, context: &str) -> Option<(String, Span)> {
        if let TokenKind::Ident(_) = self.peek() {
            let t = self.bump();
            let TokenKind::Ident(s) = t.kind else {
                unreachable!()
            };
            Some((s, t.span))
        } else {
            let found = self.peek_token();
            let (span, desc) = (found.span, found.kind.describe());
            self.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("expected an identifier {context}, found {desc}"),
                span,
            );
            None
        }
    }

    pub fn peek_span(&mut self) -> Span {
        self.peek_token().span
    }

    /// Leading comments of the next token (drains them).
    pub fn take_leading(&mut self) -> Vec<Comment> {
        self.fill(1);
        std::mem::take(&mut self.buf[0].leading)
    }

    /// Ends DSL mode: rewinds the cursor to just after the last consumed
    /// token so unconsumed lookahead (and its trivia) re-lexes in whatever
    /// mode the caller enters next.
    pub fn finish(self) {
        self.cur.set_pos(self.last_end);
    }
}
