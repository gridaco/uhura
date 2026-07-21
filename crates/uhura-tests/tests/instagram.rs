use std::path::{Path, PathBuf};

use uhura_core::{CHECKPOINT_PROTOCOL, Checkpoint, Program};
use uhura_syntax::v04::{SourceIdentity, parse as parse_v04};

const MACHINE_COUNT: usize = 1;
const PRESENTATION_COUNT: usize = 18;
const EXAMPLE_COUNT: usize = 91;

fn instagram_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/instagram/client")
}

fn checked_instagram() -> Program {
    let root = instagram_root();
    let modules = [
        ("instagram", "machine.uhura"),
        ("parts", "parts.uhura"),
        ("ui", "ui.uhura"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (logical, path))| {
        let source = std::fs::read_to_string(root.join(path))
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        let parsed = parse_v04(
            SourceIdentity::new(index as u32, "app.instagram@1", logical, path),
            &source,
        );
        assert!(
            parsed.diagnostics.is_empty(),
            "Instagram {path} parse diagnostics: {:#?}",
            parsed.diagnostics
        );
        parsed.module
    })
    .collect::<Vec<_>>();

    let evidence_path = "evidence.uhura";
    let evidence_source = std::fs::read_to_string(root.join(evidence_path))
        .unwrap_or_else(|error| panic!("{evidence_path}: {error}"));
    let evidence = parse_v04(
        SourceIdentity::new(
            modules.len() as u32,
            "app.instagram@1",
            "evidence",
            evidence_path,
        ),
        &evidence_source,
    );
    assert!(
        evidence.diagnostics.is_empty(),
        "Instagram evidence parse diagnostics: {:#?}",
        evidence.diagnostics
    );

    let checked =
        uhura_check::check_v04_project_modules_with_evidence(&modules, &[evidence.module]);
    assert!(
        checked.diagnostics.is_empty(),
        "Instagram check diagnostics: {:#?}",
        checked.diagnostics
    );
    let mut program = checked.program.expect("Instagram lowers to one program");
    program.freeze_program_hashes();
    program
}

#[test]
fn instagram_is_the_current_machine_ui_and_evidence_acceptance_corpus() {
    let program = checked_instagram();
    assert_eq!(program.machine_program.machines.len(), MACHINE_COUNT);
    assert_eq!(program.presentations.len(), PRESENTATION_COUNT);
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

        let projection = program
            .project(&instance, presentation)
            .unwrap_or_else(|error| {
                panic!("example `{name}` cannot project through `{presentation}`: {error}")
            });
        assert_eq!(projection.document.presentation, presentation);
        assert_eq!(projection.document.machine, snapshot.machine);
        assert_eq!(projection.document.instance, snapshot.instance);
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
