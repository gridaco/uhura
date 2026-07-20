use uhura_base::{Diagnostic, Severity, Span};
use uhura_syntax::ast::SourceSpan;

/// Stable Uhura 0.3 diagnostic registry, implemented by the  canonical machine checker.
///
/// Multiple rules intentionally share one preregistered primary family. The
/// rule string remains the finer editor-facing discriminator; the code is the
/// compatibility boundary frozen by `implementation-gates.md`.
pub mod codes {
    pub const HEADER: &str = "R1002";
    pub const MODULE: &str = "R1002";
    pub const IMPORT: &str = "R1003";
    pub const FEATURE: &str = "R1002";
    pub const DUPLICATE: &str = "R1002";
    pub const UNKNOWN_NAME: &str = "R1003";
    pub const UNKNOWN_TYPE: &str = "R1003";
    pub const ARITY: &str = "R1004";
    pub const TYPE_MISMATCH: &str = "R1004";
    pub const INVALID_REFINEMENT: &str = "R1005";
    pub const NOT_EXHAUSTIVE: &str = "R1006";
    pub const INPUT_COVERAGE: &str = "R1007";
    pub const EFFECT: &str = "R1008";
    pub const DEPENDENCY_CYCLE: &str = "R1009";
    pub const TERMINATION: &str = "R1010";
    pub const NOT_TOTAL: &str = "R1011";
    pub const PARTIAL_OPERATION: &str = "R1011";
    pub const OUTCOME: &str = "R1012";
    pub const TRANSITION_SHAPE: &str = "R1012";
    pub const INVARIANT: &str = "R1013";
    pub const PROJECTION_NOT_TOTAL: &str = "R1013";
    pub const PORT: &str = "R1004";
    pub const UI: &str = "R3006";
    pub const EVIDENCE: &str = "R1004";
    pub const UI_NOT_ENABLED: &str = "R3001";
    pub const EVIDENCE_NOT_ENABLED: &str = "R3011";
    pub const ROUTE_CODEC_MISMATCH: &str = "R3012";
    pub const UNSUPPORTED: &str = "R1002";
}

pub fn error(
    code: &'static str,
    rule: &'static str,
    message: impl Into<String>,
    span: SourceSpan,
) -> Diagnostic {
    Diagnostic::new(code, rule, Severity::Error, message, as_span(span))
}

pub fn warning(
    code: &'static str,
    rule: &'static str,
    message: impl Into<String>,
    span: SourceSpan,
) -> Diagnostic {
    Diagnostic::new(code, rule, Severity::Warning, message, as_span(span))
}

pub(crate) fn as_span(span: SourceSpan) -> Span {
    Span::new(uhura_base::FileId(span.file), span.start, span.end)
}
