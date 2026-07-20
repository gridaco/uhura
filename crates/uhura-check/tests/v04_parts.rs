use uhura_check::{build_v04_provenance, check_v04_project_modules};
use uhura_core::{OutcomePolicy, ReactionResolution, Statement, Value, semantic_node_id};
use uhura_syntax::v04::{SourceIdentity, parse};

fn module(file: u32, logical: &str, source: &str) -> uhura_syntax::v04::ast::Module {
    let parsed = parse(
        SourceIdentity::new(file, "example.parts@1", logical, format!("{logical}.uhura")),
        source,
    );
    assert!(
        parsed.is_ok(),
        "parse diagnostics for {logical}:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn checked(sources: &[uhura_syntax::v04::ast::Module]) -> uhura_core::Program {
    let output = check_v04_project_modules(sources);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    output.program.expect("checked part program")
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    let Value::Record(fields) = value else {
        panic!("expected record, found {value:?}");
    };
    fields
        .iter()
        .find_map(|(field, value)| (field == name).then_some(value))
        .unwrap_or_else(|| panic!("record has no field `{name}`: {value:?}"))
}

fn outcome(receipt: &uhura_core::ReactionReceipt) -> (&str, OutcomePolicy) {
    let ReactionResolution::Completed { outcome, policy } = &receipt.resolution else {
        panic!("reaction faulted: {:?}", receipt.resolution);
    };
    let Value::Variant { constructor, .. } = outcome else {
        panic!("outcome is not a variant: {outcome:?}");
    };
    (constructor, *policy)
}

fn unreachable_site(statements: &[Statement]) -> Option<&str> {
    for statement in statements {
        match statement {
            Statement::Unreachable { source } => return Some(&source.id),
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                if let Some(site) =
                    unreachable_site(then_body).or_else(|| unreachable_site(else_body))
                {
                    return Some(site);
                }
            }
            Statement::Match { arms, .. } => {
                if let Some(site) = arms.iter().find_map(|arm| unreachable_site(&arm.body)) {
                    return Some(site);
                }
            }
            Statement::While { body, .. } => {
                if let Some(site) = unreachable_site(body) {
                    return Some(site);
                }
            }
            Statement::Let { .. }
            | Statement::Set { .. }
            | Statement::Emit { .. }
            | Statement::Finish { .. }
            | Statement::Delegate { .. } => {}
        }
    }
    None
}

const PARTS: &str = r#"
pub part Counter(step: Int) {
  requires outcomes {
    commit Done,
  }

  events {
    Tick,
  }

  commands {
    Changed(value: Int),
  }

  state {
    count: Int = 0,
  }

  pub computed current: Int = count;

  observe {
    count,
  }

  pub update reset() {
    count = 0;
  }

  on Tick {
    count = count + step;
    emit Changed(count);
    Done
  }
}

pub part Mirror(
  counter: Counter::Reads,
  counter_updates: Counter::Updates,
) {
  requires outcomes {
    commit Done,
  }

  events {
    Capture,
    ResetCounter,
  }

  state {
    seen: Int = 0,
  }

  observe {
    seen,
  }

  on Capture {
    seen = counter.current;
    Done
  }

  on ResetCounter {
    if counter.current > 0 {
      counter_updates.reset();
    }
    Done
  }
}
"#;

const APP: &str = r#"
use crate::parts::{Counter, Mirror};

pub machine Composed {
  config {
    step: Int,
  }

  outcomes {
    commit Done,
  }

  state {}
  observe {}

  part mirror = Mirror(counter.reads, counter.updates);
  part counter = Counter(step);
}

pub machine Flat {
  config {
    step: Int,
  }

  events {
    Tick,
    Capture,
    ResetCounter,
  }

  commands {
    Changed(value: Int),
  }

  outcomes {
    commit Done,
  }

  state {
    count: Int = 0,
    seen: Int = 0,
  }

  observe {
    count,
    seen,
  }

  on Tick {
    count = count + step;
    emit Changed(count);
    Done
  }

  on Capture {
    seen = count;
    Done
  }

  on ResetCounter {
    count = 0;
    Done
  }
}
"#;

#[test]
fn root_observation_and_handlers_use_composed_read_and_update_interfaces() {
    let source = r#"
pub part Counter {
  state { count: Int = 1 }
  pub computed current: Int = count;
  observe {}
  pub update increment() { count = count + 1; }
}

pub machine App {
  events { Run }
  outcomes { commit Done }
  state {}
  part counter = Counter();
  observe { count: counter.reads.current }
  on Run {
    counter.updates.increment();
    Done
  }
}
"#;
    let program = checked(&[module(1, "root_interfaces", source)]);
    let machine_id = "example.parts@1::App";
    let (instance, _) = program
        .admit(machine_id, Value::Unit, "parts/root-interfaces")
        .expect("admission");
    assert_eq!(
        field(&instance.observation, "count"),
        &Value::int(1),
        "root observation must read the part's current draft projection"
    );

    let input = Value::variant(format!("{machine_id}.Input"), "Run", Vec::new());
    let reacted = program.react(&instance, input).expect("root update call");
    assert_eq!(
        field(&reacted.instance.observation, "count"),
        &Value::int(2),
        "root update call must mutate the composed owner in the same transaction"
    );
}

#[test]
fn direct_parts_flatten_into_one_kernel_with_namespaced_domains() {
    let program = checked(&[module(1, "parts", PARTS), module(2, "app", APP)]);
    let machine_id = "example.parts@1::Composed";
    let machine = &program.machines[machine_id];
    assert_eq!(
        machine
            .state
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["counter.count", "mirror.seen"]
    );
    assert_eq!(
        machine
            .observation
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["counter.count", "mirror.seen"]
    );
    let uhura_core::TypeDef::Sum { constructors, .. } = &machine.local_input else {
        panic!("composed input is not a sum");
    };
    assert_eq!(
        constructors
            .iter()
            .map(|value| value.name.as_str())
            .collect::<Vec<_>>(),
        ["counter.Tick", "mirror.Capture", "mirror.ResetCounter"]
    );
    assert_eq!(
        machine
            .local_commands
            .iter()
            .map(|value| value.constructor.name.as_str())
            .collect::<Vec<_>>(),
        ["counter.Changed"]
    );

    let (instance, _) = program
        .admit(
            machine_id,
            Value::Record(vec![("step".into(), Value::int(2))]),
            "parts/composed",
        )
        .expect("composed admission");
    let tick = Value::variant(format!("{machine_id}.Input"), "counter.Tick", Vec::new());
    let ticked = program.react(&instance, tick).expect("part event");
    assert_eq!(outcome(&ticked.receipt), ("Done", OutcomePolicy::Commit));
    assert_eq!(
        field(&ticked.instance.observation, "counter.count"),
        &Value::int(2)
    );
    assert_eq!(ticked.receipt.ordered_commands.len(), 1);
    let Value::Variant { constructor, .. } = &ticked.receipt.ordered_commands[0] else {
        panic!("part command is not a variant");
    };
    assert_eq!(constructor, "counter.Changed");

    let capture = Value::variant(format!("{machine_id}.Input"), "mirror.Capture", Vec::new());
    let captured = program
        .react(&ticked.instance, capture)
        .expect("read handle");
    assert_eq!(
        field(&captured.instance.observation, "mirror.seen"),
        &Value::int(2)
    );

    let reset = Value::variant(
        format!("{machine_id}.Input"),
        "mirror.ResetCounter",
        Vec::new(),
    );
    let reset = program
        .react(&captured.instance, reset)
        .expect("update handle");
    assert_eq!(
        field(&reset.instance.observation, "counter.count"),
        &Value::int(0)
    );
}

#[test]
fn flat_and_part_forms_have_equivalent_mapped_reaction_traces() {
    let program = checked(&[module(1, "parts", PARTS), module(2, "app", APP)]);
    let composed_id = "example.parts@1::Composed";
    let flat_id = "example.parts@1::Flat";
    let configuration = Value::Record(vec![("step".into(), Value::int(2))]);
    let (mut composed, _) = program
        .admit(
            composed_id,
            configuration.clone(),
            "parts/equivalence/composed",
        )
        .expect("composed admission");
    let (mut flat, _) = program
        .admit(flat_id, configuration, "parts/equivalence/flat")
        .expect("flat admission");

    for (composed_case, flat_case) in [
        ("counter.Tick", "Tick"),
        ("mirror.Capture", "Capture"),
        ("mirror.ResetCounter", "ResetCounter"),
    ] {
        let composed_result = program
            .react(
                &composed,
                Value::variant(format!("{composed_id}.Input"), composed_case, Vec::new()),
            )
            .expect("composed reaction");
        let flat_result = program
            .react(
                &flat,
                Value::variant(format!("{flat_id}.Input"), flat_case, Vec::new()),
            )
            .expect("flat reaction");
        assert_eq!(
            outcome(&composed_result.receipt),
            outcome(&flat_result.receipt)
        );
        assert_eq!(
            field(&composed_result.instance.observation, "counter.count"),
            field(&flat_result.instance.observation, "count")
        );
        assert_eq!(
            field(&composed_result.instance.observation, "mirror.seen"),
            field(&flat_result.instance.observation, "seen")
        );
        assert_eq!(
            composed_result.receipt.ordered_commands.len(),
            flat_result.receipt.ordered_commands.len()
        );
        for (composed_command, flat_command) in composed_result
            .receipt
            .ordered_commands
            .iter()
            .zip(&flat_result.receipt.ordered_commands)
        {
            let (
                Value::Variant {
                    constructor: composed_constructor,
                    fields: composed_fields,
                    ..
                },
                Value::Variant {
                    constructor: flat_constructor,
                    fields: flat_fields,
                    ..
                },
            ) = (composed_command, flat_command)
            else {
                panic!("mapped commands must be variants");
            };
            assert_eq!(
                (composed_constructor.as_str(), flat_constructor.as_str()),
                ("counter.Changed", "Changed")
            );
            assert_eq!(composed_fields, flat_fields);
        }
        composed = composed_result.instance;
        flat = flat_result.instance;
    }
}

#[test]
fn part_instance_order_is_nonsemantic_and_owner_rename_is_semantic() {
    let reordered = APP.replace(
        "part mirror = Mirror(counter.reads, counter.updates);\n  part counter = Counter(step);",
        "part counter = Counter(step);\n  part mirror = Mirror(counter.reads, counter.updates);",
    );
    let normal = checked(&[module(1, "parts", PARTS), module(2, "app", APP)]);
    let reordered = checked(&[
        module(20, "renamed::parts", PARTS),
        module(
            10,
            "renamed::app",
            &reordered.replace("crate::parts", "crate::renamed::parts"),
        ),
    ]);
    assert_eq!(normal.program_hashes, reordered.program_hashes);

    let renamed_owner = APP
        .replace(
            "part counter = Counter(step);",
            "part tally = Counter(step);",
        )
        .replace("counter.reads", "tally.reads")
        .replace("counter.updates", "tally.updates");
    let renamed = checked(&[module(1, "parts", PARTS), module(2, "app", &renamed_owner)]);
    assert_ne!(normal.program_hashes, renamed.program_hashes);
}

#[test]
fn inlined_part_update_fault_keeps_the_called_instance_owner() {
    let source = r#"
pub part Guard {
  state { remaining: Seq<Int> = [] }
  observe {}

  pub update fail() {
    while remaining.uncons() is Some(Uncons { head: _, tail })
    decreases(remaining.len()) {
      unreachable;
      remaining = tail;
    }
  }
}

pub machine App {
  events { Run }
  outcomes { commit Done }
  state {}
  observe {}

  part left = Guard();
  part right = Guard();

  on Run {
    right.updates.fail();
    Done
  }
}
"#;
    let program = checked(&[module(1, "part_update_fault", source)]);
    let machine_id = "example.parts@1::App";
    let site = unreachable_site(&program.machines[machine_id].handlers["Run"].body)
        .expect("the called update retains its unreachable site");
    assert_eq!(
        site,
        semantic_node_id(
            machine_id,
            "right",
            "unreachable",
            "update/fail/statement/0/body/statement/0",
        ),
    );
    assert_ne!(
        site,
        semantic_node_id(
            machine_id,
            "left",
            "unreachable",
            "update/fail/statement/0/body/statement/0",
        ),
    );
    uhura_core::Program::from_json(&program.to_canonical_string())
        .expect("composed site identity frames survive public IR admission");
}

#[test]
fn fault_site_identity_ignores_nonsemantic_pattern_syntax() {
    let source = r#"
pub struct Pair {
  left: Int,
  right: Int,
}

pub machine App {
  events { Run(value: Pair) }
  outcomes { commit Done }
  state {}
  observe {}

  on Run(value) {
    match value {
      Pair { left, right } => {
        unreachable;
      },
    };
    Done
  }
}
"#;
    let reordered = source.replace("Pair { left, right }", "Pair { right, left }");
    let direct = checked(&[module(1, "pattern_direct", source)]);
    let reordered = checked(&[module(2, "pattern_reordered", &reordered)]);
    let machine_id = "example.parts@1::App";
    let direct = unreachable_site(&direct.machines[machine_id].handlers["Run"].body)
        .expect("direct pattern fault site");
    let reordered = unreachable_site(&reordered.machines[machine_id].handlers["Run"].body)
        .expect("reordered pattern fault site");
    assert_eq!(
        direct, reordered,
        "record field order is syntax, not fault-site identity material",
    );

    let text_pattern = r#"
pub machine App {
  events { Run(value: Text) }
  outcomes { commit Done }
  state {}
  observe {}

  on Run(value) {
    match value {
      "A" => {
        unreachable;
      },
      _ => {},
    };
    Done
  }
}
"#;
    let escaped_pattern = text_pattern.replace(r#""A""#, r#""\u0041""#);
    let direct = checked(&[module(3, "text_pattern_direct", text_pattern)]);
    let escaped = checked(&[module(4, "text_pattern_escaped", &escaped_pattern)]);
    let direct = unreachable_site(&direct.machines[machine_id].handlers["Run"].body)
        .expect("direct text-pattern fault site");
    let escaped = unreachable_site(&escaped.machines[machine_id].handlers["Run"].body)
        .expect("escaped text-pattern fault site");
    assert_eq!(
        direct, escaped,
        "literal source spelling is not fault-site identity material",
    );
}

#[test]
fn private_computed_types_are_inferred_across_part_local_dependencies() {
    let source = r#"
pub part Counter {
  state {
    count: Int = 2,
  }

  computed doubled = count + count;
  computed quadrupled = doubled + doubled;

  observe {
    doubled,
    quadrupled,
  }
}

pub machine App {
  outcomes {
    commit Done,
  }

  state {}
  observe {}
  part counter = Counter();
}
"#;
    let program = checked(&[module(1, "computed", source)]);
    let machine_id = "example.parts@1::App";
    let machine = &program.machines[machine_id];
    assert_eq!(
        machine
            .derives
            .iter()
            .map(|(name, ty, _, _)| (name.as_str(), ty.canonical_name()))
            .collect::<Vec<_>>(),
        [
            ("counter.doubled", "Int".into()),
            ("counter.quadrupled", "Int".into())
        ]
    );
    let (instance, _) = program
        .admit(machine_id, Value::Unit, "parts/computed")
        .expect("computed admission");
    assert_eq!(
        field(&instance.observation, "counter.doubled"),
        &Value::int(4)
    );
    assert_eq!(
        field(&instance.observation, "counter.quadrupled"),
        &Value::int(8)
    );
}

#[test]
fn part_owned_ports_lower_to_owner_qualified_kernel_protocols() {
    let source = r#"
pub part Worker {
  requires outcomes {
    commit Done,
  }

  port requests = RequestPort<Int, Int, Int> {};

  events {
    Submit(id: Int, payload: Int),
  }

  state {
    settlement: Int = 0,
  }

  observe {
    settlement,
  }

  on Submit(id, payload) {
    emit requests.Request(id, payload);
    Done
  }

  on requests.Settled(id, result) {
    settlement = result;
    Done
  }
}

pub machine App {
  outcomes {
    commit Done,
  }

  state {}
  observe {}
  part worker = Worker();
}
"#;
    let program = checked(&[module(1, "ports", source)]);
    let machine_id = "example.parts@1::App";
    let machine = &program.machines[machine_id];
    assert_eq!(
        machine
            .ports
            .iter()
            .map(|port| port.name.as_str())
            .collect::<Vec<_>>(),
        ["worker.requests"]
    );
    assert!(machine.handlers.contains_key("worker.Submit"));
    assert!(machine.handlers.contains_key("worker.requests.settled"));

    let (instance, _) = program
        .admit(machine_id, Value::Unit, "parts/ports")
        .expect("port admission");
    let submit = Value::variant(
        format!("{machine_id}.Input"),
        "worker.Submit",
        vec![
            (Some("id".into()), Value::int(7)),
            (Some("payload".into()), Value::int(11)),
        ],
    );
    let submitted = program.react(&instance, submit).expect("local part input");
    assert_eq!(submitted.receipt.ordered_commands.len(), 1);
    let Value::Variant {
        type_id,
        constructor,
        fields,
    } = &submitted.receipt.ordered_commands[0]
    else {
        panic!("port command is not a variant");
    };
    assert_eq!(type_id, &format!("{machine_id}::port.worker.requests.Send"));
    assert_eq!(constructor, "worker.requests.request");
    assert_eq!(
        fields,
        &vec![
            (Some("id".into()), Value::int(7)),
            (Some("payload".into()), Value::int(11))
        ]
    );

    let settled = Value::variant(
        format!("{machine_id}::port.worker.requests.Receive"),
        "worker.requests.settled",
        vec![
            (Some("id".into()), Value::int(7)),
            (Some("result".into()), Value::int(23)),
        ],
    );
    let settled = program
        .react(&submitted.instance, settled)
        .expect("part-owned port input");
    assert_eq!(outcome(&settled.receipt), ("Done", OutcomePolicy::Commit));
    assert_eq!(
        field(&settled.instance.observation, "worker.settlement"),
        &Value::int(23)
    );
}

#[test]
fn root_and_part_port_order_is_canonical_and_source_order_invariant() {
    let source = r#"
pub part Worker {
  port zeta = SinkPort<Int> {};
  port alpha = SinkPort<Int> {};
  state {}
  observe {}
}

pub machine App {
  port zeta = SinkPort<Int> {};
  port alpha = SinkPort<Int> {};
  outcomes { commit Done }
  state {}
  observe {}
  part worker = Worker();
}
"#;
    let reordered = source.replace(
        "port zeta = SinkPort<Int> {};\n  port alpha = SinkPort<Int> {};",
        "port alpha = SinkPort<Int> {};\n  port zeta = SinkPort<Int> {};",
    );
    let direct = checked(&[module(1, "ports", source)]);
    let reordered = checked(&[module(2, "moved::ports", &reordered)]);
    assert_eq!(direct.program_hashes, reordered.program_hashes);
    assert_eq!(
        direct.machines["example.parts@1::App"]
            .ports
            .iter()
            .map(|port| port.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta", "worker.alpha", "worker.zeta"]
    );
}

fn diagnostic_rules(source: &str) -> Vec<String> {
    let output = check_v04_project_modules(&[module(1, "invalid", source)]);
    assert!(output.program.is_none());
    output
        .diagnostics
        .into_iter()
        .map(|value| value.rule.to_owned())
        .collect()
}

#[test]
fn rejects_invalid_capabilities_outcomes_and_non_config_bindings() {
    let cycle = r#"
pub part A(other: B::Reads) {
  pub computed value: Int = other.value;
  state {}
  observe {}
}

pub part B(other: A::Reads) {
  pub computed value: Int = other.value;
  state {}
  observe {}
}
pub machine Invalid {
  outcomes { commit Done }
  state {}
  observe {}
  part a = A(b.reads);
  part b = B(a.reads);
}
"#;
    assert!(
        diagnostic_rules(cycle)
            .iter()
            .any(|rule| rule == "uhura-0.4/part-dependency-cycle")
    );

    let wrong_outcome = r#"
pub part Child {
  requires outcomes { abort Done }
  events { Run }
  state {}
  observe {}
  on Run { Done }
}
pub machine Invalid {
  outcomes { commit Done }
  state {}
  observe {}
  part child = Child();
}
"#;
    assert!(
        diagnostic_rules(wrong_outcome)
            .iter()
            .any(|rule| rule == "uhura-0.4/outcome-requirement-mismatch")
    );

    let state_argument = r#"
pub part Child(value: Int) {
  state { copy: Int = value }
  observe { copy }
}
pub machine Invalid {
  outcomes { commit Done }
  state { secret: Int = 1 }
  observe {}
  part child = Child(secret);
}
"#;
    assert!(
        diagnostic_rules(state_argument)
            .iter()
            .any(|rule| rule == "uhura-0.4/non-config-part-binding")
    );

    let private_interfaces = r#"
pub part Child {
  state { value: Int = 0 }
  computed hidden: Int = value;
  update secret() { value = 1; }
  observe {}
}
pub part Reader(
  child: Child::Reads,
  child_updates: Child::Updates,
) {
  requires outcomes { commit Done }
  events { Run }
  state { copy: Int = 0 }
  observe {}
  on Run {
    copy = child.hidden;
    child_updates.secret();
    Done
  }
}
pub machine Invalid {
  outcomes { commit Done }
  state {}
  observe {}
  part child = Child();
  part reader = Reader(child.reads, child.updates);
}
"#;
    let private_rules = diagnostic_rules(private_interfaces);
    assert!(
        private_rules
            .iter()
            .any(|rule| rule == "uhura-0.4/undeclared-part-read")
    );
    assert!(
        private_rules
            .iter()
            .any(|rule| rule == "uhura-0.4/undeclared-part-update")
    );

    let public_outcome_update = r#"
pub part Child {
  requires outcomes { commit Done }
  state {}
  observe {}
  pub update invalid() -> Outcome { Done }
}
pub machine Invalid {
  outcomes { commit Done }
  state {}
  observe {}
  part child = Child();
}
"#;
    assert!(
        diagnostic_rules(public_outcome_update)
            .iter()
            .any(|rule| rule == "uhura-0.4/public-update-outcome")
    );

    let owner_collision = r#"
pub part Child {
  state {}
  observe {}
}
pub machine Invalid {
  port child = WorkerPool {};
  outcomes { commit Done }
  state {}
  observe {}
  part child = Child();
}
"#;
    assert!(
        diagnostic_rules(owner_collision)
            .iter()
            .any(|rule| rule == "uhura-0.4/part-port-owner-collision")
    );

    let wrong_argument_type = r#"
pub part Child(value: Int) {
  state {}
  observe {}
}
pub machine Invalid {
  config { label: Text }
  outcomes { commit Done }
  state {}
  observe {}
  part child = Child(label);
}
"#;
    assert!(
        diagnostic_rules(wrong_argument_type)
            .iter()
            .any(|rule| rule == "uhura/type-mismatch")
    );

    let public_inferred_read = r#"
pub part Child {
  state { value: Int = 1 }
  pub computed exposed = value;
  observe {}
}
pub machine Invalid {
  outcomes { commit Done }
  state {}
  observe {}
  part child = Child();
}
"#;
    assert!(
        diagnostic_rules(public_inferred_read)
            .iter()
            .any(|rule| rule == "uhura-0.4/public-computed-type-required")
    );

    let inferred_cycle = r#"
pub machine Invalid {
  outcomes { commit Done }
  state {}
  computed left = right;
  computed right = left;
  observe {}
}
"#;
    assert!(
        diagnostic_rules(inferred_cycle)
            .iter()
            .any(|rule| rule == "uhura/recursive-derive-inference")
    );

    let duplicate_port = r#"
pub machine Invalid {
  port sink = SinkPort<Int> {};
  port sink = SinkPort<Int> {};
  outcomes { commit Done }
  state {}
  observe {}
}
"#;
    assert!(
        diagnostic_rules(duplicate_port)
            .iter()
            .any(|rule| rule == "uhura-0.4/duplicate-port")
    );

    let wrong_port_direction = r#"
pub machine Invalid {
  port sink = SinkPort<Int> {};
  outcomes { commit Done }
  state {}
  observe {}
  on sink.Send(value) { Done }
}
"#;
    assert!(
        diagnostic_rules(wrong_port_direction)
            .iter()
            .any(|rule| rule == "uhura/extra-handler")
    );

    let invalid_port_configuration = r#"
pub machine Invalid {
  port sink = SinkPort<Int> { routes: 1 };
  outcomes { commit Done }
  state {}
  observe {}
}
"#;
    assert!(
        diagnostic_rules(invalid_port_configuration)
            .iter()
            .any(|rule| rule == "uhura-0.4/port-configuration")
    );
}

#[test]
fn value_and_unit_updates_return_to_their_callers() {
    let source = r#"
pub enum Change { Unchanged, Changed { value: Int } }

pub part Counter {
  state { count: Int = 5 }
  pub computed current: Int = count;
  observe { count }
  update classify(delta: Int) -> Change {
    let accepted = match delta {
      0 => return Change::Unchanged,
      _ => delta,
    };
    count = count + accepted;
    Change::Changed { value: count }
  }
  pub update bump(delta: Int) -> Change { classify(delta) }
  pub update clear_if(flag: Bool) {
    if flag { count = 0; return; }
    count = count + 100;
  }
}

pub part Driver(counter: Counter::Reads, updates: Counter::Updates) {
  requires outcomes { commit Done }
  events { Run, Clear }
  state { seen: Int = -2, continued: Int = 0 }
  observe { seen, continued }
  on Run {
    let change = updates.bump(2);
    seen = match change {
      Change::Unchanged => -1,
      Change::Changed { value } => value,
    };
    continued = continued + 1;
    Done
  }
  on Clear {
    updates.clear_if(true);
    seen = counter.current;
    continued = continued + 1;
    Done
  }
}

pub machine App {
  events { Local }
  outcomes { commit Done }
  state { local: Int = 0, local_continued: Int = 0 }
  observe { local, local_continued }
  update stop_early(flag: Bool) {
    local = 1;
    if flag { return; }
    local = 2;
  }
  on Local {
    stop_early(true);
    local_continued = 9;
    Done
  }
  part counter = Counter();
  part driver = Driver(counter.reads, counter.updates);
}
"#;
    let program = checked(&[module(1, "updates", source)]);
    let machine_id = "example.parts@1::App";
    let (mut instance, _) = program
        .admit(machine_id, Value::Unit, "parts/update-results")
        .expect("admission");
    for (event, expected_count, expected_seen, expected_continued) in
        [("driver.Run", 7, 7, 1), ("driver.Clear", 0, 0, 2)]
    {
        let input = Value::variant(format!("{machine_id}.Input"), event, Vec::new());
        let result = program.react(&instance, input).expect("part update call");
        assert_eq!(
            field(&result.instance.observation, "counter.count"),
            &Value::int(expected_count)
        );
        assert_eq!(
            field(&result.instance.observation, "driver.seen"),
            &Value::int(expected_seen)
        );
        assert_eq!(
            field(&result.instance.observation, "driver.continued"),
            &Value::int(expected_continued)
        );
        instance = result.instance;
    }
    let input = Value::variant(format!("{machine_id}.Input"), "Local", Vec::new());
    let result = program
        .react(&instance, input)
        .expect("machine update call");
    assert_eq!(field(&result.instance.observation, "local"), &Value::int(1));
    assert_eq!(
        field(&result.instance.observation, "local_continued"),
        &Value::int(9)
    );
}

#[test]
fn outcome_updates_remain_terminal_and_invalid_value_calls_are_rejected() {
    let outcome_source = r#"
pub part Worker {
  requires outcomes { commit Done, abort Refused }
  events { Finish(flag: Bool) }
  state {}
  observe {}
  update settle(flag: Bool) -> Outcome {
    if flag { return Done; }
    Refused
  }
  on Finish(flag) { settle(flag) }
}
pub machine App {
  outcomes { commit Done, abort Refused }
  state {}
  observe {}
  part worker = Worker();
}
"#;
    let program = checked(&[module(1, "outcome_updates", outcome_source)]);
    let machine_id = "example.parts@1::App";
    let (instance, _) = program
        .admit(machine_id, Value::Unit, "parts/outcome-update")
        .expect("admission");
    for (flag, expected) in [
        (true, ("Done", OutcomePolicy::Commit)),
        (false, ("Refused", OutcomePolicy::Abort)),
    ] {
        let input = Value::variant(
            format!("{machine_id}.Input"),
            "worker.Finish",
            vec![(Some("flag".into()), Value::Bool(flag))],
        );
        let result = program.react(&instance, input).expect("outcome update");
        assert_eq!(outcome(&result.receipt), expected);
    }

    let discarded = r#"
pub machine Invalid {
  events { Run }
  outcomes { commit Done }
  state { value: Int = 0 }
  observe {}
  update next() -> Int { 1 }
  on Run { next(); Done }
}
"#;
    assert!(
        diagnostic_rules(discarded)
            .iter()
            .any(|rule| { rule == "uhura-0.4/discarded-effectful-update" })
    );

    let nested = discarded.replace("next();", "value = next() + 1;");
    assert!(
        diagnostic_rules(&nested)
            .iter()
            .any(|rule| { rule == "uhura-0.4/nested-effectful-update" })
    );

    let recursive = r#"
pub machine Invalid {
  outcomes { commit Done }
  state {}
  observe {}
  update left() { right(); }
  update right() { left(); }
}
"#;
    assert!(
        diagnostic_rules(recursive)
            .iter()
            .any(|rule| rule == "uhura-0.4/update-cycle")
    );

    let wrong = r#"
pub machine Invalid {
  events { Run }
  outcomes { commit Done }
  state {}
  observe {}
  update wrong() { return 1; }
  on Run { wrong(); Done }
}
"#;
    assert!(
        diagnostic_rules(wrong)
            .iter()
            .any(|rule| rule == "uhura/type-mismatch")
    );
}

#[test]
fn part_fault_sites_keep_composition_identity_and_join_provenance() {
    let source = r#"
pub part Guard {
  requires outcomes {
    commit Done,
  }

  events {
    Crash,
  }

  state {
    valid: Bool = true,
  }

  invariant valid;

  on Crash {
    unreachable;
  }
}

pub machine App {
  outcomes {
    commit Done,
  }

  state {}
  observe {}

  part right = Guard();
  part left = Guard();
}
"#;
    let original = module(71, "fault_sites", source);
    let program = checked(std::slice::from_ref(&original));
    let machine_id = "example.parts@1::App";
    let machine = &program.machines[machine_id];

    let left_invariant = semantic_node_id(machine_id, "left", "invariant", "invariant/0");
    let right_invariant = semantic_node_id(machine_id, "right", "invariant", "invariant/0");
    assert_eq!(machine.invariants[0].1.id, left_invariant);
    assert_eq!(machine.invariants[1].1.id, right_invariant);

    let left_unreachable = semantic_node_id(
        machine_id,
        "left",
        "unreachable",
        "handler/Crash/statement/0",
    );
    let right_unreachable = semantic_node_id(
        machine_id,
        "right",
        "unreachable",
        "handler/Crash/statement/0",
    );
    assert_eq!(
        unreachable_site(&machine.handlers["left.Crash"].body),
        Some(left_unreachable.as_str())
    );
    assert_eq!(
        unreachable_site(&machine.handlers["right.Crash"].body),
        Some(right_unreachable.as_str())
    );
    assert_ne!(left_invariant, right_invariant);
    assert_ne!(left_unreachable, right_unreachable);

    let provenance = build_v04_provenance(std::slice::from_ref(&original)).unwrap();
    for (node, owner) in [
        (&left_invariant, "left"),
        (&right_invariant, "right"),
        (&left_unreachable, "left"),
        (&right_unreachable, "right"),
    ] {
        assert!(
            provenance.occurrences.iter().any(|occurrence| {
                occurrence.node == *node
                    && occurrence.owner == owner
                    && occurrence.role == "generated"
            }),
            "runtime SiteId {node} must join generated `{owner}` provenance"
        );
    }

    let moved = module(99, "renamed_logical_module", source);
    let moved_program = checked(std::slice::from_ref(&moved));
    let moved_machine = &moved_program.machines[machine_id];
    assert_eq!(
        program.program_hashes[machine_id],
        moved_program.program_hashes[machine_id]
    );
    assert_eq!(
        machine
            .invariants
            .iter()
            .map(|(_, source)| &source.id)
            .collect::<Vec<_>>(),
        moved_machine
            .invariants
            .iter()
            .map(|(_, source)| &source.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        unreachable_site(&machine.handlers["left.Crash"].body),
        unreachable_site(&moved_machine.handlers["left.Crash"].body)
    );
}
