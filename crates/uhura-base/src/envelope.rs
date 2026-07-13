//! The `uhura-diagnostics/0` JSON envelope — the one stable shape editors
//! and downstream tools integrate against (design §12.4) — plus a plain-text
//! renderer for terminals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::span::{SourceMap, Span};

fn span_json(sm: &SourceMap, span: Span) -> serde_json::Value {
    let start = sm.line_col(span.file, span.start);
    let end = sm.line_col(span.file, span.end);
    serde_json::json!({
        "offset": span.start,
        "len": span.len(),
        "start": { "line": start.line, "col": start.col },
        "end": { "line": end.line, "col": end.col },
    })
}

/// Serializes diagnostics to the versioned envelope.
pub fn to_envelope(diags: &[Diagnostic], sm: &SourceMap) -> serde_json::Value {
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    let list: Vec<serde_json::Value> = diags
        .iter()
        .map(|d| {
            let mut obj = serde_json::json!({
                "code": d.code,
                "rule": d.rule,
                "severity": d.severity.as_str(),
                "message": d.message,
                "file": sm.path(d.span.file),
                "span": span_json(sm, d.span),
            });
            if !d.labels.is_empty() {
                obj["labels"] = d
                    .labels
                    .iter()
                    .map(|l| {
                        serde_json::json!({
                            "file": sm.path(l.span.file),
                            "span": span_json(sm, l.span),
                            "message": l.message,
                        })
                    })
                    .collect();
            }
            if !d.notes.is_empty() {
                obj["notes"] = serde_json::json!(d.notes);
            }
            if let Some(fix) = &d.fix {
                obj["fix"] = serde_json::json!({
                    "title": fix.title,
                    "edits": fix.edits.iter().map(|e| serde_json::json!({
                        "file": sm.path(e.span.file),
                        "offset": e.span.start,
                        "len": e.span.len(),
                        "insert": e.insert,
                    })).collect::<Vec<_>>(),
                });
            }
            obj
        })
        .collect();

    serde_json::json!({
        "format": "uhura-diagnostics",
        "version": 0,
        "summary": { "errors": errors, "warnings": warnings },
        "diagnostics": list,
    })
}

/// Human-readable terminal rendering (one block per diagnostic).
pub fn render_text(diags: &[Diagnostic], sm: &SourceMap) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for d in diags {
        let lc = sm.line_col(d.span.file, d.span.start);
        let _ = writeln!(
            out,
            "{}[{} {}]: {}",
            d.severity.as_str(),
            d.code,
            d.rule,
            d.message
        );
        let _ = writeln!(out, "  --> {}:{}:{}", sm.path(d.span.file), lc.line, lc.col);
        for l in &d.labels {
            let llc = sm.line_col(l.span.file, l.span.start);
            let _ = writeln!(
                out,
                "  = {} ({}:{}:{})",
                l.message,
                sm.path(l.span.file),
                llc.line,
                llc.col
            );
        }
        for n in &d.notes {
            let _ = writeln!(out, "  note: {n}");
        }
        if let Some(fix) = &d.fix {
            let _ = writeln!(out, "  fix: {}", fix.title);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Edit;
    use crate::span::SourceMap;

    #[test]
    fn envelope_shape() {
        let mut sm = SourceMap::new();
        let f = sm.add(
            "components/post-card.uhura",
            "component post-card\n<view>\n",
        );
        let d = Diagnostic::error(
            "UH0301",
            "markup/unkeyed-each",
            "`{#each}` has no key",
            Span::new(f, 20, 26),
        )
        .with_note("repeated content requires a stable key")
        .with_fix(
            "add a key",
            vec![Edit {
                span: Span::new(f, 26, 26),
                insert: " (s.id)".into(),
            }],
        );
        let env = to_envelope(&[d], &sm);
        assert_eq!(env["format"], "uhura-diagnostics");
        assert_eq!(env["version"], 0);
        assert_eq!(env["summary"]["errors"], 1);
        let diag = &env["diagnostics"][0];
        assert_eq!(diag["code"], "UH0301");
        assert_eq!(diag["file"], "components/post-card.uhura");
        assert_eq!(diag["span"]["start"]["line"], 2);
        assert_eq!(diag["fix"]["edits"][0]["offset"], 26);
    }
}
