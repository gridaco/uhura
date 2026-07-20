//! `uhura check [path] [--emit-ir] [--deny-warnings] [--format=json]`.

use std::path::Path;
use std::process::ExitCode;

use uhura_base::{Diagnostic, FileId, Severity, Span, render_text, sha256_hex, to_envelope};

use crate::CommonArgs;

pub fn run(common: &CommonArgs) -> ExitCode {
    let mut project = match super::project::load(&common.root, "check") {
        Ok(project) => project,
        Err(code) => return code,
    };

    let has_static_errors = project
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    let evidence = if has_static_errors {
        None
    } else {
        project
            .program
            .as_ref()
            .map(uhura_core::Program::run_evidence)
    };
    if let Some(report) = &evidence {
        project
            .diagnostics
            .extend(report.failures.iter().map(|failure| {
                Diagnostic::new(
                    "R3013",
                    "uhura/evidence",
                    Severity::Error,
                    failure.message.clone(),
                    evidence_span(&failure.source, &project.files),
                )
            }));
    }
    project.diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
        )
    });

    let has_errors = project
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    if common.emit_ir && !has_errors {
        let Some(program) = project.program.as_ref() else {
            eprintln!("uhura check: clean source produced no machine program");
            return ExitCode::from(2);
        };
        if let Err(error) = emit_artifacts(&common.root, program, evidence.as_ref()) {
            eprintln!("uhura check: emit-ir failed: {error}");
            return ExitCode::from(2);
        }
    }

    if common.format_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&to_envelope(&project.diagnostics, &project.source_map))
                .expect("diagnostic envelope serializes")
        );
    } else if project.diagnostics.is_empty() {
        let examples = evidence
            .as_ref()
            .map_or(0, |report| report.artifacts.examples.len());
        println!(
            "checked {} Uhura module{} and {examples} example{}: clean",
            project.files.len(),
            if project.files.len() == 1 { "" } else { "s" },
            if examples == 1 { "" } else { "s" },
        );
    } else {
        print!("{}", render_text(&project.diagnostics, &project.source_map));
    }

    let failing = project.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            || (common.deny_warnings && diagnostic.severity == Severity::Warning)
    });
    if failing {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn evidence_span(source: &uhura_core::ir::SourceRef, files: &[crate::fsio::SourceFile]) -> Span {
    files
        .iter()
        .position(|file| file.rel_path == source.path)
        .filter(|index| {
            source.start <= source.end && source.end as usize <= files[*index].text.len()
        })
        .map_or_else(
            || Span::new(FileId(0), 0, 0),
            |file| Span::new(FileId(file as u32), source.start, source.end),
        )
}

fn emit_artifacts(
    root: &Path,
    program: &uhura_core::Program,
    evidence: Option<&uhura_core::EvidenceReport>,
) -> std::io::Result<()> {
    let build = root.join("build");
    std::fs::create_dir_all(&build)?;

    let ir = program.to_canonical_string();
    std::fs::write(build.join("ir.json"), &ir)?;
    let graph = uhura_core::build_interaction_graph_artifacts(program);
    std::fs::write(
        build.join("interaction-graph.json"),
        uhura_base::to_canonical_json(
            &serde_json::to_value(&graph.graph).expect("interaction graph serializes"),
        ),
    )?;
    std::fs::write(
        build.join("interaction-graph-sources.json"),
        uhura_base::to_canonical_json(
            &serde_json::to_value(&graph.provenance).expect("graph provenance serializes"),
        ),
    )?;
    if let Some(evidence) = evidence {
        std::fs::write(build.join("evidence.json"), evidence.to_canonical_string())?;
    }
    println!(
        "emitted build/ir.json ({} bytes, hash {}), interaction graph, and evidence",
        ir.len(),
        sha256_hex(ir.as_bytes())
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_diagnostics_retain_authored_source_spans() {
        let files = vec![
            crate::fsio::SourceFile {
                rel_path: "machine.uhura".into(),
                abs_path: "/project/machine.uhura".into(),
                text: "machine source".into(),
            },
            crate::fsio::SourceFile {
                rel_path: "nested/evidence.uhura".into(),
                abs_path: "/project/nested/evidence.uhura".into(),
                text: "0123456789abcdefghijklmnop".into(),
            },
        ];
        let source = uhura_core::ir::SourceRef {
            id: "module#declarations[2]".into(),
            path: "nested/evidence.uhura".into(),
            start: 7,
            end: 19,
        };
        let span = evidence_span(&source, &files);
        assert_eq!(span.file, FileId(1));
        assert_eq!((span.start, span.end), (7, 19));
    }
}
