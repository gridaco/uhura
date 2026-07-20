use std::collections::BTreeMap;

use uhura_check::check_v04_project_modules;
use uhura_syntax::v04::{SourceIdentity, parse};

const MACHINE: &str = include_str!("../../../examples/instagram/client/machine.uhura");
const PARTS: &str = include_str!("../../../examples/instagram/client/parts.uhura");
const UI: &str = include_str!("../../../examples/instagram/client/ui.uhura");

#[test]
fn checks_the_complete_instagram_machine() {
    let parsed = parse(
        SourceIdentity::new(41, "app.instagram@1", "instagram", "client/machine.uhura"),
        MACHINE,
    );
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );

    let parts = parse(
        SourceIdentity::new(42, "app.instagram@1", "parts", "client/parts.uhura"),
        PARTS,
    );
    assert!(
        parts.diagnostics.is_empty(),
        "parts diagnostics:\n{:#?}",
        parts.diagnostics
    );

    let checked = check_v04_project_modules(&[parsed.module, parts.module]);
    let mut grouped = BTreeMap::<(String, String), usize>::new();
    for diagnostic in &checked.diagnostics {
        *grouped
            .entry((diagnostic.code.to_string(), diagnostic.message.clone()))
            .or_default() += 1;
    }
    assert!(
        checked.diagnostics.is_empty(),
        "checker diagnostics ({} total):\n{grouped:#?}",
        checked.diagnostics.len()
    );
}

#[test]
fn checks_the_complete_instagram_machine_and_ui_together() {
    let machine = parse(
        SourceIdentity::new(41, "app.instagram@1", "instagram", "client/machine.uhura"),
        MACHINE,
    );
    let ui = parse(
        SourceIdentity::new(43, "app.instagram@1", "ui", "client/ui.uhura"),
        UI,
    );
    let parts = parse(
        SourceIdentity::new(42, "app.instagram@1", "parts", "client/parts.uhura"),
        PARTS,
    );
    assert!(
        machine.diagnostics.is_empty() && parts.diagnostics.is_empty() && ui.diagnostics.is_empty(),
        "machine diagnostics:\n{:#?}\nparts diagnostics:\n{:#?}\nUI diagnostics:\n{:#?}",
        machine.diagnostics,
        parts.diagnostics,
        ui.diagnostics,
    );

    let checked = check_v04_project_modules(&[machine.module, parts.module, ui.module]);
    let mut grouped = BTreeMap::<(String, String), usize>::new();
    for diagnostic in &checked.diagnostics {
        *grouped
            .entry((diagnostic.code.to_string(), diagnostic.message.clone()))
            .or_default() += 1;
    }
    assert!(
        checked.diagnostics.is_empty(),
        "checker diagnostics ({} total):\n{grouped:#?}",
        checked.diagnostics.len(),
    );
    let program = checked.program.expect("complete Instagram project checks");
    let icon_fonts =
        uhura_check::icon_fonts::load_icon_fonts(&Default::default(), &BTreeMap::new())
            .expect("built-in icon registry");
    let icon_issues = uhura_check::check_program_icon_tokens(&program, &icon_fonts);
    assert!(
        icon_issues.is_empty(),
        "Instagram icon tokens must resolve before rendering:\n{icon_issues:#?}"
    );
    assert_eq!(program.machine_program.machines.len(), 1);
    assert_eq!(program.presentations.len(), 18);
    assert!(
        program
            .machine_program
            .machines
            .contains_key("app.instagram@1::Instagram")
    );
    let machine = &program.machine_program.machines["app.instagram@1::Instagram"];
    assert!(
        machine
            .state
            .iter()
            .any(|field| field.name == "notice.message"),
        "Instagram must retain the composed Notice state owner after lowering"
    );
    assert!(
        machine
            .handlers
            .contains_key("notice_controls.DismissNotice"),
        "Instagram must retain its part-owned dismissal input"
    );
    assert!(
        machine
            .observation
            .iter()
            .any(|field| field.name == "notice"),
        "Instagram must preserve the existing flat notice presentation contract"
    );
    assert!(
        program
            .presentations
            .contains_key("app.instagram@1::FeedPage"),
    );
}
