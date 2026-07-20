use std::str::FromStr;

use serde_json::{Value as JsonValue, json};
use uhura_base::FileId;
use uhura_check::{check_project, check_v04_module};
use uhura_core::{
    BoundaryNumber, Checkpoint, Decimal, Instance, InstanceLifecycle, OutcomePolicy, Program,
    ProgramFault, ReactionResolution, Step, Value,
};
use uhura_syntax::v04::{SourceIdentity, parse as parse_v04};
use uhura_syntax::{SourceFile, parse_project};

const V03_PROGRAMS: &str =
    include_str!("../../../examples/programs/answers/uhura-0.3/programs.uhura");
const V04_PROGRAMS: &str =
    include_str!("../../../examples/programs/answers/uhura-0.4/programs.uhura");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Dialect {
    V03,
    V04,
}

#[derive(Clone, Copy)]
struct PathMapping {
    semantic: &'static str,
    v03: &'static str,
    v04: &'static str,
}

// These are language-version bindings, not fuzzy spelling rules. The
// differential comparison fails if a checked value contains an unmapped
// package path or nominal constructor.
const PATHS: &[PathMapping] = &[
    PathMapping {
        semantic: "counter.input",
        v03: "examples.programs.uhura_0_3@1::BoundedCounter.Input",
        v04: "examples.programs@1::BoundedCounter.Input",
    },
    PathMapping {
        semantic: "counter.outcome",
        v03: "examples.programs.uhura_0_3@1::BoundedCounter.Outcome",
        v04: "examples.programs@1::BoundedCounter.Outcome",
    },
    PathMapping {
        semantic: "river.input",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Input",
        v04: "examples.programs@1::RiverCrossing.Input",
    },
    PathMapping {
        semantic: "river.outcome",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Outcome",
        v04: "examples.programs@1::RiverCrossing.Outcome",
    },
    PathMapping {
        semantic: "river.side",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Side",
        v04: "examples.programs@1::__uhura_private_c663aed4ca5f51b76c05f18d_Side",
    },
    PathMapping {
        semantic: "river.entity",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Entity",
        v04: "examples.programs@1::__uhura_private_4fe21be47b48ae305a0fc116_Entity",
    },
    PathMapping {
        semantic: "river.cargo",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Cargo",
        v04: "examples.programs@1::__uhura_private_d087ae2ec90bdcbf23ba4935_Cargo",
    },
    PathMapping {
        semantic: "river.violation",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Violation",
        v04: "examples.programs@1::__uhura_private_506dac6d2b644a5e6813a617_Violation",
    },
    PathMapping {
        semantic: "river.status",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Status",
        v04: "examples.programs@1::__uhura_private_c098b431dd19f69d7ba28a6b_RiverStatus",
    },
    PathMapping {
        semantic: "river.crossing",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Crossing",
        v04: "examples.programs@1::__uhura_private_fdbcf8d3fb4666cf9e0ea93d_Crossing",
    },
    PathMapping {
        semantic: "river.refusal",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing.Refusal",
        v04: "examples.programs@1::__uhura_private_3d1a6247462dec1f54bfd11b_Refusal",
    },
    PathMapping {
        semantic: "supervisor.input",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Input",
        v04: "examples.programs@1::KeyedTaskSupervisor.Input",
    },
    PathMapping {
        semantic: "supervisor.command",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Command",
        v04: "examples.programs@1::KeyedTaskSupervisor.Command",
    },
    PathMapping {
        semantic: "supervisor.outcome",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Outcome",
        v04: "examples.programs@1::KeyedTaskSupervisor.Outcome",
    },
    PathMapping {
        semantic: "supervisor.task-id",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.TaskId",
        v04: "examples.programs@1::TaskId",
    },
    PathMapping {
        semantic: "supervisor.terminal",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Terminal",
        v04: "examples.programs@1::__uhura_private_8e9bd9985c08c62edd2cbd74_Terminal",
    },
    PathMapping {
        semantic: "supervisor.phase",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Phase",
        v04: "examples.programs@1::__uhura_private_45df36e28bd309067490fbcd_Phase",
    },
    PathMapping {
        semantic: "supervisor.task",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Task",
        v04: "examples.programs@1::__uhura_private_627d6c9470c84d769debb7c9_Task",
    },
    PathMapping {
        semantic: "supervisor.running",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor.Running",
        v04: "examples.programs@1::__uhura_private_67e27bf7cf38804f447bb2a3_Running",
    },
    PathMapping {
        semantic: "fault.input",
        v03: "differential.fault.v03@1::FaultProbe.Input",
        v04: "differential.fault@1::FaultProbe.Input",
    },
    PathMapping {
        semantic: "fault.outcome",
        v03: "differential.fault.v03@1::FaultProbe.Outcome",
        v04: "differential.fault@1::FaultProbe.Outcome",
    },
    PathMapping {
        semantic: "machine.counter",
        v03: "examples.programs.uhura_0_3@1::BoundedCounter",
        v04: "examples.programs@1::BoundedCounter",
    },
    PathMapping {
        semantic: "machine.river",
        v03: "examples.programs.uhura_0_3@1::RiverCrossing",
        v04: "examples.programs@1::RiverCrossing",
    },
    PathMapping {
        semantic: "machine.supervisor",
        v03: "examples.programs.uhura_0_3@1::KeyedTaskSupervisor",
        v04: "examples.programs@1::KeyedTaskSupervisor",
    },
    PathMapping {
        semantic: "machine.fault",
        v03: "differential.fault.v03@1::FaultProbe",
        v04: "differential.fault@1::FaultProbe",
    },
];

#[derive(Clone, Copy)]
struct ConstructorMapping {
    semantic: &'static str,
    v03: &'static str,
    v04: &'static str,
}

const CONSTRUCTORS: &[ConstructorMapping] = &[
    ConstructorMapping {
        semantic: "none",
        v03: "none",
        // Source constructors are Rust-shaped in 0.4, but the common kernel
        // keeps the built-in Option constructors lowercase.
        v04: "none",
    },
    ConstructorMapping {
        semantic: "some",
        v03: "some",
        v04: "some",
    },
    ConstructorMapping {
        semantic: "increment",
        v03: "increment",
        v04: "Increment",
    },
    ConstructorMapping {
        semantic: "decrement",
        v03: "decrement",
        v04: "Decrement",
    },
    ConstructorMapping {
        semantic: "reset",
        v03: "reset",
        v04: "Reset",
    },
    ConstructorMapping {
        semantic: "cross",
        v03: "cross",
        v04: "Cross",
    },
    ConstructorMapping {
        semantic: "submit",
        v03: "submit",
        v04: "Submit",
    },
    ConstructorMapping {
        semantic: "cancel",
        v03: "cancel",
        v04: "Cancel",
    },
    ConstructorMapping {
        semantic: "retry",
        v03: "retry",
        v04: "Retry",
    },
    ConstructorMapping {
        semantic: "progress",
        v03: "progress",
        v04: "Progress",
    },
    ConstructorMapping {
        semantic: "succeed",
        v03: "succeed",
        v04: "Succeed",
    },
    ConstructorMapping {
        semantic: "fail",
        v03: "fail",
        v04: "Fail",
    },
    ConstructorMapping {
        semantic: "crash",
        v03: "crash",
        v04: "Crash",
    },
    ConstructorMapping {
        semantic: "accepted",
        v03: "accepted",
        v04: "Accepted",
    },
    ConstructorMapping {
        semantic: "refused",
        v03: "refused",
        v04: "Refused",
    },
    ConstructorMapping {
        semantic: "duplicate",
        v03: "duplicate",
        v04: "Duplicate",
    },
    ConstructorMapping {
        semantic: "stale",
        v03: "stale",
        v04: "Stale",
    },
    ConstructorMapping {
        semantic: "invalid",
        v03: "invalid",
        v04: "Invalid",
    },
    ConstructorMapping {
        semantic: "start",
        v03: "start",
        v04: "Start",
    },
    ConstructorMapping {
        semantic: "left",
        v03: "left",
        v04: "Left",
    },
    ConstructorMapping {
        semantic: "right",
        v03: "right",
        v04: "Right",
    },
    ConstructorMapping {
        semantic: "farmer",
        v03: "farmer",
        v04: "Farmer",
    },
    ConstructorMapping {
        semantic: "wolf",
        v03: "wolf",
        v04: "Wolf",
    },
    ConstructorMapping {
        semantic: "goat",
        v03: "goat",
        v04: "Goat",
    },
    ConstructorMapping {
        semantic: "cabbage",
        v03: "cabbage",
        v04: "Cabbage",
    },
    ConstructorMapping {
        semantic: "wolf-with-goat",
        v03: "wolf_with_goat",
        v04: "WolfWithGoat",
    },
    ConstructorMapping {
        semantic: "goat-with-cabbage",
        v03: "goat_with_cabbage",
        v04: "GoatWithCabbage",
    },
    ConstructorMapping {
        semantic: "in-progress",
        v03: "in_progress",
        v04: "InProgress",
    },
    ConstructorMapping {
        semantic: "solved",
        v03: "solved",
        v04: "Solved",
    },
    ConstructorMapping {
        semantic: "passenger-not-with-farmer",
        v03: "passenger_not_with_farmer",
        v04: "PassengerNotWithFarmer",
    },
    ConstructorMapping {
        semantic: "unsafe",
        v03: "unsafe",
        v04: "Unsafe",
    },
    ConstructorMapping {
        semantic: "success",
        v03: "success",
        v04: "Success",
    },
    ConstructorMapping {
        semantic: "failure",
        v03: "failure",
        v04: "Failure",
    },
    ConstructorMapping {
        semantic: "queued",
        v03: "queued",
        v04: "Queued",
    },
    ConstructorMapping {
        semantic: "running",
        v03: "running",
        v04: "Running",
    },
    ConstructorMapping {
        semantic: "succeeded",
        v03: "succeeded",
        v04: "Succeeded",
    },
    ConstructorMapping {
        semantic: "failed",
        v03: "failed",
        v04: "Failed",
    },
    ConstructorMapping {
        semantic: "cancelled",
        v03: "cancelled",
        v04: "Cancelled",
    },
];

fn dialect_path(mapping: &PathMapping, dialect: Dialect) -> &'static str {
    match dialect {
        Dialect::V03 => mapping.v03,
        Dialect::V04 => mapping.v04,
    }
}

fn path(dialect: Dialect, semantic: &str) -> &'static str {
    let mapping = PATHS
        .iter()
        .find(|mapping| mapping.semantic == semantic)
        .unwrap_or_else(|| panic!("missing semantic path mapping `{semantic}`"));
    dialect_path(mapping, dialect)
}

fn spelling(dialect: Dialect, semantic: &str) -> &'static str {
    let mapping = CONSTRUCTORS
        .iter()
        .find(|mapping| mapping.semantic == semantic)
        .unwrap_or_else(|| panic!("missing semantic constructor mapping `{semantic}`"));
    match dialect {
        Dialect::V03 => mapping.v03,
        Dialect::V04 => mapping.v04,
    }
}

fn normalize_path(dialect: Dialect, value: &str) -> String {
    let mut normalized = value.to_owned();
    for mapping in PATHS {
        normalized = normalized.replace(dialect_path(mapping, dialect), mapping.semantic);
    }
    let stale_package = match dialect {
        Dialect::V03 => "examples.programs.uhura_0_3@1",
        Dialect::V04 => "examples.programs@1",
    };
    assert!(
        !normalized.contains(stale_package),
        "unmapped nominal path `{value}` in {dialect:?}"
    );
    normalized
}

fn normalize_constructor(dialect: Dialect, value: &str) -> &'static str {
    CONSTRUCTORS
        .iter()
        .find_map(|mapping| {
            let source = match dialect {
                Dialect::V03 => mapping.v03,
                Dialect::V04 => mapping.v04,
            };
            (source == value).then_some(mapping.semantic)
        })
        .unwrap_or_else(|| panic!("unmapped nominal constructor `{value}` in {dialect:?}"))
}

fn normalized_value(dialect: Dialect, value: &Value) -> JsonValue {
    match value {
        Value::Unit => json!({"kind": "unit"}),
        Value::Bool(value) => json!({"kind": "bool", "value": value}),
        Value::Integer { kind, value } => {
            json!({"kind": kind.name(), "value": value.to_string()})
        }
        Value::Decimal(value) => json!({"kind": "decimal", "value": value.canonical_text()}),
        Value::Ratio(value) => json!({"kind": "ratio", "value": value.canonical_text()}),
        Value::Boundary(value) => match value {
            BoundaryNumber::Finite(value) => {
                json!({"kind": "boundary", "case": "finite", "value": value.canonical_text()})
            }
            BoundaryNumber::Nan => json!({"kind": "boundary", "case": "nan"}),
            BoundaryNumber::PositiveInfinity => {
                json!({"kind": "boundary", "case": "positive-infinity"})
            }
            BoundaryNumber::NegativeInfinity => {
                json!({"kind": "boundary", "case": "negative-infinity"})
            }
        },
        Value::Text(value) => json!({"kind": "text", "value": value}),
        Value::Key { type_id, value } => json!({
            "kind": "key",
            "type": normalize_path(dialect, type_id),
            "value": normalized_value(dialect, value),
        }),
        Value::Tuple(values) => json!({
            "kind": "tuple",
            "items": values.iter().map(|value| normalized_value(dialect, value)).collect::<Vec<_>>(),
        }),
        Value::Record(fields) => json!({
            "kind": "record",
            "fields": fields.iter().map(|(name, value)| json!({
                "name": name,
                "value": normalized_value(dialect, value),
            })).collect::<Vec<_>>(),
        }),
        Value::Variant {
            type_id,
            constructor,
            fields,
        } => json!({
            "kind": "variant",
            "type": normalize_path(dialect, type_id),
            "case": normalize_constructor(dialect, constructor),
            // The paired sources intentionally move some payloads from
            // positional to named fields. Variant position and value are the
            // shared kernel contract; source parameter labels are not.
            "fields": fields.iter().map(|(_, value)| normalized_value(dialect, value)).collect::<Vec<_>>(),
        }),
        Value::Seq(values) => json!({
            "kind": "seq",
            "items": values.iter().map(|value| normalized_value(dialect, value)).collect::<Vec<_>>(),
        }),
        Value::NonEmpty(values) => json!({
            "kind": "nonempty",
            "items": values.iter().map(|value| normalized_value(dialect, value)).collect::<Vec<_>>(),
        }),
        Value::Set(values) => {
            let mut items = values
                .iter()
                .map(|value| normalized_value(dialect, value))
                .collect::<Vec<_>>();
            items.sort_by_key(JsonValue::to_string);
            json!({"kind": "set", "items": items})
        }
        Value::Map(entries) => {
            let mut entries = entries
                .iter()
                .map(|(key, value)| {
                    json!([
                        normalized_value(dialect, key),
                        normalized_value(dialect, value),
                    ])
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry[0].to_string());
            json!({"kind": "map", "entries": entries})
        }
        Value::Table { key_type, entries } => json!({
            "kind": "table",
            "key_type": normalize_path(dialect, key_type),
            "entries": entries.iter().map(|(key, value)| json!({
                "key": normalize_constructor(dialect, key),
                "value": normalized_value(dialect, value),
            })).collect::<Vec<_>>(),
        }),
    }
}

fn checked_v03(source: &str) -> Program {
    let parsed = parse_project([SourceFile::new(FileId(1), "programs.uhura", source)]);
    assert!(
        parsed.diagnostics.is_empty(),
        "0.3 parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    let checked = check_project(&parsed.project);
    assert!(
        checked.diagnostics.is_empty(),
        "0.3 check diagnostics:\n{:#?}",
        checked.diagnostics
    );
    checked.program.expect("checked 0.3 program")
}

fn checked_v04(source: &str, package: &str) -> Program {
    let parsed = parse_v04(
        SourceIdentity::new(2, package, "programs", "programs.uhura"),
        source,
    );
    assert!(
        parsed.is_ok(),
        "0.4 parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    let checked = check_v04_module(&parsed.module);
    assert!(
        checked.diagnostics.is_empty(),
        "0.4 check diagnostics:\n{:#?}",
        checked.diagnostics
    );
    checked.program.expect("checked 0.4 program")
}

struct Programs {
    v03: Program,
    v04: Program,
}

impl Programs {
    fn examples() -> Self {
        Self {
            v03: checked_v03(V03_PROGRAMS),
            v04: checked_v04(V04_PROGRAMS, "examples.programs@1"),
        }
    }

    fn program(&self, dialect: Dialect) -> &Program {
        match dialect {
            Dialect::V03 => &self.v03,
            Dialect::V04 => &self.v04,
        }
    }
}

struct Instances {
    v03: Instance,
    v04: Instance,
}

impl Instances {
    fn instance(&self, dialect: Dialect) -> &Instance {
        match dialect {
            Dialect::V03 => &self.v03,
            Dialect::V04 => &self.v04,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NormalizedResolution {
    Completed {
        outcome: JsonValue,
        policy: OutcomePolicy,
    },
    Fault(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedStep {
    protocol: String,
    sequence: u64,
    input: JsonValue,
    resolution: NormalizedResolution,
    commands: Vec<JsonValue>,
    observation: JsonValue,
    state: JsonValue,
    lifecycle: InstanceLifecycle,
    next_sequence: u64,
}

fn normalized_resolution(
    dialect: Dialect,
    resolution: &ReactionResolution,
) -> NormalizedResolution {
    match resolution {
        ReactionResolution::Completed { outcome, policy } => NormalizedResolution::Completed {
            outcome: normalized_value(dialect, outcome),
            policy: *policy,
        },
        ReactionResolution::Fault { fault } => NormalizedResolution::Fault(match fault {
            ProgramFault::InvariantViolation { .. } => "invariant-violation",
            ProgramFault::UnreachableReached { .. } => "unreachable-reached",
        }),
    }
}

fn normalized_step(dialect: Dialect, step: &Step) -> NormalizedStep {
    NormalizedStep {
        protocol: step.receipt.protocol.clone(),
        sequence: step.receipt.sequence,
        input: normalized_value(dialect, &step.receipt.input),
        resolution: normalized_resolution(dialect, &step.receipt.resolution),
        commands: step
            .receipt
            .ordered_commands
            .iter()
            .map(|command| normalized_value(dialect, command))
            .collect(),
        observation: normalized_value(dialect, &step.receipt.post_observation),
        state: normalized_value(dialect, &step.instance.state),
        lifecycle: step.instance.lifecycle,
        next_sequence: step.instance.next_sequence,
    }
}

fn assert_admission_equivalent(
    programs: &Programs,
    machine: &str,
    configuration: Value,
    id: &str,
) -> Instances {
    let (v03, v03_genesis) = programs
        .v03
        .admit(path(Dialect::V03, machine), configuration.clone(), id)
        .expect("0.3 admission");
    let (v04, v04_genesis) = programs
        .v04
        .admit(path(Dialect::V04, machine), configuration, id)
        .expect("0.4 admission");

    assert_eq!(v03_genesis.protocol, v04_genesis.protocol);
    assert_eq!(v03_genesis.sequence, v04_genesis.sequence);
    assert_eq!(
        v03_genesis.configuration_hash,
        v04_genesis.configuration_hash
    );
    assert_eq!(
        normalized_value(Dialect::V03, &v03_genesis.initial_observation),
        normalized_value(Dialect::V04, &v04_genesis.initial_observation),
    );
    assert_eq!(
        normalized_value(Dialect::V03, &v03.state),
        normalized_value(Dialect::V04, &v04.state)
    );
    assert_eq!(v03.lifecycle, InstanceLifecycle::Running);
    assert_eq!(v03.lifecycle, v04.lifecycle);
    assert_eq!(v03.next_sequence, v04.next_sequence);

    // Semantic receipt encodings intentionally include the independently
    // versioned machine program identity, so validate rather than equate them.
    programs
        .v03
        .canonical_genesis_receipt_bytes(path(Dialect::V03, machine), &v03_genesis)
        .expect("valid 0.3 genesis receipt");
    programs
        .v04
        .canonical_genesis_receipt_bytes(path(Dialect::V04, machine), &v04_genesis)
        .expect("valid 0.4 genesis receipt");

    Instances { v03, v04 }
}

fn react_pair(
    programs: &Programs,
    instances: &Instances,
    v03_input: Value,
    v04_input: Value,
) -> (Step, Step, NormalizedStep) {
    let v03 = programs.v03.react(&instances.v03, v03_input).unwrap();
    let v04 = programs.v04.react(&instances.v04, v04_input).unwrap();
    assert_reaction_receipts_valid(programs, instances, &v03, &v04);
    let v03_normalized = normalized_step(Dialect::V03, &v03);
    let v04_normalized = normalized_step(Dialect::V04, &v04);
    assert_eq!(v03_normalized, v04_normalized);
    (v03, v04, v03_normalized)
}

fn assert_reaction_receipts_valid(
    programs: &Programs,
    instances: &Instances,
    v03: &Step,
    v04: &Step,
) {
    programs
        .v03
        .canonical_reaction_receipt_bytes(&instances.v03.machine, &v03.receipt)
        .expect("valid 0.3 reaction receipt");
    programs
        .v04
        .canonical_reaction_receipt_bytes(&instances.v04.machine, &v04.receipt)
        .expect("valid 0.4 reaction receipt");
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedCheckpoint {
    protocol: String,
    configuration: JsonValue,
    state: JsonValue,
    inbox: Vec<JsonValue>,
    lifecycle: InstanceLifecycle,
    next_sequence: u64,
}

fn normalized_checkpoint(dialect: Dialect, checkpoint: &Checkpoint) -> NormalizedCheckpoint {
    NormalizedCheckpoint {
        protocol: checkpoint.protocol.clone(),
        configuration: normalized_value(dialect, &checkpoint.configuration),
        state: normalized_value(dialect, &checkpoint.state),
        inbox: checkpoint
            .inbox
            .iter()
            .map(|input| normalized_value(dialect, input))
            .collect(),
        lifecycle: checkpoint.lifecycle,
        next_sequence: checkpoint.next_sequence,
    }
}

fn assert_checkpoint_equivalent(
    programs: &Programs,
    machine: &str,
    instances: &Instances,
) -> (Checkpoint, Checkpoint) {
    let v03 = programs.v03.checkpoint(&instances.v03);
    let v04 = programs.v04.checkpoint(&instances.v04);
    assert_eq!(
        normalized_checkpoint(Dialect::V03, &v03),
        normalized_checkpoint(Dialect::V04, &v04)
    );

    programs
        .v03
        .canonical_checkpoint_bytes(&v03)
        .expect("valid 0.3 checkpoint");
    programs
        .v04
        .canonical_checkpoint_bytes(&v04)
        .expect("valid 0.4 checkpoint");

    for (dialect, checkpoint) in [(Dialect::V03, &v03), (Dialect::V04, &v04)] {
        let restored = programs
            .program(dialect)
            .restore(checkpoint)
            .expect("checkpoint restore");
        let original = instances.instance(dialect);
        assert_eq!(restored.state, original.state);
        assert_eq!(restored.observation, original.observation);
        assert_eq!(restored.next_sequence, original.next_sequence);
        assert_eq!(restored.lifecycle, original.lifecycle);
    }

    assert_eq!(normalize_path(Dialect::V03, &v03.machine), machine);
    assert_eq!(normalize_path(Dialect::V04, &v04.machine), machine);
    (v03, v04)
}

fn variant(
    dialect: Dialect,
    type_path: &str,
    constructor: &str,
    fields: Vec<(Option<String>, Value)>,
) -> Value {
    Value::variant(
        path(dialect, type_path),
        spelling(dialect, constructor),
        fields,
    )
}

fn option_cargo(dialect: Dialect, passenger: Option<&str>) -> Value {
    let cargo_type = path(dialect, "river.cargo");
    let option_type = format!("Option<{cargo_type}>");
    match passenger {
        None => Value::variant(option_type, spelling(dialect, "none"), Vec::new()),
        Some(passenger) => Value::variant(
            option_type,
            spelling(dialect, "some"),
            vec![(
                Some("value".into()),
                variant(dialect, "river.cargo", passenger, Vec::new()),
            )],
        ),
    }
}

#[test]
fn l0_counter_is_trace_equivalent_in_03_and_04() {
    let programs = Programs::examples();
    let configuration = Value::record([
        ("minimum".into(), Value::int(0)),
        ("maximum".into(), Value::int(2)),
        ("initial".into(), Value::int(0)),
    ])
    .unwrap();
    let mut instances = assert_admission_equivalent(
        &programs,
        "machine.counter",
        configuration,
        "differential/l0",
    );
    let events = [
        "increment",
        "increment",
        "increment",
        "decrement",
        "reset",
        "decrement",
    ];
    let expected_counts = [1, 2, 2, 1, 0, 0];
    let mut checkpoint = None;
    let mut receipts = Vec::new();

    for (index, event) in events.into_iter().enumerate() {
        let (v03, v04, normalized) = react_pair(
            &programs,
            &instances,
            variant(Dialect::V03, "counter.input", event, Vec::new()),
            variant(Dialect::V04, "counter.input", event, Vec::new()),
        );
        assert!(matches!(
            normalized.resolution,
            NormalizedResolution::Completed {
                policy: OutcomePolicy::Commit,
                ..
            }
        ));
        assert!(normalized.commands.is_empty());
        assert_eq!(
            normalized.observation["fields"][0]["value"]["value"],
            expected_counts[index].to_string()
        );
        receipts.push((v03.receipt.clone(), v04.receipt.clone()));
        instances = Instances {
            v03: v03.instance,
            v04: v04.instance,
        };
        if index == 1 {
            checkpoint = Some(assert_checkpoint_equivalent(
                &programs,
                "machine.counter",
                &instances,
            ));
        }
    }

    let invalid = Value::record([
        ("minimum".into(), Value::int(2)),
        ("maximum".into(), Value::int(1)),
        ("initial".into(), Value::int(1)),
    ])
    .unwrap();
    assert!(
        programs
            .v03
            .admit(
                path(Dialect::V03, "machine.counter"),
                invalid.clone(),
                "differential/l0-invalid-v03"
            )
            .is_err()
    );
    assert!(
        programs
            .v04
            .admit(
                path(Dialect::V04, "machine.counter"),
                invalid,
                "differential/l0-invalid-v04"
            )
            .is_err()
    );

    let (v03_checkpoint, v04_checkpoint) = checkpoint.unwrap();
    let restored = Instances {
        v03: programs.v03.restore(&v03_checkpoint).unwrap(),
        v04: programs.v04.restore(&v04_checkpoint).unwrap(),
    };
    let (v03_replay, v04_replay, _) = react_pair(
        &programs,
        &restored,
        variant(Dialect::V03, "counter.input", "increment", Vec::new()),
        variant(Dialect::V04, "counter.input", "increment", Vec::new()),
    );
    assert_eq!(v03_replay.receipt, receipts[2].0);
    assert_eq!(v04_replay.receipt, receipts[2].1);
}

#[test]
fn l1_river_commits_refuses_and_replays_equivalently() {
    let programs = Programs::examples();
    let mut instances =
        assert_admission_equivalent(&programs, "machine.river", Value::Unit, "differential/l1");
    let solution = [
        Some("goat"),
        None,
        Some("wolf"),
        Some("goat"),
        Some("cabbage"),
        None,
        Some("goat"),
    ];
    let mut checkpoint = None;
    let mut receipts = Vec::new();

    for (index, passenger) in solution.into_iter().enumerate() {
        let input = |dialect| {
            variant(
                dialect,
                "river.input",
                "cross",
                vec![(Some("passenger".into()), option_cargo(dialect, passenger))],
            )
        };
        let (v03, v04, normalized) = react_pair(
            &programs,
            &instances,
            input(Dialect::V03),
            input(Dialect::V04),
        );
        assert!(matches!(
            normalized.resolution,
            NormalizedResolution::Completed {
                policy: OutcomePolicy::Commit,
                ..
            }
        ));
        receipts.push((v03.receipt.clone(), v04.receipt.clone()));
        instances = Instances {
            v03: v03.instance,
            v04: v04.instance,
        };
        if index == 2 {
            checkpoint = Some(assert_checkpoint_equivalent(
                &programs,
                "machine.river",
                &instances,
            ));
        }
    }
    assert_eq!(
        normalized_value(Dialect::V03, &instances.v03.observation),
        normalized_value(Dialect::V04, &instances.v04.observation)
    );
    assert_eq!(
        normalized_value(Dialect::V03, &instances.v03.observation)["fields"][1]["value"]["case"],
        "solved"
    );

    let (v03_checkpoint, v04_checkpoint) = checkpoint.unwrap();
    let mut replay = Instances {
        v03: programs.v03.restore(&v03_checkpoint).unwrap(),
        v04: programs.v04.restore(&v04_checkpoint).unwrap(),
    };
    for (index, passenger) in solution.into_iter().enumerate().skip(3) {
        let input = |dialect| {
            variant(
                dialect,
                "river.input",
                "cross",
                vec![(Some("passenger".into()), option_cargo(dialect, passenger))],
            )
        };
        let (v03, v04, _) =
            react_pair(&programs, &replay, input(Dialect::V03), input(Dialect::V04));
        assert_eq!(v03.receipt, receipts[index].0);
        assert_eq!(v04.receipt, receipts[index].1);
        replay = Instances {
            v03: v03.instance,
            v04: v04.instance,
        };
    }
    assert_eq!(replay.v03.state, instances.v03.state);
    assert_eq!(replay.v04.state, instances.v04.state);

    // From the initial bank: wolf alone leaves goat with cabbage and must
    // abort. Abort is a completed declared outcome and stutters state.
    let initial = assert_admission_equivalent(
        &programs,
        "machine.river",
        Value::Unit,
        "differential/l1-abort",
    );
    let input = |dialect| {
        variant(
            dialect,
            "river.input",
            "cross",
            vec![(
                Some("passenger".into()),
                option_cargo(dialect, Some("wolf")),
            )],
        )
    };
    let (v03, v04, normalized) = react_pair(
        &programs,
        &initial,
        input(Dialect::V03),
        input(Dialect::V04),
    );
    assert!(matches!(
        normalized.resolution,
        NormalizedResolution::Completed {
            policy: OutcomePolicy::Abort,
            ..
        }
    ));
    assert_eq!(v03.instance.state, initial.v03.state);
    assert_eq!(v04.instance.state, initial.v04.state);
}

#[derive(Clone, Copy)]
enum SupervisorInput<'a> {
    Submit(&'a str),
    Cancel(&'a str),
    Retry(&'a str),
    Progress(&'a str, i64, &'a str),
    Succeed(&'a str, i64),
    Fail(&'a str, i64),
}

const SUPERVISOR_TRACE: [SupervisorInput<'static>; 26] = [
    SupervisorInput::Submit("A"),
    SupervisorInput::Submit("B"),
    SupervisorInput::Submit("C"),
    SupervisorInput::Submit("D"),
    SupervisorInput::Submit("A"),
    SupervisorInput::Progress("A", 2, "0.5"),
    SupervisorInput::Cancel("C"),
    SupervisorInput::Retry("C"),
    SupervisorInput::Cancel("B"),
    SupervisorInput::Succeed("B", 1),
    SupervisorInput::Fail("A", 1),
    SupervisorInput::Retry("A"),
    SupervisorInput::Progress("D", 1, "0.75"),
    SupervisorInput::Progress("D", 1, "0.5"),
    SupervisorInput::Succeed("D", 1),
    SupervisorInput::Progress("A", 1, "0.9"),
    SupervisorInput::Progress("A", 2, "0.6"),
    SupervisorInput::Progress("A", 2, "0.6"),
    SupervisorInput::Fail("A", 2),
    SupervisorInput::Retry("A"),
    SupervisorInput::Cancel("A"),
    SupervisorInput::Succeed("A", 3),
    SupervisorInput::Progress("C", 1, "1"),
    SupervisorInput::Succeed("C", 1),
    SupervisorInput::Succeed("C", 1),
    SupervisorInput::Retry("C"),
];

const SUPERVISOR_OUTCOMES: [&str; 26] = [
    "accepted",
    "accepted",
    "accepted",
    "accepted",
    "invalid",
    "invalid",
    "accepted",
    "accepted",
    "accepted",
    "stale",
    "accepted",
    "accepted",
    "accepted",
    "stale",
    "accepted",
    "stale",
    "accepted",
    "duplicate",
    "accepted",
    "accepted",
    "accepted",
    "stale",
    "accepted",
    "accepted",
    "duplicate",
    "invalid",
];

const SUPERVISOR_COMMAND_COUNTS: [usize; 26] = [
    1, 1, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0,
];

fn task_id(dialect: Dialect, name: &str) -> Value {
    Value::Key {
        type_id: path(dialect, "supervisor.task-id").into(),
        value: Box::new(Value::Text(name.into())),
    }
}

fn supervisor_input(dialect: Dialect, input: SupervisorInput<'_>) -> Value {
    let task = |name: &str| (Some("task".into()), task_id(dialect, name));
    let attempt = |value| (Some("attempt".into()), Value::int(value));
    let (constructor, fields) = match input {
        SupervisorInput::Submit(name) => ("submit", vec![task(name)]),
        SupervisorInput::Cancel(name) => ("cancel", vec![task(name)]),
        SupervisorInput::Retry(name) => ("retry", vec![task(name)]),
        SupervisorInput::Progress(name, serial, value) => (
            "progress",
            vec![
                task(name),
                attempt(serial),
                (
                    Some("value".into()),
                    Value::Boundary(BoundaryNumber::Finite(Decimal::from_str(value).unwrap())),
                ),
            ],
        ),
        SupervisorInput::Succeed(name, serial) => ("succeed", vec![task(name), attempt(serial)]),
        SupervisorInput::Fail(name, serial) => ("fail", vec![task(name), attempt(serial)]),
    };
    variant(dialect, "supervisor.input", constructor, fields)
}

fn normalized_outcome_name(step: &NormalizedStep) -> &str {
    let NormalizedResolution::Completed { outcome, .. } = &step.resolution else {
        panic!("expected completed outcome, found {:?}", step.resolution);
    };
    outcome["case"].as_str().expect("outcome constructor")
}

#[test]
fn l2_supervisor_preserves_outcomes_commands_checkpoints_and_replay() {
    let programs = Programs::examples();
    let mut instances = assert_admission_equivalent(
        &programs,
        "machine.supervisor",
        Value::Unit,
        "differential/l2",
    );
    let mut checkpoint = None;
    let mut receipts = Vec::new();

    for (index, input) in SUPERVISOR_TRACE.into_iter().enumerate() {
        let previous_v03 = instances.v03.state.clone();
        let previous_v04 = instances.v04.state.clone();
        let (v03, v04, normalized) = react_pair(
            &programs,
            &instances,
            supervisor_input(Dialect::V03, input),
            supervisor_input(Dialect::V04, input),
        );
        assert_eq!(
            normalized_outcome_name(&normalized),
            SUPERVISOR_OUTCOMES[index]
        );
        assert_eq!(normalized.commands.len(), SUPERVISOR_COMMAND_COUNTS[index]);
        let expected_policy = if SUPERVISOR_OUTCOMES[index] == "accepted" {
            OutcomePolicy::Commit
        } else {
            OutcomePolicy::Abort
        };
        assert!(matches!(
            normalized.resolution,
            NormalizedResolution::Completed { policy, .. } if policy == expected_policy
        ));
        if expected_policy == OutcomePolicy::Abort {
            assert_eq!(v03.instance.state, previous_v03);
            assert_eq!(v04.instance.state, previous_v04);
        }
        receipts.push((v03.receipt.clone(), v04.receipt.clone()));
        instances = Instances {
            v03: v03.instance,
            v04: v04.instance,
        };
        if index == 12 {
            checkpoint = Some(assert_checkpoint_equivalent(
                &programs,
                "machine.supervisor",
                &instances,
            ));
        }
    }

    let (v03_checkpoint, v04_checkpoint) = checkpoint.unwrap();
    let mut replay = Instances {
        v03: programs.v03.restore(&v03_checkpoint).unwrap(),
        v04: programs.v04.restore(&v04_checkpoint).unwrap(),
    };
    for (index, input) in SUPERVISOR_TRACE.into_iter().enumerate().skip(13) {
        let (v03, v04, _) = react_pair(
            &programs,
            &replay,
            supervisor_input(Dialect::V03, input),
            supervisor_input(Dialect::V04, input),
        );
        assert_eq!(v03.receipt, receipts[index].0);
        assert_eq!(v04.receipt, receipts[index].1);
        replay = Instances {
            v03: v03.instance,
            v04: v04.instance,
        };
    }
    assert_eq!(replay.v03.state, instances.v03.state);
    assert_eq!(replay.v04.state, instances.v04.state);
    assert_eq!(
        normalized_value(Dialect::V03, &instances.v03.observation),
        normalized_value(Dialect::V04, &instances.v04.observation),
    );
}

#[test]
fn l2_available_capacity_retains_its_nat_refinement_across_versions() {
    let programs = Programs::examples();
    let (_, v03) = programs
        .v03
        .admit(
            path(Dialect::V03, "machine.supervisor"),
            Value::Unit,
            "differential/l2-refinement-v03",
        )
        .unwrap();
    let (_, v04) = programs
        .v04
        .admit(
            path(Dialect::V04, "machine.supervisor"),
            Value::Unit,
            "differential/l2-refinement-v04",
        )
        .unwrap();
    assert_eq!(
        normalized_value(Dialect::V03, &v03.initial_observation),
        normalized_value(Dialect::V04, &v04.initial_observation)
    );
}

const V03_FAULT: &str = r#"language uhura 0.3
module differential.fault.v03@1

machine FaultProbe {
  input = crash
  command = Never
  outcome = accepted commit
  state {}
  observe { ready = true }
  on crash { unreachable }
}
"#;

const V04_FAULT: &str = r#"pub machine FaultProbe {
  events { Crash }
  outcomes { commit Accepted }
  state {}
  observe { ready: true }
  on Crash { unreachable; }
}
"#;

#[test]
fn reachable_unreachable_faults_and_faulted_checkpoints_are_equivalent() {
    let programs = Programs {
        v03: checked_v03(V03_FAULT),
        v04: checked_v04(V04_FAULT, "differential.fault@1"),
    };
    let instances = assert_admission_equivalent(
        &programs,
        "machine.fault",
        Value::Unit,
        "differential/fault",
    );
    let (v03, v04, normalized) = react_pair(
        &programs,
        &instances,
        variant(Dialect::V03, "fault.input", "crash", Vec::new()),
        variant(Dialect::V04, "fault.input", "crash", Vec::new()),
    );
    assert_eq!(
        normalized.resolution,
        NormalizedResolution::Fault("unreachable-reached")
    );
    assert_eq!(v03.instance.lifecycle, InstanceLifecycle::Faulted);
    assert_eq!(v04.instance.lifecycle, InstanceLifecycle::Faulted);
    assert_eq!(v03.instance.state, instances.v03.state);
    assert_eq!(v04.instance.state, instances.v04.state);

    let faulted = Instances {
        v03: v03.instance,
        v04: v04.instance,
    };
    let (v03_checkpoint, v04_checkpoint) =
        assert_checkpoint_equivalent(&programs, "machine.fault", &faulted);
    assert_eq!(v03_checkpoint.lifecycle, InstanceLifecycle::Faulted);
    assert_eq!(v04_checkpoint.lifecycle, InstanceLifecycle::Faulted);
}
