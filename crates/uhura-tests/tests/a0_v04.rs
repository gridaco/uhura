use std::path::{Path, PathBuf};

const EXAMPLE_COUNT: usize = 12;

fn a0_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/applications/a0-return-desk/answers/uhura-0.4")
}

#[test]
fn a0_v04_admits_the_complete_editor_and_play_candidate() {
    let snapshot = uhura_host::capture_project_snapshot(&a0_root());
    let candidate = uhura_host::build_candidate(&snapshot, 1);
    let summary = candidate.summary();

    assert!(
        summary.editor_current && summary.play_ok,
        "A0 0.4 candidate diagnostics: {:?}",
        candidate.diagnostics()
    );
    assert_eq!(summary.preview_count, Some(EXAMPLE_COUNT));
}
