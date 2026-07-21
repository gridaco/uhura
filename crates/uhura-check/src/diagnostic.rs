use crate::checker_ir::SourceSpan;
use uhura_base::{Diagnostic, Severity, Span};

/// Compatibility-preserving machine diagnostic families, centralized in
/// `uhura-base` so syntax, checker, CLI, and host do not grow parallel
/// registries.
pub use uhura_base::codes::machine as codes;

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
