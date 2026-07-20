use std::str::FromStr;

use uhura_base::FileId;
use uhura_core::{
    BoundaryNumber, Checkpoint, Decimal, EvidencePresentationKind, Instance, Program,
    ReactionReceipt, ReactionResolution, Step, TypeRef, Value,
};
use uhura_syntax::{SourceFile, parse_project};

use super::check_project;

const PROGRAMS: &str = include_str!("../../../examples/programs/answers/uhura-0.3/programs.uhura");
const MACHINE: &str =
    include_str!("../../../examples/applications/a0-return-desk/answers/uhura-0.3/machine.uhura");
const WEB: &str =
    include_str!("../../../examples/applications/a0-return-desk/answers/uhura-0.3/web.uhura");
const CONFORMANCE: &str = include_str!(
    "../../../examples/applications/a0-return-desk/answers/uhura-0.3/conformance.uhura"
);

const COUNTER: &str = "examples.programs.uhura_0_3@1::BoundedCounter";
const RIVER: &str = "examples.programs.uhura_0_3@1::RiverCrossing";
const SUPERVISOR: &str = "examples.programs.uhura_0_3@1::KeyedTaskSupervisor";
const RETURN_DESK: &str = "app.return_desk.machine@1::ReturnDesk";

fn checked(files: &[(u32, &str, &str)]) -> super::CheckOutput {
    let parsed = parse_project(
        files
            .iter()
            .map(|(id, path, source)| SourceFile::new(FileId(*id), *path, source)),
    );
    assert!(
        parsed.diagnostics.is_empty(),
        "parse: {:#?}",
        parsed.diagnostics
    );
    check_project(&parsed.project)
}

fn checked_programs() -> Program {
    let output = checked(&[(0, "programs.uhura", PROGRAMS)]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics: {:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked program");
    assert_eq!(program.program_hashes.len(), 3);
    program
}

fn local_type(machine: &str, name: &str) -> String {
    format!("{machine}.{name}")
}

fn input(machine: &str, constructor: &str, fields: Vec<(Option<String>, Value)>) -> Value {
    Value::variant(local_type(machine, "Input"), constructor, fields)
}

fn completed_outcome(receipt: &ReactionReceipt) -> &Value {
    match &receipt.resolution {
        ReactionResolution::Completed { outcome, .. } => outcome,
        ReactionResolution::Fault { fault } => panic!("reaction faulted: {fault:?}"),
    }
}

fn constructor(value: &Value) -> &str {
    match value {
        Value::Variant { constructor, .. } => constructor,
        other => panic!("expected variant, got {other:?}"),
    }
}

fn variant_field(value: &Value, index: usize) -> &Value {
    match value {
        Value::Variant { fields, .. } => &fields[index].1,
        other => panic!("expected variant, got {other:?}"),
    }
}

fn record_field<'a>(value: &'a Value, name: &str) -> &'a Value {
    match value {
        Value::Record(fields) => fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("record has no `{name}` field")),
        other => panic!("expected record, got {other:?}"),
    }
}

#[test]
fn l0_l2_programs_lower_to_uhura_ir() {
    let output = checked(&[(0, "programs.uhura", PROGRAMS)]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{}",
        output
            .diagnostics
            .iter()
            .map(|value| format!("{} {} @ {:?}", value.code, value.message, value.span))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let program = output.program.expect("checked program");
    assert_eq!(program.machines.len(), 3);
    assert_eq!(program.program_hashes.len(), 3);
}

#[test]
fn a0_machine_ui_and_evidence_lower_as_one_project() {
    let output = checked(&[
        (0, "machine.uhura", MACHINE),
        (1, "web.uhura", WEB),
        (2, "conformance.uhura", CONFORMANCE),
    ]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{}",
        output
            .diagnostics
            .iter()
            .map(|value| format!("{} {} @ {:?}", value.code, value.message, value.span))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut program = output.program.expect("checked program");
    assert_eq!(program.machines.len(), 1);
    assert_eq!(program.presentations.len(), 1);
    assert_eq!(program.evidence.scenarios.len(), 19);
    assert_eq!(program.evidence.examples.len(), 12);
    assert_eq!(program.evidence.checkpoints.len(), 1);
    assert_eq!(program.route_tables.len(), 1);
    let machine = &program.machines[RETURN_DESK];
    for port in &machine.ports {
        let instance = port
            .contract_instance
            .as_ref()
            .unwrap_or_else(|| panic!("port `{}` has no resolved contract instance", port.name));
        assert_eq!(port.contract, instance.identity.to_string());
        assert_eq!(port.contract_hash, instance.content_hash);
        assert_eq!(
            port.type_arguments
                .iter()
                .map(TypeRef::canonical_name)
                .collect::<Vec<_>>(),
            instance
                .type_arguments
                .iter()
                .map(|argument| argument.argument.as_str().to_string())
                .collect::<Vec<_>>()
        );
    }
    let router = machine
        .ports
        .iter()
        .find(|port| port.name == "router")
        .expect("A0 router port");
    let router_instance = router
        .contract_instance
        .as_ref()
        .expect("A0 router contract instance");
    let router_configuration_hash = router_instance.configuration.hash();
    assert_eq!(
        router_instance.codecs[0].configuration_hash.as_deref(),
        Some(router_configuration_hash.as_str())
    );
    let hashes = program.program_hashes.clone();
    program.freeze_program_hashes();
    assert_eq!(
        program.program_hashes, hashes,
        "hash freezing is idempotent"
    );
    let report = program.run_evidence();
    assert!(report.passed, "evidence failures: {:#?}", report.failures);
}

#[test]
fn a0_lowering_is_independent_of_filesystem_module_order() {
    let output = checked(&[
        (0, "conformance.uhura", CONFORMANCE),
        (1, "web.uhura", WEB),
        (2, "machine.uhura", MACHINE),
    ]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{}",
        output
            .diagnostics
            .iter()
            .map(|value| format!("{} {} @ {:?}", value.code, value.message, value.span))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let program = output.program.unwrap();
    assert_eq!(program.evidence.scenarios.len(), 19);
    assert_eq!(program.route_tables.len(), 1);
}

#[test]
fn a0_preserves_revisions_beyond_javascript_safe_integer() {
    let output = checked(&[
        (0, "machine.uhura", MACHINE),
        (1, "web.uhura", WEB),
        (2, "conformance.uhura", CONFORMANCE),
    ]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.unwrap();
    let machine = "app.return_desk.machine@1::ReturnDesk";
    let integer = |kind: &str| {
        Value::from_wire_json(&serde_json::json!({
            "$": kind,
            "value": "9007199254740993",
        }))
        .unwrap()
    };
    let order_id = Value::Key {
        type_id: "app.return_desk.machine@1::OrderId".into(),
        value: Box::new(Value::Text("order-100".into())),
    };
    let wire = Value::record([
        ("id".into(), order_id),
        ("revision".into(), integer("Int")),
        ("lines".into(), Value::Seq(Vec::new())),
        ("allowed_methods".into(), Value::Seq(Vec::new())),
    ])
    .unwrap();
    let delivered = Value::variant(
        format!("{machine}::port.orders.Receive"),
        "orders.observed",
        vec![(Some("value".into()), wire)],
    );
    let (initial, _) = program.admit(machine, Value::Unit, "a0/bigint").unwrap();
    let initial_checkpoint = program.checkpoint(&initial);
    let first = program.react(&initial, delivered.clone()).unwrap();
    assert_eq!(constructor(completed_outcome(&first.receipt)), "accepted");
    let order = record_field(&first.instance.observation, "order");
    assert_eq!(constructor(order), "some");
    assert_eq!(
        record_field(variant_field(order, 0), "revision"),
        &integer("PositiveInt")
    );

    let receipt_json = first.receipt.to_canonical_string();
    assert!(receipt_json.contains("9007199254740993"));
    let decoded_receipt: ReactionReceipt = serde_json::from_str(&receipt_json).unwrap();
    assert_eq!(decoded_receipt, first.receipt);

    let restored_initial = program.restore(&initial_checkpoint).unwrap();
    let replay = program.react(&restored_initial, delivered).unwrap();
    assert_eq!(replay.receipt.to_canonical_string(), receipt_json);

    let checkpoint = program.checkpoint(&first.instance);
    let checkpoint_json = checkpoint.to_canonical_string();
    assert!(checkpoint_json.contains("9007199254740993"));
    let decoded_checkpoint: Checkpoint = serde_json::from_str(&checkpoint_json).unwrap();
    let restored = program.restore(&decoded_checkpoint).unwrap();
    assert_eq!(restored.state, first.instance.state);
    assert_eq!(restored.observation, first.instance.observation);
}

#[test]
fn invalid_program_is_gated_with_stable_diagnostic() {
    let output = checked(&[(
        0,
        "invalid.uhura",
        "language uhura 0.3\nmodule invalid@1\nconst x: Missing = 1\n",
    )]);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|value| value.code == "R1003"));
}

#[test]
fn diagnostic_registry_matches_preregistered_uhura_families() {
    use super::diagnostic::codes;

    assert_eq!(codes::HEADER, "R1002");
    assert_eq!(codes::UNKNOWN_NAME, "R1003");
    assert_eq!(codes::TYPE_MISMATCH, "R1004");
    assert_eq!(codes::INVALID_REFINEMENT, "R1005");
    assert_eq!(codes::NOT_EXHAUSTIVE, "R1006");
    assert_eq!(codes::INPUT_COVERAGE, "R1007");
    assert_eq!(codes::EFFECT, "R1008");
    assert_eq!(codes::DEPENDENCY_CYCLE, "R1009");
    assert_eq!(codes::TERMINATION, "R1010");
    assert_eq!(codes::PARTIAL_OPERATION, "R1011");
    assert_eq!(codes::TRANSITION_SHAPE, "R1012");
    assert_eq!(codes::PROJECTION_NOT_TOTAL, "R1013");
    assert_eq!(codes::UI_NOT_ENABLED, "R3001");
    assert_eq!(codes::EVIDENCE_NOT_ENABLED, "R3011");
    assert_eq!(codes::ROUTE_CODEC_MISMATCH, "R3012");
}

fn assert_semantic_error(source: &str, code: &str, rule: &str) {
    let output = checked(&[(0, "negative.uhura", source)]);
    assert!(
        output.program.is_none(),
        "negative source unexpectedly checked"
    );
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code && diagnostic.rule.contains(rule)),
        "expected {code}/{rule}, found {:#?}",
        output.diagnostics
    );
}

#[test]
fn handler_fallthrough_has_isolated_r1012_diagnostic() {
    let output = checked(&[(
        0,
        "transition-shape.uhura",
        r#"language uhura 0.3
module regression.transition_shape@1

machine Invalid {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  on go {}
}
"#,
    )]);

    assert!(output.program.is_none());
    assert_eq!(output.diagnostics.len(), 1, "{:#?}", output.diagnostics);
    assert_eq!(output.diagnostics[0].code, "R1012");
    assert_eq!(output.diagnostics[0].rule, "uhura/handler-fallthrough");
}

#[test]
fn projection_partial_index_has_isolated_r1013_diagnostic() {
    let output = checked(&[(
        0,
        "projection-totality.uhura",
        r#"language uhura 0.3
module regression.projection_totality@1

machine Invalid {
  input = go
  command = Never
  outcome = accepted commit
  state {
    values: Map<Text, Int> = Map.empty
  }
  observe {
    selected = values["missing"]
  }
  on go { finish accepted }
}
"#,
    )]);

    assert!(output.program.is_none());
    assert_eq!(output.diagnostics.len(), 1, "{:#?}", output.diagnostics);
    assert_eq!(output.diagnostics[0].code, "R1013");
    assert_eq!(output.diagnostics[0].rule, "uhura/projection-partial-index");
}

#[test]
fn integer_refinements_fail_closed_but_nat_plus_one_is_proved() {
    let valid = checked(&[(
        0,
        "refinement-ok.uhura",
        "language uhura 0.3\nmodule regression.refinement_ok@1\n\
         fn successor(value: Nat) -> PositiveInt = value + 1\n",
    )]);
    assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);

    for body in ["value", "0 - 1"] {
        assert_semantic_error(
            &format!(
                "language uhura 0.3\nmodule regression.refinement_bad@1\n\
                 fn bad(value: Nat) -> PositiveInt = {body}\n"
            ),
            "R1005",
            "unproved-integer-refinement",
        );
    }
}

#[test]
fn boundary_finite_accepts_and_executes_an_exact_decimal_expression() {
    const MACHINE: &str = "regression.boundary_finite@1::BoundaryFinite";
    let output = checked(&[(
        0,
        "boundary-finite.uhura",
        r#"language uhura 0.3
module regression.boundary_finite@1

machine BoundaryFinite {
  input = apply(value: Decimal)
  command = Never
  outcome = accepted commit
  state { value: Decimal = 0 }
  observe { boundary = finite(value + 0.25) }
  on apply(next) {
    set value = next
    finish accepted
  }
}
"#,
    )]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.expect("checked BoundaryNumber program");
    let (instance, genesis) = program.admit(MACHINE, Value::Unit, "boundary/1").unwrap();
    assert_eq!(
        record_field(&genesis.initial_observation, "boundary"),
        &Value::Boundary(BoundaryNumber::Finite(Decimal::from_str("0.25").unwrap()))
    );

    let step = program
        .react(
            &instance,
            input(
                MACHINE,
                "apply",
                vec![(
                    Some("value".into()),
                    Value::Decimal(Decimal::from_str("1.20").unwrap()),
                )],
            ),
        )
        .unwrap();
    assert_eq!(
        record_field(&step.instance.observation, "boundary"),
        &Value::Boundary(BoundaryNumber::Finite(Decimal::from_str("1.45").unwrap()))
    );
}

#[test]
fn match_totality_rejects_overlap_open_domains_and_arms_after_wildcard() {
    for (module, body, rule) in [
        (
            "overlap",
            "match value { true => 1 true => 2 false => 3 }",
            "overlapping-match-arm",
        ),
        ("open", "match value { true => 1 }", "non-exhaustive-match"),
        (
            "after",
            "match value { _ => 0 true => 1 }",
            "arm-after-wildcard",
        ),
    ] {
        assert_semantic_error(
            &format!(
                "language uhura 0.3\nmodule regression.{module}@1\n\
                 fn classify(value: Bool) -> Int = {body}\n"
            ),
            "R1006",
            rule,
        );
    }
}

#[test]
fn while_requires_a_proved_strict_decrease() {
    for assignment in ["n", "n + 1"] {
        assert_semantic_error(
            &format!(
                r#"language uhura 0.3
module regression.loop_bad@1

machine LoopBad {{
  input = go
  command = Never
  outcome = accepted commit
  state {{ n: Nat = 1 }}
  observe {{ n = n }}
  on go {{ finish accepted }}
  before commit {{
    while n > 0 decreases n {{
      set n = {assignment}
    }}
  }}
}}
"#
            ),
            "R1010",
            "unproved-loop-decrease",
        );
    }

    assert_semantic_error(
        r#"language uhura 0.3
module regression.loop_branch@1

machine LoopBranch {
  input = go
  command = Never
  outcome = accepted commit
  state { n: Nat = 2 }
  observe { n = n }
  on go { finish accepted }
  before commit {
    while n > 0 decreases n {
      if n > 1 {
        set n = n - 1
      }
    }
  }
}
"#,
        "R1010",
        "unproved-loop-decrease",
    );
}

#[test]
fn dependency_cycles_cover_imports_functions_and_derives() {
    let imports = checked(&[
        (
            0,
            "a.uhura",
            "language uhura 0.3\nmodule regression.a@1\n\
             import { b } from \"regression.b@1\"\nconst a: Int = b\n",
        ),
        (
            1,
            "b.uhura",
            "language uhura 0.3\nmodule regression.b@1\n\
             import { a } from \"regression.a@1\"\nconst b: Int = a\n",
        ),
    ]);
    assert!(imports.program.is_none());
    assert!(
        imports.diagnostics.iter().any(
            |diagnostic| diagnostic.code == "R1009" && diagnostic.rule.contains("import-cycle")
        )
    );

    assert_semantic_error(
        "language uhura 0.3\nmodule regression.functions@1\n\
         fn left() -> Int = right()\nfn right() -> Int = left()\n",
        "R1009",
        "recursive-function",
    );
    assert_semantic_error(
        r#"language uhura 0.3
module regression.derives@1
machine Cyclic {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  derive left: Int = right
  derive right: Int = left
  observe { value = left }
  on go { finish accepted }
}
"#,
        "R1009",
        "recursive-derive",
    );
}

#[test]
fn machine_topology_initializer_scope_and_reserved_patterns_are_closed() {
    assert_semantic_error(
        r#"language uhura 0.3
module regression.members@1
machine Duplicate {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  state {}
  observe {}
  on go { finish accepted }
}
"#,
        "R1002",
        "duplicate-machine-member",
    );
    assert_semantic_error(
        r#"language uhura 0.3
module regression.initializer@1
machine Initializer {
  input = go
  command = Never
  outcome = accepted commit
  state {
    first: Int = 0
    second: Int = first
  }
  observe {}
  on go { finish accepted }
}
"#,
        "R1003",
        "unknown-name",
    );
    assert_semantic_error(
        r#"language uhura 0.3
module regression.reserved_pattern@1
fn invalid(value: Int) -> Int =
  match value {
    Map => 0
  }
"#,
        "R1002",
        "reserved-pattern-binding",
    );
}

#[test]
fn collection_and_ratio_partiality_is_rejected_statically() {
    assert_semantic_error(
        r#"language uhura 0.3
module regression.dynamic_map@1
fn singleton(entry: Text) -> Map<Text, Int> = {
  entry: 1,
}
"#,
        "R1011",
        "dynamic-map-key",
    );
    assert_semantic_error(
        r#"language uhura 0.3
module regression.table_key@1
fn invalid() -> Table<Text, Int> = {
  "only": 1,
}
"#,
        "R1011",
        "table-key-not-finite",
    );
    assert_semantic_error(
        "language uhura 0.3\nmodule regression.ratio@1\n\
         fn invalid(value: Ratio) -> Ratio = value + value\n",
        "R1005",
        "ratio-arithmetic",
    );
}

#[test]
fn ui_event_contracts_admit_only_selected_pairs_and_payloads() {
    let source = r#"language uhura 0.3
module regression.ui_events@1

use ui

machine Controls {
  input = changed(BoundaryNumber) | pressed
  command = Never
  outcome = accepted commit
  state {}
  observe { ready = true }
  on changed(_) { finish accepted }
  on pressed { finish accepted }
}

ui ControlsWeb for Controls(view) {
  <main>
    <input
      type="number"
      on change -> changed(event.number)
    />
    <button on press -> pressed>Press</button>
  </main>
}
"#;
    let valid = checked(&[(0, "ui-events.uhura", source)]);
    assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);

    for invalid in [
        source.replacen("on change ->", "on input ->", 1),
        source.replacen("type=\"number\"", "type=\"text\"", 1),
    ] {
        assert_semantic_error(&invalid, "R3006", "ui-event");
    }
    assert_semantic_error(
        &source.replacen("event.number", "event.text", 1),
        "R1003",
        "unknown-member",
    );
    assert_semantic_error(
        &source.replacen(
            "on press -> pressed",
            "on press -> changed(event.number)",
            1,
        ),
        "R1003",
        "unknown-member",
    );
}

#[test]
fn semantic_ui_primitives_have_closed_props_events_and_payloads() {
    let source = r#"language uhura 0.3
module regression.semantic_ui@1

use ui

machine NativeControls {
  input =
    | pressed
    | text_changed(Text)
    | near_end
    | activated
    | double_activated
  command = Never
  outcome = accepted commit
  state {}
  observe { ready = true }
  on pressed { finish accepted }
  on text_changed(_) { finish accepted }
  on near_end { finish accepted }
  on activated { finish accepted }
  on double_activated { finish accepted }
}

ui NativeControlsWeb for NativeControls(view) {
  <view class="root" role="list">
    <scroll
      class="strip"
      direction="horizontal"
      on near-end -> near_end
    >
      <pager label="Gallery" indicator="dots">
        <img src="photo" alt="A photo" />
      </pager>
    </scroll>
    <video
      class="clip"
      src="clip"
      poster="poster"
      label="A clip"
      autoplay={false}
      muted={true}
      loop={true}
      controls={true}
      playsinline={true}
    />
    <icon name="heart" family="lucide" />
    <textfield
      value="draft"
      placeholder="Write"
      label="Caption"
      disabled={false}
      on change -> text_changed(event.text)
    />
    <region
      label="Open"
      supplementary={true}
      on activate -> activated
    >
      <text class="label">Open</text>
    </region>
    <region
      label="Inspect"
      on activate-double -> double_activated
    >
      <img src="thumb" decorative={true} />
    </region>
    <button
      class="save"
      label="Save"
      busy={false}
      pressed={false}
      current={true}
      disabled={false}
      on press -> pressed
    >
      <text>Save</text>
    </button>
  </view>
}
"#;
    let valid = checked(&[(0, "semantic-ui.uhura", source)]);
    assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);
    assert!(valid.program.is_some());

    for (invalid, rule) in [
        (
            source.replacen("direction=\"horizontal\"", "direction=\"diagonal\"", 1),
            "ui-attribute-value",
        ),
        (
            source.replacen("<icon name=\"heart\"", "<icon", 1),
            "missing-ui-attribute",
        ),
        (
            source.replacen(
                "src=\"photo\" alt=\"A photo\"",
                "src=\"photo\" alt=\"A photo\" decorative={false}",
                1,
            ),
            "ui-attribute-alternative",
        ),
        (
            source.replacen("      on change -> text_changed(event.text)\n", "", 1),
            "ui-controlled-field",
        ),
        (
            source.replacen("controls={true}", "controls=\"true\"", 1),
            "ui-attribute-type",
        ),
        (
            source.replacen("on activate -> activated", "on press -> activated", 1),
            "ui-event",
        ),
        (
            source.replacen(
                "<view class=\"root\"",
                "<view class=\"root\" mystery=\"value\"",
                1,
            ),
            "invalid-ui-attribute",
        ),
    ] {
        assert_semantic_error(&invalid, "R3006", rule);
    }
}

#[test]
fn before_commit_allows_fault_but_not_outcome_replacement() {
    assert_semantic_error(
        r#"language uhura 0.3
module regression.before_finish@1
machine Invalid {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  on go { finish accepted }
  before commit { finish accepted }
}
"#,
        "R1008",
        "terminal-in-before-commit",
    );

    let output = checked(&[(
        0,
        "before-fault.uhura",
        r#"language uhura 0.3
module regression.before_fault@1
machine Faulting {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  on go { finish accepted }
  before commit { unreachable }
}
"#,
    )]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.unwrap();
    let machine = "regression.before_fault@1::Faulting";
    let (instance, _) = program.admit(machine, Value::Unit, "fault/1").unwrap();
    let step = program
        .react(&instance, input(machine, "go", Vec::new()))
        .unwrap();
    assert!(matches!(
        step.receipt.resolution,
        ReactionResolution::Fault { .. }
    ));
}

#[test]
fn semantic_source_ids_ignore_path_and_whitespace() {
    let source = r#"language uhura 0.3
module regression.source_id@1
machine Stable {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  invariant { true }
  observe {}
  on go { finish accepted }
}
"#;
    let spaced = source.replace("machine Stable", "\n\nmachine Stable");
    let left = checked(&[(3, "first/location.uhura", source)])
        .program
        .unwrap();
    let right = checked(&[(9, "moved/location.uhura", &spaced)])
        .program
        .unwrap();
    let left = &left.machines["regression.source_id@1::Stable"];
    let right = &right.machines["regression.source_id@1::Stable"];
    assert_eq!(left.source.id, right.source.id);
    assert_eq!(left.invariants[0].1.id, right.invariants[0].1.id);
    assert_ne!(left.source.path, right.source.path);
    assert_ne!(left.source.start, right.source.start);
}

#[test]
fn evidence_physical_sources_survive_checker_and_runtime_registration() {
    let output = checked(&[(
        7,
        "nested/conformance.uhura",
        r#"language uhura 0.3
module regression.evidence_sources@1

use evidence

machine Proof {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  on go { finish accepted }
}

scenario origin for Proof {
  start
  pin ready
}

example missing_example = origin::absent
checkpoint missing_checkpoint = origin::absent
"#,
    )]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.expect("evidence program");
    let module = "regression.evidence_sources@1";
    let example = format!("{module}::missing_example");
    let checkpoint = format!("{module}::missing_checkpoint");
    for source in [
        &program.evidence.example_sources[&example],
        &program.evidence.checkpoint_sources[&checkpoint],
    ] {
        assert_eq!(source.path, "nested/conformance.uhura");
        assert!(source.start > 0);
        assert!(source.end > source.start);
    }

    let report = program.run_evidence();
    let pin = &report.artifacts.pins[&format!("{module}::origin::ready")];
    assert_eq!(pin.source.path, "nested/conformance.uhura");
    assert!(pin.source.start > 0);
    assert!(report.failures.iter().any(|failure| {
        failure.source == program.evidence.example_sources[&example]
            && failure.source_id == program.evidence.example_sources[&example].id
    }));
    assert!(report.failures.iter().any(|failure| {
        failure.source == program.evidence.checkpoint_sources[&checkpoint]
            && failure.source_id == program.evidence.checkpoint_sources[&checkpoint].id
    }));
}

#[test]
fn evidence_examples_target_one_presentation_with_editor_metadata() {
    let output = checked(&[(
        0,
        "targeted-example.uhura",
        r#"language uhura 0.3
module regression.targeted_example@1

use ui
use evidence

machine Proof {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe { ready = true }
  on go { finish accepted }
}

ui ProofWeb for Proof(view) {
  <main><p>Ready</p></main>
}

scenario origin for Proof {
  start
  pin ready
}

example ready for ProofWeb as component default note "Ready state" = origin::ready
"#,
    )]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.expect("targeted evidence program");
    let example_id = "regression.targeted_example@1::ready";
    let metadata = &program.evidence.example_metadata[example_id];
    assert_eq!(
        metadata.presentation.as_deref(),
        Some("regression.targeted_example@1::ProofWeb")
    );
    assert_eq!(metadata.kind, Some(EvidencePresentationKind::Component));
    assert!(metadata.is_default);
    assert_eq!(metadata.note.as_deref(), Some("Ready state"));

    let report = program.run_evidence();
    assert_eq!(
        report.artifacts.examples[example_id].metadata,
        metadata.clone()
    );
}

#[test]
fn evidence_examples_cannot_target_a_different_machine_presentation() {
    let output = checked(&[(
        0,
        "mismatched-example.uhura",
        r#"language uhura 0.3
module regression.mismatched_example@1

use ui
use evidence

machine First {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  on go { finish accepted }
}

machine Second {
  input = go
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  on go { finish accepted }
}

ui SecondWeb for Second(view) {
  <main />
}

scenario first for First {
  start
  pin ready
}

example invalid for SecondWeb as page = first::ready
"#,
    )]);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.rule == "uhura/example-presentation-machine" })
    );
    assert!(output.program.is_none());
}

fn configured_evidence_project(configuration: &str) -> String {
    format!(
        r#"language uhura 0.3
module regression.configured_evidence@1

use evidence

fn dynamic_config() -> {{ initial: Int, step: Int }} =
  {{ initial: 1, step: 2 }}

machine ConfiguredCounter {{
  config {{
    initial: Int
    step: Int
  }}

  input = advance
  command = Never
  outcome = accepted commit

  state {{
    count: Int = initial
  }}

  observe {{
    count = count
  }}

  on advance {{
    set count = count + step
    finish accepted
  }}
}}

scenario configured for ConfiguredCounter{configuration} {{
  start
  send advance
  expect accepted commands []
  pin ready
}}

scenario resumed from configured::ready {{
  expect restore commands []
  send advance
  expect accepted commands []
  pin final
}}

example final = resumed::final
"#
    )
}

#[test]
fn configured_evidence_is_checked_admitted_and_replayed_deterministically() {
    let source = configured_evidence_project("({ initial: 1, step: 2 })");
    let output = checked(&[(0, "configured-evidence.uhura", &source)]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.expect("configured evidence program");
    let scenario = &program.evidence.scenarios["regression.configured_evidence@1::configured"];
    let uhura_core::ScenarioOrigin::Machine { configuration, .. } = &scenario.origin else {
        panic!("expected fresh machine scenario");
    };
    let expected_configuration = Value::record([
        ("initial".into(), Value::int(1)),
        ("step".into(), Value::int(2)),
    ])
    .unwrap();
    assert_eq!(configuration, &expected_configuration);

    let first = program.run_evidence();
    let second = program.run_evidence();
    assert_eq!(first, second, "evidence execution must be deterministic");
    assert!(first.passed, "{:#?}", first.failures);
    let fresh =
        &first.artifacts.pins["regression.configured_evidence@1::configured::ready"].snapshot;
    let restored =
        &first.artifacts.pins["regression.configured_evidence@1::resumed::final"].snapshot;
    assert_eq!(fresh.configuration, expected_configuration);
    assert_eq!(restored.configuration, expected_configuration);
    assert_eq!(record_field(&fresh.observation, "count"), &Value::int(3));
    assert_eq!(record_field(&restored.observation, "count"), &Value::int(5));
}

#[test]
fn configured_evidence_requires_configuration_for_non_unit_machines() {
    let source = configured_evidence_project("");
    let output = checked(&[(0, "missing-config.uhura", &source)]);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/missing-scenario-configuration"
            && diagnostic.message.contains("for ConfiguredCounter(...)")
    }));
}

#[test]
fn configured_evidence_rejects_the_wrong_configuration_type() {
    let source = configured_evidence_project("({ initial: \"wrong\", step: 2 })");
    let output = checked(&[(0, "wrong-config.uhura", &source)]);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "R1004"
            && diagnostic.message.contains("expected")
            && diagnostic.message.contains("Int")
            && diagnostic.message.contains("Text")
    }));
}

#[test]
fn configured_evidence_rejects_non_constant_configuration() {
    let source = configured_evidence_project("(dynamic_config())");
    let output = checked(&[(0, "dynamic-config.uhura", &source)]);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura/non-constant-scenario-configuration"
            && diagnostic.message.contains("not compile-time total")
    }));
}

#[test]
fn resolved_local_and_port_inputs_require_exact_handler_coverage() {
    let source = r#"language uhura 0.3
module regression.input_coverage@1

import { Observation } from "uhura.observation@1"

machine MissingHandlers {
  port orders: Observation<Int>
  input = ping | omitted
  command = Never
  outcome = accepted commit
  state {}
  observe {}

  on ping {
    finish accepted
  }
}
"#;
    let output = checked(&[(0, "input-coverage.uhura", source)]);
    assert!(output.program.is_none());
    let missing = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "R1007")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert!(missing.iter().any(|message| message.contains("`omitted`")));
    assert!(
        missing
            .iter()
            .any(|message| message.contains("`orders.observed`"))
    );
}

#[test]
fn controlled_match_binding_does_not_escape_into_its_continuation() {
    let source = r#"language uhura 0.3
module regression.pattern_scope@1

machine Shadow {
  type Box = empty | full(Int)

  input = apply(Box)
  command = Never
  outcome = accepted commit | invalid abort

  state {
    value: Box = empty
  }

  observe {
    value = value
  }

  on apply(candidate) {
    let next =
      match candidate {
        full(value) => value
        empty => finish invalid
      }

    match value {
      empty => {
        set value = full(next)
      }
      full(_) => {}
    }

    finish accepted
  }
}
"#;
    let output = checked(&[(0, "pattern-scope.uhura", source)]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.unwrap();
    let machine = "regression.pattern_scope@1::Shadow";
    let boxed = Value::variant(
        local_type(machine, "Box"),
        "full",
        vec![(None, Value::int(7))],
    );
    let (instance, _) = program.admit(machine, Value::Unit, "scope/1").unwrap();
    let step = program
        .react(
            &instance,
            input(machine, "apply", vec![(None, boxed.clone())]),
        )
        .unwrap();
    assert_eq!(constructor(completed_outcome(&step.receipt)), "accepted");
    assert_eq!(record_field(&step.instance.state, "value"), &boxed);
}

#[test]
fn l0_counter_matches_canonical_and_adversarial_harness() {
    let program = checked_programs();
    let configuration = |minimum, maximum, initial| {
        Value::record([
            ("minimum".into(), Value::int(minimum)),
            ("maximum".into(), Value::int(maximum)),
            ("initial".into(), Value::int(initial)),
        ])
        .unwrap()
    };
    let run = |config: Value, inputs: &[&str], id: &str| {
        let (mut instance, genesis) = program.admit(COUNTER, config, id).unwrap();
        let mut observations = vec![genesis.initial_observation];
        for name in inputs {
            let step = program
                .react(&instance, input(COUNTER, name, Vec::new()))
                .unwrap();
            assert_eq!(constructor(completed_outcome(&step.receipt)), "accepted");
            assert!(step.receipt.ordered_commands.is_empty());
            instance = step.instance;
            observations.push(instance.observation.clone());
        }
        observations
    };
    let count = |observation: &Value| record_field(observation, "count").clone();

    let canonical = run(
        configuration(0, 2, 0),
        &[
            "increment",
            "increment",
            "increment",
            "decrement",
            "reset",
            "decrement",
        ],
        "l0/canonical",
    );
    assert_eq!(
        canonical.iter().map(count).collect::<Vec<_>>(),
        [0, 1, 2, 2, 1, 0, 0]
            .into_iter()
            .map(Value::int)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        record_field(&canonical[0], "at_minimum"),
        &Value::Bool(true)
    );
    assert_eq!(
        record_field(&canonical[2], "at_maximum"),
        &Value::Bool(true)
    );

    for (index, (config, inputs, expected)) in [
        (
            configuration(7, 7, 7),
            vec!["increment", "decrement", "reset"],
            vec![7, 7, 7],
        ),
        (
            configuration(-2, 1, -1),
            vec!["decrement", "decrement", "increment", "reset"],
            vec![-2, -2, -1, -1],
        ),
        (
            configuration(0, 2, 1),
            vec!["increment", "reset", "decrement", "reset"],
            vec![2, 1, 0, 1],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let observations = run(config, &inputs, &format!("l0/adversarial/{index}"));
        assert_eq!(
            observations.iter().skip(1).map(count).collect::<Vec<_>>(),
            expected.into_iter().map(Value::int).collect::<Vec<_>>()
        );
    }

    let degenerate = run(configuration(7, 7, 7), &[], "l0/degenerate");
    assert_eq!(
        record_field(&degenerate[0], "at_minimum"),
        &Value::Bool(true)
    );
    assert_eq!(
        record_field(&degenerate[0], "at_maximum"),
        &Value::Bool(true)
    );
    assert!(
        program
            .admit(COUNTER, configuration(2, 1, 1), "l0/invalid-bounds")
            .is_err()
    );
    assert!(
        program
            .admit(COUNTER, configuration(0, 2, 3), "l0/invalid-initial")
            .is_err()
    );
}

const RIVER_ORACLE: [(&str, [&str; 4]); 10] = [
    ("0000", ["U:WG+GC", "U:GC", "A:1010:goat:01", "U:WG"]),
    (
        "0001",
        ["U:WG", "A:1101:wolf:01", "A:1011:goat:01", "P:cabbage"],
    ),
    (
        "0010",
        [
            "A:1010:none:01",
            "A:1110:wolf:01",
            "P:goat",
            "A:1011:cabbage:01",
        ],
    ),
    (
        "0100",
        ["U:GC", "P:wolf", "A:1110:goat:01", "A:1101:cabbage:01"],
    ),
    (
        "0101",
        ["A:1101:none:01", "P:wolf", "A:1111:goat:01", "P:cabbage"],
    ),
    (
        "1010",
        ["A:0010:none:10", "P:wolf", "A:0000:goat:10", "P:cabbage"],
    ),
    (
        "1011",
        ["U:GC", "P:wolf", "A:0001:goat:10", "A:0010:cabbage:10"],
    ),
    (
        "1101",
        [
            "A:0101:none:10",
            "A:0001:wolf:10",
            "P:goat",
            "A:0100:cabbage:10",
        ],
    ),
    (
        "1110",
        ["U:WG", "A:0010:wolf:10", "A:0100:goat:10", "P:cabbage"],
    ),
    ("1111", ["U:WG+GC", "U:GC", "A:0101:goat:10", "U:WG"]),
];

fn river_side(right: bool) -> Value {
    Value::variant(
        local_type(RIVER, "Side"),
        if right { "right" } else { "left" },
        Vec::new(),
    )
}

fn river_positions(bits: &str) -> Value {
    let entries = ["farmer", "wolf", "goat", "cabbage"]
        .into_iter()
        .zip(bits.bytes())
        .map(|(entity, bit)| (entity.into(), river_side(bit == b'1')))
        .collect();
    Value::Table {
        key_type: local_type(RIVER, "Entity"),
        entries,
    }
}

fn river_instance(program: &Program, bits: &str, id: &str) -> Instance {
    let (initial, _) = program.admit(RIVER, Value::Unit, id).unwrap();
    let mut checkpoint = program.checkpoint(&initial);
    checkpoint.state = Value::record([("positions".into(), river_positions(bits))]).unwrap();
    program.restore(&checkpoint).unwrap()
}

fn river_input(passenger: Option<&str>) -> Value {
    let cargo_type = local_type(RIVER, "Cargo");
    let passenger = passenger.map_or_else(
        || Value::variant(format!("Option<{cargo_type}>"), "none", Vec::new()),
        |name| {
            Value::variant(
                format!("Option<{cargo_type}>"),
                "some",
                vec![(
                    Some("value".into()),
                    Value::variant(cargo_type.clone(), name, Vec::new()),
                )],
            )
        },
    );
    input(RIVER, "cross", vec![(Some("passenger".into()), passenger)])
}

fn river_bits(instance: &Instance) -> String {
    let positions = record_field(&instance.state, "positions");
    let Value::Table { entries, .. } = positions else {
        panic!("river positions are not a table")
    };
    ["farmer", "wolf", "goat", "cabbage"]
        .into_iter()
        .map(|entity| {
            let side = entries
                .iter()
                .find(|(name, _)| name == entity)
                .map(|(_, value)| constructor(value))
                .unwrap();
            if side == "right" { '1' } else { '0' }
        })
        .collect()
}

fn river_step_key(step: &Step) -> String {
    let outcome = completed_outcome(&step.receipt);
    match constructor(outcome) {
        "accepted" => {
            let crossing = variant_field(outcome, 0);
            let passenger = record_field(crossing, "passenger");
            let passenger = if constructor(passenger) == "none" {
                "none"
            } else {
                constructor(variant_field(passenger, 0))
            };
            let departure = constructor(record_field(crossing, "departure"));
            let arrival = constructor(record_field(crossing, "arrival"));
            format!(
                "A:{}:{passenger}:{}{}",
                river_bits(&step.instance),
                if departure == "right" { 1 } else { 0 },
                if arrival == "right" { 1 } else { 0 }
            )
        }
        "refused" => {
            let refusal = variant_field(outcome, 0);
            match constructor(refusal) {
                "passenger_not_with_farmer" => {
                    format!("P:{}", constructor(variant_field(refusal, 0)))
                }
                "unsafe" => {
                    let Value::NonEmpty(violations) = variant_field(refusal, 0) else {
                        panic!("unsafe refusal does not carry NonEmpty violations")
                    };
                    let profile = violations
                        .iter()
                        .map(|value| match constructor(value) {
                            "wolf_with_goat" => "WG",
                            "goat_with_cabbage" => "GC",
                            other => panic!("unknown violation `{other}`"),
                        })
                        .collect::<Vec<_>>()
                        .join("+");
                    format!("U:{profile}")
                }
                other => panic!("unknown refusal `{other}`"),
            }
        }
        other => panic!("unknown river outcome `{other}`"),
    }
}

#[test]
fn l1_river_matches_all_forty_safe_state_evaluations_and_replay() {
    let program = checked_programs();
    let passengers = [None, Some("wolf"), Some("goat"), Some("cabbage")];
    let mut accepted = 0;
    let mut passenger_refusals = 0;
    let mut unsafe_wg = 0;
    let mut unsafe_gc = 0;
    let mut unsafe_both = 0;

    for (bits, expected) in RIVER_ORACLE {
        for (index, passenger) in passengers.into_iter().enumerate() {
            let instance = river_instance(&program, bits, &format!("l1/{bits}/{index}"));
            let step = program.react(&instance, river_input(passenger)).unwrap();
            let actual = river_step_key(&step);
            assert_eq!(actual, expected[index], "frozen oracle {bits}/{index}");
            if actual.starts_with("A:") {
                accepted += 1;
                let reverse = program
                    .react(&step.instance, river_input(passenger))
                    .unwrap();
                assert_eq!(reverse.instance.state, instance.state, "accepted reverse");
                assert!(river_step_key(&reverse).starts_with("A:"));
            } else {
                assert_eq!(step.instance.state, instance.state, "refusal stutters");
                let repeated = program.react(&instance, river_input(passenger)).unwrap();
                assert_eq!(river_step_key(&repeated), actual, "refusal repeats");
                match actual.as_str() {
                    value if value.starts_with("P:") => passenger_refusals += 1,
                    "U:WG" => unsafe_wg += 1,
                    "U:GC" => unsafe_gc += 1,
                    "U:WG+GC" => unsafe_both += 1,
                    other => panic!("unknown oracle result `{other}`"),
                }
            }
        }
    }
    assert_eq!(accepted, 20);
    assert_eq!(passenger_refusals, 10);
    assert_eq!((unsafe_wg, unsafe_gc, unsafe_both), (4, 4, 2));

    let canonical = [
        Some("goat"),
        None,
        Some("wolf"),
        Some("goat"),
        Some("cabbage"),
        None,
        Some("goat"),
    ];
    let expected = [
        "A:1010:goat:01",
        "A:0010:none:10",
        "A:1110:wolf:01",
        "A:0100:goat:10",
        "A:1101:cabbage:01",
        "A:0101:none:10",
        "A:1111:goat:01",
    ];
    let run = |id: &str| {
        let (mut instance, genesis) = program.admit(RIVER, Value::Unit, id).unwrap();
        let mut receipts = Vec::new();
        let mut observations = vec![genesis.initial_observation];
        for (index, passenger) in canonical.into_iter().enumerate() {
            let step = program.react(&instance, river_input(passenger)).unwrap();
            assert_eq!(river_step_key(&step), expected[index]);
            receipts.push(step.receipt.to_canonical_string());
            instance = step.instance;
            observations.push(instance.observation.clone());
        }
        (instance, receipts, observations)
    };
    let (solved, receipts, observations) = run("l1/replay");
    assert_eq!(river_bits(&solved), "1111");
    assert_eq!(
        constructor(record_field(&solved.observation, "status")),
        "solved"
    );
    let (replayed, replay_receipts, replay_observations) = run("l1/replay");
    assert_eq!(replayed.state, solved.state);
    assert_eq!(replay_receipts, receipts);
    assert_eq!(replay_observations, observations);

    let leaves_goal = program.react(&solved, river_input(Some("goat"))).unwrap();
    assert_eq!(river_bits(&leaves_goal.instance), "0101");
    assert_eq!(
        constructor(record_field(&leaves_goal.instance.observation, "status")),
        "in_progress"
    );
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

const SUPERVISOR_CLASSIFICATIONS: [&str; 26] = [
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

const SUPERVISOR_COMMANDS: [&[&str]; 26] = [
    &["start:A:1"],
    &["start:B:1"],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &["cancel:B:1", "start:D:1"],
    &[],
    &["start:C:1"],
    &[],
    &[],
    &[],
    &["start:A:2"],
    &[],
    &[],
    &[],
    &[],
    &["start:A:3"],
    &["cancel:A:3"],
    &[],
    &[],
    &[],
    &[],
    &[],
];

fn task_id(name: &str) -> Value {
    Value::Key {
        type_id: local_type(SUPERVISOR, "TaskId"),
        value: Box::new(Value::Text(name.into())),
    }
}

fn supervisor_input(value: SupervisorInput<'_>) -> Value {
    let task = |name: &str| (Some("task".into()), task_id(name));
    let attempt = |value| (Some("attempt".into()), Value::int(value));
    match value {
        SupervisorInput::Submit(name) => input(SUPERVISOR, "submit", vec![task(name)]),
        SupervisorInput::Cancel(name) => input(SUPERVISOR, "cancel", vec![task(name)]),
        SupervisorInput::Retry(name) => input(SUPERVISOR, "retry", vec![task(name)]),
        SupervisorInput::Progress(name, serial, value) => input(
            SUPERVISOR,
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
        SupervisorInput::Succeed(name, serial) => {
            input(SUPERVISOR, "succeed", vec![task(name), attempt(serial)])
        }
        SupervisorInput::Fail(name, serial) => {
            input(SUPERVISOR, "fail", vec![task(name), attempt(serial)])
        }
    }
}

fn command_key(command: &Value) -> String {
    let name = constructor(command);
    let task = variant_field(command, 0);
    let Value::Key { value: key, .. } = task else {
        panic!("command task is not a key")
    };
    let Value::Text(task) = key.as_ref() else {
        panic!("task key is not text")
    };
    let attempt = variant_field(command, 1);
    let attempt = match attempt {
        Value::Integer { value, .. } => value.to_string(),
        other => panic!("attempt is not an integer: {other:?}"),
    };
    format!("{name}:{task}:{attempt}")
}

fn terminal_task(phase: &str, started: i64) -> Value {
    Value::record([
        (
            "phase".into(),
            Value::variant(local_type(SUPERVISOR, "Phase"), phase, Vec::new()),
        ),
        ("started".into(), Value::nat(started).unwrap()),
    ])
    .unwrap()
}

fn final_supervisor_tasks() -> Value {
    Value::map([
        (task_id("A"), terminal_task("cancelled", 3)),
        (task_id("B"), terminal_task("cancelled", 1)),
        (task_id("C"), terminal_task("succeeded", 1)),
        (task_id("D"), terminal_task("succeeded", 1)),
    ])
    .unwrap()
}

fn run_supervisor_trace(program: &Program, id: &str) -> (Instance, Vec<String>) {
    let (mut instance, genesis) = program.admit(SUPERVISOR, Value::Unit, id).unwrap();
    assert_eq!(
        record_field(&genesis.initial_observation, "tasks"),
        &Value::Map(Vec::new())
    );
    assert_eq!(
        record_field(&genesis.initial_observation, "queue"),
        &Value::Seq(Vec::new())
    );
    let mut receipts = Vec::new();
    let mut midpoint = None;
    for (index, source_input) in SUPERVISOR_TRACE.into_iter().enumerate() {
        let previous = instance.state.clone();
        let step = program
            .react(&instance, supervisor_input(source_input))
            .unwrap();
        assert_eq!(
            constructor(completed_outcome(&step.receipt)),
            SUPERVISOR_CLASSIFICATIONS[index],
            "classification at step {}",
            index + 1
        );
        assert_eq!(
            step.receipt
                .ordered_commands
                .iter()
                .map(command_key)
                .collect::<Vec<_>>(),
            SUPERVISOR_COMMANDS[index],
            "ordered commands at step {}",
            index + 1
        );
        if SUPERVISOR_CLASSIFICATIONS[index] != "accepted" {
            assert_eq!(step.instance.state, previous, "ignored input stutters");
        }
        receipts.push(step.receipt.to_canonical_string());
        instance = step.instance;
        if index == 12 {
            midpoint = Some(program.checkpoint(&instance));
        }
    }

    let checkpoint = midpoint.expect("midpoint checkpoint");
    let mut restored = program.restore(&checkpoint).unwrap();
    for (index, source_input) in SUPERVISOR_TRACE.into_iter().enumerate().skip(13) {
        let step = program
            .react(&restored, supervisor_input(source_input))
            .unwrap();
        assert_eq!(step.receipt.to_canonical_string(), receipts[index]);
        restored = step.instance;
    }
    assert_eq!(restored.state, instance.state, "checkpoint suffix replay");
    (instance, receipts)
}

#[test]
fn l2_supervisor_matches_complete_twenty_six_step_trace_and_replay() {
    let program = checked_programs();
    let (final_instance, receipts) = run_supervisor_trace(&program, "l2/replay");
    assert_eq!(
        final_instance.state,
        Value::record([
            ("tasks".into(), final_supervisor_tasks()),
            ("queue".into(), Value::Seq(Vec::new())),
        ])
        .unwrap()
    );
    assert_eq!(
        record_field(&final_instance.observation, "tasks"),
        &final_supervisor_tasks()
    );
    assert_eq!(
        record_field(&final_instance.observation, "queue"),
        &Value::Seq(Vec::new())
    );
    assert_eq!(
        record_field(&final_instance.observation, "running"),
        &Value::Set(Vec::new())
    );
    assert_eq!(
        record_field(&final_instance.observation, "available_capacity"),
        &Value::nat(2).unwrap()
    );

    let (replayed, replay_receipts) = run_supervisor_trace(&program, "l2/replay");
    assert_eq!(replayed.state, final_instance.state);
    assert_eq!(replayed.observation, final_instance.observation);
    assert_eq!(replay_receipts, receipts);
}
