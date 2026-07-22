use std::path::{Path, PathBuf};

use uhura_core::{CHECKPOINT_PROTOCOL, Checkpoint, EvidencePresentationKind, Program};
use uhura_project::{ResolvedApplication, ResolvedUiRole};

const MACHINE_COUNT: usize = 1;
const PAGE_COUNT: usize = 9;
const COMPONENT_COUNT: usize = 9;
const PRESENTATION_COUNT: usize = PAGE_COUNT + 1;
const SUBJECT_COUNT: usize = PAGE_COUNT + COMPONENT_COUNT;
const EXAMPLE_COUNT: usize = 91;

fn instagram_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/instagram/client")
}

fn checked_instagram() -> (Program, ResolvedApplication) {
    let root = instagram_root();
    let snapshot = uhura_project::capture_project_snapshot(&root);
    let resolved = uhura_project::resolve_project(&snapshot).unwrap_or_else(|rejection| {
        panic!(
            "Instagram project resolution diagnostics: {:#?}",
            rejection.diagnostics
        )
    });
    let application = resolved.application().clone();
    let checked = resolved.check();
    assert!(
        checked.diagnostics.is_empty(),
        "Instagram check diagnostics: {:#?}",
        checked.diagnostics
    );
    let mut program = checked.program.expect("Instagram lowers to one program");
    program.freeze_program_hashes();
    (program, application)
}

#[test]
fn instagram_is_the_current_machine_ui_and_evidence_acceptance_corpus() {
    let (program, application) = checked_instagram();
    let web_app = application.web_app.expect("Instagram selects web-app@1");
    assert_eq!(program.machine_program.machines.len(), MACHINE_COUNT);
    assert_eq!(program.presentations.len(), PRESENTATION_COUNT);
    assert_eq!(program.components.len(), COMPONENT_COUNT);
    assert_eq!(web_app.subjects.len(), SUBJECT_COUNT);
    assert_eq!(
        web_app
            .subjects
            .iter()
            .filter(|subject| subject.role == ResolvedUiRole::Page)
            .count(),
        PAGE_COUNT
    );
    assert_eq!(
        web_app
            .subjects
            .iter()
            .filter(|subject| matches!(
                subject.role,
                ResolvedUiRole::Component | ResolvedUiRole::Surface
            ))
            .count(),
        COMPONENT_COUNT
    );
    assert_eq!(program.evidence.examples.len(), EXAMPLE_COUNT);

    let report = program.run_evidence();
    assert!(
        report.passed,
        "Instagram evidence failures: {:#?}",
        report.failures
    );
    assert_eq!(report.artifacts.examples.len(), EXAMPLE_COUNT);

    for (name, reference) in &program.evidence.examples {
        let metadata = program
            .evidence
            .example_metadata
            .get(name)
            .unwrap_or_else(|| panic!("example `{name}` has no presentation metadata"));
        let presentation = metadata
            .presentation
            .as_deref()
            .unwrap_or_else(|| panic!("example `{name}` is not targeted"));
        let artifact = report
            .artifacts
            .examples
            .get(name)
            .unwrap_or_else(|| panic!("example `{name}` did not resolve"));
        assert_eq!(&artifact.reference, reference);

        let snapshot = &artifact.snapshot;
        let instance = program
            .machine_program
            .restore(&Checkpoint {
                protocol: CHECKPOINT_PROTOCOL.into(),
                instance: snapshot.instance.clone(),
                machine: snapshot.machine.clone(),
                machine_program_hash: snapshot.machine_program_hash.clone(),
                configuration: snapshot.configuration.clone(),
                state: snapshot.state.clone(),
                inbox: snapshot.inbox.clone(),
                lifecycle: snapshot.lifecycle,
                next_sequence: snapshot.next_sequence,
                trace_prefix_hash: snapshot.trace_prefix_hash.clone(),
            })
            .unwrap_or_else(|error| panic!("example `{name}` cannot restore: {error}"));
        assert_eq!(instance.observation, artifact.observation);

        let projection = match metadata.kind {
            Some(EvidencePresentationKind::Page) => program.project(&instance, presentation),
            Some(EvidencePresentationKind::Component | EvidencePresentationKind::Surface) => {
                program.project_component(&instance, presentation, &metadata.component_props)
            }
            None => panic!("example `{name}` has no presentation role"),
        }
        .unwrap_or_else(|error| {
            panic!("example `{name}` cannot project through `{presentation}`: {error}")
        });
        assert_eq!(projection.document.presentation, presentation);
        assert_eq!(projection.document.machine, snapshot.machine);
        assert_eq!(projection.document.instance, snapshot.instance);
        if !matches!(metadata.kind, Some(EvidencePresentationKind::Page)) {
            assert!(
                projection.bindings.is_empty(),
                "direct component example `{name}` exposed live dispatch bindings"
            );
        }
    }
}

#[test]
fn instagram_host_candidate_admits_editor_and_play_from_the_same_project() {
    let snapshot = uhura_host::capture_project_snapshot(&instagram_root());
    let candidate = uhura_host::build_candidate(&snapshot, 1);
    let summary = candidate.summary();

    assert!(
        summary.editor_current && summary.play_ok,
        "Instagram candidate diagnostics: {:?}",
        candidate.diagnostics()
    );
    assert_eq!(summary.preview_count, Some(EXAMPLE_COUNT));
}
