//! A DSL token stream over the shared cursor, with bounded lookahead and
//! position resync — so a parser can leave DSL mode (say, at the `}` closing
//! an interpolation) without ever having lexed markup text as DSL tokens.

use uhura_base::{Diagnostic, Span, codes};

use crate::ast::{DocForm, DslTrivia};
use crate::cursor::Cursor;
use crate::token::{CommentKind, Token, TokenKind};

pub struct DslStream<'a, 'src> {
    pub cur: &'a mut Cursor<'src>,
    buf: Vec<Token>,
    /// End offset of the last consumed token (resync point).
    last_end: u32,
    allow_markup_transition: bool,
}

impl<'a, 'src> DslStream<'a, 'src> {
    pub fn new(cur: &'a mut Cursor<'src>) -> Self {
        let last_end = cur.pos();
        DslStream {
            cur,
            buf: Vec::new(),
            last_end,
            allow_markup_transition: false,
        }
    }

    pub fn new_module(cur: &'a mut Cursor<'src>) -> Self {
        let mut stream = Self::new(cur);
        stream.allow_markup_transition = true;
        stream
    }

    fn fill(&mut self, n: usize) {
        while self.buf.len() < n {
            let mut t = if self.allow_markup_transition {
                self.cur.module_dsl_token()
            } else {
                self.cur.dsl_token()
            };
            if t.kind == TokenKind::Error {
                // Lexical errors already own their diagnostic. Keeping the
                // token in lookahead lets parser expectations diagnose it a
                // second time and `finish` rewind it into another source mode.
                // Consume it exactly once while still checking any leading
                // comment placement it carried.
                if !t.leading.is_empty() {
                    let trivia = DslTrivia::new(std::mem::take(&mut t.leading));
                    self.diagnose_unclaimed(&trivia, t.span);
                }
                self.last_end = t.span.end;
                continue;
            }
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
        let mut t = self.buf.remove(0);
        if !t.leading.is_empty() {
            let trivia = DslTrivia::new(std::mem::take(&mut t.leading));
            self.diagnose_unclaimed(&trivia, t.span);
        }
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
    pub fn take_leading(&mut self) -> DslTrivia {
        self.fill(1);
        DslTrivia::new(std::mem::take(&mut self.buf[0].leading))
    }

    /// Accept outer docs on a documentable target. Inner docs have file-doc
    /// precedence, and a second non-empty run for the same target is invalid.
    pub fn accept_outer_docs(&mut self, trivia: &DslTrivia, target: Span) {
        let mut first = None;
        for doc in &trivia.docs {
            match doc.form {
                DocForm::Inner => self.misplaced_inner(doc.span),
                DocForm::Outer => {
                    if let Some(first_span) = first {
                        self.incompatible_doc(
                            doc.span,
                            target,
                            "a declaration has at most one documentation run",
                            Some(first_span),
                        );
                    } else {
                        first = Some(doc.span);
                    }
                }
            }
        }
    }

    /// Validate the file preamble. Inner and outer docs have different
    /// targets there, so singularity is checked independently by form.
    pub fn accept_preamble_docs(&mut self, trivia: &DslTrivia, header: Span) {
        let mut first_inner = None;
        let mut first_outer = None;
        for doc in &trivia.docs {
            let first = match doc.form {
                DocForm::Inner => &mut first_inner,
                DocForm::Outer => &mut first_outer,
            };
            if let Some(first_span) = *first {
                self.incompatible_doc(
                    doc.span,
                    header,
                    "documentation is singular for its source target",
                    Some(first_span),
                );
            } else {
                *first = Some(doc.span);
            }
        }
    }

    /// Validate trivia before the first item of an examples file when that
    /// item is not itself documentable (for example, `use fixture`).
    pub fn accept_file_docs_only(&mut self, trivia: &DslTrivia, target: Span) {
        let mut first_inner = None;
        for doc in &trivia.docs {
            match doc.form {
                DocForm::Inner => {
                    if let Some(first) = first_inner {
                        self.incompatible_doc(
                            doc.span,
                            target,
                            "source-module documentation is singular",
                            Some(first),
                        );
                    } else {
                        first_inner = Some(doc.span);
                    }
                }
                DocForm::Outer => self.incompatible_doc(
                    doc.span,
                    target,
                    "documentation cannot target this source construct",
                    None,
                ),
            }
        }
    }

    /// Validate an examples-file preamble which reaches EOF. Inner docs still
    /// target the source module; outer docs have no declaration to target.
    pub fn accept_file_docs_at_eof(&mut self, trivia: &DslTrivia, eof: Span) {
        let mut first_inner = None;
        for doc in &trivia.docs {
            match doc.form {
                DocForm::Inner => {
                    if let Some(first) = first_inner {
                        self.incompatible_doc(
                            doc.span,
                            eof,
                            "source-module documentation is singular",
                            Some(first),
                        );
                    } else {
                        first_inner = Some(doc.span);
                    }
                }
                DocForm::Outer => {
                    self.cur.diagnostics.push(
                        Diagnostic::error(
                            codes::DANGLING_METADATA.0,
                            codes::DANGLING_METADATA.1,
                            "documentation reaches end of file without a target",
                            doc.span,
                        )
                        .with_label(eof, "end of file"),
                    );
                }
            }
        }
    }

    /// A complete construct exists at this legal trivia boundary, but it is
    /// not documentable.
    pub fn reject_docs(&mut self, trivia: &DslTrivia, target: Span) {
        for doc in &trivia.docs {
            match doc.form {
                DocForm::Inner => self.misplaced_inner(doc.span),
                DocForm::Outer => self.incompatible_doc(
                    doc.span,
                    target,
                    "documentation cannot target this source construct",
                    None,
                ),
            }
        }
    }

    /// A list/region boundary was reached without a source construct.
    pub fn reject_boundary_docs(&mut self, trivia: &DslTrivia, boundary: Span) {
        for doc in &trivia.docs {
            match doc.form {
                DocForm::Inner => self.misplaced_inner(doc.span),
                DocForm::Outer => {
                    self.cur.diagnostics.push(
                        Diagnostic::error(
                            codes::DANGLING_METADATA.0,
                            codes::DANGLING_METADATA.1,
                            "documentation reaches a scope boundary without a target",
                            doc.span,
                        )
                        .with_label(boundary, "the documentation cannot cross this boundary"),
                    );
                }
            }
        }
    }

    fn diagnose_unclaimed(&mut self, trivia: &DslTrivia, token: Span) {
        // Ordinary comments are legal only at parser-owned complete item/list
        // boundaries. Empty doc runs are intentional no-ops.
        for piece in &trivia.pieces {
            if piece.kind == CommentKind::Ordinary {
                self.cur.diagnostics.push(
                    Diagnostic::error(
                        codes::UNEXPECTED_TOKEN.0,
                        codes::UNEXPECTED_TOKEN.1,
                        "ordinary comments are only allowed at complete item or list boundaries",
                        piece.span,
                    )
                    .with_label(token, "this comment occurs inside a source construct")
                    .with_note("move the comment before the nearest complete item or list close"),
                );
            }
        }
        for doc in &trivia.docs {
            match doc.form {
                DocForm::Inner => self.misplaced_inner(doc.span),
                DocForm::Outer => self.incompatible_doc(
                    doc.span,
                    token,
                    "documentation occurs inside a source construct",
                    None,
                ),
            }
        }
    }

    fn misplaced_inner(&mut self, span: Span) {
        self.cur.error(
            codes::MISPLACED_INNER_DOC,
            "file documentation (`//!`) is only legal in the file preamble",
            span,
        );
    }

    fn incompatible_doc(&mut self, span: Span, target: Span, message: &str, first: Option<Span>) {
        let mut diagnostic = Diagnostic::error(
            codes::INCOMPATIBLE_METADATA_TARGET.0,
            codes::INCOMPATIBLE_METADATA_TARGET.1,
            message,
            span,
        )
        .with_label(target, "incompatible source target");
        if let Some(first) = first {
            diagnostic = diagnostic.with_label(first, "the first documentation run is here");
        }
        self.cur.diagnostics.push(diagnostic);
    }

    /// Ends DSL mode: rewinds the cursor to just after the last consumed
    /// token so unconsumed lookahead (and its trivia) re-lexes in whatever
    /// mode the caller enters next.
    pub fn finish(self) {
        let resume = if self.buf.is_empty() {
            self.last_end
        } else {
            let token = &self.buf[0];
            token
                .leading
                .first()
                .map_or(token.span.start, |comment| comment.span.start)
        };
        self.cur.set_pos(resume);
    }
}
