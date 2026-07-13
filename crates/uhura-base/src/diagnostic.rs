//! Structured diagnostics (design §12.4): stable `UHnxxx` code + human
//! `rule` slug, primary span, labeled secondary spans, notes, and an
//! optional mechanical fix (emitted only when the edit is safe).

use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// A secondary span with its own message (possibly in another file).
#[derive(Clone, Debug)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

/// One text edit of a fix; an empty span is a pure insertion.
#[derive(Clone, Debug)]
pub struct Edit {
    pub span: Span,
    pub insert: String,
}

/// A safe, mechanical repair. Never emitted for semantic choices
/// (e.g. "wrap in region" is a note, not a fix — design §4.8).
#[derive(Clone, Debug)]
pub struct Fix {
    pub title: String,
    pub edits: Vec<Edit>,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Stable registry code, e.g. `UH0301`.
    pub code: &'static str,
    /// Human-readable rule slug, e.g. `markup/unkeyed-each`.
    pub rule: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Primary location.
    pub span: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        rule: &'static str,
        severity: Severity,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Diagnostic {
            code,
            rule,
            severity,
            message: message.into(),
            span,
            labels: Vec::new(),
            notes: Vec::new(),
            fix: None,
        }
    }

    pub fn error(
        code: &'static str,
        rule: &'static str,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(code, rule, Severity::Error, message, span)
    }

    pub fn warning(
        code: &'static str,
        rule: &'static str,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(code, rule, Severity::Warning, message, span)
    }

    pub fn info(
        code: &'static str,
        rule: &'static str,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(code, rule, Severity::Info, message, span)
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_fix(mut self, title: impl Into<String>, edits: Vec<Edit>) -> Self {
        self.fix = Some(Fix {
            title: title.into(),
            edits,
        });
        self
    }
}

/// True if any diagnostic is an error (the "lowering is gated" predicate).
pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}
