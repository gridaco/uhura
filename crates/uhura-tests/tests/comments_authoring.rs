//! End-to-end RFC 0003 checker gates: checked authoring metadata remains a
//! sidecar, target IDs are comment-insensitive, and clean lowering has total
//! template-origin coverage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_check::manifest::load_manifest;
use uhura_check::metadata::{MetadataClass, SourceTargetClass};
use uhura_check::{CheckInput, SourceInput, check};
use uhura_cli::cmd::trace::run_script;
use uhura_syntax::SourceKind;

include!("common/corpus.rs");

fn identity(_: &str, text: String) -> String {
    text
}

fn with_authoring(path: &str, text: String) -> String {
    match path {
        "components/post-card.uhura" => text.replacen(
            "<!-- @doc The complete post-card canvas occurrence. -->",
            "<!-- @doc The complete post-card canvas occurrence. -->\n<!-- @review-note Verify compact layouts. -->",
            1,
        ),
        _ => text,
    }
}

#[test]
fn docs_and_annotations_are_checked_sidecars_with_stable_target_ids() {
    let baseline = check(&corpus_input(true, &identity));
    assert!(baseline.diagnostics.is_empty());
    let annotated = check(&corpus_input(true, &with_authoring));
    assert!(
        annotated.diagnostics.is_empty(),
        "authoring-only edits must remain clean: {:?}",
        annotated
            .diagnostics
            .iter()
            .map(|diagnostic| (&diagnostic.code, &diagnostic.message))
            .collect::<Vec<_>>()
    );
    annotated.authoring.validate().expect("valid projection");

    let baseline_lowered = baseline.lowered.as_ref().expect("baseline lowers");
    let baseline_ir = baseline_lowered.program.to_canonical_string();
    let lowered = annotated.lowered.as_ref().expect("annotated source lowers");
    assert_eq!(baseline_ir, lowered.program.to_canonical_string());
    assert_eq!(
        baseline_lowered.program.hash(),
        lowered.program.hash(),
        "authoring metadata does not perturb the canonical IR hash"
    );
    lowered
        .validate_template_origin_coverage()
        .expect("one origin per template operation");

    // The canonical scripts exercise view hashes, commands, intents,
    // outcomes, runtime diagnostics, and trace serialization. Comparing the
    // complete JSONL makes the runtime-inertness contract explicit rather
    // than relying only on the equal ProgramIr assertion above.
    let fixture = std::fs::read_to_string(corpus_root().join("fixtures/standard.toml"))
        .expect("standard fixture");
    for script in ["like-ok", "comment-ok", "demo"] {
        let script_text =
            std::fs::read_to_string(corpus_root().join(format!("fixtures/scripts/{script}.toml")))
                .unwrap_or_else(|error| panic!("{script}: {error}"));
        let baseline_trace = run_script(&baseline_lowered.program, &fixture, &script_text, false)
            .unwrap_or_else(|error| panic!("baseline {script}: {error}"));
        let annotated_trace = run_script(&lowered.program, &fixture, &script_text, false)
            .unwrap_or_else(|error| panic!("annotated {script}: {error}"));
        assert_eq!(
            baseline_trace, annotated_trace,
            "{script}: docs and annotations must not change V, commands, intents, outcomes, or traces"
        );
    }

    let baseline_ids = baseline
        .authoring
        .targets
        .iter()
        .map(|target| target.id.as_str())
        .collect::<Vec<_>>();
    let annotated_ids = annotated
        .authoring
        .targets
        .iter()
        .map(|target| target.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        baseline_ids, annotated_ids,
        "comment prose/spans do not define target IDs"
    );

    let root = annotated
        .authoring
        .targets
        .iter()
        .find(|target| {
            target.file == "components/post-card.uhura"
                && target.class == SourceTargetClass::CatalogElement
                && target.label == "view"
                && annotated
                    .authoring
                    .entries
                    .iter()
                    .any(|entry| entry.target_id == target.id)
        })
        .expect("annotated root target");
    let annotations = annotated
        .authoring
        .entries
        .iter()
        .filter(|entry| entry.target_id == root.id)
        .collect::<Vec<_>>();
    assert_eq!(annotations.len(), 2);
    assert_eq!(annotations[0].class, MetadataClass::Annotation);
    assert_eq!(annotations[0].kind, "doc");
    assert_eq!(annotations[0].order, 0);
    assert_eq!(annotations[1].kind, "review-note");
    assert_eq!(annotations[1].order, 1);

    let image_preview = annotated
        .previews
        .iter()
        .find(|preview| {
            preview.example == "image-post" && preview.subject.name().as_str() == "post-card"
        })
        .expect("post-card image example");
    assert!(image_preview.declaration_doc_id.is_some());
    assert!(image_preview.example_doc_id.is_some());
    for (doc_id, target_class) in [
        (
            image_preview.declaration_doc_id.as_ref().unwrap(),
            SourceTargetClass::ComponentDeclaration,
        ),
        (
            image_preview.example_doc_id.as_ref().unwrap(),
            SourceTargetClass::ExampleDeclaration,
        ),
    ] {
        let entry = annotated
            .authoring
            .entries
            .iter()
            .find(|entry| &entry.id == doc_id)
            .expect("preview doc id resolves in the authoring projection");
        assert_eq!(entry.class, MetadataClass::Doc);
        let target = annotated
            .authoring
            .targets
            .iter()
            .find(|target| target.id == entry.target_id)
            .expect("preview doc target resolves");
        assert_eq!(target.class, target_class);
    }
}

#[test]
fn incomplete_template_origins_are_rejected_before_lowered_artifact_escape() {
    let mut output = check(&corpus_input(true, &identity));
    assert!(output.diagnostics.is_empty());
    let mut lowered = output.lowered.take().expect("clean corpus lowers");
    let mut incomplete = std::mem::take(&mut lowered.template_origins);
    let missing = incomplete
        .keys()
        .next()
        .cloned()
        .expect("corpus has template origins");
    incomplete.remove(&missing);

    let error = match lowered.with_template_origins(incomplete) {
        Ok(_) => panic!("incomplete provenance must not produce a lowered artifact"),
        Err(error) => error,
    };
    assert!(error.contains("1 missing"));
    assert!(error.contains(&format!("{missing:?}")));
}

#[test]
fn annotation_on_an_unknown_element_is_incompatible_and_not_projected() {
    let mutate = |path: &str, text: String| {
        if path != "components/post-card.uhura" {
            return text;
        }
        text.replacen(
            "      <img class=\"avatar\" src={post.author.avatar.src} alt={post.author.avatar.alt} />",
            "      <!-- @review-note This target is unresolved. -->\n      <mystery />",
            1,
        )
    };
    let output = check(&corpus_input(true, &mutate));
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0019")
    );
    assert!(!output.authoring.entries.iter().any(|entry| {
        entry.class == MetadataClass::Annotation && entry.text == "This target is unresolved."
    }));
}

#[test]
fn independently_valid_docs_survive_an_unrelated_dirty_check() {
    let mutate = |path: &str, text: String| {
        let text = with_authoring(path, text);
        if path == "components/post-card.uhura" {
            text.replacen("liked: bool", "liked: no-such-type", 1)
        } else {
            text
        }
    };
    let output = check(&corpus_input(true, &mutate));
    assert!(!output.diagnostics.is_empty());
    assert!(output.lowered.is_none());
    assert!(output.authoring.entries.iter().any(|entry| {
        entry.class == MetadataClass::Doc
            && entry.text == "Presents one post and its primary interactions."
    }));
}

#[test]
fn page_docs_and_annotations_survive_a_route_collision() {
    let mut input = corpus_input(false, &identity);
    input.sources.push(SourceInput {
        rel_path: "app/feed/[shadow]/page.uhura".into(),
        text: "//! The colliding source module.\n/// The colliding feed page.\npage\n\n<!-- @review-note Keep this target visible while routes are dirty. -->\n<view />\n"
            .into(),
        kind: SourceKind::Module,
    });
    input
        .sources
        .sort_by(|left, right| left.rel_path.cmp(&right.rel_path));

    let output = check(&input);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH1002")
    );
    assert!(output.authoring.entries.iter().any(|entry| {
        entry.class == MetadataClass::Doc && entry.text == "The colliding feed page."
    }));
    let annotation = output
        .authoring
        .entries
        .iter()
        .find(|entry| entry.text == "Keep this target visible while routes are dirty.")
        .expect("annotation on the unresolved page's catalog element");
    let target = output
        .authoring
        .targets
        .iter()
        .find(|target| target.id == annotation.target_id)
        .expect("annotation target");
    assert_eq!(target.class, SourceTargetClass::CatalogElement);
    assert_eq!(target.file, "app/feed/[shadow]/page.uhura");
}

#[test]
fn structural_and_component_annotations_survive_an_unavailable_catalog() {
    let mutate = |path: &str, text: String| {
        match path {
        "components/post-card.uhura" => text.replacen(
            "  {#match post.media}",
            "  <!-- @review-note Match survives a missing catalog. -->\n  {#match post.media}",
            1,
        ),
        "app/feed/page.uhura" => text.replacen(
            "<post-card ",
            "<!-- @review-note Component survives a missing catalog. -->\n              <post-card ",
            1,
        ),
        _ => text,
    }
    };
    let mut input = corpus_input(false, &mutate);
    input.catalog_file.1 = None;

    let output = check(&input);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH2002")
    );
    for (text, class) in [
        (
            "Match survives a missing catalog.",
            SourceTargetClass::MatchBlock,
        ),
        (
            "Component survives a missing catalog.",
            SourceTargetClass::ComponentInvocation,
        ),
    ] {
        let entry = output
            .authoring
            .entries
            .iter()
            .find(|entry| entry.text == text)
            .unwrap_or_else(|| panic!("missing `{text}`"));
        let target = output
            .authoring
            .targets
            .iter()
            .find(|target| target.id == entry.target_id)
            .expect("annotation target");
        assert_eq!(target.class, class);
    }
}

#[test]
fn catalog_and_imported_component_name_collision_is_rejected_consistently() {
    let mut input = corpus_input(false, &identity);
    input.sources.extend([
        SourceInput {
            rel_path: "components/view.uhura".into(),
            text: "component view\n\n<text>Component named view</text>\n".into(),
            kind: SourceKind::Module,
        },
        SourceInput {
            rel_path: "components/collision-host.uhura".into(),
            text: "component collision-host\n\nuse component view\n\n<!-- @review-note This target is ambiguous. -->\n<view />\n"
                .into(),
            kind: SourceKind::Module,
        },
    ]);
    input
        .sources
        .sort_by(|left, right| left.rel_path.cmp(&right.rel_path));

    let output = check(&input);
    assert!(
        output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "UH1007" && diagnostic.message.contains("ambiguous")
        }),
        "the checker must not choose a different meaning from lowering"
    );
    assert!(output.lowered.is_none());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "UH0019" && diagnostic.message.contains("ambiguous")
    }));
    assert!(
        !output
            .authoring
            .entries
            .iter()
            .any(|entry| entry.text == "This target is ambiguous.")
    );
}
