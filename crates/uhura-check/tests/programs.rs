//! Canonical machine-program behavior.

use std::str::FromStr;
use uhura_check::check_module;

use uhura_core::{
    BoundaryNumber, Decimal, MACHINE_PROGRAM_ID_PROTOCOL, OutcomePolicy, ReactionResolution,
    TypeDef, TypeRef, Value,
};
use uhura_syntax::{SourceIdentity, parse};

const PROGRAMS: &str = include_str!("../../../examples/programs/answers/uhura-0.4/programs.uhura");

fn checked_programs() -> uhura_core::Program {
    let parsed = parse(
        SourceIdentity::new(7, "examples.programs@1", "programs", "programs.uhura"),
        PROGRAMS,
    );
    assert!(
        parsed.is_ok(),
        "0.4 parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    let output = check_module(&parsed.module);
    assert!(
        output.diagnostics.is_empty(),
        "0.4 check diagnostics:\n{:#?}",
        output.diagnostics
    );
    output.program.expect("successful 0.4 program")
}

#[test]
fn authored_type_mismatch_is_single_precise_and_hides_private_lowering_names() {
    let source = r#"struct User {
  id: Int,
}

pub machine Example {
  events {
    Run,
  }

  outcomes {
    commit Accepted,
  }

  state {
    current: User = User {
      id: 1,
    },
  }

  observe {
    current,
  }

  on Run {
    current = 1;
    Accepted
  }
}
"#;
    let parsed = parse(
        SourceIdentity::new(17, "example@1", "example", "example.uhura"),
        source,
    );
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let output = check_module(&parsed.module);
    assert!(output.program.is_none());
    assert_eq!(output.diagnostics.len(), 1, "{:#?}", output.diagnostics);
    let diagnostic = &output.diagnostics[0];
    assert_eq!(diagnostic.code, "R1004");
    assert_eq!(diagnostic.rule, "uhura/type-mismatch");
    assert_eq!(diagnostic.message, "expected `User`, found `Int`");
    assert!(!diagnostic.message.contains("__uhura"));
    assert_eq!(
        &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
        "1"
    );
    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(
        &source[diagnostic.labels[0].span.start as usize..diagnostic.labels[0].span.end as usize],
        "current = 1;"
    );
    assert_eq!(
        diagnostic.labels[0].message,
        "the enclosing authored construct requires this type"
    );
}

#[test]
fn complete_l0_l1_l2_program_reaches_the_machine_kernel() {
    let program = checked_programs();
    assert_eq!(program.machine_program.language, "uhura 0.4");
    assert_eq!(
        program.machine_program.identity_protocol,
        MACHINE_PROGRAM_ID_PROTOCOL
    );
    assert_eq!(program.machine_program.modules, ["examples.programs@1"]);
    assert_eq!(
        program
            .machine_program
            .machines
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "examples.programs@1::BoundedCounter",
            "examples.programs@1::KeyedTaskSupervisor",
            "examples.programs@1::RiverCrossing",
        ]
    );
    assert_eq!(program.machine_program.program_hashes.len(), 3);
    program
        .validate_protocol()
        .expect("0.4 machine program protocol validates");
}

fn record_field<'a>(value: &'a Value, name: &str) -> &'a Value {
    let Value::Record(fields) = value else {
        panic!("expected record, found {value:?}");
    };
    fields
        .iter()
        .find_map(|(field, value)| (field == name).then_some(value))
        .unwrap_or_else(|| panic!("record has no `{name}` field: {value:?}"))
}

fn completed(receipt: &uhura_core::ReactionReceipt) -> (&Value, OutcomePolicy) {
    let ReactionResolution::Completed { outcome, policy } = &receipt.resolution else {
        panic!("reaction faulted: {:?}", receipt.resolution);
    };
    (outcome, *policy)
}

fn constructor(value: &Value) -> &str {
    let Value::Variant { constructor, .. } = value else {
        panic!("expected variant, found {value:?}");
    };
    constructor
}

#[test]
fn l0_counter_executes_commit_and_boundary_clamp() {
    let program = checked_programs();
    let machine = "examples.programs@1::BoundedCounter";
    let configuration = Value::Record(vec![
        ("minimum".into(), Value::int(0)),
        ("maximum".into(), Value::int(2)),
        ("initial".into(), Value::int(1)),
    ]);
    let (instance, _) = program
        .machine_program
        .admit(machine, configuration, "programs/l0-counter")
        .expect("counter admission");
    assert_eq!(record_field(&instance.observation, "count"), &Value::int(1));

    let increment = Value::variant(format!("{machine}.Input"), "Increment", Vec::new());
    let first = program
        .machine_program
        .react(&instance, increment.clone())
        .expect("first increment");
    assert_eq!(completed(&first.receipt).1, OutcomePolicy::Commit);
    assert_eq!(constructor(completed(&first.receipt).0), "Accepted");
    assert_eq!(
        record_field(&first.instance.observation, "count"),
        &Value::int(2)
    );

    let clamped = program
        .machine_program
        .react(&first.instance, increment)
        .expect("clamped increment");
    assert_eq!(
        record_field(&clamped.instance.observation, "count"),
        &Value::int(2)
    );
    assert_eq!(
        record_field(&clamped.instance.observation, "at_maximum"),
        &Value::Bool(true)
    );
}

fn closed_variant(type_id: &str, case: &str) -> Value {
    Value::variant(type_id, case, Vec::new())
}

#[test]
fn l1_river_crossing_commits_safe_move_and_aborts_unsafe_request() {
    let program = checked_programs();
    let package = "examples.programs@1";
    let machine = format!("{package}::RiverCrossing");
    let TypeDef::Sum { constructors, .. } = &program.machine_program.machines[&machine].local_input
    else {
        panic!("RiverCrossing input must be a closed sum")
    };
    let cross = constructors
        .iter()
        .find(|constructor| constructor.name == "Cross")
        .expect("Cross input constructor");
    let (_, TypeRef::Option { value }) = &cross.fields[0] else {
        panic!("Cross.passenger must be Option<Cargo>")
    };
    let TypeRef::Named { id: cargo } = value.as_ref() else {
        panic!("Cross.passenger payload must retain nominal Cargo identity")
    };
    assert!(
        cargo.starts_with(&format!("{package}::__uhura_private_")) && cargo.ends_with("_Cargo"),
        "private Cargo must retain its owner-derived nominal identity: {cargo}"
    );
    assert!(
        program.machine_program.types.contains_key(cargo),
        "resolved Cargo identity must exist in the public machine IR"
    );
    let option_cargo = format!("Option<{cargo}>");
    let goat = closed_variant(cargo, "Goat");
    let passenger = Value::variant(
        option_cargo.clone(),
        "some",
        vec![(Some("value".into()), goat)],
    );
    let cross_goat = Value::variant(
        format!("{machine}.Input"),
        "Cross",
        vec![(Some("passenger".into()), passenger)],
    );
    let (instance, _) = program
        .machine_program
        .admit(&machine, Value::Unit, "programs/l1-river")
        .expect("river admission");
    let safe = program
        .machine_program
        .react(&instance, cross_goat)
        .expect("safe crossing");
    assert_eq!(completed(&safe.receipt).1, OutcomePolicy::Commit);
    assert_eq!(constructor(completed(&safe.receipt).0), "Accepted");

    let positions = record_field(&safe.instance.observation, "positions");
    let Value::Table { entries, .. } = positions else {
        panic!("positions are not a table: {positions:?}");
    };
    for entity in ["Farmer", "Goat"] {
        let side = entries
            .iter()
            .find_map(|(name, value)| (name == entity).then_some(value))
            .unwrap_or_else(|| panic!("missing `{entity}` position"));
        assert_eq!(constructor(side), "Right");
    }

    let wolf = closed_variant(cargo, "Wolf");
    let wolf_passenger = Value::variant(option_cargo, "some", vec![(Some("value".into()), wolf)]);
    let unsafe_cross = Value::variant(
        format!("{machine}.Input"),
        "Cross",
        vec![(Some("passenger".into()), wolf_passenger)],
    );
    let refused = program
        .machine_program
        .react(&safe.instance, unsafe_cross)
        .expect("unsafe crossing is a declared abort");
    assert_eq!(completed(&refused.receipt).1, OutcomePolicy::Abort);
    assert_eq!(constructor(completed(&refused.receipt).0), "Refused");
    assert_eq!(refused.instance.state, safe.instance.state);
}

#[test]
fn l2_supervisor_runs_before_commit_emits_command_and_rejects_duplicate_progress() {
    let program = checked_programs();
    let package = "examples.programs@1";
    let machine = format!("{package}::KeyedTaskSupervisor");
    let task = Value::Key {
        type_id: format!("{package}::TaskId"),
        value: Box::new(Value::Text("build".into())),
    };
    let submit = Value::variant(
        format!("{machine}.Input"),
        "Submit",
        vec![(Some("task".into()), task.clone())],
    );
    let (instance, _) = program
        .machine_program
        .admit(&machine, Value::Unit, "programs/l2-supervisor")
        .expect("supervisor admission");
    let started = program
        .machine_program
        .react(&instance, submit)
        .expect("submit");
    assert_eq!(completed(&started.receipt).1, OutcomePolicy::Commit);
    assert_eq!(started.receipt.ordered_commands.len(), 1);
    assert_eq!(
        constructor(&started.receipt.ordered_commands[0]),
        "Start",
        "before commit must consume one queued task"
    );
    assert_eq!(
        record_field(&started.instance.observation, "queue"),
        &Value::Seq(Vec::new())
    );

    let progress = Value::variant(
        format!("{machine}.Input"),
        "Progress",
        vec![
            (Some("task".into()), task),
            (Some("attempt".into()), Value::int(1)),
            (
                Some("value".into()),
                Value::Boundary(BoundaryNumber::Finite(
                    Decimal::from_str("0.5").expect("decimal"),
                )),
            ),
        ],
    );
    let advanced = program
        .machine_program
        .react(&started.instance, progress.clone())
        .expect("progress");
    assert_eq!(completed(&advanced.receipt).1, OutcomePolicy::Commit);
    assert_eq!(constructor(completed(&advanced.receipt).0), "Accepted");

    let duplicate = program
        .machine_program
        .react(&advanced.instance, progress)
        .expect("duplicate progress is a declared abort");
    assert_eq!(completed(&duplicate.receipt).1, OutcomePolicy::Abort);
    assert_eq!(constructor(completed(&duplicate.receipt).0), "Duplicate");
    assert_eq!(duplicate.instance.state, advanced.instance.state);
}

const IDENTITY_PROBE: &str = r#"
pub machine IdentityProbe {
  events { Run }
  outcomes { commit Done }
  state {}
  observe {}
  on Run { Done }
}
"#;

#[test]
fn logical_module_and_physical_path_do_not_enter_public_or_program_identity() {
    let check = |file, module, path| {
        let parsed = parse(
            SourceIdentity::new(file, "example.identity@1", module, path),
            IDENTITY_PROBE,
        );
        assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
        let output = check_module(&parsed.module);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.program.expect("identity probe")
    };
    let before = check(1, "probe", "probe.uhura");
    let moved = check(9, "shared::probe", "src/shared/probe.uhura");
    assert_eq!(
        before.machine_program.machines.keys().collect::<Vec<_>>(),
        moved.machine_program.machines.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        before.machine_program.program_hashes,
        moved.machine_program.program_hashes
    );
    assert!(
        before
            .machine_program
            .machines
            .contains_key("example.identity@1::IdentityProbe")
    );
}

#[test]
fn loop_decrease_proofs_are_erased_from_machine_program_identity() {
    let source = |measure: &str| {
        format!(
            r#"pub machine Countdown {{
  events {{
    Run,
  }}

  outcomes {{
    commit Done,
  }}

  state {{
    first: Seq<Nat> = [2, 1],
    second: Seq<Nat> = [2, 1],
  }}

  observe {{
    first,
    second,
  }}

  on Run {{
    while first.uncons() is Some(Uncons {{ head: first_head, tail: first_tail }})
      && second.uncons() is Some(Uncons {{ head: second_head, tail: second_tail }})
    decreases({measure}.len()) {{
      first = first_tail;
      second = second_tail;
    }}
    Done
  }}
}}
"#
        )
    };
    let check = |measure| {
        let source = source(measure);
        let parsed = parse(
            SourceIdentity::new(
                21,
                "example.proof-erasure@1",
                "countdown",
                "countdown.uhura",
            ),
            &source,
        );
        assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
        let output = check_module(&parsed.module);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        output.program.expect("proved countdown loop")
    };

    let first = check("first");
    let second = check("second");
    assert_eq!(
        first.machine_program.program_hashes, second.machine_program.program_hashes,
        "a compile-time termination witness must not enter executable identity",
    );
}

#[test]
fn loop_decrease_rejects_transitive_update_writes_to_the_measure() {
    let source = r#"pub machine Countdown {
  events {
    Run,
  }

  outcomes {
    commit Done,
  }

  state {
    remaining: Seq<Nat> = [2, 1],
  }

  observe {
    remaining,
  }

  update restore_one() {
    remaining = [1];
  }

  update restore_transitively() {
    restore_one();
  }

  on Run {
    while remaining.uncons() is Some(Uncons { head: _, tail })
    decreases(remaining.len()) {
      remaining = tail;
      restore_transitively();
    }
    Done
  }
}
"#;
    let parsed = parse(
        SourceIdentity::new(22, "example.loop-effects@1", "countdown", "countdown.uhura"),
        source,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    let output = check_module(&parsed.module);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "uhura/unproved-loop-decrease"),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
}

#[test]
fn loop_decrease_preserves_transitively_disjoint_update_calls() {
    let source = r#"pub machine Countdown {
  events {
    Run,
  }

  outcomes {
    commit Done,
  }

  state {
    remaining: Seq<Nat> = [2, 1],
    observed_step: Bool = false,
  }

  observe {
    remaining,
    observed_step,
  }

  update record_step() {
    observed_step = true;
  }

  update record_step_transitively() {
    record_step();
  }

  on Run {
    while remaining.uncons() is Some(Uncons { head: _, tail })
    decreases(remaining.len()) {
      remaining = tail;
      record_step_transitively();
    }
    Done
  }
}
"#;
    let parsed = parse(
        SourceIdentity::new(23, "example.loop-effects@1", "countdown", "countdown.uhura"),
        source,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    let output = check_module(&parsed.module);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    output
        .program
        .expect("proved loop with disjoint update effects");
}

#[test]
fn whitespace_and_comment_motion_do_not_enter_l0_l2_program_identity() {
    let baseline = checked_programs();
    let moved_source = format!(
        "\n// Formatting-only prefix: source coordinates move, semantics do not.\n\n{PROGRAMS}\n"
    );
    let parsed = parse(
        SourceIdentity::new(
            11,
            "examples.programs@1",
            "relocated::programs",
            "src/relocated/programs.uhura",
        ),
        &moved_source,
    );
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let output = check_module(&parsed.module);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let moved = output.program.expect("format-moved program");
    assert_eq!(
        baseline.machine_program.program_hashes,
        moved.machine_program.program_hashes
    );
}

#[test]
fn qualified_constructor_owner_is_checked_instead_of_discarded() {
    const SOURCE: &str = r#"
enum Expected { Same }
enum Wrong { Same }

pub machine QualifierProbe {
  events { Run }
  outcomes { commit Done }
  state { value: Expected = Wrong::Same }
  observe { value }
  on Run { Done }
}
"#;
    let parsed = parse(
        SourceIdentity::new(3, "example.qualifier@1", "probe", "probe.uhura"),
        SOURCE,
    );
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let output = check_module(&parsed.module);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "uhura/type-mismatch"),
        "wrong nominal owner must not silently resolve by the shared variant spelling: {:?}",
        output.diagnostics
    );
}
