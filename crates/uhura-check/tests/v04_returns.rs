use uhura_check::check_v04_project_modules;
use uhura_core::{ReactionResolution, Statement, Value};
use uhura_syntax::v04::{ParseDiagnosticKind, SourceIdentity, parse};

use std::fmt::Write as _;

fn module(source: &str) -> uhura_syntax::v04::ast::Module {
    let parsed = parse(
        SourceIdentity::new(1, "example.returns@1", "main", "main.uhura"),
        source,
    );
    assert!(
        parsed.is_ok(),
        "parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn checked(source: &str) -> uhura_core::Program {
    let output = check_v04_project_modules(&[module(source)]);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    output.program.expect("checked return program")
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

fn replace_first_loop_break(statements: &mut [Statement], replacement: &str) -> bool {
    for statement in statements {
        match statement {
            Statement::While {
                break_local, body, ..
            } => {
                if break_local.is_some() {
                    *break_local = Some(replacement.into());
                    return true;
                }
                if replace_first_loop_break(body, replacement) {
                    return true;
                }
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                if replace_first_loop_break(then_body, replacement)
                    || replace_first_loop_break(else_body, replacement)
                {
                    return true;
                }
            }
            Statement::Match { arms, .. } => {
                if arms
                    .iter_mut()
                    .any(|arm| replace_first_loop_break(&mut arm.body, replacement))
                {
                    return true;
                }
            }
            Statement::Let { .. }
            | Statement::Set { .. }
            | Statement::Emit { .. }
            | Statement::Finish { .. }
            | Statement::Unreachable { .. }
            | Statement::Delegate { .. } => {}
        }
    }
    false
}

fn checked_program_size(source: &str) -> usize {
    let parsed = module(source);
    let output = check_v04_project_modules(&[parsed]);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    serde_json::to_vec(&output.program.expect("checked stress program"))
        .expect("serialize checked stress program")
        .len()
}

fn pure_return_stress_source(count: usize) -> String {
    let mut source = String::from("pub fn stress(flag: Bool) -> Int {\n");
    for index in 0..count {
        writeln!(
            source,
            "  let value{index} = if (if flag {{ return 100; }} else {{ true }}) {{ {index} }} else {{ {index} }};"
        )
        .expect("write pure stress source");
    }
    source.push_str("  ");
    for index in 0..count {
        if index > 0 {
            source.push_str(" + ");
        }
        write!(source, "value{index}").expect("write pure stress sum");
    }
    source.push_str("\n}\n");
    source
}

fn update_return_stress_source(count: usize) -> String {
    let mut source = String::from(
        "pub machine App {\n  events { Run }\n  outcomes { commit Done }\n  state { total: Int = 0 }\n  observe { total }\n\n  update choose(flag: Bool, value: Int) -> Int {\n    if flag { return value; }\n    value + 1\n  }\n\n  on Run {\n",
    );
    for index in 0..count {
        writeln!(source, "    let value{index} = choose(false, {index});")
            .expect("write update stress source");
    }
    source.push_str("    total = ");
    for index in 0..count {
        if index > 0 {
            source.push_str(" + ");
        }
        write!(source, "value{index}").expect("write update stress sum");
    }
    source.push_str(";\n    Done\n  }\n}\n");
    source
}

#[test]
fn lexical_return_lowering_has_bounded_linear_program_growth() {
    let pure_small = checked_program_size(&pure_return_stress_source(4));
    let pure_large = checked_program_size(&pure_return_stress_source(8));
    assert!(
        pure_large < pure_small * 3,
        "pure CPS output grew superlinearly: 4={pure_small} bytes, 8={pure_large} bytes"
    );

    let update_small = checked_program_size(&update_return_stress_source(4));
    let update_large = checked_program_size(&update_return_stress_source(8));
    assert!(
        update_large < update_small * 3,
        "update CPS output grew superlinearly: 4={update_small} bytes, 8={update_large} bytes"
    );
}

#[test]
fn shared_pure_continuation_survives_canonical_program_roundtrip() {
    let program = checked(
        r#"
fn choose(flag: Bool) -> Int {
  let selected = if flag { return 7; } else { 1 };
  selected + 1
}

pub machine App {
  state {
    returned: Int = choose(true),
    continued: Int = choose(false),
  }
  observe { returned, continued }
}
"#,
    );
    let canonical = program.to_canonical_string();
    let roundtripped = uhura_core::Program::from_json(&canonical).expect("canonical IR roundtrip");
    let (instance, _) = roundtripped
        .machine_program
        .admit(
            "example.returns@1::App",
            Value::Unit,
            "returns/canonical-continuation",
        )
        .expect("roundtripped admission");
    assert_eq!(field(&instance.observation, "returned"), &Value::int(7));
    assert_eq!(field(&instance.observation, "continued"), &Value::int(2));
}

#[test]
fn compiler_local_ordinals_do_not_make_declaration_order_semantic() {
    let function_a = r#"
pub fn alpha(flag: Bool) -> Int {
  let value = if flag { return 11; } else { 1 };
  value + 1
}
"#;
    let function_b = r#"
pub fn beta(flag: Bool) -> Int {
  let value = if flag { return 13; } else { 2 };
  value + 1
}
"#;
    let machine_start = r#"
pub machine App {
  events { Alpha, Beta }
  outcomes { commit Done }
  state { total: Int = 0 }
  observe { total }

  update choose(flag: Bool, value: Int) -> Int {
    if flag { return value; }
    value + 1
  }
"#;
    let handler_a = r#"
  on Alpha {
    let selected = choose(true, 17);
    total = selected + alpha(false);
    Done
  }
"#;
    let handler_b = r#"
  on Beta {
    let selected = choose(false, 19);
    total = selected + beta(false);
    Done
  }
"#;
    let source = |functions: [&str; 2], handlers: [&str; 2]| {
        format!(
            "{}{}{}{}{}\n}}\n",
            functions[0], functions[1], machine_start, handlers[0], handlers[1]
        )
    };
    let left = checked(&source([function_a, function_b], [handler_a, handler_b]));
    let right = checked(&source([function_b, function_a], [handler_b, handler_a]));
    assert_eq!(
        left.machine_program.program_hashes["example.returns@1::App"],
        right.machine_program.program_hashes["example.returns@1::App"],
        "independent function and handler placement must not perturb MachineProgramId"
    );
}

#[test]
fn pure_return_is_lexical_through_let_if_match_block_and_nested_operands() {
    let source = r#"
fn choose(flag: Bool, value: Int) -> Int {
  let label: Text = if flag {
    return 7;
  } else {
    "kept"
  };
  let selected = match value {
    0 => return 11,
    _ => value,
  };
  {
    if selected == 1 {
      return 13;
    }
    let offset = 1 + (if selected == 2 { return 17; } else { 2 });
    selected + offset
  }
}

pub machine App {
  outcomes { commit Done }
  state {
    from_if: Int = choose(true, 9),
    from_match: Int = choose(false, 0),
    from_block: Int = choose(false, 1),
    from_operand: Int = choose(false, 2),
    from_tail: Int = choose(false, 3),
  }
  observe { from_if, from_match, from_block, from_operand, from_tail }
}
"#;
    let program = checked(source);
    let (instance, _) = program
        .machine_program
        .admit("example.returns@1::App", Value::Unit, "returns/pure")
        .expect("admission");
    assert_eq!(field(&instance.observation, "from_if"), &Value::int(7));
    assert_eq!(field(&instance.observation, "from_match"), &Value::int(11));
    assert_eq!(field(&instance.observation, "from_block"), &Value::int(13));
    assert_eq!(
        field(&instance.observation, "from_operand"),
        &Value::int(17)
    );
    assert_eq!(field(&instance.observation, "from_tail"), &Value::int(6));
}

#[test]
fn pure_return_is_lexical_in_calls_records_collections_conditions_and_subjects() {
    let source = r#"
pub struct Pair { left: Int, right: Int }
pub enum Slot { First, Second }

fn add(left: Int, right: Int) -> Int { left + right }

fn from_call(flag: Bool) -> Int {
  add(1, if flag { return 31; } else { 2 })
}

fn from_record(flag: Bool) -> Int {
  let pair = Pair {
    left: if flag { return 37; } else { 1 },
    right: 2,
  };
  pair.left + pair.right
}

fn from_record_base(flag: Bool) -> Int {
  let base = Pair { left: 1, right: 2 };
  let pair = Pair {
    left: 9,
    ..(if flag { return 41; } else { base })
  };
  pair.right
}

fn from_collection(flag: Bool) -> Int {
  let values = [1, if flag { return 43; } else { 2 }];
  0
}

fn from_index(flag: Bool) -> Int {
  let values: Table<Slot, Int> = Table::from([
    (Slot::First, 5),
    (Slot::Second, 6),
  ]);
  values[if flag { return 47; } else { Slot::Second }]
}

fn from_condition(flag: Bool) -> Int {
  if (if flag { return 53; } else { true }) { 5 } else { 6 }
}

fn from_subject(flag: Bool) -> Int {
  match (if flag { return 59; } else { 0 }) {
    0 => 7,
    _ => 8,
  }
}

fn short_circuit(flag: Bool) -> Bool {
  false && (if flag { return true; } else { true })
}

pub machine App {
  outcomes { commit Done }
  state {
    call: Int = from_call(true),
    record: Int = from_record(true),
    base: Int = from_record_base(true),
    collection: Int = from_collection(true),
    index: Int = from_index(true),
    condition: Int = from_condition(true),
    subject: Int = from_subject(true),
    short: Bool = short_circuit(true),
  }
  observe { call, record, base, collection, index, condition, subject, short }
}
"#;
    let program = checked(source);
    let (instance, _) = program
        .machine_program
        .admit("example.returns@1::App", Value::Unit, "returns/positions")
        .expect("admission");
    for (name, expected) in [
        ("call", 31),
        ("record", 37),
        ("base", 41),
        ("collection", 43),
        ("index", 47),
        ("condition", 53),
        ("subject", 59),
    ] {
        assert_eq!(field(&instance.observation, name), &Value::int(expected));
    }
    assert_eq!(field(&instance.observation, "short"), &Value::Bool(false));
}

#[test]
fn outcome_free_state_only_machine_uses_the_empty_outcome_identity() {
    let source = r#"
pub machine App {
  state { value: Int = 7 }
  observe { value }
}
"#;
    let program = checked(source);
    let (instance, _) = program
        .machine_program
        .admit("example.returns@1::App", Value::Unit, "returns/state-only")
        .expect("state-only admission");
    assert_eq!(field(&instance.observation, "value"), &Value::int(7));

    let invalid = check_v04_project_modules(&[module(
        r#"
pub machine Invalid {
  events { Run }
  state {}
  observe {}
}
"#,
    )]);
    assert!(invalid.program.is_none());
    assert!(invalid.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura-0.4/unsupported"
            && diagnostic.message.contains("may omit outcomes only")
    }));
}

#[test]
fn handler_return_finishes_only_the_selected_branch_and_preserves_the_tail() {
    let source = r#"
pub machine App {
  events { Run(duplicate: Bool) }
  outcomes { commit Done, abort Duplicate }
  state { continued: Int = 0 }
  observe { continued }
  on Run(duplicate) {
    if duplicate {
      return Duplicate;
    }
    continued = continued + 1;
    Done
  }
}
"#;
    let program = checked(source);
    let machine_id = "example.returns@1::App";
    let (instance, _) = program
        .machine_program
        .admit(machine_id, Value::Unit, "returns/handler")
        .expect("admission");

    let run = |duplicate| {
        Value::variant(
            format!("{machine_id}.Input"),
            "Run",
            vec![(Some("duplicate".into()), Value::Bool(duplicate))],
        )
    };
    let duplicate = program
        .machine_program
        .react(&instance, run(true))
        .expect("duplicate reaction");
    let ReactionResolution::Completed { outcome, .. } = &duplicate.receipt.resolution else {
        panic!("duplicate reaction faulted")
    };
    assert!(matches!(outcome, Value::Variant { constructor, .. } if constructor == "Duplicate"));
    assert_eq!(
        field(&duplicate.instance.observation, "continued"),
        &Value::int(0)
    );

    let accepted = program
        .machine_program
        .react(&instance, run(false))
        .expect("accepted reaction");
    let ReactionResolution::Completed { outcome, .. } = &accepted.receipt.resolution else {
        panic!("accepted reaction faulted")
    };
    assert!(matches!(outcome, Value::Variant { constructor, .. } if constructor == "Done"));
    assert_eq!(
        field(&accepted.instance.observation, "continued"),
        &Value::int(1)
    );
}

#[test]
fn update_return_is_lexical_in_values_payloads_arguments_and_control_operands() {
    let source = r#"
pub struct Pair { left: Int, right: Int }
pub enum Slot { First, Second }

pub machine App {
  events { Run }
  commands { Seen(value: Int) }
  outcomes { commit Done }
  state { touched: Int = 0, total: Int = 0, continued: Int = 0 }
  observe { touched, total, continued }

  update identity(value: Int) -> Int { value }

  update from_assignment(flag: Bool) -> Int {
    touched = if flag { return 11; } else { 99 };
    1
  }

  update from_payload(flag: Bool) -> Int {
    emit Seen(if flag { return 13; } else { 1 });
    1
  }

  update from_update_argument(flag: Bool) -> Int {
    identity(if flag { return 17; } else { 1 })
  }

  update from_record(flag: Bool) -> Int {
    let pair = Pair {
      left: if flag { return 19; } else { 1 },
      right: 2,
    };
    pair.left
  }

  update from_record_base(flag: Bool) -> Int {
    let base = Pair { left: 1, right: 2 };
    let pair = Pair {
      left: 9,
      ..(if flag { return 20; } else { base })
    };
    pair.right
  }

  update from_condition(flag: Bool) -> Int {
    if (if flag { return 23; } else { true }) { 1 } else { 2 }
  }

  update from_subject(flag: Bool) -> Int {
    match (if flag { return 29; } else { 0 }) {
      0 => 1,
      _ => 2,
    }
  }

  update from_collection(flag: Bool) -> Int {
    let values = [if flag { return 31; } else { 1 }];
    0
  }

  update from_index(flag: Bool) -> Int {
    let values: Table<Slot, Int> = Table::from([
      (Slot::First, 5),
      (Slot::Second, 6),
    ]);
    values[if flag { return 37; } else { Slot::Second }]
  }

  update from_short_circuit(flag: Bool) -> Int {
    if false && (if flag { return 41; } else { true }) { 1 } else { 43 }
  }

  on Run {
    let a = from_assignment(true);
    let b = from_payload(true);
    let c = from_update_argument(true);
    let d = from_record(true);
    let e = from_record_base(true);
    let f = from_condition(true);
    let g = from_subject(true);
    let h = from_collection(true);
    let i = from_index(true);
    let j = from_short_circuit(true);
    total = a + b + c + d + e + f + g + h + i + j;
    continued = 1;
    Done
  }
}
"#;
    let program = checked(source);
    let machine_id = "example.returns@1::App";
    let (instance, _) = program
        .machine_program
        .admit(machine_id, Value::Unit, "returns/update-positions")
        .expect("admission");
    let result = program
        .machine_program
        .react(
            &instance,
            Value::variant(format!("{machine_id}.Input"), "Run", Vec::new()),
        )
        .expect("reaction");
    assert_eq!(
        field(&result.instance.observation, "touched"),
        &Value::int(0)
    );
    assert_eq!(
        field(&result.instance.observation, "total"),
        &Value::int(243)
    );
    assert_eq!(
        field(&result.instance.observation, "continued"),
        &Value::int(1)
    );
    assert!(result.receipt.ordered_commands.is_empty());
}

#[test]
fn update_return_is_lexical_from_while_and_fallthrough_keeps_the_back_edge() {
    let source = r#"
pub machine App {
  events { Run(stop: Nat) }
  outcomes { commit Done }
  state { remaining: Seq<Nat> = [], result: Int = 0, continued: Nat = 0 }
  observe { remaining, result, continued }

  update scan(stop: Nat) -> Int {
    while remaining.uncons() is Some(Uncons { head, tail })
    decreases(remaining.len()) {
      if head == stop {
        return 7;
      }
      remaining = tail;
    }
    99
  }

  on Run(stop) {
    remaining = [3, 2, 1];
    let scanned = scan(stop);
    result = scanned;
    continued = continued + 1;
    Done
  }
}
"#;
    let program = checked(source);
    let machine_id = "example.returns@1::App";
    let (instance, _) = program
        .machine_program
        .admit(machine_id, Value::Unit, "returns/update-loop")
        .expect("admission");
    let run = |stop: u64| {
        Value::variant(
            format!("{machine_id}.Input"),
            "Run",
            vec![(
                Some("stop".into()),
                Value::nat(stop).expect("test Nat payload"),
            )],
        )
    };

    let first = program
        .machine_program
        .react(&instance, run(3))
        .expect("first-iteration return");
    assert_eq!(
        field(&first.instance.observation, "remaining"),
        &Value::Seq(vec![
            Value::nat(3).unwrap(),
            Value::nat(2).unwrap(),
            Value::nat(1).unwrap(),
        ])
    );
    assert_eq!(field(&first.instance.observation, "result"), &Value::int(7));
    assert_eq!(
        field(&first.instance.observation, "continued"),
        &Value::nat(1).unwrap()
    );

    let later = program
        .machine_program
        .react(&instance, run(1))
        .expect("later-iteration return");
    assert_eq!(
        field(&later.instance.observation, "remaining"),
        &Value::Seq(vec![Value::nat(1).unwrap()])
    );
    assert_eq!(field(&later.instance.observation, "result"), &Value::int(7));
    assert_eq!(
        field(&later.instance.observation, "continued"),
        &Value::nat(1).unwrap()
    );

    let fallthrough = program
        .machine_program
        .react(&instance, run(0))
        .expect("ordinary loop fallthrough");
    assert_eq!(
        field(&fallthrough.instance.observation, "remaining"),
        &Value::Seq(Vec::new())
    );
    assert_eq!(
        field(&fallthrough.instance.observation, "result"),
        &Value::int(99)
    );
    assert_eq!(
        field(&fallthrough.instance.observation, "continued"),
        &Value::nat(1).unwrap()
    );

    let mut public_break = program.clone();
    assert!(
        public_break
            .machine_program
            .machines
            .values_mut()
            .flat_map(|machine| machine.handlers.values_mut())
            .any(|handler| replace_first_loop_break(&mut handler.body, "user_break")),
        "checked loop should retain one compiler-private break local"
    );
    let error = uhura_core::Program::from_json(&public_break.to_canonical_string())
        .expect_err("public canonical IR must reject an arbitrary break channel");
    assert!(
        error.contains("compiler-reserved namespace"),
        "unexpected public-break error: {error}"
    );

    let mut forged = program.clone();
    assert!(
        forged
            .machine_program
            .machines
            .values_mut()
            .flat_map(|machine| machine.handlers.values_mut())
            .any(|handler| replace_first_loop_break(
                &mut handler.body,
                "__uhura_update_loop_exit_forged"
            ))
    );
    let error = uhura_core::Program::from_json(&forged.to_canonical_string())
        .expect_err("public canonical IR must reject a forged break channel");
    assert!(
        error.contains("immediately initialized total Option local"),
        "unexpected forged-break error: {error}"
    );
}

#[test]
fn source_cannot_author_the_compiler_internal_loop_exit_namespace() {
    let parsed = parse(
        SourceIdentity::new(1, "example.returns@1", "main", "main.uhura"),
        r#"
pub fn invalid() -> Int {
  let __uhura_update_loop_exit_0 = 1;
  __uhura_update_loop_exit_0
}
"#,
    );
    assert!(!parsed.is_ok());
    assert!(parsed.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.kind,
        ParseDiagnosticKind::Lexical | ParseDiagnosticKind::InvalidName
    )));
}

#[test]
fn nested_update_loop_return_propagates_through_each_owned_loop_exit() {
    let program = checked(
        r#"
pub machine App {
  events { Run(stop: Nat) }
  outcomes { commit Done }
  state {
    outer: Seq<Nat> = [],
    inner: Seq<Nat> = [],
    result: Int = 0,
    continued: Nat = 0,
  }
  observe { outer, inner, result, continued }

  update nested(stop: Nat) -> Int {
    while outer.uncons() is Some(Uncons { head: _, tail: outer_tail })
    decreases(outer.len()) {
      inner = [2, 1];
      while inner.uncons() is Some(Uncons { head: value, tail: inner_tail })
      decreases(inner.len()) {
        if value == stop {
          return 23;
        }
        inner = inner_tail;
      }
      outer = outer_tail;
    }
    99
  }

  on Run(stop) {
    outer = [2, 1];
    let selected = nested(stop);
    result = selected;
    continued = continued + 1;
    Done
  }
}
"#,
    );
    let machine_id = "example.returns@1::App";
    let (instance, _) = program
        .machine_program
        .admit(machine_id, Value::Unit, "returns/nested-loop")
        .expect("admission");
    let run = |stop: u64| {
        Value::variant(
            format!("{machine_id}.Input"),
            "Run",
            vec![(Some("stop".into()), Value::nat(stop).unwrap())],
        )
    };

    let selected = program
        .machine_program
        .react(&instance, run(1))
        .expect("nested selected return");
    assert_eq!(
        field(&selected.instance.observation, "outer"),
        &Value::Seq(vec![Value::nat(2).unwrap(), Value::nat(1).unwrap()])
    );
    assert_eq!(
        field(&selected.instance.observation, "inner"),
        &Value::Seq(vec![Value::nat(1).unwrap()])
    );
    assert_eq!(
        field(&selected.instance.observation, "result"),
        &Value::int(23)
    );
    assert_eq!(
        field(&selected.instance.observation, "continued"),
        &Value::nat(1).unwrap()
    );

    let fallthrough = program
        .machine_program
        .react(&instance, run(0))
        .expect("nested ordinary fallthrough");
    assert_eq!(
        field(&fallthrough.instance.observation, "outer"),
        &Value::Seq(Vec::new())
    );
    assert_eq!(
        field(&fallthrough.instance.observation, "inner"),
        &Value::Seq(Vec::new())
    );
    assert_eq!(
        field(&fallthrough.instance.observation, "result"),
        &Value::int(99)
    );
}

#[test]
fn valueless_return_checks_as_unit_and_return_outside_a_body_is_rejected() {
    let valueless = check_v04_project_modules(&[module(
        r#"
pub fn invalid() -> Int {
  return;
}
"#,
    )]);
    assert!(valueless.program.is_none());
    assert!(
        valueless
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura/type-mismatch")
    );

    let outside = check_v04_project_modules(&[module(
        r#"
pub const INVALID: Int = return 1;
"#,
    )]);
    assert!(outside.program.is_none());
    assert!(
        outside.diagnostics.iter().any(|value| {
            value.rule == "uhura-0.4/unsupported"
                && value.message.contains("requires an enclosing function")
        }),
        "diagnostics: {:#?}",
        outside.diagnostics
    );
}
