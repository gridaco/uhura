//! `uhura graph [path] [--out=<file>]` — canonical checked interaction graph.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::CommonArgs;

pub fn run(common: &CommonArgs, out: Option<&str>) -> ExitCode {
    let program = match super::project::require_program(&common.root, "graph") {
        Ok(program) => program,
        Err(code) => return code,
    };
    let artifacts = uhura_core::build_interaction_graph_artifacts(&program);
    if let Some((identity, hash)) = artifacts
        .graph
        .machine_program_hashes
        .iter()
        .chain(artifacts.graph.presentation_hashes.iter())
        .find(|(_, hash)| !is_lower_sha256(hash))
    {
        eprintln!("uhura graph: identity `{identity}` has invalid semantic hash `{hash}`");
        return ExitCode::from(2);
    }
    let mut json = uhura_base::to_canonical_json(
        &serde_json::to_value(&artifacts.graph).expect("interaction graph serializes"),
    );
    json.push('\n');
    let path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| common.root.join("build/interaction-graph.json"));
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("uhura graph: {}: {error}", parent.display());
        return ExitCode::from(2);
    }
    if let Err(error) = std::fs::write(&path, json) {
        eprintln!("uhura graph: {}: {error}", path.display());
        return ExitCode::from(2);
    }
    println!(
        "wrote {} ({} nodes, {} edges)",
        path.display(),
        artifacts.graph.nodes.len(),
        artifacts.graph.edges.len()
    );
    ExitCode::SUCCESS
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_is_closed_and_uses_canonical_protocols() {
        let program = super::super::project::checked_test_program();
        let graph = uhura_core::build_interaction_graph(&program);
        assert_eq!(graph.protocol, uhura_core::INTERACTION_GRAPH_PROTOCOL);
        assert!(
            graph
                .machine_program_hashes
                .values()
                .all(|hash| is_lower_sha256(hash))
        );
        let ids = graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            graph
                .edges
                .iter()
                .all(|edge| ids.contains(edge.from.as_str()) && ids.contains(edge.to.as_str()))
        );
    }
}
