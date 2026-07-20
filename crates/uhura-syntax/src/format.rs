//! Conservative deterministic Uhura formatter.
//!
//! Uhura 0.3 deliberately treats comments and UI text as non-semantic but
//! author-visible source.  Until the candidate selects a comment attachment
//! model, the source layer preserves their placement and canonicalises only
//! line endings, trailing horizontal whitespace, and the final newline.  The
//! result is deterministic and idempotent without fabricating declaration or
//! transaction structure.

use super::ast::Module;

pub fn format(module: &Module) -> String {
    format_source(&module.source)
}

pub fn format_source(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(normalized.len().saturating_add(1));
    for line in normalized.lines() {
        output.push_str(line.trim_end_matches([' ', '\t']));
        output.push('\n');
    }
    if normalized.is_empty() {
        return String::new();
    }
    while output.ends_with("\n\n") && normalized.ends_with('\n') && !normalized.ends_with("\n\n") {
        output.pop();
    }
    output
}
