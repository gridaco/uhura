//! Manifest-resolved module behavior.

use std::collections::BTreeSet;

use uhura_check::{check_project_modules, resolve_project_modules};
use uhura_syntax::{Module, SourceIdentity, parse};

const SUPPORT: &str = r#"
pub enum Mode {
  Idle,
  Active,
}

pub fn active(mode: Mode) -> Bool {
  mode == Mode::Active
}
"#;

const MACHINE_MONOLITH: &str = r#"
pub enum Mode {
  Idle,
  Active,
}

pub fn active(mode: Mode) -> Bool {
  mode == Mode::Active
}

pub machine Probe {
  events {
    Toggle,
  }

  outcomes {
    commit Done,
  }

  state {
    mode: Mode = Mode::Idle,
  }

  observe {
    active: active(mode),
  }

  on Toggle {
    mode = Mode::Active;
    Done
  }
}
"#;

const MACHINE_SPLIT: &str = r#"
use crate::support::{Mode, active};

pub machine Probe {
  events {
    Toggle,
  }

  outcomes {
    commit Done,
  }

  state {
    mode: Mode = Mode::Idle,
  }

  observe {
    active: active(mode),
  }

  on Toggle {
    mode = Mode::Active;
    Done
  }
}
"#;

const MACHINE_ALIAS: &str = r#"
use crate::support::{Mode as Status, active as is_active};

pub machine Probe {
  events {
    Toggle,
  }

  outcomes {
    commit Done,
  }

  state {
    mode: Status = Status::Idle,
  }

  observe {
    active: is_active(mode),
  }

  on Toggle {
    mode = Status::Active;
    Done
  }
}
"#;

const SIMPLE_MACHINE: &str = r#"
pub machine Simple {
  events {
    Run,
  }

  outcomes {
    commit Done,
  }

  state {}
  observe {}

  on Run {
    Done
  }
}
"#;

fn module(file: u32, logical: &str, path: &str, source: &str) -> Module {
    let parsed = parse(
        SourceIdentity::new(file, "example.modules@1", logical, path),
        source,
    );
    assert!(
        parsed.is_ok(),
        "parse diagnostics for {logical}:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn checked(modules: &[Module]) -> uhura_core::Program {
    let output = check_project_modules(modules);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    output.program.expect("checked project")
}

fn semantic_json(program: &uhura_core::Program) -> serde_json::Value {
    fn remove_authoring_sources(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    remove_authoring_sources(value);
                }
            }
            serde_json::Value::Object(values) => {
                let is_source_ref = values.len() == 4
                    && values.contains_key("id")
                    && values.contains_key("path")
                    && values.contains_key("start")
                    && values.contains_key("end");
                if is_source_ref {
                    values.retain(|field, _| field == "id");
                    return;
                }
                for value in values.values_mut() {
                    remove_authoring_sources(value);
                }
            }
            _ => {}
        }
    }

    let mut value = serde_json::to_value(program).expect("program JSON");
    remove_authoring_sources(&mut value);
    value
}

fn program_source_paths(program: &uhura_core::Program) -> BTreeSet<String> {
    fn collect(value: &serde_json::Value, paths: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    collect(value, paths);
                }
            }
            serde_json::Value::Object(values) => {
                let is_source_ref = values.len() == 4
                    && values.contains_key("id")
                    && values.contains_key("path")
                    && values.contains_key("start")
                    && values.contains_key("end");
                if is_source_ref {
                    paths.insert(
                        values["path"]
                            .as_str()
                            .expect("SourceRef path is a string")
                            .to_string(),
                    );
                    return;
                }
                for value in values.values() {
                    collect(value, paths);
                }
            }
            _ => {}
        }
    }

    let value = serde_json::to_value(program).expect("program JSON");
    let mut paths = BTreeSet::new();
    collect(&value, &mut paths);
    paths
}

#[test]
fn monolith_and_split_modules_lower_to_one_semantic_program() {
    let monolith = checked(&[module(1, "program", "program.uhura", MACHINE_MONOLITH)]);
    let split = checked(&[
        module(2, "support", "support.uhura", SUPPORT),
        module(3, "program", "program.uhura", MACHINE_SPLIT),
    ]);

    assert_eq!(
        monolith.machine_program.program_hashes,
        split.machine_program.program_hashes
    );
    assert_eq!(semantic_json(&monolith), semantic_json(&split));
    assert_eq!(split.machine_program.modules, ["example.modules@1"]);
}

#[test]
fn physical_and_logical_module_moves_preserve_semantics() {
    let before = checked(&[
        module(1, "support", "support.uhura", SUPPORT),
        module(2, "program", "program.uhura", MACHINE_SPLIT),
    ]);
    let moved_machine = MACHINE_SPLIT.replace("crate::support", "crate::shared::domain");
    let after = checked(&[
        module(91, "shared::domain", "src/shared/domain.uhura", SUPPORT),
        module(92, "application", "src/application.uhura", &moved_machine),
    ]);

    assert_eq!(
        before.machine_program.program_hashes,
        after.machine_program.program_hashes
    );
    assert_eq!(semantic_json(&before), semantic_json(&after));
}

#[test]
fn checked_program_and_provenance_retain_physical_source_paths() {
    let modules = [
        module(1, "support", "src/domain/support.uhura", SUPPORT),
        module(2, "program", "src/application/program.uhura", MACHINE_SPLIT),
    ];
    let output = check_project_modules(&modules);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked project");
    let provenance = output.provenance.expect("source provenance");
    let expected = BTreeSet::from([
        "src/application/program.uhura".to_string(),
        "src/domain/support.uhura".to_string(),
    ]);

    assert_eq!(program_source_paths(&program), expected);
    assert_eq!(
        provenance
            .sources
            .iter()
            .map(|source| source.path.clone())
            .collect::<BTreeSet<_>>(),
        expected
    );
}

#[test]
fn aliases_are_resolution_only() {
    let ordinary_modules = [
        module(1, "support", "support.uhura", SUPPORT),
        module(2, "program", "program.uhura", MACHINE_SPLIT),
    ];
    let ordinary = checked(&ordinary_modules);
    let aliased_modules = [
        module(7, "support", "support.uhura", SUPPORT),
        module(8, "program", "program.uhura", MACHINE_ALIAS),
    ];
    let aliased = checked(&aliased_modules);

    assert_eq!(
        ordinary.machine_program.program_hashes,
        aliased.machine_program.program_hashes
    );
    assert_eq!(semantic_json(&ordinary), semantic_json(&aliased));

    let resolved = resolve_project_modules(&aliased_modules);
    let status = resolved
        .metadata
        .bindings
        .iter()
        .find(|binding| binding.local_name == "Status")
        .expect("resolved alias metadata");
    assert_eq!(status.module, "program");
    assert_eq!(status.target_module, "support");
    assert_eq!(status.target_name, "Mode");
    assert!(!status.reexport);
    assert_eq!(
        resolved
            .metadata
            .declarations
            .iter()
            .find(|declaration| declaration.name == "Mode")
            .and_then(|declaration| declaration.public_id.as_deref()),
        Some("example.modules@1::Mode")
    );
}

#[test]
fn private_import_is_rejected_at_the_authored_locator() {
    let support = SUPPORT.replace("pub enum Mode", "enum Mode");
    let support = support.replace("pub fn active", "fn active");
    let imported = module(42, "program", "program.uhura", MACHINE_SPLIT);
    let output =
        check_project_modules(&[module(41, "support", "support.uhura", &support), imported]);

    assert!(output.program.is_none());
    let private = output
        .diagnostics
        .iter()
        .find(|value| value.rule == "uhura-0.4/private-import")
        .expect("private import diagnostic");
    assert_eq!(private.span.file.0, 42);
    assert_eq!(
        &MACHINE_SPLIT[private.span.start as usize..private.span.end as usize],
        "Mode"
    );
}

#[test]
fn same_name_public_reexport_resolves_without_a_second_declaration() {
    let facade = module(
        2,
        "facade",
        "facade.uhura",
        "pub use crate::support::Mode;\n",
    );
    let machine = module(
        3,
        "program",
        "program.uhura",
        &MACHINE_SPLIT.replace(
            "crate::support::{Mode, active}",
            "crate::facade::Mode;\nuse crate::support::active",
        ),
    );
    let direct = checked(&[
        module(1, "support", "support.uhura", SUPPORT),
        module(4, "program", "program.uhura", MACHINE_SPLIT),
    ]);
    let through_facade = checked(&[
        module(1, "support", "support.uhura", SUPPORT),
        facade,
        machine,
    ]);

    assert_eq!(
        direct.machine_program.program_hashes,
        through_facade.machine_program.program_hashes
    );
    assert_eq!(
        direct.machine_program.types.keys().collect::<Vec<_>>(),
        through_facade
            .machine_program
            .types
            .keys()
            .collect::<Vec<_>>()
    );
}

#[test]
fn package_global_public_name_collision_is_rejected() {
    let first = module(
        1,
        "first",
        "first.uhura",
        "pub struct Shared { value: Int }\n",
    );
    let second = module(
        2,
        "second",
        "second.uhura",
        "pub struct Shared { value: Int }\n",
    );
    let output = check_project_modules(&[second, first]);

    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura-0.4/public-name-collision")
    );
}

#[test]
fn module_and_use_order_are_nonsemantic_and_unused_use_is_inert() {
    let reordered_source = MACHINE_SPLIT.replace(
        "use crate::support::{Mode, active};",
        "use crate::support::{active, Mode};",
    );
    let normal = checked(&[
        module(1, "support", "support.uhura", SUPPORT),
        module(2, "program", "program.uhura", MACHINE_SPLIT),
    ]);
    let reordered = checked(&[
        module(20, "program", "program.uhura", &reordered_source),
        module(10, "support", "support.uhura", SUPPORT),
    ]);
    assert_eq!(
        normal.machine_program.program_hashes,
        reordered.machine_program.program_hashes
    );
    assert_eq!(semantic_json(&normal), semantic_json(&reordered));

    let unused_support = "pub struct Unused { value: Int }\n";
    let with_unused = format!("use crate::unused::Unused;\n{SIMPLE_MACHINE}");
    let bare = checked(&[module(30, "program", "program.uhura", SIMPLE_MACHINE)]);
    let imported = checked(&[
        module(31, "unused", "unused.uhura", unused_support),
        module(32, "program", "program.uhura", &with_unused),
    ]);
    assert_eq!(
        bare.machine_program.program_hashes,
        imported.machine_program.program_hashes
    );
}

#[test]
fn uncomposed_parts_and_known_standard_features_are_inert() {
    let part = module(
        1,
        "part",
        "part.uhura",
        "pub part Child() { state {} observe {} }\n",
    );
    let standard = module(
        2,
        "standard",
        "standard.uhura",
        &format!("use uhura::ui;\n{SIMPLE_MACHINE}"),
    );
    let output = check_project_modules(&[part, standard]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(output.program.is_some());
}

#[test]
fn external_alias_requires_a_resolved_package_graph() {
    let external = module(
        3,
        "external",
        "external.uhura",
        "use vendor::icons::Icon;\npub struct ExternalUse { value: Int }\n",
    );
    let output = check_project_modules(&[external]);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|value| {
        value.rule == "uhura-0.4/unknown-dependency-alias" && value.message.contains("vendor")
    }));
}

#[test]
fn unknown_port_contract_is_rejected_at_the_authored_contract() {
    let port = module(
        4,
        "port",
        "port.uhura",
        r#"
pub machine Ported {
  port worker = WorkerPool { queue: "primary" };
  outcomes { commit Done }
  state {}
  observe {}
}
"#,
    );
    let output = check_project_modules(std::slice::from_ref(&port));

    assert!(output.program.is_none());
    let Some(diagnostic) = output
        .diagnostics
        .iter()
        .find(|value| value.rule == "uhura/unknown-port-contract")
    else {
        panic!(
            "unknown port contract diagnostics:\n{:#?}",
            output.diagnostics
        );
    };
    assert_eq!(
        &port.source[diagnostic.span.start as usize..diagnostic.span.end as usize],
        "WorkerPool"
    );
}

#[test]
fn unimported_cross_module_public_names_are_not_visible() {
    let support = module(
        1,
        "support",
        "support.uhura",
        "pub const INITIAL: Int = 7;\n",
    );
    let machine = module(
        2,
        "program",
        "program.uhura",
        r#"
pub machine Probe {
  events { Reset }
  outcomes { commit Done }
  state { count: Int = INITIAL }
  observe { count }
  on Reset {
    count = INITIAL;
    Done
  }
}
"#,
    );

    let output = check_project_modules(&[support, machine]);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "uhura-0.4/unimported-name"),
        "diagnostics:\n{:#?}",
        output.diagnostics,
    );
}

#[test]
fn lexical_bindings_may_shadow_an_unimported_package_name() {
    let support = module(
        1,
        "support",
        "support.uhura",
        "pub fn value() -> Int { 7 }\n",
    );
    let consumer = module(
        2,
        "consumer",
        "consumer.uhura",
        "pub fn identity(value: Int) -> Int { value }\n",
    );
    let output = check_project_modules(&[support, consumer]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(output.program.is_some());
}

#[test]
fn a_disjoint_lexical_binding_does_not_hide_an_unimported_package_name() {
    let support = module(
        1,
        "support",
        "support.uhura",
        "pub fn value() -> Int { 7 }\n",
    );
    let consumer = module(
        2,
        "consumer",
        "consumer.uhura",
        r#"
pub fn choose(flag: Bool) -> Int {
  if flag {
    let value = 1;
    value
  } else {
    value()
  }
}
"#,
    );
    let output = check_project_modules(&[support, consumer]);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "uhura-0.4/unimported-name"),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
}

#[test]
fn same_spelled_private_helpers_are_module_local() {
    let left = module(
        1,
        "left",
        "left.uhura",
        r#"
fn seed() -> Int { 1 }

pub machine Left {
  events { Reset }
  outcomes { commit Done }
  state { value: Int = seed() }
  observe { value }
  on Reset {
    value = seed();
    Done
  }
}
"#,
    );
    let right = module(
        2,
        "right",
        "right.uhura",
        r#"
fn seed() -> Int { 2 }

pub machine Right {
  events { Reset }
  outcomes { commit Done }
  state { value: Int = seed() }
  observe { value }
  on Reset {
    value = seed();
    Done
  }
}
"#,
    );

    let output = check_project_modules(&[left, right]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics,
    );
    let program = output.program.expect("both machines check");
    assert!(
        program
            .machine_program
            .machines
            .contains_key("example.modules@1::Left")
    );
    assert!(
        program
            .machine_program
            .machines
            .contains_key("example.modules@1::Right")
    );
}

#[test]
fn private_nominal_identity_follows_transitive_private_reachability() {
    let source = module(
        1,
        "program",
        "program.uhura",
        r#"
enum Secret {
  Hidden,
}

fn secret() -> Secret {
  Secret::Hidden
}

fn ready() -> Bool {
  secret() == Secret::Hidden
}

pub machine Probe {
  events { Check }
  outcomes { commit Done }
  state {}
  observe { ready: ready() }
  on Check { Done }
}
"#,
    );

    let output = check_project_modules(&[source]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics,
    );
    assert!(
        output
            .program
            .expect("transitive private closure checks")
            .machine_program
            .machines
            .contains_key("example.modules@1::Probe")
    );
}

#[test]
fn private_nominal_identity_cannot_have_multiple_public_owners() {
    let source_text = r#"
enum Secret {
  Hidden,
}

fn ready() -> Bool {
  Secret::Hidden == Secret::Hidden
}

pub machine First {
  events { Check }
  outcomes { commit Done }
  state {}
  observe { ready: ready() }
  on Check { Done }
}

pub machine Second {
  events { Check }
  outcomes { commit Done }
  state {}
  observe { ready: ready() }
  on Check { Done }
}
"#;
    let output = check_project_modules(&[module(1, "program", "program.uhura", source_text)]);

    assert!(output.program.is_none());
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.rule == "uhura-0.4/private-identity-multiple-owners")
        .expect("multiple-owner private identity diagnostic");
    assert_eq!(
        &source_text[diagnostic.span.start as usize..diagnostic.span.end as usize],
        "Secret"
    );
    assert!(diagnostic.message.contains("example.modules@1::First"));
    assert!(diagnostic.message.contains("example.modules@1::Second"));
}

#[test]
fn private_structural_helpers_may_be_shared_by_public_owners() {
    let source = module(
        1,
        "program",
        "program.uhura",
        r#"
fn ready() -> Bool { true }

pub machine First {
  events { Check }
  outcomes { commit Done }
  state {}
  observe { ready: ready() }
  on Check { Done }
}

pub machine Second {
  events { Check }
  outcomes { commit Done }
  state {}
  observe { ready: ready() }
  on Check { Done }
}
"#,
    );

    let output = check_project_modules(&[source]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics,
    );
    let program = output.program.expect("shared structural helper checks");
    assert!(
        program
            .machine_program
            .machines
            .contains_key("example.modules@1::First")
    );
    assert!(
        program
            .machine_program
            .machines
            .contains_key("example.modules@1::Second")
    );
}

#[test]
fn unrelated_public_declarations_do_not_perturb_private_nominal_identity() {
    fn private_machine(name: &str) -> String {
        format!(
            r#"
enum Secret {{
  Hidden,
}}

fn ready() -> Bool {{
  Secret::Hidden == Secret::Hidden
}}

pub machine {name} {{
  events {{ Check }}
  outcomes {{ commit Done }}
  state {{}}
  observe {{ ready: ready() }}
  on Check {{ Done }}
}}
"#
        )
    }

    let left = private_machine("Left");
    let right = private_machine("Right");
    let baseline = checked(&[
        module(1, "left", "left.uhura", &left),
        module(2, "right", "right.uhura", &right),
    ]);
    let with_unrelated = format!("pub const UNUSED: Int = 1;\n{left}");
    let changed = checked(&[
        module(11, "left", "moved/left.uhura", &with_unrelated),
        module(12, "right", "moved/right.uhura", &right),
    ]);

    assert_eq!(
        baseline
            .machine_program
            .program_hashes
            .get("example.modules@1::Left"),
        changed
            .machine_program
            .program_hashes
            .get("example.modules@1::Left"),
    );
    assert_eq!(
        baseline
            .machine_program
            .program_hashes
            .get("example.modules@1::Right"),
        changed
            .machine_program
            .program_hashes
            .get("example.modules@1::Right"),
    );
}
