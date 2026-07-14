//! `uhura graph [path] [--out=<file>]` — checked IR → deterministic,
//! renderer-neutral interaction graph. The graph is a derived read model for
//! NCC and other visualizers; Uhura remains the only owner of UI semantics.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use uhura_base::{Severity, render_text, to_canonical_json};
use uhura_check::check;
use uhura_check::lower::SpanEntry;
use uhura_editor_model::interaction_graph::{
    SourceRef, SpanLookup, build_interaction_graph_with_spans,
};

use crate::CommonArgs;

struct CheckSpans<'a>(&'a BTreeMap<String, SpanEntry>);

impl SpanLookup for CheckSpans<'_> {
    fn source_ref(&self, ir_path: &str) -> Option<SourceRef> {
        self.0.get(ir_path).map(|span| SourceRef {
            file: span.file.clone(),
            start: span.start,
            end: span.end,
            ir_path: ir_path.to_string(),
        })
    }
}

pub fn run(common: &CommonArgs, out: Option<&str>) -> ExitCode {
    let input = match super::assemble_input(&common.root) {
        Ok(input) => input,
        Err(code) => return code,
    };
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        print!("{}", render_text(&output.diagnostics, &output.source_map));
        eprintln!("uhura graph: the check must come up clean first");
        return ExitCode::from(1);
    }
    let Some(lowered) = &output.lowered else {
        eprintln!("uhura graph: no checked program");
        return ExitCode::from(1);
    };

    let graph = build_interaction_graph_with_spans(&lowered.program, &CheckSpans(&lowered.spans));
    let value = serde_json::to_value(&graph).expect("interaction graph serializes");
    let mut json = to_canonical_json(&value);
    json.push('\n');

    let path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| common.root.join("build/interaction-graph.json"));
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("uhura graph: {}: {e}", parent.display());
        return ExitCode::from(2);
    }
    if let Err(e) = std::fs::write(&path, json.as_bytes()) {
        eprintln!("uhura graph: {}: {e}", path.display());
        return ExitCode::from(2);
    }
    println!(
        "wrote {} ({} nodes, {} edges)",
        path.display(),
        graph.nodes.len(),
        graph.edges.len()
    );
    ExitCode::SUCCESS
}
