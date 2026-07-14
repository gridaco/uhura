//! Acceptance gates for the renderer-neutral interaction graph over the
//! canonical Instagram corpus. These assert the relationships NCC needs,
//! while the project-crate unit test pins individual statement lowering.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_check::manifest::load_manifest;
use uhura_check::{CheckInput, SourceInput, check};
use uhura_editor_model::interaction_graph::{EdgeKind, build_interaction_graph};
use uhura_syntax::SourceKind;

include!("common/corpus.rs");

fn identity(_: &str, text: String) -> String {
    text
}

#[test]
fn instagram_projects_navigation_surfaces_commands_and_guards() {
    let out = check(&corpus_input(true, &identity));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.severity == uhura_base::Severity::Error),
        "the canonical corpus checks clean"
    );
    let program = &out.lowered.as_ref().expect("clean check lowers").program;
    let graph = build_interaction_graph(program);

    assert_eq!(graph.entry, "page:feed");
    assert!(
        graph.nodes.iter().any(|n| {
            n.id == "surface:comments-sheet" && n.modality.as_deref() == Some("sheet")
        })
    );
    assert!(graph.edges.iter().any(|e| {
        e.kind == EdgeKind::Present
            && e.from == "page:feed"
            && e.to == "surface:comments-sheet"
            && e.event == "comments-requested"
    }));
    assert!(graph.edges.iter().any(|e| {
        e.kind == EdgeKind::Dismiss
            && e.from == "surface:comments-sheet"
            && e.to == "dynamic:opener"
    }));
    assert!(graph.edges.iter().any(|e| {
        e.kind == EdgeKind::Navigate
            && e.from == "page:feed"
            && e.to == "page:profile"
            && e.event == "tab-selected"
    }));
    assert!(graph.edges.iter().any(|e| {
        e.kind == EdgeKind::SendCommand
            && e.command.as_deref() == Some("feed.like-post")
            && e.guard.is_some()
    }));
    assert!(graph.edges.iter().any(|e| {
        e.kind == EdgeKind::ReceiveOutcome
            && e.from == "command:feed.like-post"
            && e.to == "page:feed"
            && e.outcome.is_some()
    }));

    let first = uhura_base::to_canonical_json(&serde_json::to_value(&graph).unwrap());
    let second = uhura_base::to_canonical_json(
        &serde_json::to_value(build_interaction_graph(program)).unwrap(),
    );
    assert_eq!(first, second, "the graph artifact is byte-deterministic");
}
